---
name: gba-cpu-development
description: ARM7TDMI instruction execution, cycle counting, mode switching, and interrupt dispatch for GBA emulation.
---

# GBA CPU Development: ARM7TDMI

## Introduction

Use this skill when implementing or debugging the ARM7TDMI CPU core for GBA emulation. This covers instruction decoding/execution (both ARM 32-bit and Thumb 16-bit), register model, CPU modes, interrupt dispatch, cycle counting (S/N model), and conditional execution. Focus on accuracy, clarity of implementation, and thorough testing against known traces.

## Key Concepts

### 1. Instruction Sets: ARM and Thumb

**ARM (32-bit instruction set):**
- Full access to R0-R15 (16 general-purpose registers)
- Rich conditional execution (16 conditions based on CPSR flags)
- Full instruction set with data processing, load/store, branch, control

**Thumb (16-bit instruction set):**
- Subset of ARM instructions encoded in 16 bits
- Limited register access: primarily R0-R7, with some R8-R15 limited operations
- Higher code density (smaller binary size)
- Mode switching via Branch-and-Exchange (BX) instruction

### 2. Register Model

**General-purpose registers (R0-R15):**
- R0-R7: Unrestricted in both ARM and Thumb
- R8-R12: Full in ARM, limited in Thumb (specific instructions only)
- R13 (SP): Stack pointer, mode-specific in interrupt modes
- R14 (LR): Link register, used for function returns
- R15 (PC): Program counter, auto-increments during execution

**Current Program Status Register (CPSR):**
- Bits [31:28]: Condition flags (N, Z, C, V)
- Bits [7:0]: Control bits (I, F, T, M[4:0])
- I = IRQ disable, F = FIQ disable
- T = Thumb mode flag (0=ARM, 1=Thumb)
- M[4:0] = CPU mode (00000=USR, 10000=SVC, 10001=ABT, etc.)

### 3. CPU Modes

NESER should implement at least these modes:

- **USR (User mode, 10000):** Normal program execution
- **SVC (Supervisor mode, 10011):** After SWI (software interrupt)
- **ABT (Abort mode, 10111):** Memory access violations
- **IRQ (Interrupt mode, 10010):** External interrupt (IRQ signal)
- **FIQ (Fast Interrupt mode, 10001):** Interrupt with dedicated registers

Mode switching occurs on:
- Interrupt dispatch (CPU switches to IRQ or FIQ mode)
- SWI (software interrupt) instruction (switches to SVC)
- Memory abort (switches to ABT)
- BX instruction with bit 0 set in target address (can change modes)

**Banked registers:** Some modes have dedicated SP/LR registers (e.g., IRQ has R13_irq, R14_irq separate from USR R13, R14).

### 4. Interrupt Handling

**Interrupt sources (from memory-mapped I/O):**
- V-blank, H-blank, V-counter match (PPU)
- Timer 0-3 overflow
- DMA 0-3 completion
- Serial I/O completion
- Keypad (key interrupt)
- Cartridge interrupt

**Interrupt dispatch sequence:**
1. CPU finishes current instruction
2. Check IE & IF registers (enabled and pending interrupts)
3. Determine priority: FIQ (higher) or IRQ (lower)
4. Save PC to R14_irq or R14_fiq (link register for mode)
5. Save CPSR to SPSR_irq or SPSR_fiq (saved PSR for mode)
6. Set CPSR.I (disable interrupts) and switch to IRQ/FIQ mode
7. Jump to interrupt vector (0x18 for IRQ, 0x1C for FIQ in GBA)
8. ISR executes in IRQ/FIQ mode
9. MOVS PC, LR (return from interrupt) restores PC and CPSR from saved registers

**Critical timing note:** ARM7TDMI prefetches next instruction while executing. Interrupt dispatch affects prefetch pipeline.

### 5. Cycle Counting: S/N Model

