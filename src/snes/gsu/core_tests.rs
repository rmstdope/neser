//! White-box tests of the GSU core, driving a [`Gsu`] directly over a hand-assembled program.
//!
//! Expected values follow fullsnes "SNES Cart GSU-n CPU MOV/ALU/JMP and Prefix Opcodes" and
//! "General I/O Ports"; where a test pins a Mesen2-derived detail the test says so.

use std::cell::RefCell;
use std::rc::Rc;

use super::Gsu;

/// Where the rig places the program: GSU `$00:8000`, ROM offset 0.
pub(super) const PROGRAM: u16 = 0x8000;
/// A generous bound on how long any rig program may run, in master clocks.
const RUN_LIMIT: u64 = 1_000_000;

/// SCMR with both buses handed to the GSU (RON | RAN) and 4-colour, 128-pixel-high screen.
pub(super) const SCMR_RON_RAN: u8 = 0x18;

pub(super) struct Rig {
    pub gsu: Gsu,
    pub ram: Rc<RefCell<Vec<u8>>>,
    /// Master clocks ticked so far.
    pub clocks: u64,
}

impl Rig {
    /// A GSU over a 128 KB ROM holding `program` at `$00:8000` and 32 KB of zeroed RAM.
    pub fn new(program: &[u8]) -> Self {
        Self::with_rom(|rom| rom[..program.len()].copy_from_slice(program))
    }

    pub fn with_rom(build: impl FnOnce(&mut Vec<u8>)) -> Self {
        Self::with_rom_len(0x2_0000, build)
    }

    /// As [`Rig::with_rom`], over a ROM of `len` bytes.
    pub fn with_rom_len(len: usize, build: impl FnOnce(&mut Vec<u8>)) -> Self {
        let mut rom = vec![0u8; len];
        build(&mut rom);
        let rom = Rc::new(rom);
        let ram = Rc::new(RefCell::new(vec![0u8; 0x8000]));
        Self {
            gsu: Gsu::new(rom, Rc::clone(&ram)),
            ram,
            clocks: 0,
        }
    }

    pub fn write16(&mut self, offset: u16, value: u16) {
        self.gsu.write_register(offset, value as u8);
        self.gsu.write_register(offset + 1, (value >> 8) as u8);
    }

    pub fn read16(&mut self, offset: u16) -> u16 {
        let lo = self.gsu.read_register(offset).expect("register");
        let hi = self.gsu.read_register(offset + 1).expect("register");
        u16::from_le_bytes([lo, hi])
    }

    pub fn reg(&mut self, n: u16) -> u16 {
        self.read16(0x3000 + 2 * n)
    }

    pub fn set_reg(&mut self, n: u16, value: u16) {
        self.write16(0x3000 + 2 * n, value);
    }

    pub fn sfr(&mut self) -> u16 {
        // Read the low byte only through peek so the IRQ flag in the high byte survives.
        let lo = self.gsu.peek_register(0x3030).expect("SFR");
        let hi = self.gsu.peek_register(0x3031).expect("SFR");
        u16::from_le_bytes([lo, hi])
    }

    /// Hands the GSU both buses at 21 MHz and starts it at `pc` in bank `$00`.
    pub fn start_at(&mut self, pc: u16) {
        self.gsu.write_register(0x3039, 0x01);
        self.gsu.write_register(0x303A, SCMR_RON_RAN);
        self.write16(0x301E, pc);
    }

    pub fn tick(&mut self, clocks: u64) {
        for _ in 0..clocks {
            self.gsu.tick_one_master_clock();
            self.clocks += 1;
        }
    }

    /// Ticks until GO clears. Returns the master clocks it took.
    pub fn run_until_stop(&mut self) -> u64 {
        let start = self.clocks;
        while self.sfr() & 0x20 != 0 {
            assert!(self.clocks - start < RUN_LIMIT, "GSU program did not STOP");
            self.tick(1);
        }
        self.clocks - start
    }

    /// [`Rig::run_until_stop`], then enough idle clocks for a store still in the RAM write
    /// buffer to land.
    pub fn run_until_stop_and_settle(&mut self) {
        self.run_until_stop();
        self.tick(16);
    }

    /// Starts `program` at `$00:8000` and runs it to its STOP.
    pub fn run(program: &[u8]) -> Self {
        let mut rig = Self::new(program);
        rig.start_at(PROGRAM);
        rig.run_until_stop_and_settle();
        rig
    }
}

const SFR_Z: u16 = 1 << 1;
const SFR_GO: u16 = 1 << 5;
const SFR_IRQ: u16 = 1 << 15;

#[test]
fn writing_r15_high_byte_starts_the_gsu() {
    let mut rig = Rig::new(&[0x00, 0x01]); // STOP; NOP
    rig.gsu.write_register(0x301E, 0x00);
    assert_eq!(rig.sfr() & SFR_GO, 0, "the low byte only latches");
    rig.gsu.write_register(0x301F, 0x80);
    assert_ne!(rig.sfr() & SFR_GO, 0);
}

#[test]
fn stop_clears_go_sets_irq_and_leaves_r15_two_past_stop() {
    let mut rig = Rig::run(&[0x00, 0x01]); // STOP; NOP
    let sfr = rig.sfr();
    assert_eq!(sfr & SFR_GO, 0);
    assert_ne!(sfr & SFR_IRQ, 0);
    assert!(rig.gsu.irq_line());
    // fullsnes: "STOP at $+0 ... does then stop with R15=$+2".
    assert_eq!(rig.reg(15), PROGRAM + 2);
}

#[test]
fn stop_irq_is_masked_by_cfgr_bit7_but_flag_still_sets() {
    let mut rig = Rig::new(&[0x00, 0x01]);
    rig.gsu.write_register(0x3037, 0x80);
    rig.start_at(PROGRAM);
    rig.run_until_stop();
    assert_ne!(
        rig.sfr() & SFR_IRQ,
        0,
        "fullsnes: set even if IRQ is disabled in CFGR"
    );
    assert!(!rig.gsu.irq_line());
}

#[test]
fn reading_3031_clears_irq() {
    let mut rig = Rig::run(&[0x00, 0x01]);
    assert_ne!(rig.gsu.read_register(0x3031).unwrap() & 0x80, 0);
    assert_eq!(rig.gsu.read_register(0x3031).unwrap() & 0x80, 0);
    assert!(!rig.gsu.irq_line());
}

#[test]
fn iwt_and_ibt_load_registers() {
    #[rustfmt::skip]
    let mut rig = Rig::run(&[
        0xF1, 0x34, 0x12, // IWT R1,#$1234
        0xA2, 0x80,       // IBT R2,#-128
        0xA3, 0x7F,       // IBT R3,#127
        0x00, 0x01,       // STOP; NOP
    ]);
    assert_eq!(rig.reg(1), 0x1234);
    assert_eq!(rig.reg(2), 0xFF80, "IBT sign-extends");
    assert_eq!(rig.reg(3), 0x007F);
    assert_eq!(rig.sfr() & SFR_Z, 0, "MOV opcodes leave flags alone");
}

#[test]
fn gsu_does_not_run_before_go() {
    let mut rig = Rig::new(&[0xF1, 0x34, 0x12, 0x00, 0x01]);
    rig.tick(1000);
    assert_eq!(rig.reg(1), 0);
}
