# Cartridge Module Review

## Overview
Comprehensive review of `/src/cartridge/` directory covering code quality, design patterns, test coverage, and refactoring opportunities. The cartridge module handles iNES ROM parsing, mapper implementations, and PRG/CHR memory management for NES emulation.

---

## Findings Summary

### Health Status
- ✅ **No compiler errors or warnings** - Clean compilation via `cargo check --all-targets`
- ✅ **Good test coverage** - Most mappers have unit tests
- ⚠️ **Some mappers lack comprehensive tests**
- ⚠️ **Design inconsistencies** across mapper implementations
- ⚠️ **Code duplication** in mapper implementations

### Statistics
- **Total files:** 29 mapper implementations + 6 core modules
- **Files with tests:** 20+ (most mappers tested)
- **Files without tests:** 8 (GxROM, Bandai FCG, Namco118, Namco163, Sunsoft FME7, VRC6, Camerica, Multicart15)
- **Total expect()/unwrap() usage:** ~100+ instances (mostly in tests, acceptable)

---

## 1. Code Smells & Anti-Patterns

### 1.1 Inconsistent Mapper API Implementations
**Severity:** Medium | **Impact:** Maintainability, correctness

**Issue:** Mappers implement `Mapper` trait methods inconsistently:

- Some mappers override `chr_ram_snapshot()` / `restore_chr_ram()`, others don't
- Some new custom `pub fn new()` labeled `#[cfg(test)]` (e.g., MMC3, MMC1), preventing production use
- Different patterns for CHR-ROM vs CHR-RAM detection
- Register snapshot/restore not consistently implemented across all mappers

**Examples:**
- `mmc3.rs` has test-only `new()` at line 53, forces production to use factory
- `mmc1.rs` has test-only `new()` at line 84, test-only `new_with_revision()`
- `axrom.rs` uses factory `create_mapper()` in tests (via `MapperContext`)
- `nrom.rs` has unconditional `pub fn new()`

**Recommended Fix:**
- Standardize all mappers with public `new()` constructors (not `#[cfg(test)]`)
- Create a `NewMapper` helper trait if factory is needed
- Use property-based builder if extended initialization needed

**Code Example:**
```rust
// Current (inconsistent):
#[cfg(test)]
pub fn new(...) { }  // MMC1, MMC3

pub fn new_with_revision(...) { }  // MMC1 does this
pub fn new(...) { }  // NROM, Cartridge level does this

// Should be:
pub fn new(...) { }  // All mappers have this
pub fn with_irq_mode(self, ...) -> Self { }  // Builder if needed
```

---

### 1.2 Duplicate Bank Offset Calculations
**Severity:** Medium | **Impact:** Code duplication, bug-prone

**Issue:** Multiple mappers manually calculate bank offsets instead of using the `BankedRom` helper:

**Affected files:**
- `colordreams.rs` lines 40-50 (manual bank offset calculation)
- `bnrom_nina.rs` lines 45-63 (manual offset calculation)
- `nina_tengen.rs` (custom implementation)
- `bandai_fcg.rs` (custom implementation)

**Example:**
```rust
// Current (ColorDreams):
fn prg_bank_offset(&self) -> usize {
    let num_banks = (self.prg_rom.len() / PRG_BANK_SIZE).max(1);
    let bank = (self.prg_bank_select as usize) % num_banks;
    bank * PRG_BANK_SIZE
}

// Should use BankedRom (like GxROM, CNROM):
prg_rom: BankedRom::new(prg_rom, PRG_BANK_SIZE)
// Then: prg_rom.read(bank_select as usize, offset)
```

**Recommended Fix:**
- Migrate all manual bank calculations to use `BankedRom` helper
- Reduces code duplication, improves consistency
- Benefit: single source of truth for wraparound/bounds checking

---

### 1.3 Inconsistent CHR Memory Handling
**Severity:** Medium | **Impact:** Correctness, maintainability

**Issues:**
1. Some mappers use `ChrMemory` helper (NROM, UxROM, AxROM)
2. Others manually manage `Vec<u8>` with custom logic (ColorDreams, BnromNina)
3. Mix of `chr_rom` / `chr_ram` and `chr_memory` naming

