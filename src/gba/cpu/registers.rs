//! ARM7TDMI register file and CPU mode model.
//!
//! The ARM7TDMI exposes 16 general-purpose registers (R0–R15) and a Current
//! Program Status Register (CPSR). Several registers are *banked* per
//! processor mode:
//!
//! ```text
//! mode   R8    R9    R10   R11   R12   R13(SP) R14(LR) SPSR
//! USR/SYS r8    r9    r10   r11   r12   r13_usr r14_usr  -
//! FIQ    r8_f  r9_f  r10_f r11_f r12_f r13_fiq r14_fiq spsr_fiq
//! IRQ    r8    r9    r10   r11   r12   r13_irq r14_irq spsr_irq
//! SVC    r8    r9    r10   r11   r12   r13_svc r14_svc spsr_svc
//! ABT    r8    r9    r10   r11   r12   r13_abt r14_abt spsr_abt
//! UND    r8    r9    r10   r11   r12   r13_und r14_und spsr_und
//! ```
//!
//! Reads/writes to R0–R7 and R15 are not banked. R13/R14 are banked by mode
//! (USR and SYS share the same bank). R8–R12 are only banked in FIQ mode.
//!
//! The implementation models the live registers in `r[0..16]` and the *saved*
//! copies for the currently inactive modes in dedicated fields. Switching mode
//! moves the affected registers between the live array and the saved fields.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// CPSR bit layout
// ---------------------------------------------------------------------------

/// Negative flag (CPSR bit 31).
pub const FLAG_N: u32 = 1 << 31;
/// Zero flag (CPSR bit 30).
pub const FLAG_Z: u32 = 1 << 30;
/// Carry flag (CPSR bit 29).
pub const FLAG_C: u32 = 1 << 29;
/// Overflow flag (CPSR bit 28).
pub const FLAG_V: u32 = 1 << 28;
/// IRQ disable (CPSR bit 7).
pub const FLAG_I: u32 = 1 << 7;
/// FIQ disable (CPSR bit 6).
pub const FLAG_F: u32 = 1 << 6;
/// Thumb state (CPSR bit 5).
pub const FLAG_T: u32 = 1 << 5;
/// Mask covering the mode bits M[4:0].
pub const MODE_MASK: u32 = 0x1F;

// ---------------------------------------------------------------------------
// CPU modes
// ---------------------------------------------------------------------------

/// ARM7TDMI processor mode (the 5-bit M field of CPSR).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CpuMode {
    /// User mode (non-privileged, normal program execution).
    User = 0b10000,
    /// Fast Interrupt mode.
    Fiq = 0b10001,
    /// Interrupt mode.
    Irq = 0b10010,
    /// Supervisor mode (entered on SWI or reset).
    Supervisor = 0b10011,
    /// Abort mode (memory access faults).
    Abort = 0b10111,
    /// Undefined instruction mode.
    Undefined = 0b11011,
    /// System mode (privileged, shares USR registers).
    System = 0b11111,
}

impl CpuMode {
    /// Convert a 5-bit mode field to the corresponding [`CpuMode`].
    pub fn from_bits(bits: u32) -> Option<Self> {
        match bits & MODE_MASK {
            0b10000 => Some(CpuMode::User),
            0b10001 => Some(CpuMode::Fiq),
            0b10010 => Some(CpuMode::Irq),
            0b10011 => Some(CpuMode::Supervisor),
            0b10111 => Some(CpuMode::Abort),
            0b11011 => Some(CpuMode::Undefined),
            0b11111 => Some(CpuMode::System),
            _ => None,
        }
    }

    /// Returns the mode's M[4:0] bit pattern.
    pub fn bits(self) -> u32 {
        self as u32
    }

    /// Whether the mode has its own SPSR (every mode except USR/SYS).
    pub fn has_spsr(self) -> bool {
        !matches!(self, CpuMode::User | CpuMode::System)
    }
}

