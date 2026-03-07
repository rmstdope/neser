//! Mapper 088 - Namco 118 (Namco 108 chip, CHR A12 wired to CHR A16)
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::{Mapper, MapperCapabilities};

/// Mapper 088 - Namco 118 / Namco 108 (CHR A12→A16 variant)
///
/// Hardware: Namco 108 chip on boards where CHR address line A12 is wired to CHR A16.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_088>
/// - Related: <https://www.nesdev.org/wiki/Namco_108>
/// - PRG-ROM: Up to 512 KB (switchable 8 KB banks at $8000/$A000, fixed at $C000/$E000)
/// - CHR-ROM: 128 KB (CHR A12 wired to A16 splits access into two halves)
/// - Mirroring: Fixed from cartridge header (not programmable)
///
/// CHR Banking differences vs. Mapper 206:
/// - R0, R1 are 6-bit (bits 0–5 only); they select 2 KB banks from lower 64 KB of CHR
/// - R2–R5 always have bit 6 forced high; they select 1 KB banks from upper 64 KB of CHR
/// - CHR mode bit (bit 7 of bank select) is always forced to 0 → left PT = 2 KB, right PT = 1 KB
///
/// PRG Banking:
/// - PRG mode bit (bit 6 of bank select) is always forced to 0
/// - R6 selects 8 KB at $8000–$9FFF
/// - R7 selects 8 KB at $A000–$BFFF
/// - $C000–$DFFF fixed to second-last 8 KB bank
/// - $E000–$FFFF fixed to last 8 KB bank
///
/// Notes:
/// - All writes to $8000–$FFFF are redirected to $8000/$8001 (no mirroring, no IRQ)
/// - Used in games such as Dragon Spirit: The New Legend, Youkai Douchuuki
pub struct Mapper88 {
    base: BaseMapper,

    bank_select: u8,
    regs: [u8; 8],
}

impl Mapper88 {
    const REG_SELECT_MASK: u8 = 0b0000_0111;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(8 * 1024);
        base.configure_chr_banking(1024);
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

        // CHR mode is always 0: R0/R1 → 2 KB at $0000–$0FFF; R2–R5 → 1 KB at $1000–$1FFF
        // R0, R1 are 6-bit (lower 64 KB of CHR); R2–R5 have bit 6 forced high (upper 64 KB).
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
}

