# Cycle-Accurate DMA in NES (DMC and OAM)

This document describes how **DMC DMA** and **OAM DMA** should be modeled in a cycle-accurate NES emulator.  
Primary references: **NESDev Wiki** and **Mesen2 implementation**.

---

## 1. Overview

The NES has two DMA systems:

| DMA | Purpose | Trigger |
|-----|----------|---------|
| **OAM DMA** | Copies sprite data into PPU OAM | Write to `$4014` |
| **DMC DMA** | Fetches audio sample bytes | APU DMC playback |

Both DMAs:
- Halt the CPU using `/RDY`
- Steal memory cycles
- Must be modeled at CPU-cycle granularity

---

## 2. OAM DMA

### Trigger
Write to `$4014`:

```
STA $4014   ; value = source page (xx00-xxFF)
```

### Operation

1. CPU finishes current instruction
2. DMA halts CPU on next read cycle
3. Optional alignment cycle
4. 256 read/write pairs
5. CPU resumes

### Timing

| Condition | Cycles |
|-----------|--------|
| Aligned | 513 |
| Unaligned | 514 |

### Timeline

```
CPU write $4014
   |
   v
[HALT]         1 cycle
[ALIGN]        0-1 cycles
[READ]         byte 0
[WRITE]        to $2004
...
(repeat 256 times)
...
[CPU RESUME]
```

### Rules

- Transfer from `$xx00-$xxFF`
- Alternate read/write cycles
- Last write to `$4014` wins
- CPU is completely stalled

---

## 3. DMC DMA

### Trigger

Occurs when:
- DMC enabled via `$4015`
- Sample buffer empty
- Bytes remain

### Two cases

| Type | Trigger |
|------|---------|
| Load DMA | Immediately after enabling |
| Reload DMA | During playback |

### Timing

| Type | Cycles |
|------|--------|
| Load | 3 |
| Reload | 4 |

### Sequence

```
[HALT]
[DUMMY]
[ALIGN] (optional)
[READ sample byte]
[CPU RESUME]
```

### Notes

- Always performs a dummy read
- Only 1 byte transferred
- DMA scheduled on specific half-cycles

---

## 4. DMA Priority

**DMC DMA has priority over OAM DMA**

If they collide:
1. DMC executes first
2. OAM pauses
3. OAM realigns

Extra cost:
- Usually +2 cycles

---

## 5. Hardware Quirks

### DMC Bugs

- Aborted DMA if DMC stopped at exact moment
- Extra DMA on late hardware revisions

### Register Reads

During DMA:
- CPU repeatedly re-reads same address
- Can affect:
  - `$2002` (PPUSTATUS)
  - `$2007` (PPUDATA)
  - `$4015` (APU status)

### Controller Reads

- Behavior differs per CPU revision
- Some clones clock controllers incorrectly

---

## 6. Mapper IRQs

- DMA cycles **count** as CPU cycles
- IRQs may be delayed until DMA ends
- Cycle counters must include DMA

---

## 7. Integration Strategy (Rust)

Recommended model:

```rust
loop {
    cpu.tick();

    if dma_active {
        dma.tick();
        continue;
    }

    apu.tick();
    ppu.tick();
}
```

### Rules

- DMA runs **instead of CPU**
- Still increment global cycle counter
- Mappers see stolen cycles
- Interrupts delayed until DMA ends

---

## 8. Implementation Checklist

### OAM DMA

- [ ] Start on CPU read cycle
- [ ] Optional alignment cycle
- [ ] 256 read/write pairs
- [ ] Total 513/514 cycles

### DMC DMA

- [ ] Schedule correctly
- [ ] Always dummy read
- [ ] 1 byte transfer
- [ ] Correct priority

### Conflicts

- [ ] DMC preempts OAM
- [ ] Realign OAM after

---

## 9. References

- NESDev DMA:
  https://www.nesdev.org/wiki/DMA
- NESDev DMC:
  https://www.nesdev.org/wiki/APU_DMC
- Mesen2:
  https://github.com/SourMesen/Mesen2

---

## 10. Notes for neser

Target emulator:
https://github.com/rmstdope/neser

Recommended:
- Central cycle counter
- DMA state machine
- Per-cycle scheduling

---

## End


---

# 11. Exact Cycle Tables (Based on Mesen2)

## OAM DMA (Write to $4014)

| Phase | CPU Cycle | Action |
|--------|------------|--------|
| Trigger | N | CPU writes $4014 |
| Halt | N+1 | CPU halted on read cycle |
| Align | N+2 (optional) | Dummy read if needed |
| Transfer 0 | N+3 | Read $xx00 |
| Transfer 0 | N+4 | Write $2004 |
| ... | ... | Repeat 256 times |
| Final write | N+512 | Write last byte |
| Resume | N+513 / N+514 | CPU resumes |

**Total cycles**
- Aligned: 513 cycles
- Unaligned: 514 cycles

---

## DMC DMA

### Load DMA (after enabling)

| Phase | Cycle |
|--------|--------|
| Halt | N |
| Dummy | N+1 |
| Read sample | N+2 |
| Resume | N+3 |

**Total: 3 cycles**

### Reload DMA

| Phase | Cycle |
|--------|--------|
| Halt | N |
| Dummy | N+1 |
| Align | N+2 |
| Read sample | N+3 |
| Resume | N+4 |

**Total: 4 cycles**

---

# 12. ASCII Timing Diagrams

## OAM DMA

```
CPU:  ----W4014-----------------------------HALTED-----------------------------------RESUME
DMA:        | HALT | ALIGN | R0 | W0 | R1 | W1 | ... | R255 | W255 |
Cycle:      N     N+1     N+2   N+3  N+4   ...               N+512 N+513
```

Legend:
- R = Read from CPU memory
- W = Write to $2004

---

## DMC DMA (Load)

```
CPU:  ----W4015------HALTED----------RESUME
DMA:        | HALT | DUMMY | READ |
Cycle:      N     N+1     N+2    N+3
```

---

## DMC DMA (Reload)

```
CPU:  ----PLAYING------HALTED--------------RESUME
DMA:             | HALT | DUMMY | ALIGN | READ |
Cycle:           N     N+1     N+2     N+3   N+4
```

---

# 13. Collision Case (DMC vs OAM)

```
OAM:  R12 | W12 | R13 | W13 | R14 | W14 |
DMC:                 | HALT | DUMMY | READ |
OAM:                 [PAUSE]   [REALIGN] R14 | W14 |
```

Effect:
- DMC steals cycle
- OAM waits
- OAM realigns
- +2 cycles total

---

# 14. Mesen2 Implementation Notes

Mesen2 behavior:
- DMA only starts on CPU read cycles
- Uses internal scheduler
- DMC has higher priority
- Handles aborted DMAs
- Mapper IRQs see stolen cycles

Key code locations (Mesen2):
- Core/NES/APU/Dmc.cpp
- Core/NES/CPU/Dma.cpp
- Core/NES/CPU/Cpu.cpp

---