GBA CPU cycles are counted in **S (sequential) and N (non-sequential)** units:

**S-cycle:** Sequential memory access (fetch next word in sequence from cache/fast path)
**N-cycle:** Non-sequential access (jump to new address, cache miss, wait state)

**Base instruction costs (approximate):**
- Data processing (ALU): 1S
- Load/Store: nS+1N (n = number of words, +1N for address change)
- Branch: 1S+1N (fetch new address + prefetch)
- Multiply: nS (n = 1-4 depending on operand size)
- Prefetch stalls: Pipeline refills with N-cycle

**Implementation strategy:**
- Track S/N cycles explicitly in CPU step
- Accumulate cycles for frontend timing
- Return total cycles from `run_tick()` as `u8` (should not exceed 4 cycles typically)

### 6. Conditional Execution

**ARM supports 16 condition codes:**
- EQ (Z=1), NE (Z=0), CS (C=1), CC (C=0)
- MI (N=1), PL (N=0), VS (V=1), VC (V=0)
- HI (C=1 && Z=0), LS (C=0 || Z=1)
- GE (N==V), LT (N!=V), GT (N==V && Z=0), LE (N!=V || Z=1)
- AL (always), NV (never)

**Thumb has limited condition codes:**
- First 4 bits of many Thumb instructions specify condition

**Implementation:**
- Before executing instruction, check condition bits against CPSR
- If condition fails, skip instruction (but still advance PC for prefetch accuracy)
- Some instructions (like branches) always update flags; others update only on explicit S-bit

## Implementation Patterns for NESER

### 1. Register Model

```rust
pub struct Registers {
    r: [u32; 16],           // R0-R15
    cpsr: u32,
    spsr_array: [u32; 5],   // One SPSR per mode (except USR)
}

impl Registers {
    pub fn get_r(&self, index: u8) -> u32 { /* handle PC special case */ }
    pub fn set_r(&mut self, index: u8, value: u32) { /* handle PC special case */ }
    pub fn mode(&self) -> CpuMode { /* extract M[4:0] from CPSR */ }
    pub fn set_mode(&mut self, mode: CpuMode) { /* update CPSR M bits */ }
    pub fn condition_met(&self, condition: u8) -> bool { /* check CPSR flags */ }
}
```

### 2. Instruction Decoding Pattern

Separate ARM and Thumb decoders:

```rust
pub enum Instruction {
    // ARM variant
    DataProcessing { opcode: u8, rn: u8, rd: u8, operand: Operand },
    LoadStore { ... },
    Branch { ... },
    // Thumb variants
    ThumbDataProcessing { ... },
    // ... other variants
}

fn decode_arm(word: u32) -> Instruction { /* match top 4 bits, then sub-patterns */ }
fn decode_thumb(word: u16) -> Instruction { /* match top 5 bits, then sub-patterns */ }
```

### 3. Execution Pattern

```rust
pub fn execute_instruction(&mut self, instr: Instruction) -> u32 {
    // Check condition
    if !instr.condition_met(self.cpsr) {
        return self.prefetch_cycles();  // Skip instruction, return prefetch cycles
    }

    // Execute based on instruction type
    match instr {
        Instruction::DataProcessing { ... } => { /* ALU operation, 1S */ },
        Instruction::LoadStore { ... } => { /* memory access, nS+1N */ },
        // ... other cases
    }

    // Update cycle counter (return S/N accumulated)
    self.cycles += instr_cycles;
    instr_cycles as u32
}
```

### 4. Cycle Accounting

```rust
pub fn run_tick(&mut self) -> u8 {
    // Fetch instruction (prefetch from pipeline)
    let instr = self.prefetch_buffer;
    self.prefetch_buffer = self.fetch_next_instr();  // S or N cycle

    // Execute instruction
    let exec_cycles = self.execute_instruction(instr);

    // Total = prefetch + exec
    exec_cycles as u8
}
```

