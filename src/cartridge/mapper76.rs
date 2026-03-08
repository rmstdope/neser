//! Mapper 076 - Namco 109 (Namco 108 chip, 2 KB CHR banking via registers 2–5)
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::{Mapper, MapperCapabilities};

/// Mapper 076 - Namco 109 / Namco 108 (2 KB CHR pages via registers 2–5)
///
/// Hardware: Namco 108 chip with CHR page size wired to 2 KB.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_076>
/// - Related: <https://www.nesdev.org/wiki/Namco_108>
/// - PRG-ROM: Up to 512 KB (switchable 8 KB banks at $8000/$A000, fixed at $C000/$E000)
/// - CHR-ROM: 8 KB (four 2 KB banks mapped through registers 2–5)
/// - Mirroring: Fixed from cartridge header (not programmable)
///
/// CHR Banking:
/// - Four 2 KB CHR pages cover the full $0000–$1FFF address space.
/// - R2 → $0000–$07FF, R3 → $0800–$0FFF, R4 → $1000–$17FF, R5 → $1800–$1FFF
/// - Registers 0 and 1 are accepted but have no effect on CHR.
/// - CHR mode bit (bit 7 of bank select) is always forced to 0.
///
/// PRG Banking:
/// - PRG mode bit (bit 6 of bank select) is always forced to 0.
/// - R6 selects 8 KB at $8000–$9FFF
/// - R7 selects 8 KB at $A000–$BFFF
/// - $C000–$DFFF fixed to second-last 8 KB bank
/// - $E000–$FFFF fixed to last 8 KB bank
///
/// Notes:
/// - All writes to $8000–$FFFF are redirected to $8000/$8001 (no mirroring, no IRQ)
/// - Used in: Megami Tensei: Digital Devil Story (Famicom)
pub struct Mapper76 {
    base: BaseMapper,

    bank_select: u8,
    regs: [u8; 8],
}

impl Mapper76 {
    const REG_SELECT_MASK: u8 = 0b0000_0111;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
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
        // PRG mode is always 0: R6@$8000, R7@$A000, -2@$C000, -1@$E000
        let r6 = self.regs[6] as i16;
        let r7 = self.regs[7] as i16;
        self.base.select_prg_page(0, r6);
        self.base.select_prg_page(1, r7);
        self.base.select_prg_page(2, -2);
        self.base.select_prg_page(3, -1);

        // CHR mode is always 0: R2–R5 each select one 2 KB page ($0000–$1FFF).
        // R0, R1 are accepted but unused for CHR.
        self.base.select_chr_page(0, self.regs[2] as i16);
        self.base.select_chr_page(1, self.regs[3] as i16);
        self.base.select_chr_page(2, self.regs[4] as i16);
        self.base.select_chr_page(3, self.regs[5] as i16);
    }
}