impl Mapper for Mapper88 {
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
        // Bank select (even): bit 7 (CHR mode) and bit 6 (PRG mode) are ignored.
        // Bank data (odd): written to selected register.
        if addr >= 0x8000 {
            match addr & 0x8001 {
                0x8000 => {
                    self.bank_select = value & Self::REG_SELECT_MASK;
                    self.update_banks();
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

    fn create_mapper88(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(88, prg_rom, chr_rom, mirroring))
    }

    // ---------- PRG banking tests ----------

    #[test]
    fn prg_mode0_r6_r7_switchable_c000_e000_fixed() {
        // PRG mode is always 0 (bit 6 of bank select forced to 0).
        // R6 → $8000–$9FFF, R7 → $A000–$BFFF, second-last → $C000, last → $E000.
        let prg_rom = banked_data(8 * 1024, 8); // 8 banks of 8 KB
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_mapper88(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("Mapper 88 should be implemented");

        mapper.write_prg(0x8000, 0b0000_0110); // select R6
        mapper.write_prg(0x8001, 1);
        mapper.write_prg(0x8000, 0b0000_0111); // select R7
        mapper.write_prg(0x8001, 2);

        // R6=1 → $8000, R7=2 → $A000, fixed second-last (6) @ $C000, last (7) @ $E000
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
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_mapper88(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("Mapper 88 should be implemented");

        // Set R6=3 with PRG mode bit 6 set — must still apply mode 0 layout.
        mapper.write_prg(0x8000, 0b0100_0110); // register 6 with bit 6 set
        mapper.write_prg(0x8001, 3);

        // Mode 0 expected: R6 @ $8000 (=3), second-last @ $C000 (=6)
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

    // ---------- CHR banking tests ----------

    #[test]
    fn chr_r0_r1_select_2kb_banks_from_lower_chr_half() {
        // R0 and R1 are 6-bit (0x3F mask, even-aligned) and address the lower 64 KB of CHR.
        // Using 128 CHR 1 KB banks (banked_data fills bank N with byte value N).
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 128);

        let mut mapper = create_mapper88(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 88 should be implemented");

        // Set R0=4 (even-aligned 2KB bank 4→5) at $0000–$07FF
        mapper.write_prg(0x8000, 0b0000_0000); // select R0
        mapper.write_prg(0x8001, 4);
        // Set R1=6 (even-aligned 2KB bank 6→7) at $0800–$0FFF
        mapper.write_prg(0x8000, 0b0000_0001); // select R1
        mapper.write_prg(0x8001, 6);

        assert_eq!(
            mapper.read_chr(0x0000),
            4,
            "$0000 should be CHR bank 4 (R0=4)"
        );
        assert_eq!(
            mapper.read_chr(0x0400),
            5,
            "$0400 should be CHR bank 5 (R0+1=5)"
        );
        assert_eq!(
            mapper.read_chr(0x0800),
            6,
            "$0800 should be CHR bank 6 (R1=6)"
        );
        assert_eq!(
            mapper.read_chr(0x0C00),
            7,
            "$0C00 should be CHR bank 7 (R1+1=7)"
        );
    }

    #[test]
    fn chr_r2_r5_select_1kb_banks_from_upper_chr_half() {
        // R2–R5 always have bit 6 set → bank indices 64–127 (upper 64 KB of CHR).
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 128);

        let mut mapper = create_mapper88(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 88 should be implemented");

        // Write R2=0, R3=1, R4=2, R5=3 (effective banks 64, 65, 66, 67)
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 0);
        mapper.write_prg(0x8000, 0b0000_0011); // R3
        mapper.write_prg(0x8001, 1);
        mapper.write_prg(0x8000, 0b0000_0100); // R4
        mapper.write_prg(0x8001, 2);
        mapper.write_prg(0x8000, 0b0000_0101); // R5
        mapper.write_prg(0x8001, 3);

        // 0x40 = 64; R2|0x40=64, R3|0x40=65, R4|0x40=66, R5|0x40=67
        assert_eq!(
            mapper.read_chr(0x1000),
            64,
            "$1000 should be CHR bank 64 (R2=0|0x40)"
        );
        assert_eq!(
            mapper.read_chr(0x1400),
            65,
            "$1400 should be CHR bank 65 (R3=1|0x40)"
        );
        assert_eq!(
            mapper.read_chr(0x1800),
            66,
            "$1800 should be CHR bank 66 (R4=2|0x40)"
        );
        assert_eq!(
            mapper.read_chr(0x1C00),
            67,
            "$1C00 should be CHR bank 67 (R5=3|0x40)"
        );
    }

    #[test]
    fn chr_mode_bit_is_always_forced_to_zero() {
        // Writing bank_select with bit 7 set must not change the CHR layout.
        // Layout stays: 2 KB pairs at $0000–$0FFF, 1 KB at $1000–$1FFF.
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 128);

        let mut mapper = create_mapper88(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 88 should be implemented");

        // Write R0=4 with CHR mode bit 7 set → must still map as 2KB bank at $0000
        mapper.write_prg(0x8000, 0b1000_0000); // register 0, bit 7 set
        mapper.write_prg(0x8001, 4);

        // If CHR mode 1 were incorrectly applied, R0 would go to $1000 not $0000.
        // With correct mode 0 forced: CHR bank 4 at $0000 (and 5 at $0400).
        assert_eq!(
            mapper.read_chr(0x0000),
            4,
            "$0000 should be CHR bank 4 (mode 0 forced, R0=4)"
        );
        assert_eq!(
            mapper.read_chr(0x0400),
            5,
            "$0400 should be CHR bank 5 (R0+1=5, mode 0 forced)"
        );
    }

    #[test]
    fn chr_r0_upper_bits_ignored_6bit_mask() {
        // R0 and R1 are 6-bit only (mask 0x3F). Bits 6–7 must be ignored.
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 128);

        let mut mapper = create_mapper88(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 88 should be implemented");

        // Write R0=0b1100_0100 (0xC4). After masking: 0xC4 & 0x3E = 0x04 = 4.
        // Even-aligned: 4 → pages 4 and 5 at $0000–$07FF.
        mapper.write_prg(0x8000, 0b0000_0000); // register 0
        mapper.write_prg(0x8001, 0xC4);

        // 0xC4 & 0x3E = 0x04; bank 4 at $0000, bank 5 at $0400
        assert_eq!(
            mapper.read_chr(0x0000),
            4,
            "$0000 should be CHR bank 4 (R0=0xC4 masked to 4)"
        );
        assert_eq!(
            mapper.read_chr(0x0400),
            5,
            "$0400 should be CHR bank 5 (R0+1=5)"
        );
    }

    // ---------- Mirroring and IRQ tests ----------

    #[test]
    fn mirroring_is_fixed_from_header_writes_to_high_addresses_ignored() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_mapper88(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 88 should be implemented");

        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        // Writes to $A000–$FFFF must not change mirroring
        mapper.write_prg(0xA000, 1);
        mapper.write_prg(0xC000, 5);
        mapper.write_prg(0xE000, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn irq_never_asserted() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_mapper88(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 88 should be implemented");

        // IRQ-related writes (if any were routed) must never assert IRQ
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
                "IRQ must never be asserted for mapper 88"
            );
        }
    }

    // ---------- Snapshot/restore tests ----------

    #[test]
    fn registers_snapshot_restore_roundtrip() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 128);

        let mut mapper =
            create_mapper88(prg_rom.clone(), chr_rom.clone(), NametableLayout::Vertical)
                .expect("Mapper 88 should be implemented");

        // Set R6=3, R7=5, R0=4, R2=2 (effective CHR bank 66)
        mapper.write_prg(0x8000, 0b0000_0110);
        mapper.write_prg(0x8001, 3);
        mapper.write_prg(0x8000, 0b0000_0111);
        mapper.write_prg(0x8001, 5);
        mapper.write_prg(0x8000, 0b0000_0000);
        mapper.write_prg(0x8001, 4);
        mapper.write_prg(0x8000, 0b0000_0100); // R4
        mapper.write_prg(0x8001, 2); // effective CHR bank 0x42=66

        let snap = mapper.registers_snapshot();

        let mut restored = create_mapper88(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("Mapper 88 should be implemented");
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
            "R0=4 should map CHR bank 4 at $0000 after restore"
        );
        assert_eq!(
            restored.read_chr(0x1800),
            66,
            "R4=2|0x40=66 should map CHR at $1800 after restore"
        );
    }

    // ---------- Factory / instantiation test ----------

    #[test]
    fn mapper88_instantiates_via_factory() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        assert!(
            create_mapper88(prg_rom, chr_rom, NametableLayout::Horizontal).is_ok(),
            "Mapper 88 must be creatable via the factory"
        );
    }
}
