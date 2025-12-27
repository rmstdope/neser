# CPU2 Implementation TODO List

Based on review of [NESdev Wiki CPU documentation](https://www.nesdev.org/wiki/CPU) compared with current cpu2 implementation.

## Critical Missing Features

### 1. IRQ (Interrupt Request) Implementation

**Status:** ✅ FIXED  
**Wiki Reference:** [CPU interrupts](https://www.nesdev.org/wiki/CPU_interrupts)

**Implemented:**

- [x] IRQ trigger sequence (7 cycles)
- [x] IRQ polling logic respecting I flag
- [x] Push PC and P to stack (with B flag clear, unused flag set)
- [x] Load PC from IRQ vector ($FFFE-$FFFF)
- [x] Set I flag to prevent nested IRQs
- [x] `should_poll_irq()` implementation
- [x] `set_irq_pending()` and `is_irq_pending()` helper methods
- [x] Comprehensive test coverage (5 tests)

**Tests Added:**

- `test_irq_trigger_basic` - Basic IRQ functionality
- `test_irq_respects_i_flag` - I flag masking behavior
- `test_irq_clears_b_flag` - B flag handling in pushed status
- `test_irq_set_and_check` - Helper methods
- `test_irq_stack_wrapping` - Stack pointer wrapping

### 2. Interrupt Polling Behavior

**Status:** ✅ FIXED (Partial - Core polling logic implemented)
**Wiki Reference:** [CPU interrupts - Detailed interrupt behavior](https://www.nesdev.org/wiki/CPU_interrupts#Detailed_interrupt_behavior)

**Implemented:**

- [x] Interrupt polling infrastructure with `poll_pending_interrupt()` method
- [x] NMI has priority over IRQ when both pending
- [x] Interrupt sequences don't poll for interrupts (at least one instruction executes before next interrupt)
- [x] `in_interrupt_sequence` flag tracks interrupt handler execution
- [x] Automatic clearing of flag when instruction completes

**Tests Added:**

- `test_interrupt_polling_nmi_priority` - Verifies NMI priority over IRQ
- `test_interrupt_not_polled_during_interrupt_sequence` - Verifies no polling during interrupt handler

**Note:** Edge/level detection is handled externally by NES hardware (PPU/APU).
The CPU provides the polling mechanism via `poll_pending_interrupt()` which external code should call after each instruction.

### 3. Delayed IRQ Response After CLI, SEI, PLP

**Status:** ✅ FIXED
**Wiki Reference:** [CPU interrupts - Delayed IRQ response](https://www.nesdev.org/wiki/CPU_interrupts#Delayed_IRQ_response_after_CLI,_SEI,_and_PLP)

**Implemented:**

- [x] CLI, SEI, PLP delay IRQ by one instruction
  - These instructions poll interrupts at end of first cycle (before changing I flag)
  - One more instruction executes before IRQ handler runs
- [x] Track "delayed I flag change" state via `delay_interrupt_check` field in Cpu2 and CpuState
- [x] `should_poll_irq()` respects delay flag
- [x] Delay mechanism activated when instruction completes, cleared after one instruction
- [x] RTI affects I flag immediately (IRQ can trigger right after RTI)
  - RTI correctly does NOT set delay flag (unlike PLP)
  - IRQ can fire immediately after RTI if I flag is cleared

**Tests Passing:**

- Test #3: "APU should generate IRQ when $4017 = $00" ✅
- Test #4: "Exactly one instruction after CLI should execute before IRQ" ✅

**Known Issue:**

- Test #5: "CLI SEI should allow only one IRQ just after SEI" ❌
  - Edge case involving CLI followed immediately by SEI
  - Requires deeper investigation of hardware timing

**Missing:**

- [ ] RTI affects I flag immediately (IRQ can trigger right after RTI)
  - RTI should NOT set delay flag (unlike PLP)
  - IRQ can fire immediately after RTI if I flag is cleared

### 4. Branch Instructions and Interrupts

**Status:** ⚠️ DEFERRED - Requires architectural changes  
**Wiki Reference:** [CPU interrupts - Branch instructions](https://www.nesdev.org/wiki/CPU_interrupts#Branch_instructions_and_interrupts)  
**Test ROM:** `roms/blargg/cpu_interrupts_v2/rom_singles/5-branch_delays_irq.nes` (commented out)

**Requirements:**

- [ ] Branch instructions have special interrupt polling:
  - Always polled before second cycle (operand fetch)
  - NOT polled before third cycle on taken branch
  - For page-crossing branches, polled before PCH fixup cycle

**Investigation Summary:**
Two approaches were attempted:

1. **Skip mechanism**: Skip IRQ polling after taken non-page-crossing branches
   - Test still timed out
2. **Delay mechanism**: Use existing `delay_interrupt_check` to delay IRQ by one instruction
   - Broke other interrupt tests (CLI latency)

**Root Cause:**
Current architecture polls interrupts AFTER instruction completion in the NES loop.
Proper implementation requires cycle-level interrupt polling WITHIN instruction execution
("before second cycle", "before third cycle"). This would need significant architectural
changes to support mid-instruction polling points.

**Recommendation:** Defer until other CPU2 features are complete, then revisit with
proper mid-instruction polling architecture.

### 5. Interrupt Hijacking

**Status:** Partially implemented (BRK checks nmi_pending)  
**Wiki Reference:** [CPU interrupts - Interrupt hijacking](https://www.nesdev.org/wiki/CPU_interrupts#Interrupt_hijacking)

**Current:** BRK checks `nmi_pending` during cycle 4 to determine vector  
**Missing:**

- [ ] IRQ can hijack BRK (same way NMI can)
- [ ] NMI can hijack IRQ
- [ ] Hijacking window is during cycles 1-4 of interrupt sequence
- [ ] At cycle 5 (\*\*\*), signal status determines which vector is used
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

**Status:** ✅ FIXED  
**Wiki Reference:** [CPU interrupts - IRQ and NMI tick-by-tick](https://www.nesdev.org/wiki/CPU_interrupts#IRQ_and_NMI_tick-by-tick_execution)

**Previous Implementation:** Just decremented SP by 3  
**Fixed Implementation:**

- [x] Reset performs 3 dummy stack reads (cycles 3-5)
- [x] Each read accesses $0100+SP and decrements SP
- [x] Reads PCL from $FFFC on cycle 6, sets I flag
- [x] Reads PCH from $FFFD on cycle 7
- [x] Stack memory is read but NOT written (writes suppressed)
- [x] Matches hardware behavior: reset is like NMI/IRQ but suppresses writes

**Cycle-by-cycle breakdown:**

1. Fetch opcode (forced to $00, discarded) - not simulated
2. Read next byte (discarded) - not simulated
3. Dummy read from stack at $0100+SP, decrement SP
4. Dummy read from stack at $0100+SP, decrement SP
5. Dummy read from stack at $0100+SP, decrement SP
6. Read PCL from $FFFC, set I flag
7. Read PCH from $FFFD

**Test Added:** `test_reset_performs_dummy_stack_reads()` validates that stack reads occur but memory is not modified.

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

### 13. Comprehensive Reset Tests

**Status:** ✅ FIXED  
**Wiki Reference:** [CPU power up state](https://www.nesdev.org/wiki/CPU_power_up_state)

**Tests Added:**

- [x] `test_power_on_state()` - Validates initial state after power-on
- [x] `test_reset_after_power_on()` - Tests reset immediately after power-on
- [x] `test_reset_preserves_registers()` - Verifies A, X, Y and flags preserved
- [x] `test_reset_with_sp_wrapping()` - Tests SP wrapping edge cases (0x00, 0x01, 0x02)
- [x] `test_multiple_resets()` - Tests multiple consecutive resets
- [x] All reset tests verify I flag is set
- [x] All reset tests verify SP is decremented by 3
- [x] All reset tests verify PC loaded from reset vector

**Note:** Reset performing 3 dummy stack reads (cycle-accurate implementation) is tracked separately in issue #7.

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

**Medium Priority (Important for accuracy):** 5. Branch instruction interrupt polling 6. Reset sequence cycle-accurate dummy reads 7. B flag verification in all interrupt paths 8. Comprehensive interrupt tests

**Low Priority (Polish):** 9. Power-up vs Reset API clarity 10. Open bus behavior 11. Documentation improvements

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
