//! CPU operation implementations
//!
//! This module implements all 6502 operations as concrete types that implement
//! the Operation trait. Operations are timing-independent - they just perform
//! the logical operation on the CPU state.

use super::traits::{CpuState, Operation};

// Status flag bit positions
const FLAG_C: u8 = 0b0000_0001; // Carry
const FLAG_Z: u8 = 0b0000_0010; // Zero
const FLAG_I: u8 = 0b0000_0100; // Interrupt Disable
const FLAG_D: u8 = 0b0000_1000; // Decimal Mode (not used in NES)
const FLAG_B: u8 = 0b0001_0000; // Break Command
const FLAG_U: u8 = 0b0010_0000; // Unused (always set)
const FLAG_V: u8 = 0b0100_0000; // Overflow
const FLAG_N: u8 = 0b1000_0000; // Negative

/// Helper to set or clear a flag
fn set_flag(p: &mut u8, flag: u8, condition: bool) {
    if condition {
        *p |= flag;
    } else {
        *p &= !flag;
    }
}

/// Helper to update N and Z flags based on a value
fn update_nz_flags(p: &mut u8, value: u8) {
    set_flag(p, FLAG_N, value & 0x80 != 0);
    set_flag(p, FLAG_Z, value == 0);
}

// ============================================================================
// Load/Store Operations
// ============================================================================

/// LDA - Load Accumulator
#[derive(Debug, Clone, Copy)]
pub struct LDA;

impl Operation for LDA {
    fn execute(&self, state: &mut CpuState, operand: u8) {
        state.a = operand;
        update_nz_flags(&mut state.p, state.a);
    }
}

/// LDX - Load X Register
#[derive(Debug, Clone, Copy)]
pub struct LDX;

impl Operation for LDX {
    fn execute(&self, state: &mut CpuState, operand: u8) {
        state.x = operand;
        update_nz_flags(&mut state.p, state.x);
    }
}

/// LDY - Load Y Register
#[derive(Debug, Clone, Copy)]
pub struct LDY;

impl Operation for LDY {
    fn execute(&self, state: &mut CpuState, operand: u8) {
        state.y = operand;
        update_nz_flags(&mut state.p, state.y);
    }
}

/// STA - Store Accumulator
#[derive(Debug, Clone, Copy)]
pub struct STA;

impl Operation for STA {
    fn execute(&self, _state: &mut CpuState, _operand: u8) {
        // Store operations don't need to do anything in execute
        // The value to store comes from the register, not the operand
    }
}

/// STX - Store X Register
#[derive(Debug, Clone, Copy)]
pub struct STX;

impl Operation for STX {
    fn execute(&self, _state: &mut CpuState, _operand: u8) {
        // Store operations don't need to do anything in execute
    }
}

/// STY - Store Y Register
#[derive(Debug, Clone, Copy)]
pub struct STY;

impl Operation for STY {
    fn execute(&self, _state: &mut CpuState, _operand: u8) {
        // Store operations don't need to do anything in execute
    }
}

// ============================================================================
// Arithmetic Operations
// ============================================================================

/// ADC - Add with Carry
#[derive(Debug, Clone, Copy)]
pub struct ADC;

impl Operation for ADC {
    fn execute(&self, state: &mut CpuState, operand: u8) {
        let carry = if state.p & FLAG_C != 0 { 1 } else { 0 };
        let sum = state.a as u16 + operand as u16 + carry as u16;

        // Check for overflow: occurs when signs of A and operand are the same,
        // but result has different sign
        let overflow = (state.a ^ sum as u8) & (operand ^ sum as u8) & 0x80 != 0;

        state.a = sum as u8;
        set_flag(&mut state.p, FLAG_C, sum > 0xFF);
        set_flag(&mut state.p, FLAG_V, overflow);
        update_nz_flags(&mut state.p, state.a);
    }
}

/// SBC - Subtract with Carry
#[derive(Debug, Clone, Copy)]
pub struct SBC;

impl Operation for SBC {
    fn execute(&self, state: &mut CpuState, operand: u8) {
        // SBC is equivalent to ADC with inverted operand
        let carry = if state.p & FLAG_C != 0 { 1 } else { 0 };
        let diff = state.a as u16 + (!operand) as u16 + carry as u16;

        let overflow = (state.a ^ diff as u8) & ((!operand) ^ diff as u8) & 0x80 != 0;

        state.a = diff as u8;
        set_flag(&mut state.p, FLAG_C, diff > 0xFF);
        set_flag(&mut state.p, FLAG_V, overflow);
        update_nz_flags(&mut state.p, state.a);
    }
}

// ============================================================================
// Logical Operations
// ============================================================================

/// AND - Logical AND
#[derive(Debug, Clone, Copy)]
pub struct AND;

impl Operation for AND {
    fn execute(&self, state: &mut CpuState, operand: u8) {
        state.a &= operand;
        update_nz_flags(&mut state.p, state.a);
    }
}

/// ORA - Logical OR
#[derive(Debug, Clone, Copy)]
pub struct ORA;

impl Operation for ORA {
    fn execute(&self, state: &mut CpuState, operand: u8) {
        state.a |= operand;
        update_nz_flags(&mut state.p, state.a);
    }
}

/// EOR - Exclusive OR
#[derive(Debug, Clone, Copy)]
pub struct EOR;

impl Operation for EOR {
    fn execute(&self, state: &mut CpuState, operand: u8) {
        state.a ^= operand;
        update_nz_flags(&mut state.p, state.a);
    }
}

// ============================================================================
// Shift/Rotate Operations (RMW)
// ============================================================================

/// ASL - Arithmetic Shift Left
#[derive(Debug, Clone, Copy)]
pub struct ASL;

impl Operation for ASL {
    fn execute(&self, state: &mut CpuState, operand: u8) {
        // For accumulator mode
        let result = operand << 1;
        state.a = result;
        set_flag(&mut state.p, FLAG_C, operand & 0x80 != 0);
        update_nz_flags(&mut state.p, result);
    }

