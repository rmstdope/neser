//! Mapper 334 – 5/20-in-1 Copyright Multicart
//!
//! ## Specifications
//!
//! - Primary source: NesDev wiki
//!   <https://www.nesdev.org/wiki/NES_2.0_Mapper_334>
//!
//! ## Hardware overview
//!
//! A multicart board that uses MMC3 for CHR banking and mirroring, but overrides
//! all PRG banking with an outer register written to `$6000-$6001`.
//!
//! - PRG-ROM: `$8000-$FFFF` is a fixed 32 KiB window determined by the `$6000`
//!   register (bits 2:1 → 32 KiB bank number). MMC3's PRG bank registers are
//!   **ignored** for PRG; only CHR and mirroring use MMC3.
//! - CHR-ROM: standard MMC3 1 KiB CHR banking via `$8000-$9FFF`.
//! - `$6000` write (mask `$6003`): bits 2:1 select 32 KiB PRG bank at `$8000`.
//! - `$6001` write (mask `$6003`): unknown; stored for save-state compatibility.
//! - `$6002` read (mask `$6003`): bit 0 = jumper (0=20-in-1, 1=5-in-1); bits 7:1 = open bus.
//! - `$8000-$FFFF`: forwarded to MMC3 (CHR + mirroring registers only).
//! - WRAM must be enabled via `$A001.7=1, $A001.6=0` to access `$600x` registers.
//!
//! ## PRG banking
//!
//! All four 8 KiB PRG slots (`$8000-$FFFF`) come from the 32 KiB bank selected by
//! `$6000` bits 2:1.  Bank n maps as:
//!
//! ```text
//! $8000-$9FFF → 8 KiB slot 0 of 32 KiB bank n
//! $A000-$BFFF → 8 KiB slot 1
//! $C000-$DFFF → 8 KiB slot 2
//! $E000-$FFFF → 8 KiB slot 3
//! ```
//!
//! ## DIP switch (jumper)
//!
//! - `$6002` bit 0: 0=20-in-1 menu, 1=5-in-1 menu.
//! - Set via submapper number: submapper 1 = 5-in-1 (jumper=1).
//!
//! ## IRQ
//!
//! Standard MMC3 scanline counter (A12 rising-edge).
//!
//! ## Known Limitations
//!
//! - Exact behaviour of `$6001` is undocumented; it is stored but not acted upon.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};
use crate::nes::cartridge::mmc3::MMC3Mapper;

const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB (for slot addressing)
const PRG_BANK_MASK: usize = PRG_BANK_SIZE - 1;
const CHR_BANK_SIZE: usize = 0x0400; // 1 KiB
const CHR_BANK_MASK: usize = CHR_BANK_SIZE - 1;

/// Mapper 334 – 5/20-in-1 Copyright multicart.
pub struct Mapper334 {
    mmc3: MMC3Mapper,
    /// `$6000` register: bits 2:1 = 32 KiB PRG bank.
    reg6000: u8,
    /// `$6001` register: unknown function; stored for save states.
    reg6001: u8,
    /// DIP switch / jumper state (false=20-in-1, true=5-in-1).
    jumper: bool,
}

impl Mapper334 {
    const MAPPER_NUMBER: u16 = 334;

    pub fn new(ctx: MapperContext) -> Self {
        let jumper = ctx.submapper == 1;
        let mmc3 = MMC3Mapper::new_with_irq_mode_and_prg_ram_banks(
            ctx.prg_rom,
            ctx.chr_rom,
            ctx.mirroring,
            false,
            1, // 8 KiB PRG-RAM for $A001 WRAM register
        );
        Self {
            mmc3,
            reg6000: 0,
            reg6001: 0,
            jumper,
        }
    }

    /// 32 KiB bank base index (in 8 KiB units) from `$6000` bits 2:1.
    fn prg_base_slot(&self) -> usize {
        (((self.reg6000 >> 1) & 0x03) as usize) * 4
    }

    fn prg_bank_for_addr(&self, addr: u16) -> usize {
        let slot = (addr as usize - 0x8000) >> 13;
        self.prg_base_slot() + (slot & 0x03)
    }
}

