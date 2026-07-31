//! SA-1 enhancement chip: dual-CPU core, `$2200-$220F` control/vector registers, I-RAM, Super
//! MMC ROM banking, BW-RAM, cross-CPU IRQ/status handshake, and the read-only register block.
//!
//! Scope so far (issues #2957-#2961): a second, independently-clocked 65816 CPU core for SA-1,
//! reusing the existing generic [`Cpu`] unmodified, the SA-1 control/vector register block needed
//! to boot it, SA-1's 2KB I-RAM (see [`iram`]) with its per-CPU-side write protection, configurable
//! ROM banking, and BW-RAM mapping/write-protection (see [`memory_control`]); the cross-CPU
//! IRQ/status handshake (`$2300`/`$2301` SFR/CFR, SNV/SIV vector-override interception); and the
//! remaining read-only register block (`$2302-$230E`: H/V counter reads sharing the main [`Ppu`],
//! stubbed arithmetic-result/overflow and variable-length-data-port registers, and the
//! never-implemented-on-real-hardware `$230E` version code). Automating the absindx conformance
//! ROMs themselves is a separate sub-issue of #2956 that lands on top of this.
//!
//! Register bit layouts and reset values are sourced from fullsnes ("SNES Cart SA-1 I/O Map" /
//! "Interrupt/Control on SNES Side" / "Interrupt/Control on SA-1 Side" / "Memory Control" / "Timer"
//! / "Arithmetic Maths" / "Variable-Length Bit Processing" sections), per the
//! `snes-hardware-research` skill's source priority; the ROM-banking default LoROM-range behavior
//! and `$230E`'s open-bus-only nature are additionally cross-checked against bsnes since fullsnes's
//! own prose there is ambiguous or explicitly says "unknown" in isolation (see
//! [`memory_control::decode_rom_index`]'s doc comment and [`Sa1Bus`]'s doc comment respectively).

mod iram;
mod memory_control;

pub use iram::{Sa1IRam, decode_mirror_offset};
pub use memory_control::{
    Sa1MemoryControl, decode_direct_offset as decode_bwram_direct_offset, decode_rom_index,
    decode_windowed_offset as decode_bwram_windowed_offset,
};

use crate::platform::save_state::Stateful;
use crate::snes::bus::SnesBus;
use crate::snes::console::save_state::SnesCpuState;
use crate::snes::cpu::Cpu;
use crate::snes::ppu::Ppu;
use std::cell::RefCell;
use std::rc::Rc;

/// `$2200-$220F`: SA-1 CPU control and reset/NMI/IRQ vector registers.
///
/// All of these are write-only on real hardware (fullsnes lists them under "SA-1 I/O Map (Write
/// Only Registers)"), so there is deliberately no read path here -- reads of this range fall
/// through to open bus in [`crate::snes::bus::system_bus::SnesSystemBus`].
///
/// Interrupt pending/enable semantics (bits confirmed unambiguous by fullsnes but the write-time
/// *behavior* -- when a pending flag latches, when it's cleared, whether writing 0 to a trigger
/// bit is a no-op -- is only clearly specified by bsnes's `SA1::writeIOCPU`/`writeIOSA1`
/// (`sfc/coprocessor/sa1/io.cpp`), since fullsnes marks some of this "(0=No Change?)"):
/// - Writing a trigger bit as 1 (CCNT bit 4 NMI / bit 7 IRQ; SCNT bit 7 IRQ) always latches the
///   corresponding pending flag, regardless of the matching enable bit -- fullsnes: "When
///   interrupts are disabled (in CIE/SIE), then it sounds as if the interrupt flags still do get
///   set". Writing 0 to a trigger bit does *not* clear it; only the matching CIC/SIC bit does.
/// - IRQ is level-triggered: `pending && enabled` is asserted for as long as both hold, exactly
///   like the existing PPU IRQ line this bus already models via `poll_irq()`.
/// - NMI is edge-triggered like real 65816 hardware: the SA-1 CPU's actual dispatch happens once
///   per rising edge (a *new* CCNT bit 4 write, not "still set from before"), consumed via
///   `poll_nmi()` exactly like the existing PPU NMI edge -- independently of the CFR-visible
///   pending flag, which persists until CIC acknowledges it.
#[derive(Debug, Clone)]
pub struct Sa1ControlRegisters {
    /// `$2200` CCNT (SNES-writable). Bits 0-3: message SNES->SA-1. Bit 4: NMI SNES->SA-1. Bit
    /// 5: hold SA-1 in reset (1=reset, matches the `$20` power-on default). Bit 6: wait (freeze
    /// the SA-1 CPU). Bit 7: IRQ SNES->SA-1.
    ccnt: u8,
    /// `$2201` SIE (SNES-writable): SNES CPU interrupt enable bits. Bit 7 (IRQ from SA-1) is
    /// acted on; bit 5 (character-conversion DMA IRQ) is stored verbatim (DMA is out of scope).
    sie: u8,
    /// `$2203`/`$2204` CRV: SA-1 CPU reset vector. Fullsnes: "Exception Vectors on SA-1 side
    /// (these are ALWAYS replacing the normal vectors in ROM)".
    reset_vector: u16,
    /// `$2205`/`$2206` CNV: SA-1 CPU NMI vector (always replaces the ROM vector).
    nmi_vector: u16,
    /// `$2207`/`$2208` CIV: SA-1 CPU IRQ vector (always replaces the ROM vector).
    irq_vector: u16,
    /// `$2209` SCNT (SA-1-writable): SNES CPU control. Bit 7 (IRQ from SA-1) is acted on; bits
    /// 4/6 (NMI/IRQ vector-override switches) are acted on by `SnesSystemBus`'s vector
    /// interception, not here.
    scnt: u8,
    /// `$220A` CIE (SA-1-writable): SA-1 CPU interrupt enable bits. Bits 4/7 (NMI/IRQ from
    /// SNES) are acted on; bits 5/6 (DMA/timer IRQ) are stored verbatim (out of scope).
    cie: u8,
    /// `$220C`/`$220D` SNV: SNES CPU NMI vector override (optional, gated by `scnt` bit 4).
    snes_nmi_vector: u16,
    /// `$220E`/`$220F` SIV: SNES CPU IRQ vector override (optional, gated by `scnt` bit 6). The
    /// absindx RAM protection test repurposes this as a data side-channel: it writes an
    /// arbitrary byte here (not a real address) and relies on the main CPU's own IRQ handler
    /// reading `$00FFEE` (redirected here by the override) to retrieve it.
    snes_irq_vector: u16,
    /// SA-1-side IRQ-from-SNES pending flag (CFR bit 7), latched on CCNT bit 7 = 1, cleared on
    /// CIC bit 7 = 1.
    sa1_irq_pending: bool,
    /// `$220A` CIE bit 7: SA-1-side IRQ-from-SNES enable.
    sa1_irq_enabled: bool,
    /// SA-1-side NMI-from-SNES pending flag (CFR bit 4), latched on CCNT bit 4 = 1, cleared on
    /// CIC bit 4 = 1. Independent of the edge-consumed dispatch signal below.
    sa1_nmi_pending: bool,
    /// `$220A` CIE bit 4: SA-1-side NMI-from-SNES enable.
    sa1_nmi_enabled: bool,
    /// Edge-triggered NMI dispatch signal, consumed once by `Sa1Bus::poll_nmi()`. Set on every
    /// CCNT bit 4 = 1 write (a new edge), regardless of `sa1_nmi_enabled` -- matches real 65816
    /// NMI hardware, which latches the edge even while masked, then dispatches once unmasked.
    sa1_nmi_edge: bool,
    /// SNES-side IRQ-from-SA-1 pending flag (SFR bit 7), latched on SCNT bit 7 = 1, cleared on
    /// SIC bit 7 = 1.
    snes_irq_pending: bool,
    /// `$2201` SIE bit 7: SNES-side IRQ-from-SA-1 enable.
    snes_irq_enabled: bool,
    /// `$2302`/`$2303` HCR: the PPU dot position latched by the last `$2302` read (see
    /// [`Self::latch_hv_counter`]). Transient hardware state, like `sa1_nmi_edge` -- not
    /// persisted across save-states, since its pre-first-latch value is unobserved anyway.
    hcr: u16,
    /// `$2304`/`$2305` VCR: the PPU scanline latched alongside `hcr`.
    vcr: u16,
}

