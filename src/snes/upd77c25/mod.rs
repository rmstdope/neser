//! NEC uPD77C25 digital signal processor, the CPU inside the DSP-1/1A/1B (and DSP-2/3/4).
//!
//! The chip runs its own mask-ROM program (2048 x 24-bit opcodes) against a 1024 x 16-bit data
//! ROM and 256 x 16-bit data RAM, and talks to the SNES only through two ports: the data
//! register DR and the status register SR (fullsnes "SNES Cart DSP-n/ST010/ST011 - NEC
//! uPD77C25 - Registers & Flags & Overview"). The program is Nintendo's, so it is supplied by the player
//! as a firmware image ([`Upd77c25Firmware`]) and never shipped with NESER.
//!
//! Instruction semantics follow fullsnes' "ALU and LD Instructions" and "JP Instructions"
//! tables. Where fullsnes is silent or ambiguous this follows Mesen2
//! (`Core/SNES/Coprocessors/DSP/NecDsp.cpp`), the project's SNES implementation reference:
//! - the S1/OV1 update. fullsnes words S1 as "if OV0 then S1=S0" and, for the logic
//!   opcodes, "actually seems to be S1=sf". Mesen2 and ares both set S1=S0 whenever the
//!   *previous* OV1 is clear, and resolve OV1 from S0==S1 on a second overflow. The two
//!   agree whenever no overflow is pending, and the implementations are what run the games.
//! - the order of an ALU instruction: the source read, the ALU, the destination load, then
//!   the DP/RP adjustment, which is skipped when the destination is DP or RP.
//! - the host side of DR: the 16-bit mode's LSB-then-MSB order and when RQM/DRS change.
//! - the SR bits a program write cannot change (`0x907C`).
//! - JSIAK/JSOAK testing SR's SIC/SOC bits (the serial ports are not wired on a SNES cart).
//! - the RQM wait-loop shortcut, a speed measure: a `JRQM`/`JNRQM` to itself parks the
//!   chip until the SNES touches DR, since nothing else can change RQM.
//!
//! Reset follows fullsnes ("PC=000h, FlagA=00h, FlagB=00h, SR=0000h, … RP=3FFh"), where Mesen2
//! clears the whole state.

mod instructions;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) mod asm;

use crate::snes::ppu::SnesVideoRegion;
use serde::{Deserialize, Serialize};
use std::rc::Rc;

/// Program ROM words (fullsnes "Memory": "2048 x 24bit Instruction ROM").
pub const PROGRAM_WORDS: usize = 2048;
/// Data ROM words ("1024 x 16bit Data ROM").
pub const DATA_WORDS: usize = 1024;
/// Data RAM words ("256 x 16bit Data RAM").
pub const RAM_WORDS: usize = 256;
/// Bytes in a DSP-n firmware image: the program ROM then the data ROM (fullsnes "ROM-Images").
pub const DSP_IMAGE_SIZE: usize = PROGRAM_WORDS * 3 + DATA_WORDS * 2;
/// Stack levels ("STACK 11-bit x 4-levels").
const STACK_LEVELS: usize = 4;

/// The DSP-n crystal (fullsnes cartridge component list: "DSPn 7.600MHz"). One opcode per clock.
const CLOCK_HZ: u64 = 7_600_000;

/// SR bit 15: RQM, request for master.
pub const SR_RQM: u16 = 0x8000;
/// SR bit 12: DRS, the second half of a 16-bit DR transfer is pending.
pub const SR_DRS: u16 = 0x1000;
/// SR bit 10: DRC, DR is 8-bit (1) or 16-bit (0).
pub const SR_DRC: u16 = 0x0400;
/// SR bit 9: SOC.
pub const SR_SOC: u16 = 0x0200;
/// SR bit 8: SIC.
pub const SR_SIC: u16 = 0x0100;
/// The SR bits an `@SR` write leaves alone (Mesen2 `Load` case 0x07).
const SR_WRITE_PROTECTED: u16 = 0x907C;

/// Same values as Mesen2 `SnesConsole::GetMasterClockRate`, as the CX4 uses.
const fn master_clock_hz(region: SnesVideoRegion) -> u64 {
    match region {
        SnesVideoRegion::Ntsc => 21_477_270,
        SnesVideoRegion::Pal => 21_281_370,
    }
}

/// A uPD77C25 program and data ROM, parsed from a player's firmware image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upd77c25Firmware {
    program: Vec<u32>,
    data: Vec<u16>,
}

