//! Generic disassembly window generation for debuggers.
//!
//! This module provides the shared disassembly window logic used by both
//! GB and NES debuggers. Platform-specific instruction decoding remains
//! in the respective platform modules.

/// Configuration for disassembly window display.
///
/// Defines how many lines to show before and after the current PC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisasmWindowConfig {
    /// Number of lines to show before the current PC.
    pub before: usize,
    /// Number of lines to show after the current PC.
    pub after: usize,
    /// Margin from top before re-centering (not currently used).
    pub top_margin: usize,
    /// Margin from bottom before re-centering (not currently used).
    pub bottom_margin: usize,
}

impl Default for DisasmWindowConfig {
    fn default() -> Self {
        // Total window height is before + 1 + after.
        // 10 + 1 + 9 = 20 lines.
        Self {
            before: 10,
            after: 9,
            top_margin: 3,
            bottom_margin: 3,
        }
    }
}

/// Disassembly window viewport state.
///
/// Tracks the start address of the disassembly window to maintain scroll position
/// when PC moves within the visible window.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DisasmWindowState {
    /// Start address of the current disassembly window, if set.
    pub(crate) start: Option<u16>,
}

impl DisasmWindowState {
    /// Create a new empty window state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset the window state (forces re-centering on next call).
    pub fn reset(&mut self) {
        self.start = None;
    }
}

/// A single disassembled instruction line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisasmLine {
    /// Address of this instruction.
    pub addr: u16,
    /// Raw instruction bytes.
    pub bytes: Vec<u8>,
    /// Formatted instruction text.
    pub text: String,
    /// Whether this is the current instruction (at PC).
    pub is_current: bool,
}

impl DisasmLine {
    /// Create a new disassembly line.
    pub fn new(addr: u16, bytes: Vec<u8>, text: String, is_current: bool) -> Self {
        Self {
            addr,
            bytes,
            text,
            is_current,
        }
    }
}

/// Trait for platform-specific instruction decoding.
///
/// Implemented by GB and NES to provide instruction length lookup
/// for disassembly window generation.
pub trait InstructionDecoder {
    /// Get the length of the instruction at the given opcode.
    fn instruction_length(opcode: u8) -> usize;

    /// Disassemble a single instruction at the given address.
    /// Returns the address, bytes, and formatted text.
    fn disassemble_one<F: Fn(u16) -> u8>(read: &F, addr: u16) -> (usize, String);
}

/// Generate a disassembly window centered on the current PC.
///
/// Attempts to show `config.before` lines before PC and `config.after` lines after.
/// Returns a vector of disassembly lines with one marked as `is_current`.
///
/// # Arguments
/// * `read` - Function to read a byte from memory
/// * `pc` - Current program counter
/// * `config` - Window configuration
/// * `prev_start` - Function to find the previous instruction start
/// * `disasm_one` - Function to disassemble a single instruction
pub fn disassemble_window<F, P, D>(
    read: F,
    pc: u16,
    config: DisasmWindowConfig,
    prev_start: P,
    disasm_one: D,
) -> Vec<DisasmLine>
where
    F: Fn(u16) -> u8,
    P: Fn(&dyn Fn(u16) -> u8, u16) -> Option<u16>,
    D: Fn(&dyn Fn(u16) -> u8, u16, u16) -> DisasmLine,
{
    let mut start = pc;
    for _ in 0..config.before {
        let Some(prev) = prev_start(&read, start) else {
            break;
        };
        // Stop if we wrapped around (prev > start indicates wrapping past 0)
        if prev > start {
            break;
        }
        start = prev;
    }

    let target_lines = config.before + 1 + config.after;
    disassemble_from_start(&read, start, pc, target_lines, &disasm_one)
}