    fn execute_rmw(&self, state: &mut CpuState, operand: u8) -> u8 {
        let result = operand << 1;
        set_flag(&mut state.p, FLAG_C, operand & 0x80 != 0);
        update_nz_flags(&mut state.p, result);
        result
    }
}

/// LSR - Logical Shift Right
#[derive(Debug, Clone, Copy)]
pub struct LSR;

impl Operation for LSR {
    fn execute(&self, state: &mut CpuState, operand: u8) {
        // For accumulator mode
        let result = operand >> 1;
        state.a = result;
        set_flag(&mut state.p, FLAG_C, operand & 0x01 != 0);
        update_nz_flags(&mut state.p, result);
    }

    fn execute_rmw(&self, state: &mut CpuState, operand: u8) -> u8 {
        let result = operand >> 1;
        set_flag(&mut state.p, FLAG_C, operand & 0x01 != 0);
        update_nz_flags(&mut state.p, result);
        result
    }
}

/// ROL - Rotate Left
#[derive(Debug, Clone, Copy)]
pub struct ROL;

impl Operation for ROL {
    fn execute(&self, state: &mut CpuState, operand: u8) {
        // For accumulator mode
        let carry_in = if state.p & FLAG_C != 0 { 1 } else { 0 };
        let result = (operand << 1) | carry_in;
        state.a = result;
        set_flag(&mut state.p, FLAG_C, operand & 0x80 != 0);
        update_nz_flags(&mut state.p, result);
    }

    fn execute_rmw(&self, state: &mut CpuState, operand: u8) -> u8 {
        let carry_in = if state.p & FLAG_C != 0 { 1 } else { 0 };
        let result = (operand << 1) | carry_in;
        set_flag(&mut state.p, FLAG_C, operand & 0x80 != 0);
        update_nz_flags(&mut state.p, result);
        result
    }
}

/// ROR - Rotate Right
#[derive(Debug, Clone, Copy)]
pub struct ROR;

impl Operation for ROR {
    fn execute(&self, state: &mut CpuState, operand: u8) {
        // For accumulator mode
        let carry_in = if state.p & FLAG_C != 0 { 0x80 } else { 0 };
        let result = (operand >> 1) | carry_in;
        state.a = result;
        set_flag(&mut state.p, FLAG_C, operand & 0x01 != 0);
        update_nz_flags(&mut state.p, result);
    }

    fn execute_rmw(&self, state: &mut CpuState, operand: u8) -> u8 {
        let carry_in = if state.p & FLAG_C != 0 { 0x80 } else { 0 };
        let result = (operand >> 1) | carry_in;
        set_flag(&mut state.p, FLAG_C, operand & 0x01 != 0);
        update_nz_flags(&mut state.p, result);
        result
    }
}

// ============================================================================
// Increment/Decrement Operations
// ============================================================================

/// INC - Increment Memory
#[derive(Debug, Clone, Copy)]
pub struct INC;

impl Operation for INC {
    fn execute(&self, _state: &mut CpuState, _operand: u8) {
        // RMW only
    }

    fn execute_rmw(&self, state: &mut CpuState, operand: u8) -> u8 {
        let result = operand.wrapping_add(1);
        update_nz_flags(&mut state.p, result);
        result
    }
}

/// DEC - Decrement Memory
#[derive(Debug, Clone, Copy)]
pub struct DEC;

impl Operation for DEC {
    fn execute(&self, _state: &mut CpuState, _operand: u8) {
        // RMW only
    }

    fn execute_rmw(&self, state: &mut CpuState, operand: u8) -> u8 {
        let result = operand.wrapping_sub(1);
        update_nz_flags(&mut state.p, result);
        result
    }
}

/// INX - Increment X
#[derive(Debug, Clone, Copy)]
pub struct INX;

impl Operation for INX {
    fn execute(&self, state: &mut CpuState, _operand: u8) {
        state.x = state.x.wrapping_add(1);
        update_nz_flags(&mut state.p, state.x);
    }
}

/// INY - Increment Y
#[derive(Debug, Clone, Copy)]
pub struct INY;

impl Operation for INY {
    fn execute(&self, state: &mut CpuState, _operand: u8) {
        state.y = state.y.wrapping_add(1);
        update_nz_flags(&mut state.p, state.y);
    }
}

/// DEX - Decrement X
#[derive(Debug, Clone, Copy)]
pub struct DEX;

impl Operation for DEX {
    fn execute(&self, state: &mut CpuState, _operand: u8) {
        state.x = state.x.wrapping_sub(1);
        update_nz_flags(&mut state.p, state.x);
    }
}

/// DEY - Decrement Y
#[derive(Debug, Clone, Copy)]
pub struct DEY;

impl Operation for DEY {
    fn execute(&self, state: &mut CpuState, _operand: u8) {
        state.y = state.y.wrapping_sub(1);
        update_nz_flags(&mut state.p, state.y);
    }
}

// ============================================================================
// Compare Operations
// ============================================================================

/// CMP - Compare Accumulator
#[derive(Debug, Clone, Copy)]
pub struct CMP;

impl Operation for CMP {
    fn execute(&self, state: &mut CpuState, operand: u8) {
        let result = state.a.wrapping_sub(operand);
        set_flag(&mut state.p, FLAG_C, state.a >= operand);
        update_nz_flags(&mut state.p, result);
    }
}

/// CPX - Compare X Register
#[derive(Debug, Clone, Copy)]
pub struct CPX;

impl Operation for CPX {
    fn execute(&self, state: &mut CpuState, operand: u8) {
        let result = state.x.wrapping_sub(operand);
        set_flag(&mut state.p, FLAG_C, state.x >= operand);
        update_nz_flags(&mut state.p, result);
    }
}

/// CPY - Compare Y Register
#[derive(Debug, Clone, Copy)]
pub struct CPY;

impl Operation for CPY {
    fn execute(&self, state: &mut CpuState, operand: u8) {
        let result = state.y.wrapping_sub(operand);
        set_flag(&mut state.p, FLAG_C, state.y >= operand);
        update_nz_flags(&mut state.p, result);
    }
}

// ============================================================================
// Transfer Operations
// ============================================================================