impl Sa1ControlRegisters {
    /// Hardware reset values (fullsnes "Reset" table): `$2200`=`$20` (SA-1 held in reset);
    /// everything else here resets to `$00` (the vector registers are individually listed as
    /// "N/A" at reset, i.e. undefined until written -- `$00` is a safe deterministic default
    /// since real software always writes them before releasing SA-1 from reset).
    pub fn new() -> Self {
        Self {
            ccnt: 0x20,
            sie: 0x00,
            reset_vector: 0x0000,
            nmi_vector: 0x0000,
            irq_vector: 0x0000,
            scnt: 0x00,
            cie: 0x00,
            snes_nmi_vector: 0x0000,
            snes_irq_vector: 0x0000,
            sa1_irq_pending: false,
            sa1_irq_enabled: false,
            sa1_nmi_pending: false,
            sa1_nmi_enabled: false,
            sa1_nmi_edge: false,
            snes_irq_pending: false,
            snes_irq_enabled: false,
            hcr: 0,
            vcr: 0,
        }
    }

    /// Dispatches a write to the raw `$2200-$220F` MMIO offset.
    pub fn write(&mut self, port: u16, value: u8) {
        match port {
            0x2200 => {
                self.ccnt = value;
                if value & 0x80 != 0 {
                    self.sa1_irq_pending = true;
                }
                if value & 0x10 != 0 {
                    self.sa1_nmi_pending = true;
                    self.sa1_nmi_edge = true;
                }
            }
            0x2201 => {
                self.sie = value;
                self.snes_irq_enabled = value & 0x80 != 0;
            }
            // $2202 SIC: SNES CPU interrupt-acknowledge strobe.
            0x2202 => {
                if value & 0x80 != 0 {
                    self.snes_irq_pending = false;
                }
            }
            0x2203 => self.reset_vector = (self.reset_vector & 0xFF00) | u16::from(value),
            0x2204 => self.reset_vector = (self.reset_vector & 0x00FF) | (u16::from(value) << 8),
            0x2205 => self.nmi_vector = (self.nmi_vector & 0xFF00) | u16::from(value),
            0x2206 => self.nmi_vector = (self.nmi_vector & 0x00FF) | (u16::from(value) << 8),
            0x2207 => self.irq_vector = (self.irq_vector & 0xFF00) | u16::from(value),
            0x2208 => self.irq_vector = (self.irq_vector & 0x00FF) | (u16::from(value) << 8),
            0x2209 => {
                self.scnt = value;
                if value & 0x80 != 0 {
                    self.snes_irq_pending = true;
                }
            }
            0x220A => {
                self.cie = value;
                self.sa1_irq_enabled = value & 0x80 != 0;
                self.sa1_nmi_enabled = value & 0x10 != 0;
            }
            // $220B CIC: SA-1 CPU interrupt-acknowledge strobe.
            0x220B => {
                if value & 0x80 != 0 {
                    self.sa1_irq_pending = false;
                }
                if value & 0x10 != 0 {
                    self.sa1_nmi_pending = false;
                }
            }
            0x220C => {
                self.snes_nmi_vector = (self.snes_nmi_vector & 0xFF00) | u16::from(value);
            }
            0x220D => {
                self.snes_nmi_vector = (self.snes_nmi_vector & 0x00FF) | (u16::from(value) << 8);
            }
            0x220E => {
                self.snes_irq_vector = (self.snes_irq_vector & 0xFF00) | u16::from(value);
            }
            0x220F => {
                self.snes_irq_vector = (self.snes_irq_vector & 0x00FF) | (u16::from(value) << 8);
            }
            _ => {}
        }
    }

    /// SA-1-side IRQ line (`pending && enabled`), polled every master clock like the existing
    /// PPU IRQ line.
    pub(crate) fn sa1_irq_line(&self) -> bool {
        self.sa1_irq_pending && self.sa1_irq_enabled
    }

    /// Consumes the edge-triggered SA-1-side NMI dispatch signal (see the struct doc comment).
    /// Does nothing while SA-1-side NMI is disabled (CIE bit 4) -- the edge stays latched so
    /// enabling NMI afterward still dispatches it, and a masked edge is never delivered to the
    /// SA-1 CPU. Does not affect the separately-tracked, CIC-acknowledged `sa1_nmi_pending` flag
    /// CFR exposes.
    pub(crate) fn take_sa1_nmi_edge(&mut self) -> bool {
        if !self.sa1_nmi_enabled {
            return false;
        }
        std::mem::take(&mut self.sa1_nmi_edge)
    }

    /// SNES-side IRQ line (`pending && enabled`).
    pub(crate) fn snes_irq_line(&self) -> bool {
        self.snes_irq_pending && self.snes_irq_enabled
    }

    /// `$2301` CFR: SA-1 CPU flag read. Bits 5/6 (DMA/timer IRQ) always read 0 -- neither is
    /// implemented (out of scope; see the module doc comment).
    pub(crate) fn cfr(&self) -> u8 {
        let mut value = self.ccnt & 0x0F; // message SNES->SA-1
        if self.sa1_nmi_pending {
            value |= 0x10;
        }
        if self.sa1_irq_pending {
            value |= 0x80;
        }
        value
    }

    /// `$2300` SFR: SNES CPU flag read. Bit 5 (character-conversion DMA IRQ) always reads 0 --
    /// not implemented (out of scope).
    pub(crate) fn sfr(&self) -> u8 {
        let mut value = self.scnt & 0x0F; // message SA-1->SNES
        if self.scnt & 0x10 != 0 {
            value |= 0x10; // cpu_nvsw (NMI vector override switch), mirrors SCNT bit 4
        }
        if self.scnt & 0x40 != 0 {
            value |= 0x40; // cpu_ivsw (IRQ vector override switch), mirrors SCNT bit 6
        }
        if self.snes_irq_pending {
            value |= 0x80;
        }
        value
    }

    /// `$2209` SCNT bit 4: NMI vector override switch (fullsnes: "NMI Vector for SNES (0=ROM
    /// FFEAh, 1=Port 220Ch)").
    pub(crate) fn snes_nmi_vector_override_enabled(&self) -> bool {
        self.scnt & 0x10 != 0
    }

    /// `$2209` SCNT bit 6: IRQ vector override switch (fullsnes: "IRQ Vector for SNES (0=ROM
    /// FFEEh, 1=Port 220Eh)"). The absindx RAM protection test relies on this to smuggle a data
    /// byte to the main CPU (see `snes_irq_vector`'s doc comment).
    pub(crate) fn snes_irq_vector_override_enabled(&self) -> bool {
        self.scnt & 0x40 != 0
    }

    /// CCNT bit 5: SA-1 CPU is held in reset (fullsnes: "Reset from SNES to SA-1 (0=No Reset,
    /// 1=Reset)"). The chip powers on with this set (`$20`) and stays halted until software
    /// clears it.
    pub fn is_held_in_reset(&self) -> bool {
        self.ccnt & 0b0010_0000 != 0
    }

