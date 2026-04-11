//! Mapper 190 — Magic Kid Googoo (Zemina)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_190>
//! - Reference implementation: Mesen2 `MagicKidGooGoo.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Unlicensed/MagicKidGooGoo.h>
//!
//! ## Hardware behavior
//!
//! Used exclusively by the unlicensed Korean game *Magic Kid Googoo* (Zemina, 1992).
//!
//! ### Memory map
//!
//! | Region         | Content                                      |
//! |----------------|----------------------------------------------|
//! | CPU `$6000–$7FFF` | 8 KiB PRG-RAM                           |
//! | CPU `$8000–$BFFF` | 16 KiB switchable PRG-ROM bank          |
//! | CPU `$C000–$FFFF` | 16 KiB PRG-ROM bank, fixed to bank 0    |
//! | PPU `$0000–$07FF` | 2 KiB switchable CHR-ROM bank (slot 0)  |
//! | PPU `$0800–$0FFF` | 2 KiB switchable CHR-ROM bank (slot 1)  |
//! | PPU `$1000–$17FF` | 2 KiB switchable CHR-ROM bank (slot 2)  |
//! | PPU `$1800–$1FFF` | 2 KiB switchable CHR-ROM bank (slot 3)  |
//!
//! ### Registers (write only)
//!
//! **PRG bank select** (`$8000–$9FFF`, write):
//! ```text
//! D~[.... .PPP]   (A14=0: selects PRG banks 0–7)
//!         +++--- PRG A16..A14 → 16 KiB bank at $8000–$BFFF
//! ```
//!
//! **PRG bank select** (`$C000–$DFFF`, write):
//! ```text
//! D~[.... .PPP]   (A14=1: selects PRG banks 8–15, adds bit 3)
//!         +++--- PRG A16..A14 → 16 KiB bank at $8000–$BFFF (bank = (value & 0x07) | 0x08)
//! ```
//!
//! **CHR bank select** (`$A000–$BFFF` and `$E000–$FFFF`, write):
//! ```text
//! A~[.... .... .... ..ss]  D~[CCCC CCCC]
//!                    ||        |||| ||||
//!                    ||        ++++-++++--- 2 KiB CHR bank number
//!                    ++-------------------- PPU slot index (0–3)
//! ```
//!
//! Mirroring is fixed vertical (hardwired on the board).
//!
//! No IRQ, no expansion audio.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 190;
const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 2 * 1024;

/// Mapper 190 — Magic Kid Googoo (Zemina).
///
/// See the module-level documentation for hardware details.
pub struct Mapper190 {
    base: BaseMapper,
    /// 4-bit PRG bank number for the `$8000–$BFFF` window.
    /// Bit 3 is set when the write originated from `$C000–$DFFF`.
    prg_bank: u8,
    /// 2 KiB CHR bank for each of the four 2 KiB PPU slots.
    chr_banks: [u8; 4],
}

impl Mapper190 {
    pub fn new(mut ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        // Mapper 190 always has 8 KiB PRG-RAM at $6000–$7FFF regardless of
        // what the ROM header specifies.
        ctx.prg_ram_banks_8k = 1;
        ctx.prg_ram_size_specified = true;
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 2,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);
        let mut mapper = Self {
            base,
            prg_bank: 0,
            chr_banks: [0; 4],
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        // Switchable 16 KiB bank at $8000–$BFFF.
        self.base.select_prg_page(0, self.prg_bank as i16);
        // $C000–$FFFF: always fixed to bank 0 (first bank).
        self.base.select_prg_page(1, 0);
        for (slot, &bank) in self.chr_banks.iter().enumerate() {
            self.base.select_chr_page(slot, bank as i16);
        }
        // Mirroring is hardwired vertical on the board.
        self.base.set_mirroring(NametableLayout::Vertical);
    }
}

