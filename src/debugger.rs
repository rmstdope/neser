use crate::cpu;
use crate::nes::Nes;

// Disassembly window size:
// Total window height is N_BEFORE + 1 + N_AFTER.
// Previously 8 + 1 + 8 = 17; doubled to 34.
const DISASM_N_BEFORE: usize = 16;
const DISASM_N_AFTER: usize = 17;

// When stepping forward and the current line reaches the bottom margin,
// scroll so it is TOP_MARGIN lines from the top.
const DISASM_TOP_MARGIN: usize = 3;
const DISASM_BOTTOM_MARGIN: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuRegsSnapshot {
    pub pc: u16,
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub sp: u8,
    pub p: u8,
    pub cycles: u64,
    pub interrupt: Option<crate::cpu::InterruptKind>,
    pub nmi_vector: u16,
    pub irq_vector: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuDisasmLineSnapshot {
    pub addr: u16,
    pub bytes: Vec<u8>,
    pub text: String,
    pub is_current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebuggerSnapshot {
    pub cpu_regs: CpuRegsSnapshot,
    pub prg_hexdump_base: u16,
    pub prg_hexdump_bytes: Vec<u8>,
    pub cpu_disasm: Vec<CpuDisasmLineSnapshot>,
    pub cpu: String,
    pub ppu: String,
    pub apu: String,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CpuDisasmWindowState {
    start: Option<u16>,
}

fn prg_hexdump_base_from_pc(pc: u16) -> u16 {
    let centered = pc & 0xFFF0;
    let mut prg_hexdump_base = centered.saturating_sub(0x80).max(0x8000);
    // Ensure base+0xFF stays within u16.
    prg_hexdump_base = prg_hexdump_base.min(0xFF00);
    prg_hexdump_base
}

fn read_vectors_for_snapshot(nes: &Nes) -> (u16, u16) {
    let memory = nes.memory.borrow();
    let nmi_lo = memory.read_cpu_for_debugger(0xFFFA) as u16;
    let nmi_hi = memory.read_cpu_for_debugger(0xFFFB) as u16;
    let irq_lo = memory.read_cpu_for_debugger(0xFFFE) as u16;
    let irq_hi = memory.read_cpu_for_debugger(0xFFFF) as u16;
    ((nmi_hi << 8) | nmi_lo, (irq_hi << 8) | irq_lo)
}

fn build_snapshot(nes: &Nes, cpu_disasm: Vec<CpuDisasmLineSnapshot>) -> DebuggerSnapshot {
    let cpu_cycles = nes.cpu.get_total_cycles();

    let pc = nes.cpu.pc;

    let prg_hexdump_base = prg_hexdump_base_from_pc(pc);

    let prg_hexdump_bytes = {
        let memory = nes.memory.borrow();
        (0u16..=0x00FF)
            .map(|offset| memory.read_prg_rom_for_debugger(prg_hexdump_base + offset))
            .collect::<Vec<u8>>()
    };

    let (nmi_vector, irq_vector) = read_vectors_for_snapshot(nes);

    let cpu_regs = CpuRegsSnapshot {
        pc: nes.cpu.pc,
        a: nes.cpu.a,
        x: nes.cpu.x,
        y: nes.cpu.y,
        sp: nes.cpu.sp,
        p: nes.cpu.p,
        cycles: cpu_cycles,
        interrupt: nes.cpu.current_interrupt(),
        nmi_vector,
        irq_vector,
    };

    let cpu = format!(
        "CPU\n\
PC: {pc:04X}  A: {a:02X} X: {x:02X} Y: {y:02X}  SP: {sp:02X}  P: {p:02X}\n\
CYC: {cycles}",
        pc = nes.cpu.pc,
        a = nes.cpu.a,
        x = nes.cpu.x,
        y = nes.cpu.y,
        sp = nes.cpu.sp,
        p = nes.cpu.p,
        cycles = cpu_cycles,
    );

    let (scanline, pixel) = {
        let ppu = nes.ppu.borrow();
        (ppu.scanline(), ppu.pixel())
    };

    let ppu = format!(
        "PPU\n\
scanline: {scanline:3}  pixel: {pixel:3}",
        scanline = scanline,
        pixel = pixel
    );

    let (apu_cycle, frame_counter_cycle) = {
        let apu = nes.apu.borrow();
        (apu.apu_cycle(), apu.debug_frame_counter_cycle())
    };

    let apu = format!(
        "APU\n\
apu_cycle: {apu_cycle}  frame_counter_cycle: {frame_counter_cycle}",
        apu_cycle = apu_cycle,
        frame_counter_cycle = frame_counter_cycle
    );

    DebuggerSnapshot {
        cpu_regs,
        prg_hexdump_base,
        prg_hexdump_bytes,
        cpu_disasm,
        cpu,
        ppu,
        apu,
    }
}

pub fn snapshot(nes: &Nes) -> DebuggerSnapshot {
    let cpu_disasm = {
        let memory = nes.memory.borrow();
        disassemble_window(
            |addr| memory.read_cpu_for_debugger(addr),
            nes.cpu.pc,
            DISASM_N_BEFORE,
            DISASM_N_AFTER,
        )
    };
    build_snapshot(nes, cpu_disasm)
}

pub fn snapshot_with_disasm_state(nes: &Nes, state: &mut CpuDisasmWindowState) -> DebuggerSnapshot {
    let cpu_disasm = {
        let memory = nes.memory.borrow();
        disassemble_window_with_state(
            |addr| memory.read_cpu_for_debugger(addr),
            nes.cpu.pc,
            state,
            DISASM_N_BEFORE,
            DISASM_N_AFTER,
            DISASM_TOP_MARGIN,
            DISASM_BOTTOM_MARGIN,
        )
    };
    build_snapshot(nes, cpu_disasm)
}

fn disassemble_window<F: Fn(u16) -> u8>(
    read: F,
    pc: u16,
    before: usize,
    after: usize,
) -> Vec<CpuDisasmLineSnapshot> {
    let mut start = pc;
    for _ in 0..before {
        let Some(prev) = prev_instruction_start(&read, start) else {
            break;
        };
        start = prev;
    }

    let target_lines = before + 1 + after;
    disassemble_from_start(&read, start, pc, target_lines)
}

fn disassemble_window_with_state<F: Fn(u16) -> u8>(
    read: F,
    pc: u16,
    state: &mut CpuDisasmWindowState,
    before: usize,
    after: usize,
    top_margin: usize,
    bottom_margin: usize,
) -> Vec<CpuDisasmLineSnapshot> {
    let target_lines = before + 1 + after;
    let bottom_trigger_index = target_lines.saturating_sub(1 + bottom_margin);

    let mut lines = if let Some(start) = state.start {
        disassemble_from_start(&read, start, pc, target_lines)
    } else {
        disassemble_window(&read, pc, before, after)
    };

    let current_index = lines.iter().position(|l| l.is_current);

    if let Some(idx) = current_index {
        if idx >= bottom_trigger_index {
            let desired_start_idx = idx.saturating_sub(top_margin);
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
    lines = disassemble_window(&read, pc, before, after);
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
        format!("???")
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
    use crate::cartridge::Cartridge;
    use crate::cartridge::MirroringMode;
    use crate::nes::{Nes, TvSystem};

    #[test]
    fn test_snapshot_contains_basic_cpu_ppu_apu_info() {
        let mut nes = Nes::new(TvSystem::Ntsc);

        // Insert a cartridge so PRG hexdump can be generated.
        let mut prg_rom = vec![0u8; 32 * 1024];
        prg_rom[0] = 0x00;
        prg_rom[1] = 0x01;
        prg_rom[2] = 0x02;
        prg_rom[3] = 0x03;

        // Seed interrupt vectors in the PRG ROM image.
        // For a 32 KiB NROM mapping, CPU $8000-$FFFF maps to prg_rom[0x0000..0x8000).
        // So vectors at $FFFA/$FFFE correspond to the last bytes of PRG ROM.
        let nmi_vector = 0x1234u16;
        let irq_vector = 0xABCDu16;
        let [nmi_lo, nmi_hi] = nmi_vector.to_le_bytes();
        let [irq_lo, irq_hi] = irq_vector.to_le_bytes();
        prg_rom[0x7FFA] = nmi_lo;
        prg_rom[0x7FFB] = nmi_hi;
        prg_rom[0x7FFE] = irq_lo;
        prg_rom[0x7FFF] = irq_hi;

        let cartridge = Cartridge::from_parts(prg_rom, vec![], MirroringMode::Horizontal);
        nes.insert_cartridge(cartridge);

        // Seed a couple of CPU registers so the snapshot has something meaningful.
        nes.cpu.pc = 0xC000;
        nes.cpu.a = 0x12;
        nes.cpu.x = 0x34;
        nes.cpu.y = 0x56;
        nes.cpu.sp = 0xFD;
        nes.cpu.p = 0x24;

        let snap = snapshot(&nes);

        assert!(snap.cpu.contains("PC"));
        assert!(snap.cpu.contains("A"));
        assert!(snap.cpu.contains("X"));
        assert!(snap.cpu.contains("Y"));
        assert!(snap.cpu.contains("SP"));
        assert!(snap.cpu.contains("P"));

        assert!(snap.ppu.contains("scanline"));
        assert!(snap.ppu.contains("pixel"));

        assert!(snap.apu.contains("apu_cycle"));

        assert_eq!(snap.cpu_regs.pc, 0xC000);
        assert_eq!(snap.cpu_regs.a, 0x12);
        assert_eq!(snap.cpu_regs.x, 0x34);
        assert_eq!(snap.cpu_regs.y, 0x56);
        assert_eq!(snap.cpu_regs.sp, 0xFD);
        assert_eq!(snap.cpu_regs.p, 0x24);

        assert_eq!(snap.cpu_regs.nmi_vector, nmi_vector);
        assert_eq!(snap.cpu_regs.irq_vector, irq_vector);

        assert!(snap.prg_hexdump_base >= 0x8000);
        assert_eq!(snap.prg_hexdump_bytes.len(), 0x100);
    }

    #[test]
    fn test_snapshot_includes_disassembly_around_pc() {
        let mut nes = Nes::new(TvSystem::Ntsc);

        let mut prg_rom = vec![0u8; 32 * 1024];
        // $8000: LDA #$01; TAX; INX; BRK
        prg_rom[0x0000] = 0xA9;
        prg_rom[0x0001] = 0x01;
        prg_rom[0x0002] = 0xAA;
        prg_rom[0x0003] = 0xE8;
        prg_rom[0x0004] = 0x00;
        let cartridge = Cartridge::from_parts(prg_rom, vec![], MirroringMode::Horizontal);
        nes.insert_cartridge(cartridge);

        nes.cpu.pc = 0x8000;

        let snap = snapshot(&nes);

        // Disassembly window is expected to be a fixed-size viewport.
        assert_eq!(snap.cpu_disasm.len(), 34);

        assert!(snap.cpu_disasm.iter().any(|l| l.addr == 0x8000
            && l.text.contains("LDA")
            && l.text.contains("#$01")
            && l.is_current));
        assert!(snap.cpu_disasm.iter().any(|l| l.text.contains("TAX")));
        assert!(snap.cpu_disasm.iter().any(|l| l.text.contains("INX")));
        assert!(snap.cpu_disasm.iter().any(|l| l.text.contains("BRK")));
    }

    #[test]
    fn test_disasm_window_scrolls_when_current_reaches_bottom_margin() {
        // Use a memory model of all NOPs (0xEA = 1 byte) so instruction boundaries are trivial.
        let read = |_addr: u16| 0xEA;

        const BEFORE: usize = 8;
        const AFTER: usize = 8;
        const TOP_MARGIN: usize = 3;
        const BOTTOM_MARGIN: usize = 3;

        let mut state = CpuDisasmWindowState::default();

        // Initial view is centered around pc (current at index BEFORE).
        let base_pc = 0xC000;
        let lines0 = disassemble_window_with_state(
            read,
            base_pc,
            &mut state,
            BEFORE,
            AFTER,
            TOP_MARGIN,
            BOTTOM_MARGIN,
        );
        let idx0 = lines0.iter().position(|l| l.is_current).unwrap();
        assert_eq!(idx0, BEFORE);

        // Step forward until current is at the bottom trigger index.
        let target_lines = BEFORE + 1 + AFTER;
        let bottom_trigger_index = target_lines - 1 - BOTTOM_MARGIN;
        let pc_at_trigger = base_pc.wrapping_add((bottom_trigger_index - idx0) as u16);

        let lines1 = disassemble_window_with_state(
            read,
            pc_at_trigger,
            &mut state,
            BEFORE,
            AFTER,
            TOP_MARGIN,
            BOTTOM_MARGIN,
        );
        let idx1 = lines1.iter().position(|l| l.is_current).unwrap();
        assert_eq!(idx1, TOP_MARGIN);
    }
}
