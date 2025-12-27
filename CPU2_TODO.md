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

**Status:** ✅ FIXED  
**Wiki Reference:** [Status flags - The B flag](https://www.nesdev.org/wiki/Status_flags#The_B_flag)

**Requirements:**

- [x] B=1 when pushed by BRK and PHP
- [x] B=0 when pushed by NMI (verified with test)
- [x] B=0 when pushed by IRQ (verified with test)
- [x] B flag doesn't physically exist, only appears when pushed to stack
- [x] RTI/PLP ignore bits 4 and 5 when pulling from stack (verified with tests)

**Tests Added:**

- `test_nmi_clears_b_flag` - Verifies NMI pushes status with B=0 and unused=1
- `test_irq_clears_b_flag` - Verifies IRQ pushes status with B=0 and unused=1 (already existed)
- `test_rti_ignores_break_and_unused_bits` - Verifies RTI sets B=0 and unused=1 regardless of stack value
- `test_plp_ignores_break_and_unused_bits` - Verifies PLP sets B=0 and unused=1 regardless of stack value

**Implementation Details:**

- `trigger_nmi()` clears B flag: `p_with_break = self.state.p & !FLAG_BREAK`
- `trigger_irq()` clears B flag: `p_with_flags = self.state.p & !FLAG_BREAK`
- `Rti::tick()` ignores bits 4-5: `cpu_state.p = (self.p & !0x10) | 0x20`
- `Plp::tick()` ignores bits 4-5: `cpu_state.p = (status & 0xCF) | 0x20`

### 9. Unused Flag (Bit 5)

**Status:** ✅ FIXED  
**Wiki Reference:** [Status flags](https://www.nesdev.org/wiki/Status_flags)

**Requirements:**

- [x] Always pushed as 1 to stack (implemented in BRK, trigger_nmi, trigger_irq)
- [x] Verify NMI implementation sets bit 5 (verified by test_nmi_clears_b_flag)
- [x] Verify IRQ implementation sets bit 5 (verified by test_irq_clears_b_flag)
- [x] RTI/PLP ignore bit 5 when pulling (verified by test_rti_ignores_break_and_unused_bits and test_plp_ignores_break_and_unused_bits)

**Tests Verifying Unused Flag:**

- `test_nmi_clears_b_flag` - Verifies NMI pushes status with unused=1
- `test_irq_clears_b_flag` - Verifies IRQ pushes status with unused=1
- `test_rti_ignores_break_and_unused_bits` - Verifies RTI always sets unused=1 regardless of stack value
- `test_plp_ignores_break_and_unused_bits` - Verifies PLP always sets unused=1 regardless of stack value

**Implementation Details:**

- `trigger_nmi()` sets unused flag: `p_with_break |= FLAG_UNUSED`
- `trigger_irq()` sets unused flag: `p_with_flags |= FLAG_UNUSED`
- `Rti::tick()` masks and sets bit 5: `cpu_state.p = (self.p & !0x10) | 0x20`
- `Plp::tick()` masks and sets bit 5: `cpu_state.p = (status & 0xCF) | 0x20`

**Note:** All unused flag behavior was comprehensively verified by the B flag tests added in issue #8.

## Cycle Accuracy

### 10. Every Cycle is Read or Write

**Status:** ✅ FIXED  
**Wiki Reference:** [CPU Notes](https://www.nesdev.org/wiki/CPU#Notes)

**Requirement:**

> "Every cycle on 6502 is either a read or write cycle."

**Verification:**

- [x] All instruction cycles perform memory access
- [x] No "idle" cycles that don't access memory
- [x] Implied addressing mode performs dummy reads
- [x] All instruction types verified

**Implementation:**

- Modified `Implied` addressing mode to perform dummy read of next byte at PC
- This satisfies the 6502 hardware requirement that every cycle generates a bus cycle
- The read value is discarded (dummy read), but ensures proper bus behavior

**Tests Added:**

- `test_nop_performs_dummy_read` - Verifies NOP performs memory read during execution
- `test_clc_performs_dummy_read` - Verifies CLC performs memory read during execution  
- `test_tax_performs_dummy_read` - Verifies TAX performs memory read during execution

**Details:**

Implied mode instructions (like NOP, CLC, SEC, TAX, etc.) are 2-cycle instructions:
- Cycle 1: Fetch opcode (handled before our execution)
- Cycle 2: Execute operation + perform dummy read of next byte at PC

The dummy read doesn't affect program behavior but ensures every CPU cycle performs
a memory operation, matching real 6502 hardware behavior.

### 11. Dummy Reads/Writes

**Status:** ✅ FIXED  
**Wiki Reference:** Implied by cycle-accurate behavior requirements

**Requirement:**
The 6502 performs dummy reads in several situations to maintain cycle timing:
- Write instructions perform a dummy read before the final write
- Indexed addressing modes perform dummy reads when crossing page boundaries
- Read-Modify-Write operations perform all required reads including dummy reads

**Implementation:**
Uses the `MemoryAccess` enum to distinguish operation types:
- `Read`: Performs final value read
- `Write`: Skips final read (performs dummy read earlier in cycle sequence)
- `ReadModifyWrite`: Performs all reads including dummy read during modification
- `Jump`: Special case for JMP instruction

**Tests Added:**
- `test_absolute_write_skips_final_read` - Verifies STA doesn't perform final read
- `test_absolutex_page_cross_dummy_read` - Verifies page-crossing generates 5-cycle LDA
- `test_absolutex_write_always_takes_5_cycles` - Verifies STA abs,X always takes 5 cycles with dummy read
- `test_rmw_performs_dummy_read` - Verifies INC performs all required reads (6 cycles)

**Key Behaviors Verified:**
- Write operations (STA, STX, STY) skip unnecessary final read
- Indexed modes with page-crossing perform dummy read from wrong page
- Write operations in indexed modes always take max cycles (perform dummy read)
- RMW operations perform complete read-modify-write sequence including dummy reads

**Implementation Files:**
- `src/cpu2/addressing.rs`: MemoryAccess enum and dummy read logic
- Lines 222-225: Absolute mode Write optimization
- Lines 459-467: AbsoluteX page-crossing dummy read
- Lines 474-476: AbsoluteX Write optimization

## Documentation and Testing

### 12. Comprehensive Interrupt Tests

**Status:** ✅ MOSTLY COMPLETE  
**Wiki Reference:** [CPU interrupts - Notes](https://www.nesdev.org/wiki/CPU_interrupts#Notes)

**Completed Tests:**

- [x] NMI triggering and execution - `test_nmi_trigger_full_sequence` (cycle-accurate, validates stack, vectors, flags)
- [x] IRQ triggering and execution - `test_irq_trigger_full_sequence` (cycle-accurate, validates stack, vectors, flags)
- [x] NMI priority over IRQ - `test_nmi_priority_over_irq` (from Issue #15)
- [x] Interrupt hijacking (NMI hijacks IRQ) - `test_nmi_hijacks_irq_sequence` (from Issue #15)
- [x] Interrupt sequencing - `test_no_nested_interrupts_during_sequence`, `test_rti_ends_interrupt_sequence` (from Issue #15)
- [x] Edge/level detection - 6 tests from Issue #15 verify NMI edge-triggered and IRQ level-triggered behavior

**Deferred (Architectural Changes Required):**

- [ ] Interrupt hijacking (NMI hijacks BRK) - Test exists but ignored; requires BRK to check NMI during sequence
- [ ] Interrupt hijacking (IRQ hijacks BRK) - Test exists but ignored; requires BRK to check IRQ during sequence
- [ ] CLI/SEI/PLP delayed IRQ behavior - Partially tested in Issue #3
- [ ] Branch instruction interrupt polling - Tracked in Issue #4
- [ ] Test ROM: cpu_interrupts_v2 - External validation

**Notes:**

BRK hijacking tests are marked `#[ignore]` because BRK currently hard-codes the IRQ vector and doesn't check for
NMI/IRQ during its 7-cycle sequence. This would require similar architectural changes to Issue #4 (Branch Instructions)
where interrupt polling needs to happen at specific cycle boundaries within an instruction's execution.

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

**Status:** ✅ FIXED  
**Wiki Reference:** [CPU interrupts](https://www.nesdev.org/wiki/CPU_interrupts)

**Completed:**

- [x] Edge detector state for NMI - `nmi_line_prev` field tracks previous line state
- [x] Level detector state for IRQ - `irq_line` field tracks current line state
- [x] Delayed I flag state (for CLI/SEI/PLP) - already implemented via `delay_interrupt_check` and `saved_i_flag_for_delay`
- [x] NMI edge-triggered behavior - triggers on high-to-low transition only
- [x] IRQ level-triggered behavior - active when line is low
- [x] IRQ pending state tracking - already implemented
- [x] Current interrupt sequence state - `in_interrupt_sequence` prevents nested interrupts
- [x] Interrupt priority handling - NMI has priority over IRQ, can hijack IRQ sequence

**Implementation:**

Signal Handling:
- `set_nmi_line(bool)` - Proper hardware interface for NMI signal, detects falling edges
- `set_irq_line(bool)` - Proper hardware interface for IRQ signal, level-triggered
- NMI triggers on high-to-low transition and won't retrigger while line stays low
- IRQ is active whenever line is low (level-triggered), respects I flag masking

Priority and Sequencing:
- `should_service_nmi()` - Returns true if NMI should be serviced (has priority)
- `get_interrupt_vector()` - Returns NMI vector if NMI pending, else IRQ (enables hijacking)
- `mark_interrupt_sequence_start/end()` - Track when in 7-cycle interrupt initiation
- `is_in_interrupt_sequence()` - Check if currently in interrupt sequence
- `should_poll_interrupts()` - Returns false during sequence to prevent nested interrupts
- Sequence flag cleared after first handler instruction executes
- Allows NMI to interrupt IRQ handler (I flag prevents IRQ self-nesting)

**Tests Added:**

Edge/Level Detection (6 tests):
- `test_nmi_edge_detection_high_to_low` - NMI triggers on falling edge
- `test_nmi_edge_detection_stays_low` - NMI doesn't retrigger while low
- `test_nmi_edge_detection_multiple_edges` - Multiple edges work correctly
- `test_irq_level_detection_active_low` - IRQ level-triggered behavior
- `test_irq_level_detection_masked_by_i_flag` - I flag masking
- `test_nmi_not_masked_by_i_flag` - NMI is non-maskable

Priority and Sequencing (4 tests):
- `test_nmi_priority_over_irq` - NMI has priority when both pending
- `test_nmi_hijacks_irq_sequence` - NMI hijacks IRQ (vector redirection)
- `test_no_nested_interrupts_during_sequence` - Polling blocked during 7-cycle sequence
- `test_rti_ends_interrupt_sequence` - Sequence flag cleared after first handler instruction

**Architecture:**

The interrupt system now properly models 6502 hardware:
1. NMI edge detector - tracks line state transitions
2. IRQ level detector - tracks current line state  
3. Priority - NMI always has priority over IRQ
4. Hijacking - NMI can hijack IRQ by redirecting vector read
5. Sequencing - 7-cycle interrupt initiation is atomic (no polling)
6. After first handler instruction, interrupts can be polled again
7. I flag prevents IRQ nesting but doesn't prevent NMI

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
