use crate::cpu;

use super::types::{CpuDisasmLineSnapshot, CpuDisasmWindowState};

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
        // Previously 8 + 1 + 8 = 17; doubled to 34.
        Self {
            before: 16,
            after: 17,
            top_margin: 3,
            bottom_margin: 3,
        }
    }
}

pub fn disassemble_window<F: Fn(u16) -> u8>(
    read: F,
    pc: u16,
    config: DisasmWindowConfig,
) -> Vec<CpuDisasmLineSnapshot> {
    let mut start = pc;
    for _ in 0..config.before {
        let Some(prev) = prev_instruction_start(&read, start) else {
            break;
        };
        start = prev;
    }

    let target_lines = config.before + 1 + config.after;
    disassemble_from_start(&read, start, pc, target_lines)
}

pub fn disassemble_window_with_state<F: Fn(u16) -> u8>(
    read: F,
    pc: u16,
    state: &mut CpuDisasmWindowState,
    config: DisasmWindowConfig,
) -> Vec<CpuDisasmLineSnapshot> {
    let target_lines = config.before + 1 + config.after;
    let bottom_trigger_index = target_lines.saturating_sub(1 + config.bottom_margin);

    let mut lines = if let Some(start) = state.start {
        disassemble_from_start(&read, start, pc, target_lines)
    } else {
        disassemble_window(&read, pc, config)
    };

    let current_index = lines.iter().position(|l| l.is_current);

    if let Some(idx) = current_index {
        if idx >= bottom_trigger_index {
            let desired_start_idx = idx.saturating_sub(config.top_margin);
            if let Some(new_start) = lines.get(desired_start_idx).map(|l| l.addr) {
                lines = disassemble_from_start(&read, new_start, pc, target_lines);
                state.start = Some(new_start);
                return lines;
            }
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

fn disassemble_from_start<F: Fn(u16) -> u8>(
    read: &F,
    start: u16,
    pc: u16,
    target_lines: usize,
) -> Vec<CpuDisasmLineSnapshot> {
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

fn prev_instruction_start<F: Fn(u16) -> u8>(read: &F, pc: u16) -> Option<u16> {
    for len in (1u16..=3u16).rev() {
        let start = pc.wrapping_sub(len);
        let op = read(start);
        let Some(meta) = cpu::lookup(op) else {
            continue;
        };

        if meta.bytes() as u16 == len {
            return Some(start);
        }
    }

    None
}

fn disassemble_one<F: Fn(u16) -> u8>(read: &F, addr: u16, pc: u16) -> CpuDisasmLineSnapshot {
    let op = read(addr);
    let meta = cpu::lookup(op);
    let len = meta.map(|m| m.bytes()).unwrap_or(1) as usize;

    let mut bytes = Vec::with_capacity(len);
    for i in 0..len {
        bytes.push(read(addr.wrapping_add(i as u16)));
    }

    let text = if let Some(meta) = meta {
        format_instruction(meta, addr, &bytes)
    } else {
        "???".to_string()
    };

    CpuDisasmLineSnapshot {
        addr,
        bytes,
        text,
        is_current: addr == pc,
    }
}

fn format_instruction(meta: &cpu::OpCode, addr: u16, bytes: &[u8]) -> String {
    let operand = match meta.mode {
        "IMP" => String::new(),
        "ACC" => "A".to_string(),
        "IMM" => format!("#${:02X}", bytes.get(1).copied().unwrap_or(0)),
        "ZP" => format!("${:02X}", bytes.get(1).copied().unwrap_or(0)),
        "ZPX" => format!("${:02X},X", bytes.get(1).copied().unwrap_or(0)),
        "ZPY" => format!("${:02X},Y", bytes.get(1).copied().unwrap_or(0)),
        "INDX" => format!("(${:02X},X)", bytes.get(1).copied().unwrap_or(0)),
        "INDY" | "INDYW" => format!("(${:02X}),Y", bytes.get(1).copied().unwrap_or(0)),
        "REL" => {
            let off = bytes.get(1).copied().unwrap_or(0) as i8;
            let next = addr.wrapping_add(2);
            let target = next.wrapping_add(off as i16 as u16);
            format!("${:04X}", target)
        }
        "ABS" => {
            let lo = bytes.get(1).copied().unwrap_or(0);
            let hi = bytes.get(2).copied().unwrap_or(0);
            format!("${:04X}", u16::from_le_bytes([lo, hi]))
        }
        "ABSX" | "ABSXW" => {
            let lo = bytes.get(1).copied().unwrap_or(0);
            let hi = bytes.get(2).copied().unwrap_or(0);
            format!("${:04X},X", u16::from_le_bytes([lo, hi]))
        }
        "ABSY" | "ABSYW" => {
            let lo = bytes.get(1).copied().unwrap_or(0);
            let hi = bytes.get(2).copied().unwrap_or(0);
            format!("${:04X},Y", u16::from_le_bytes([lo, hi]))
        }
        "IND" => {
            let lo = bytes.get(1).copied().unwrap_or(0);
            let hi = bytes.get(2).copied().unwrap_or(0);
            format!("(${:04X})", u16::from_le_bytes([lo, hi]))
        }
        _ => String::new(),
    };

    if operand.is_empty() {
        meta.mnemonic.to_string()
    } else {
        format!("{} {}", meta.mnemonic, operand)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disasm_window_scrolls_when_current_reaches_bottom_margin() {
        // Use a memory model of all NOPs (0xEA = 1 byte) so instruction boundaries are trivial.
        let read = |_addr: u16| 0xEA;

        let config = DisasmWindowConfig {
            before: 8,
            after: 8,
            top_margin: 3,
            bottom_margin: 3,
        };

        let mut state = CpuDisasmWindowState::default();

        // Initial view is centered around pc (current at index BEFORE).
        let base_pc = 0xC000;
        let lines0 = disassemble_window_with_state(read, base_pc, &mut state, config);
        let idx0 = lines0.iter().position(|l| l.is_current).unwrap();
        assert_eq!(idx0, config.before);

        // Step forward until current is at the bottom trigger index.
        let target_lines = config.before + 1 + config.after;
        let bottom_trigger_index = target_lines - 1 - config.bottom_margin;
        let pc_at_trigger = base_pc.wrapping_add((bottom_trigger_index - idx0) as u16);

        let lines1 = disassemble_window_with_state(read, pc_at_trigger, &mut state, config);
        let idx1 = lines1.iter().position(|l| l.is_current).unwrap();
        assert_eq!(idx1, config.top_margin);
    }
}