**Affected:**
- `colordreams.rs`: Raw `Vec<u8>` management
- `bnrom_nina.rs`: Uses `ChrMemory` but named `chr_memory` (good)
- `gxrom.rs`: Uses `BankedRom` for CHR (good)
- `mmc3.rs`, `mmc5.rs`: Mix of approaches

**Example:**
```rust
// ColorDreams (bad):
chr_rom: Vec<u8>
// vs NROM (good):
chr_memory: ChrMemory
```

**Recommended Fix:**
- Standardize all mappers to use `ChrMemory` for CHR-RAM
- Standardize all mappers to use `BankedRom` for CHR-ROM banks
- Eliminates ad-hoc bounds checking

---

### 1.4 Missing Snapshot/Restore for Complex State
**Severity:** High | **Impact:** Save state corruption

**Issue:** Some mappers with banked WRAM don't properly implement `wram_snapshot()` / `load_wram_snapshot()`:

**Affected:**
- `mmc5.rs`: Has 64KB PRG-RAM (8×8KB), must override snapshot methods to capture all banks
- Currently uses default impl which only captures $6000-$7FFF window
- **Risk:** WRAM state loss on save/load if game uses PRG-RAM banking

**Example:**
```rust
// Current (mmc5.rs, BROKEN):
// Uses default MapperContext impl which captures only 8KB window

// Should be:
impl Mapper for MMC5Mapper {
    fn wram_size(&self) -> usize {
        self.prg_ram.len()  // 64KB for MMC5
    }
    
    fn wram_snapshot(&self) -> Vec<u8> {
        // Capture all banks independent of current bank select
        self.prg_ram.clone()  // Or iterate through all banks
    }
    
    fn load_wram_snapshot(&mut self, data: &[u8]) {
        // Restore all banks
        self.prg_ram[..data.len().min(self.prg_ram.len())].copy_from_slice(...)
    }
}
```

**Recommended Fix:**
- Add tests that verify save/load works across multiple PRG-RAM banks
- Override snapshot/restore methods for any mapper with >8KB WRAM
- Document requirement in `Mapper` trait

---

### 1.5 Inconsistent Mirroring Mode Return Values
**Severity:** Low | **Impact:** Maintainability

**Issue:** `get_mirroring()` implementations return different values for dynamic mirroring:

- MMC1: Returns `SingleScreenLower` / `SingleScreenUpper` (actually correct)
- AxROM: Returns hardcoded `SingleScreen` regardless of bit 4
- MMC5: Returns `Mirroring` from header, doesn't support dynamic modes

**Recommended Fix:**
- Document dynamic vs static mirroring capability in trait
- Update AxROM to distinguish `SingleScreenLower` vs `SingleScreenUpper` (bit 4 of register)

---

### 1.6 Manual `.max(1)` Bank Wrapping in Multiple Places
**Severity:** Low | **Impact:** Code clarity

**Issue:** Common pattern repeated across mappers:

```rust
let bank = (some_value as usize) % num_banks.max(1);
let num_banks = (self.prg_rom.len() / BANK_SIZE).max(1);
```

**Recommended Fix:**
- Extract `fn safe_bank_index(bank: u8, num_banks: usize) -> usize` helper in `common.rs`
- Use consistently across all mappers

---

## 2. Test Coverage Gaps

### 2.1 Mappers Missing Unit Tests
**Files without test modules:**

| Mapper | File | Reason |
|--------|------|--------|
| GxROM (66) | `gxrom.rs` | ❌ No tests |
| Bandai FCG | `bandai_fcg.rs` | ❌ No tests |
| Namco118 | `namco118.rs` | ❌ No tests |
| Namco163 | `namco163.rs` | ❌ No tests |
| Sunsoft FME7 | `sunsoft_fme7.rs` | ❌ No tests |
| VRC6 | `vrc6.rs` | ❌ No tests |
| Camerica | `camerica.rs` | ⚠️ Minimal coverage |
| Multicart 15 | `multicart_15.rs` | ⚠️ Minimal coverage |

**Impact:** No validation that these mappers work correctly, potential silent bugs.

---

### 2.2 Incomplete Test Coverage in Core Files

#### `cartridge.rs`
- ✅ Covers ROM parsing, NROM creation
- ❌ Missing tests for:
  - Unsupported mapper handling
  - Edge cases in save/load with malformed ROMs
  - Different header versions (NES 1.0 vs 2.0)

