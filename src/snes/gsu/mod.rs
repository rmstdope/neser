//! Super FX (GSU-1) coprocessor, built against fullsnes "SNES Cart GSU-n" (Graphic Support Unit).
//!
//! The GSU is a 16-bit RISC CPU on the cartridge with sixteen registers, its own view of the Game
//! Pak ROM and RAM, a 512-byte code cache, and a PLOT unit that draws pixels straight into SNES
//! bitplane tiles in Game Pak RAM. The S-CPU talks to it through `$3000-$34FF` and hands it the
//! ROM and RAM buses with the SCMR RON/RAN bits.
//!
//! fullsnes describes the instruction set and registers but leaves timing and several corners
//! open; Mesen2 (`Core/SNES/Coprocessors/GSU/`) is the implementation reference for those, and
//! each place that relies on it says so. The GSU runs by catch-up: every master clock the bus
//! calls [`Gsu::tick_one_master_clock`], and whole instructions execute while the GSU's own cycle
//! count is behind the master clock (as Mesen2's `Gsu::Run`). Costs are counted in master clocks:
//! one GSU cycle is one master clock at 21.4 MHz (CLSR bit 0 set) and two at 10.7 MHz.

mod core;
mod instructions;
pub(crate) mod memory;
mod plot;

use std::cell::RefCell;
use std::rc::Rc;

use serde::{Deserialize, Serialize};

/// Version code register (`$303B`). fullsnes knows `$01` for the MC1 ("Black Blob") and `$04`
/// for the GSU2; the MC1 is the only GSU-1-family value it records, and it is Star Fox's chip.
const VERSION_CODE: u8 = 0x01;

/// Opcode `NOP`, which fills the program prefetch at power-on and after STOP so that the first
/// instruction a (re)started GSU executes is a harmless one (Mesen2 `ProgramReadBuffer = 0x01`).
const NOP_OPCODE: u8 = 0x01;

fn default_program_prefetch() -> u8 {
    NOP_OPCODE
}

fn default_code_cache() -> Vec<u8> {
    vec![0; CODE_CACHE_SIZE]
}

/// The code cache: 512 bytes in 32 lines of 16 (fullsnes "Code-Cache").
const CODE_CACHE_SIZE: usize = 0x200;
const CODE_CACHE_LINES: usize = CODE_CACHE_SIZE / 16;

/// One eight-pixel row buffer of the PLOT unit (fullsnes "Pixel-Cache"): the row it belongs to
/// (X rounded down to 8, and Y), the pixels indexed by bit position (bit 7 = leftmost), and which
/// of them were plotted.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GsuPixelCache {
    pub x: u8,
    pub y: u8,
    pub pixels: [u8; 8],
    pub valid: u8,
}

/// All GSU state, which is also its save state.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct GsuState {
    /// R0-R15. R15 is the program counter.
    #[serde(default)]
    pub r: [u16; 16],
    // SFR (`$3030/$3031`).
    #[serde(default)]
    pub zero: bool,
    #[serde(default)]
    pub carry: bool,
    #[serde(default)]
    pub sign: bool,
    #[serde(default)]
    pub overflow: bool,
    /// SFR bit 5, GO: the GSU is running.
    #[serde(default)]
    pub go: bool,
    /// SFR bit 6, R: a ROM read via R14 is in flight.
    #[serde(default)]
    pub rom_read_pending: bool,
    #[serde(default)]
    pub alt1: bool,
    #[serde(default)]
    pub alt2: bool,
    /// SFR bit 12, B: set by WITH, turns the next `1n`/`Bn` into MOVE/MOVES.
    #[serde(default)]
    pub b_prefix: bool,
    /// SFR bit 15, IRQ: set by STOP, cleared by reading `$3031`.
    #[serde(default)]
    pub irq: bool,
    /// Low byte of an S-CPU register write, held until the odd (high) byte arrives.
    #[serde(default)]
    pub write_latch: u8,
    #[serde(default)]
    pub pbr: u8,
    #[serde(default)]
    pub rombr: u8,
    #[serde(default)]
    pub rambr: u8,
    /// CBR: the code-cache base, always a multiple of 16.
    #[serde(default)]
    pub cbr: u16,
    /// BRAMR bit 0. Kept for completeness; no GSU-1 board has the backup RAM it guards.
    #[serde(default)]
    pub bram_enabled: bool,
    /// CFGR (`$3037`): bit 5 MS0 (fast multiply), bit 7 IRQ mask.
    #[serde(default)]
    pub cfgr: u8,
    /// CLSR (`$3039`) bit 0: 21.4 MHz instead of 10.7 MHz.
    #[serde(default)]
    pub clock_21mhz: bool,
    /// SCBR (`$3038`): screen base in 1 KB units.
    #[serde(default)]
    pub scbr: u8,
    /// SCMR (`$303A`) as written: colour depth, height, RAN, RON.
    #[serde(default)]
    pub scmr: u8,
    /// POR, the plot option register set by CMODE.
    #[serde(default)]
    pub por: u8,
    /// COLR, the plot colour.
    #[serde(default)]
    pub colr: u8,
    /// Source and destination registers selected by FROM/TO/WITH (R0 when none).
    #[serde(default)]
    pub sreg: u8,
    #[serde(default)]
    pub dreg: u8,
    /// The ROM read buffer, prefetched from `[ROMBR:R14]` whenever R14 changes.
    #[serde(default)]
    pub rom_buffer: u8,
    /// Master clocks until the ROM read buffer fill completes.
    #[serde(default)]
    pub rom_delay: u8,
    /// The prefetched program byte: the opcode the next instruction executes.
    #[serde(default = "default_program_prefetch")]
    pub program_prefetch: u8,
    /// The RAM write buffer: a byte on its way to `[RAMBR:address]`.
    #[serde(default)]
    pub ram_write_address: u16,
    #[serde(default)]
    pub ram_write_value: u8,
    /// Master clocks until the buffered RAM write lands.
    #[serde(default)]
    pub ram_delay: u8,
    /// The most recently used RAM address, for SBK.
    #[serde(default)]
    pub last_ram_address: u16,
    #[serde(default)]
    pub primary_pixels: GsuPixelCache,
    #[serde(default)]
    pub secondary_pixels: GsuPixelCache,
    /// The code cache, indexed by GSU address bits 0-8.
    #[serde(default = "default_code_cache")]
    pub code_cache: Vec<u8>,
    #[serde(default)]
    pub code_cache_valid: [bool; CODE_CACHE_LINES],
    /// Master clocks the GSU has accounted for, and the master clocks elapsed; the GSU executes
    /// while the first is behind the second.
    #[serde(default)]
    pub cycle_count: u64,
    #[serde(default)]
    pub master_clock: u64,
    /// Halted on a ROM or RAM access while the S-CPU holds that bus (RON/RAN clear).
    #[serde(default)]
    pub waiting_for_rom: bool,
    #[serde(default)]
    pub waiting_for_ram: bool,
}

