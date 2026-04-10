//! Mapper 154 - NAMCOT-3453
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::ines::NametableLayout;
use crate::nes::cartridge::{Mapper, MapperCapabilities};

/// Mapper 154 - NAMCOT-3453 (Devil Man)
///
/// Hardware: Namco 108 chip variant with mapper-controlled one-screen mirroring.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_154>
/// - Related: <https://www.nesdev.org/wiki/INES_Mapper_088>
/// - PRG-ROM: Switchable 8 KB banks at $8000/$A000, fixed last 16KB
/// - CHR-ROM: 128 KB (CHR A12 wired to A16 splits access into two halves)
/// - Mirroring: Mapper-controlled one-screen (bit 6 of any $8000-$FFFF write)
///
/// Identical to Mapper 88, with the addition of mapper-controlled one-screen mirroring:
/// - $8000-$FFFF: [.Mxx xxxx]
///   - x = See mapper 206 documentation
///   - M (bit 6) = Mirroring: 0 = 1-screen A (lower), 1 = 1-screen B (upper)
///
/// The mirroring bit is present over the entire 32kB range.
///
/// CHR banking (same as Mapper 88):
/// - R0, R1 are 6-bit (bits 0-5 only); they select 2 KB banks from lower 64 KB of CHR
/// - R2-R5 always have bit 6 forced high; they select 1 KB banks from upper 64 KB of CHR
/// - CHR mode bit (bit 7 of bank select) is always forced to 0
///
/// PRG banking (same as Mapper 88):
/// - PRG mode bit (bit 6 of bank select) used for mirroring, not PRG mode
/// - R6 selects 8 KB at $8000-$9FFF
/// - R7 selects 8 KB at $A000-$BFFF
/// - $C000-$DFFF fixed to second-last 8 KB bank
/// - $E000-$FFFF fixed to last 8 KB bank
pub struct Namcot3453Mapper {
    base: BaseMapper,

    bank_select: u8,
    regs: [u8; 8],
}

impl Namcot3453Mapper {
    const REG_SELECT_MASK: u8 = 0b0000_0111;
    const MIRRORING_BIT: u8 = 0b0100_0000;

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            max_prg_ram_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(8 * 1024);
        base.configure_chr_banking(1024);
        // Power-on state: 1-screen A
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
        // PRG mode is always 0: R6@$8000, R7@$A000, -2@$C000, -1@$E000
        let r6 = self.regs[6] as i16;
        let r7 = self.regs[7] as i16;
        self.base.select_prg_page(0, r6);
        self.base.select_prg_page(1, r7);
        self.base.select_prg_page(2, -2);
        self.base.select_prg_page(3, -1);

        // CHR mode is always 0: R0/R1 → 2 KB at $0000-$0FFF; R2-R5 → 1 KB at $1000-$1FFF
        // R0, R1 are 6-bit (lower 64 KB of CHR); R2-R5 have bit 6 forced high (upper 64 KB).
        let r0 = (self.regs[0] & 0x3E) as i16; // 6-bit, even-aligned → 2 KB bank pair
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

