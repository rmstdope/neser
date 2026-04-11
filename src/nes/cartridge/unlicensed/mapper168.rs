//! Mapper 168 – Racermate Challenge 2
//!
//! # Specifications
//! - Primary source: NESdev Wiki <https://www.nesdev.org/wiki/INES_Mapper_168>
//!
//! ## Overview
//!
//! Used exclusively by *Racermate Challenge 2*, a US release by Racermate Inc.
//! The board combines switchable PRG-ROM with battery-backed CHR-RAM and a
//! free-running CPU-cycle IRQ counter.
//!
//! ## Memory Map
//!
//! - `CPU $8000–$BFFF`: 16 KiB switchable PRG-ROM bank
//! - `CPU $C000–$FFFF`: 16 KiB PRG-ROM bank, fixed to the last bank
//! - `PPU $0000–$0FFF`: 4 KiB CHR-RAM bank, fixed to bank 0 (always)
//! - `PPU $1000–$1FFF`: 4 KiB switchable CHR-RAM bank
//!
//! ## CHR-RAM Layout
//!
//! Total: 64 KiB CHR-RAM (16 × 4 KiB banks, banks 0–15).
//! - Banks 0–7 (SRAM U1): may be battery-backed depending on board jumpers.
//!   Default factory configuration uses non-backed supply for U1.
//! - Banks 8–15 (SRAM U2): always battery-backed.
//!
//! ## Registers
//!
//! ### Bank Select (`$8000–$BFFF`)
//! ```text
//! D~7654 3210
//!   PP.. CCCC
//!   ||   ++++- Select 4 KiB CHR-RAM bank for PPU $1000–$1FFF (banks 0–15)
//!   ++-------- Select 16 KiB PRG-ROM bank for CPU $8000–$BFFF
//! ```
//!
//! ### RAM Protection and IRQ Acknowledge (`$C000–$FFFF`)
//! ```text
//! D~7654 3210   addr~FFFF FFFF FFFF FFFF
//!   .... ..Z.   .... .... .... .... ....
//!               Z = data bit 2 (D2)
//! ```
//! When `Z == 1`: IRQ is acknowledged and the cycle counter is frozen at 0.
//! When `Z == 0`: The cycle counter counts up every M2 cycle.
//!   Transitioning from `Z=1` to `Z=0` clears the RAM protection flag.
//!
//! ## IRQ
//!
//! A free-running 16-bit binary counter increments every M2 cycle when enabled.
//! The IRQ line is asserted while counter bit 10 is set (i.e., `counter & 0x400 != 0`).
//! Because the counter is continuous, the IRQ auto-asserts for 1024 cycles, then
//! de-asserts for 1024 cycles, repeating indefinitely while counting.
//!
//! When `Z=1` is written: counter resets to 0 and stops counting.
//! When `Z=0` is written: counter resumes counting from 0.
//!
//! ## Mirroring
//!
//! Hardwired vertical.
//!
//! ## Power-on State
//!
//! PRG bank = 0, CHR bank = 0, IRQ ack = 1 (counter frozen at 0).

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 168;
const PRG_16K_BANK_SIZE: usize = 16 * 1024;
const CHR_4K_BANK_SIZE: usize = 4 * 1024;
const CHR_RAM_SIZE: usize = 64 * 1024;

/// Mapper 168 – Racermate Challenge 2.
///
/// See the module-level documentation for hardware details.
pub struct Mapper168 {
    base: BaseMapper,
    /// PRG-ROM bank register (bits [7:6] of the $8000 write).
    prg_bank: u8,
    /// CHR-RAM bank register (bits [3:0] of the $8000 write).
    chr_bank: u8,
    /// 64 KiB CHR-RAM.
    chr_ram: [u8; CHR_RAM_SIZE],
    /// Free-running CPU cycle counter.
    cycle_counter: u16,
    /// When true the counter is frozen at 0 (IRQ acknowledge / Z=1 state).
    irq_frozen: bool,
}

