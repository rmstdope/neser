# NES Emulator Refactoring Plan for Cycle-Accurate Execution

## Current Architecture Analysis

### Problems Identified

1. **Instruction-Level Granularity**: The current architecture executes complete CPU instructions before ticking PPU/APU, making it impossible to detect PPU state changes (like NMI) mid-instruction.

2. **Tight Coupling**: CPU owns `Rc<RefCell<MemController>>`, which contains PPU and APU references, creating circular dependency issues.

3. ~~**No Event Scheduler**: Components tick in a fixed order (CPU → PPU → APU) rather than based on actual cycle timing.~~ (DEFERRED)

4. **Coarse Interrupt Handling**: NMI/IRQ checked only between instructions, not between individual memory operations.

5. ~~**Fractional Cycle Accumulation**: PAL timing handled via floating-point accumulation rather than proper cycle accounting.~~ (DEFERRED)

## Proposed Architecture: Simplified Cycle-Accurate Model

### Core Concept

Keep the existing main loop structure but make the CPU execute **cycle-by-cycle** instead of instruction-by-instruction. After each CPU cycle, tick the PPU and check for interrupt edges. This provides cycle accuracy without requiring a full event scheduler refactor.

---

## Simplified Refactoring Approach

### Phase 1: CPU Cycle-by-Cycle Execution

The key change is making the CPU execute one cycle at a time instead of one instruction at a time.

### 1.1 Add Instruction State to CPU

```rust
// src/cpu/cpu.rs

pub struct Cpu {
    // Existing registers (unchanged)
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub sp: u8,
    pub pc: u16,
    pub p: u8,
    pub memory: Rc<RefCell<MemController>>,  // Keep existing for now
    pub halted: bool,
    pub total_cycles: u64,
    pub delayed_i_flag: Option<bool>,
    pub nmi_pending: bool,

    // NEW: Instruction execution state
    current_instruction: Option<InstructionState>,
    cycle_in_instruction: u8,
}

#[derive(Debug)]
struct InstructionState {
    opcode: Opcode,
    cycles_remaining: u8,
    // Store any intermediate values needed across cycles
    temp_addr: Option<u16>,
    temp_value: Option<u8>,
}

impl Cpu {
    /// Execute one CPU cycle (replaces run_opcode)
    pub fn tick_cycle(&mut self) -> bool {
        if self.halted {
            return false;
        }

        // Start new instruction if needed
        if self.current_instruction.is_none() {
            let opcode_byte = self.memory.borrow().read(self.pc);
            self.pc += 1;
            let opcode = lookup(opcode_byte).unwrap();

            self.current_instruction = Some(InstructionState {
                opcode,
                cycles_remaining: opcode.cycles,
                temp_addr: None,
                temp_value: None,
            });
            self.cycle_in_instruction = 0;
        }

        // Execute one cycle of current instruction
        if let Some(ref mut state) = self.current_instruction {
            self.execute_instruction_cycle(state);
            self.cycle_in_instruction += 1;
            state.cycles_remaining -= 1;

            // Instruction complete?
            if state.cycles_remaining == 0 {
                self.current_instruction = None;
                self.cycle_in_instruction = 0;
                return true; // Instruction finished
            }
        }

        false // Instruction still in progress
    }

    fn execute_instruction_cycle(&mut self, state: &mut InstructionState) {
        // Execute one cycle based on opcode and cycle_in_instruction
        match state.opcode.mnemonic {
            BRK => self.execute_brk_cycle(state),
            LDA_IMM => self.execute_lda_imm_cycle(state),
            // ... other instructions
            _ => {
                // For now, can execute whole instruction on first cycle
                if self.cycle_in_instruction == 0 {
                    self.execute_instruction_immediately(&state.opcode);
                }
            }
        }
    }

    fn execute_brk_cycle(&mut self, state: &mut InstructionState) {
        match self.cycle_in_instruction {
            0 => { /* Cycle 1: Dummy read */ }
            1 => { /* Cycle 2: Dummy read, increment PC */ }
            2 => { /* Cycle 3: Push PCH */ }
            3 => { /* Cycle 4: Push PCL */ }
            4 => {
                // Cycle 5: CRITICAL - Check NMI pending before push
                let use_nmi = self.nmi_pending;
                if use_nmi {
                    self.nmi_pending = false;
                }
                state.temp_addr = Some(if use_nmi { NMI_VECTOR } else { IRQ_VECTOR });
                // Push status with B flag
                self.push_byte(self.p | FLAG_BREAK | FLAG_UNUSED);
                self.p |= FLAG_INTERRUPT;
            }
            5 => { /* Cycle 6: Read vector low */ }
            6 => { /* Cycle 7: Read vector high and jump */ }
            _ => unreachable!()
        }
    }
}
```