impl Upd77c25Firmware {
    /// Parses an 8192-byte DSP-n image, returning the actual size when it is anything else.
    ///
    /// fullsnes "ROM-Images" documents two 8 KB layouts: little-endian (the newer, preferred
    /// one) and big-endian. Every original ROM has "JRQM $" (`97C00xh`) within its first four
    /// opcodes, so finding it big-endian (`97h,C0h,0xh`) selects that order; otherwise the
    /// image is read little-endian. The 10 KB padded "oldest" layout is not accepted.
    pub fn from_image(bytes: &[u8]) -> Result<Self, usize> {
        if bytes.len() != DSP_IMAGE_SIZE {
            return Err(bytes.len());
        }
        let (program_bytes, data_bytes) = bytes.split_at(PROGRAM_WORDS * 3);
        let big_endian = program_bytes
            .as_chunks::<3>()
            .0
            .iter()
            .take(4)
            .any(|op| op[0] == 0x97 && op[1] == 0xC0 && op[2] & 0xF3 == 0);
        let program = program_bytes
            .as_chunks::<3>()
            .0
            .iter()
            .map(|op| {
                let (lo, mid, hi) = if big_endian {
                    (op[2], op[1], op[0])
                } else {
                    (op[0], op[1], op[2])
                };
                u32::from(lo) | (u32::from(mid) << 8) | (u32::from(hi) << 16)
            })
            .collect();
        let data = data_bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|w| {
                if big_endian {
                    u16::from_be_bytes([w[0], w[1]])
                } else {
                    u16::from_le_bytes([w[0], w[1]])
                }
            })
            .collect();
        Ok(Self { program, data })
    }

    /// A data-ROM word, for tests that tell firmware images apart.
    #[cfg(test)]
    pub(crate) fn data_word(&self, index: usize) -> u16 {
        self.data[index]
    }

    /// Builds a firmware from already-decoded words (tests' synthetic programs).
    #[cfg(test)]
    pub(crate) fn from_words(program: Vec<u32>, data: Vec<u16>) -> Self {
        assert_eq!(program.len(), PROGRAM_WORDS);
        assert_eq!(data.len(), DATA_WORDS);
        Self { program, data }
    }
}

/// One accumulator's flags (fullsnes "FlagA/FlagB").
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Flags {
    pub s0: bool,
    pub s1: bool,
    pub c: bool,
    pub z: bool,
    pub ov0: bool,
    pub ov1: bool,
}

/// Every piece of uPD77C25 state, saved and restored as one value. The firmware is not part of
/// it: it is the player's file and is read again when the game loads.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct Upd77c25State {
    pub pc: u16,
    pub rp: u16,
    pub dp: u16,
    pub sp: u8,
    pub stack: [u16; STACK_LEVELS],
    pub a: u16,
    pub b: u16,
    pub flags_a: Flags,
    pub flags_b: Flags,
    pub tr: u16,
    pub trb: u16,
    pub k: u16,
    pub l: u16,
    /// K*L*2 high word (the ALU's P=2 input).
    pub m: u16,
    /// K*L*2 low word (P=3).
    pub n: u16,
    pub sr: u16,
    pub dr: u16,
    pub si: u16,
    pub so: u16,
    pub ram: Vec<u16>,
    /// Master-clock remainder towards the next chip clock.
    pub clock_budget: u64,
    /// Chip clocks run, and the count owed so far by elapsed master clocks.
    pub cycle_count: u64,
    pub target_cycle: u64,
    /// Parked in a `JRQM`/`JNRQM` self-loop until the SNES accesses DR.
    pub in_rqm_loop: bool,
}

impl Default for Upd77c25State {
    fn default() -> Self {
        Self {
            pc: 0,
            rp: 0x3FF,
            dp: 0,
            sp: 0,
            stack: [0; STACK_LEVELS],
            a: 0,
            b: 0,
            flags_a: Flags::default(),
            flags_b: Flags::default(),
            tr: 0,
            trb: 0,
            k: 0,
            l: 0,
            m: 0,
            n: 0,
            sr: 0,
            dr: 0,
            si: 0,
            so: 0,
            ram: vec![0; RAM_WORDS],
            clock_budget: 0,
            cycle_count: 0,
            target_cycle: 0,
            in_rqm_loop: false,
        }
    }
}

/// A uPD77C25 running a firmware.
pub struct Upd77c25 {
    firmware: Rc<Upd77c25Firmware>,
    pub(crate) state: Upd77c25State,
    master_clock_hz: u64,
}

impl Upd77c25 {
    /// A powered-on chip: every register and the data RAM zero, then reset.
    pub fn new(firmware: Rc<Upd77c25Firmware>, region: SnesVideoRegion) -> Self {
        let mut chip = Self {
            firmware,
            state: Upd77c25State::default(),
            master_clock_hz: master_clock_hz(region),
        };
        chip.reset();
        chip
    }

