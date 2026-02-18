#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuRegsSnapshot {
    pub pc: u16,
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub sp: u8,
    pub p: u8,
    pub cycles: u64,
    pub frame_count: u64,
    pub scanline: u16,
    pub pixel: u16,
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
    pub(super) start: Option<u16>,
}
