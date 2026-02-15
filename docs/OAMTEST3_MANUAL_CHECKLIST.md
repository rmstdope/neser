# OAMTEST3 Manual Validation Checklist

## Purpose

`oam3.nes` is an interactive **visual** test ROM for OAM behavior around partial OAM writes and sprite 0/1 replacement behavior. It does **not** print a final `PASSED`/`FAILED` string.

This checklist makes the test repeatable and gives concrete expected behavior.

## Scope and Caveats

- Target PPU: NTSC `2C02` behavior.
- Not intended for PAL `2C07` (forum notes explicitly say behavior differs).
- This ROM is a manual visual probe, not a fully automated self-asserting test.

## Launch

Run with keyboard-only input to avoid gamepad-to-port assignment ambiguity:

```bash
cargo run --bin neser --all-features -- --no-audio --no-gamepads roms/automated_tests/oamtest3/oam3.nes
```

Optional I/O trace mode:

```bash
NESER_TRACE_OAM3_IO=1 cargo run --bin neser --all-features -- --no-audio --no-gamepads roms/automated_tests/oamtest3/oam3.nes
```

## Controls

Keyboard mapping in SDL frontend:

- `W` = Up
- `S` = Down
- `A` = Left
- `D` = Right
- `F` = NES A
- `G` = NES B
- `R` = Select
- `T` = Start

Only `W/S/A/D` are needed for this test.

## On-Screen Model

- Row with hex bytes: editable values used for OAM upload payload.
- `!` marker row: current nibble/cursor position.
- Leftmost value controls number of bytes uploaded each input-triggered update (clamped to `0..14`).

## Validation Procedure

### 1) Basic sanity (ROM is alive)

- Press `A`/`D`: `!` marker should move.
- Press `W`/`S`: selected hex nibble should change.

If these do not change, input routing is still broken.

### 2) Prepare payload while minimizing OAM drift

Because this ROM applies edits immediately on each keypress, treat setup as approximate and compare behavior across fresh resets.

Recommended workflow:

- Reset/relaunch ROM before each scenario.
- Edit payload bytes first.
- Then move count into target range for that scenario and observe immediately.

### 3) Set a reference payload

Use `W/S/A/D` to set the first 14 bytes to:

- `34 56 78 9A BC DE F0 00 54 32 10 EC A8 64`

(These are the exact bytes shown in lidnariq’s annotated hardware example.)

### 4) Exercise the critical range

- Keep changing values and move the cursor so that updates are triggered.
- In particular, test with leftmost byte count in the `8..14` range.

### 5) Compare against below-range behavior

- In a fresh run, observe behavior around count `7` (below critical range).
- In another fresh run, observe behavior around count `14` (critical range).
- Compare qualitative behavior, not exact pixel-for-pixel persistence.

### 6) Compare behavior when reducing count

- Change leftmost count from `14` down to `7` and keep triggering updates.

This transition is the specific hardware-vs-emulator discriminator described in the NESdev posts.

## Expected vs Incorrect Visual Outcome

### Expected (hardware-correct)

When upload count is `8..14`, sprite 0/1 should not behave like stable independent sprites; they should be concealed/replaced per the OAMADDR-coupled behavior described by lidnariq.

Note: seeing only one clearly visible sprite at count `14` can still be consistent with correct behavior, depending on current OAMADDR history and what bytes were last written.

### Incorrect (known bad emulator behavior)

Sprites 0/1 visibly reappear after the frame where input was pressed in the `8..14` range.

If you observe this reappearance, behavior does **not** match the described hardware result.

## Quick Visual Identifier for Sprite 0/1 vs 2/3

Use this when you want an unambiguous on-screen discriminator.

Sprite entries are 4 bytes each: `Y, tile, attr, X`.

- Sprite 0 = bytes `0..3`
- Sprite 1 = bytes `4..7`
- Sprite 2 = bytes `8..11`
- Sprite 3 = bytes `12..15`

### Suggested setup

Set the first 16 bytes to this pattern:

- `50 00 00 30`
- `70 01 00 30`
- `90 0E 00 A0`
- `B0 0F 00 A0`

Equivalent flat byte list:

- `50 00 00 30 70 01 00 30 90 0E 00 A0 B0 0F 00 A0`

Interpretation:

- Sprites 0/1 are near the **left** (`X=30`) and use low tile IDs (`00`,`01`).
- Sprites 2/3 are near the **right** (`X=A0`) and use different tile IDs (`0E`,`0F`).

### How to use it

1. Set leftmost byte count to `14` and trigger updates.
2. Observe whether the left pair (sprite 0/1) behaves independently or appears replaced by the right pair’s data signature.
3. Change count down to `7` and compare transition behavior.

If the left pair appears to mirror/duplicate the right pair in the critical `8..14` range and does not spuriously reappear as independent sprites, that matches the hardware-described behavior.

### Visual placement sketch (quick reference)

Approximate intent for the suggested bytes:

```text
X ~ 0x30 (left)                     X ~ 0xA0 (right)

Y~0x50 : [sprite0 tile00]
Y~0x70 : [sprite1 tile01]

Y~0x90 :                             [sprite2 tile0E]
Y~0xB0 :                             [sprite3 tile0F]
```

What to look for in the critical `8..14` byte-count range:

- Hardware-described behavior: left pair (sprite 0/1) does not behave as an independent pair after updates; it follows duplication/replacement behavior tied to sprite 2/3.
- Incorrect behavior: left pair reappears as distinct, independent sprites after the input-triggered frame.

## Pass/Fail Decision Rule

Mark **PASS** if all are true:

- Interactive editing works (`W/S/A/D` visibly changes marker/values).
- Below range (`<=7`), sprite behavior is more independent than in the critical range.
- In critical range (`8..14`), sprite 0/1 do **not** spuriously reappear as stable independent sprites after input-triggered updates.
- Transitioning between `14` and `7` shows the expected qualitative shift (coupled/replaced behavior at high count vs more independent behavior below 8).

Mark **FAIL** if sprite 0/1 reappearance is observed in that critical range.

## References

- Forum discussion and ROM release context: `https://forums.nesdev.org/viewtopic.php?p=128842#p128842`
- Follow-up with annotated hardware photos: `https://forums.nesdev.org/viewtopic.php?p=128913#p128913`