impl Mapper for Mapper334 {
    fn base(&self) -> &BaseMapper {
        &self.mmc3.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.mmc3.base
    }

    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        Some(&self.mmc3)
    }

    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        Some(&mut self.mmc3)
    }

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 32, // outer 32 KiB PRG granularity
            chr_bank_size_kb: 1,
            max_prg_ram_kb: 8,
            ..Default::default()
        }
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.mmc3.read_prg(addr),
            0x8000..=0xFFFF => {
                let bank = self.prg_bank_for_addr(addr);
                let count = self.mmc3.base.prg_rom().len() / PRG_BANK_SIZE;
                let wrapped = if count > 0 { bank % count } else { 0 };
                let offset = (addr as usize) & PRG_BANK_MASK;
                self.mmc3.read_prg_at_bank(wrapped, offset)
            }
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            // $6002 (masked $6003): bit 0 = jumper.
            a @ 0x6000..=0x7FFF if (a & 0x6003) == 0x6002 => {
                (open_bus & 0xFE) | (self.jumper as u8)
            }
            0x6000..=0x7FFF => self.mmc3.read_prg_open_bus(addr, open_bus),
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => open_bus,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            a @ 0x6000..=0x7FFF => {
                match a & 0x6003 {
                    0x6000 => self.reg6000 = value,
                    0x6001 => self.reg6001 = value,
                    _ => {}
                }
                // Pass through so MMC3 WRAM enable ($A001) still works.
                self.mmc3.write_prg(addr, value);
            }
            0x8000..=0xFFFF => self.mmc3.write_prg(addr, value),
            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let bank = self.mmc3.raw_chr_1k_bank(addr);
        let count = self.mmc3.chr_bank_count_1k();
        let wrapped = if count > 0 { bank % count } else { 0 };
        let offset = (addr as usize) & CHR_BANK_MASK;
        self.mmc3.read_chr_1k_at(wrapped, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let bank = self.mmc3.raw_chr_1k_bank(addr);
        let count = self.mmc3.chr_bank_count_1k();
        let wrapped = if count > 0 { bank % count } else { 0 };
        let offset = (addr as usize) & CHR_BANK_MASK;
        self.mmc3.write_chr_1k_at(wrapped, offset, value);
    }

    fn wram_size(&self) -> usize {
        self.mmc3.wram_size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.mmc3.wram_snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.mmc3.load_wram_snapshot(data);
    }

    fn initialize_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        self.mmc3.initialize_ram(mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.reg6000);
        snap.push(self.reg6001);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let n = data.len();
        if n >= 2 {
            self.reg6000 = data[n - 2];
            self.reg6001 = data[n - 1];
            self.mmc3.restore_registers(&data[..n - 2]);
        }
    }

    fn reset(&mut self) {
        self.mmc3.reset();
        self.reg6000 = 0;
        self.reg6001 = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::create_mapper;
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 24; // 24 × 8 KiB = 192 KiB (non-power-of-two, 6 × 32 KiB)
    const CHR_BANKS: usize = 12; // 12 × 1 KiB (non-power-of-two)

    fn make_mapper() -> Mapper334 {
        Mapper334::new(MapperContext::new_for_test(
            Mapper334::MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Horizontal,
        ))
    }

    // ── Registration ────────────────────────────────────────────────

    #[test]
    fn mapper_334_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            Mapper334::MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 334 must be registered in the factory"
        );
    }

    // ── PRG banking ─────────────────────────────────────────────────

    #[test]
    fn power_on_prg_maps_first_32k_bank() {
        let mapper = make_mapper();
        // reg6000=0 → slot 0 → banks 0-3
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must be bank 0 at power-on"
        );
        assert_eq!(
            mapper.read_prg(0xA000),
            1,
            "$A000 must be bank 1 at power-on"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            2,
            "$C000 must be bank 2 at power-on"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            3,
            "$E000 must be bank 3 at power-on"
        );
    }

    #[test]
    fn reg6000_bits_2_1_select_32k_prg_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x02); // bits 2:1 = 01 → 32KiB bank 1 → 8KiB slots 4-7
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "$8000 must be bank 4 (32KiB bank 1)"
        );
        assert_eq!(mapper.read_prg(0xE000), 7, "$E000 must be bank 7");
    }

    #[test]
    fn reg6000_bits_2_1_value2_selects_bank_2() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x04); // bits 2:1 = 10 → 32KiB bank 2 → slots 8-11
        assert_eq!(mapper.read_prg(0x8000), 8, "$8000 must be bank 8");
        assert_eq!(mapper.read_prg(0xE000), 11, "$E000 must be bank 11");
    }

    #[test]
    fn mmc3_prg_banking_does_not_affect_prg_output() {
        let mut mapper = make_mapper();
        // Set MMC3 R6 to bank 10; it should NOT affect PRG output.
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 10);
        // reg6000=0 → still reads bank 0 at $8000.
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "MMC3 PRG register must not affect PRG output in mapper 334"
        );
    }

    // ── CHR banking ─────────────────────────────────────────────────

    #[test]
    fn chr_uses_mmc3_banking() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x00); // bank_select R0
        mapper.write_prg(0x8001, 8); // R0=8 (even-aligned → 1KiB bank 8 at $0000)
        assert_eq!(
            mapper.read_chr(0x0000),
            8 % CHR_BANKS as u8,
            "CHR must use MMC3 bank 8 at $0000"
        );
    }

    // ── $6002 jumper read ────────────────────────────────────────────

    #[test]
    fn jumper_off_reads_zero_in_bit0() {
        let mapper = make_mapper(); // submapper 0 → jumper=false
        let val = mapper.read_prg_open_bus(0x6002, 0xFE);
        assert_eq!(val & 0x01, 0, "Jumper off: bit 0 of $6002 must be 0");
    }

    #[test]
    fn jumper_on_reads_one_in_bit0() {
        let ctx = MapperContext::new_for_test(
            Mapper334::MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Horizontal,
        )
        .with_submapper(1);
        let mapper = Mapper334::new(ctx);
        let val = mapper.read_prg_open_bus(0x6002, 0xFE);
        assert_eq!(val & 0x01, 1, "Jumper on: bit 0 of $6002 must be 1");
    }

    // ── Reset ───────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_prg_mapping() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x04);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "After reset $8000 must be PRG bank 0"
        );
    }
}
