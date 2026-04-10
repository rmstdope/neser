//! Mapper 154 - NAMCOT-3453
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 154 - NAMCOT-3453
///
/// Hardware: Namco 108 chip (NAMCOT-3453), used only for Devil Man.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_154>
/// - Related: <https://www.nesdev.org/wiki/INES_Mapper_088>
/// - PRG-ROM: Up to 512 KB (8 KB banks, same as mapper 88)
/// - CHR-ROM: 128 KB (same CHR banking as mapper 88)
/// - Mirroring: Software-controlled one-screen (bit 6 of all $8000–$FFFF writes)
///
/// Identical to Mapper 88 with one addition: bit 6 of every write to $8000–$FFFF
/// selects the nametable:
///   - 0 → Single screen lower (screen A)
///   - 1 → Single screen upper (screen B)
///
/// Unlike the base Namco 108 chip where the nametable bit is at $8000–$9FFF only,
/// here it spans the entire $8000–$FFFF range.
pub struct Namcot3453Mapper {
    base: BaseMapper,
    bank_select: u8,
    regs: [u8; 8],
}

impl Namcot3453Mapper {
    const REG_SELECT_MASK: u8 = 0b0000_0111;

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(8 * 1024);
        base.configure_chr_banking(1024);
        base.set_mirroring(NametableLayout::SingleScreenLower);
        let mut mapper = Self {
            base,
            bank_select: 0,
            regs: [0; 8],
        };
        mapper.update_banks();
        mapper
    }

    fn selected_reg(&self) -> usize {
        (self.bank_select & Self::REG_SELECT_MASK) as usize
    }

    fn update_banks(&mut self) {
        // PRG mode always 0: R6@$8000, R7@$A000, -2@$C000, -1@$E000
        self.base.select_prg_page(0, self.regs[6] as i16);
        self.base.select_prg_page(1, self.regs[7] as i16);
        self.base.select_prg_page(2, -2);
        self.base.select_prg_page(3, -1);

        // CHR mode always 0: R0/R1 → 2 KB at $0000–$0FFF; R2–R5 → 1 KB at $1000–$1FFF
        // R0, R1 are 6-bit (lower 64 KB of CHR); R2–R5 have bit 6 forced high (upper 64 KB).
        let r0 = (self.regs[0] & 0x3E) as i16;
        let r1 = (self.regs[1] & 0x3E) as i16;
        let r2 = (self.regs[2] | 0x40) as i16;
        let r3 = (self.regs[3] | 0x40) as i16;
        let r4 = (self.regs[4] | 0x40) as i16;
        let r5 = (self.regs[5] | 0x40) as i16;

        self.base.select_chr_page(0, r0);
        self.base.select_chr_page(1, r0 + 1);
        self.base.select_chr_page(2, r1);
        self.base.select_chr_page(3, r1 + 1);
        self.base.select_chr_page(4, r2);
        self.base.select_chr_page(5, r3);
        self.base.select_chr_page(6, r4);
        self.base.select_chr_page(7, r5);
    }

    fn apply_mirroring_bit(&mut self, value: u8) {
        let mirroring = if (value & 0x40) != 0 {
            NametableLayout::SingleScreenUpper
        } else {
            NametableLayout::SingleScreenLower
        };
        self.base.set_mirroring(mirroring);
    }
}

impl Mapper for Namcot3453Mapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        if addr >= 0x8000 {
            // Bit 6 controls one-screen nametable selection across the full $8000–$FFFF range.
            self.apply_mirroring_bit(value);
            match addr & 0x8001 {
                0x8000 => {
                    self.bank_select = value & 0x3F;
                }
                0x8001 => {
                    let reg = self.selected_reg();
                    self.regs[reg] = value;
                    self.update_banks();
                }
                _ => {}
            }
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snapshot = Vec::with_capacity(10);
        snapshot.push(self.bank_select);
        snapshot.extend_from_slice(&self.regs);
        let mirroring_byte = match self.base.mirroring() {
            NametableLayout::SingleScreenUpper => 1,
            _ => 0,
        };
        snapshot.push(mirroring_byte);
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 9 {
            self.bank_select = data[0];
            self.regs.copy_from_slice(&data[1..9]);
            self.update_banks();
        }
        if data.len() >= 10 {
            let mirroring = if data[9] != 0 {
                NametableLayout::SingleScreenUpper
            } else {
                NametableLayout::SingleScreenLower
            };
            self.base.set_mirroring(mirroring);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    fn create_mapper154(prg_rom: Vec<u8>, chr_rom: Vec<u8>) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(
            154,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
    }

    // ---------- Instantiation ----------

    #[test]
    fn mapper154_instantiates_via_factory() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        assert!(
            create_mapper154(prg_rom, chr_rom).is_ok(),
            "Mapper 154 must be creatable via the factory"
        );
    }

    // ---------- PRG banking (same as mapper 88) ----------

    #[test]
    fn prg_r6_r7_switchable_c000_e000_fixed() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper154(prg_rom, chr_rom).unwrap();