### 1.2 Update NES Main Loop

Keep the existing structure but call `tick_cycle()` instead of `run_opcode()`:

```rust
// src/nes.rs

impl Nes {
    pub fn run_cpu_tick(&mut self) -> u8 {
        // Handle DMA (unchanged)
        // ...

        // Execute one CPU cycle
        let instruction_complete = self.cpu.tick_cycle();
        self.cpu.total_cycles += 1;

        // Tick PPU for 3 cycles (NTSC) after each CPU cycle
        self.tick_ppu(1);
        self.tick_apu(1);

        // Check for NMI edge after every CPU cycle
        if self.ppu.borrow_mut().poll_nmi() {
            self.cpu.set_nmi_pending(true);
        }

        // Only check for IRQ/NMI triggering after instruction completes
        if instruction_complete {
            if self.cpu.nmi_pending {
                let nmi_cycles = self.cpu.trigger_nmi();
                self.tick_ppu(nmi_cycles);
                self.tick_apu(nmi_cycles);
            }

            if self.cpu.should_poll_irq() && self.apu.borrow().poll_irq() {
                let irq_cycles = self.cpu.trigger_irq();
                if irq_cycles > 0 {
                    self.tick_ppu(irq_cycles);
                    self.tick_apu(irq_cycles);
                }
            }
        }

        if self.ppu.borrow_mut().poll_frame_complete() {
            self.ready_to_render = true;
        }

        1 // Always 1 CPU cycle per tick
    }
}
```

---

## Phase 2: Gradual Instruction Migration

Start by implementing cycle-by-cycle execution for timing-critical instructions:

### Priority 1: BRK (for NMI hijacking)

Already shown above - this is the highest priority

### Priority 2: Branch Instructions (for branch timing tests)

```rust
fn execute_bcc_cycle(&mut self, state: &mut InstructionState) {
    match self.cycle_in_instruction {
        0 => {
            // Read offset
            let offset = self.read_byte() as i8;
            if (self.p & FLAG_CARRY) == 0 {
                state.temp_value = Some(offset as u8);
                state.cycles_remaining += 1; // Branch taken
            }
        }
        1 => {
            // Branch taken - add offset, check page crossing
            let offset = state.temp_value.unwrap() as i8;
            let old_pc = self.pc;
            self.pc = self.pc.wrapping_add(offset as u16);
            if page_crossed(old_pc, self.pc) {
                state.cycles_remaining += 1;
            }
        }
        2 => {
            // Page crossing penalty cycle
        }
        _ => unreachable!()
    }
}
```

### Priority 3: Other Instructions

For instructions that don't need cycle-accurate execution yet, execute them immediately on cycle 0 and burn remaining cycles:

```rust
fn execute_instruction_immediately(&mut self, opcode: &Opcode) {
    // Use existing implementation
    match opcode.mnemonic {
        LDA_IMM => {
            let value = self.read_byte();
            self.lda(value);
        }
        // ... existing logic
    }
}
```

---

## Phase 3: Testing and Validation

### 3.1 Unit Tests

- Update existing CPU tests to work with `tick_cycle()`
- Test that instructions complete in correct number of cycles
- Test BRK with NMI pending at different cycles

### 3.2 Integration Tests