#### `mapper.rs`
- ✅ Has basic factory tests
- ❌ Missing tests for:
  - All mapper factory cases (only NROM tested)
  - Error handling for unsupported mappers
  - Submapper routing

#### `ines.rs`
- ✅ Good header parsing tests
- ❌ Missing tests for:
  - Extended NES 2.0 format sizes
  - Trainer handling edge cases
  - Fallback from NES2 to NES1 sizing

---

### 2.3 Insufficient Integration Tests
**Issue:** Unit tests focus on individual mappers, but lack:

1. **Cross-mapper consistency tests** - Verify all mappers behave similarly for common operations
2. **Register snapshot/restore round-trip tests** - For MMC3, MMC1 with complex state
3. **Open-bus behavior tests** - For mappers returning open-bus on disabled regions
4. **PPU address tracking tests** - For A12 edge detection (now in MMC3, needed for VRC)
5. **Real ROM tests** - Integration tests with actual game ROMs

---

## 3. Design Improvements

### 3.1 Consolidate Bank Switching Logic
**Recommendation:** Create a `BankSwitch` helper struct

```rust
pub struct BankSwitch {
    num_banks: usize,
    bank: u8,
}

impl BankSwitch {
    pub fn new(num_banks: usize) -> Self { ... }
    pub fn set(&mut self, bank: u8) { self.bank = bank; }
    pub fn current(&self) -> usize { (self.bank as usize) % self.num_banks }
    pub fn offset(&self, bank_size: usize) -> usize {
        self.current() * bank_size
    }
}
```

**Usage:**
```rust
pub struct ColorDreamsMapper {
    prg_bank: BankSwitch,
    chr_bank: BankSwitch,
    ...
}
```

**Benefit:** Eliminates repetitive modulo/multiplication logic.

---

### 3.2 Standardize Snapshot Pattern
**Recommendation:** Create `StateSnapshot` trait

```rust
pub trait StateSnapshot {
    fn snapshot(&self) -> Vec<u8>;
    fn restore(&mut self, data: &[u8]);
}

// Implement for all registe sets:
impl StateSnapshot for Mmc3Registers { ... }
impl StateSnapshot for Mmc5PrgBanking { ... }
```

**Benefit:** Consistent naming, easier to test, clearer intent.

---

### 3.3 Explicit Mapper Feature Matrix
**Recommendation:** Document mapper capabilities in code

```rust
pub trait MapperCapabilities {
    const HAS_IRQ: bool;
    const HAS_CHR_BANKING: bool;
    const HAS_DYNAMIC_MIRRORING: bool;
    const MAX_PRG_RAM_KB: usize;
    const EXPANSION_AUDIO: bool;
}
```

**Allows:**
- Compile-time feature detection
- Automatic skip of tests for unsupported features
- Documentation of capabilities

---

### 3.4 Separate Trait for Complex Features
**Recommendation:** Split `Mapper` trait into smaller concerns

```rust
pub trait Mapper {
    // Basic I/O
    fn read_prg(&self, addr: u16) -> u8;
    fn write_prg(&mut self, addr: u16, value: u8);
    // ... minimal essential
}

pub trait MapperIrq: Mapper {
    fn irq_pending(&self) -> bool;
    fn clock_irq(&mut self);
}

pub trait MapperPpuExtension: Mapper {
    fn ppu_address_changed(&mut self, addr: u16);
    fn ppu_scanline(&mut self, scanline: u16);
}
```

**Benefit:** Simpler trait, less `default` noise, clearer dependencies.

---

## 4. Refactoring Opportunities

### 4.1 Extract Common Mapper Patterns
**High Priority** | **Effort: Medium**

Create mapper templates for:
1. **SimpleFixedPRG** - Fixed PRG + bank switchable CHR (CNROM pattern)
2. **SimpleBankedPRG** - Banked PRG + CHR-RAM (UxROM pattern)
3. **DualBank32** - 32KB PRG + 8KB CHR select (GxROM, ColorDreams pattern)

**Example:**
```rust
pub struct DualBankMapper<const PRG_BANK_KB: usize, const CHR_BANK_KB: usize> {
    prg_rom: BankedRom,
    chr_rom: BankedRom,
    bank_register: u8,
}

// Used by: GxROM, ColorDreams, etc.
```

