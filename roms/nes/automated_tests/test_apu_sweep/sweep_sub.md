`# Sweep Sub Audio Expectations

This document describes the expected sound output for the `sweep_sub.asm` test.

High‑level summary (matches the comment):

1. A continuous square‑wave note plays for 12 × 200 ms windows.
2. Slight periodic clicks are audible.
3. Halfway through, the pitch drops very slightly.
4. Then it stops (silence forever).

Below is the detailed, explicit breakdown in order.

## Timeline (approximate)

### **Initial setup**

- $4017 = $C0 synchronizes the APU frame counter.
- $4015 = $01 enables square channel 1 only.
- $4000 = $BF sets square 1 to constant volume 15 with a 50% duty cycle.
- A short startup delay (~250 ms) occurs before these writes.

### **Run 1: steady tone with periodic clicks (about 6 × 200 ms)**

```none
A = 0
jsr run_test
```

`run_test` performs 7 steps (Y = 7 down to 1). Each step lasts ~200 ms. In practice, about 6
of those windows are clearly audible per run.

1. Sets the pulse timer period from `table_h/table_l` plus offset A.
2. Enables sweep **negate** with shift = Y (`$4001 = $88 | Y`).
3. Immediately clocks the sweep once via `$4017 = $C0`.
4. Writes `$4003` again (timer high + length reload), which resets the sequencer.
5. Disables sweep (`$4001 = $00`).
6. Waits 200 ms.

**What you hear:**

- About **6 audible 200 ms windows** in this run.
- A **continuous, steady square‑wave tone** across the audible windows.
- **Small periodic clicks** at the step boundaries (from the immediate sweep clock and `$4003` write/reset).
- The pitch is stable within each 200 ms step, and any changes between steps are subtle.

### **Run 2: same pattern, slightly lower pitch (about 6 × 200 ms)**

```none
A = 1
jsr run_test
```

The second run repeats the same 7 steps, but the final low‑byte offset is incremented by 1. This shifts the underlying timer values slightly.

**What you hear:**

- About **6 audible 200 ms windows** in this run.
- The **same continuous tone** with periodic clicks at each 200 ms step.
- A **very slight drop in pitch** compared to Run 1 (audible if listening carefully or comparing the waveform).

### **End (silence forever)**

```none
$4015 = $00
```

The square channel is disabled and the ROM loops forever.

## Audible characteristics

- **Continuous tone:** Both runs play without gaps; the tone does not stop between steps.
- **Clicks:** Small ticks occur at each step due to sweep clocking + timer reload.
- **Pitch drop:** The second run is slightly lower in pitch than the first, matching the reference WAV.

## Summary

Expected order of audible events:

1. Short initial silence (~250 ms)
2. Continuous square‑wave tone with small periodic clicks (about 6 × 200 ms)
3. Same tone pattern, slightly lower pitch (about 6 × 200 ms)
4. Silence forever
