use crate::console::Nes;

use super::disasm::{DisasmWindowConfig, disassemble_window, disassemble_window_with_state};
use super::types::{
    CpuDisasmLineSnapshot, CpuDisasmWindowState, CpuRegsSnapshot, DebuggerSnapshot,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DebuggerViewState {
    cpu_disasm: CpuDisasmWindowState,
    show_ppu_viewer: bool,
    prg_hexdump_base: Option<u16>,
}

impl DebuggerViewState {
    pub fn snapshot(&mut self, nes: &Nes) -> DebuggerSnapshot {
        if let Some(base) = self.prg_hexdump_base {
            snapshot_impl(
                nes,
                Some(&mut self.cpu_disasm),
                DisasmWindowConfig::default(),
                Some(base),
            )
        } else {
            snapshot_with_disasm_state(nes, &mut self.cpu_disasm)
        }
    }

    pub fn toggle_ppu_viewer(&mut self) {
        self.show_ppu_viewer = !self.show_ppu_viewer;
    }

    pub fn is_ppu_viewer_visible(&self) -> bool {
        self.show_ppu_viewer
    }

    pub fn set_prg_hexdump_base(&mut self, base: u16) {
        self.prg_hexdump_base = Some(normalize_prg_hexdump_base(base));
    }

    pub fn nudge_prg_hexdump_base_by_bytes_from(&mut self, visible_base: u16, delta: i16) {
        let current = self.prg_hexdump_base.unwrap_or(visible_base);
        let nudged = if delta >= 0 {
            current.saturating_add(delta as u16)
        } else {
            current.saturating_sub((-delta) as u16)
        };
        self.prg_hexdump_base = Some(normalize_prg_hexdump_base(nudged));
    }

    #[cfg(test)]
    pub fn prg_hexdump_base(&self) -> Option<u16> {
        self.prg_hexdump_base
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Debugger {
    disasm: DisasmWindowConfig,
}

impl Debugger {
    pub fn snapshot(&self, nes: &Nes) -> DebuggerSnapshot {
        snapshot_impl(nes, None, self.disasm, None)
    }

    pub fn snapshot_with_disasm_state(
        &self,
        nes: &Nes,
        state: &mut CpuDisasmWindowState,
    ) -> DebuggerSnapshot {
        snapshot_impl(nes, Some(state), self.disasm, None)
    }
}

fn prg_hexdump_base_from_pc(pc: u16) -> u16 {
    let centered = pc & 0xFFF0;
    let mut prg_hexdump_base = centered.saturating_sub(0x80).max(0x8000);
    // Ensure base+0xFF stays within u16.
    prg_hexdump_base = prg_hexdump_base.min(0xFF00);
    prg_hexdump_base
}

fn normalize_prg_hexdump_base(base: u16) -> u16 {
    let aligned = base & 0xFFF0;
    aligned.clamp(0x8000, 0xFF00)
}

fn read_vectors_for_snapshot(nes: &Nes) -> (u16, u16) {
    let memory = nes.bus.borrow();
    let nmi_lo = memory.read_cpu_for_debugger(0xFFFA) as u16;
    let nmi_hi = memory.read_cpu_for_debugger(0xFFFB) as u16;
    let irq_lo = memory.read_cpu_for_debugger(0xFFFE) as u16;
    let irq_hi = memory.read_cpu_for_debugger(0xFFFF) as u16;
    ((nmi_hi << 8) | nmi_lo, (irq_hi << 8) | irq_lo)
}

fn build_snapshot(
    nes: &Nes,
    cpu_disasm: Vec<CpuDisasmLineSnapshot>,
    prg_hexdump_base_override: Option<u16>,
) -> DebuggerSnapshot {
    let cpu_cycles = nes.cpu.get_total_cycles();
    let pc = nes.cpu.pc();

    let prg_hexdump_base = prg_hexdump_base_override
        .map(normalize_prg_hexdump_base)
        .unwrap_or_else(|| prg_hexdump_base_from_pc(pc));

    let prg_hexdump_bytes = {
        let memory = nes.bus.borrow();
        (0u16..=0x00FF)
            .map(|offset| memory.read_prg_rom_for_debugger(prg_hexdump_base + offset))
            .collect::<Vec<u8>>()
    };

    let (nmi_vector, irq_vector) = read_vectors_for_snapshot(nes);

    let (frame_count, scanline, pixel) = {
        let ppu = nes.ppu.borrow();
        (ppu.frame_count(), ppu.scanline(), ppu.pixel())
    };

    let cpu_regs = CpuRegsSnapshot {
        pc,
        a: nes.cpu.a(),
        x: nes.cpu.x(),
        y: nes.cpu.y(),
        sp: nes.cpu.sp(),
        p: nes.cpu.p(),
        cycles: cpu_cycles,
        frame_count,
        scanline,
        pixel,
        interrupt: nes.cpu.current_interrupt(),
        nmi_vector,
        irq_vector,
    };

    let cpu = format!(
        "CPU\n\
PC: {pc:04X}  A: {a:02X} X: {x:02X} Y: {y:02X}  SP: {sp:02X}  P: {p:02X}\n\
CYC: {cycles}",
        pc = nes.cpu.pc(),
        a = nes.cpu.a(),
        x = nes.cpu.x(),
        y = nes.cpu.y(),
        sp = nes.cpu.sp(),
        p = nes.cpu.p(),
        cycles = cpu_cycles,
    );

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

fn snapshot_impl(
    nes: &Nes,
    state: Option<&mut CpuDisasmWindowState>,
    disasm_config: DisasmWindowConfig,
    prg_hexdump_base_override: Option<u16>,
) -> DebuggerSnapshot {
    let cpu_disasm = {
        let memory = nes.bus.borrow();
        match state {
            Some(state) => disassemble_window_with_state(
                |addr| memory.read_cpu_for_debugger(addr),
                nes.cpu.pc(),
                state,
                disasm_config,
            ),
            None => disassemble_window(
                |addr| memory.read_cpu_for_debugger(addr),
                nes.cpu.pc(),
                disasm_config,
            ),
        }
    };

    build_snapshot(nes, cpu_disasm, prg_hexdump_base_override)
}

pub fn snapshot(nes: &Nes) -> DebuggerSnapshot {
    Debugger::default().snapshot(nes)
}

pub fn snapshot_with_disasm_state(nes: &Nes, state: &mut CpuDisasmWindowState) -> DebuggerSnapshot {
    Debugger::default().snapshot_with_disasm_state(nes, state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::Cartridge;
    use crate::cartridge::NametableLayout;
    use crate::console::{Config, Nes};

    #[test]
    fn test_snapshot_contains_basic_cpu_ppu_apu_info() {
        let mut nes = Nes::new(Config::default());

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

        let cartridge = Cartridge::from_parts(prg_rom, vec![], NametableLayout::Horizontal);
        nes.insert_cartridge(cartridge);

        // Seed a couple of CPU registers so the snapshot has something meaningful.
        nes.cpu.set_pc(0xC000);
        nes.cpu.set_a_register(0x12);
        nes.cpu.set_x(0x34);
        nes.cpu.set_y(0x56);
        nes.cpu.set_sp(0xFD);
        nes.cpu.set_p(0x24);

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
        let mut nes = Nes::new(Config::default());

        let mut prg_rom = vec![0u8; 32 * 1024];
        // $8000: LDA #$01; TAX; INX; BRK
        prg_rom[0x0000] = 0xA9;
        prg_rom[0x0001] = 0x01;
        prg_rom[0x0002] = 0xAA;
        prg_rom[0x0003] = 0xE8;
        prg_rom[0x0004] = 0x00;
        let cartridge = Cartridge::from_parts(prg_rom, vec![], NametableLayout::Horizontal);
        nes.insert_cartridge(cartridge);
        nes.cpu.set_pc(0x8000);

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
    fn test_debugger_view_state_prg_hexdump_base_set_and_step_by_16() {
        let mut state = DebuggerViewState::default();

        state.set_prg_hexdump_base(0xC120);
        assert_eq!(state.prg_hexdump_base(), Some(0xC120));

        state.nudge_prg_hexdump_base_by_bytes_from(0x8000, 16);
        assert_eq!(state.prg_hexdump_base(), Some(0xC130));

        state.nudge_prg_hexdump_base_by_bytes_from(0x8000, -16);
        assert_eq!(state.prg_hexdump_base(), Some(0xC120));
    }

    #[test]
    fn test_first_hexdump_nudge_uses_visible_base_not_default_8000() {
        let mut state = DebuggerViewState::default();

        state.nudge_prg_hexdump_base_by_bytes_from(0xC000, 16);
        assert_eq!(state.prg_hexdump_base(), Some(0xC010));
    }
}