impl GsuState {
    fn power_on() -> Self {
        Self {
            program_prefetch: NOP_OPCODE,
            code_cache: default_code_cache(),
            ..Self::default()
        }
    }
}

/// The Super FX chip.
pub struct Gsu {
    state: GsuState,
    rom: Rc<Vec<u8>>,
    ram: Rc<RefCell<Vec<u8>>>,
    /// Set by an instruction that wrote R15, so the pipeline does not also advance it. Only
    /// meaningful within one `exec`.
    r15_changed: bool,
}

impl Gsu {
    /// A powered-on GSU over the cartridge ROM and the Game Pak RAM (which the bus also uses as
    /// the cartridge's save RAM).
    pub fn new(rom: Rc<Vec<u8>>, ram: Rc<RefCell<Vec<u8>>>) -> Self {
        Self {
            state: GsuState::power_on(),
            rom,
            ram,
            r15_changed: false,
        }
    }

    /// The GSU's IRQ output to the S-CPU: the SFR IRQ flag unless CFGR bit 7 masks it
    /// (fullsnes CFGR: "IRQ Interrupt Mask (0=Trigger IRQ on STOP opcode, 1=Disable IRQ)").
    pub fn irq_line(&self) -> bool {
        self.state.irq && self.state.cfgr & 0x80 == 0
    }

    /// Whether the GSU currently owns the ROM bus, so the S-CPU sees fixed vectors instead.
    pub fn snes_rom_blocked(&self) -> bool {
        self.state.go && self.ron()
    }

    /// Whether the GSU currently owns the RAM bus, so the S-CPU sees open bus instead.
    pub fn snes_ram_blocked(&self) -> bool {
        self.state.go && self.ran()
    }

    fn ron(&self) -> bool {
        self.state.scmr & 0x10 != 0
    }

    fn ran(&self) -> bool {
        self.state.scmr & 0x08 != 0
    }

    /// An S-CPU read of `$3000-$34FF` (offset within the bank). `None` is open bus.
    ///
    /// fullsnes says only SFR, SCMR and VCR "may be accessed" while the GSU runs; what the others
    /// return then is undocumented. They return their live value here, which is also what Mesen2
    /// effectively does (its read guard never fires for a non-FX3 chip).
    pub fn read_register(&mut self, offset: u16) -> Option<u8> {
        let value = self.peek_register(offset)?;
        if register_offset(offset) == Some(0x31) {
            // fullsnes: SFR bit 15 "IRQ ... reset on read".
            self.state.irq = false;
        }
        Some(value)
    }

