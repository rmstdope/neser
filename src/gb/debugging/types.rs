//! Game Boy debugger snapshot types.
//!
//! These types represent immutable snapshots of emulator state for display in the debugger UI.
//! They mirror the NES debugger architecture but are adapted for SM83 CPU and GB PPU.

use super::disasm::GbCpuDisasmLineSnapshot;

/// CPU register snapshot for SM83.
///
/// Captures all CPU registers, flags, timing information, and interrupt state at a point in time.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GbCpuRegsSnapshot {
    // 8-bit registers
    pub a: u8,
    pub f: u8, // Flags: Z(bit 7), N(bit 6), H(bit 5), C(bit 4), bits 3-0 always 0
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,

    // 16-bit registers
    pub af: u16,
    pub bc: u16,
    pub de: u16,
    pub hl: u16,
    pub sp: u16,
    pub pc: u16,

    // Flags (extracted from F register for convenience)
    pub z_flag: bool, // Zero
    pub n_flag: bool, // Subtract
    pub h_flag: bool, // Half-carry
    pub c_flag: bool, // Carry

    // Timing
    pub cycles: u64, // Total M-cycles elapsed
    pub frame_count: u64,
    pub scanline: u8, // Current scanline (0-153)
    pub dot: u16,     // Current dot within scanline (0-455)
    pub ppu_mode: u8, // PPU mode: 0=HBlank, 1=VBlank, 2=OamScan, 3=PixelTransfer

    // CPU state
    pub ime: bool,      // Interrupt Master Enable
    pub halted: bool,   // CPU is halted (HALT instruction)
    pub halt_bug: bool, // HALT bug is active

    // Interrupt registers
    pub ie: u8,     // Interrupt Enable ($FFFF)
    pub if_reg: u8, // Interrupt Flag ($FF0F)
}

/// Single CPU trace entry (executed instruction).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GbCpuTraceLineSnapshot {
    pub addr: u16,
    pub bytes: Vec<u8>,
    pub text: String,
}

/// Memory watch entry with current value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GbMemoryWatchEntrySnapshot {
    pub address: u16,
    pub value: u8,
}

/// Complete debugger snapshot for GB emulator.
///
/// Contains all information needed to render the debugger UI:
/// - CPU register state
/// - Disassembly window around current PC
/// - Recent execution trace
/// - Memory hexdumps (WRAM and VRAM)
/// - Memory watch values
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GbDebuggerSnapshot {
    pub cpu_regs: GbCpuRegsSnapshot,

    /// WRAM hexdump starting address
    pub wram_hexdump_base: u16,
    /// WRAM hexdump bytes (typically 256 bytes = 16 rows × 16 bytes)
    pub wram_hexdump_bytes: Vec<u8>,

    /// VRAM hexdump starting address
    pub vram_hexdump_base: u16,
    /// VRAM hexdump bytes (typically 256 bytes = 16 rows × 16 bytes)
    pub vram_hexdump_bytes: Vec<u8>,

    /// Disassembly window (typically 20 lines: 10 before PC, 9 after PC, 1 at PC)
    pub cpu_disasm: Vec<GbCpuDisasmLineSnapshot>,

    /// Formatted CPU state string for display
    pub cpu: String,

    /// Memory watch entries with live values
    pub watch_values: Vec<GbMemoryWatchEntrySnapshot>,

    /// Recent executed instructions (ring buffer tail, typically last 32)
    pub recent_trace: Vec<GbCpuTraceLineSnapshot>,
}

impl Default for GbDebuggerSnapshot {
    fn default() -> Self {
        Self {
            cpu_regs: GbCpuRegsSnapshot::default(),
            wram_hexdump_base: 0xC000,
            wram_hexdump_bytes: vec![0; 256],
            vram_hexdump_base: 0x8000,
            vram_hexdump_bytes: vec![0; 256],
            cpu_disasm: Vec::new(),
            cpu: String::new(),
            watch_values: Vec::new(),
            recent_trace: Vec::new(),
        }
    }
}