- Run cpu_interrupts test 2 - should now pass!
- Ensure all other blargg tests still pass
- Check that existing games still work

---

## Migration Strategy (Simplified)

### Week 1: Infrastructure

- Add `InstructionState` to CPU
- Implement `tick_cycle()` method
- Keep `run_opcode()` calling `tick_cycle()` in a loop for compatibility

### Week 2: NES Loop Update

- Change `run_cpu_tick()` to call `tick_cycle()` once
- Update PPU/APU ticking to happen after each CPU cycle
- Add NMI edge detection after each cycle

### Week 3: BRK Implementation

- Implement cycle-accurate BRK
- Test with cpu_interrupts test 2
- Fix any timing issues

### Week 4: Branch Instructions

- Implement cycle-accurate branches
- Test with branch timing ROMs
- Ensure page crossing penalties work

### Week 5: Testing and Cleanup

- Run full test suite
- Profile performance
- Optimize if needed
- Remove old code

---

## Expected Benefits

### Accuracy Improvements

✅ **Cycle-perfect NMI/BRK interaction** - Can pass cpu_interrupts test 2
✅ **Better branch timing** - Pass branch timing tests
✅ **Interrupt timing** - NMI can be detected mid-instruction

### Code Quality

✅ **Simpler than full event scheduler** - Keep existing structure
✅ **Easier testing** - Each instruction cycle can be tested independently
✅ **Clearer execution flow** - Explicit cycle-by-cycle state machine
✅ **Gradual migration** - Can migrate instructions one at a time

### Flexibility

✅ **Foundation for future work** - Can add event scheduler later if needed
✅ **Easy to debug** - Can step through individual cycles
✅ **Maintains compatibility** - Existing code mostly unchanged

---

## Risks and Mitigations

### Risk 1: Performance Degradation

**Impact**: More function calls per instruction could slow things down

**Mitigation**:

- Profile before and after
- Most instructions can still execute in one cycle (fast path)
- Only timing-critical instructions need full cycle-by-cycle execution
- Inline small functions

### Risk 2: Migration Complexity

**Impact**: Need to update ~56 instruction implementations

**Mitigation**:

- Start with high-priority instructions (BRK, branches)
- Keep existing implementation for low-priority instructions
- Gradual migration over several weeks
- Comprehensive test coverage

### Risk 3: Testing Burden

**Impact**: Need to test each instruction's cycle behavior

**Mitigation**:

- Focus on timing-critical instructions first
- Use existing test ROMs for validation
- Add cycle-counting assertions to unit tests
- Run full test suite after each change

---

## Success Criteria

1. ✅ Pass cpu_interrupts test 2 (NMI/BRK timing)
2. ✅ Pass all existing blargg tests (no regressions)
3. ✅ No performance regression > 20%
4. ✅ BRK executes in exactly 7 cycles with correct NMI hijacking behavior
5. ✅ Branch instructions have correct timing with page crossing penalties

---

## Timeline Estimate (Simplified)

- **Week 1** (Infrastructure): Add instruction state, implement tick_cycle()
- **Week 2** (Main Loop): Update NES loop to call tick_cycle(), add per-cycle NMI checking
- **Week 3** (BRK): Implement cycle-accurate BRK, test with cpu_interrupts
- **Week 4** (Branches): Implement cycle-accurate branches, test with branch timing ROMs
- **Week 5** (Testing & Polish): Full test suite, performance profiling, cleanup

**Total**: ~5 weeks

---

## Future Enhancements (Post-Refactor)

Once this simplified refactor is complete, we can consider:

1. **Event Scheduler** - Add full event-driven architecture for even better accuracy
2. **PAL Timing** - Fix fractional cycle accumulation with proper rational number handling
3. **PPU Dot-by-Dot** - Make PPU render dot-by-dot for mid-scanline register changes
4. **Component Decoupling** - Remove Rc<RefCell<>> chains with proper ownership model
5. **DMC Cycle Stealing** - Exact CPU cycle stealing by DMC DMA

These can be tackled incrementally without disrupting the working emulator.
