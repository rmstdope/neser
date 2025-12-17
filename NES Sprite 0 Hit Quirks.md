# NES Sprite 0 Hit — Hardware Quirks and Edge Cases

This document describes **real NES PPU hardware quirks** related to **Sprite 0 Hit**. These behaviors are observable on original hardware and are frequently misimplemented in emulators. The focus is on **pixel-pipeline behavior**, **timing races**, and **conditions under which the hit does or does not occur**.

---

## 1. Sprite 0 Hit Is Based on Rendered Pixels

**Hardware behavior:**
Sprite 0 hit is detected using **final rendered pixels**, not logical sprite/background overlap.

- Left-edge clipping
- Fine X scroll
- Forced blanking

All affect hit detection.

**Implication:**
If a pixel is not actually rendered, it cannot generate a hit.

---

## 2. Sprite Priority Bit Is Ignored

**Quirk:**
The sprite priority attribute (behind background) does **not** suppress sprite 0 hit.

```
OAM attribute bit 5 = 1 → hit still occurs
```

**Reason:**
Hit detection occurs **before** the priority mux in the pixel pipeline.

**Practical use:**
Games often hide sprite 0 behind the background while still using it for timing.

---

## 3. Pattern Index 0 Is Always Transparent

**Quirk:**
Pattern index 0 does **not** count for hit detection.

- Applies to background and sprites
- Applies even if a visible color is output

**Example:**
Background color `$3F00` + sprite pixel → **no hit**.

**Reason:**
Hit logic uses **pattern value**, not final palette color.

---

## 4. Left-Edge Clipping Is Applied Before Hit Detection

**Relevant PPUMASK bits:**

```
Bit 1 – Show background in leftmost 8 pixels
Bit 2 – Show sprites in leftmost 8 pixels
```

If either pixel is clipped in X = 0–7, **no sprite 0 hit can occur**.

Clipping zeroes pixels before they reach the hit detector.

---

## 5. Sprite 0 Hit Can Occur on the First Visible Pixel

**Quirk:**
The hit may be set on the **very first pixel** where sprite 0 becomes visible.

**Consequence:**
Polling loops that assume a delay may miss the hit.

No internal buffering delays the hit signal.

---

## 6. Sprite 0 Hit Is Edge-Triggered

Once set:

- It remains set for the rest of the frame
- It is cleared only by:
  - Reading `PPUSTATUS`, or
  - Entering VBlank

Multiple overlaps in one frame have no additional effect.

---

## 7. Race Condition with PPUSTATUS Reads

**Hardware race:**
If the CPU reads `PPUSTATUS` on the **same cycle** the hit would be set:

- The hit may or may not be observed

**Result:**
Some games rely on precise CPU↔PPU cycle alignment.

**Emulator note:**
Accurate cycle synchronization is required to reproduce this behavior.

---

## 8. No Sprite 0 Hit During Forced Blanking

If either rendering bit is disabled:

```
PPUMASK bit 3 = 0 (background disabled)
PPUMASK bit 4 = 0 (sprites disabled)
```

Then:

- No sprite 0 hit can occur

This includes deliberate forced blanking used for timing.

---

## 9. Scanline 0 vs Pre-render Scanline

**Clarification:**

- Sprite 0 hit does **not** occur on the pre-render scanline
- Sprite 0 hit **can** occur on scanline 0 (first visible scanline)

Some emulators incorrectly suppress hits on scanline 0.

---

## 10. PAL vs NTSC Differences

**Logic:**
- Identical on PAL and NTSC

**Timing:**
- Scanline counts and CPU cycles differ

Games that poll sprite 0 hit must be region-aware.

---

## 11. Offscreen Sprites Never Hit

If sprite 0 is:

- Completely offscreen horizontally, or
- Outside visible scanlines

Then no sprite 0 hit can occur, even if tile data overlaps logically.

---

## 12. Sprite Overflow Is Unrelated

Sprite overflow:

- Does not suppress sprite 0 hit
- Does not enable it
- Is evaluated independently

Common emulator bug: coupling overflow and hit behavior.

---

## 13. Pixel Pipeline Ordering (Simplified)

Actual PPU behavior effectively follows this order:

1. Fetch background pixel
2. Fetch sprite pixel
3. Apply left-edge clipping
4. Apply transparency rules
5. Detect sprite 0 hit
6. Apply priority
7. Output final pixel

Changing this order results in incorrect behavior.

---

## 14. Why These Quirks Exist

Sprite 0 hit is not a designed synchronization feature. It is a **side effect** of the NES PPU’s internal pixel pipeline.

Later Nintendo hardware eliminated this behavior entirely.

---

## Summary Table

| Quirk | Real Hardware | Common Emulator Mistake |
|------|---------------|------------------------|
| Priority ignored | Yes | Often wrong |
| Left clipping affects hit | Yes | Often wrong |
| Pattern 0 transparent | Yes | Often wrong |
| PPUSTATUS race | Yes | Rarely implemented |
| Hit on scanline 0 | Yes | Often suppressed |
| Forced blank disables hit | Yes | Often ignored |

---

**Intended audience:** NES emulator authors and low-level PPU implementers.

