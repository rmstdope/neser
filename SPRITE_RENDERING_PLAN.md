# Sprite Rendering Implementation Plan

Following TDD approach - implement features incrementally with tests.

## Phase 1: Sprite Shift Registers and Basic Structure

- [ ] Add sprite shift register fields to PPU struct (8 sprites × pattern lo/hi)
- [ ] Add sprite attribute latches (8 sprites × x position, attributes)
- [ ] Add test for sprite shift register initialization

## Phase 2: Sprite Pattern Fetching (Dots 257-320)

- [ ] Implement fetch cycle detection for sprites (dots 257-320)
- [ ] Implement sprite pattern table address calculation
- [ ] Test: Verify pattern table selection from PPUCTRL bit 3
- [ ] Implement 8x8 sprite pattern fetching
- [ ] Test: Verify correct pattern data is fetched for 8x8 sprites
- [ ] Implement 8x16 sprite pattern fetching
- [ ] Test: Verify correct pattern data is fetched for 8x16 sprites

## Phase 3: Sprite Attribute Handling

- [ ] Implement sprite attribute byte parsing
- [ ] Test: Verify palette selection (bits 0-1)
- [ ] Test: Verify priority bit (bit 5)
- [ ] Implement horizontal flip logic
- [ ] Test: Verify horizontal flip (bit 6)
- [ ] Implement vertical flip logic
- [ ] Test: Verify vertical flip (bit 7)

## Phase 4: Sprite Rendering

- [ ] Implement sprite pixel output during visible pixels
- [ ] Test: Verify sprite renders at correct X position
- [ ] Implement transparency check (palette index 0)
- [ ] Test: Verify transparent pixels don't render
- [ ] Implement sprite priority (foreground/background)
- [ ] Test: Verify background priority works correctly
- [ ] Implement sprite-to-sprite priority
- [ ] Test: Verify lower OAM index wins

## Phase 5: Sprite 0 Hit

- [ ] Implement sprite 0 hit detection
- [ ] Test: Verify sprite 0 hit sets on opaque pixel overlap
- [ ] Test: Verify sprite 0 hit doesn't set at x=255
- [ ] Test: Verify sprite 0 hit clears on pre-render scanline

## Phase 6: Integration

- [ ] Enable sprite rendering via PPUMASK bit 4
- [ ] Implement sprite clipping (PPUMASK bit 2)
- [ ] Test with actual ROM (e.g., Donkey Kong, Pac-Man)
