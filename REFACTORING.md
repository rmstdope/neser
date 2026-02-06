# Refactoring Suggestions for Neser NES Emulator

This document identifies areas of the codebase where the implementation is overly complicated and suggests refactoring or changes to simplify. The suggestions are organized by module.

---

## Table of Contents

1. [Overall Architecture Patterns](#overall-architecture-patterns)
2. [CPU Module](#cpu-module)
3. [PPU Module](#ppu-module)
4. [APU Module](#apu-module)
5. [Bus Module](#bus-module)
6. [Cartridge Module](#cartridge-module)
7. [Debugger Module](#debugger-module)
8. [Savestate Module](#savestate-module)
9. [NES Main Module](#nes-main-module)
10. [Input Module](#input-module)
11. [Summary Table](#summary-table)

---

## Overall Architecture Patterns

### 1. Excessive Use of `Rc<RefCell<T>>`

**Issue:** The codebase extensively uses `Rc<RefCell<T>>` for shared mutable state, resulting in verbose code with many `.borrow()` and `.borrow_mut()` calls.

**Examples:**
- `src/nes.rs` - PPU, APU, and Bus are all wrapped in `Rc<RefCell<>>`
- `src/bus/bus.rs` - 8 different `Rc<RefCell<>>` fields for shared state

**Suggested Refactoring:**
- Consider passing references directly where possible rather than using shared ownership
- For truly shared state, consider using interior mutability patterns that hide the borrow calls
- Evaluate whether some components really need to be shared or if ownership can be restructured

### 2. Repetitive Nested Cartridge Access Pattern

**Issue:** Throughout the codebase, there's a repeated pattern of deeply nested borrow chains to access the cartridge mapper:

```rust
self.cartridge
    .borrow()
    .as_ref()
    .map(|cart| cart.borrow().mapper().get_mirroring())
    .unwrap_or(...)
```

This pattern appears 10+ times in different methods.

**Suggested Refactoring:**
- Extract to a helper method like `fn with_mapper<F, T>(&self, f: F) -> Option<T>`
- Create convenience methods for common operations like `get_mirroring_or_default()`

---

## CPU Module

**Location:** `src/cpu/`

### 1. Large Monolithic CPU Implementation

**Issue:** `src/cpu/cpu.rs` is extremely large (~540KB), making it difficult to navigate and maintain.

**Suggested Refactoring:**
- Extract addressing mode logic into a separate module
- Extract opcode execution into smaller modules organized by instruction category (arithmetic, branching, memory, etc.)
- Consider a more data-driven approach where instruction behavior is defined declaratively

### 3. Many Test-Only Setter Methods

**Issue:** The CPU struct has numerous `#[cfg(test)]` setter methods like `set_a_register()`, `set_x()`, `set_y()`, etc.

**Suggested Refactoring:**
- Consider a test-only constructor that takes initial state
- Or use a builder pattern for test setup
- Or make the fields public in test builds

---

## PPU Module

**Location:** `src/ppu/`

### 1. Multiple VBlank-Related Flags

**Issue:** The PPU maintains several distinct flags for VBlank handling:
- `vblank_suppressed_for_frame` (bool)
- `vblank_for_nmi` (bool)

While comments explain these are "intentionally distinct" for boundary timing quirks, this creates confusion.

**Suggested Refactoring:**
- Create a `VBlankState` enum or struct that encapsulates all VBlank-related state with clear documentation
- Add inline documentation explaining the hardware behavior that requires this distinction

### 2. Excessive Helper Methods for Mapper Access

**Issue:** The PPU has multiple nearly identical helper methods for mapper interaction:
- `with_mapper_mut()`
- `notify_chr_fetch_kind()`
- `notify_chr_fetch_is_ppudata()`

All follow similar borrow-and-call patterns.

**Suggested Refactoring:**
- Create a single generic `with_mapper()` method that takes a closure
- Use this for all mapper interactions

### 3. Deep Nesting in Tick Loop

**Issue:** The PPU tick function (`ppu/ppu/tick.rs`) has complex nested condition checks for scanline/pixel positions with multiple trace macros containing 10+ parameters.

**Suggested Refactoring:**
- Extract timing checks to named helper functions like `is_vblank_enter()`, `is_vblank_exit()`, `is_visible_scanline()`
- Consider a state machine approach for scanline phases
- Reduce trace macro parameter counts by grouping related state

---

## APU Module

**Location:** `src/apu/`

### 1. Large Hardcoded Mixer Lookup Tables

**Issue:** In `apu.rs`, there are large hardcoded lookup tables:
- `PULSE_TABLE`: 31 entries with 15+ decimal places
- `TND_TABLE`: 203 entries with excessive precision

These use `#[allow(clippy::excessive_precision)]`.

**Suggested Refactoring:**
- Generate these tables programmatically at compile time using `const fn` or build scripts
- Or compute them at startup and cache in a static
- Or reference the NES APU documentation for formulas and compute on-demand

### 2. Multiple Enable Flags Mixed with Core Logic

**Issue:** Debug/channel-enable flags are mixed with core APU logic:
```rust
pulse1_enabled, pulse2_enabled, triangle_enabled, noise_enabled, dmc_enabled
```

**Suggested Refactoring:**
- Group debugging flags into a separate `ApuDebugConfig` struct
- Or use feature flags to conditionally include debug functionality

### 3. Duplicate Constructor Logic

**Issue:** `new()` and `new_for_testing()` have nearly identical initialization code with only minor differences.

**Suggested Refactoring:**
- Create a private `new_internal()` constructor that both call with appropriate parameters
- Or use a builder pattern

### 4. Sample Rate Constant Defined Multiple Times

**Issue:** `DEFAULT_SAMPLE_RATE: f32 = 44100.0` appears to be defined in multiple places.

**Suggested Refactoring:**
- Define as a module-level constant and reference from a single location

---

## Bus Module

**Location:** `src/bus/`

### 1. Device Iteration in Hot Path

**Issue:** The bus device lookup iterates through all devices for every memory access:

```rust
for device in self.devices.iter_mut() {
    if device.address_range().contains(&addr)
        && let Some(value) = device.read(...)
    {
        return Some(value);
    }
}
```

This is O(n) per memory access.

**Suggested Refactoring:**
- Create an address-to-device lookup table at initialization time
- Use address range partitioning for O(1) device lookup
- Consider a match statement for known fixed address ranges

### 2. Scattered OAM DMA State

**Issue:** OAM DMA state is split across two separate fields:
- `oam_dma_page: Rc<RefCell<Option<u8>>>`
- `dma_triggered: Rc<RefCell<bool>>`

**Suggested Refactoring:**
- Combine into a single `Option<DmaRequest>` enum or struct
- Reduces number of separate RefCell borrows needed

### 3. Dummy Write Distinction in Device Interface

**Issue:** The `BusDevice::write()` method has an `is_dummy_write` parameter to distinguish OAM DMA no-ops, adding complexity to every device implementation.

**Suggested Refactoring:**
- Consider having separate `write()` and `write_dummy()` methods
- Or handle dummy writes internally in the bus before dispatching to devices

---

## Cartridge Module

**Location:** `src/cartridge/`

### 2. Common Mapper State Snapshots Could Use Trait Defaults More

**Issue:** Most mappers implement snapshot/restore methods in a similar way:
- `wram_snapshot()` / `load_wram_snapshot()`
- `chr_ram_snapshot()` / `restore_chr_ram()`
- `registers_snapshot()` / `restore_registers()`

While the `Mapper` trait provides defaults, many mappers still override them with nearly identical logic.

**Suggested Refactoring:**
- Review which mappers can rely entirely on the trait defaults
- For mappers with common patterns (like using `PrgRam` and `ChrMemory` helpers), the default implementations should be sufficient without override

### 3. VRC2/VRC4 and VRC6 Mapper Factory Functions

**Issue:** Separate factory functions exist for different submapper variants:
```rust
fn vrc2_vrc4_21(...) -> Vrc2Vrc4Mapper
fn vrc2_vrc4_22(...) -> Vrc2Vrc4Mapper
fn vrc2_vrc4_23(...) -> Vrc2Vrc4Mapper
fn vrc2_vrc4_25(...) -> Vrc2Vrc4Mapper
fn vrc6_24(...) -> VRC6Mapper
fn vrc6_26(...) -> VRC6Mapper
```

**Suggested Refactoring:**
- Consider parameterizing these in the mapper registry macro
- Or have the mapper constructor take the mapper number directly (which they already do)

### 4. MMC3 and MMC5 Special Handling in create_mapper

**Issue:** The `create_mapper()` function has special-case handling for mappers 4 (MMC3) and 5 (MMC5) outside the registry macro.

**Suggested Refactoring:**
- Consider extending the registry macro to support mappers that need additional context
- Or create a separate factory function map for complex mappers

---

## Debugger Module

**Location:** `src/debugger/`

### 1. Hardcoded Layout Constants

**Issue:** Layout constants are hardcoded at the top of `ui.rs`:
```rust
const DEBUGGER_OUTER_MARGIN: f32 = 10.0;
const DEBUGGER_OUTER_GAP: f32 = 10.0;
```

**Suggested Refactoring:**
- Consider making these configurable
- Or derive from display size for responsive layouts

### 2. Layout Calculation Complexity

**Issue:** The `layout_models()` function has complex floating-point calculations for a fixed 3-window layout.

**Suggested Refactoring:**
- For a fixed layout, simplify by hardcoding relative positions
- Or use a simpler layout algorithm if the positions don't need to be pixel-perfect

---

## Savestate Module

**Location:** `src/savestate/`

### 1. Overly Granular State Capture

**Issue:** `CpuState` has 20+ fields including implementation details like:
- `skip_interrupt_latch_this_cycle`
- `oob_master_clock_ppu`
- `dmc_dma_need_dummy_read`

**Suggested Refactoring:**
- Document which fields are essential vs. which are internal implementation details
- Consider grouping related state into nested structs for clarity
- Evaluate if some transient state can be reconstructed rather than serialized

### 2. No Version Migration Strategy

**Issue:** `SAVESTATE_VERSION: u32 = 5` suggests the format has evolved, but there's no visible migration logic for loading older save states.

**Suggested Refactoring:**
- Add version upgrade handlers that transform older formats to the current version
- Or document that only the current version is supported and old saves are incompatible

### 3. Test Helper Functions Are Verbose

**Issue:** Test functions like `create_test_cpu_state()`, `create_test_ppu_state()`, etc. have very verbose struct initialization.

**Suggested Refactoring:**
- Implement `Default` trait for test state structs
- Use struct update syntax: `CpuState { a: 0x42, ..Default::default() }`

---

## NES Main Module

**Location:** `src/nes.rs`

### 1. System Palette as Large Inline Array

**Issue:** The 64-entry system palette is defined as a large inline array with `#[rustfmt::skip]`:

```rust
const SYSTEM_PALETTE: [(u8, u8, u8); 0x40] = [
    /* 0x00 */ (0x54, 0x54, 0x54), ...
];
```

**Suggested Refactoring:**
- Load palette from an embedded resource file
- Or define in a separate palette module for easier maintenance and potential multiple palette support

### 2. Repetitive borrow_mut() Calls

**Issue:** The `reset()` method has repetitive borrow patterns:
```rust
self.ppu.borrow_mut().reset();
self.apu.borrow_mut().reset(cpu_cycle, soft_reset);
self.memory.borrow_mut().reset_cartridge();
```

**Suggested Refactoring:**
- Consider a `with_components_mut()` helper that borrows all at once
- Or restructure to reduce the need for shared mutable state

---

## Input Module

**Location:** `src/input/`

**Assessment:** This module is clean and well-structured. No significant complexity issues identified.

The `Joypad` implementation is straightforward (~250 lines) with clear documentation and good test coverage.

---

## Summary Table

| Category | Issue | Impact | Suggested Fix |
|----------|-------|--------|---------------|
| **Architecture** | `Rc<RefCell<T>>` overuse | Code verbosity, runtime overhead | Use references or restructure ownership |
| **Architecture** | Nested cartridge borrow chains | Hard to read, potential panic | Extract to helper methods |
| **CPU** | Large monolithic file | Hard to navigate | Split into smaller modules |
| **CPU** | Linear opcode lookup | Performance | Use direct array indexing |
| **PPU** | Multiple VBlank flags | Confusing | Create `VBlankState` enum |
| **PPU** | Deep nesting in tick loop | Hard to read | Extract timing checks to named functions |
| **APU** | Large lookup tables | Maintenance burden | Generate programmatically |
| **APU** | Duplicate constructors | DRY violation | Single internal constructor |
| **Bus** | Linear device search | O(n) per access | Address lookup table |
| **Bus** | Scattered DMA state | Multiple borrows needed | Combine into single struct |
| **Cartridge** | Builder pattern overhead | Unnecessary complexity | Direct construction |
| **Cartridge** | Mapper factory special cases | Inconsistent | Extend registry macro |
| **Savestate** | Granular state capture | Large save files | Group related state |
| **Savestate** | No version migration | Old saves incompatible | Add upgrade handlers |
| **NES** | Large inline palette | Hard to maintain | External resource file |

---

## Priority Recommendations

### High Priority (Significant Impact)
1. **Bus device lookup optimization** - Affects every memory access
2. **Cartridge access helper methods** - Reduces boilerplate throughout codebase
3. **CPU module split** - Improves maintainability of core component

### Medium Priority (Quality of Life)
4. **PPU VBlank state consolidation** - Reduces confusion
5. **APU lookup table generation** - Easier maintenance
6. **Savestate Default implementations** - Cleaner tests

### Low Priority (Nice to Have)
7. **Opcode direct indexing** - Minor performance gain
8. **Palette external resource** - Minor maintenance improvement
9. **Builder pattern simplification** - Minor code reduction

---

## Notes

- This analysis focuses on structural complexity rather than correctness
- Some complexity may be intentional to match NES hardware behavior accurately
- Any refactoring should maintain full compatibility with existing tests
- Performance-critical sections should be benchmarked before and after changes