    /// CCNT bit 6: SA-1 CPU is frozen ("Wait from SNES to SA-1") without losing reset state.
    pub fn is_waiting(&self) -> bool {
        self.ccnt & 0b0100_0000 != 0
    }

    pub fn reset_vector(&self) -> u16 {
        self.reset_vector
    }

    pub fn nmi_vector(&self) -> u16 {
        self.nmi_vector
    }

    pub fn irq_vector(&self) -> u16 {
        self.irq_vector
    }

    pub(crate) fn ccnt(&self) -> u8 {
        self.ccnt
    }

    pub(crate) fn sie(&self) -> u8 {
        self.sie
    }

    pub(crate) fn scnt(&self) -> u8 {
        self.scnt
    }

    pub(crate) fn cie(&self) -> u8 {
        self.cie
    }

    pub(crate) fn snes_nmi_vector(&self) -> u16 {
        self.snes_nmi_vector
    }

    pub(crate) fn snes_irq_vector(&self) -> u16 {
        self.snes_irq_vector
    }

    pub(crate) fn sa1_irq_pending(&self) -> bool {
        self.sa1_irq_pending
    }

    pub(crate) fn sa1_nmi_pending(&self) -> bool {
        self.sa1_nmi_pending
    }

    pub(crate) fn snes_irq_pending(&self) -> bool {
        self.snes_irq_pending
    }

    /// `$2302` HCR read side effect (fullsnes: "Reading from 2302h automatically latches the
    /// other HV-Counter bits to 2303h-2305h"): latches the PPU's current dot/scanline position.
    /// `dot` ranges 0-340 and `scanline` 0-311 (PAL) -- both fit comfortably in the 9-bit
    /// counters real hardware exposes here, so the unused upper bits always read 0.
    pub(crate) fn latch_hv_counter(&mut self, dot: u16, scanline: u16) {
        self.hcr = dot;
        self.vcr = scanline;
    }

    pub(crate) fn hcr_low(&self) -> u8 {
        (self.hcr & 0xFF) as u8
    }

    pub(crate) fn hcr_high(&self) -> u8 {
        (self.hcr >> 8) as u8
    }

    pub(crate) fn vcr_low(&self) -> u8 {
        (self.vcr & 0xFF) as u8
    }

    pub(crate) fn vcr_high(&self) -> u8 {
        (self.vcr >> 8) as u8
    }

    /// Restores every register and latched interrupt-line state to an exact value, for
    /// save-state loading. Unlike [`Self::write`], this bypasses per-port dispatch semantics
    /// (there's no "message" or "strobe" to interpret, and the pending/enabled flags can't be
    /// re-derived from the raw register bytes alone -- e.g. `ccnt`'s message nibble may have
    /// been overwritten since a still-pending IRQ trigger).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn restore_raw(
        &mut self,
        ccnt: u8,
        sie: u8,
        reset_vector: u16,
        nmi_vector: u16,
        irq_vector: u16,
        scnt: u8,
        cie: u8,
        snes_nmi_vector: u16,
        snes_irq_vector: u16,
        sa1_irq_pending: bool,
        sa1_nmi_pending: bool,
        snes_irq_pending: bool,
    ) {
        self.ccnt = ccnt;
        self.sie = sie;
        self.reset_vector = reset_vector;
        self.nmi_vector = nmi_vector;
        self.irq_vector = irq_vector;
        self.scnt = scnt;
        self.cie = cie;
        self.snes_nmi_vector = snes_nmi_vector;
        self.snes_irq_vector = snes_irq_vector;
        self.sa1_irq_pending = sa1_irq_pending;
        self.sa1_irq_enabled = cie & 0x80 != 0;
        self.sa1_nmi_pending = sa1_nmi_pending;
        self.sa1_nmi_enabled = cie & 0x10 != 0;
        self.sa1_nmi_edge = false; // transient dispatch signal; never persisted
        self.snes_irq_pending = snes_irq_pending;
        self.snes_irq_enabled = sie & 0x80 != 0;
        // Transient H/V-counter latch, also never persisted (see the field doc comment) --
        // cleared here too, since `restore_raw` runs on the live shared instance rather than a
        // fresh one, and would otherwise leak a stale pre-restore latch into $2303-$2305.
        self.hcr = 0;
        self.vcr = 0;
    }
}

impl Default for Sa1ControlRegisters {
    fn default() -> Self {
        Self::new()
    }
}

/// SA-1-side bus: serves the SA-1 CPU's reset/NMI/IRQ vectors from the control registers
/// instead of ROM, its 2KB I-RAM (direct `$0000-$07FF` and mirrored `$3000-$37FF`, gated by
/// `$222A` CIWP on writes), cartridge ROM through SA-1's configurable Super MMC mapping, and
/// BW-RAM (windowed `$6000-$7FFF`, gated by its own `$2225` BMAP block select, and direct
/// `$40-$4F`), gated by the shared write-protection rule (see [`memory_control`]).
///
/// `$2302-$2305` (HCR/VCR H/V counter reads) are also SA-1-side (#2961), sharing the main bus's
/// [`Ppu`] read-only to latch its live dot/scanline position. `$2306-$230B` (arithmetic
/// result/overflow) and `$230C`/`$230D` (variable-length data port) always read their
/// power-on-equivalent default of `0`, since the arithmetic and variable-length-bit units behind
/// them are out of scope (#2961's issue text defers their actual computation) -- only `$230E`
/// (SNES-side VC, confirmed by bsnes's `SA1::readIOCPU` to not exist on real hardware at all) and
/// genuinely unmapped offsets fall through to open bus.
pub struct Sa1Bus {
    registers: Rc<RefCell<Sa1ControlRegisters>>,
    iram: Rc<RefCell<Sa1IRam>>,
    memory_control: Rc<RefCell<Sa1MemoryControl>>,
    rom: Rc<Vec<u8>>,
    sram: Rc<RefCell<Vec<u8>>>,
    ppu: Rc<RefCell<Ppu>>,
}

impl Sa1Bus {
    pub fn new(
        registers: Rc<RefCell<Sa1ControlRegisters>>,
        iram: Rc<RefCell<Sa1IRam>>,
        memory_control: Rc<RefCell<Sa1MemoryControl>>,
        rom: Rc<Vec<u8>>,
        sram: Rc<RefCell<Vec<u8>>>,
        ppu: Rc<RefCell<Ppu>>,
    ) -> Self {
        Self {
            registers,
            iram,
            memory_control,
            rom,
            sram,
            ppu,
        }
    }

    /// Fullsnes: "IRQ/NMI/Reset vectors can be mapped. Other vectors (BRK/COP etc) are always
    /// taken from ROM (for BOTH CPUs)." -- so only these 5 vector-word addresses (reset has a
    /// single pair; NMI/IRQ each have both a native- and emulation-mode pair, both always
    /// replaced per "SNES Cart SA-1 Interrupt/Control on SNES Side") are intercepted.
    fn vector_override_byte(addr: u32, registers: &Sa1ControlRegisters) -> Option<u8> {
        let (vector, low_byte) = match addr {
            0x00_FFFC => (registers.reset_vector(), true),
            0x00_FFFD => (registers.reset_vector(), false),
            0x00_FFEA | 0x00_FFFA => (registers.nmi_vector(), true),
            0x00_FFEB | 0x00_FFFB => (registers.nmi_vector(), false),
            0x00_FFEE | 0x00_FFFE => (registers.irq_vector(), true),
            0x00_FFEF | 0x00_FFFF => (registers.irq_vector(), false),
            _ => return None,
        };
        Some(if low_byte {
            (vector & 0xFF) as u8
        } else {
            (vector >> 8) as u8
        })
    }