    fn apply_mirroring(&mut self, value: u8) {
        if (value & Self::MIRRORING_BIT) != 0 {
            self.base.set_mirroring(NametableLayout::SingleScreenUpper);
        } else {
            self.base.set_mirroring(NametableLayout::SingleScreenLower);
        }
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
            // Mirroring bit (bit 6) applies over the entire $8000-$FFFF range
            self.apply_mirroring(value);

            match addr & 0x8001 {
                0x8000 => {
                    // Bits 6-7 forced to 0; bits 0-5 select the target register
                    self.bank_select = value & 0x3F;
                }
                0x8001 => {
                    let reg = self.selected_reg();
                    self.regs[reg] = value;
                    self.update_banks();
                }
                _ => unreachable!(),
            }
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snapshot = Vec::with_capacity(10);
        snapshot.push(self.bank_select);
        snapshot.extend_from_slice(&self.regs);
        snapshot.push(self.base.mirroring().to_snapshot_byte());
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 10 {
            self.bank_select = data[0];
            self.regs.copy_from_slice(&data[1..9]);
            self.update_banks();
            self.base
                .set_mirroring(NametableLayout::from_snapshot_byte(data[9]));
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
            NametableLayout::Horizontal,
        ))
    }

    // ---------- PRG banking tests ----------

    #[test]
    fn prg_r6_r7_switchable_c000_e000_fixed() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper =
            create_mapper154(prg_rom, chr_rom).expect("Mapper 154 should be implemented");

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
            "$C000 should be second-last bank"
        );
        assert_eq!(mapper.read_prg(0xE000), 7, "$E000 should be last bank");
    }

    #[test]
    fn prg_mode_bit_is_always_forced_to_zero() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper =
            create_mapper154(prg_rom, chr_rom).expect("Mapper 154 should be implemented");

        // Bit 6 is now mirroring bit, not PRG mode bit
        mapper.write_prg(0x8000, 0b0100_0110); // register 6, bit 6 = mirroring
        mapper.write_prg(0x8001, 3);

        // PRG layout should still be mode 0: R6 @ $8000
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "$8000 should be R6 bank 3 (mode 0 forced)"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "$C000 should be second-last (mode 0 forced)"
        );
    }

    // ---------- CHR banking tests ----------

    #[test]
    fn chr_r0_r1_select_2kb_banks_from_lower_chr_half() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 128);

        let mut mapper =
            create_mapper154(prg_rom, chr_rom).expect("Mapper 154 should be implemented");

        mapper.write_prg(0x8000, 0b0000_0000); // select R0
        mapper.write_prg(0x8001, 4);
        mapper.write_prg(0x8000, 0b0000_0001); // select R1
        mapper.write_prg(0x8001, 6);

        assert_eq!(mapper.read_chr(0x0000), 4, "$0000 should be CHR bank 4");
        assert_eq!(mapper.read_chr(0x0400), 5, "$0400 should be CHR bank 5");
        assert_eq!(mapper.read_chr(0x0800), 6, "$0800 should be CHR bank 6");
        assert_eq!(mapper.read_chr(0x0C00), 7, "$0C00 should be CHR bank 7");
    }

    #[test]
    fn chr_r2_r5_select_1kb_banks_from_upper_chr_half() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 128);

        let mut mapper =
            create_mapper154(prg_rom, chr_rom).expect("Mapper 154 should be implemented");

        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 0);
        mapper.write_prg(0x8000, 0b0000_0011); // R3
        mapper.write_prg(0x8001, 1);
        mapper.write_prg(0x8000, 0b0000_0100); // R4
        mapper.write_prg(0x8001, 2);
        mapper.write_prg(0x8000, 0b0000_0101); // R5
        mapper.write_prg(0x8001, 3);

        assert_eq!(mapper.read_chr(0x1000), 64, "$1000 should be CHR bank 64");
        assert_eq!(mapper.read_chr(0x1400), 65, "$1400 should be CHR bank 65");
        assert_eq!(mapper.read_chr(0x1800), 66, "$1800 should be CHR bank 66");
        assert_eq!(mapper.read_chr(0x1C00), 67, "$1C00 should be CHR bank 67");
    }

    // ---------- Mirroring tests ----------

    #[test]
    fn mirroring_defaults_to_one_screen_a_on_power_on() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);

        let mapper = create_mapper154(prg_rom, chr_rom).expect("Mapper 154 should be implemented");

        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenLower,
            "Default mirroring should be 1-screen A"
        );
    }

    #[test]
    fn mirroring_bit_zero_sets_one_screen_a() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);

        let mut mapper =
            create_mapper154(prg_rom, chr_rom).expect("Mapper 154 should be implemented");

        // Write with M=1 first, then M=0
        mapper.write_prg(0x8000, 0b0100_0000); // M=1
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);

        mapper.write_prg(0x8000, 0b0000_0000); // M=0
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenLower,
            "M=0 should select 1-screen A"
        );
    }

    #[test]
    fn mirroring_bit_one_sets_one_screen_b() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);

        let mut mapper =
            create_mapper154(prg_rom, chr_rom).expect("Mapper 154 should be implemented");

        mapper.write_prg(0x8000, 0b0100_0000); // M=1 via bank select
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenUpper,
            "M=1 should select 1-screen B"
        );
    }

    #[test]
    fn mirroring_bit_applies_over_entire_32kb_range() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);

        let mut mapper =
            create_mapper154(prg_rom, chr_rom).expect("Mapper 154 should be implemented");

        // M bit present at any address in $8000-$FFFF
        mapper.write_prg(0x8000, 0b0100_0000); // M=1
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);

        mapper.write_prg(0xA000, 0b0000_0000); // M=0 at $A000
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenLower,
            "M bit should apply at $A000 too"
        );

        mapper.write_prg(0xC000, 0b0100_0000); // M=1 at $C000
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenUpper,
            "M bit should apply at $C000 too"
        );

        mapper.write_prg(0xFFFF, 0b0000_0000); // M=0 at $FFFF
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenLower,
            "M bit should apply at $FFFF too"
        );
    }

    #[test]
    fn bank_data_writes_also_update_mirroring() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);

        let mut mapper =
            create_mapper154(prg_rom, chr_rom).expect("Mapper 154 should be implemented");

        // Bank data write with M=1 bit set also updates mirroring
        mapper.write_prg(0x8000, 0b0000_0110); // Select R6 (M=0)
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        mapper.write_prg(0x8001, 0b0100_0001); // Bank data with M=1 bit
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenUpper,
            "M bit in bank data write should also update mirroring"
        );
    }

    #[test]
    fn irq_never_asserted() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);

        let mut mapper =
            create_mapper154(prg_rom, chr_rom).expect("Mapper 154 should be implemented");

        for _ in 0..5 {
            mapper.ppu_address_changed(0x1000);
            mapper.ppu_scanline(0, true);
            mapper.cpu_cycle();
            assert!(
                !mapper.irq_pending(),
                "IRQ must never be asserted for mapper 154"
            );
        }
    }

    // ---------- Snapshot/restore tests ----------

    #[test]
    fn registers_snapshot_restore_roundtrip_single_screen_lower() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 128);

        let mut mapper = create_mapper154(prg_rom.clone(), chr_rom.clone())
            .expect("Mapper 154 should be implemented");

        mapper.write_prg(0x8000, 0b0100_0110); // select R6; M=1 → mirroring = SingleScreenUpper
        mapper.write_prg(0x8001, 3); // value 3 has bit 6 = 0 → reverts mirroring to SingleScreenLower
        mapper.write_prg(0x8000, 0b0000_0111); // select R7; M=0 → mirroring = SingleScreenLower
        mapper.write_prg(0x8001, 5); // value 5 has bit 6 = 0 → mirroring stays SingleScreenLower

        // After all writes, mirroring is SingleScreenLower
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        let snap = mapper.registers_snapshot();

        let mut restored =
            create_mapper154(prg_rom, chr_rom).expect("Mapper 154 should be implemented");
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), 3, "R6=3 should map at $8000");
        assert_eq!(restored.read_prg(0xA000), 5, "R7=5 should map at $A000");
        assert_eq!(
            restored.get_mirroring(),
            NametableLayout::SingleScreenLower,
            "SingleScreenLower mirroring should be restored"
        );
    }

    #[test]
    fn registers_snapshot_restore_roundtrip_single_screen_upper() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 128);

        let mut mapper = create_mapper154(prg_rom.clone(), chr_rom.clone())
            .expect("Mapper 154 should be implemented");

        mapper.write_prg(0x8000, 0b0000_0110); // select R6; M=0 → mirroring = SingleScreenLower
        mapper.write_prg(0x8001, 0b0100_0001); // value has bit 6 = 1 → mirroring = SingleScreenUpper

        // After bank data write with M=1, mirroring is SingleScreenUpper
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);

        let snap = mapper.registers_snapshot();

        let mut restored =
            create_mapper154(prg_rom, chr_rom).expect("Mapper 154 should be implemented");
        restored.restore_registers(&snap);

        assert_eq!(
            restored.get_mirroring(),
            NametableLayout::SingleScreenUpper,
            "SingleScreenUpper mirroring should be restored"
        );
    }

    // ---------- Factory test ----------

    #[test]
    fn mapper154_instantiates_via_factory() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        assert!(
            create_mapper154(prg_rom, chr_rom).is_ok(),
            "Mapper 154 must be creatable via the factory"
        );
    }
}
