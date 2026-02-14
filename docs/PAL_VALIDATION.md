# PAL Validation and Testing Guide

## Overview

This document outlines PAL (Phase Alternating Line) validation procedures for neser, the NES emulator. PAL is the 50 Hz television standard used in Europe, Australia, and other regions. The PAL NES differs from the NTSC NES (used in North America and Japan) in several important ways.

## PAL Hardware Differences

### Clock Rates

| Property | NTSC | PAL |
| ---------- | ------ | ----- |
| Master Clock | 21.477272 MHz | 26.601712 MHz |
| CPU Clock | 1.789773 MHz | 1.662607 MHz |
| PPU Dots per CPU Cycle | 3 | 3.2 |
| Frame Rate | 60 Hz | 50 Hz |
| CPU Cycles per Scanline | ~113.67 | ~106.56 |

### PPU Timing

- **Visible Picture Height**: 239 scanlines (vs 240 NTSC)
- **Vertical Blanking Lines**: 70 scanlines (vs 20 NTSC)
- **Total Scanlines per Frame**: 312 (vs 262 NTSC)
- **Border Behavior**: Black border extends 2 pixels into left and right edges, 1 pixel into top
- **Color Emphasis Bits**: Red and green bits are swapped in PPUMASK register

### APU Timing

- **Frame Counter Rate**: 50 Hz (vs 60 Hz NTSC)
- **DMC and Noise Tables**: Different frequency divider values
- **Overall APU Behavior**: Closely tied to CPU clock rate differences

## PAL Test ROMs

### Required PAL Validation ROMs

The following test ROMs are specifically designed for PAL validation. They are available from the NESDev community and are critical for verifying PAL correctness:

#### 1. **nmi_sync** (blargg)

- **Location**: NESDev test ROM repository
- **Purpose**: Tests NMI timing accuracy with PAL-specific timing
- **Variants**: Contains both NTSC and PAL versions
- **Download**: Available at <https://github.com/christopherpow/nes-test-roms/tree/master/scanline>
- **Test Format**: Visual output test - produces specific pattern if timing is correct

#### 2. **pal_apu_tests** (blargg)

- **Location**: NESDev test ROM repository
- **Purpose**: PAL-specific APU tests (length counters, frame counter, IRQ, etc.)
- **Variants**: PAL versions matching blargg_apu_2005.07.30
- **Download**: Available at <https://github.com/christopherpow/nes-test-roms/tree/master/apu>
- **Test Format**: Text-based output tests

#### 3. **tvpassfail** (tepples)

- **Location**: NESDev test ROM repository
- **Purpose**: NTSC/PAL color and pixel aspect ratio test
- **Download**: Available at <https://github.com/christopherpow/nes-test-roms>
- **Test Format**: Visual pattern test

### Obtaining Test ROMs

1. Clone or download the NESDev test ROM repository:

   ```bash
   git clone https://github.com/christopherpow/nes-test-roms.git
   ```

2. Copy PAL-specific ROMs to `roms/automated_tests/`:
   - Copy `nmi_sync/demo_pal.nes` to `roms/automated_tests/nmi_sync/`
   - Copy `apu/pal_apu_tests` directory to `roms/automated_tests/`
   - Copy `tvpassfail.nes` (PAL variant) to `roms/automated_tests/`

## Running PAL Validation Tests

### Quick PAL Test

To run the emulator with PAL mode enabled:

```bash
cargo run --release --features sdl -- --pal roms/games/your_pal_game.nes
```

Or via configuration:

```bash
echo "region=PAL" >> neser.conf
cargo run --release --features sdl roms/games/your_pal_game.nes
```

### Automated PAL ROM Testing

Add PAL versions of test ROMs to the integration test suite. Example test structure:

```rust
#[cfg(test)]
mod pal_tests {
    use crate::setup_rom_test;
    
    // NMI sync PAL test
    #[test]
    fn test_nmi_sync_demo_pal() {
        // Load nmi_sync/demo_pal.nes
        // Verify frame output matches expected pattern
    }
    
    // APU tests for PAL
    setup_rom_console_test!(
        test_pal_apu_tests_1,
        "roms/automated_tests/pal_apu_tests/1.len_ctr.nes",
        "$01"
    );
}
```

### Manual Testing Steps

1. **Enable PAL Mode**: Set `--pal` flag or configure region in config file
2. **Visual Inspection**: Look for:
   - Correct frame rate (50 Hz display)
   - Proper border rendering (black edges)
   - No timing artifacts or visual glitches
3. **Audio Verification**: Verify audio pitch and timing matches PAL-expected frequencies
4. **Performance Measurement**: Monitor frame timing on real PAL hardware or PAL-capable emulator

## Known PAL Games with NTSC Compatibility Issues

PAL games that are known to exhibit different behavior or issues when run under NTSC timing:

### Critical Issues

| Game | Issue | NTSC Behavior | PAL Behavior |
| ------ | ------- | --------------- | -------------- |
| Kirby's Adventure | Sprite lag/timing | Can be visible | Correct |
| Castlevania IV | Projectile timing | Incorrect shot speed | Correct |
| Double Dragon II | NMI timing | Audio sync issues | Correct |

### Behavioral Differences

- **Music Pitch**: Songs may play at wrong frequency (PAL games typically run slower on NTSC)
- **Character Speed**: Sprites may move too fast under NTSC timing
- **NMI Timing**: Frame syncing may cause visual glitches or timing-dependent bugs
- **APU Behavior**: Audio envelope, decay, and DMC timing will be incorrect

### Test Games Recommended

For manual validation of PAL support:

1. **Kirby's Adventure** - Good for testing sprite and scroll behavior
2. **Super Mario Bros.** - Basic rendering and timing test
3. **The Legend of Zelda: A Link to the Past** (if available on NES) - Complex graphics and timing
4. **Castlevania** series - Tests projectile and enemy timing
5. **Any PAL-exclusive release** - Most likely to expose issues

## Validation Checklist

Before considering PAL support complete:

- [ ] All PAL test ROMs run successfully
- [ ] NMI timing matches specification (70 VBlank scanlines)
- [ ] APU frame counter operates at 50 Hz
- [ ] Visible picture is 239 scanlines at correct timing
- [ ] Border rendering shows black edges appropriately
- [ ] Color emphasis bits are correct (red/green swapped)
- [ ] Manual testing shows PAL games run correctly
- [ ] No regression in NTSC test ROMs
- [ ] Frame rate display shows 50 Hz for PAL

## Configuration

Set PAL mode in `neser.conf`:

```ini
# PAL mode (50 Hz)
region=PAL
```

Or via command line:

```bash
cargo run --release --features sdl -- --pal game.nes
```

## Related Issues

- **Issue #480**: Main PAL support implementation issue
- **Issue #483**: PPU PAL timing implementation
- **Issue #484**: APU PAL timing implementation
- **Issue #485**: Frontend PAL frame pacing (SDL, web)
- **Issue #486**: ROM metadata detection for PAL

## References

- [NESDev Clock Rate Documentation](https://www.nesdev.org/wiki/Cycle_reference_chart)
- [NESDev PPU Rendering](https://www.nesdev.org/wiki/PPU_rendering)
- [NESDev Emulator Tests](https://www.nesdev.org/wiki/Emulator_tests)
- [NESDev Test ROMs Repository](https://github.com/christopherpow/nes-test-roms)

## Notes

- Always test both NTSC and PAL variants to ensure no regressions
- PAL validation is an ongoing process as edge cases are discovered
- Some Dendy (Russian famiclone) features may differ; refer to main #480 issue for clarification