/// TAX - Transfer A to X
#[derive(Debug, Clone, Copy)]
pub struct TAX;

impl Operation for TAX {
    fn execute(&self, state: &mut CpuState, _operand: u8) {
        state.x = state.a;
        update_nz_flags(&mut state.p, state.x);
    }
}

/// TAY - Transfer A to Y
#[derive(Debug, Clone, Copy)]
pub struct TAY;

impl Operation for TAY {
    fn execute(&self, state: &mut CpuState, _operand: u8) {
        state.y = state.a;
        update_nz_flags(&mut state.p, state.y);
    }
}

/// TXA - Transfer X to A
#[derive(Debug, Clone, Copy)]
pub struct TXA;

impl Operation for TXA {
    fn execute(&self, state: &mut CpuState, _operand: u8) {
        state.a = state.x;
        update_nz_flags(&mut state.p, state.a);
    }
}

/// TYA - Transfer Y to A
#[derive(Debug, Clone, Copy)]
pub struct TYA;

impl Operation for TYA {
    fn execute(&self, state: &mut CpuState, _operand: u8) {
        state.a = state.y;
        update_nz_flags(&mut state.p, state.a);
    }
}

/// TSX - Transfer SP to X
#[derive(Debug, Clone, Copy)]
pub struct TSX;

impl Operation for TSX {
    fn execute(&self, state: &mut CpuState, _operand: u8) {
        state.x = state.sp;
        update_nz_flags(&mut state.p, state.x);
    }
}

/// TXS - Transfer X to SP
#[derive(Debug, Clone, Copy)]
pub struct TXS;

impl Operation for TXS {
    fn execute(&self, state: &mut CpuState, _operand: u8) {
        state.sp = state.x;
        // TXS does not affect flags
    }
}

// ============================================================================
// Flag Operations
// ============================================================================

/// CLC - Clear Carry Flag
#[derive(Debug, Clone, Copy)]
pub struct CLC;

impl Operation for CLC {
    fn execute(&self, state: &mut CpuState, _operand: u8) {
        state.p &= !FLAG_C;
    }
}

/// SEC - Set Carry Flag
#[derive(Debug, Clone, Copy)]
pub struct SEC;

impl Operation for SEC {
    fn execute(&self, state: &mut CpuState, _operand: u8) {
        state.p |= FLAG_C;
    }
}

/// CLI - Clear Interrupt Disable
#[derive(Debug, Clone, Copy)]
pub struct CLI;

impl Operation for CLI {
    fn execute(&self, state: &mut CpuState, _operand: u8) {
        state.p &= !FLAG_I;
    }
}

/// SEI - Set Interrupt Disable
#[derive(Debug, Clone, Copy)]
pub struct SEI;

impl Operation for SEI {
    fn execute(&self, state: &mut CpuState, _operand: u8) {
        state.p |= FLAG_I;
    }
}

/// CLD - Clear Decimal Mode
#[derive(Debug, Clone, Copy)]
pub struct CLD;

impl Operation for CLD {
    fn execute(&self, state: &mut CpuState, _operand: u8) {
        state.p &= !FLAG_D;
    }
}

/// SED - Set Decimal Mode
#[derive(Debug, Clone, Copy)]
pub struct SED;

impl Operation for SED {
    fn execute(&self, state: &mut CpuState, _operand: u8) {
        state.p |= FLAG_D;
    }
}

/// CLV - Clear Overflow Flag
#[derive(Debug, Clone, Copy)]
pub struct CLV;

impl Operation for CLV {
    fn execute(&self, state: &mut CpuState, _operand: u8) {
        state.p &= !FLAG_V;
    }
}

// ============================================================================
// Stack Operations
// ============================================================================

/// PHA - Push Accumulator
#[derive(Debug, Clone, Copy)]
pub struct PHA;

impl Operation for PHA {
    fn execute(&self, _state: &mut CpuState, _operand: u8) {
        // Not used for stack operations
    }

    fn execute_stack(&self, state: &mut CpuState) -> u8 {
        let value = state.a;
        state.sp = state.sp.wrapping_sub(1);
        value
    }
}

/// PHP - Push Processor Status
#[derive(Debug, Clone, Copy)]
pub struct PHP;

impl Operation for PHP {
    fn execute(&self, _state: &mut CpuState, _operand: u8) {
        // Not used for stack operations
    }

    fn execute_stack(&self, state: &mut CpuState) -> u8 {
        // PHP pushes P with B and U flags set
        let value = state.p | FLAG_B | FLAG_U;
        state.sp = state.sp.wrapping_sub(1);
        value
    }
}

/// PLA - Pull Accumulator
#[derive(Debug, Clone, Copy)]
pub struct PLA;

impl Operation for PLA {
    fn execute(&self, _state: &mut CpuState, _operand: u8) {
        // Not used for stack operations
    }

    fn execute_pull(&self, state: &mut CpuState, value: u8) {
        state.sp = state.sp.wrapping_add(1);
        state.a = value;
        update_nz_flags(&mut state.p, value);
    }
}

/// PLP - Pull Processor Status
#[derive(Debug, Clone, Copy)]
pub struct PLP;

impl Operation for PLP {
    fn execute(&self, _state: &mut CpuState, _operand: u8) {
        // Not used for stack operations
    }

    fn execute_pull(&self, state: &mut CpuState, value: u8) {
        state.sp = state.sp.wrapping_add(1);
        // B flag is always clear, U flag is always set
        state.p = (value & !FLAG_B) | FLAG_U;
    }
}

// ============================================================================
// Bit Test Operation
// ============================================================================

/// BIT - Bit Test
#[derive(Debug, Clone, Copy)]
pub struct BIT;

impl Operation for BIT {
    fn execute(&self, state: &mut CpuState, operand: u8) {
        let result = state.a & operand;
        set_flag(&mut state.p, FLAG_Z, result == 0);
        set_flag(&mut state.p, FLAG_V, operand & FLAG_V != 0);
        set_flag(&mut state.p, FLAG_N, operand & FLAG_N != 0);
    }
}