### 5. Mode Switching & Interrupt Dispatch

```rust
pub fn handle_interrupt(&mut self, interrupt: InterruptType) {
    // Determine target mode (IRQ or FIQ)
    let target_mode = match interrupt {
        InterruptType::Irq => CpuMode::Irq,
        InterruptType::Fiq => CpuMode::Fiq,
    };

    // Save current PC and CPSR
    let lr = self.pc;  // Will be adjusted by 4 or 8 depending on prefetch state
    let spsr = self.cpsr;

    // Store in banked registers
    self.spsr_array[target_mode as usize] = spsr;
    self.r_banked[target_mode as usize][14] = lr;  // R14 (LR)

    // Switch mode, disable interrupts
    self.cpsr = (self.cpsr & 0xFFFFFF00) | (target_mode as u32);
    self.cpsr |= 0x80;  // Set I bit (disable IRQ)

    // Jump to interrupt vector
    self.pc = match interrupt {
        InterruptType::Irq => 0x18,
        InterruptType::Fiq => 0x1C,
    };
    self.prefetch_buffer = self.fetch_next_instr();
}
```

## Testing Strategy

### 1. Unit Tests for Each Instruction

```rust
#[test]
fn test_arm_add() {
    let mut cpu = Cpu::new();
    cpu.r[0] = 10;
    cpu.r[1] = 20;
    cpu.execute_add(rd=2, rn=0, operand=Operand::Reg(1));
    assert_eq!(cpu.r[2], 30);
}
```

### 2. Full Integration Tests Against Known ROMs

- Use small test ROMs (e.g., custom validation ROMs)
- Compare CPU register state after ROM execution
- Verify cycle counts match expected traces

### 3. Cycle Accuracy Tests

- Compare S/N cycle output against known emulator traces (mGBA, DeSmuME)
- Verify instruction prefetch timing matches hardware
- For timer-driven diagnostics, check both the target diagnostic and any
  broader timing suites that use timers as measurement instruments.
- When adding delayed events or global phase counters, include save-state
  roundtrip coverage for every new timing field that can affect future events.
- Keep timer register reads side-effect free unless there is source-backed
  evidence that the read itself advances hardware-visible state; instruction
  cycle advancement should happen at instruction boundaries.
- If generated BIOS binaries or suite CRC approvals change as part of timing
  work, rebuild/update them in the same increment and validate the affected
  diagnostics before merging.

### 4. Mode Switching Tests

- Test interrupt dispatch and mode switching
- Verify banked register handling (SP_irq, LR_irq, etc.)
- Test return from interrupt with MOVS PC, LR

## References & Tools

- **ARM Architecture Reference Manual:** Instruction set specification (free from ARM)
- **GBATek CPU Section:** GBA-specific ARM7TDMI details
- **TONC CPU Chapter:** Cycle counting and timing
- **mGBA Source:** `src/arm/core.c` for implementation reference
- **NO$GBA Trace Tools:** Cycle-accurate instruction traces (if available)
- **Test ROMs:** Custom validation ROMs to verify instruction behavior

## Common Pitfalls

1. **PC prefetch handling:** PC is always 2 words ahead (8 bytes). Update carefully.
2. **Condition code evaluation:** Some instructions always execute; others are conditional. Don't mix them up.
3. **Mode-specific registers:** Banked registers (SP, LR) differ by mode. Use lookup tables to avoid confusion.
4. **Cycle counting:** S/N model is different from simple cycle counts. Verify against traces.
5. **Thumb vs ARM addressing:** Branch targets in Thumb must have bit 0 set for mode switch; in ARM, use BX.

## Integration with NESER

- CPU module exports `Arm7tdmi` struct implementing `execute()` method
- CPU integrates with `GbaConsole` orchestration
- Cycle counts returned to frontend for timing synchronization
- Interrupts dispatched when I/O layer raises signals (V-blank, timer overflow, DMA complete)
