//! Game Boy debugger snapshot generation.
//!
//! Generates immutable snapshots of emulator state for display in the debugger UI.

use crate::gb::bus::GbBus;
use crate::gb::console::Gb;

use super::disasm::{
    DisasmWindowConfig, GbCpuDisasmLineSnapshot, GbCpuDisasmWindowState, disassemble_window,
    disassemble_window_with_state,
};
use super::types::{
    GbCpuRegsSnapshot, GbCpuTraceLineSnapshot, GbDebuggerSnapshot, GbMemoryWatchEntrySnapshot,
};

/// UI navigation state for the GB debugger.
///
/// Tracks viewport positions and user preferences that persist across frames.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GbDebuggerViewState {
    cpu_disasm: GbCpuDisasmWindowState,
    show_ppu_viewer: bool,
    wram_hexdump_base: Option<u16>,
    vram_hexdump_base: Option<u16>,
    watch_addresses: Vec<u16>,
}

impl GbDebuggerViewState {
    /// Generate a debugger snapshot using current view state.
    pub fn snapshot<B: GbBus>(&mut self, gb: &Gb<B>) -> GbDebuggerSnapshot {
        snapshot_impl(
            gb,
            Some(&mut self.cpu_disasm),
            DisasmWindowConfig::default(),
            self.wram_hexdump_base,
            self.vram_hexdump_base,
            &self.watch_addresses,
        )
    }

    /// Toggle PPU viewer window visibility.
    pub fn toggle_ppu_viewer(&mut self) {
        self.show_ppu_viewer = !self.show_ppu_viewer;
    }

    /// Check if PPU viewer is visible.
    pub fn is_ppu_viewer_visible(&self) -> bool {
        self.show_ppu_viewer
    }

    /// Set WRAM hexdump base address.
    pub fn set_wram_hexdump_base(&mut self, base: u16) {
        self.wram_hexdump_base = Some(normalize_wram_hexdump_base(base));
    }

    /// Adjust WRAM hexdump base address by byte offset.
    pub fn nudge_wram_hexdump_base_by_bytes_from(&mut self, visible_base: u16, delta: i16) {
        let current = self.wram_hexdump_base.unwrap_or(visible_base);
        let nudged = if delta >= 0 {
            current.saturating_add(delta as u16)
        } else {
            current.saturating_sub((-delta) as u16)
        };
        self.wram_hexdump_base = Some(normalize_wram_hexdump_base(nudged));
    }

    /// Set VRAM hexdump base address.
    pub fn set_vram_hexdump_base(&mut self, base: u16) {
        self.vram_hexdump_base = Some(normalize_vram_hexdump_base(base));
    }

    /// Adjust VRAM hexdump base address by byte offset.
    pub fn nudge_vram_hexdump_base_by_bytes_from(&mut self, visible_base: u16, delta: i16) {
        let current = self.vram_hexdump_base.unwrap_or(visible_base);
        let nudged = if delta >= 0 {
            current.saturating_add(delta as u16)
        } else {
            current.saturating_sub((-delta) as u16)
        };
        self.vram_hexdump_base = Some(normalize_vram_hexdump_base(nudged));
    }

    /// Clear all memory watch addresses.
    pub fn clear_watch_addresses(&mut self) {
        self.watch_addresses.clear();
    }

    /// Add a new memory watch address.
    pub fn add_watch_address(&mut self, address: u16) {
        if !self.watch_addresses.contains(&address) {
            self.watch_addresses.push(address);
        }
    }

    /// Remove a memory watch address by index.
    pub fn remove_watch_address(&mut self, index: usize) {
        if index < self.watch_addresses.len() {
            self.watch_addresses.remove(index);
        }
    }

    /// Update a memory watch address at index.
    pub fn update_watch_address(&mut self, index: usize, address: u16) {
        if index < self.watch_addresses.len() {
            self.watch_addresses[index] = address;
            // Enforce uniqueness by removing any other occurrences of `address`.
            // Vec::dedup() only removes consecutive duplicates, so we use retain()
            // to remove non-consecutive duplicates (e.g. [A,B,C] update 0→C => [C,B]).
            let mut current_index = 0usize;
            self.watch_addresses.retain(|&entry| {
                let keep = current_index == index || entry != address;
                current_index += 1;
                keep
            });
        }
    }

    /// Get current WRAM hexdump base address (for testing).
    #[cfg(test)]
    pub fn wram_hexdump_base(&self) -> Option<u16> {
        self.wram_hexdump_base
    }

    /// Get current VRAM hexdump base address (for testing).
    #[cfg(test)]
    pub fn vram_hexdump_base(&self) -> Option<u16> {
        self.vram_hexdump_base
    }

    /// Get current memory watch addresses.
    pub fn watch_addresses(&self) -> Vec<u16> {
        self.watch_addresses.clone()
    }
}