// ============================================================================
// Control Flow Operations
// ============================================================================

/// JMP - Jump
#[derive(Debug, Clone, Copy)]
pub struct JMP;

impl Operation for JMP {
    fn execute(&self, _state: &mut CpuState, _operand: u8) {
        // Not used for control flow operations
    }

    fn execute_control(&self, _state: &mut CpuState, target_addr: u16) -> Option<u16> {
        // JMP simply sets PC to the target address
        Some(target_addr)
    }
}

/// JSR - Jump to Subroutine
#[derive(Debug, Clone, Copy)]
pub struct JSR;

impl Operation for JSR {
    fn execute(&self, _state: &mut CpuState, _operand: u8) {
        // Not used for control flow operations
    }

    fn execute_jsr(&self, state: &mut CpuState, _target_addr: u16, current_pc: u16) -> (u8, u8) {
        // JSR pushes PC-1 to stack (return address points to last byte of JSR instruction)
        let return_addr = current_pc.wrapping_sub(1);
        let high_byte = (return_addr >> 8) as u8;
        let low_byte = (return_addr & 0xFF) as u8;
        
        // Decrement SP twice (high byte pushed first, then low byte)
        state.sp = state.sp.wrapping_sub(2);
        
        (high_byte, low_byte)
    }
}

/// RTS - Return from Subroutine
#[derive(Debug, Clone, Copy)]
pub struct RTS;

impl Operation for RTS {
    fn execute(&self, _state: &mut CpuState, _operand: u8) {
        // Not used for control flow operations
    }

    fn execute_rts(&self, state: &mut CpuState, low_byte: u8, high_byte: u8) -> u16 {
        // Increment SP twice (pull low byte, then high byte)
        state.sp = state.sp.wrapping_add(2);
        
        // RTS pulls address and increments it (to skip past JSR instruction)
        let addr = ((high_byte as u16) << 8) | (low_byte as u16);
        addr.wrapping_add(1)
    }
}

/// RTI - Return from Interrupt
#[derive(Debug, Clone, Copy)]
pub struct RTI;

impl Operation for RTI {
    fn execute(&self, _state: &mut CpuState, _operand: u8) {
        // Not used for control flow operations
    }

    fn execute_rti(&self, state: &mut CpuState, status: u8, pc_low: u8, pc_high: u8) -> u16 {
        // Increment SP three times (pull status, PC low, PC high)
        state.sp = state.sp.wrapping_add(3);
        
        // Restore status with B flag clear, U flag set
        state.p = (status & !FLAG_B) | FLAG_U;
        
        // Restore PC
        ((pc_high as u16) << 8) | (pc_low as u16)
    }
}

// ============================================================================
// No Operation
// ============================================================================

/// NOP - No Operation
#[derive(Debug, Clone, Copy)]
pub struct NOP;

impl Operation for NOP {
    fn execute(&self, _state: &mut CpuState, _operand: u8) {
        // Do nothing
    }
}

// ============================================================================
// Unofficial/Undocumented Operations
// ============================================================================

/// LAX - Load A and X (LDA + LDX combined)
#[derive(Debug, Clone, Copy)]
pub struct LAX;

impl Operation for LAX {
    fn execute(&self, state: &mut CpuState, operand: u8) {
        state.a = operand;
        state.x = operand;
        update_nz_flags(&mut state.p, operand);
    }
}

/// SAX - Store A AND X
#[derive(Debug, Clone, Copy)]
pub struct SAX;

impl Operation for SAX {
    fn execute(&self, state: &mut CpuState, _operand: u8) {
        // For write instructions, the value to write is A & X
        // This is handled by the instruction sequencing
        // We just need to make sure state has the right value
        let _ = state.a & state.x; // Value will be used by sequencer
    }
}

/// DCP - Decrement memory then Compare with A (DEC + CMP)
#[derive(Debug, Clone, Copy)]
pub struct DCP;

impl Operation for DCP {
    fn execute(&self, _state: &mut CpuState, _operand: u8) {
        // Read-side effects if any
    }

    fn execute_rmw(&self, state: &mut CpuState, operand: u8) -> u8 {
        // Decrement
        let result = operand.wrapping_sub(1);

        // Compare A with result (same as CMP)
        let diff = state.a.wrapping_sub(result);
        set_flag(&mut state.p, FLAG_C, state.a >= result);
        update_nz_flags(&mut state.p, diff);

        result
    }
}

/// ISB (ISC) - Increment memory then SBC (INC + SBC)
#[derive(Debug, Clone, Copy)]
pub struct ISB;

impl Operation for ISB {
    fn execute(&self, _state: &mut CpuState, _operand: u8) {
        // Read-side effects if any
    }

    fn execute_rmw(&self, state: &mut CpuState, operand: u8) -> u8 {
        // Increment
        let result = operand.wrapping_add(1);

        // SBC with result
        let carry = if state.p & FLAG_C != 0 { 1 } else { 0 };
        let diff = state.a.wrapping_sub(result).wrapping_sub(1 - carry);

        set_flag(
            &mut state.p,
            FLAG_C,
            (state.a as u16) >= (result as u16 + 1 - carry as u16),
        );
        set_flag(
            &mut state.p,
            FLAG_V,
            ((state.a ^ result) & (state.a ^ diff) & 0x80) != 0,
        );
        update_nz_flags(&mut state.p, diff);

        state.a = diff;
        result
    }
}

/// SLO - Shift Left then OR with A (ASL + ORA)
#[derive(Debug, Clone, Copy)]
pub struct SLO;

impl Operation for SLO {
    fn execute(&self, _state: &mut CpuState, _operand: u8) {
        // Read-side effects if any
    }

    fn execute_rmw(&self, state: &mut CpuState, operand: u8) -> u8 {
        // Shift left
        let result = operand << 1;
        set_flag(&mut state.p, FLAG_C, operand & 0x80 != 0);

        // OR with A
        state.a |= result;
        update_nz_flags(&mut state.p, state.a);

        result
    }
}

/// RLA - Rotate Left then AND with A (ROL + AND)
#[derive(Debug, Clone, Copy)]
pub struct RLA;

