//! SA-1 enhancement chip: dual-CPU core, `$2200-$220F` control/vector registers, and I-RAM.
//!
//! Scope so far (issues #2957, #2958): a second, independently-clocked 65816 CPU core for SA-1,
//! reusing the existing generic [`Cpu`] unmodified, the SA-1 control/vector register block
//! needed to boot it, and SA-1's 2KB I-RAM (see [`iram`]) with its per-CPU-side write
//! protection. BW-RAM, the cross-CPU IRQ/status handshake, and the version-code/read-only
//! register block are separate sub-issues of #2956 and land on top of this.
//!
//! Register bit layouts and reset values are sourced from fullsnes ("SNES Cart SA-1 I/O Map" /
//! "Interrupt/Control on SNES Side" / "Interrupt/Control on SA-1 Side" / "Memory Control"
//! sections), per the `snes-hardware-research` skill's source priority.

mod iram;

pub use iram::{Sa1IRam, decode_mirror_offset};

use crate::platform::save_state::Stateful;
use crate::snes::bus::SnesBus;
use crate::snes::console::save_state::SnesCpuState;
use crate::snes::cpu::Cpu;
use std::cell::RefCell;
use std::rc::Rc;

/// `$2200-$220F`: SA-1 CPU control and reset/NMI/IRQ vector registers.
///
/// All of these are write-only on real hardware (fullsnes lists them under "SA-1 I/O Map (Write
/// Only Registers)"), so there is deliberately no read path here -- reads of this range fall
/// through to open bus in [`crate::snes::bus::system_bus::SnesSystemBus`].
#[derive(Debug, Clone)]
pub struct Sa1ControlRegisters {
    /// `$2200` CCNT (SNES-writable). Bits 0-3: message SNES->SA-1. Bit 4: NMI SNES->SA-1. Bit
    /// 5: hold SA-1 in reset (1=reset, matches the `$20` power-on default). Bit 6: wait (freeze
    /// the SA-1 CPU). Bit 7: IRQ SNES->SA-1.
    ccnt: u8,
    /// `$2201` SIE (SNES-writable): SNES CPU interrupt enable bits. Stored verbatim; behavior
    /// lands with the cross-CPU IRQ handshake (#2960).
    sie: u8,
    /// `$2203`/`$2204` CRV: SA-1 CPU reset vector. Fullsnes: "Exception Vectors on SA-1 side
    /// (these are ALWAYS replacing the normal vectors in ROM)".
    reset_vector: u16,
    /// `$2205`/`$2206` CNV: SA-1 CPU NMI vector (always replaces the ROM vector).
    nmi_vector: u16,
    /// `$2207`/`$2208` CIV: SA-1 CPU IRQ vector (always replaces the ROM vector).
    irq_vector: u16,
    /// `$2209` SCNT (SA-1-writable): SNES CPU control. Stored verbatim; behavior lands with #2960.
    scnt: u8,
    /// `$220A` CIE (SA-1-writable): SA-1 CPU interrupt enable bits. Stored verbatim; behavior
    /// lands with #2960.
    cie: u8,
    /// `$220C`/`$220D` SNV: SNES CPU NMI vector override (optional, gated by `scnt` bit 4 per
    /// fullsnes; the gating itself is #2960's job). Stored verbatim here.
    snes_nmi_vector: u16,
    /// `$220E`/`$220F` SIV: SNES CPU IRQ vector override (optional, gated by `scnt` bit 6).
    /// Stored verbatim here.
    snes_irq_vector: u16,
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
        }
    }

    /// Dispatches a write to the raw `$2200-$220F` MMIO offset.
    pub fn write(&mut self, port: u16, value: u8) {
        match port {
            0x2200 => self.ccnt = value,
            0x2201 => self.sie = value,
            // $2202 SIC: SNES CPU interrupt-acknowledge strobe. No persistent state of its own;
            // applying its clear effect to pending-IRQ status lands with #2960.
            0x2202 => {}
            0x2203 => self.reset_vector = (self.reset_vector & 0xFF00) | u16::from(value),
            0x2204 => self.reset_vector = (self.reset_vector & 0x00FF) | (u16::from(value) << 8),
            0x2205 => self.nmi_vector = (self.nmi_vector & 0xFF00) | u16::from(value),
            0x2206 => self.nmi_vector = (self.nmi_vector & 0x00FF) | (u16::from(value) << 8),
            0x2207 => self.irq_vector = (self.irq_vector & 0xFF00) | u16::from(value),
            0x2208 => self.irq_vector = (self.irq_vector & 0x00FF) | (u16::from(value) << 8),
            0x2209 => self.scnt = value,
            0x220A => self.cie = value,
            // $220B CIC: SA-1 CPU interrupt-acknowledge strobe; same as $2202 above.
            0x220B => {}
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

    /// Restores every register to an exact byte value, for save-state loading. Unlike
    /// [`Self::write`], this bypasses per-port dispatch semantics (there's no "message" or
    /// "strobe" to interpret -- just raw state to reinstate).
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
    }
}

impl Default for Sa1ControlRegisters {
    fn default() -> Self {
        Self::new()
    }
}

/// SA-1-side bus: serves the SA-1 CPU's reset/NMI/IRQ vectors from the control registers
/// instead of ROM, its 2KB I-RAM (direct `$0000-$07FF` and mirrored `$3000-$37FF`, gated by
/// `$222A` CIWP on writes), and cartridge ROM through SA-1's default (pre-#2959) Super MMC
/// mapping.
///
/// BW-RAM and the rest of the `$2200-$230E` register block are not yet readable/writable from
/// the SA-1 side (#2959/#2961); all other reads return open bus (`0`) and writes are no-ops.
pub struct Sa1Bus {
    registers: Rc<RefCell<Sa1ControlRegisters>>,
    iram: Rc<RefCell<Sa1IRam>>,
    rom: Rc<Vec<u8>>,
}