// ---------------------------------------------------------------------------
// Condition codes
// ---------------------------------------------------------------------------

/// Evaluate an ARM 4-bit condition code against a CPSR value.
pub fn condition_met(cpsr: u32, cond: u8) -> bool {
    let n = cpsr & FLAG_N != 0;
    let z = cpsr & FLAG_Z != 0;
    let c = cpsr & FLAG_C != 0;
    let v = cpsr & FLAG_V != 0;
    match cond & 0xF {
        0x0 => z,              // EQ
        0x1 => !z,             // NE
        0x2 => c,              // CS / HS
        0x3 => !c,             // CC / LO
        0x4 => n,              // MI
        0x5 => !n,             // PL
        0x6 => v,              // VS
        0x7 => !v,             // VC
        0x8 => c && !z,        // HI
        0x9 => !c || z,        // LS
        0xA => n == v,         // GE
        0xB => n != v,         // LT
        0xC => !z && (n == v), // GT
        0xD => z || (n != v),  // LE
        0xE => true,           // AL
        0xF => false,          // NV (reserved on ARMv4)
        _ => unreachable!(),
    }
}

// ---------------------------------------------------------------------------
// Register file
// ---------------------------------------------------------------------------

/// ARM7TDMI register file with banked register support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registers {
    /// Live R0..R15.
    pub r: [u32; 16],
    /// Current Program Status Register.
    pub cpsr: u32,

    // FIQ-banked R8..R12 (used when *not* in FIQ mode)
    fiq_r8_12: [u32; 5],
    // USR-banked copies of R8..R12 (used when in FIQ mode)
    usr_r8_12: [u32; 5],

    // Banked R13 (SP) and R14 (LR) per mode.
    sp_usr: u32,
    lr_usr: u32,
    sp_fiq: u32,
    lr_fiq: u32,
    sp_irq: u32,
    lr_irq: u32,
    sp_svc: u32,
    lr_svc: u32,
    sp_abt: u32,
    lr_abt: u32,
    sp_und: u32,
    lr_und: u32,

    // Saved Program Status Registers per mode.
    spsr_fiq: u32,
    spsr_irq: u32,
    spsr_svc: u32,
    spsr_abt: u32,
    spsr_und: u32,

    /// The mode currently considered "live" in `r[]`. This is normally in sync
    /// with the M bits of CPSR, but is tracked separately so that the bank
    /// switching code can safely move registers around when the mode changes.
    live_mode: CpuMode,
}

impl Default for Registers {
    fn default() -> Self {
        Self::new()
    }
}

impl Registers {
    /// Construct a new register file in Supervisor mode with all registers
    /// zeroed. ARM7TDMI hardware enters Supervisor on reset.
    pub fn new() -> Self {
        Self {
            r: [0; 16],
            cpsr: CpuMode::Supervisor.bits() | FLAG_I | FLAG_F,
            fiq_r8_12: [0; 5],
            usr_r8_12: [0; 5],
            sp_usr: 0,
            lr_usr: 0,
            sp_fiq: 0,
            lr_fiq: 0,
            sp_irq: 0,
            lr_irq: 0,
            sp_svc: 0,
            lr_svc: 0,
            sp_abt: 0,
            lr_abt: 0,
            sp_und: 0,
            lr_und: 0,
            spsr_fiq: 0,
            spsr_irq: 0,
            spsr_svc: 0,
            spsr_abt: 0,
            spsr_und: 0,
            live_mode: CpuMode::Supervisor,
        }
    }

    // --- Mode helpers -----------------------------------------------------

    /// Return the current mode according to CPSR.
    pub fn mode(&self) -> CpuMode {
        CpuMode::from_bits(self.cpsr).unwrap_or(self.live_mode)
    }

    /// Whether the CPU is in Thumb state.
    pub fn thumb(&self) -> bool {
        self.cpsr & FLAG_T != 0
    }