impl Operation for RLA {
    fn execute(&self, _state: &mut CpuState, _operand: u8) {
        // Read-side effects if any
    }

    fn execute_rmw(&self, state: &mut CpuState, operand: u8) -> u8 {
        // Rotate left
        let carry_in = if state.p & FLAG_C != 0 { 1 } else { 0 };
        let result = (operand << 1) | carry_in;
        set_flag(&mut state.p, FLAG_C, operand & 0x80 != 0);

        // AND with A
        state.a &= result;
        update_nz_flags(&mut state.p, state.a);

        result
    }
}

/// SRE - Shift Right then XOR with A (LSR + EOR)
#[derive(Debug, Clone, Copy)]
pub struct SRE;

impl Operation for SRE {
    fn execute(&self, _state: &mut CpuState, _operand: u8) {
        // Read-side effects if any
    }

    fn execute_rmw(&self, state: &mut CpuState, operand: u8) -> u8 {
        // Shift right
        let result = operand >> 1;
        set_flag(&mut state.p, FLAG_C, operand & 0x01 != 0);

        // XOR with A
        state.a ^= result;
        update_nz_flags(&mut state.p, state.a);

        result
    }
}

/// RRA - Rotate Right then ADC (ROR + ADC)
#[derive(Debug, Clone, Copy)]
pub struct RRA;

impl Operation for RRA {
    fn execute(&self, _state: &mut CpuState, _operand: u8) {
        // Read-side effects if any
    }