/// Default debugger configuration.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Debugger {
    disasm: DisasmWindowConfig,
}

impl Debugger {
    /// Generate a basic snapshot without view state.
    pub fn snapshot<B: GbBus>(&self, gb: &Gb<B>) -> GbDebuggerSnapshot {
        snapshot_impl(gb, None, self.disasm, None, None, &[])
    }

    /// Generate a snapshot with disassembly state and watch addresses.
    pub fn snapshot_with_disasm_state<B: GbBus>(
        &self,
        gb: &Gb<B>,
        state: &mut GbCpuDisasmWindowState,
        watch_addresses: &[u16],
    ) -> GbDebuggerSnapshot {
        snapshot_impl(gb, Some(state), self.disasm, None, None, watch_addresses)
    }
}

/// Calculate WRAM hexdump base from PC (centered around PC).
fn wram_hexdump_base_from_pc(pc: u16) -> u16 {
    let centered = pc & 0xFFF0; // Align to 16-byte boundary
    let base = centered.saturating_sub(0x80);
    // WRAM is at $C000-$DFFF. Ensure base+0xFF stays within WRAM
    base.clamp(0xC000, 0xDF00)
}

/// Normalize WRAM hexdump base to 16-byte boundary within WRAM region.
fn normalize_wram_hexdump_base(base: u16) -> u16 {
    let aligned = base & 0xFFF0;
    aligned.clamp(0xC000, 0xDF00)
}

/// Calculate VRAM hexdump base from PC (if PC is in VRAM range).
fn vram_hexdump_base_from_pc(pc: u16) -> u16 {
    if (0x8000..=0x9FFF).contains(&pc) {
        let centered = pc & 0xFFF0;
        let base = centered.saturating_sub(0x80);
        base.clamp(0x8000, 0x9F00)
    } else {
        0x8000 // Default to VRAM start
    }
}

/// Normalize VRAM hexdump base to 16-byte boundary within VRAM region.
fn normalize_vram_hexdump_base(base: u16) -> u16 {
    let aligned = base & 0xFFF0;
    aligned.clamp(0x8000, 0x9F00)
}