        mapper.write_prg(0x8000, 0b0000_0110); // select R6
        mapper.write_prg(0x8001, 1);
        mapper.write_prg(0x8000, 0b0000_0111); // select R7
        mapper.write_prg(0x8001, 2);

        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "$8000 should be PRG bank 1 (R6)"
        );
        assert_eq!(
            mapper.read_prg(0xA000),
            2,
            "$A000 should be PRG bank 2 (R7)"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "$C000 should be second-last bank (6)"
        );
        assert_eq!(mapper.read_prg(0xE000), 7, "$E000 should be last bank (7)");
    }

    // ---------- Mirroring (unique to mapper 154) ----------

    #[test]
    fn default_mirroring_is_single_screen_lower() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        let mapper = create_mapper154(prg_rom, chr_rom).unwrap();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenLower,
            "Mapper 154 should boot with one-screen lower mirroring"
        );
    }

    #[test]
    fn mirroring_bit6_set_selects_single_screen_upper() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper154(prg_rom, chr_rom).unwrap();

        // Write to bank_select register with bit 6 set → should set upper screen
        mapper.write_prg(0x8000, 0b0100_0000);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenUpper,
            "Bit 6 set in any $8000-$FFFF write should select single-screen upper"
        );
    }

    #[test]
    fn mirroring_bit6_clear_selects_single_screen_lower() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper154(prg_rom, chr_rom).unwrap();

        // First set upper, then clear it
        mapper.write_prg(0x8000, 0b0100_0000);
        mapper.write_prg(0x8000, 0b0000_0000);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenLower,
            "Bit 6 clear should select single-screen lower"
        );
    }

    #[test]
    fn mirroring_bit_applies_from_bank_data_register_too() {
        // The nametable bit is present over the FULL $8000–$FFFF range, not just $8000
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper154(prg_rom, chr_rom).unwrap();

        // Write to $8001 (bank data register) with bit 6 set
        mapper.write_prg(0x8001, 0b0100_0000);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenUpper,
            "Bit 6 of $8001 write should also control mirroring"
        );

        // Write to $C000 with bit 6 set
        mapper.write_prg(0xC000, 0b0100_0001);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenUpper,
            "Bit 6 at $C000 should control mirroring"
        );

        // Write to $C000 with bit 6 clear
        mapper.write_prg(0xC000, 0b0000_0001);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenLower,
            "Bit 6 clear at $C000 should revert to lower screen"
        );
    }

    #[test]
    fn mirroring_write_does_not_corrupt_bank_select() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper154(prg_rom, chr_rom).unwrap();

        // Set R6=3 then R7=5
        mapper.write_prg(0x8000, 0b0000_0110); // bank_select R6
        mapper.write_prg(0x8001, 3);
        mapper.write_prg(0x8000, 0b0000_0111); // bank_select R7
        mapper.write_prg(0x8001, 5);

        // PRG banking should still work correctly regardless of mirroring writes
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "R6=3 should map bank 3 at $8000"
        );
        assert_eq!(
            mapper.read_prg(0xA000),
            5,
            "R7=5 should map bank 5 at $A000"
        );
    }

    // ---------- CHR banking (same as mapper 88) ----------

    #[test]
    fn chr_r0_selects_2kb_from_lower_chr_half() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 128);
        let mut mapper = create_mapper154(prg_rom, chr_rom).unwrap();

        mapper.write_prg(0x8000, 0b0000_0000); // R0
        mapper.write_prg(0x8001, 4);

        assert_eq!(
            mapper.read_chr(0x0000),
            4,
            "$0000 should be CHR bank 4 (R0=4)"
        );
        assert_eq!(
            mapper.read_chr(0x0400),
            5,
            "$0400 should be CHR bank 5 (R0+1)"
        );
    }

    #[test]
    fn chr_r2_selects_1kb_from_upper_chr_half() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 128);
        let mut mapper = create_mapper154(prg_rom, chr_rom).unwrap();

        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 0); // effective page = 0 | 0x40 = 64

        assert_eq!(
            mapper.read_chr(0x1000),
            64,
            "$1000 should be CHR bank 64 (R2=0|0x40)"
        );
    }

    // ---------- Snapshot / restore ----------

    #[test]
    fn snapshot_restore_preserves_mirroring_and_banks() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 128);
        let mut mapper = create_mapper154(prg_rom.clone(), chr_rom.clone()).unwrap();

        // Set R6=3 with bit 6 set throughout to keep SingleScreenUpper mirroring.
        // Bank data 0x43 = (bit6=1, bank_bits=3); PRG read returns 0x43 % 8 = 3.
        mapper.write_prg(0x8000, 0b0100_0110); // bank_select R6, bit 6 → upper screen
        mapper.write_prg(0x8001, 0b0100_0011); // data 0x43 — bit 6 keeps upper screen, bank=3

        let snap = mapper.registers_snapshot();

        let mut restored = create_mapper154(prg_rom, chr_rom).unwrap();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            3,
            "R6=3 should restore correctly"
        );
        assert_eq!(
            restored.get_mirroring(),
            NametableLayout::SingleScreenUpper,
            "Upper screen mirroring should be restored"
        );
    }
}