    /// True if `addr` is `system_offset` within a system bank (`$00-$3F`/`$80-$BF`), the same
    /// bank range the `$2200-$23FF` register block lives in.
    fn is_system_offset(addr: u32, system_offset: u16) -> bool {
        let bank = ((addr >> 16) & 0xFF) as u8;
        let offset = (addr & 0xFFFF) as u16;
        matches!(bank, 0x00..=0x3F | 0x80..=0xBF) && offset == system_offset
    }

    /// Like [`Self::is_system_offset`], but for a range of offsets; returns the matched offset.
    fn system_offset_in(addr: u32, range: std::ops::RangeInclusive<u16>) -> Option<u16> {
        let bank = ((addr >> 16) & 0xFF) as u8;
        let offset = (addr & 0xFFFF) as u16;
        if matches!(bank, 0x00..=0x3F | 0x80..=0xBF) && range.contains(&offset) {
            Some(offset)
        } else {
            None
        }
    }

    /// BW-RAM linear offset from the SA-1 CPU's own perspective: its `$2225` BMAP block select
    /// for the windowed `$6000-$7FFF` view, or the direct `$40-$5F` banks (twice as wide as the
    /// SNES side's `$40-$4F`; see [`memory_control::decode_sa1_direct_offset`]).
    fn bwram_index(&self, addr: u32) -> Option<usize> {
        let control = self.memory_control.borrow();
        if let Some(window_offset) = memory_control::decode_windowed_offset(addr) {
            Some(control.sa1_bwram_block() * 0x2000 + window_offset)
        } else {
            memory_control::decode_sa1_direct_offset(addr)
        }
    }
}

impl SnesBus for Sa1Bus {
    /// SA-1's CPU shares the main PPU's master clock, so its trace lines carry real stamps
    /// rather than the trait default's 0 -- otherwise an SA-1 title traced at `--trace-cpu=2`
    /// interleaves `clk=0` lines with the main CPU's real ones (or, with a clock window set,
    /// silently drops every SA-1 line), which makes an ordinal-aligned diff meaningless.
    fn master_clock(&self) -> u64 {
        self.ppu.borrow().total_master_clocks()
    }

    fn read(&self, addr: u32) -> u8 {
        let addr = addr & 0xFF_FFFF;
        if let Some(byte) = Self::vector_override_byte(addr, &self.registers.borrow()) {
            return byte;
        }
        if let Some(offset) =
            iram::decode_direct_offset(addr).or_else(|| iram::decode_mirror_offset(addr))
        {
            return self.iram.borrow().read(offset);
        }
        if let Some(index) = self.bwram_index(addr) {
            let sram = self.sram.borrow();
            return if sram.is_empty() {
                0
            } else {
                sram[index % sram.len()]
            };
        }
        // $2301 CFR is SA-1-side-readable (fullsnes I/O map "Side" column); the SNES-side $2300
        // SFR is read from `SnesSystemBus` instead.
        if Self::is_system_offset(addr, 0x2301) {
            return self.registers.borrow().cfr();
        }
        // $2302 HCR: reading it latches the PPU's current dot/scanline (fullsnes: "Reading from
        // 2302h automatically latches the other HV-Counter bits to 2303h-2305h"); $2303-$2305
        // just report the already-latched value without re-latching.
        if Self::is_system_offset(addr, 0x2302) {
            let position = self.ppu.borrow().position();
            let mut registers = self.registers.borrow_mut();
            registers.latch_hv_counter(position.dot, position.scanline);
            return registers.hcr_low();
        }
        if Self::is_system_offset(addr, 0x2303) {
            return self.registers.borrow().hcr_high();
        }
        if Self::is_system_offset(addr, 0x2304) {
            return self.registers.borrow().vcr_low();
        }
        if Self::is_system_offset(addr, 0x2305) {
            return self.registers.borrow().vcr_high();
        }
        // $2306-$230A (MR arithmetic result) and $230B (OF overflow flag) always read their
        // power-on-equivalent default of 0 -- the arithmetic unit behind them is out of scope
        // (#2961's issue text explicitly defers real multiply/divide/cumulative-sum computation
        // to a future issue).
        if Self::system_offset_in(addr, 0x2306..=0x230B).is_some() {
            return 0;
        }
        // $230C/$230D (VDP variable-length data read port) likewise always read 0 -- the
        // variable-length-bit unit behind them is out of scope (#2961's issue text explicitly
        // defers real VLB computation to a future issue).
        if Self::system_offset_in(addr, 0x230C..=0x230D).is_some() {
            return 0;
        }
        memory_control::decode_rom_index(addr, &self.memory_control.borrow())
            .and_then(|index| self.rom.get(index).copied())
            .unwrap_or(0)
    }

    fn write(&mut self, addr: u32, value: u8) {
        let addr = addr & 0xFF_FFFF;
        if let Some(offset) =
            iram::decode_direct_offset(addr).or_else(|| iram::decode_mirror_offset(addr))
        {
            self.iram.borrow_mut().write_from_sa1(offset, value);
            return;
        }
        if let Some(index) = self.bwram_index(addr) {
            let mut sram = self.sram.borrow_mut();
            let len = sram.len();
            if len != 0 {
                // Protection is checked against the *linear* bus offset, BEFORE physical-size
                // wrapping -- see the matching comment on `SnesSystemBus::write_sa1_bwram`.
                if !self.memory_control.borrow().is_bwram_write_protected(index) {
                    sram[index % len] = value;
                }
            }
            return;
        }
        // $2209-$220F (SCNT, CIE, CIC, SNV, SIV) are SA-1-side-writable (fullsnes I/O map "Side"
        // column); the SNES-side register block ($2200-$2208) is written from `SnesSystemBus`
        // instead -- see the module doc comment.
        if let Some(offset) = Self::system_offset_in(addr, 0x2209..=0x220F) {
            self.registers.borrow_mut().write(offset, value);
            return;
        }
        // $2225 BMAP and $2227 CBWE are SA-1-side-writable (fullsnes I/O map "Side" column); the
        // SNES-side register block ($2220-$2224, $2226, $2228) is written from `SnesSystemBus`
        // instead -- see the module doc comment.
        if Self::is_system_offset(addr, 0x2225) {
            self.memory_control.borrow_mut().write(0x2225, value);
            return;
        }
        if Self::is_system_offset(addr, 0x2227) {
            self.memory_control.borrow_mut().write(0x2227, value);
            return;
        }
        // $222A CIWP is SA-1-side-writable (fullsnes I/O map "Side" column); the SNES-side
        // register block ($2229 SIWP) is written from `SnesSystemBus` instead -- see the module
        // doc comment.
        if Self::is_system_offset(addr, 0x222A) {
            self.iram.borrow_mut().set_sa1_write_protect(value);
        }
        // ROM is read-only; BW-RAM writes land in #2959.
    }

    fn tick(&mut self) {
        // SA-1's own clock-domain bookkeeping lives on `Sa1Core`, not the bus (see the
        // module-level clock note): unlike `SnesSystemBus`, this bus has no PPU/APU/input of
        // its own to advance per tick.
    }

    /// Edge-triggered: consumes the SNES->SA-1 NMI dispatch signal set by a CCNT bit 4 = 1
    /// write. See [`Sa1ControlRegisters`]'s doc comment for why this is edge-based (real 65816
    /// NMI hardware) rather than the level-based `sa1_nmi_pending` flag CFR exposes.
    fn poll_nmi(&mut self) -> bool {
        self.registers.borrow_mut().take_sa1_nmi_edge()
    }

