# Sprite Overflow Bug Emulation Implementation Plan

## Overview

Implement the buggy sprite overflow detection behavior as found in real NES hardware. This is one of the most famous bugs in the NES PPU - after finding 8 sprites, the hardware has a bug in how it continues checking for more sprites.

## Background: The Hardware Bug

### Normal Behavior (First 8 Sprites)

- For each sprite n (0-63), check if sprite is on next scanline
- If yes, copy 4 bytes to secondary OAM
- If no, increment n and continue
- After finding 8 sprites, enter overflow check mode

### Buggy Behavior (After 8 Sprites Found)

The hardware has TWO indices:

- `n`: OAM sprite number (0-63, which sprite we're checking)
- `m`: Byte offset within sprite (0-3, which byte we're reading)

**The Bug**: After finding 8 sprites, the hardware should:

1. Check sprite Y at OAM[n*4 + 0]
2. If in range, set overflow flag and increment n
3. If not in range, increment n

**What it actually does**:

1. Check sprite Y at OAM[n*4 + m] (wrong! should always be m=0)
2. If in range, set overflow flag, increment n AND m (bug!)
3. If not in range, increment n AND m (bug!)

This causes:

- **False positives**: Reading wrong bytes (tile, attr, x) as Y coordinates
- **False negatives**: Skipping sprites because m wraps around
- **Diagonal scanning**: n and m both increment, causing diagonal reads through OAM

### Specific Bug Behaviors

1. **Initial state after 8 sprites**: n=8, m=0
2. **First overflow check**: Reads OAM[32] correctly (sprite 8, byte 0 = Y)
3. **If not in range**: n=9, m=1 (BUG! m should stay 0)
4. **Second check**: Reads OAM[37] (sprite 9, byte 1 = TILE) as Y coordinate
5. **If matches by chance**: Sets overflow flag (FALSE POSITIVE)
6. **If not**: n=10, m=2
7. **Third check**: Reads OAM[42] (sprite 10, byte 2 = ATTRIBUTES) as Y
8. **Continues diagonally** through OAM until m wraps to 0 or n reaches 64

### Impact

- Games can have 9+ sprites on scanline without overflow being set
- Overflow flag is unreliable - games rarely use it
- Some games exploit this bug for effects
- Accurate emulation requires implementing the bug

## Implementation Phases

### Phase 1: Add Bug Emulation State (RED-GREEN-REFACTOR)

**Goal**: Add the necessary state variables to track the buggy behavior

**RED**: Write test that expects buggy behavior

- Test with 9 sprites where overflow should trigger
- Test with 9 sprites where sprite 9's tile byte looks like valid Y
- Verify current implementation doesn't have the bug (test should fail)

**GREEN**: Add state variables

- Add `sprite_eval_m: u8` to track byte offset (0-3)
- Initialize to 0 in constructor and reset
- Track when we've entered overflow check mode

**REFACTOR**: Clean up state management

### Phase 2: Implement Buggy Evaluation Logic (RED-GREEN-REFACTOR)

**Goal**: Change evaluation to read from OAM[n*4 + m] instead of OAM[n*4]

**RED**: Write test showing diagonal scanning

- Set up OAM with specific pattern
- Verify reads happen at wrong offsets

**GREEN**: Modify evaluate_sprites()

- After 8 sprites found, enter overflow check mode
- Read Y from OAM[n*4 + m] instead of OAM[n*4 + 0]
- Continue copying sprite data (even though it won't be used)

**REFACTOR**: Simplify logic, add comments

### Phase 3: Implement Buggy Index Increment (RED-GREEN-REFACTOR)

**Goal**: Increment both n and m after each check (the core bug)

**RED**: Write test for false negative

- 9 sprites on scanline, but sprite 9 at wrong byte offset
- Overflow should not be set due to bug

**GREEN**: Modify increment logic

- After overflow check (hit or miss), increment BOTH n and m
- m wraps from 3 to 0
- Continue until n reaches 64

**REFACTOR**: Clean up increment logic

### Phase 4: False Positive Detection (RED-GREEN-REFACTOR)

**Goal**: Set overflow flag when wrong byte matches Y range

**RED**: Write tests for false positives

- Sprite 9's tile/attr/x byte matches as Y coordinate
- Overflow flag should be set

**GREEN**: Implement overflow flag setting

- When OAM[n*4 + m] matches range, set sprite_overflow = true
- Continue evaluation after setting flag

**REFACTOR**: Ensure flag is set correctly

### Phase 5: Edge Cases and Timing (RED-GREEN-REFACTOR)

**Goal**: Handle timing and edge cases

**RED**: Write tests for:

- Overflow flag clearing at pre-render scanline
- Overflow with exactly 8 sprites (should not set)
- Overflow with sprite 8 at m=1,2,3 positions
- m wrapping scenarios

**GREEN**: Fix any edge cases

- Clear overflow flag at scanline 261, dot 1 (same as sprite 0 hit)
- Handle m wrap correctly
- Stop evaluation at sprite 64

**REFACTOR**: Clean up edge case handling

### Phase 6: Documentation and Verification (RED-GREEN-REFACTOR)

**Goal**: Document the bug thoroughly

**RED**: Write integration test

- Complex scenario with multiple sprites
- Verify exact bug behavior matches hardware

**GREEN**: Add comprehensive comments

- Explain the bug in code comments
- Reference NesDev wiki
- Document why this seemingly wrong code is correct

**REFACTOR**: Final cleanup

## Test Strategy

### Unit Tests

1. `test_sprite_overflow_flag_clear_initially` - Flag starts cleared
2. `test_sprite_overflow_with_9_sprites_normal` - Sprite 9 at m=0, should set overflow
3. `test_sprite_overflow_false_positive_tile` - Sprite 9's tile byte matches Y
4. `test_sprite_overflow_false_positive_attr` - Sprite 9's attr byte matches Y
5. `test_sprite_overflow_false_positive_x` - Sprite 9's x byte matches Y
6. `test_sprite_overflow_false_negative` - 9 sprites but overflow not set due to bug
7. `test_sprite_overflow_diagonal_scan` - Verify diagonal OAM reading
8. `test_sprite_overflow_cleared_at_prerender` - Flag clears at scanline 261, dot 1
9. `test_sprite_overflow_exactly_8_sprites` - No overflow with exactly 8
10. `test_sprite_overflow_m_wrap` - Behavior when m wraps from 3 to 0

### Integration Tests

- Complex multi-sprite scenarios
- Verify against known hardware behavior
- Test games that are affected by this bug

## Implementation Notes

### Key References

- NesDev Wiki: PPU sprite evaluation
- Skinny's implementation notes
- Real hardware tests from various emulator developers

### Critical Details

1. Bug only affects sprites 9-64 (after first 8)
2. m increment happens on both hit and miss
3. n increment happens on both hit and miss
4. Evaluation stops when n reaches 64 or pixel >= 256
5. Flag persists until cleared at pre-render scanline

### Common Pitfalls

- Forgetting to increment m on miss
- Not reading from OAM[n*4 + m]
- Clearing overflow flag at wrong time
- Not handling m wrap correctly

## Success Criteria

- All new tests pass
- All existing tests still pass
- Bug behavior matches real hardware
- Code is well-documented explaining the bug
- False positives and false negatives work correctly

## Estimated Complexity

**Medium-High** - The bug logic is well-documented but requires careful attention to detail. The challenge is implementing something that seems wrong but is actually correct emulation of hardware bugs.