---

### 4.2 Consolidate CHR-ROM vs CHR-RAM Selection
**Priority: Medium** | **Effort: Low**

Currently mixed patterns:
- Some use `ChrMemory` (good abstraction)
- Some use `chr_rom: Vec<u8>` + branch on `is_empty()`
- Some use separate `chr_rom` and `chr_ram` fields

**Fix:**
- Always use `ChrMemory` wrapper
- Updates to `ChrMemory::new()` already support auto-detection
- Removes branching at read/write time

---

### 4.3 Move ROM Database Out of `rom_db.rs`
**Priority: Low** | **Effort: High**

Current `rom_db.rs` mixes:
1. CRC32 calculation (utility)
2. ROM quirk database (application-specific)

**Recommendation:**
- Keep CRC32 in `rom_db.rs`
- Move quirk lists (MMC3_ALTERNATE_IRQ, ARKANOID_PADDLE, ZAPPER) to `console.rs` or config file
- Makes cartridge module reusable without game-specific overrides

---

### 4.4 Reduce `#[allow(...)]` Annotations
**Priority: Low** | **Effort: Low**

Found in:
- `mapper.rs` line 5: `#[allow(clippy::module_inception)]` - Can rename to avoid
- `cartridge.rs` line 6: Same - Rename or restructure
- Multiple test modules: `#[allow(dead_code)]` - Could be removed by refactoring

**Action:** Review these before next refactoring phase.

---

### 4.5 Consolidate Test Utilities
**Priority: Low** | **Effort: Medium**

Many mappers define identical helpers:

```rust
// Repeated in: mmc3.rs, mmc1.rs, axrom.rs, etc.
fn banked_data(bank_size: usize, num_banks: usize) -> Vec<u8> {
    let mut data = vec![0u8; bank_size * num_banks];
    for bank in 0..num_banks {
        let start = bank * bank_size;
        let end = start + bank_size;
        data[start..end].fill(bank as u8);
    }
    data
}

// Could be:
#[cfg(test)]
pub mod test_helpers {
    pub fn banked_data(...) { ... }
}
```

---

## 5. Missing Test Cases (Priority List)

### Critical (Correctness Risk)
1. **MMC5 WRAM Snapshot** - Verify all 64KB banks saved/loaded correctly
2. **Open Bus Fallback** - Test `read_prg_open_bus()` for all mappers supporting it
3. **Bank Wrapping Edge Cases** - Large bank numbers on small ROMs
4. **CHR-RAM vs CHR-ROM** - Initialization and write protection

### High (Integration)
1. **Real ROM Tests** - Load public test ROMs (blargg CPU test, nestest, etc.)
2. **Mapper Factory** - Test all 32 mappers can be created via factory
3. **Save/Load Round-Trip** - Save state → load state → matches
4. **PPU A12 Detection** - For all mappers using A12 edge (MMC3, VRC)

### Medium (Quality)
1. **Register Snapshot Consistency** - Snapshot → restore → identical state
2. **Dynamic Mirroring** - Verify changes to mirroring apply immediately
3. **Multiple Banks** - Test behavior with odd bank counts (3, 5, 7 banks)

### Low (Documentation)
1. **Spec Compliance** - Cross-check against nesdev.org for each mapper
2. **Known Issues** - Document mapper limitations (e.g., MMC5 split-screen partial)

---

## 6. Documentation Gaps

### 6.1 Insufficient Comments on Complex Logic
**Files:**
- `mmc5.rs` - Very long (3595 lines), needs section markers
- `mmc3.rs` - A12 edge detection complex, could use diagrams
- `mapper.rs` - Factory function could list all implemented mappers with numbers

### 6.2 Missing Hardware Specification Links
**Issue:** Each mapper should reference its NESdev wiki page

**Example:**
```rust
/// MMC1 mapper (Mapper 1)
/// 
/// See: https://www.nesdev.org/wiki/MMC1
/// Revision detection: https://www.nesdev.org/wiki/MMC1#ASIC_Revisions
pub struct MMC1Mapper { ... }
```

**Status:** MMC5 has good docs, others inconsistent.

---