impl Mapper for Mapper76 {
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
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_mapper76(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(76, prg_rom, chr_rom, mirroring))
    }

    // ---------- Factory / instantiation test ----------

    #[test]
    fn mapper76_instantiates_via_factory() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(2 * 1024, 4);
        assert!(
            create_mapper76(prg_rom, chr_rom, NametableLayout::Horizontal).is_ok(),
            "Mapper 76 must be creatable via the factory"
        );
    }

    // ---------- PRG banking tests ----------

    #[test]
    fn prg_r6_r7_switchable_c000_e000_fixed() {
        // R6 → $8000, R7 → $A000, second-last → $C000, last → $E000 (mode always 0).
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

    #[test]
    fn prg_mode_bit_is_always_forced_to_zero() {
        // Writing bank_select with bit 6 set must not switch to PRG mode 1.
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(2 * 1024, 4);

        let mut mapper = create_mapper76(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("Mapper 76 should be implemented");

        // Set R6=3 with PRG mode bit 6 set — must still apply mode 0 layout.
        mapper.write_prg(0x8000, 0b0100_0110); // register 6 with bit 6 set
        mapper.write_prg(0x8001, 3);

        // Mode 0: R6 @ $8000 (=3), second-last @ $C000 (=6)
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "$8000 should be R6=3 (mode 0 forced)"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "$C000 should be second-last (mode 0 forced)"
        );
    }

    // ---------- CHR banking tests ----------

    #[test]
    fn chr_r2_r5_select_2kb_banks() {
        // R2–R5 each select one 2 KB CHR bank.
        // 4 × 2 KB = 8 KB total; registers cover $0000–$1FFF.
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(2 * 1024, 16); // 16 × 2 KB banks

        let mut mapper = create_mapper76(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 76 should be implemented");

        // Write R2=1, R3=2, R4=3, R5=4
        mapper.write_prg(0x8000, 0b0000_0010); // select R2
        mapper.write_prg(0x8001, 1);
        mapper.write_prg(0x8000, 0b0000_0011); // select R3
        mapper.write_prg(0x8001, 2);
        mapper.write_prg(0x8000, 0b0000_0100); // select R4
        mapper.write_prg(0x8001, 3);
        mapper.write_prg(0x8000, 0b0000_0101); // select R5
        mapper.write_prg(0x8001, 4);

        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "$0000 should be CHR bank 1 (R2=1)"
        );
        assert_eq!(
            mapper.read_chr(0x0800),
            2,
            "$0800 should be CHR bank 2 (R3=2)"
        );
        assert_eq!(
            mapper.read_chr(0x1000),
            3,
            "$1000 should be CHR bank 3 (R4=3)"
        );
        assert_eq!(
            mapper.read_chr(0x1800),
            4,
            "$1800 should be CHR bank 4 (R5=4)"
        );
    }

    #[test]
    fn chr_r2_r5_independent_bank_selection() {
        // Verify each register maps independently (R2-R5 are not linked to each other).
        // Using more than 4 banks to ensure no wrap-around confusion.
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(2 * 1024, 12); // 12 × 2 KB CHR banks

        let mut mapper = create_mapper76(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 76 should be implemented");

        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 5);
        mapper.write_prg(0x8000, 0b0000_0011); // R3
        mapper.write_prg(0x8001, 7);
        mapper.write_prg(0x8000, 0b0000_0100); // R4
        mapper.write_prg(0x8001, 9);
        mapper.write_prg(0x8000, 0b0000_0101); // R5
        mapper.write_prg(0x8001, 11);

        assert_eq!(mapper.read_chr(0x0000), 5, "$0000 = CHR bank 5 (R2=5)");
        assert_eq!(mapper.read_chr(0x0800), 7, "$0800 = CHR bank 7 (R3=7)");
        assert_eq!(mapper.read_chr(0x1000), 9, "$1000 = CHR bank 9 (R4=9)");
        assert_eq!(mapper.read_chr(0x1800), 11, "$1800 = CHR bank 11 (R5=11)");
    }

    #[test]
    fn chr_r0_r1_writes_accepted_but_no_chr_effect() {
        // Writing to R0 or R1 must not affect CHR mapping (only R2-R5 are used for CHR).
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(2 * 1024, 8);

        let mut mapper = create_mapper76(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 76 should be implemented");

        // Set known CHR state first (R2=2, R3=3)
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 2);
        mapper.write_prg(0x8000, 0b0000_0011); // R3
        mapper.write_prg(0x8001, 3);

        // Now write to R0, R1 — these must not move CHR mapping
        mapper.write_prg(0x8000, 0b0000_0000); // R0
        mapper.write_prg(0x8001, 7); // arbitrary value
        mapper.write_prg(0x8000, 0b0000_0001); // R1
        mapper.write_prg(0x8001, 6); // arbitrary value

        // CHR banks 2 and 3 should remain at their positions
        assert_eq!(
            mapper.read_chr(0x0000),
            2,
            "$0000 should still be CHR bank 2 (R2=2, unaffected by R0/R1 writes)"
        );
        assert_eq!(
            mapper.read_chr(0x0800),
            3,
            "$0800 should still be CHR bank 3 (R3=3, unaffected by R0/R1 writes)"
        );
    }

    #[test]
    fn chr_mode_bit_is_always_forced_to_zero() {
        // Writing bank_select with bit 7 set must not change CHR layout.
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(2 * 1024, 8);

        let mut mapper = create_mapper76(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 76 should be implemented");

        // Write R2=3 with CHR mode bit 7 set → must still map at $0000
        mapper.write_prg(0x8000, 0b1000_0010); // R2 with bit 7 set
        mapper.write_prg(0x8001, 3);

        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "$0000 should be CHR bank 3 (mode 0 forced, R2=3)"
        );
    }

    // ---------- Mirroring tests ----------

    #[test]
    fn mirroring_is_fixed_from_header() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(2 * 1024, 4);

        let mut mapper = create_mapper76(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 76 should be implemented");

        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        // High-address writes must not change mirroring
        mapper.write_prg(0xA000, 1);
        mapper.write_prg(0xC000, 5);
        mapper.write_prg(0xE000, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_vertical_from_header() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(2 * 1024, 4);

        let mapper = create_mapper76(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("Mapper 76 should be implemented");

        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ---------- IRQ tests ----------

    #[test]
    fn irq_never_asserted() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(2 * 1024, 4);

        let mut mapper = create_mapper76(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 76 should be implemented");

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
        let chr_rom = banked_data(2 * 1024, 16);

        let mut mapper =
            create_mapper76(prg_rom.clone(), chr_rom.clone(), NametableLayout::Vertical)
                .expect("Mapper 76 should be implemented");

        // Set R6=1, R7=2, R2=5, R3=7, R4=9, R5=11
        mapper.write_prg(0x8000, 0b0000_0110);
        mapper.write_prg(0x8001, 1);
        mapper.write_prg(0x8000, 0b0000_0111);
        mapper.write_prg(0x8001, 2);
        mapper.write_prg(0x8000, 0b0000_0010);
        mapper.write_prg(0x8001, 5);
        mapper.write_prg(0x8000, 0b0000_0011);
        mapper.write_prg(0x8001, 7);

        let snap = mapper.registers_snapshot();

        let mut restored = create_mapper76(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("Mapper 76 should be implemented");
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            1,
            "R6=1 should map bank 1 at $8000 after restore"
        );
        assert_eq!(
            restored.read_prg(0xA000),
            2,
            "R7=2 should map bank 2 at $A000 after restore"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            5,
            "R2=5 should map CHR bank 5 at $0000 after restore"
        );
        assert_eq!(
            restored.read_chr(0x0800),
            7,
            "R3=7 should map CHR bank 7 at $0800 after restore"
        );
    }
}