    /// [`Gsu::read_register`] without its side effect (reading `$3031` clears IRQ), for the
    /// debugger.
    pub fn peek_register(&self, offset: u16) -> Option<u8> {
        if let Some(slot) = code_cache_window_slot(offset) {
            return Some(self.state.code_cache[slot]);
        }
        let s = &self.state;
        let value = match register_offset(offset)? {
            reg @ 0x00..=0x1F => {
                let word = s.r[usize::from(reg >> 1)];
                if reg & 1 == 0 {
                    word as u8
                } else {
                    (word >> 8) as u8
                }
            }
            0x30 => {
                u8::from(s.zero) << 1
                    | u8::from(s.carry) << 2
                    | u8::from(s.sign) << 3
                    | u8::from(s.overflow) << 4
                    | u8::from(s.go) << 5
                    | u8::from(s.rom_read_pending) << 6
            }
            // IL/IH (bits 2-3) are internal immediate-fetch flags that nothing can observe
            // mid-instruction here, so they read 0.
            0x31 => {
                u8::from(s.alt1)
                    | u8::from(s.alt2) << 1
                    | u8::from(s.b_prefix) << 4
                    | u8::from(s.irq) << 7
            }
            0x34 => s.pbr,
            0x36 => s.rombr,
            0x3B => VERSION_CODE,
            0x3C => s.rambr,
            0x3E => s.cbr as u8,
            0x3F => (s.cbr >> 8) as u8,
            // Unused and write-only ports. fullsnes: on the GSU2 they "return 00h"; the MC1
            // mirrors SFR there instead. Mesen2 returns 0, as here.
            _ => 0x00,
        };
        Some(value)
    }

    /// An S-CPU write to `$3000-$34FF` (offset within the bank).
    pub fn write_register(&mut self, offset: u16, value: u8) {
        let Some(reg) = register_offset(offset) else {
            if let Some(slot) = code_cache_window_slot(offset) {
                self.write_code_cache_window(slot, value);
            }
            return;
        };
        // fullsnes: "During GSU operation, only SFR, SCMR, and VCR may be accessed"; writes to
        // anything else while GO is set are dropped, as in Mesen2.
        if self.state.go && reg != 0x30 && reg != 0x3A {
            return;
        }
        match reg {
            0x00..=0x1F if reg & 1 == 0 => self.state.write_latch = value,
            0x00..=0x1F => {
                let index = usize::from(reg >> 1);
                let word = u16::from(value) << 8 | u16::from(self.state.write_latch);
                self.state.r[index] = word;
                match index {
                    // fullsnes leaves open whether an S-CPU write of R14 prefetches `[R14]` like
                    // a GSU one does; Mesen2 does, and so does this.
                    14 => self.start_rom_buffer_fill(),
                    // fullsnes: writing R15's MSB sets GO and starts execution.
                    15 => self.state.go = true,
                    _ => {}
                }
            }
            0x30 => self.write_sfr(value),
            0x33 => self.state.bram_enabled = value & 0x01 != 0,
            0x34 => {
                self.state.pbr = value;
                // Cached code belonged to the old bank (Mesen2 empties the cache here too).
                self.invalidate_all_code_cache_lines();
            }
            0x37 => self.state.cfgr = value,
            0x38 => self.state.scbr = value,
            0x39 => self.state.clock_21mhz = value & 0x01 != 0,
            0x3A => {
                self.state.scmr = value;
                if self.ron() {
                    self.state.waiting_for_rom = false;
                }
                if self.ran() {
                    self.state.waiting_for_ram = false;
                }
            }
            _ => {}
        }
    }

    fn write_sfr(&mut self, value: u8) {
        let s = &mut self.state;
        s.zero = value & 0x02 != 0;
        s.carry = value & 0x04 != 0;
        s.sign = value & 0x08 != 0;
        s.overflow = value & 0x10 != 0;
        s.go = value & 0x20 != 0;
        if !s.go {
            // fullsnes "Code-Cache": an S-CPU write of GO=0 sets CBR to 0 and marks every cache
            // line empty (how the S-CPU prepares to write code into the cache itself).
            s.cbr = 0;
            self.invalidate_all_code_cache_lines();
        }
    }

    fn write_code_cache_window(&mut self, slot: usize, value: u8) {
        if self.state.go {
            return;
        }
        self.state.code_cache[slot] = value;
        // fullsnes: "writing the last byte of a line at [3xxFh] will mark the line as not-empty".
        if slot & 0x0F == 0x0F {
            self.state.code_cache_valid[slot >> 4] = true;
        }
    }
}

/// The register a `$3000-$34FF` offset addresses, as `$00-$3F` of the `$3000-$303F` block.
/// fullsnes "Full I/O Map with Mirrors": the block repeats through `$3040-$30FF` and again at
/// `$3300-$34FF`; `$3100-$32FF` is the code cache.
fn register_offset(offset: u16) -> Option<u8> {
    match offset {
        0x3000..=0x30FF | 0x3300..=0x34FF => Some((offset & 0x3F) as u8),
        _ => None,
    }
}

/// The code-cache slot the S-CPU window `$3100-$32FF` addresses. fullsnes "Code-Cache": the slot
/// is the GSU address' low 9 bits ("SNES_Addr = (CBR AND 1FFh)+3100h" for the first cached byte).
fn code_cache_window_slot(offset: u16) -> Option<usize> {
    matches!(offset, 0x3100..=0x32FF).then(|| usize::from(offset - 0x3100))
}

#[cfg(test)]
mod alu_tests;
#[cfg(test)]
mod bus_tests;
#[cfg(test)]
mod core_tests;
#[cfg(test)]
mod flow_tests;
#[cfg(test)]
mod memory_tests;
