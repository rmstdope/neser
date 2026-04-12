# Sweep Cutoff Audio Expectations

This document describes the expected sound output for the `sweep_cutoff.asm` test.

High‑level summary (matches the comment):

1. Silence
2. White noise burst
3. Several steady square‑wave tones in sequence, with no silence between them
4. A sweep (pitch glide) upward, then silence.

Below is the detailed, explicit breakdown in order.

## Timeline (approximate)

### **Initial setup**

- $4015 = $01 enables square channel 1 only.
- $4000 = $BF sets square 1 volume to max (15) and a fixed duty/envelope.
- After this, the code begins programming periods and sweeps.

### **First silence block (about 200 ms).**

```none
$4001 = $A1  (sweep on, period=7)
$4002 = $07  (low period)
$4003 = $00  (high period)
delay 200 ms
```

**What you hear:**
silence (even though square is enabled and volume is max).

**Why silence:**
APU square channels mute when the internal period becomes less than 8 (cutoff). The code deliberately sets a period that is below the audible cutoff, so output is forced to 0.

### **First cutoff test sequence (about 8 × 200 ms)**

```none
A = 1
jsr test_cutoffs
```

`test_cutoffs` runs 8 steps (Y = 7 down to 0).
Each step:
- Sets square period to cutoffs[y] + A (A = 1 here).
- Sets sweep shift = Y.
- Waits 200 ms.

**What you hear:**
No sound throughout this block, because these are still “below‑cutoff” / silenced cases.

### **Sweep-to-silence then sweep-up recovery (about 200 ms)**

```none
$4017 = $C0   ; synchronize APU
$4001 = $89   ; sweep: enabled, negate, shift=1
$4002 = 16
$4003 = 0
$4017 = $C0   ; clock sweep once
; period becomes 7 -> silenced
$4001 = $91   ; switch to add mode to catch >= 8
delay 200 ms
```

**What you hear:**
Silence for this 200 ms.

**Why:**
The forced sweep calculation drops the period from 16 to 7, which is below the cutoff, and the channel mutes. The sweep config is flipped to add mode, but no audible recovery occurs yet in this section.

### **Noise marker (about 200 ms) **

```none
$4015 = $08   ; enable noise channel only
$400C = $3F   ; noise volume high
$400E = $04   ; noise period
$400F = $08   ; length counter reload
delay 200 ms
$4015 = $01   ; back to square 1 only
```

**What you hear:**
A short 200 ms burst of white noise (a harsh hiss).

This is just a separator between the “silent” cutoff tests and the next audible section.

### **Second cutoff test sequence (about 8 × 200 ms)**
```none
A = 0
jsr test_cutoffs
```

Same loop as before, but now A = 0.

**What you hear:**
Several steady square‑wave tones in a row, no silence between them.
Each tone lasts ~200 ms, then immediately changes to the next tone.

The tones will jump in pitch with each step. The exact pitches are not musically tuned; they are APU timer values around the cutoff thresholds.

This is the “several tones without any silence between” mentioned in the header comment.

### Single audible tone at period 8 (about 200 ms)

```none
$4001 = $91
$4002 = $08
$4003 = $00
delay 200 ms
```

**What you hear:**
A single steady square‑wave tone for ~200 ms.
This tone should be audible (period exactly 8 is just above the cutoff).

### **End (silence forever)**

```none
$4015 = $00
```

**What you hear:**
Silence from here on.

## Total audible sequence (human‑ear version)

1. Long-ish silence
2. White noise burst (~200 ms)
3. 8 square‑wave tones, each ~200 ms, no silence between
4. One more square‑wave tone (~200 ms)
5. Silence forever

If you compare to the provided reference WAV, the timing and ordering should match.