impl Mapper for Mapper190 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        if addr < 0x8000 {
            return;
        }
        if addr <= 0x9FFF {
            // PRG bank select for banks 0–7 (A14=0).
            self.prg_bank = value & 0x07;
            self.base.select_prg_page(0, self.prg_bank as i16);
        } else if (0xC000..=0xDFFF).contains(&addr) {
            // PRG bank select for banks 8–15 (A14=1, adds bit 3).
            self.prg_bank = (value & 0x07) | 0x08;
            self.base.select_prg_page(0, self.prg_bank as i16);
        } else if (addr & 0xA000) == 0xA000 {
            // CHR bank select: (addr & 0xA000) == 0xA000 matches $A000–$BFFF and $E000–$FFFF.
            let slot = (addr & 0x03) as usize;
            self.chr_banks[slot] = value;
            self.base.select_chr_page(slot, value as i16);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = vec![self.prg_bank];
        snap.extend_from_slice(&self.chr_banks);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 5 {
            self.prg_bank = data[0];
            self.chr_banks.copy_from_slice(&data[1..5]);
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.chr_banks = [0; 4];
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 16; // 16 × 16 KiB = 256 KiB
    const CHR_BANKS: usize = 64; // 64 × 2 KiB = 128 KiB

    fn make_mapper() -> Mapper190 {
        Mapper190::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Horizontal, // overridden to Vertical by hardware
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_190_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 190 must be creatable via factory");
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_bank_0_at_8000_and_c000() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should be bank 0 on power-on"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 should be fixed to bank 0 on power-on"
        );
    }

    #[test]
    fn power_on_chr_all_slots_bank_0() {
        let mut mapper = make_mapper();
        for slot in 0..4usize {
            let ppu_addr = (slot * 0x800) as u16;
            assert_eq!(
                mapper.read_chr(ppu_addr),
                0,
                "CHR slot {slot} should be bank 0 on power-on"
            );
        }
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring must be hardwired Vertical regardless of header"
        );
    }

    // ── PRG banking ($8000–$9FFF → banks 0–7) ────────────────────────────────

    #[test]
    fn write_8000_selects_prg_bank_low_range() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 5);
        assert_eq!(mapper.read_prg(0x8000), 5, "PRG slot 0 should be bank 5");
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 must remain fixed to bank 0"
        );
    }

    #[test]
    fn write_9fff_also_sets_prg_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9FFF, 7);
        assert_eq!(
            mapper.read_prg(0x8000),
            7,
            "Write to $9FFF should set PRG bank 7"
        );
    }

    // ── PRG banking ($C000–$DFFF → banks 8–15) ───────────────────────────────

    #[test]
    fn write_c000_selects_prg_bank_high_range() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 3); // bank = 3 | 8 = 11
        assert_eq!(
            mapper.read_prg(0x8000),
            11,
            "Write to $C000 should select bank 11 (3 | 0x08)"
        );
    }

    #[test]
    fn write_dfff_also_sets_high_prg_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xDFFF, 0); // bank = 0 | 8 = 8
        assert_eq!(
            mapper.read_prg(0x8000),
            8,
            "Write to $DFFF should select bank 8"
        );
    }

    #[test]
    fn c000_fixed_bank_unaffected_by_prg_writes() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 7);
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 must always be bank 0 after $8000 write"
        );
        mapper.write_prg(0xC000, 7);
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 must always be bank 0 after $C000 write"
        );
    }

    // ── CHR banking ───────────────────────────────────────────────────────────

    #[test]
    fn write_a000_to_a003_selects_chr_slots() {
        let mut mapper = make_mapper();
        for slot in 0u16..4 {
            let bank = (slot * 10 + 1) as u8;
            mapper.write_prg(0xA000 + slot, bank);
            assert_eq!(
                mapper.read_chr(slot * 0x800),
                bank,
                "CHR slot {slot} should be bank {bank}"
            );
        }
    }

    #[test]
    fn write_e000_to_e003_also_selects_chr_slots() {
        let mut mapper = make_mapper();
        for slot in 0u16..4 {
            let bank = (slot * 5 + 2) as u8;
            mapper.write_prg(0xE000 + slot, bank);
            assert_eq!(
                mapper.read_chr(slot * 0x800),
                bank,
                "CHR slot {slot} should be selectable from $E000 range"
            );
        }
    }

    #[test]
    fn chr_slots_are_independently_selectable() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 3);
        mapper.write_prg(0xA001, 7);
        mapper.write_prg(0xA002, 12);
        mapper.write_prg(0xA003, 20);
        assert_eq!(mapper.read_chr(0x0000), 3);
        assert_eq!(mapper.read_chr(0x0800), 7);
        assert_eq!(mapper.read_chr(0x1000), 12);
        assert_eq!(mapper.read_chr(0x1800), 20);
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 6);
        mapper.write_prg(0xA000, 10);
        mapper.write_prg(0xA001, 20);
        mapper.write_prg(0xA002, 30);
        mapper.write_prg(0xA003, 40);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.prg_bank, mapper.prg_bank);
        assert_eq!(restored.chr_banks, mapper.chr_banks);
        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.read_chr(0x0000), mapper.read_chr(0x0000));
        assert_eq!(restored.read_chr(0x1800), mapper.read_chr(0x1800));
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_returns_to_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 7);
        mapper.write_prg(0xA003, 15);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG should be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_chr(0x1800),
            0,
            "CHR slot 3 should be bank 0 after reset"
        );
    }

    // ── PRG-RAM always present ─────────────────────────────────────────────────

    #[test]
    fn prg_ram_available_even_without_header_specification() {
        let mut mapper = Mapper190::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                banked_data(CHR_BANK_SIZE, CHR_BANKS),
                NametableLayout::Horizontal,
            )
            .with_unspecified_prg_ram_size(),
        );
        // 8 KiB PRG-RAM must be available at $6000–$7FFF.
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(
            mapper.read_prg(0x6000),
            0xAB,
            "$6000 PRG-RAM must be writable even when header omits PRG-RAM size"
        );
        assert_eq!(
            mapper.base().wram_size(),
            8 * 1024,
            "wram_size() must report 8 KiB"
        );
    }
}
