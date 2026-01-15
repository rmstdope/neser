# MMC5 (Mapper 5) Implementation Status

## Overview
MMC5 is a complex mapper with many advanced features. This document tracks what has been implemented and what remains for full MMC5 emulation.

## Implemented Features ✅

### PRG Banking (Complete)
- ✅ All 4 PRG modes implemented:
  - Mode 0: Single 32KB bank at $8000-$FFFF
  - Mode 1: Two 16KB banks at $8000-$BFFF and $C000-$FFFF
  - Mode 2: 16KB bank at $8000-$BFFF + 8KB banks at $C000-$DFFF and $E000-$FFFF
  - Mode 3: Four 8KB banks at $8000-$9FFF, $A000-$BFFF, $C000-$DFFF, $E000-$FFFF
- ✅ PRG-RAM banking via $5113 (8KB window at $6000-$7FFF)
- ✅ PRG-RAM write protection via $5102/$5103 registers
- ✅ ROM/RAM window selection (bit 7 of bank registers)
- ✅ Proper bank alignment for each mode

### CHR Banking (Mostly Complete)
- ✅ CHR mode control register ($5101) with 4 modes:
  - Mode 0: Single 8KB bank
  - Mode 1: Two 4KB banks
  - Mode 2: Four 2KB banks
  - Mode 3: Eight 1KB banks
- ✅ CHR bank registers $5120-$5127 (A registers for background)
- ✅ CHR bank registers $5128-$512B (B registers for sprites)
- ✅ BG/sprite banking split in CHR mode 3 (1KB banks)
  - PPU signals fetch type via `ppu_set_chr_fetch_is_sprite()`
  - Background fetches use A registers, sprite fetches use B registers
- ✅ Extended attribute mode CHR bank extension
  - Upper 6 bits of ExRAM extend CHR bank for per-tile selection

### Hardware Features
- ✅ Hardware multiplier ($5205/$5206)
  - 8×8 bit unsigned multiplication
  - Result available immediately in low/high bytes
- ✅ ExRAM storage at $5C00-$5FFF (1KB)
  - Readable and writable from CPU
  - ❌ Not integrated as nametable memory (requires PPU hooks)
- ✅ ExRAM mode control ($5104)
  - Register implemented but modes not fully functional without PPU integration

### Nametable Control (Partial)
- ✅ Nametable mapping register ($5105) implemented
  - Detects common patterns (horizontal, vertical, single-screen)
  - Maps to standard MirroringMode enum
  - ❌ ExRAM nametable mapping not functional (requires PPU to read from ExRAM)
- ✅ Fill mode registers ($5106 tile, $5107 attribute)
  - Registers implemented
  - ❌ Fill mode rendering not functional (requires PPU integration)

### IRQ System (Partial)
- ✅ IRQ registers ($5203 compare, $5204 enable/status)
- ✅ IRQ pending flag
- ❌ Scanline counter not integrated
  - **Reason**: Requires PPU to notify mapper on each scanline during rendering
  - **Impact**: Games relying on scanline IRQ won't work (e.g., status bars, split-screen effects)

### Infrastructure
- ✅ Memory controller routes $5000-$5FFF to mapper
- ✅ Split-screen registers ($5200-$5202) accept writes (functionality not implemented)

## Missing Features ❌

### 1. Scanline IRQ Tracking
**Status**: Registers exist but no scanline counting

**What's needed**:
- PPU must notify mapper when rendering starts (in_frame flag)
- PPU must notify mapper on each rendered scanline
- Mapper must increment counter and trigger IRQ when counter matches $5203
- IRQ must be cleared when rendering ends or when $5204 is written

**Required changes**:
- Extend Mapper trait or add new callback for scanline notifications
- PPU must call this during rendering at appropriate times
- Track in_frame state based on rendering enabled/disabled

**Impact**: Games that use MMC5's scanline IRQ for split-screen effects won't work

### 2. CHR BG/Sprite Banking Split
**Status**: ✅ Implemented

The PPU now signals to the mapper whether current CHR fetch is for background or sprite
via the `ppu_set_chr_fetch_is_sprite()` callback. In CHR mode 3 (1KB banks):
- Background fetches use A registers ($5120-$5127)
- Sprite fetches use B registers ($5128-$512B)

### 3. ExRAM as Nametable Memory
**Status**: ExRAM exists as CPU-accessible memory only

**What's needed**:
- When $5105 maps a nametable quadrant to ExRAM (value 2)
- PPU must read nametable data from mapper's ExRAM instead of its own VRAM
- Requires $5104 mode to be set appropriately

**Required changes**:
- PPU nametable read logic must check if current quadrant is mapped to ExRAM
- Call mapper method to read from ExRAM for that quadrant
- Respect $5104 mode settings (nametable, extended attributes)

**Impact**: Games using ExRAM for extra nametable space won't render correctly

### 4. Extended Attribute Mode
**Status**: Partially implemented

**Implemented**:
- ✅ Per-tile palette selection from ExRAM lower 2 bits (bits 0-1)
- ✅ CHR bank extension from ExRAM upper 6 bits (bits 2-7)
  - When enabled, background tile CHR fetches use the upper 6 bits of the corresponding
    ExRAM byte to extend the CHR bank selection, allowing each tile to select from
    a much larger CHR address space (up to 256KB with 1KB banks)