### 6.3 Trait Method Default Implementations
**Issue:** `Mapper` trait has 20+ methods with defaults, unclear which must be overridden

**Recommendation:** Add documentation markers:
```rust
/// **REQUIRED**: Implement in all mappers
fn read_prg(&self, addr: u16) -> u8;

/// **OPTIONAL**: Override only if mapper supports IRQ
fn irq_pending(&self) -> bool {
    false
}

/// **CONDITIONAL**: Must override if WRAM > 8KB
fn wram_snapshot(&self) -> Vec<u8> {
    // ...
}
```

---

## 7. Configuration & Usability

### 7.1 Magic Numbers
**Scattered throughout:**
- `0x4000`, `0x8000`, `0x1FFF` repeated without named constants
- `0x80`, `0x10`, `0x01` for bit masks

**Recommendation:**
- Each mapper file defines `const BANK_SIZE: usize = ...`
- Move common masks to `common.rs` or mapper-level constants

---

### 7.2 Feature Flag Gating
**Issue:** Some features are `#[cfg(test)]` but should be available

**Examples:**
- MMC1::new() is test-only, should be public
- MMC3::irq_counter() is test-only, could be useful for debugging

**Recommendation:**
- Public API for querying mapper state (read-only)
- Dev/debug features behind feature flag, not test-only

---

## 8. Summary Table

| Category | Issue | Severity | Effort | Impact |
|----------|-------|----------|--------|--------|
| Consistency | Inconsistent `new()` implementations | Medium | Medium | Maintainability |
| Duplication | Manual bank offset calculations | Medium | Medium | Code quality |
| Duplication | CHR memory handling patterns | Medium | Medium | Correctness |
| Correctness | Missing WRAM snapshot for complex mappers | High | Low | Save state bugs |
| Consistency | Mirroring mode return values | Low | Low | Correctness |
| Clarity | Manual `.max(1)` wrapping | Low | Low | Readability |
| Testing | Missing tests for 8 mappers | High | High | Validation |
| Testing | Incomplete core module tests | Medium | High | Coverage |
| Design | Consolidate bank logic | Low | Medium | Maintainability |
| Design | Standardize snapshot pattern | Low | Medium | Consistency |
| Design | Split Mapper trait | Low | High | Usability |
| Refactor | Extract common patterns | Low | Medium | Maintainability |
| Refactor | Consolidate CHR selection | Medium | Low | Code clarity |
| Refactor | Move ROM database | Low | High | Modularity |
| Documentation | Complex logic comments | Low | Low | Maintainability |
| Documentation | Missing NESdev links | Low | Low | Usability |

---

## Recommendations by Priority

### Phase 1 (Critical - Do First)
1. ✅ **Add unit tests** for 8 mappers without tests (GxROM, Namco*, Sunsoft FME7, VRC6, Camerica, Multicart15)
2. ✅ **Fix MMC5 WRAM snapshot** to handle all 64KB banks
3. ✅ **Add round-trip tests** for save/load with register snapshots

### Phase 2 (Important - Do Soon)
1. Standardize all mappers to use `BankedRom` for bank operations
2. Standardize all mappers to use `ChrMemory` wrapper
3. Make all `pub fn new()` unconditionally available (not `#[cfg(test)]`)
4. Add open-bus behavior tests

### Phase 3 (Nice to Have - Refactoring)
1. Create `StateSnapshot` trait for consistency
2. Extract common bank-switch patterns
3. Create mapper template types
4. Add mapper capability documentation matrix
5. Consider splitting `Mapper` trait

### Phase 4 (Polish - Documentation)
1. Add NESdev wiki links to all mappers
2. Document mapper limitations (e.g., MMC5 split-screen)
3. Add section markers and diagrams to complex files
4. Extract test utilities to shared module

---

## Conclusion

The cartridge module is **well-structured** with good separation of concerns, but shows signs of **organic growth** with inconsistent patterns. The main risks are:

1. **Save state corruption** due to incomplete WRAM snapshot handling (MMC5)
2. **Silent failures** in untested mappers
3. **Maintainability burden** from code duplication in bank operations

**Recommended action:** Start with Phase 1 recommendations to address correctness issues, then tackle Phase 2 for consistency before larger refactoring in Phase 3.
