# NES X-Axis Pipeline Offset and Its Interaction with Sprite 0 Hit

This document explains the commonly observed **“−2 pixel X offset”** in NES rendering. It clarifies why the offset exists, how it affects **both background and sprite rendering**, and—critically—**how it interacts with Sprite 0 Hit detection**. The explanation is hardware-accurate and intended for emulator authors.

---

## 1. The Observed Phenomenon

Many emulator developers notice that:

- Background graphics appear shifted **~2 pixels left**
- Sprite pixels seem to align only after applying a `+2` X correction
- Sprite 0 hit appears to occur **earlier than expected**

This often leads to the statement:

> “The NES renders at X − 2.”

This statement is **directionally correct**, but **mechanically misleading**.

---

## 2. There Is No Explicit X Offset Register

The NES PPU does **not** apply a literal coordinate shift such as:

```text
screen_x = internal_x - 2
```

Instead, the effect arises from **pixel pipeline latency** caused by:

- Prefetching tile data
- Shift-register–based pixel output
- Rendering driven by **PPU dots (cycles)**, not pixel counters

The “−2” is an **emergent property** of this pipeline.

---

## 3. Background Rendering Pipeline and the −2 Effect

### 3.1 Pipeline overview

For background rendering:

1. Tile and attribute data are fetched **ahead of time**
2. Pattern bits are loaded into 16-bit shift registers
3. Registers are shifted **every PPU dot**
4. Output pixels come from the high bits of the shifters

Crucially:

- Shifters are already **two shifts ahead** of the visible pixel
- Fine X scroll selects bits from a pipeline that is already advanced

---

### 3.2 Practical consequence

If you compute screen X naïvely as:

```c
screen_x = dot - 1;
```

and mentally expect tile boundaries to line up exactly, the background will appear **2 pixels left** of expectations.

Correct mental model:

> The PPU’s internal pixel cursor is **~2 pixels ahead** of the visible screen X coordinate.

---

## 4. Fine X Scroll Amplifies the Illusion

Fine X scroll (`0–7`) works by **bit-selecting** into the background shifters.

Because the shifters are already advanced:

- Fine X = 0 still skips pixels that are already in-flight
- Rendering behaves as if it started at **X = −2**

This is why fine X handling and the −2 effect are inseparable.

---

## 5. Sprite Rendering Pipeline and the Same Offset

Sprites are rendered using **separate shifters**, but with similar timing properties.

### Sprite specifics

- Sprite pattern bits are fetched during sprite fetch cycles
- Bits are loaded into shifters
- The first visible sprite pixel appears **after two shifts**

Thus:

> Sprite shifters are also **~2 pixels ahead** of visible output.

---

## 6. Why Background and Sprites Still Align

This is a key point:

- Background pipeline: ~2 pixels ahead
- Sprite pipeline: ~2 pixels ahead

Because both pipelines are offset by the **same amount**:

- Sprites align correctly with the background
- Priority logic works
- Sprite 0 hit works

The offset only becomes visible when mapping internal state to screen-space coordinates manually.

---

## 7. Interaction with Left-Edge Clipping

Left-edge clipping applies to **screen pixels X = 0–7** after pipeline delay.

Because of the internal lead:

- Internal X ≈ −2 to 5 maps to screen X = 0–7

If clipping is applied too early (before the pipeline delay), results will be incorrect.

---

## 8. Interaction with Sprite 0 Hit (Critical)

Sprite 0 hit detection occurs **after**:

- Background and sprite shifters have advanced
- Fine X scroll has been applied
- Left-edge clipping has been applied

### 8.1 What this means

- Sprite 0 hit is evaluated using the **same pipelined pixels** that are output to the screen
- The internal −2 offset is already “baked in”

Therefore:

> Sprite 0 hit does **not** require any special X correction.

---

### 8.2 Common emulator bug

A frequent mistake is:

- Applying a manual `x -= 2` correction
- Then testing sprite 0 hit using corrected coordinates

This causes:

- Sprite 0 hit to fire too early or too late
- Incorrect behavior near the left edge
- Broken timing in status-bar splits

---

### 8.3 Correct model for Sprite 0 Hit

Sprite 0 hit should be detected when:

- Background pixel (after clipping) is non-transparent
- Sprite 0 pixel (after clipping) is non-transparent
- Both pixels correspond to the **current PPU dot**

Not when tile coordinates overlap.

---

## 9. Recommended Emulator Model

### Dot-driven rendering

Correct approach:

- Advance rendering **one PPU dot at a time**
- Shift background and sprite shifters every dot
- Evaluate sprite 0 hit using the **current output pixel**

Example sketch:

```c
// Each visible PPU dot
shift_bg_shifters();
shift_sprite_shifters();

bg = bg_pixel();
spr = sprite_pixel();

if (sprite0 && bg != 0 && spr != 0)
    set_sprite0_hit();

framebuffer[scanline][dot - 1] = mix(bg, spr);
```

No coordinate offsets are needed.

---

## 10. What Not to Do

❌ Do not apply a literal −2 X correction
❌ Do not offset sprites and background differently
❌ Do not evaluate sprite 0 hit in tile space
❌ Do not clip pixels before pipeline delay

All of these will break real hardware behavior.

---

## 11. Why the −2 Offset Is Still Mentioned

The phrase exists because:

- Humans reason in pixels and tiles
- The PPU operates in cycles and pipelines

“−2 pixels” is a **debugging heuristic**, not a hardware rule.

---

## 12. Summary

- The NES has an **effective ~−2 pixel X offset**
- It is caused by **pipeline latency**, not a coordinate shift
- Both background and sprites are affected equally
- Sprite 0 hit naturally incorporates this offset
- Correct emulation requires **cycle-accurate, dot-driven rendering**

---

**Intended audience:** NES emulator developers implementing accurate PPU rendering and sprite 0 hit behavior.

