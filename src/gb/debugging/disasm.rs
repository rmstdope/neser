/// SM83 (Game Boy CPU) disassembler for CPU tracing.
///
/// Formats SM83 instructions with resolved operand values similar to NES CPU tracing format.
use crate::gb::cpu::opcode;

/// Format a single SM83 instruction with resolved operands.
///
/// # Arguments
/// * `opcode` - The opcode byte (or second byte for CB-prefixed instructions)
/// * `pc` - The program counter value at the start of the instruction
/// * `bytes` - The raw instruction bytes (includes opcode + operands)
///
/// # Returns
/// A formatted string like "LD A,$50" or "JP $C000"
pub fn format_instruction(opcode: u8, pc: u16, bytes: &[u8]) -> String {
    // Handle CB-prefixed instructions (0xCB prefix)
    let (meta, is_cb) = if bytes.len() >= 2 && bytes[0] == 0xCB {
        (opcode::lookup_cb(opcode), true)
    } else {
        (opcode::lookup(opcode), false)
    };

    let mnemonic = meta.mnemonic;

    // Parse mnemonic to identify operand patterns and resolve them
    resolve_operands(mnemonic, pc, bytes, is_cb)
}

/// Format instruction bytes as a hex string with proper padding (8 characters).
///
/// Examples:
/// - 1 byte:  "3E       "
/// - 2 bytes: "3E 50    "
/// - 3 bytes: "C3 00 01 "
pub fn format_disasm_bytes(bytes: &[u8]) -> String {
    match bytes.len() {
        0 => String::from("        "),
        1 => format!("{:02X}       ", bytes[0]),
        2 => format!("{:02X} {:02X}    ", bytes[0], bytes[1]),
        _ => format!("{:02X} {:02X} {:02X} ", bytes[0], bytes[1], bytes[2]),
    }
}

fn resolve_operands(mnemonic: &str, pc: u16, bytes: &[u8], is_cb: bool) -> String {
    // If mnemonic doesn't contain operand placeholders, return as-is
    if !mnemonic.contains("n8") && !mnemonic.contains("n16") && !mnemonic.contains("e8") {
        return mnemonic.to_string();
    }

    let mut result = mnemonic.to_string();

    // Determine operand start index (1 for base opcodes, 2 for CB-prefixed)
    let operand_start = if is_cb { 2 } else { 1 };

    // Replace n8 (8-bit immediate) with actual value
    if result.contains("n8") {
        if let Some(&byte) = bytes.get(operand_start) {
            result = result.replace("n8", &format!("${:02X}", byte));
        }
    }

    // Replace n16 (16-bit immediate, little-endian) with actual value
    if result.contains("n16") {
        if bytes.len() >= operand_start + 2 {
            let lo = bytes[operand_start];
            let hi = bytes[operand_start + 1];
            let addr = u16::from_le_bytes([lo, hi]);
            result = result.replace("n16", &format!("${:04X}", addr));
        }
    }

    // Replace e8 (signed 8-bit offset) with calculated target address
    if result.contains("e8") {
        if let Some(&offset_byte) = bytes.get(operand_start) {
            let offset = offset_byte as i8;
            // Target = PC + instruction_length + offset
            // For e8 instructions, length is always 2 bytes
            let target = pc.wrapping_add(2).wrapping_add(offset as i16 as u16);
            result = result.replace("e8", &format!("${:04X}", target));
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_disasm_bytes_one_byte() {
        let bytes = vec![0x3E];
        assert_eq!(format_disasm_bytes(&bytes), "3E       ");
    }

    #[test]
    fn test_format_disasm_bytes_two_bytes() {
        let bytes = vec![0x3E, 0x50];
        assert_eq!(format_disasm_bytes(&bytes), "3E 50    ");
    }

    #[test]
    fn test_format_disasm_bytes_three_bytes() {
        let bytes = vec![0xC3, 0x00, 0x01];
        assert_eq!(format_disasm_bytes(&bytes), "C3 00 01 ");
    }

    #[test]
    fn test_format_instruction_ld_a_n8() {
        // 0x3E = LD A,n8
        let opcode = 0x3E;
        let pc = 0x0100;
        let bytes = vec![0x3E, 0x50];
        let result = format_instruction(opcode, pc, &bytes);
        assert_eq!(result, "LD A,$50");
    }

    #[test]
    fn test_format_instruction_jp_n16() {
        // 0xC3 = JP n16
        let opcode = 0xC3;
        let pc = 0x0100;
        let bytes = vec![0xC3, 0x00, 0x01];
        let result = format_instruction(opcode, pc, &bytes);
        assert_eq!(result, "JP $0100");
    }

    #[test]
    fn test_format_instruction_jr_e8() {
        // 0x18 = JR e8
        // If at PC=0x0100, with offset +10, target = 0x0102 + 10 = 0x010C
        let opcode = 0x18;
        let pc = 0x0100;
        let bytes = vec![0x18, 0x0A]; // offset = +10
        let result = format_instruction(opcode, pc, &bytes);
        assert_eq!(result, "JR $010C");
    }

    #[test]
    fn test_format_instruction_jr_e8_negative() {
        // JR e8 with negative offset
        // At PC=0x0100, with offset -5 (0xFB), target = 0x0102 + (-5) = 0x00FD
        let opcode = 0x18;
        let pc = 0x0100;
        let bytes = vec![0x18, 0xFB]; // offset = -5
        let result = format_instruction(opcode, pc, &bytes);
        assert_eq!(result, "JR $00FD");
    }

    #[test]
    fn test_format_instruction_nop() {
        // 0x00 = NOP (no operands)
        let opcode = 0x00;
        let pc = 0x0100;
        let bytes = vec![0x00];
        let result = format_instruction(opcode, pc, &bytes);
        assert_eq!(result, "NOP");
    }

    #[test]
    fn test_format_instruction_ld_bc_n16() {
        // 0x01 = LD BC,n16
        let opcode = 0x01;
        let pc = 0x0100;
        let bytes = vec![0x01, 0x34, 0x12]; // BC = 0x1234 (little-endian)
        let result = format_instruction(opcode, pc, &bytes);
        assert_eq!(result, "LD BC,$1234");
    }

    #[test]
    fn test_format_instruction_cb_prefix() {
        // 0xCB 0x00 = RLC B
        let opcode = 0x00; // The byte after CB
        let pc = 0x0100;
        let bytes = vec![0xCB, 0x00];
        let result = format_instruction(opcode, pc, &bytes);
        assert_eq!(result, "RLC B");
    }

    #[test]
    fn test_format_instruction_ldh_n8_a() {
        // 0xE0 = LDH (n8),A
        let opcode = 0xE0;
        let pc = 0x0100;
        let bytes = vec![0xE0, 0x90];
        let result = format_instruction(opcode, pc, &bytes);
        // Mnemonic should show "LDH (n8),A" → "LDH ($90),A"
        assert_eq!(result, "LDH ($90),A");
    }
}
