# CPU2 Implementation TODO List

Based on review of [NESdev Wiki CPU documentation](https://www.nesdev.org/wiki/CPU) compared with current cpu2 implementation.

## Critical Missing Features

### 1. IRQ (Interrupt Request) Implementation
**Status:** Stub only - `trigger_irq()` returns 7 but does nothing  
**Wiki Reference:** [CPU interrupts](https://www.nesdev.org/wiki/CPU_interrupts)

**Missing:**
- [ ] IRQ edge/level detection logic
- [ ] IRQ polling during instruction execution
- [ ] Proper IRQ sequence (7 cycles):
  - Fetch opcode (forced to $00)
  - Read next byte (discarded)
  - Push PCH to stack
  - Push PCL to stack
  - Push P to stack (with B flag clear)
  - Fetch PCL from $FFFE, set I flag
  - Fetch PCH from $FFFF
- [ ] IRQ can be masked by I flag (unlike NMI)
- [ ] `should_poll_irq()` implementation

### 2. Interrupt Polling Behavior
**Status:** Not implemented  
**Wiki Reference:** [CPU interrupts - Detailed interrupt behavior](https://www.nesdev.org/wiki/CPU_interrupts#Detailed_interrupt_behavior)

**Missing:**
- [ ] NMI edge detection (high-to-low transition during φ2)
- [ ] IRQ level detection (low level during φ2)
- [ ] Interrupt polling happens during final cycle of most instructions
- [ ] NMI has priority over IRQ when both pending
- [ ] Interrupt sequences don't poll for interrupts (at least one instruction executes before next interrupt)

### 3. Delayed IRQ Response After CLI, SEI, PLP
**Status:** Not implemented  
**Wiki Reference:** [CPU interrupts - Delayed IRQ response](https://www.nesdev.org/wiki/CPU_interrupts#Delayed_IRQ_response_after_CLI,_SEI,_and_PLP)

**Missing:**
- [ ] RTI affects I flag immediately (IRQ can trigger right after RTI)
- [ ] CLI, SEI, PLP delay IRQ by one instruction
  - These instructions poll interrupts at end of first cycle (before changing I flag)
  - One more instruction executes before IRQ handler runs
- [ ] Need to track "delayed I flag change" state

### 4. Branch Instructions and Interrupts
**Status:** Not implemented  
**Wiki Reference:** [CPU interrupts - Branch instructions](https://www.nesdev.org/wiki/CPU_interrupts#Branch_instructions_and_interrupts)

**Missing:**
- [ ] Branch instructions have special interrupt polling:
  - Always polled before second cycle (operand fetch)
  - NOT polled before third cycle on taken branch
  - For page-crossing branches, polled before PCH fixup cycle
- [ ] Need to add interrupt polling points to branch addressing modes

### 5. Interrupt Hijacking
**Status:** Partially implemented (BRK checks nmi_pending)  
**Wiki Reference:** [CPU interrupts - Interrupt hijacking](https://www.nesdev.org/wiki/CPU_interrupts#Interrupt_hijacking)

**Current:** BRK checks `nmi_pending` during cycle 4 to determine vector  
**Missing:**
- [ ] IRQ can hijack BRK (same way NMI can)
- [ ] NMI can hijack IRQ
- [ ] Hijacking window is during cycles 1-4 of interrupt sequence
- [ ] At cycle 5 (***), signal status determines which vector is used
- [ ] IRQ hijacking detection (not just NMI)

## Power-up and Reset Behavior

### 6. Power-up State
**Status:** ✅ FIXED  
**Wiki Reference:** [CPU power up state](https://www.nesdev.org/wiki/CPU_power_up_state)

**Current Implementation:**
```rust
a: 0, x: 0, y: 0      // ✓ Correct at power-on
sp: 0x00              // ✓ Correct (before reset sequence)
pc: 0                 // ✓ Will be loaded from reset vector
p: FLAG_UNUSED        // ✓ Correct (only bit 5 set)
```

**Fixed:**
- [x] After reset, registers are now unchanged (A, X, Y preserved)
- [x] After reset, status flags C, Z, D, V, N are preserved
- [x] Power-up vs Reset distinction clarified in documentation
- [x] `new()` does power-on, `reset()` preserves registers per spec
- [x] Added comprehensive test `test_reset_preserves_registers()`

### 7. Reset Sequence
**Status:** Mostly correct but simplified  
**Wiki Reference:** [CPU interrupts - IRQ and NMI tick-by-tick](https://www.nesdev.org/wiki/CPU_interrupts#IRQ_and_NMI_tick-by-tick_execution)

**Current:** Sets I flag, subtracts 3 from SP, loads PC from reset vector, adds 7 cycles  
**Missing:**
- [ ] Reset goes through same 7-cycle sequence as interrupts but suppresses writes
- [ ] Should actually perform 3 dummy reads from stack (not just decrement SP)
- [ ] Reset is like NMI/IRQ but with writes disabled
- [ ] This is why I flag is always set on reset

## Status Flag Behavior

### 8. B Flag Handling
**Status:** Implemented in BRK, needs verification elsewhere  
**Wiki Reference:** [Status flags - The B flag](https://www.nesdev.org/wiki/Status_flags#The_B_flag)

**Requirements:**
- [x] B=1 when pushed by BRK and PHP
- [ ] B=0 when pushed by NMI (implemented in trigger_nmi)
- [ ] B=0 when pushed by IRQ (not implemented - IRQ stub)
- [ ] B flag doesn't physically exist, only appears when pushed to stack
- [ ] RTI/PLP ignore bits 4 and 5 when pulling from stack

### 9. Unused Flag (Bit 5)
**Status:** Partially implemented  
**Wiki Reference:** [Status flags](https://www.nesdev.org/wiki/Status_flags)

**Requirements:**
- [x] Always pushed as 1 to stack (implemented in BRK, trigger_nmi)
- [ ] Verify NMI implementation sets bit 5
- [ ] Verify IRQ implementation will set bit 5
- [ ] RTI/PLP ignore bit 5 when pulling

## Cycle Accuracy

### 10. Every Cycle is Read or Write
**Status:** Unknown - needs verification  
**Wiki Reference:** [CPU Notes](https://www.nesdev.org/wiki/CPU#Notes)

**Requirement:**
> "Every cycle on 6502 is either a read or write cycle."

**Needs verification:**
- [ ] All instruction cycles perform memory access
- [ ] No "idle" cycles that don't access memory
- [ ] Check all addressing modes (especially Implied)
- [ ] Check all instruction types

### 11. Dummy Reads/Writes
**Status:** Recently implemented for addressing modes  
**Wiki Reference:** Implied by cycle-accurate behavior requirements

**Recent work:**
- [x] Write operations skip unnecessary reads (fix-dummy-reads branch)
- [x] RMW operations perform dummy reads correctly
- [ ] Verify all dummy read/write behavior matches hardware
- [ ] Page-crossing dummy reads in indexed modes
- [ ] Verify write dummy reads are truly suppressed

## Documentation and Testing

### 12. Comprehensive Interrupt Tests
**Status:** Basic BRK test exists  
**Wiki Reference:** [CPU interrupts - Notes](https://www.nesdev.org/wiki/CPU_interrupts#Notes)

**Missing tests:**
- [ ] NMI triggering and execution
- [ ] IRQ triggering and execution (when implemented)
- [ ] NMI priority over IRQ
- [ ] Interrupt hijacking (NMI hijacks BRK)
- [ ] Interrupt hijacking (NMI hijacks IRQ)
- [ ] Interrupt hijacking (IRQ hijacks BRK)
- [ ] CLI/SEI/PLP delayed IRQ behavior
- [ ] Branch instruction interrupt polling
- [ ] Test ROM: cpu_interrupts_v2 (mentioned in wiki)

### 13. Reset Tests
**Status:** One basic test exists (test_opcode_00)  
**Missing:**
- [ ] Power-on state test
- [ ] Reset after power-on test
- [ ] Reset preserves A, X, Y, C, Z, D, V, N flags
- [ ] Reset sets I flag
- [ ] Reset decrements SP by 3
- [ ] Reset performs 3 dummy stack reads

### 14. Edge Case Tests
**Status:** Limited  
**Missing:**
- [ ] Multiple interrupts in succession
- [ ] Interrupt during BRK instruction
- [ ] RTI immediately followed by interrupt
- [ ] CLI immediately followed by interrupt
- [ ] Page boundary crossing during interrupt sequence

## Architecture/Design Issues

### 15. Interrupt State Management
**Status:** Minimal  
**Current:** Only `nmi_pending` boolean flag

**Needs:**
- [ ] IRQ pending state tracking
- [ ] Edge detector state for NMI
- [ ] Level detector state for IRQ
- [ ] Delayed I flag state (for CLI/SEI/PLP)
- [ ] Current interrupt sequence state
- [ ] Interrupt priority handling

### 16. Cycle-Level Interrupt Integration
**Status:** Not integrated  
**Issue:** Current cycle-accurate system doesn't check for interrupts

**Needs:**
- [ ] Interrupt polling at appropriate cycle boundaries
- [ ] Integration with instruction completion
- [ ] Integration with branch instructions
- [ ] Proper φ1/φ2 cycle modeling

### 17. Public API for Interrupts
**Status:** Basic methods exist but incomplete  
**Current API:**
- `trigger_nmi()` - implemented but not cycle-accurate polling
- `trigger_irq()` - stub only
- `set_nmi_pending()` - sets flag but doesn't trigger
- `is_nmi_pending()` - getter
- `should_poll_irq()` - stub returns false

**Missing:**
- [ ] Proper external IRQ/NMI signal interface
- [ ] Edge detection for NMI line
- [ ] Level detection for IRQ line
- [ ] Documentation of when/how to call these methods

## Lower Priority Items

### 18. Open Bus Behavior
**Status:** Unknown  
**Wiki Reference:** Mentioned in power-up state testing

**Needs investigation:**
- [ ] Verify open bus behavior during reset dummy reads
- [ ] Open bus during invalid memory access

### 19. Timing Precision
**Status:** Good for instruction execution  
**Wiki Reference:** [CPU Frequencies](https://www.nesdev.org/wiki/CPU#Frequencies)

**Current:** Tracks cycles, timing is correct  
**Future consideration:**
- [ ] NTSC vs PAL timing differences (if needed for accuracy)
- [ ] Master clock synchronization options

## Summary

**High Priority (Blocking):**
1. IRQ implementation (complete stub)
2. Interrupt polling behavior
3. Delayed IRQ response after CLI/SEI/PLP
4. Interrupt hijacking (complete implementation)

**Medium Priority (Important for accuracy):**
5. Branch instruction interrupt polling
6. Reset sequence cycle-accurate dummy reads
7. B flag verification in all interrupt paths
8. Comprehensive interrupt tests

**Low Priority (Polish):**
9. Power-up vs Reset API clarity
10. Open bus behavior
11. Documentation improvements

## Test ROMs to Validate

From NESdev Wiki recommendations:
- [ ] cpu_interrupts_v2 - Tests interrupt behavior
- [ ] branch_timing_tests - Tests branch and interrupt interaction
- [ ] cpu_reset - Tests reset behavior
- [ ] instr_timing - General timing verification

## Notes

- The cpu2 implementation is generally well-structured and cycle-accurate for normal instruction execution
- The main gap is comprehensive interrupt handling
- Recent work on dummy reads/writes (fix-dummy-reads branch) shows good attention to cycle accuracy
- The addressing mode refactoring (MemoryAccess enum) improved code quality significantly
- Flag constant usage is now consistent after recent refactoring