    /// Set or clear the Thumb-state flag.
    pub fn set_thumb(&mut self, value: bool) {
        if value {
            self.cpsr |= FLAG_T;
        } else {
            self.cpsr &= !FLAG_T;
        }
    }

    /// Read a register from the user/system bank, regardless of current mode.
    pub fn read_user_reg(&self, index: usize) -> u32 {
        match index {
            0..=7 | 15 => self.r[index],
            8..=12 => {
                if self.live_mode == CpuMode::Fiq {
                    self.usr_r8_12[index - 8]
                } else {
                    self.r[index]
                }
            }
            13 => {
                if self.live_mode == CpuMode::User || self.live_mode == CpuMode::System {
                    self.r[13]
                } else {
                    self.sp_usr
                }
            }
            14 => {
                if self.live_mode == CpuMode::User || self.live_mode == CpuMode::System {
                    self.r[14]
                } else {
                    self.lr_usr
                }
            }
            _ => panic!("register index out of range: {index}"),
        }
    }

    /// Write a register in the user/system bank, regardless of current mode.
    pub fn write_user_reg(&mut self, index: usize, value: u32) {
        match index {
            0..=7 | 15 => self.r[index] = value,
            8..=12 => {
                if self.live_mode == CpuMode::Fiq {
                    self.usr_r8_12[index - 8] = value;
                } else {
                    self.r[index] = value;
                }
            }
            13 => {
                if self.live_mode == CpuMode::User || self.live_mode == CpuMode::System {
                    self.r[13] = value;
                }
                self.sp_usr = value;
            }
            14 => {
                if self.live_mode == CpuMode::User || self.live_mode == CpuMode::System {
                    self.r[14] = value;
                }
                self.lr_usr = value;
            }
            _ => panic!("register index out of range: {index}"),
        }
    }

    // --- Flag helpers -----------------------------------------------------

    pub fn n_flag(&self) -> bool {
        self.cpsr & FLAG_N != 0
    }
    pub fn z_flag(&self) -> bool {
        self.cpsr & FLAG_Z != 0
    }
    pub fn c_flag(&self) -> bool {
        self.cpsr & FLAG_C != 0
    }
    pub fn v_flag(&self) -> bool {
        self.cpsr & FLAG_V != 0
    }
    pub fn i_flag(&self) -> bool {
        self.cpsr & FLAG_I != 0
    }
    pub fn f_flag(&self) -> bool {
        self.cpsr & FLAG_F != 0
    }

