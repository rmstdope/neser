//! Mapper 076 - Namco 109 (Megami Tensei)
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::{Mapper, MapperCapabilities};

/// Mapper 076 - Namco 109 (Megami Tensei: Digital Devil Story)
///
/// Hardware: Namco 109 chip — a Namco 108 / Namco 206-family board with all-2 KB CHR banking.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_076>
/// - Related: <https://www.nesdev.org/wiki/Namco_108>
/// - PRG-ROM: Up to 512 KB (two switchable 8 KB banks at $8000/$A000, last two fixed)
/// - CHR-ROM: 4 × 2 KB switchable banks mapping the full 8 KB CHR space
/// - Mirroring: Fixed from cartridge header (not programmable)
///
/// PRG Banking (same as Namco 108 / mapper 206):
/// - R6 selects 8 KB at $8000–$9FFF
/// - R7 selects 8 KB at $A000–$BFFF
/// - $C000–$DFFF fixed to second-last 8 KB bank
/// - $E000–$FFFF fixed to last 8 KB bank
/// - PRG mode bit (bit 6 of bank select) is always forced to 0
///
/// CHR Banking (distinct from other Namco 108 variants):
/// - R2 selects 2 KB at $0000–$07FF
/// - R3 selects 2 KB at $0800–$0FFF
/// - R4 selects 2 KB at $1000–$17FF
/// - R5 selects 2 KB at $1800–$1FFF
/// - R0 and R1 are unused for CHR (writes accepted but ignored for banking)
/// - CHR mode bit (bit 7 of bank select) is always forced to 0
///
/// Notes:
/// - All writes to $8000–$FFFF are redirected to $8000/$8001 (no IRQ, no mirroring control)
/// - Used in Megami Tensei: Digital Devil Story
pub struct Namcot3446Mapper {
    base: BaseMapper,

    bank_select: u8,
    regs: [u8; 8],
}

impl Namcot3446Mapper {
    const REG_SELECT_MASK: u8 = 0b0000_0111;

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 2,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(8 * 1024);
        base.configure_chr_banking(2 * 1024);
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
        // PRG mode is always 0: R6@$8000, R7@$A000, second-last@$C000, last@$E000
        let r6 = self.regs[6] as i16;
        let r7 = self.regs[7] as i16;
        self.base.select_prg_page(0, r6);
        self.base.select_prg_page(1, r7);
        self.base.select_prg_page(2, -2);
        self.base.select_prg_page(3, -1);

        // CHR mode always 0: R2–R5 each select a 2 KB page
        // R0 and R1 are unused for CHR in mapper 76
        let r2 = self.regs[2] as i16;
        let r3 = self.regs[3] as i16;
        let r4 = self.regs[4] as i16;
        let r5 = self.regs[5] as i16;
        self.base.select_chr_page(0, r2);
        self.base.select_chr_page(1, r3);
        self.base.select_chr_page(2, r4);
        self.base.select_chr_page(3, r5);
    }
}