    /// Level-triggered: the SNES->SA-1 IRQ line stays asserted for as long as CCNT's IRQ-pending
    /// bit and CIE's IRQ-enable bit are both set (fullsnes/bsnes-confirmed; see
    /// [`Sa1ControlRegisters`]'s doc comment).
    fn poll_irq(&self) -> bool {
        self.registers.borrow().sa1_irq_line()
    }
}

/// Owns the second 65816 CPU core for SA-1 and reconciles its clock against the master-clock
/// tick loop driven by the main CPU's own bus accesses (see `SnesSystemBus::tick_one_master_clock`).
///
/// Clock model: SA-1's own CPU clock (10.74MHz, fullsnes) is uniformly fast, unlike the main
/// CPU's FastROM/SlowROM-gated access. Rather than modeling an independent SA-1 clock domain,
/// the SA-1 `Cpu` is constructed with `fast_rom` forced on (see [`Cpu::set_fast_rom`]), so
/// `Cpu::step()`'s returned cycle count is already expressed in the same "master clock ticks"
/// unit the main CPU uses internally -- [`Sa1Core::tick_one_master_clock`] treats that return
/// value directly as a master-clock debt rather than converting between two clock domains. This
/// is a deliberate simplification: real SA-1/SNES cycle-for-cycle bus arbitration (fullsnes:
/// "SA-1 CPU can access memory at 10.74MHz rate, or less if the SNES does simultaneously access
/// cartridge memory") is not modeled. Revisit if a conformance ROM proves timing-sensitive.
pub struct Sa1Core {
    cpu: Cpu<Sa1Bus>,
    registers: Rc<RefCell<Sa1ControlRegisters>>,
    /// Whether `do_reset()` has run since the last time SA-1 was held in reset. Cleared
    /// whenever CCNT's reset-hold bit is (re-)asserted, so releasing reset again re-boots.
    booted: bool,
    /// Master clocks already "spent" on the SA-1 instruction in flight; see the module-level
    /// clock note for why this is a direct debt rather than a fractional-budget conversion.
    master_clock_debt: i64,
}

impl Sa1Core {
    pub fn new(
        registers: Rc<RefCell<Sa1ControlRegisters>>,
        iram: Rc<RefCell<Sa1IRam>>,
        memory_control: Rc<RefCell<Sa1MemoryControl>>,
        rom: Rc<Vec<u8>>,
        sram: Rc<RefCell<Vec<u8>>>,
        ppu: Rc<RefCell<Ppu>>,
    ) -> Self {
        let bus = Sa1Bus::new(Rc::clone(&registers), iram, memory_control, rom, sram, ppu);
        let mut cpu = Cpu::new(bus);
        cpu.set_fast_rom(true);
        Self {
            cpu,
            registers,
            booted: false,
            master_clock_debt: 0,
        }
    }

    /// Advances SA-1 by one master clock. Must be called once per real master clock --
    /// including during `SnesSystemBus`'s DRAM-refresh stolen-clock replay -- or SA-1 drifts
    /// off the shared timeline (the same lesson already learned for the APU/PPU/input; see
    /// `SnesSystemBus::tick_one_master_clock`'s doc comment).
    pub fn tick_one_master_clock(&mut self) {
        if self.registers.borrow().is_held_in_reset() {
            self.booted = false;
            self.master_clock_debt = 0;
            return;
        }
        if !self.booted {
            self.cpu.do_reset();
            self.booted = true;
        }
        if self.registers.borrow().is_waiting() {
            return;
        }
        if self.master_clock_debt > 0 {
            self.master_clock_debt -= 1;
            return;
        }
        let cycles = i64::from(self.cpu.step());
        self.master_clock_debt = cycles.saturating_sub(1);
    }

    #[cfg(test)]
    pub(crate) fn cpu(&self) -> &Cpu<Sa1Bus> {
        &self.cpu
    }

    /// Captures the inner 65816 CPU's register/flag state, for save-state serialization. This
    /// reuses `Cpu<B>`'s existing bus-agnostic `Stateful` impl -- the same mechanism the main
    /// CPU already uses -- since `Cpu<Sa1Bus>`'s architectural state doesn't depend on the bus.
    pub(crate) fn cpu_state(&self) -> SnesCpuState {
        self.cpu.capture_state()
    }

    pub(crate) fn restore_cpu_state(&mut self, state: &SnesCpuState) {
        self.cpu.restore_state(state);
    }

    pub(crate) fn booted(&self) -> bool {
        self.booted
    }

    pub(crate) fn set_booted(&mut self, booted: bool) {
        self.booted = booted;
    }

    pub(crate) fn master_clock_debt(&self) -> i64 {
        self.master_clock_debt
    }

