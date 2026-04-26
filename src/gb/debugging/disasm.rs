/// SM83 (Game Boy CPU) disassembler for CPU tracing and debugger window display.
///
/// Formats SM83 instructions with resolved operand values similar to NES CPU tracing format.
/// Also provides window generation for displaying disassembly around the current PC.
use crate::gb::cpu::opcode;

/// Single disassembled instruction line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GbCpuDisasmLineSnapshot {
    pub addr: u16,
    pub bytes: Vec<u8>,
    pub text: String,
    pub is_current: bool,
}

/// Disassembly window viewport state.
///
/// Tracks the start address of the disassembly window to maintain scroll position
/// when PC moves within the visible window.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GbCpuDisasmWindowState {
    pub(super) start: Option<u16>,
}

/// Configuration for disassembly window display.
///
/// Defines how many lines to show before and after the current PC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisasmWindowConfig {
    pub before: usize,
    pub after: usize,
    pub top_margin: usize,
    pub bottom_margin: usize,
}

impl Default for DisasmWindowConfig {
    fn default() -> Self {
        // Total window height is before + 1 + after.
        // 10 + 1 + 9 = 20 lines (matches NES debugger).
        Self {
            before: 10,
            after: 9,
            top_margin: 3,
            bottom_margin: 3,
        }
    }
}

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
    if result.contains("n8")
        && let Some(&byte) = bytes.get(operand_start)
    {
        result = result.replace("n8", &format!("${:02X}", byte));
    }

    // Replace n16 (16-bit immediate, little-endian) with actual value
    if result.contains("n16") && bytes.len() >= operand_start + 2 {
        let lo = bytes[operand_start];
        let hi = bytes[operand_start + 1];
        let addr = u16::from_le_bytes([lo, hi]);
        result = result.replace("n16", &format!("${:04X}", addr));
    }

    // Replace e8 (signed 8-bit offset) with calculated target address
    if result.contains("e8")
        && let Some(&offset_byte) = bytes.get(operand_start)
    {
        let offset = offset_byte as i8;
        // Target = PC + instruction_length + offset
        // For e8 instructions, length is always 2 bytes
        let target = pc.wrapping_add(2).wrapping_add(offset as i16 as u16);
        result = result.replace("e8", &format!("${:04X}", target));
    }

    result
}

/// Generate a disassembly window centered on the current PC.
///
/// Attempts to show `config.before` lines before PC and `config.after` lines after.
/// Returns a vector of disassembly lines with one marked as `is_current`.
pub fn disassemble_window<F: Fn(u16) -> u8>(
    read: F,
    pc: u16,
    config: DisasmWindowConfig,
) -> Vec<GbCpuDisasmLineSnapshot> {
    let mut start = pc;
    for _ in 0..config.before {
        let Some(prev) = prev_instruction_start(&read, start) else {
            break;
        };
        // Stop if we wrapped around (prev > start indicates wrapping past 0)
        if prev > start {
            break;
        }
        start = prev;
    }

    let target_lines = config.before + 1 + config.after;
    disassemble_from_start(&read, start, pc, target_lines)
}

/// Generate a disassembly window with viewport state tracking.
///
/// Maintains the window start address when PC moves within the visible window.
/// Re-centers when PC jumps outside the window or approaches the bottom.
pub fn disassemble_window_with_state<F: Fn(u16) -> u8>(
    read: F,
    pc: u16,
    state: &mut GbCpuDisasmWindowState,
    config: DisasmWindowConfig,
) -> Vec<GbCpuDisasmLineSnapshot> {
    let target_lines = config.before + 1 + config.after;

    let mut lines = if let Some(start) = state.start {
        disassemble_from_start(&read, start, pc, target_lines)
    } else {
        disassemble_window(&read, pc, config)
    };

    let current_index = lines.iter().position(|l| l.is_current);

    if let Some(idx) = current_index {
        let last_two_start = lines.len().saturating_sub(2);
        if idx >= last_two_start {
            // PC is in last 2 lines - re-center
            lines = disassemble_window(&read, pc, config);
            state.start = lines.first().map(|l| l.addr);
            return lines;
        }

        // Keep the existing start when the current line is safely within the window.
        state.start = lines.first().map(|l| l.addr);
        return lines;
    }

    // PC not found (e.g., jumped). Re-center using the original logic.
    lines = disassemble_window(&read, pc, config);
    state.start = lines.first().map(|l| l.addr);
    lines
}

/// Disassemble instructions starting from a specific address.
fn disassemble_from_start<F: Fn(u16) -> u8>(
    read: &F,
    start: u16,
    pc: u16,
    target_lines: usize,
) -> Vec<GbCpuDisasmLineSnapshot> {
    let mut lines = Vec::with_capacity(target_lines);

    let mut addr = start;
    for _ in 0..target_lines {
        let line = disassemble_one(read, addr, pc);
        let step = (line.bytes.len() as u16).max(1);
        addr = addr.wrapping_add(step);
        lines.push(line);

        if addr == 0 {
            break;
        }
    }

    lines
}

/// Find the start address of the instruction preceding the given PC.
///
/// Uses a 3-2-1 byte lookback strategy for SM83's variable-length instructions.
/// SM83 instructions can be 1, 2, or 3 bytes (CB-prefixed are always 2 bytes).
fn prev_instruction_start<F: Fn(u16) -> u8>(read: &F, pc: u16) -> Option<u16> {
    // Try 3, 2, 1 byte lookback
    for len in (1u16..=3u16).rev() {
        let start = pc.wrapping_sub(len);
        let op = read(start);

        // Check if it's a CB-prefixed instruction
        if op == 0xCB && len >= 2 {
            // CB-prefixed instructions are always 2 bytes
            if len == 2 {
                return Some(start);
            }
        } else {
            // Regular instruction - check length
            let meta = opcode::lookup(op);
            if meta.bytes() as u16 == len {
                return Some(start);
            }
        }
    }

    None
}

/// Disassemble a single instruction at the given address.
fn disassemble_one<F: Fn(u16) -> u8>(read: &F, addr: u16, pc: u16) -> GbCpuDisasmLineSnapshot {
    let op = read(addr);

    // Determine instruction length
    let (len, actual_op) = if op == 0xCB {
        // CB-prefixed instruction (always 2 bytes)
        let cb_op = read(addr.wrapping_add(1));
        (2, cb_op)
    } else {
        // Regular instruction
        let meta = opcode::lookup(op);
        (meta.bytes() as usize, op)
    };

    // Read instruction bytes
    let mut bytes = Vec::with_capacity(len);
    for i in 0..len {
        bytes.push(read(addr.wrapping_add(i as u16)));
    }

    // Format instruction text
    let text = format_instruction(actual_op, addr, &bytes);

    GbCpuDisasmLineSnapshot {
        addr,
        bytes,
        text,
        is_current: addr == pc,
    }
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