impl Mapper for Namcot3446Mapper {
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
        // All writes $8000–$FFFF are redirected to $8000/$8001 only.
        // Bank data (odd): written to selected register.
        if addr >= 0x8000 {
            match addr & 0x8001 {
                0x8000 => {
                    // Bits 6–7 (PRG/CHR mode) are always forced to 0; bits 3–5 are preserved
                    // in the snapshot but unused. Bits 0–2 select the target register (0–7).
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
        let mut snapshot = Vec::with_capacity(9);
        snapshot.push(self.bank_select);
        snapshot.extend_from_slice(&self.regs);
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 9 {
            self.bank_select = data[0];
            self.regs.copy_from_slice(&data[1..9]);
            self.update_banks();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    fn create_mapper76(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(76, prg_rom, chr_rom, mirroring))
    }

    // ---------- PRG banking tests ----------

    #[test]
    fn prg_mode0_r6_r7_switchable_c000_e000_fixed() {
        // PRG mode always 0: R6→$8000, R7→$A000, second-last→$C000, last→$E000
        let prg_rom = banked_data(8 * 1024, 8); // 8 banks × 8 KB
        let chr_rom = banked_data(2 * 1024, 4);

        let mut mapper = create_mapper76(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("Mapper 76 should be implemented");

        mapper.write_prg(0x8000, 0b0000_0110); // select R6
        mapper.write_prg(0x8001, 1);
        mapper.write_prg(0x8000, 0b0000_0111); // select R7
        mapper.write_prg(0x8001, 2);

        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "$8000 should be PRG bank 1 (R6=1)"
        );
        assert_eq!(
            mapper.read_prg(0xA000),
            2,
            "$A000 should be PRG bank 2 (R7=2)"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "$C000 should be second-last bank (6)"
        );
        assert_eq!(mapper.read_prg(0xE000), 7, "$E000 should be last bank (7)");
    }

    #[test]
    fn prg_mode_bit_is_always_forced_to_zero() {
        // Writing bank_select with bit 6 set must not switch PRG to mode 1.
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(2 * 1024, 4);

        let mut mapper = create_mapper76(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("Mapper 76 should be implemented");

        // Set R6=3 with PRG mode bit 6 set — must still apply mode 0 layout.
        mapper.write_prg(0x8000, 0b0100_0110); // register 6 with bit 6 set
        mapper.write_prg(0x8001, 3);

        // Mode 0 expected: R6 @ $8000 (=3), second-last (6) @ $C000
        // Mode 1 (wrong) would place second-last @ $8000 and R6 @ $C000
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

    #[test]
    fn prg_high_addr_writes_all_redirect_to_bank_registers() {
        // Writes to $A000-$FFFF must redirect to bank select/data, not IRQ/mirroring.
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(2 * 1024, 4);

        let mut mapper = create_mapper76(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("Mapper 76 should be implemented");

        // High-address write to $A000 (odd bit clear) must behave as $8000 bank select.
        // Here we select PRG register R7.
        mapper.write_prg(0xA000, 0b0000_0111); // select R7 (redirected from $A000 to $8000)
        // High-address write to $A001 (odd bit set) must behave as $8001 bank data for
        // the currently selected register (R7).
        mapper.write_prg(0xA001, 4); // write 4 to R7 (redirected from $A001 to $8001)

        assert_eq!(
            mapper.read_prg(0xA000),
            4,
            "$A000 should map to bank 4 (R7=4 set via high-addr redirect)"
        );
    }

    // ---------- CHR banking tests ----------

    #[test]
    fn chr_r2_r5_each_select_2kb_bank() {
        // R2-R5 each select a 2 KB CHR page:
        //   R2 → $0000–$07FF, R3 → $0800–$0FFF, R4 → $1000–$17FF, R5 → $1800–$1FFF
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(2 * 1024, 12); // 12 × 2KB CHR banks (non-power-of-two)

        let mut mapper = create_mapper76(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 76 should be implemented");

        mapper.write_prg(0x8000, 0b0000_0010); // select R2
        mapper.write_prg(0x8001, 3);
        mapper.write_prg(0x8000, 0b0000_0011); // select R3
        mapper.write_prg(0x8001, 4);
        mapper.write_prg(0x8000, 0b0000_0100); // select R4
        mapper.write_prg(0x8001, 5);
        mapper.write_prg(0x8000, 0b0000_0101); // select R5
        mapper.write_prg(0x8001, 6);

        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "$0000 should be CHR bank 3 (R2=3)"
        );
        assert_eq!(
            mapper.read_chr(0x0400),
            3,
            "$0400 should also be CHR bank 3 (same 2KB page)"
        );
        assert_eq!(
            mapper.read_chr(0x0800),
            4,
            "$0800 should be CHR bank 4 (R3=4)"
        );
        assert_eq!(
            mapper.read_chr(0x0C00),
            4,
            "$0C00 should also be CHR bank 4 (same 2KB page)"
        );
        assert_eq!(
            mapper.read_chr(0x1000),
            5,
            "$1000 should be CHR bank 5 (R4=5)"
        );
        assert_eq!(
            mapper.read_chr(0x1400),
            5,
            "$1400 should also be CHR bank 5 (same 2KB page)"
        );
        assert_eq!(
            mapper.read_chr(0x1800),
            6,
            "$1800 should be CHR bank 6 (R5=6)"
        );
        assert_eq!(
            mapper.read_chr(0x1C00),
            6,
            "$1C00 should also be CHR bank 6 (same 2KB page)"
        );
    }

    #[test]
    fn chr_r0_r1_do_not_affect_chr_banks() {
        // R0 and R1 are unused for CHR in mapper 76; writing them must not change CHR mapping.
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(2 * 1024, 12);

        let mut mapper = create_mapper76(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 76 should be implemented");

        // Set up R2=3 so $0000 maps to bank 3
        mapper.write_prg(0x8000, 0b0000_0010);
        mapper.write_prg(0x8001, 3);

        // Now write a different value to R0 and R1 — CHR mapping at $0000 must not change
        mapper.write_prg(0x8000, 0b0000_0000); // select R0
        mapper.write_prg(0x8001, 7);
        mapper.write_prg(0x8000, 0b0000_0001); // select R1
        mapper.write_prg(0x8001, 8);

        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "$0000 should still be CHR bank 3 (R0/R1 do not affect CHR)"
        );
    }

    #[test]
    fn chr_mode_bit_is_always_forced_to_zero() {
        // Writing bank_select with bit 7 set must not change the CHR layout.
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(2 * 1024, 12);

        let mut mapper = create_mapper76(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 76 should be implemented");

        // Write R2=5 with CHR mode bit 7 set — layout must stay as 4 × 2KB
        mapper.write_prg(0x8000, 0b1000_0010); // register 2, bit 7 set
        mapper.write_prg(0x8001, 5);

        // With mode 0 forced: R2=5 maps 2KB at $0000-$07FF
        assert_eq!(
            mapper.read_chr(0x0000),
            5,
            "$0000 should be CHR bank 5 (R2=5, CHR mode 0 forced)"
        );
        assert_eq!(
            mapper.read_chr(0x0400),
            5,
            "$0400 should be CHR bank 5 (same 2KB page, mode 0 forced)"
        );
    }

    // ---------- Mirroring and IRQ tests ----------

    #[test]
    fn mirroring_is_fixed_from_header() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(2 * 1024, 4);

        let mut mapper_h = create_mapper76(
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
        )
        .expect("Mapper 76 should be implemented");

        let mut mapper_v = create_mapper76(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("Mapper 76 should be implemented");

        assert_eq!(mapper_h.get_mirroring(), NametableLayout::Horizontal);
        assert_eq!(mapper_v.get_mirroring(), NametableLayout::Vertical);

        // Writes to any address must not change mirroring
        mapper_h.write_prg(0xA000, 1);
        mapper_h.write_prg(0xC000, 5);
        assert_eq!(mapper_h.get_mirroring(), NametableLayout::Horizontal);
        mapper_v.write_prg(0xA000, 0);
        assert_eq!(mapper_v.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn irq_never_asserted() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(2 * 1024, 4);

        let mut mapper = create_mapper76(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 76 should be implemented");

        mapper.write_prg(0xC000, 1);
        mapper.write_prg(0xC001, 0);
        mapper.write_prg(0xE000, 0);
        mapper.write_prg(0xE001, 0);

        for _ in 0..5 {
            mapper.ppu_address_changed(0x1000);
            mapper.ppu_scanline(0, true);
            mapper.cpu_cycle();
            assert!(
                !mapper.irq_pending(),
                "IRQ must never be asserted for mapper 76"
            );
        }
    }

    // ---------- Snapshot/restore tests ----------

    #[test]
    fn registers_snapshot_restore_roundtrip() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(2 * 1024, 12);

        let mut mapper =
            create_mapper76(prg_rom.clone(), chr_rom.clone(), NametableLayout::Vertical)
                .expect("Mapper 76 should be implemented");

        // Set R6=3, R7=5, R2=4, R4=7
        mapper.write_prg(0x8000, 0b0000_0110);
        mapper.write_prg(0x8001, 3);
        mapper.write_prg(0x8000, 0b0000_0111);
        mapper.write_prg(0x8001, 5);
        mapper.write_prg(0x8000, 0b0000_0010);
        mapper.write_prg(0x8001, 4);
        mapper.write_prg(0x8000, 0b0000_0100);
        mapper.write_prg(0x8001, 7);

        let snap = mapper.registers_snapshot();

        let mut restored = create_mapper76(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("Mapper 76 should be implemented");
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            3,
            "R6=3 should map bank 3 at $8000 after restore"
        );
        assert_eq!(
            restored.read_prg(0xA000),
            5,
            "R7=5 should map bank 5 at $A000 after restore"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            4,
            "R2=4 should map CHR bank 4 at $0000 after restore"
        );
        assert_eq!(
            restored.read_chr(0x1000),
            7,
            "R4=7 should map CHR bank 7 at $1000 after restore"
        );
    }

    // ---------- Factory / instantiation test ----------

    #[test]
    fn mapper76_instantiates_via_factory() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(2 * 1024, 8);
        assert!(
            create_mapper76(prg_rom, chr_rom, NametableLayout::Horizontal).is_ok(),
            "Mapper 76 must be creatable via the factory"
        );
    }
}