    pub(crate) fn set_master_clock_debt(&mut self, master_clock_debt: i64) {
        self.master_clock_debt = master_clock_debt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registers_with(
        setup: impl FnOnce(&mut Sa1ControlRegisters),
    ) -> Rc<RefCell<Sa1ControlRegisters>> {
        let mut registers = Sa1ControlRegisters::new();
        setup(&mut registers);
        Rc::new(RefCell::new(registers))
    }

    fn write_vector(registers: &mut Sa1ControlRegisters, lo_port: u16, hi_port: u16, value: u16) {
        registers.write(lo_port, (value & 0xFF) as u8);
        registers.write(hi_port, (value >> 8) as u8);
    }

    fn fresh_iram() -> Rc<RefCell<Sa1IRam>> {
        Rc::new(RefCell::new(Sa1IRam::new()))
    }

    fn fresh_memory_control() -> Rc<RefCell<Sa1MemoryControl>> {
        Rc::new(RefCell::new(Sa1MemoryControl::new()))
    }

    fn fresh_sram(size: usize) -> Rc<RefCell<Vec<u8>>> {
        Rc::new(RefCell::new(vec![0u8; size]))
    }

    fn fresh_ppu() -> Rc<RefCell<Ppu>> {
        Rc::new(RefCell::new(Ppu::new()))
    }

    #[test]
    fn new_control_registers_hold_sa1_in_reset_by_default() {
        let registers = Sa1ControlRegisters::new();
        assert!(registers.is_held_in_reset());
        assert!(!registers.is_waiting());
    }

    #[test]
    fn writing_ccnt_zero_releases_reset() {
        let mut registers = Sa1ControlRegisters::new();
        registers.write(0x2200, 0x00);
        assert!(!registers.is_held_in_reset());
    }

    #[test]
    fn writing_ccnt_wait_bit_freezes_without_clearing_reset() {
        let mut registers = Sa1ControlRegisters::new();
        registers.write(0x2200, 0b0110_0000); // reset + wait both set
        assert!(registers.is_held_in_reset());
        assert!(registers.is_waiting());
    }

    #[test]
    fn reset_nmi_irq_vectors_store_low_and_high_bytes() {
        let mut registers = Sa1ControlRegisters::new();
        write_vector(&mut registers, 0x2203, 0x2204, 0x9ABC);
        write_vector(&mut registers, 0x2205, 0x2206, 0x1234);
        write_vector(&mut registers, 0x2207, 0x2208, 0x5678);
        assert_eq!(registers.reset_vector(), 0x9ABC);
        assert_eq!(registers.nmi_vector(), 0x1234);
        assert_eq!(registers.irq_vector(), 0x5678);
    }

    #[test]
    fn restore_raw_clears_a_stale_hv_counter_latch() {
        let mut registers = Sa1ControlRegisters::new();
        registers.latch_hv_counter(123, 45); // simulate a pre-restore $2302 read
        assert_eq!(registers.hcr_low(), 123);
        assert_eq!(registers.vcr_low(), 45);

        registers.restore_raw(
            0x00, 0x00, 0x0000, 0x0000, 0x0000, 0x00, 0x00, 0x0000, 0x0000, false, false, false,
        );

        assert_eq!(
            registers.hcr_low(),
            0,
            "hcr must not leak a pre-restore latch"
        );
        assert_eq!(registers.hcr_high(), 0);
        assert_eq!(
            registers.vcr_low(),
            0,
            "vcr must not leak a pre-restore latch"
        );
        assert_eq!(registers.vcr_high(), 0);
    }

    #[test]
    fn snes_side_registers_store_verbatim_for_later_sub_issues() {
        let mut registers = Sa1ControlRegisters::new();
        registers.write(0x2201, 0xA0);
        registers.write(0x2209, 0x55);
        registers.write(0x220A, 0xF0);
        write_vector(&mut registers, 0x220C, 0x220D, 0x4242);
        write_vector(&mut registers, 0x220E, 0x220F, 0x8181);
        assert_eq!(registers.sie(), 0xA0);
        assert_eq!(registers.scnt(), 0x55);
        assert_eq!(registers.cie(), 0xF0);
        assert_eq!(registers.snes_nmi_vector(), 0x4242);
        assert_eq!(registers.snes_irq_vector(), 0x8181);
    }

    #[test]
    fn unhandled_offset_write_is_ignored() {
        let mut registers = Sa1ControlRegisters::new();
        registers.write(0x2202, 0xFF); // SIC strobe: no stored state
        registers.write(0x220B, 0xFF); // CIC strobe: no stored state
        registers.write(0x2299, 0xFF); // out of range for this block
        // None of the above touch CCNT bits 5/6, so the power-on reset/wait state must be
        // unchanged.
        assert!(registers.is_held_in_reset());
        assert!(!registers.is_waiting());
    }

    #[test]
    fn ccnt_irq_trigger_latches_pending_regardless_of_enable() {
        let mut registers = Sa1ControlRegisters::new();
        registers.write(0x2200, 0x80); // IRQ trigger bit, CIE not yet enabled
        assert!(registers.sa1_irq_pending());
        assert!(!registers.sa1_irq_line(), "not enabled yet");

        registers.write(0x220A, 0x80); // CIE: enable SA-1-side IRQ
        assert!(
            registers.sa1_irq_line(),
            "pending flag persists across the enable write"
        );
    }

    #[test]
    fn cic_clears_sa1_irq_pending() {
        let mut registers = Sa1ControlRegisters::new();
        registers.write(0x2200, 0x80);
        registers.write(0x220A, 0x80);
        assert!(registers.sa1_irq_line());

        registers.write(0x220B, 0x80); // CIC bit 7: acknowledge
        assert!(!registers.sa1_irq_line());
        assert!(!registers.sa1_irq_pending());
    }

    #[test]
    fn ccnt_nmi_trigger_sets_edge_and_persistent_pending_independently() {
        let mut registers = Sa1ControlRegisters::new();
        registers.write(0x220A, 0x10); // CIE bit 4: enable SA-1-side NMI
        registers.write(0x2200, 0x10); // NMI trigger bit
        assert!(registers.sa1_nmi_pending());
        assert!(registers.take_sa1_nmi_edge(), "edge consumed once");
        assert!(
            !registers.take_sa1_nmi_edge(),
            "edge does not re-fire without a new write"
        );
        // The CFR-visible pending flag survives the edge being consumed -- it's cleared only by
        // CIC, matching real 65816 NMI hardware (edge dispatch, level-persistent status flag).
        assert!(registers.sa1_nmi_pending());
    }

    #[test]
    fn take_sa1_nmi_edge_stays_latched_while_nmi_is_disabled_then_fires_once_enabled() {
        let mut registers = Sa1ControlRegisters::new();
        registers.write(0x2200, 0x10); // NMI trigger bit, CIE bit 4 not yet set
        assert!(
            !registers.take_sa1_nmi_edge(),
            "masked NMI must not be delivered to the SA-1 CPU"
        );

        registers.write(0x220A, 0x10); // CIE bit 4: enable SA-1-side NMI
        assert!(
            registers.take_sa1_nmi_edge(),
            "the edge must still be latched once NMI is enabled"
        );
        assert!(!registers.take_sa1_nmi_edge(), "edge consumed once");
    }

    #[test]
    fn cic_clears_sa1_nmi_pending_without_affecting_the_edge_signal() {
        let mut registers = Sa1ControlRegisters::new();
        registers.write(0x2200, 0x10);
        registers.write(0x220B, 0x10); // CIC bit 4: acknowledge NMI
        assert!(!registers.sa1_nmi_pending());
    }

    #[test]
    fn writing_ccnt_message_only_does_not_retrigger_irq_or_nmi() {
        // Mirrors the real ROM's `SetSnesStatus` (AND #$0F; STA CCNT): updates only the message
        // nibble, must not spuriously latch a new IRQ/NMI trigger.
        let mut registers = Sa1ControlRegisters::new();
        registers.write(0x2200, 0x05);
        assert!(!registers.sa1_irq_pending());
        assert!(!registers.sa1_nmi_pending());
        assert_eq!(registers.cfr() & 0x0F, 0x05);
    }

    #[test]
    fn scnt_irq_trigger_latches_snes_side_pending_gated_by_sie() {
        let mut registers = Sa1ControlRegisters::new();
        registers.write(0x2209, 0x80); // SCNT IRQ trigger, SIE not yet enabled
        assert!(registers.snes_irq_pending());
        assert!(!registers.snes_irq_line());

        registers.write(0x2201, 0x80); // SIE: enable SNES-side IRQ
        assert!(registers.snes_irq_line());

        registers.write(0x2202, 0x80); // SIC: acknowledge
        assert!(!registers.snes_irq_line());
        assert!(!registers.snes_irq_pending());
    }

    #[test]
    fn cfr_reports_message_and_both_pending_bits() {
        let mut registers = Sa1ControlRegisters::new();
        registers.write(0x2200, 0x90 | 0x0A); // NMI + IRQ triggers, message = $A
        assert_eq!(registers.cfr(), 0x90 | 0x0A);
    }

    #[test]
    fn sfr_reports_message_override_switches_and_pending() {
        let mut registers = Sa1ControlRegisters::new();
        // Bit 7 (IRQ trigger), bit 6 (IRQ vector override), bit 4 (NMI vector override),
        // message = 3.
        registers.write(0x2209, 0b1101_0011);
        assert_eq!(registers.sfr(), 0b1101_0011);
        assert!(registers.snes_nmi_vector_override_enabled());
        assert!(registers.snes_irq_vector_override_enabled());
    }

    #[test]
    fn bus_serves_reset_vector_instead_of_rom() {
        let registers = registers_with(|r| write_vector(r, 0x2203, 0x2204, 0x9000));
        let rom = Rc::new(vec![0u8; 0x8000]);
        let bus = Sa1Bus::new(
            registers,
            fresh_iram(),
            fresh_memory_control(),
            rom,
            fresh_sram(0),
            fresh_ppu(),
        );
        assert_eq!(bus.read(0x00_FFFC), 0x00);
        assert_eq!(bus.read(0x00_FFFD), 0x90);
    }

    #[test]
    fn bus_serves_nmi_and_irq_vectors_for_both_native_and_emulation_addresses() {
        let registers = registers_with(|r| {
            write_vector(r, 0x2205, 0x2206, 0x1234);
            write_vector(r, 0x2207, 0x2208, 0x5678);
        });
        let rom = Rc::new(vec![0u8; 0x8000]);
        let bus = Sa1Bus::new(
            registers,
            fresh_iram(),
            fresh_memory_control(),
            rom,
            fresh_sram(0),
            fresh_ppu(),
        );
        assert_eq!(bus.read(0x00_FFEA), 0x34);
        assert_eq!(bus.read(0x00_FFEB), 0x12);
        assert_eq!(bus.read(0x00_FFFA), 0x34);
        assert_eq!(bus.read(0x00_FFFB), 0x12);
        assert_eq!(bus.read(0x00_FFEE), 0x78);
        assert_eq!(bus.read(0x00_FFEF), 0x56);
        assert_eq!(bus.read(0x00_FFFE), 0x78);
        assert_eq!(bus.read(0x00_FFFF), 0x56);
    }

    #[test]
    fn bus_reads_rom_through_default_super_mmc_mapping() {
        let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
        let mut rom = vec![0u8; 0x8000];
        rom[0x0000] = 0xAA; // bank $00 offset $8000
        rom[0x1000] = 0xBB; // bank $00 offset $9000
        let bus = Sa1Bus::new(
            registers,
            fresh_iram(),
            fresh_memory_control(),
            Rc::new(rom),
            fresh_sram(0),
            fresh_ppu(),
        );
        assert_eq!(bus.read(0x00_8000), 0xAA);
        assert_eq!(bus.read(0x00_9000), 0xBB);
    }

    #[test]
    fn bus_returns_open_bus_zero_outside_mapped_rom_and_vectors() {
        let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
        let rom = Rc::new(vec![0xFFu8; 0x8000]);
        let bus = Sa1Bus::new(
            registers,
            fresh_iram(),
            fresh_memory_control(),
            rom,
            fresh_sram(0),
            fresh_ppu(),
        );
        // Bank $40 offset $0000 is not in the default Super MMC mapping, nor BW-RAM.
        assert_eq!(bus.read(0x40_0000), 0x00);
    }

    #[test]
    fn bus_2302_latches_the_shared_ppus_live_dot_and_scanline_into_hcr_vcr() {
        let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
        let rom = Rc::new(vec![0u8; 0x8000]);
        let ppu = fresh_ppu();
        // Advance the shared PPU so its dot/scanline position is non-trivial before latching.
        for _ in 0..100 {
            ppu.borrow_mut().tick();
        }
        let position = ppu.borrow().position();
        let bus = Sa1Bus::new(
            registers,
            fresh_iram(),
            fresh_memory_control(),
            rom,
            fresh_sram(0),
            Rc::clone(&ppu),
        );

        assert_eq!(bus.read(0x00_2302), (position.dot & 0xFF) as u8);
        assert_eq!(bus.read(0x00_2303), (position.dot >> 8) as u8);
        assert_eq!(bus.read(0x00_2304), (position.scanline & 0xFF) as u8);
        assert_eq!(bus.read(0x00_2305), (position.scanline >> 8) as u8);
    }

    #[test]
    fn bus_2303_2305_report_the_latch_without_re_latching_on_ppu_advance() {
        let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
        let rom = Rc::new(vec![0u8; 0x8000]);
        let ppu = fresh_ppu();
        let bus = Sa1Bus::new(
            registers,
            fresh_iram(),
            fresh_memory_control(),
            rom,
            fresh_sram(0),
            Rc::clone(&ppu),
        );

        bus.read(0x00_2302); // latch at dot 0
        for _ in 0..500 {
            ppu.borrow_mut().tick();
        }
        // The PPU has moved on, but $2303-$2305 must still report the original latch.
        assert_eq!(bus.read(0x00_2303), 0);
        assert_eq!(bus.read(0x00_2304), 0);
        assert_eq!(bus.read(0x00_2305), 0);
    }

    #[test]
    fn bus_arithmetic_result_overflow_and_variable_length_data_port_default_to_zero() {
        // The arithmetic and variable-length-bit units behind these registers are out of scope
        // for #2961 (its issue text defers real computation to a future issue) -- they must
        // still read back a plausible power-on-equivalent default rather than crashing or
        // falling through to open bus.
        let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
        let rom = Rc::new(vec![0u8; 0x8000]);
        let bus = Sa1Bus::new(
            registers,
            fresh_iram(),
            fresh_memory_control(),
            rom,
            fresh_sram(0),
            fresh_ppu(),
        );

        for port in 0x2306..=0x230D {
            assert_eq!(bus.read(port), 0x00, "port ${port:04X} should default to 0");
        }
    }

    #[test]
    fn bus_reads_iram_through_both_direct_and_mirror_windows() {
        let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
        let iram = fresh_iram();
        iram.borrow_mut().set_sa1_write_protect(0xFF);
        iram.borrow_mut().write_from_sa1(0x0010, 0x7E);
        let rom = Rc::new(vec![0u8; 0x8000]);
        let bus = Sa1Bus::new(
            registers,
            Rc::clone(&iram),
            fresh_memory_control(),
            rom,
            fresh_sram(0),
            fresh_ppu(),
        );
        assert_eq!(bus.read(0x00_0010), 0x7E); // direct window
        assert_eq!(bus.read(0x00_3010), 0x7E); // mirror window
    }

    #[test]
    fn bus_write_honors_sa1_side_iram_protection() {
        let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
        let iram = fresh_iram();
        let rom = Rc::new(vec![0u8; 0x8000]);
        let mut bus = Sa1Bus::new(
            registers,
            Rc::clone(&iram),
            fresh_memory_control(),
            rom,
            fresh_sram(0),
            fresh_ppu(),
        );
        bus.write(0x00_0000, 0x99); // blocked: CIWP is $00 at reset
        assert_eq!(iram.borrow().read(0x0000), 0x00);

        iram.borrow_mut().set_sa1_write_protect(0x01);
        bus.write(0x00_3000, 0xAB); // via mirror window this time
        assert_eq!(iram.borrow().read(0x0000), 0xAB);
    }

    #[test]
    fn bus_reads_and_writes_bwram_through_the_windowed_view_using_bmap() {
        let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
        let memory_control = fresh_memory_control();
        memory_control.borrow_mut().write(0x2228, 0x00); // shrink BWPA protection
        let rom = Rc::new(vec![0u8; 0x8000]);
        let sram = fresh_sram(0x1_0000);
        let mut bus = Sa1Bus::new(
            registers,
            fresh_iram(),
            Rc::clone(&memory_control),
            rom,
            Rc::clone(&sram),
            fresh_ppu(),
        );

        bus.write(0x2225, 0x02); // BMAP: block 2 (SA-1-writable, routed through Sa1Bus)
        bus.write(0x00_6010, 0x5A);
        assert_eq!(sram.borrow()[0x4010], 0x5A); // block 2 * 0x2000 + 0x10
        assert_eq!(bus.read(0x00_6010), 0x5A);
    }

    #[test]
    fn bus_reads_and_writes_bwram_through_the_direct_banks() {
        let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
        let memory_control = fresh_memory_control();
        memory_control.borrow_mut().write(0x2227, 0x80); // CBWE: SA-1 side enables writes
        let rom = Rc::new(vec![0u8; 0x8000]);
        let sram = fresh_sram(0x2_0000);
        let mut bus = Sa1Bus::new(
            registers,
            fresh_iram(),
            memory_control,
            rom,
            sram,
            fresh_ppu(),
        );

        bus.write(0x41_0000, 0x33);
        assert_eq!(bus.read(0x41_0000), 0x33);

        // The SA-1 side's direct window extends through bank $5F (unlike the SNES side's
        // $40-$4F): a write to $500000 lands at linear offset $100000, which mirrors onto this
        // 128KB backing at physical offset 0 -- absindx TEST ID 160's expected behavior.
        bus.write(0x50_0000, 0x44);
        assert_eq!(bus.read(0x40_0000), 0x44);
        assert_eq!(bus.read(0x50_0000), 0x44);
    }

    #[test]
    fn bus_bwram_write_honors_the_shared_protection_rule() {
        let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
        let memory_control = fresh_memory_control(); // BWPA=$FF, SBWE=CBWE=0 at reset: protected
        let rom = Rc::new(vec![0u8; 0x8000]);
        let sram = fresh_sram(0x1_0000);
        let mut bus = Sa1Bus::new(
            registers,
            fresh_iram(),
            memory_control,
            rom,
            Rc::clone(&sram),
            fresh_ppu(),
        );

        bus.write(0x00_6000, 0x77);
        assert_eq!(sram.borrow()[0], 0x00);
    }

    #[test]
    fn bus_bwram_write_protection_is_checked_against_the_linear_bus_offset_before_wrapping() {
        // Same scenario as the SnesSystemBus-level test of the same name: an 8KB-per-block
        // BW-RAM with only 0x8000 (32KB, blocks 0-3) of physical backing, block 4 wrapping to
        // physical offset 0. BWPA's comparator sees the *linear* offset (0x8000, outside its
        // 256-byte range), so the write goes through and lands on physical offset 0 via the RAM
        // chip's own wraparound -- absindx `SA1RamProtectionTest` TEST ID 50's documented
        // hardware behavior (see that test's doc comment for the full derivation).
        let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
        let memory_control = fresh_memory_control();
        memory_control.borrow_mut().write(0x2228, 0x00); // protect only the first 256 bytes
        memory_control.borrow_mut().write(0x2225, 0x04); // BMAP: block 4
        let rom = Rc::new(vec![0u8; 0x8000]);
        let sram = fresh_sram(0x8000);
        let mut bus = Sa1Bus::new(
            registers,
            fresh_iram(),
            memory_control,
            rom,
            Rc::clone(&sram),
            fresh_ppu(),
        );

        bus.write(0x00_6000, 0x42);
        assert_eq!(
            sram.borrow()[0],
            0x42,
            "linear offset $8000 is outside BWPA's range, so the write lands (wrapped)"
        );
        // A write addressed inside the protected linear range is still blocked.
        bus.write(0x40_0000, 0x99);
        assert_eq!(sram.borrow()[0], 0x42);
    }

    #[test]
    fn core_stays_halted_while_held_in_reset() {
        let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
        let rom = Rc::new(vec![0u8; 0x8000]);
        let mut core = Sa1Core::new(
            Rc::clone(&registers),
            fresh_iram(),
            fresh_memory_control(),
            rom,
            fresh_sram(0),
            fresh_ppu(),
        );
        for _ in 0..1000 {
            core.tick_one_master_clock();
        }
        assert_eq!(core.cpu().read_pc(), 0);
    }

    #[test]
    fn core_boots_and_executes_once_released_from_reset() {
        let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
        let mut rom = vec![0u8; 0x8000];
        // Program at bank $00 offset $9000 (rom index $1000): SEI; LDA #$42; self-branch.
        rom[0x1000] = 0x78; // SEI
        rom[0x1001] = 0xA9; // LDA #imm
        rom[0x1002] = 0x42;
        rom[0x1003] = 0x80; // BRA -2 (infinite self-loop)
        rom[0x1004] = 0xFE;
        {
            let mut regs = registers.borrow_mut();
            write_vector(&mut regs, 0x2203, 0x2204, 0x9000);
        }
        let mut core = Sa1Core::new(
            Rc::clone(&registers),
            fresh_iram(),
            fresh_memory_control(),
            Rc::new(rom),
            fresh_sram(0),
            fresh_ppu(),
        );
        registers.borrow_mut().write(0x2200, 0x00); // release reset

        for _ in 0..100 {
            core.tick_one_master_clock();
        }

        assert_eq!(core.cpu().read_a() & 0xFF, 0x42);
        assert_eq!(core.cpu().read_pc(), 0x9003);
    }

    #[test]
    fn core_reboots_if_reset_is_reasserted_and_released_again() {
        let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
        let mut rom = vec![0u8; 0x8000];
        rom[0x1000] = 0xA9; // LDA #imm
        rom[0x1001] = 0x11;
        rom[0x1002] = 0x80; // BRA -2
        rom[0x1003] = 0xFE;
        {
            let mut regs = registers.borrow_mut();
            write_vector(&mut regs, 0x2203, 0x2204, 0x9000);
        }
        let mut core = Sa1Core::new(
            Rc::clone(&registers),
            fresh_iram(),
            fresh_memory_control(),
            Rc::new(rom),
            fresh_sram(0),
            fresh_ppu(),
        );
        registers.borrow_mut().write(0x2200, 0x00);
        for _ in 0..50 {
            core.tick_one_master_clock();
        }
        assert_eq!(core.cpu().read_a() & 0xFF, 0x11);

        // Re-assert reset, then release again: SA-1 must re-run do_reset().
        registers.borrow_mut().write(0x2200, 0x20);
        core.tick_one_master_clock();
        registers.borrow_mut().write(0x2200, 0x00);
        for _ in 0..50 {
            core.tick_one_master_clock();
        }
        // This fixture has no leading SEI (unlike the boot test above), so its 2-byte
        // `LDA #imm` + 2-byte `BRA -2` settle the infinite self-loop at $9002, not $9003.
        assert_eq!(core.cpu().read_pc(), 0x9002);
    }

    #[test]
    fn core_dispatches_an_nmi_from_snes_once_enabled() {
        let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
        let mut rom = vec![0u8; 0x8000];
        // SA-1 boot at $9000: unlock I-RAM (its own stack lives there and is write-protected by
        // default -- see $222A CIWP's `$00` reset value), unmask interrupts (CLI), then idle.
        rom[0x1000] = 0xA9; // LDA #$FF
        rom[0x1001] = 0xFF;
        rom[0x1002] = 0x8D; // STA $222A (CIWP: unlock all I-RAM chunks)
        rom[0x1003] = 0x2A;
        rom[0x1004] = 0x22;
        rom[0x1005] = 0x58; // CLI
        rom[0x1006] = 0x4C; // JMP $9006 (self)
        rom[0x1007] = 0x06;
        rom[0x1008] = 0x90;
        // NMI handler at $9100: LDA #$99; RTI.
        rom[0x1100] = 0xA9;
        rom[0x1101] = 0x99;
        rom[0x1102] = 0x40; // RTI
        {
            let mut regs = registers.borrow_mut();
            write_vector(&mut regs, 0x2203, 0x2204, 0x9000); // CRV
            write_vector(&mut regs, 0x2205, 0x2206, 0x9100); // CNV
        }
        let mut core = Sa1Core::new(
            Rc::clone(&registers),
            fresh_iram(),
            fresh_memory_control(),
            Rc::new(rom),
            fresh_sram(0),
            fresh_ppu(),
        );
        registers.borrow_mut().write(0x220A, 0x10); // CIE: enable SA-1-side NMI
        registers.borrow_mut().write(0x2200, 0x00); // release reset
        for _ in 0..50 {
            core.tick_one_master_clock();
        }
        assert_eq!(core.cpu().read_pc(), 0x9006, "idling, waiting for the NMI");

        registers.borrow_mut().write(0x2200, 0x10); // CCNT: NMI trigger
        for _ in 0..300 {
            core.tick_one_master_clock();
        }

        assert_eq!(
            core.cpu().read_a() & 0xFF,
            0x99,
            "NMI handler must have run"
        );
        assert_eq!(core.cpu().read_pc(), 0x9006, "RTI returns to the idle loop");
    }
}
