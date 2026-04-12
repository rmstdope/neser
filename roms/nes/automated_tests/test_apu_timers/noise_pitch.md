# Noise Pitch Audio Expectations

This document describes the expected sound output for the `noise_pitch.asm` test.

High‑level summary (matches the comment):

1. Constant‑timbre noise at a fixed pitch (noise period index 15).
2. A continuous CPU‑timed $4011 DAC toggle acting as a reference tone.
3. Output runs forever; no end‑of‑test silence.

Below is the detailed, explicit breakdown in order.

## Timeline (approximate)

### **Initial setup / hardware delay**

- The routine waits briefly for hardware to settle.
- NMI is disabled.
- Noise channel is enabled at maximum constant volume.
- Noise period is set to index 15 (slowest noise clock, lowest pitch).

```none
$4015 = $08  ; enable noise
$400C = $7F  ; constant volume = 15
$400E = $0F  ; mode=0 (long LFSR), period index=15
$400F = $00  ; length counter reload (immediate), no envelope reset effects
```

**What you hear:**
A steady noise tone begins (low‑pitched hiss/buzz) with no envelope changes.

### **Main loop (continuous)**

The loop repeatedly performs a fixed‑length CPU delay, then writes to $4011 and toggles the value by XORing with 25 (0x19). This creates a continuous, CPU‑timed DAC toggle tone that mixes with the noise channel.

```none
$4011 = alternating values (… ^ 0x19 …)
```

**What you hear (continuous):**

- A **steady‑timbre noise** at the fixed pitch for period index 15.
- A **stable, synthetic “buzzy” reference tone** from $4011 toggling at a constant rate.
- No pitch sweep, no gaps, no phase modulation in the noise.

### **End**

There is no termination; the loop runs forever.

**What you hear:**
Continuous noise + reference tone indefinitely.

## Total audible sequence (human‑ear version)

1. (Brief setup) Noise starts.
2. Constant low‑pitched noise plus a steady $4011 reference tone.
3. Continues forever.

## Notes / common failure indicators

- If the **noise seems to beat/phase** or the pitch wobbles, the noise timer period handling is likely wrong.
- If the **reference tone drifts**, check CPU instruction timing and $4011 write timing.
- Compare against `noise_pitch.wav` for expected behavior and `noise_pitch_bad.wav` for incorrect output.