    /// The RST line (fullsnes "Reset (Vector 000h)"): PC=000h, both flag sets and SR cleared,
    /// RP=3FFh. The serial/DMA request lines it also names are not modelled. Every other
    /// register and the data RAM are left unchanged.
    pub fn reset(&mut self) {
        let s = &mut self.state;
        s.pc = 0;
        s.flags_a = Flags::default();
        s.flags_b = Flags::default();
        s.sr = 0;
        s.rp = 0x3FF;
        s.in_rqm_loop = false;
    }

    /// Retunes the clock to a region's master clock (as `Cx4::set_video_region`).
    pub fn set_video_region(&mut self, region: SnesVideoRegion) {
        self.master_clock_hz = master_clock_hz(region);
    }

    /// The 256-word data RAM, for the power-on fill.
    pub fn data_ram_mut(&mut self) -> &mut [u16] {
        &mut self.state.ram
    }

    /// Advances by one SNES master clock, running every chip clock that has come due.
    pub fn tick_master_clock(&mut self) {
        // 7.6 MHz is below the master clock, so each master clock owes at most one opcode.
        self.state.clock_budget += CLOCK_HZ;
        if self.state.clock_budget >= self.master_clock_hz {
            self.state.clock_budget -= self.master_clock_hz;
            self.state.target_cycle += 1;
        }
        while self.state.cycle_count < self.state.target_cycle {
            if self.state.in_rqm_loop {
                self.state.cycle_count = self.state.target_cycle;
                break;
            }
            self.step();
        }
    }

    /// Executes one opcode (one chip clock).
    pub(crate) fn step(&mut self) {
        let pc = usize::from(self.state.pc) & (PROGRAM_WORDS - 1);
        let opcode = self.firmware.program[pc];
        self.state.pc = ((pc + 1) & (PROGRAM_WORDS - 1)) as u16;
        self.execute(opcode);
        // fullsnes "Multiplier": after each instruction the hardware computes K*L*2.
        let product = i32::from(self.state.k as i16) * i32::from(self.state.l as i16);
        self.state.m = (product >> 15) as u16;
        self.state.n = (product << 1) as u16;
        self.state.cycle_count += 1;
    }

    /// SNES read of DR: one byte, the low half first in 16-bit mode.
    pub fn read_dr(&mut self) -> u8 {
        self.state.in_rqm_loop = false;
        let s = &mut self.state;
        if s.sr & SR_DRC != 0 {
            s.sr &= !SR_RQM;
            s.dr as u8
        } else if s.sr & SR_DRS != 0 {
            s.sr &= !(SR_RQM | SR_DRS);
            (s.dr >> 8) as u8
        } else {
            s.sr |= SR_DRS;
            s.dr as u8
        }
    }

    /// SNES write of DR: one byte, the low half first in 16-bit mode.
    pub fn write_dr(&mut self, value: u8) {
        self.state.in_rqm_loop = false;
        let s = &mut self.state;
        if s.sr & SR_DRC != 0 {
            s.sr &= !SR_RQM;
            s.dr = (s.dr & 0xFF00) | u16::from(value);
        } else if s.sr & SR_DRS != 0 {
            s.sr &= !(SR_RQM | SR_DRS);
            s.dr = (s.dr & 0x00FF) | (u16::from(value) << 8);
        } else {
            s.sr |= SR_DRS;
            s.dr = (s.dr & 0xFF00) | u16::from(value);
        }
    }

    /// SNES read of SR: its high byte (fullsnes "SR.Bit15-8 are output to D7-0 pins").
    pub fn read_sr(&self) -> u8 {
        (self.state.sr >> 8) as u8
    }

    /// A side-effect-free view of a port, for debugger peeks: SR's high byte, or DR's byte that
    /// the next read would return.
    pub fn peek(&self, is_sr: bool) -> u8 {
        let s = &self.state;
        if is_sr {
            self.read_sr()
        } else if s.sr & SR_DRC == 0 && s.sr & SR_DRS != 0 {
            (s.dr >> 8) as u8
        } else {
            s.dr as u8
        }
    }

    pub fn capture_state(&self) -> Upd77c25State {
        self.state.clone()
    }

    /// Restores a captured state, rejecting one whose RAM is the wrong size or whose stack
    /// pointer is out of range (a corrupt file) rather than letting it panic later.
    pub fn restore_state(&mut self, state: &Upd77c25State) -> Result<(), String> {
        if state.ram.len() != RAM_WORDS || usize::from(state.sp) >= STACK_LEVELS {
            return Err("uPD77C25 state size mismatch".to_string());
        }
        self.state = state.clone();
        Ok(())
    }
}
