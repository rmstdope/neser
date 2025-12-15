# High-Accuracy NES Emulator Architecture & Design Report

## Overview

This document outlines how to architect and design a **high‑accuracy NES
emulator**, with a focus on coordinating the **CPU**, **PPU**, and
**APU** within a deterministic timing model. The guidance assumes an
implementation in **Rust**, emphasizes correctness over
tooling/debugging features, and highlights architectural considerations
needed to reach cycle accuracy.

------------------------------------------------------------------------

## 1. Architectural Philosophy

### \### Accuracy-First Goals

-   Maintain **cycle‑exact behavior** for CPU, PPU, and APU.
-   Model real hardware timing rather than approximate timing.
-   Avoid frame-based or instruction-based stepping; operate on **clock
    events**.
-   Treat the emulator as a set of **hardware components that react to
    shared time**.

### \### Rust-Specific Benefits

-   Zero-cost abstractions allow clean separation of components without
    runtime overhead.
-   Enums, traits, and ownership modeling help enforce correct data
    flow.
-   Borrow checker ensures safe memory mirroring and bus interactions.

------------------------------------------------------------------------

## 2. System Clock Model

### \### Master Cycle Relationships

  Component    Frequency               Ratio to CPU
  ------------ ----------------------- ----------------------------
  CPU (2A03)   \~1.789773 MHz (NTSC)   1
  PPU          5.369318 MHz            3 PPU cycles per CPU cycle
  APU          Clocked by CPU cycle    1

**All timing originates from the CPU master clock.**

### \### Time Scheduler

Implement a **priority event queue**, where each subsystem requests its
next wake-up time.

Typical events: - CPU executes next cycle - PPU renders next dot - APU
frame sequencer step - APU DMC memory fetch - Mapper IRQ edge - NMI and
IRQ dispatch

------------------------------------------------------------------------

## 3. CPU Integration

### \### Responsibilities

-   Drives system timing.
-   Coordinates memory bus and open bus behavior.
-   Evaluates interrupts at well-defined timing points.
-   Produces 1 cycle per scheduler invocation.

### \### Timing-Sensitive Behavior

-   IRQ/NMI latency differs depending on exact cycle alignment.
-   BRK/IRQ/NMI arbitration rules must follow transistor-level behavior.
-   Read/Write timing for memory and mappers must match real 6502 bus
    cycles.

------------------------------------------------------------------------

## 4. PPU Integration

### \### Core Requirements

-   Emulate PPU **dot-by-dot**.
-   Track exact timing for:
    -   Sprite evaluation
    -   Pattern table fetches
    -   Scroll register latches
    -   VBlank start/end
    -   NMI triggering edge

### \### PPU Events

-   One event every **PPU cycle**.
-   Must integrate with CPU schedule at 3:1 ratio.
-   PPU must notify scheduler when:
    -   VBlank begins
    -   NMI line toggles
    -   Rendering reads from VRAM/CHR ROM

------------------------------------------------------------------------

## 5. APU Integration

### \### Components Requiring Cycle Accuracy

-   Frame counter (4-step or 5-step mode)
-   Envelope/sweep units
-   Length counters
-   DMC channel (most timing-sensitive part)

### \### DMC Cycle Interaction

DMC may: - Steal CPU cycles - Trigger IRQ - Perform timed memory fetches

Scheduler must plan for: - Next DMC fetch cycle - Next frame sequencer
step - Next IRQ time

------------------------------------------------------------------------

## 6. Memory Bus & Open Bus

### \### Key Considerations

-   Any read from unmapped memory must return **previous bus value**,
    not zero.
-   Each memory read/writelatches new open bus values.
-   PPU, CPU, and APU each have their own open bus domains.

Rust implementation benefits from: - Encapsulation of bus state into
structs that track last read value. - Immutable access via traits for
mapping.

------------------------------------------------------------------------

## 7. Interrupt Timing (NMI, IRQ, DMC)

### \### NMI

-   Triggered by PPU at a specific **PPU dot**.
-   CPU samples NMI line only at certain cycle phases.
-   There is a 1--3 cycle delay depending on current instruction.

### \### APU IRQ

-   Triggered by frame counter when in 4-step mode.
-   Delay between flag set and CPU recognition must be modeled.

### \### DMC IRQ

-   Occurs when sample buffer is drained.
-   Must fire exactly when real hardware would.

------------------------------------------------------------------------

## 8. High-Level Component Interaction

### \### Recommended Flow

    loop:
        next_event = scheduler.pop()

        match next_event.component:
            CPU: cpu.tick()
            PPU: ppu.tick()
            APU: apu.tick()
            Mapper: mapper.tick()

        new_events = component.emit_events()
        scheduler.push(new_events)

### \### Immutable Bus Perspective

CPU, PPU, APU perform no timing logic internally. They only: - Request
the scheduler to schedule the next tick - Emit side effects (writes,
IRQ/NMI changes)

Keeps components deterministic and isolated.

------------------------------------------------------------------------

## 9. Rust Implementation Patterns

### \### Traits for Behavior

``` rust
trait Tickable {
    fn tick(&mut self, scheduler: &mut Scheduler);
}
```

### \### Strong Typing for Clarity

-   `CpuCycle`, `PpuDot`, `ApuCycle` newtypes.
-   Separate structs for bus mirrors.
-   Enum for mapper IRQ sources.

### \### Memory Safety

-   PPU, CPU, and APU share no mutable references simultaneously.
-   Borrow checker prevents incorrect bus state mutations.

------------------------------------------------------------------------

## 10. Essential Accuracy Considerations

### Cycle-Level

-   CPU instruction timings
-   PPU scrolling and fetch windows
-   Sprite evaluation timing
-   APU envelope clocks
-   DMC fetch alignment

### Edge Cases

-   NMI/BRK/IRQ race conditions
-   Sprite overflow bugs
-   PPU odd-frame cycle skip
-   DMC DMA steal timing

### Hardware Quirks

-   PPU read buffer delay
-   Open bus behavior
-   Palette mirroring
-   OAM decay behavior

------------------------------------------------------------------------

## 11. Summary

A highly accurate NES emulator requires: - A cycle-driven
architecture. - A deterministic scheduler. - Accurate modeling of
CPU/PPU/APU timing down to individual cycles. - Precise bus, interrupt,
and rendering rules. - A separation of clock-driven hardware components
with controlled interaction.

Rust's safety and zero-cost abstractions make it ideal for building a
highly maintainable, robust, and accurate NES emulator core.
