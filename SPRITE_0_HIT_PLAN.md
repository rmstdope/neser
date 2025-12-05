# Sprite 0 Hit Detection Implementation Plan

## Overview

Sprite 0 hit is a critical feature for raster effects in NES games. It sets a flag (PPUSTATUS bit 6) when the first non-transparent pixel of sprite 0 overlaps with a non-transparent background pixel.

## Current State

- ✅ `sprite_0_hit` flag exists in PPU struct
- ✅ Flag is initialized to false and cleared on reset
- ✅ Flag is included in PPUSTATUS register (bit 6)
- ❌ No detection logic implemented
- ❌ No flag clearing at pre-render scanline
- ❌ No tests for sprite 0 hit

## Implementation Requirements (from Issue #8)

### Core Detection Logic

1. **Pixel-level overlap detection**

   - Check if sprite 0's current pixel is non-transparent (pattern != 0)
   - Check if background's current pixel is non-transparent (bg_pixel != 0)
   - Both must be true for a hit

2. **Timing Constraints**

   - Only detect during visible scanlines (0-239)
   - Only detect at visible pixels (dots 1-256)
   - Ignore leftmost 8 pixels if SHOW_SPRITES_LEFT or SHOW_BACKGROUND_LEFT is disabled

3. **Flag Behavior**
   - Set when first overlap detected
   - Stays set until cleared
   - Cleared at dot 1 of pre-render scanline (261)
   - Reading PPUSTATUS does NOT clear this flag (unlike VBlank)

### Implementation Location

The detection should be added in `render_pixel_to_screen()` where we:

1. Already have bg_pixel value
2. Already have sprite_pixel value with sprite index
3. Can check if sprite_idx == 0
4. Can check both pixels are non-transparent
5. Can verify we're in visible area

### Edge Cases to Handle

1. **Clipping**

   - If SHOW_BACKGROUND_LEFT is disabled and pixel < 8, bg is clipped → no hit
   - If SHOW_SPRITES_LEFT is disabled and pixel < 8, sprite is clipped → no hit
   - Detection logic must respect both clipping flags

2. **Background priority**

   - Hit detection is independent of sprite priority bit
   - Even if sprite is behind background, hit still occurs if both pixels opaque

3. **Sprite 0 not present**

   - If sprite 0 is not in the current scanline's sprite list, no hit can occur
   - Need to track if sprite 0 is in secondary OAM

4. **Off-screen sprites**
   - Sprite Y >= 0xEF puts sprite off-screen → no hit possible
   - X position > 255 puts sprite off-screen → no hit possible

## Implementation Plan (TDD Approach)

### Phase 1: Basic Detection (RED-GREEN-REFACTOR)

1. **RED**: Write test for sprite 0 hit at center of screen

   - Setup: Sprite 0 at (100, 100), background tile at same position
   - Both have opaque pixels
   - Assert: sprite_0_hit flag set after rendering

2. **GREEN**: Implement basic detection in render_pixel_to_screen()

   - Check if sprite_idx == 0
   - Check if both pixels are opaque
   - Set sprite_0_hit flag

3. **REFACTOR**: Clean up if needed

### Phase 2: Timing and Clearing (RED-GREEN-REFACTOR)

1. **RED**: Write test for flag clearing at pre-render scanline

   - Set sprite_0_hit to true
   - Run pre-render scanline
   - Assert: flag cleared at dot 1

2. **GREEN**: Implement clearing in tick_ppu_cycle()

   - At scanline 261, pixel 1, clear sprite_0_hit

3. **REFACTOR**: Clean up if needed

### Phase 3: Clipping (RED-GREEN-REFACTOR)

1. **RED**: Write tests for leftmost 8 pixels clipping

   - Test 1: Sprite 0 at X=4, clipping disabled → hit occurs
   - Test 2: Sprite 0 at X=4, SHOW_SPRITES_LEFT disabled → no hit
   - Test 3: Sprite 0 at X=4, SHOW_BACKGROUND_LEFT disabled → no hit

2. **GREEN**: Implement clipping checks

   - Check screen_x < 8 && clipping flags
   - Skip detection if clipped

3. **REFACTOR**: Clean up if needed

### Phase 4: Miss Scenarios (RED-GREEN-REFACTOR)

1. **RED**: Write tests for scenarios where hit should NOT occur

   - Sprite 0 transparent pixel over opaque background → no hit
   - Opaque sprite 0 over transparent background → no hit
   - Sprite 0 at Y >= 0xEF (off-screen) → no hit
   - Different sprite (not sprite 0) overlapping background → no hit

2. **GREEN**: Verify implementation handles these correctly

3. **REFACTOR**: Clean up if needed

### Phase 5: Flag Persistence (RED-GREEN-REFACTOR)

1. **RED**: Write test for flag staying set

   - Trigger hit at scanline 100
   - Continue rendering through scanline 200
   - Assert: flag still set
   - Read PPUSTATUS → flag still set

2. **GREEN**: Verify flag persistence (should already work)

3. **REFACTOR**: Clean up if needed

## Code Locations

### Files to Modify

- `src/ppu.rs`:
  - `render_pixel_to_screen()` - Add detection logic
  - `tick_ppu_cycle()` - Add flag clearing at scanline 261, pixel 1
  - Add tests in `#[cfg(test)] mod tests`

### Sprite 0 Tracking

We need to track if sprite 0 is in the current scanline:

- During sprite evaluation, mark if sprite at OAM index 0 was found
- Store this info (could add `sprite_0_in_range: bool` flag)
- Or check secondary_oam to see if it contains sprite from OAM[0]

## Testing Strategy

### Test Cases to Implement

1. `test_sprite_0_hit_basic` - Basic hit detection at center
2. `test_sprite_0_hit_cleared_at_prerender` - Flag clearing
3. `test_sprite_0_hit_with_sprite_clipping` - Left 8 pixels, sprite clipping
4. `test_sprite_0_hit_with_background_clipping` - Left 8 pixels, BG clipping
5. `test_sprite_0_miss_sprite_transparent` - Transparent sprite pixel
6. `test_sprite_0_miss_background_transparent` - Transparent background
7. `test_sprite_0_miss_different_sprite` - Sprite 1, not sprite 0
8. `test_sprite_0_hit_flag_persists` - Flag stays set
9. `test_sprite_0_hit_visible_area_only` - Only in scanlines 0-239, pixels 1-256

### Test Helper Functions

Consider creating helper functions:

- `setup_sprite_0_test_scene()` - Common setup for sprite 0 tests
- `render_scanline()` - Render a full scanline
- `check_sprite_0_hit()` - Read PPUSTATUS and check bit 6

## Success Criteria

- ✅ All 10 checklist items from issue #8 completed
- ✅ All new tests passing (target: 9+ new tests)
- ✅ All existing tests still passing
- ✅ Code follows TDD methodology (RED-GREEN-REFACTOR)
- ✅ Implementation handles all edge cases
- ✅ Code is well-documented with comments

## Notes

- Sprite 0 hit is used for split-screen effects (e.g., status bar in Super Mario Bros)
- Games often place sprite 0 at a specific Y coordinate and wait for the hit
- This allows changing scroll registers mid-frame for raster effects
- Detection must be pixel-perfect for games to work correctly

## References

- NesDev Wiki: https://www.nesdev.org/wiki/PPU_OAM#Sprite_0_hits
- PPUSTATUS register: https://www.nesdev.org/wiki/PPU_registers#PPUSTATUS