impl Sa1Bus {
    pub fn new(
        registers: Rc<RefCell<Sa1ControlRegisters>>,
        iram: Rc<RefCell<Sa1IRam>>,
        rom: Rc<Vec<u8>>,
    ) -> Self {
        Self {
            registers,
            iram,
            rom,
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

    /// Default (pre-#2959) Super MMC mapping: CXB/DXB/EXB/FXB left at their hardware reset
    /// values (`$00/$01/$02/$03`, i.e. ROM slots 0-3 in order, fullsnes "Reset" table) with no
    /// remapping -- equivalent to a plain LoROM decode for banks `$00-$3F`/`$80-$BF` at offset
    /// `$8000-$FFFF` (fullsnes "Memory Map (SA-1 Side)": "Four mappable 1MByte LoROM blocks").
    /// Configurable banking via `$2220-$2223` (including the HiROM-only vs LoROM+HiROM bit 7
    /// distinction) lands in #2959.
    fn decode_rom_index(addr: u32) -> Option<usize> {
        let addr = addr & 0xFF_FFFF;
        let bank = ((addr >> 16) & 0xFF) as u8;
        let offset = (addr & 0xFFFF) as u16;
        if matches!(bank, 0x00..=0x3F | 0x80..=0xBF) && offset >= 0x8000 {
            let bank_index = (bank & 0x7F) as usize;
            Some(bank_index * 0x8000 + (offset as usize - 0x8000))
        } else {
            None
        }
    }

    /// True if `addr` is `system_offset` within a system bank (`$00-$3F`/`$80-$BF`), the same
    /// bank range the `$2200-$23FF` register block lives in.
    fn is_system_offset(addr: u32, system_offset: u16) -> bool {
        let bank = ((addr >> 16) & 0xFF) as u8;
        let offset = (addr & 0xFFFF) as u16;
        matches!(bank, 0x00..=0x3F | 0x80..=0xBF) && offset == system_offset
    }
}

impl SnesBus for Sa1Bus {
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
        Self::decode_rom_index(addr)
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
        // $222A CIWP is SA-1-side-writable (fullsnes I/O map "Side" column); the SNES-side
        // register block ($2200-$220F, $2229 SIWP) is written from `SnesSystemBus` instead --
        // see the module doc comment.
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
        rom: Rc<Vec<u8>>,
    ) -> Self {
        let bus = Sa1Bus::new(Rc::clone(&registers), iram, rom);
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
    fn bus_serves_reset_vector_instead_of_rom() {
        let registers = registers_with(|r| write_vector(r, 0x2203, 0x2204, 0x9000));
        let rom = Rc::new(vec![0u8; 0x8000]);
        let bus = Sa1Bus::new(registers, fresh_iram(), rom);
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
        let bus = Sa1Bus::new(registers, fresh_iram(), rom);
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
    fn bus_reads_rom_through_default_lorom_shaped_mapping() {
        let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
        let mut rom = vec![0u8; 0x8000];
        rom[0x0000] = 0xAA; // bank $00 offset $8000
        rom[0x1000] = 0xBB; // bank $00 offset $9000
        let bus = Sa1Bus::new(registers, fresh_iram(), Rc::new(rom));
        assert_eq!(bus.read(0x00_8000), 0xAA);
        assert_eq!(bus.read(0x00_9000), 0xBB);
    }

    #[test]
    fn bus_returns_open_bus_zero_outside_mapped_rom_and_vectors() {
        let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
        let rom = Rc::new(vec![0xFFu8; 0x8000]);
        let bus = Sa1Bus::new(registers, fresh_iram(), rom);
        // Bank $40 offset $0000 is not in the default LoROM-shaped window.
        assert_eq!(bus.read(0x40_0000), 0x00);
    }

    #[test]
    fn bus_reads_iram_through_both_direct_and_mirror_windows() {
        let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
        let iram = fresh_iram();
        iram.borrow_mut().set_sa1_write_protect(0xFF);
        iram.borrow_mut().write_from_sa1(0x0010, 0x7E);
        let rom = Rc::new(vec![0u8; 0x8000]);
        let bus = Sa1Bus::new(registers, Rc::clone(&iram), rom);
        assert_eq!(bus.read(0x00_0010), 0x7E); // direct window
        assert_eq!(bus.read(0x00_3010), 0x7E); // mirror window
    }

    #[test]
    fn bus_write_honors_sa1_side_iram_protection() {
        let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
        let iram = fresh_iram();
        let rom = Rc::new(vec![0u8; 0x8000]);
        let mut bus = Sa1Bus::new(registers, Rc::clone(&iram), rom);
        bus.write(0x00_0000, 0x99); // blocked: CIWP is $00 at reset
        assert_eq!(iram.borrow().read(0x0000), 0x00);

        iram.borrow_mut().set_sa1_write_protect(0x01);
        bus.write(0x00_3000, 0xAB); // via mirror window this time
        assert_eq!(iram.borrow().read(0x0000), 0xAB);
    }

    #[test]
    fn core_stays_halted_while_held_in_reset() {
        let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
        let rom = Rc::new(vec![0u8; 0x8000]);
        let mut core = Sa1Core::new(Rc::clone(&registers), fresh_iram(), rom);
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
        let mut core = Sa1Core::new(Rc::clone(&registers), fresh_iram(), Rc::new(rom));
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
        let mut core = Sa1Core::new(Rc::clone(&registers), fresh_iram(), Rc::new(rom));
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
}