/// Generate a disassembly window with viewport state tracking.
///
/// Maintains the window start address when PC moves within the visible window.
/// Re-centers when PC jumps outside the window or approaches the bottom.
pub fn disassemble_window_with_state<F, P, D>(
    read: F,
    pc: u16,
    state: &mut DisasmWindowState,
    config: DisasmWindowConfig,
    prev_start: P,
    disasm_one: D,
) -> Vec<DisasmLine>
where
    F: Fn(u16) -> u8,
    P: Fn(&dyn Fn(u16) -> u8, u16) -> Option<u16>,
    D: Fn(&dyn Fn(u16) -> u8, u16, u16) -> DisasmLine,
{
    let target_lines = config.before + 1 + config.after;

    let mut lines = if let Some(start) = state.start {
        disassemble_from_start(&read, start, pc, target_lines, &disasm_one)
    } else {
        disassemble_window(&read, pc, config, &prev_start, &disasm_one)
    };

    let current_index = lines.iter().position(|l| l.is_current);

    if let Some(idx) = current_index {
        let last_two_start = lines.len().saturating_sub(2);
        if idx >= last_two_start {
            // PC is in last 2 lines - re-center
            lines = disassemble_window(&read, pc, config, &prev_start, &disasm_one);
            state.start = lines.first().map(|l| l.addr);
            return lines;
        }

        // Keep the existing start when the current line is safely within the window.
        state.start = lines.first().map(|l| l.addr);
        return lines;
    }

    // PC not found (e.g., jumped). Re-center using the original logic.
    lines = disassemble_window(&read, pc, config, &prev_start, &disasm_one);
    state.start = lines.first().map(|l| l.addr);
    lines
}

/// Disassemble instructions starting from a specific address.
fn disassemble_from_start<F, D>(
    read: &F,
    start: u16,
    pc: u16,
    target_lines: usize,
    disasm_one: &D,
) -> Vec<DisasmLine>
where
    F: Fn(u16) -> u8,
    D: Fn(&dyn Fn(u16) -> u8, u16, u16) -> DisasmLine,
{
    let mut lines = Vec::with_capacity(target_lines);

    let mut addr = start;
    for _ in 0..target_lines {
        let line = disasm_one(read, addr, pc);
        let step = (line.bytes.len() as u16).max(1);
        addr = addr.wrapping_add(step);
        lines.push(line);

        if addr == 0 {
            break;
        }
    }

    lines
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
    fn test_disasm_window_state_reset() {
        let mut state = DisasmWindowState {
            start: Some(0x8000),
        };
        state.reset();
        assert_eq!(state.start, None);
    }

    fn mock_disasm_one(_read: &dyn Fn(u16) -> u8, addr: u16, pc: u16) -> DisasmLine {
        DisasmLine {
            addr,
            bytes: vec![0xEA], // NOP
            text: "NOP".to_string(),
            is_current: addr == pc,
        }
    }

    fn mock_prev_start(_read: &dyn Fn(u16) -> u8, pc: u16) -> Option<u16> {
        if pc == 0 { None } else { Some(pc - 1) }
    }

    #[test]
    fn test_disassemble_window_centers_on_pc() {
        let read = |_addr: u16| 0xEA;

        let config = DisasmWindowConfig {
            before: 4,
            after: 4,
            top_margin: 3,
            bottom_margin: 3,
        };

        let lines = disassemble_window(read, 0xC004, config, mock_prev_start, mock_disasm_one);

        assert_eq!(lines.len(), 9); // before + 1 + after

        // Find current line
        let current_idx = lines.iter().position(|l| l.is_current).unwrap();
        assert_eq!(current_idx, 4); // Should be in the middle (index 4 for before=4)
        assert_eq!(lines[current_idx].addr, 0xC004);
    }

    #[test]
    fn test_disassemble_window_with_state_recenters_when_pc_at_bottom() {
        let read = |_addr: u16| 0xEA;

        let config = DisasmWindowConfig {
            before: 4,
            after: 4,
            top_margin: 3,
            bottom_margin: 3,
        };

        let mut state = DisasmWindowState::default();

        // Initial view
        let lines0 = disassemble_window_with_state(
            read,
            0xC004,
            &mut state,
            config,
            mock_prev_start,
            mock_disasm_one,
        );
        let idx0 = lines0.iter().position(|l| l.is_current).unwrap();
        assert_eq!(idx0, config.before);

        // Move PC to second-to-last row - should recenter
        let lines1 = disassemble_window_with_state(
            read,
            0xC00B,
            &mut state,
            config,
            mock_prev_start,
            mock_disasm_one,
        );
        let idx1 = lines1.iter().position(|l| l.is_current).unwrap();
        assert_eq!(idx1, config.before); // Should be recentered
    }
}