    fn execute_rmw(&self, state: &mut CpuState, operand: u8) -> u8 {
        // Rotate right
        let carry_in = if state.p & FLAG_C != 0 { 0x80 } else { 0 };
        let result = (operand >> 1) | carry_in;
        let carry_out = operand & 0x01 != 0;

        // ADC with result
        let carry = if carry_out { 1 } else { 0 };
        let sum = state.a as u16 + result as u16 + carry as u16;

        set_flag(&mut state.p, FLAG_C, sum > 0xFF);
        set_flag(
            &mut state.p,
            FLAG_V,
            ((state.a ^ result) & 0x80 == 0) && ((state.a ^ sum as u8) & 0x80 != 0),
        );

        state.a = sum as u8;
        update_nz_flags(&mut state.p, state.a);

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_state() -> CpuState {
        CpuState {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFF,
            p: 0,
        }
    }

    // ========================================================================
    // Load/Store Tests
    // ========================================================================

    #[test]
    fn test_lda() {
        let mut state = create_state();
        let op = LDA;

        op.execute(&mut state, 0x42);
        assert_eq!(state.a, 0x42);
        assert_eq!(state.p & FLAG_Z, 0);
        assert_eq!(state.p & FLAG_N, 0);

        // Test zero flag
        op.execute(&mut state, 0x00);
        assert_eq!(state.a, 0x00);
        assert_eq!(state.p & FLAG_Z, FLAG_Z);

        // Test negative flag
        op.execute(&mut state, 0x80);
        assert_eq!(state.a, 0x80);
        assert_eq!(state.p & FLAG_N, FLAG_N);
    }

    #[test]
    fn test_ldx() {
        let mut state = create_state();
        let op = LDX;

        op.execute(&mut state, 0x42);
        assert_eq!(state.x, 0x42);
        assert_eq!(state.p & FLAG_Z, 0);
        assert_eq!(state.p & FLAG_N, 0);
    }

    #[test]
    fn test_ldy() {
        let mut state = create_state();
        let op = LDY;

        op.execute(&mut state, 0x42);
        assert_eq!(state.y, 0x42);
        assert_eq!(state.p & FLAG_Z, 0);
        assert_eq!(state.p & FLAG_N, 0);
    }

    // ========================================================================
    // Arithmetic Tests
    // ========================================================================

    #[test]
    fn test_adc_no_carry() {
        let mut state = create_state();
        let op = ADC;

        state.a = 0x10;
        op.execute(&mut state, 0x20);

        assert_eq!(state.a, 0x30);
        assert_eq!(state.p & FLAG_C, 0);
        assert_eq!(state.p & FLAG_V, 0);
        assert_eq!(state.p & FLAG_Z, 0);
        assert_eq!(state.p & FLAG_N, 0);
    }

    #[test]
    fn test_adc_with_carry() {
        let mut state = create_state();
        let op = ADC;

        state.a = 0xFF;
        op.execute(&mut state, 0x01);

        assert_eq!(state.a, 0x00);
        assert_eq!(state.p & FLAG_C, FLAG_C);
        assert_eq!(state.p & FLAG_Z, FLAG_Z);
    }

    #[test]
    fn test_adc_overflow() {
        let mut state = create_state();
        let op = ADC;

        state.a = 0x7F; // Positive
        op.execute(&mut state, 0x01);

        assert_eq!(state.a, 0x80);
        assert_eq!(state.p & FLAG_V, FLAG_V); // Overflow
        assert_eq!(state.p & FLAG_N, FLAG_N); // Result is negative
    }

    #[test]
    fn test_sbc() {
        let mut state = create_state();
        let op = SBC;

        state.a = 0x30;
        state.p |= FLAG_C; // Carry set (no borrow)
        op.execute(&mut state, 0x10);

        assert_eq!(state.a, 0x20);
        assert_eq!(state.p & FLAG_C, FLAG_C);
    }

    // ========================================================================
    // Logical Tests
    // ========================================================================

    #[test]
    fn test_and() {
        let mut state = create_state();
        let op = AND;

        state.a = 0b1111_0000;
        op.execute(&mut state, 0b1010_1010);

        assert_eq!(state.a, 0b1010_0000);
        assert_eq!(state.p & FLAG_N, FLAG_N);
    }

    #[test]
    fn test_ora() {
        let mut state = create_state();
        let op = ORA;

        state.a = 0b1111_0000;
        op.execute(&mut state, 0b0000_1111);

        assert_eq!(state.a, 0b1111_1111);
        assert_eq!(state.p & FLAG_N, FLAG_N);
    }

    #[test]
    fn test_eor() {
        let mut state = create_state();
        let op = EOR;

        state.a = 0b1111_0000;
        op.execute(&mut state, 0b1010_1010);

        assert_eq!(state.a, 0b0101_1010);
    }

    // ========================================================================
    // Shift/Rotate Tests
    // ========================================================================

    #[test]
    fn test_asl_accumulator() {
        let mut state = create_state();
        let op = ASL;

        state.a = 0b0100_0001;
        let value = state.a;
        op.execute(&mut state, value);

        assert_eq!(state.a, 0b1000_0010);
        assert_eq!(state.p & FLAG_C, 0);
        assert_eq!(state.p & FLAG_N, FLAG_N);
    }

    #[test]
    fn test_asl_rmw() {
        let mut state = create_state();
        let op = ASL;

        let result = op.execute_rmw(&mut state, 0b1100_0001);

        assert_eq!(result, 0b1000_0010);
        assert_eq!(state.p & FLAG_C, FLAG_C); // Bit 7 was set
    }

    #[test]
    fn test_lsr() {
        let mut state = create_state();
        let op = LSR;

        let result = op.execute_rmw(&mut state, 0b1000_0011);

        assert_eq!(result, 0b0100_0001);
        assert_eq!(state.p & FLAG_C, FLAG_C); // Bit 0 was set
    }

    #[test]
    fn test_rol() {
        let mut state = create_state();
        let op = ROL;

        state.p |= FLAG_C; // Set carry
        let result = op.execute_rmw(&mut state, 0b0100_0000);

        assert_eq!(result, 0b1000_0001); // Shifted left with carry in
        assert_eq!(state.p & FLAG_C, 0); // Bit 7 was 0
    }

    #[test]
    fn test_ror() {
        let mut state = create_state();
        let op = ROR;

        state.p |= FLAG_C; // Set carry
        let result = op.execute_rmw(&mut state, 0b0000_0010);

        assert_eq!(result, 0b1000_0001); // Shifted right with carry in
        assert_eq!(state.p & FLAG_C, 0); // Bit 0 was 0
    }

    // ========================================================================
    // Increment/Decrement Tests
    // ========================================================================

    #[test]
    fn test_inc() {
        let mut state = create_state();
        let op = INC;

        let result = op.execute_rmw(&mut state, 0x42);
        assert_eq!(result, 0x43);

        // Test wrap
        let result = op.execute_rmw(&mut state, 0xFF);
        assert_eq!(result, 0x00);
        assert_eq!(state.p & FLAG_Z, FLAG_Z);
    }

    #[test]
    fn test_dec() {
        let mut state = create_state();
        let op = DEC;

        let result = op.execute_rmw(&mut state, 0x42);
        assert_eq!(result, 0x41);

        // Test wrap
        let result = op.execute_rmw(&mut state, 0x00);
        assert_eq!(result, 0xFF);
        assert_eq!(state.p & FLAG_N, FLAG_N);
    }

    #[test]
    fn test_inx() {
        let mut state = create_state();
        let op = INX;

        state.x = 0x42;
        op.execute(&mut state, 0);
        assert_eq!(state.x, 0x43);
    }

    #[test]
    fn test_iny() {
        let mut state = create_state();
        let op = INY;

        state.y = 0x42;
        op.execute(&mut state, 0);
        assert_eq!(state.y, 0x43);
    }

    #[test]
    fn test_dex() {
        let mut state = create_state();
        let op = DEX;

        state.x = 0x42;
        op.execute(&mut state, 0);
        assert_eq!(state.x, 0x41);
    }

    #[test]
    fn test_dey() {
        let mut state = create_state();
        let op = DEY;

        state.y = 0x42;
        op.execute(&mut state, 0);
        assert_eq!(state.y, 0x41);
    }

    // ========================================================================
    // Compare Tests
    // ========================================================================

    #[test]
    fn test_cmp_equal() {
        let mut state = create_state();
        let op = CMP;

        state.a = 0x42;
        op.execute(&mut state, 0x42);

        assert_eq!(state.p & FLAG_Z, FLAG_Z); // Equal
        assert_eq!(state.p & FLAG_C, FLAG_C); // A >= operand
    }

    #[test]
    fn test_cmp_less() {
        let mut state = create_state();
        let op = CMP;

        state.a = 0x30;
        op.execute(&mut state, 0x40);

        assert_eq!(state.p & FLAG_Z, 0); // Not equal
        assert_eq!(state.p & FLAG_C, 0); // A < operand
        assert_eq!(state.p & FLAG_N, FLAG_N); // Result is negative
    }

    #[test]
    fn test_cmp_greater() {
        let mut state = create_state();
        let op = CMP;

        state.a = 0x50;
        op.execute(&mut state, 0x40);

        assert_eq!(state.p & FLAG_Z, 0); // Not equal
        assert_eq!(state.p & FLAG_C, FLAG_C); // A >= operand
    }

    #[test]
    fn test_cpx() {
        let mut state = create_state();
        let op = CPX;

        state.x = 0x42;
        op.execute(&mut state, 0x42);

        assert_eq!(state.p & FLAG_Z, FLAG_Z);
        assert_eq!(state.p & FLAG_C, FLAG_C);
    }

    #[test]
    fn test_cpy() {
        let mut state = create_state();
        let op = CPY;

        state.y = 0x42;
        op.execute(&mut state, 0x42);

        assert_eq!(state.p & FLAG_Z, FLAG_Z);
        assert_eq!(state.p & FLAG_C, FLAG_C);
    }

    // ========================================================================
    // Transfer Tests
    // ========================================================================

    #[test]
    fn test_tax() {
        let mut state = create_state();
        let op = TAX;

        state.a = 0x42;
        op.execute(&mut state, 0);

        assert_eq!(state.x, 0x42);
        assert_eq!(state.p & FLAG_Z, 0);
    }

    #[test]
    fn test_tay() {
        let mut state = create_state();
        let op = TAY;

        state.a = 0x42;
        op.execute(&mut state, 0);

        assert_eq!(state.y, 0x42);
    }

    #[test]
    fn test_txa() {
        let mut state = create_state();
        let op = TXA;

        state.x = 0x42;
        op.execute(&mut state, 0);

        assert_eq!(state.a, 0x42);
    }

    #[test]
    fn test_tya() {
        let mut state = create_state();
        let op = TYA;

        state.y = 0x42;
        op.execute(&mut state, 0);

        assert_eq!(state.a, 0x42);
    }

    #[test]
    fn test_tsx() {
        let mut state = create_state();
        let op = TSX;

        state.sp = 0x42;
        op.execute(&mut state, 0);

        assert_eq!(state.x, 0x42);
    }

    #[test]
    fn test_txs() {
        let mut state = create_state();
        let op = TXS;

        state.x = 0x42;
        let p_before = state.p;
        op.execute(&mut state, 0);

        assert_eq!(state.sp, 0x42);
        assert_eq!(state.p, p_before); // TXS doesn't affect flags
    }

    // ========================================================================
    // Flag Tests
    // ========================================================================

    #[test]
    fn test_flag_operations() {
        let mut state = create_state();

        // CLC
        state.p = 0xFF;
        CLC.execute(&mut state, 0);
        assert_eq!(state.p & FLAG_C, 0);

        // SEC
        state.p = 0x00;
        SEC.execute(&mut state, 0);
        assert_eq!(state.p & FLAG_C, FLAG_C);

        // CLI
        state.p = 0xFF;
        CLI.execute(&mut state, 0);
        assert_eq!(state.p & FLAG_I, 0);

        // SEI
        state.p = 0x00;
        SEI.execute(&mut state, 0);
        assert_eq!(state.p & FLAG_I, FLAG_I);

        // CLD
        state.p = 0xFF;
        CLD.execute(&mut state, 0);
        assert_eq!(state.p & FLAG_D, 0);

        // SED
        state.p = 0x00;
        SED.execute(&mut state, 0);
        assert_eq!(state.p & FLAG_D, FLAG_D);

        // CLV
        state.p = 0xFF;
        CLV.execute(&mut state, 0);
        assert_eq!(state.p & FLAG_V, 0);
    }

    // ========================================================================
    // Bit Test
    // ========================================================================

    #[test]
    fn test_bit() {
        let mut state = create_state();
        let op = BIT;

        state.a = 0b1100_0011;
        op.execute(&mut state, 0b1100_0000);

        assert_eq!(state.p & FLAG_Z, 0); // Result not zero
        assert_eq!(state.p & FLAG_V, FLAG_V); // Bit 6 of operand
        assert_eq!(state.p & FLAG_N, FLAG_N); // Bit 7 of operand

        // Test zero result
        state.a = 0b0011_1100;
        op.execute(&mut state, 0b1100_0000);
        assert_eq!(state.p & FLAG_Z, FLAG_Z);
    }

    // ========================================================================
    // NOP Test
    // ========================================================================

    #[test]
    fn test_nop() {
        let mut state = create_state();
        let state_before = state.clone();
        let op = NOP;

        op.execute(&mut state, 0x42);

        // State should be unchanged
        assert_eq!(state.a, state_before.a);
        assert_eq!(state.x, state_before.x);
        assert_eq!(state.y, state_before.y);
        assert_eq!(state.sp, state_before.sp);
        assert_eq!(state.p, state_before.p);
    }

    // ============================================================================
    // Unofficial Operations Tests
    // ============================================================================

    #[test]
    fn test_lax() {
        let mut state = create_state();
        let op = LAX;

        op.execute(&mut state, 0x42);

        assert_eq!(state.a, 0x42);
        assert_eq!(state.x, 0x42);
        assert_eq!(state.p & FLAG_Z, 0);
        assert_eq!(state.p & FLAG_N, 0);
    }

    #[test]
    fn test_sax() {
        let mut state = create_state();
        state.a = 0xF0;
        state.x = 0x0F;
        let op = SAX;

        op.execute(&mut state, 0x00); // operand not used for SAX

        // SAX stores A AND X, but doesn't modify CPU state
        // The actual value is returned/used by instruction sequencing
    }

    #[test]
    fn test_dcp() {
        let mut state = create_state();
        state.a = 0x10;
        let op = DCP;

        let result = op.execute_rmw(&mut state, 0x05);

        // DCP decrements memory value
        assert_eq!(result, 0x04);
        // Then compares with A (0x10 > 0x04, so carry set, not zero, not negative)
        assert_eq!(state.p & FLAG_C, FLAG_C);
        assert_eq!(state.p & FLAG_Z, 0);
        assert_eq!(state.p & FLAG_N, 0);
    }

    #[test]
    fn test_isb() {
        let mut state = create_state();
        state.a = 0x10;
        state.p |= FLAG_C; // Set carry for SBC
        let op = ISB;

        let result = op.execute_rmw(&mut state, 0x05);

        // ISB increments memory value
        assert_eq!(result, 0x06);
        // Then subtracts from A: 0x10 - 0x06 - 0 = 0x0A
        assert_eq!(state.a, 0x0A);
    }

    #[test]
    fn test_slo() {
        let mut state = create_state();
        state.a = 0x0F;
        let op = SLO;

        let result = op.execute_rmw(&mut state, 0x40);

        // SLO shifts left (0x40 << 1 = 0x80)
        assert_eq!(result, 0x80);
        // Then ORs with A (0x80 | 0x0F = 0x8F)
        assert_eq!(state.a, 0x8F);
        assert_eq!(state.p & FLAG_N, FLAG_N);
    }

    #[test]
    fn test_rla() {
        let mut state = create_state();
        state.a = 0x0F;
        state.p &= !FLAG_C; // Clear carry
        let op = RLA;

        let result = op.execute_rmw(&mut state, 0x40);

        // RLA rotates left (0x40 << 1 with carry 0 = 0x80)
        assert_eq!(result, 0x80);
        // Then ANDs with A (0x80 & 0x0F = 0x00)
        assert_eq!(state.a, 0x00);
        assert_eq!(state.p & FLAG_Z, FLAG_Z);
    }

    #[test]
    fn test_sre() {
        let mut state = create_state();
        state.a = 0xF0;
        let op = SRE;

        let result = op.execute_rmw(&mut state, 0x81);

        // SRE shifts right (0x81 >> 1 = 0x40, carry = 1)
        assert_eq!(result, 0x40);
        assert_eq!(state.p & FLAG_C, FLAG_C);
        // Then XORs with A (0x40 ^ 0xF0 = 0xB0)
        assert_eq!(state.a, 0xB0);
    }

    #[test]
    fn test_rra() {
        let mut state = create_state();
        state.a = 0x10;
        state.p |= FLAG_C; // Set carry
        let op = RRA;

        let result = op.execute_rmw(&mut state, 0x40);

        // RRA rotates right (0x40 >> 1 with carry 1 = 0xA0)
        assert_eq!(result, 0xA0);
        // Then adds to A (0x10 + 0xA0 = 0xB0)
        assert_eq!(state.a, 0xB0);
    }

    // ========================================================================
    // Stack Operation Tests
    // ========================================================================

    #[test]
    fn test_pha() {
        let mut state = create_state();
        state.a = 0x42;
        state.sp = 0xFF;
        let op = PHA;

        // PHA should return the value to push (accumulator)
        let value = op.execute_stack(&mut state);
        assert_eq!(value, 0x42);
        // SP should be decremented after push
        assert_eq!(state.sp, 0xFE);
    }

    #[test]
    fn test_php() {
        let mut state = create_state();
        state.p = 0xA5;
        state.sp = 0xFF;
        let op = PHP;

        // PHP should return the value to push (status with B and U flags set)
        let value = op.execute_stack(&mut state);
        // PHP pushes P with B (0x10) and U (0x20) flags set
        assert_eq!(value, 0xA5 | 0x30);
        assert_eq!(state.sp, 0xFE);
    }

    #[test]
    fn test_pla() {
        let mut state = create_state();
        state.a = 0x00;
        state.sp = 0xFD;
        let op = PLA;

        // PLA should pull a value and update A and flags
        op.execute_pull(&mut state, 0x42);
        assert_eq!(state.a, 0x42);
        assert_eq!(state.sp, 0xFE);
        assert_eq!(state.p & FLAG_Z, 0);
        assert_eq!(state.p & FLAG_N, 0);

        // Test zero flag
        op.execute_pull(&mut state, 0x00);
        assert_eq!(state.a, 0x00);
        assert_eq!(state.p & FLAG_Z, FLAG_Z);

        // Test negative flag
        state.sp = 0xFD;
        op.execute_pull(&mut state, 0x80);
        assert_eq!(state.a, 0x80);
        assert_eq!(state.p & FLAG_N, FLAG_N);
    }

    #[test]
    fn test_plp() {
        let mut state = create_state();
        state.p = 0x00;
        state.sp = 0xFD;
        let op = PLP;

        // PLP should pull a value and update P (with B and U flags ignored)
        op.execute_pull(&mut state, 0xFF);
        // B flag is always clear, U flag is always set
        assert_eq!(state.p, (0xFF & !FLAG_B) | FLAG_U);
        assert_eq!(state.sp, 0xFE);
    }

    #[test]
    fn test_pha_stack_wraps() {
        let mut state = create_state();
        state.a = 0x42;
        state.sp = 0x00;
        let op = PHA;

        // SP should wrap from 0x00 to 0xFF
        let value = op.execute_stack(&mut state);
        assert_eq!(value, 0x42);
        assert_eq!(state.sp, 0xFF);
    }

    #[test]
    fn test_pla_stack_wraps() {
        let mut state = create_state();
        state.sp = 0xFF;
        let op = PLA;

        // SP should wrap from 0xFF to 0x00
        op.execute_pull(&mut state, 0x42);
        assert_eq!(state.a, 0x42);
        assert_eq!(state.sp, 0x00);
    }

    // ========================================================================
    // Control Flow Operation Tests
    // ========================================================================

    #[test]
    fn test_jmp() {
        let mut state = create_state();
        let op = JMP;

        // JMP should return the target address
        let new_pc = op.execute_control(&mut state, 0x1234);
        assert_eq!(new_pc, Some(0x1234));

        // JMP doesn't modify any CPU state
        assert_eq!(state.a, 0);
        assert_eq!(state.x, 0);
        assert_eq!(state.y, 0);
        assert_eq!(state.sp, 0xFF);
        assert_eq!(state.p, 0);
    }

    #[test]
    fn test_jsr_pushes_return_address() {
        let mut state = create_state();
        state.sp = 0xFF;
        let op = JSR;

        // JSR pushes PC-1 to stack (high byte first, then low byte)
        // Current PC is 0x1234, so it should push 0x1233
        let (high_byte, low_byte) = op.execute_jsr(&mut state, 0x5678, 0x1234);
        
        assert_eq!(high_byte, 0x12); // High byte of 0x1233
        assert_eq!(low_byte, 0x33);  // Low byte of 0x1233
        assert_eq!(state.sp, 0xFD);  // SP decremented twice
    }

    #[test]
    fn test_rts_pulls_return_address() {
        let mut state = create_state();
        state.sp = 0xFD;
        let op = RTS;

        // RTS pulls return address from stack and increments it
        // Pull 0x1233, return 0x1234
        let new_pc = op.execute_rts(&mut state, 0x33, 0x12);
        
        assert_eq!(new_pc, 0x1234);
        assert_eq!(state.sp, 0xFF); // SP incremented twice
    }

    #[test]
    fn test_rti_pulls_status_and_address() {
        let mut state = create_state();
        state.sp = 0xFC;
        state.p = 0x00;
        let op = RTI;

        // RTI pulls P, then PC low, then PC high
        let new_pc = op.execute_rti(&mut state, 0xA5, 0x34, 0x12);
        
        assert_eq!(new_pc, 0x1234);
        assert_eq!(state.p, (0xA5 & !FLAG_B) | FLAG_U); // P with B clear, U set
        assert_eq!(state.sp, 0xFF); // SP incremented three times
    }
}