    /// Update only the NZCV bits of CPSR.
    pub fn set_nzcv(&mut self, n: bool, z: bool, c: bool, v: bool) {
        let mut cpsr = self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V);
        if n {
            cpsr |= FLAG_N;
        }
        if z {
            cpsr |= FLAG_Z;
        }
        if c {
            cpsr |= FLAG_C;
        }
        if v {
            cpsr |= FLAG_V;
        }
        self.cpsr = cpsr;
    }

    // --- Banked SPSR access ----------------------------------------------

    /// Get the SPSR for the current mode. Returns CPSR for USR/SYS (no SPSR).
    pub fn spsr(&self) -> u32 {
        match self.mode() {
            CpuMode::Fiq => self.spsr_fiq,
            CpuMode::Irq => self.spsr_irq,
            CpuMode::Supervisor => self.spsr_svc,
            CpuMode::Abort => self.spsr_abt,
            CpuMode::Undefined => self.spsr_und,
            CpuMode::User | CpuMode::System => self.cpsr,
        }
    }

    /// Set the SPSR for the current mode. No-op for USR/SYS.
    pub fn set_spsr(&mut self, value: u32) {
        match self.mode() {
            CpuMode::Fiq => self.spsr_fiq = value,
            CpuMode::Irq => self.spsr_irq = value,
            CpuMode::Supervisor => self.spsr_svc = value,
            CpuMode::Abort => self.spsr_abt = value,
            CpuMode::Undefined => self.spsr_und = value,
            CpuMode::User | CpuMode::System => {}
        }
    }

    // --- Mode switching ---------------------------------------------------

    /// Switch CPU mode, banking registers as needed. Updates CPSR's M field.
    pub fn switch_mode(&mut self, new_mode: CpuMode) {
        if new_mode == self.live_mode {
            self.cpsr = (self.cpsr & !MODE_MASK) | new_mode.bits();
            return;
        }

        // 1) Save current SP/LR for the outgoing mode.
        match self.live_mode {
            CpuMode::User | CpuMode::System => {
                self.sp_usr = self.r[13];
                self.lr_usr = self.r[14];
            }
            CpuMode::Fiq => {
                self.sp_fiq = self.r[13];
                self.lr_fiq = self.r[14];
            }
            CpuMode::Irq => {
                self.sp_irq = self.r[13];
                self.lr_irq = self.r[14];
            }
            CpuMode::Supervisor => {
                self.sp_svc = self.r[13];
                self.lr_svc = self.r[14];
            }
            CpuMode::Abort => {
                self.sp_abt = self.r[13];
                self.lr_abt = self.r[14];
            }
            CpuMode::Undefined => {
                self.sp_und = self.r[13];
                self.lr_und = self.r[14];
            }
        }

        // 2) Handle FIQ banking of R8..R12.
        let was_fiq = matches!(self.live_mode, CpuMode::Fiq);
        let now_fiq = matches!(new_mode, CpuMode::Fiq);
        if was_fiq != now_fiq {
            if was_fiq {
                // Save FIQ R8..R12 and restore USR R8..R12.
                for i in 0..5 {
                    self.fiq_r8_12[i] = self.r[8 + i];
                    self.r[8 + i] = self.usr_r8_12[i];
                }
            } else {
                // Save USR R8..R12 and load FIQ R8..R12.
                for i in 0..5 {
                    self.usr_r8_12[i] = self.r[8 + i];
                    self.r[8 + i] = self.fiq_r8_12[i];
                }
            }
        }

        // 3) Load SP/LR for the incoming mode.
        match new_mode {
            CpuMode::User | CpuMode::System => {
                self.r[13] = self.sp_usr;
                self.r[14] = self.lr_usr;
            }
            CpuMode::Fiq => {
                self.r[13] = self.sp_fiq;
                self.r[14] = self.lr_fiq;
            }
            CpuMode::Irq => {
                self.r[13] = self.sp_irq;
                self.r[14] = self.lr_irq;
            }
            CpuMode::Supervisor => {
                self.r[13] = self.sp_svc;
                self.r[14] = self.lr_svc;
            }
            CpuMode::Abort => {
                self.r[13] = self.sp_abt;
                self.r[14] = self.lr_abt;
            }
            CpuMode::Undefined => {
                self.r[13] = self.sp_und;
                self.r[14] = self.lr_und;
            }
        }

        // 4) Update CPSR mode bits and remember the new live mode.
        self.cpsr = (self.cpsr & !MODE_MASK) | new_mode.bits();
        self.live_mode = new_mode;
    }

    /// Replace the entire CPSR while keeping banked registers consistent. If
    /// the mode bits change, banks are switched accordingly.
    pub fn write_cpsr(&mut self, value: u32) {
        let new_mode = CpuMode::from_bits(value).unwrap_or(self.live_mode);
        if new_mode != self.live_mode {
            self.switch_mode(new_mode);
        }
        self.cpsr = value;
        // Ensure the mode bits in CPSR remain consistent with `live_mode`.
        self.cpsr = (self.cpsr & !MODE_MASK) | self.live_mode.bits();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn condition_codes_basic() {
        // Z=1
        let cpsr = FLAG_Z | CpuMode::User.bits();
        assert!(condition_met(cpsr, 0x0)); // EQ
        assert!(!condition_met(cpsr, 0x1)); // NE
        assert!(condition_met(cpsr, 0xE)); // AL
        assert!(!condition_met(cpsr, 0xF)); // NV

        // N != V => LT
        let cpsr = FLAG_N | CpuMode::User.bits();
        assert!(condition_met(cpsr, 0xB));
        assert!(!condition_met(cpsr, 0xA)); // GE
    }

    #[test]
    fn switch_mode_banks_sp_and_lr() {
        let mut regs = Registers::new();
        // Start in USR mode for clarity.
        regs.switch_mode(CpuMode::User);
        regs.r[13] = 0x0300_7F00;
        regs.r[14] = 0xDEAD_BEEF;

        regs.switch_mode(CpuMode::Irq);
        // IRQ has its own bank — should be zeros (initial).
        assert_eq!(regs.r[13], 0);
        assert_eq!(regs.r[14], 0);
        regs.r[13] = 0x0300_7FA0;
        regs.r[14] = 0xCAFEBABE;

        // Returning to USR must restore the original USR bank values.
        regs.switch_mode(CpuMode::User);
        assert_eq!(regs.r[13], 0x0300_7F00);
        assert_eq!(regs.r[14], 0xDEAD_BEEF);

        // And IRQ values were saved.
        regs.switch_mode(CpuMode::Irq);
        assert_eq!(regs.r[13], 0x0300_7FA0);
        assert_eq!(regs.r[14], 0xCAFEBABE);
    }

    #[test]
    fn switch_mode_banks_fiq_r8_r12() {
        let mut regs = Registers::new();
        regs.switch_mode(CpuMode::User);
        for i in 8..=12 {
            regs.r[i] = 0x1000 + i as u32;
        }

        regs.switch_mode(CpuMode::Fiq);
        for i in 8..=12 {
            // FIQ has its own R8..R12 — initial value is 0.
            assert_eq!(regs.r[i], 0);
            regs.r[i] = 0x2000 + i as u32;
        }

        regs.switch_mode(CpuMode::User);
        for i in 8..=12 {
            assert_eq!(regs.r[i], 0x1000 + i as u32);
        }

        // R0..R7 are NOT banked between USR and FIQ.
        regs.r[0] = 0xAA;
        regs.switch_mode(CpuMode::Fiq);
        assert_eq!(regs.r[0], 0xAA);
        for i in 8..=12 {
            assert_eq!(regs.r[i], 0x2000 + i as u32);
        }
    }

    #[test]
    fn cpsr_mode_field_tracks_switches() {
        let mut regs = Registers::new();
        regs.switch_mode(CpuMode::Irq);
        assert_eq!(regs.cpsr & MODE_MASK, CpuMode::Irq.bits());
        regs.switch_mode(CpuMode::Supervisor);
        assert_eq!(regs.cpsr & MODE_MASK, CpuMode::Supervisor.bits());
    }

    #[test]
    fn spsr_per_mode() {
        let mut regs = Registers::new();
        regs.switch_mode(CpuMode::Irq);
        regs.set_spsr(0x1111_1111);
        regs.switch_mode(CpuMode::Fiq);
        regs.set_spsr(0x2222_2222);
        regs.switch_mode(CpuMode::Irq);
        assert_eq!(regs.spsr(), 0x1111_1111);
        regs.switch_mode(CpuMode::Fiq);
        assert_eq!(regs.spsr(), 0x2222_2222);
    }

    #[test]
    fn user_mode_has_no_spsr() {
        let mut regs = Registers::new();
        regs.switch_mode(CpuMode::User);
        // SPSR of USR is just CPSR (architectural definition).
        assert_eq!(regs.spsr(), regs.cpsr);
        // set_spsr is a no-op in USR.
        let before = regs.cpsr;
        regs.set_spsr(0xDEADBEEF);
        assert_eq!(regs.cpsr, before);
    }
}