/// Build a complete debugger snapshot from collected data.
fn build_snapshot<B: GbBus>(
    gb: &Gb<B>,
    cpu_disasm: Vec<GbCpuDisasmLineSnapshot>,
    wram_hexdump_base_override: Option<u16>,
    vram_hexdump_base_override: Option<u16>,
    watch_addresses: &[u16],
) -> GbDebuggerSnapshot {
    let pc = gb.cpu.regs.pc;
    let cycles = gb.cpu.cycles();

    // Determine hexdump base addresses
    let wram_hexdump_base = wram_hexdump_base_override
        .map(normalize_wram_hexdump_base)
        .unwrap_or_else(|| wram_hexdump_base_from_pc(pc));

    let vram_hexdump_base = vram_hexdump_base_override
        .map(normalize_vram_hexdump_base)
        .unwrap_or_else(|| vram_hexdump_base_from_pc(pc));

    // Read WRAM hexdump (256 bytes)
    let wram_hexdump_bytes = (0u16..=0x00FF)
        .map(|offset| gb.read_for_debugger(wram_hexdump_base.wrapping_add(offset)))
        .collect::<Vec<u8>>();

    // Read VRAM hexdump (256 bytes)
    let vram_hexdump_bytes = (0u16..=0x00FF)
        .map(|offset| gb.read_for_debugger(vram_hexdump_base.wrapping_add(offset)))
        .collect::<Vec<u8>>();

    // Collect memory watch values
    let watch_values = watch_addresses
        .iter()
        .map(|address| GbMemoryWatchEntrySnapshot {
            address: *address,
            value: gb.read_for_debugger(*address),
        })
        .collect::<Vec<_>>();

    // Collect recent CPU trace
    let recent_trace = gb
        .recent_cpu_trace(32)
        .into_iter()
        .map(|line| GbCpuTraceLineSnapshot {
            addr: line.addr,
            bytes: line.bytes,
            text: line.text,
        })
        .collect::<Vec<_>>();

    // Capture PPU timing info
    let ppu = gb.cpu.bus.ppu();
    let frame_count = ppu.frame_count();
    let scanline = ppu.ly();
    let dot = ppu.dot();
    // Mode is in bits 0-1 of STAT register (0xFF41)
    let stat = ppu.read_register(0xFF41);
    let ppu_mode = stat & 0x03;

    // Read interrupt registers
    let ie = gb.read_for_debugger(0xFFFF);
    let if_reg = gb.read_for_debugger(0xFF0F);

    // Build CPU registers snapshot
    let cpu_regs = GbCpuRegsSnapshot {
        a: gb.cpu.regs.a,
        f: gb.cpu.regs.f,
        b: gb.cpu.regs.b,
        c: gb.cpu.regs.c,
        d: gb.cpu.regs.d,
        e: gb.cpu.regs.e,
        h: gb.cpu.regs.h,
        l: gb.cpu.regs.l,
        af: gb.cpu.regs.af(),
        bc: gb.cpu.regs.bc(),
        de: gb.cpu.regs.de(),
        hl: gb.cpu.regs.hl(),
        sp: gb.cpu.regs.sp,
        pc,
        z_flag: gb.cpu.regs.z_flag(),
        n_flag: gb.cpu.regs.n_flag(),
        h_flag: gb.cpu.regs.h_flag(),
        c_flag: gb.cpu.regs.c_flag(),
        cycles,
        frame_count,
        scanline,
        dot,
        ppu_mode,
        ime: gb.cpu.ime,
        halted: gb.cpu.halted,
        halt_bug: gb.cpu.halt_bug,
        ie,
        if_reg,
    };

    // Format CPU state string for display
    let cpu = format!(
        "CPU\n\
PC: {pc:04X}  A: {a:02X} F: {f:02X}  B: {b:02X} C: {c:02X}  D: {d:02X} E: {e:02X}  H: {h:02X} L: {l:02X}\n\
SP: {sp:04X}  Flags: [Z:{z} N:{n} H:{h_flag} C:{c_flag}]  IME: {ime}  Halted: {halted}\n\
CYC: {cycles}  Frame: {frame_count}  Scanline: {scanline}  Dot: {dot}  Mode: {ppu_mode}\n\
IE: {ie:02X}  IF: {if_reg:02X}",
        pc = pc,
        a = gb.cpu.regs.a,
        f = gb.cpu.regs.f,
        b = gb.cpu.regs.b,
        c = gb.cpu.regs.c,
        d = gb.cpu.regs.d,
        e = gb.cpu.regs.e,
        h = gb.cpu.regs.h,
        l = gb.cpu.regs.l,
        sp = gb.cpu.regs.sp,
        z = if gb.cpu.regs.z_flag() { "1" } else { "0" },
        n = if gb.cpu.regs.n_flag() { "1" } else { "0" },
        h_flag = if gb.cpu.regs.h_flag() { "1" } else { "0" },
        c_flag = if gb.cpu.regs.c_flag() { "1" } else { "0" },
        ime = gb.cpu.ime,
        halted = gb.cpu.halted,
        cycles = cycles,
        frame_count = frame_count,
        scanline = scanline,
        dot = dot,
        ppu_mode = ppu_mode,
        ie = ie,
        if_reg = if_reg,
    );

    GbDebuggerSnapshot {
        cpu_regs,
        wram_hexdump_base,
        wram_hexdump_bytes,
        vram_hexdump_base,
        vram_hexdump_bytes,
        cpu_disasm,
        cpu,
        watch_values,
        recent_trace,
    }
}

/// Internal snapshot implementation with optional view state.
fn snapshot_impl<B: GbBus>(
    gb: &Gb<B>,
    state: Option<&mut GbCpuDisasmWindowState>,
    disasm_config: DisasmWindowConfig,
    wram_hexdump_base_override: Option<u16>,
    vram_hexdump_base_override: Option<u16>,
    watch_addresses: &[u16],
) -> GbDebuggerSnapshot {
    let pc = gb.cpu.regs.pc;

    // Generate disassembly window
    let cpu_disasm = match state {
        Some(state) => disassemble_window_with_state(
            |addr| gb.read_for_debugger(addr),
            pc,
            state,
            disasm_config,
        ),
        None => disassemble_window(|addr| gb.read_for_debugger(addr), pc, disasm_config),
    };

    build_snapshot(
        gb,
        cpu_disasm,
        wram_hexdump_base_override,
        vram_hexdump_base_override,
        watch_addresses,
    )
}

/// Generate a basic snapshot without view state.
pub fn snapshot<B: GbBus>(gb: &Gb<B>) -> GbDebuggerSnapshot {
    Debugger::default().snapshot(gb)
}