impl Mapper168 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_irq: true,
            prg_bank_size_kb: PRG_16K_BANK_SIZE / 1024,
            chr_bank_size_kb: CHR_4K_BANK_SIZE / 1024,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_16K_BANK_SIZE);
        // CHR is RAM; no CHR-ROM banking in BaseMapper.
        base.set_mirroring(NametableLayout::Vertical);

        let mut mapper = Self {
            base,
            prg_bank: 0,
            chr_bank: 0,
            chr_ram: [0; CHR_RAM_SIZE],
            cycle_counter: 0,
            irq_frozen: true, // frozen at power-on
        };
        mapper.apply_prg();
        mapper
    }

    fn apply_prg(&mut self) {
        self.base.select_prg_page(0, self.prg_bank as i16);
        // Fixed last PRG bank at $C000.
        self.base.select_prg_page(1, -1);
    }

    /// Returns the byte offset into CHR-RAM for the given PPU address.
    fn chr_offset(&self, addr: u16) -> usize {
        let bank = if addr < 0x1000 {
            0 // fixed to bank 0
        } else {
            self.chr_bank as usize
        };
        bank * CHR_4K_BANK_SIZE + (addr as usize & (CHR_4K_BANK_SIZE - 1))
    }
}

impl Mapper for Mapper168 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.chr_ram[self.chr_offset(addr)]
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let offset = self.chr_offset(addr);
        self.chr_ram[offset] = value;
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000..=0xBFFF => {
                // Bank Select: PP.. CCCC
                self.prg_bank = (value >> 6) & 0x03;
                self.chr_bank = value & 0x0F;
                self.apply_prg();
            }
            0xC000..=0xFFFF => {
                // IRQ Acknowledge / RAM protection: bit D2 is Z
                let z = (value & 0x04) != 0;
                if z {
                    // Z=1: acknowledge IRQ, freeze counter at 0
                    self.irq_frozen = true;
                    self.cycle_counter = 0;
                } else {
                    // Z=0: resume counting
                    self.irq_frozen = false;
                    self.cycle_counter = 0;
                }
            }
            _ => {}
        }
    }

    fn cpu_cycle(&mut self) {
        if !self.irq_frozen {
            self.cycle_counter = self.cycle_counter.wrapping_add(1);
        }
    }

    fn irq_pending(&self) -> bool {
        !self.irq_frozen && (self.cycle_counter & 0x400) != 0
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let flags = (self.irq_frozen as u8) | ((self.irq_pending() as u8) << 1);
        let counter_lo = (self.cycle_counter & 0xFF) as u8;
        let counter_hi = (self.cycle_counter >> 8) as u8;
        vec![self.prg_bank, self.chr_bank, flags, counter_lo, counter_hi]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 5 {
            self.prg_bank = data[0] & 0x03;
            self.chr_bank = data[1] & 0x0F;
            self.irq_frozen = (data[2] & 1) != 0;
            self.cycle_counter = (data[3] as u16) | ((data[4] as u16) << 8);
            self.apply_prg();
        }
    }

    fn initialize_chr_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        crate::nes::console::initialize_ram(&mut self.chr_ram, mode);
    }

    fn wram_size(&self) -> usize {
        CHR_RAM_SIZE
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.chr_ram.to_vec()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        if data.len() == CHR_RAM_SIZE {
            self.chr_ram.copy_from_slice(data);
        }
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.chr_bank = 0;
        self.cycle_counter = 0;
        self.irq_frozen = true;
        self.apply_prg();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::create_mapper;
    use crate::nes::cartridge::test_helpers::banked_data;

    fn make_mapper() -> Mapper168 {
        Mapper168::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_16K_BANK_SIZE, 4),
            vec![], // no CHR-ROM; uses CHR-RAM
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_168_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_16K_BANK_SIZE, 4),
            vec![],
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 168 must be creatable via factory");
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let m = make_mapper();
        assert_eq!(m.base.mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn power_on_irq_is_frozen() {
        let m = make_mapper();
        assert!(m.irq_frozen);
        assert!(!m.irq_pending());
    }

    #[test]
    fn write_8000_sets_prg_bank_from_bits7_6() {
        let mut m = make_mapper();
        m.write_prg(0x8000, 0b1100_0000);
        assert_eq!(m.prg_bank, 3);
    }

    #[test]
    fn write_8000_sets_chr_bank_from_bits3_0() {
        let mut m = make_mapper();
        m.write_prg(0x8000, 0b0000_1111);
        assert_eq!(m.chr_bank, 0x0F);
    }

    #[test]
    fn write_c000_with_bit2_high_freezes_counter() {
        let mut m = make_mapper();
        m.write_prg(0xC000, 0x00); // Z=0: start counting
        for _ in 0..100 {
            m.cpu_cycle();
        }
        m.write_prg(0xC000, 0x04); // Z=1: freeze
        assert!(m.irq_frozen);
        assert_eq!(m.cycle_counter, 0);
    }

    #[test]
    fn write_c000_with_bit2_low_resumes_counting() {
        let mut m = make_mapper();
        m.write_prg(0xC000, 0x00); // Z=0: counting
        assert!(!m.irq_frozen);
    }

    #[test]
    fn irq_fires_at_cycle_1024() {
        let mut m = make_mapper();
        m.write_prg(0xC000, 0x00); // start counting

        for _ in 0..1023 {
            m.cpu_cycle();
        }
        assert!(!m.irq_pending(), "IRQ must not fire before cycle 1024");

        m.cpu_cycle(); // cycle 1024: bit 10 of counter set
        assert!(m.irq_pending(), "IRQ must fire at cycle 1024");
    }

    #[test]
    fn irq_auto_clears_at_cycle_2048() {
        let mut m = make_mapper();
        m.write_prg(0xC000, 0x00);

        for _ in 0..2048 {
            m.cpu_cycle();
        }
        // At cycle 2048, bit 10 is clear again (0b100000000000 is bit 11)
        assert!(!m.irq_pending(), "IRQ must de-assert at cycle 2048");
    }

    #[test]
    fn irq_re_fires_at_cycle_3072() {
        let mut m = make_mapper();
        m.write_prg(0xC000, 0x00);

        for _ in 0..3072 {
            m.cpu_cycle();
        }
        assert!(m.irq_pending(), "IRQ must re-assert at cycle 3072");
    }

    #[test]
    fn irq_acknowledge_stops_irq() {
        let mut m = make_mapper();
        m.write_prg(0xC000, 0x00);
        for _ in 0..1024 {
            m.cpu_cycle();
        }
        assert!(m.irq_pending());
        m.write_prg(0xF000, 0x04); // Z=1: acknowledge
        assert!(!m.irq_pending(), "IRQ must be clear after acknowledge");
        assert_eq!(m.cycle_counter, 0);
    }

    #[test]
    fn frozen_counter_does_not_advance() {
        let mut m = make_mapper();
        // irq_frozen = true at power-on
        for _ in 0..2000 {
            m.cpu_cycle();
        }
        assert_eq!(m.cycle_counter, 0);
        assert!(!m.irq_pending());
    }

    #[test]
    fn chr_read_write_roundtrip() {
        let mut m = make_mapper();
        m.write_chr(0x0100, 0xAB);
        assert_eq!(m.read_chr(0x0100), 0xAB);
    }

    #[test]
    fn chr_bank_switch_selects_correct_4k_region() {
        let mut m = make_mapper();
        // Write distinctive value to bank 2, page $1000-$1FFF
        m.write_prg(0x8000, 0b0000_0010); // chr_bank = 2
        m.write_chr(0x1000, 0x42); // write to bank 2
        // Switch to bank 3
        m.write_prg(0x8000, 0b0000_0011);
        assert_ne!(
            m.read_chr(0x1000),
            0x42,
            "Different bank should not see value"
        );
        // Switch back to bank 2
        m.write_prg(0x8000, 0b0000_0010);
        assert_eq!(m.read_chr(0x1000), 0x42, "Bank 2 must retain value");
    }

    #[test]
    fn chr_fixed_bank0_at_ppu0000() {
        let mut m = make_mapper();
        m.write_chr(0x0200, 0x77); // write to bank 0 (fixed)
        m.write_prg(0x8000, 0b0000_1111); // switch $1000 bank to 15
        // $0000-$0FFF always bank 0
        assert_eq!(m.read_chr(0x0200), 0x77, "Fixed bank 0 must be unchanged");
    }

    #[test]
    fn reset_restores_power_on_state() {
        let mut m = make_mapper();
        m.write_prg(0x8000, 0b1100_1010);
        m.write_prg(0xC000, 0x00);
        for _ in 0..500 {
            m.cpu_cycle();
        }
        m.reset();
        assert_eq!(m.prg_bank, 0);
        assert_eq!(m.chr_bank, 0);
        assert!(m.irq_frozen);
        assert_eq!(m.cycle_counter, 0);
    }

    #[test]
    fn snapshot_restore_round_trips() {
        let mut m = make_mapper();
        m.write_prg(0x8000, 0b0100_0011); // prg=1, chr=3
        m.write_prg(0xC000, 0x00); // start counting
        for _ in 0..500 {
            m.cpu_cycle();
        }

        let snap = m.registers_snapshot();
        let mut m2 = make_mapper();
        m2.restore_registers(&snap);

        assert_eq!(m2.prg_bank, m.prg_bank);
        assert_eq!(m2.chr_bank, m.chr_bank);
        assert_eq!(m2.irq_frozen, m.irq_frozen);
        assert_eq!(m2.cycle_counter, m.cycle_counter);
    }
}