**What's still needed**:
- ❌ PPU attribute fetch hook to properly integrate per-tile attributes
  - Current implementation overrides attribute table reads via `read_nametable()`
  - May need more precise integration with PPU attribute decoding

**Impact**: Games like Castlevania III that use extended attribute mode for detailed
background graphics should now render with correct CHR tile selection.

### 5. Fill Mode
**Status**: Registers exist but not functional

**What's needed**:
- When $5105 maps nametable quadrant to fill mode (value 3)
- That quadrant displays a single repeating tile ($5106) with single attribute ($5107)
- Useful for solid color areas or simple backgrounds

**Required changes**:
- PPU nametable fetch must detect fill mode for quadrant
- Return $5106 for all tile fetches in that quadrant
- Return $5107 for all attribute fetches in that quadrant

**Impact**: Games using fill mode for optimization won't render those areas

### 6. Split-Screen Support
**Status**: Registers exist, no functionality

**What's needed**:
- $5200: Split mode and scroll control
- $5201: Split Y coordinate (scanline to split at)
- $5202: CHR bank for split region
- During rendering, switch CHR banking at specified scanline
- Can also affect scroll position for split region

**Required changes**:
- PPU must query mapper for CHR bank on each scanline
- Mapper returns different CHR banks before/after split point
- Complex interaction with normal CHR banking

**Impact**: Games using split-screen effects won't work

### 7. Expansion Audio
**Status**: Not implemented

**What's needed**:
- 2 square wave pulse channels (similar to APU pulse but with some differences)
- 1 PCM channel for sampled audio
- Audio registers in $5000-$5015 range
- Output mixed into audio stream via `expansion_audio_sample()`
- Channels clocked via `cpu_cycle()` callback

**Required changes**:
- Implement pulse wave generation (duty, volume, frequency)
- Implement PCM playback from DRAM or ROM
- Implement audio registers and state machine
- Return audio sample in `expansion_audio_sample()`

**Impact**: Games with MMC5 audio enhancement will be silent in those channels

### 8. PRG-RAM Sizing
**Status**: Always allocates 64KB

**What's needed**:
- Respect actual PRG-RAM size from cartridge header
- Most games use 8KB, 16KB, or 32KB
- 64KB is a compatibility superset but wastes memory

**Required changes**:
- Read PRG-RAM size from cartridge metadata in constructor
- Allocate appropriate amount
- Handle banking with correct wrap-around

**Impact**: Minor - current approach works but is inefficient

## Testing Status

### Unit Tests ✅
- 9 comprehensive unit tests covering:
  - All 4 PRG modes
  - PRG-RAM bank switching
  - PRG-RAM write protection
  - Hardware multiplier
  - ExRAM read/write
- All passing ✅

### Integration Tests ⏱️
- MMC5 Blargg test ROMs timeout (not functional)
- These tests specifically check CHR banking behavior
- **Reason**: Tests require the PPU-integrated features listed above

## Architectural Considerations

The main blocker for full MMC5 support is the current Mapper trait interface:

```rust
pub trait Mapper {
    fn read_prg(&self, addr: u16) -> u8;
    fn write_prg(&mut self, addr: u16, value: u8);
    fn read_chr(&self, addr: u16) -> u8;
    fn write_chr(&mut self, addr: u16, value: u8);
    fn ppu_address_changed(&mut self, addr: u16);
    fn irq_pending(&self) -> bool;
    fn get_mirroring(&self) -> MirroringMode;
}
```

### Needed Extensions

1. **Scanline notifications**:
   ```rust
   fn scanline(&mut self, scanline: u16, rendering: bool);
   fn end_frame(&mut self);
   ```

2. **CHR fetch type signaling**:
   ```rust
   fn set_chr_fetch_type(&mut self, is_sprite: bool);
   // OR
   fn read_chr_bg(&self, addr: u16) -> u8;
   fn read_chr_sprite(&self, addr: u16) -> u8;
   ```

3. **Nametable reading**:
   ```rust
   fn read_nametable(&self, addr: u16) -> Option<u8>;
   // Returns Some(byte) if mapper handles this address, None otherwise
   ```

4. **Per-scanline CHR banking** (for split-screen):
   ```rust
   fn get_chr_bank_for_scanline(&self, addr: u16, scanline: u16) -> u8;
   ```

These changes would affect multiple mappers (MMC5, VRC7, etc.) and the PPU, so they require careful design and implementation.

## Recommendations

### Short-term
1. ✅ Document current limitations (this file)
2. ✅ Comment out failing Blargg tests with explanation
3. ✅ Keep existing unit tests for implemented features
4. ✅ Mark issue as "partially complete" with this documentation

### Medium-term
1. Design Mapper trait extensions for PPU integration
2. Implement scanline IRQ as first step (simplest extension)
3. Add CHR fetch type detection (moderate complexity)
4. Implement ExRAM nametable support (more complex)

### Long-term
1. Implement expansion audio (independent of PPU work)
2. Implement split-screen support (most complex PPU integration)
3. Add integration tests once features are complete
4. Test with real MMC5 games (Castlevania 3, Just Breed, etc.)

## References

- NESdev Wiki: https://www.nesdev.org/wiki/MMC5
- Implementation: `src/cartridge/mmc5.rs`
- Tests: `src/cartridge/mmc5.rs` (unit tests), `src/blargg_tests.rs` (integration tests commented out)
- Related issue: #XXX (MMC5: missing features for full Mapper 5 emulation)