/// Generate a snapshot with disassembly state and watch addresses.
pub fn snapshot_with_disasm_state<B: GbBus>(
    gb: &Gb<B>,
    state: &mut GbCpuDisasmWindowState,
    watch_addresses: &[u16],
) -> GbDebuggerSnapshot {
    Debugger::default().snapshot_with_disasm_state(gb, state, watch_addresses)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gb::bus::DmgBus;
    use crate::gb::cartridge::load_cartridge;
    use crate::gb::console::Gb;
    use crate::gb::model::DmgModel;

    fn minimal_cart() -> Box<dyn crate::gb::cartridge::GbCartridge> {
        let mut rom = vec![0u8; 0x8000];
        // Write some test instructions at PC
        rom[0x0000] = 0x00; // NOP
        rom[0x0001] = 0x3E; // LD A,n
        rom[0x0002] = 0x42;
        rom[0x0003] = 0xC3; // JP nn
        rom[0x0004] = 0x00;
        rom[0x0005] = 0x00;
        // Cartridge header
        rom[0x0147] = 0x00; // ROM only
        rom[0x0148] = 0x00; // 32 KB
        rom[0x0149] = 0x00; // no RAM
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;
        load_cartridge(&rom).expect("valid ROM")
    }

    fn create_test_gb() -> Gb<DmgBus> {
        let bus = DmgBus::new(minimal_cart(), DmgModel::DmgB);
        let mut gb = Gb::new(bus);

        // Disable boot ROM so we can execute test instructions from $0000
        gb.cpu.bus.write(0xFF50, 0x01);

        gb.cpu.regs.pc = 0x0000;
        gb.cpu.regs.a = 0x12;
        gb.cpu.regs.f = 0x90; // Z=1, N=0, H=0, C=1

        gb
    }

    #[test]
    fn test_snapshot_captures_cpu_regs() {
        let gb = create_test_gb();
        let snap = snapshot(&gb);

        assert_eq!(snap.cpu_regs.pc, 0x0000);
        assert_eq!(snap.cpu_regs.a, 0x12);
        assert_eq!(snap.cpu_regs.f, 0x90);
        assert!(snap.cpu_regs.z_flag);
        assert!(!snap.cpu_regs.n_flag);
        assert!(!snap.cpu_regs.h_flag);
        assert!(snap.cpu_regs.c_flag);
    }

    #[test]
    fn test_snapshot_captures_disassembly() {
        let gb = create_test_gb();
        let snap = snapshot(&gb);

        // Should have disassembly lines
        assert!(!snap.cpu_disasm.is_empty());

        // Find the current instruction (PC=0x0000)
        let current = snap.cpu_disasm.iter().find(|line| line.is_current);
        assert!(current.is_some());

        let current = current.unwrap();
        assert_eq!(current.addr, 0x0000);
        assert_eq!(current.bytes, vec![0x00]);
    }

    #[test]
    fn test_snapshot_wram_hexdump_default_range() {
        let gb = create_test_gb();
        let snap = snapshot(&gb);

        // Default WRAM hexdump starts at $C000
        assert_eq!(snap.wram_hexdump_base, 0xC000);
        assert_eq!(snap.wram_hexdump_bytes.len(), 256);
    }

    #[test]
    fn test_snapshot_vram_hexdump_default_range() {
        let gb = create_test_gb();
        let snap = snapshot(&gb);

        // Default VRAM hexdump starts at $8000
        assert_eq!(snap.vram_hexdump_base, 0x8000);
        assert_eq!(snap.vram_hexdump_bytes.len(), 256);
    }

    #[test]
    fn test_view_state_wram_base_normalization() {
        let gb = create_test_gb();
        let mut view_state = GbDebuggerViewState::default();

        // Set invalid WRAM base (outside WRAM range)
        view_state.set_wram_hexdump_base(0xE000);
        let snap = view_state.snapshot(&gb);

        // Should be clamped to valid WRAM range
        assert!(snap.wram_hexdump_base >= 0xC000);
        assert!(snap.wram_hexdump_base < 0xE000);
    }

    #[test]
    fn test_view_state_vram_base_normalization() {
        let gb = create_test_gb();
        let mut view_state = GbDebuggerViewState::default();

        // Set invalid VRAM base (outside VRAM range)
        view_state.set_vram_hexdump_base(0xA000);
        let snap = view_state.snapshot(&gb);

        // Should be clamped to valid VRAM range
        assert!(snap.vram_hexdump_base >= 0x8000);
        assert!(snap.vram_hexdump_base < 0xA000);
    }

    #[test]
    fn test_snapshot_with_watch_addresses() {
        let gb = create_test_gb();
        let mut state = GbCpuDisasmWindowState::default();
        let watch_addrs = vec![0xC000, 0xC001, 0xFF00];

        let snap = snapshot_with_disasm_state(&gb, &mut state, &watch_addrs);

        assert_eq!(snap.watch_values.len(), 3);
        assert_eq!(snap.watch_values[0].address, 0xC000);
        assert_eq!(snap.watch_values[1].address, 0xC001);
        assert_eq!(snap.watch_values[2].address, 0xFF00);
    }

    #[test]
    fn test_cpu_string_formatting() {
        let gb = create_test_gb();
        let snap = snapshot(&gb);

        // CPU string should contain key info
        assert!(snap.cpu.contains("PC: 0000"));
        assert!(snap.cpu.contains("A: 12"));
        assert!(snap.cpu.contains("Z:1")); // Z flag set
        assert!(snap.cpu.contains("C:1")); // C flag set
    }
}
