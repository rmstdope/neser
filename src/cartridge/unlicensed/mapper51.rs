//! Mapper 051 - 11-in-1 Ball Games
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_051>
//! - Reference: Mesen2 `Bmc51.h`
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::BaseMapper;
use crate::cartridge::common::ChrMemory;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 051 - 11-in-1 Ball Games
///
/// Hardware: JY-010 PCB
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_051>
/// - Reference: Mesen2 `Bmc51.h`
/// - PRG-ROM: 512 KiB (8 KiB banks)
/// - CHR: 8 KiB fixed (ROM or RAM)
///
/// Registers:
/// - `bank` (4-bit): set by writes to $8000-$BFFF, $C000-$DFFF, $E000-$FFFF
/// - `mode` (2-bit): set by writes to $6000-$7FFF (full rewrite) or $C000-$DFFF (bit 1 only)
///   - bit 0: 0 = 16KB mode, 1 = 32KB mode
///   - bit 1: contributes to mirroring condition
///
/// Initial state (power-on/reset): `bank = 0`, `mode = 1` (32KB mode, Vertical mirroring)
///
/// PRG banking:
///   32KB mode (`mode & 0x01` set):
///     $6000: 8KB bank `0x23 | (bank << 2)`
///     $8000-$FFFF: 32KB bank at `bank << 2`
///   16KB mode (`mode & 0x01` clear):
///     $6000: 8KB bank `0x2F | (bank << 2)`
///     $8000-$BFFF: 16KB bank `(bank << 2) | mode`
///     $C000-$FFFF: 16KB bank `(bank << 2) | 0x0E`
///
/// Mirroring: `mode == 0x03` → Horizontal; else Vertical
///
/// CHR: Fixed 8KB bank 0
pub struct Mapper51 {
    base: BaseMapper,
    bank: u8,
    mode: u8,
}

const BAD_DUMP_CHR_ROM_ON_CHR_RAM_CRC32: u32 = 0xA912_B064;

impl Mapper51 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let submapper = ctx.submapper;
        let crc32 = ctx.crc32;
        let chr_rom_data = ctx.chr_rom.clone();

        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(0x2000); // 8KB
        base.configure_prg_6000_banking();

        // Handle bad-dump CRC32 case: replace CHR-ROM with CHR-RAM initialized
        // with the ROM data. Some bad dumps carry an 8KB CHR-ROM payload in the
        // header; keep those bytes as RAM init while preserving writable CHR
        // behavior for runtime graphics updates.
        if submapper == 0 && crc32 == BAD_DUMP_CHR_ROM_ON_CHR_RAM_CRC32 {
            let mut chr_memory = ChrMemory::new_ram(0x2000);
            if !chr_rom_data.is_empty() {
                chr_memory.load_snapshot(&chr_rom_data);
            }
            base.set_chr_memory(chr_memory);
        }

        let mut mapper = Self {
            base,
            bank: 0,
            mode: 1,
        };
        mapper.update_banks();
        mapper
    }

    fn decode_mode(value: u8) -> u8 {
        ((value >> 3) & 0x02) | ((value >> 1) & 0x01)
    }

    fn update_banks(&mut self) {
        let bank = self.bank as usize;

        // $6000-$7FFF: computed bank
        let prg_6000 = if self.mode & 0x01 != 0 {
            (0x23 | (bank << 2)) as i16
        } else {
            (0x2F | (bank << 2)) as i16
        };
        self.base.select_prg_6000_page(prg_6000);

        if self.mode & 0x01 != 0 {
            // 32KB mode: $8000-$FFFF mapped to 4 consecutive 8KB pages
            self.base.select_prg_page(0, (bank << 2) as i16);
            self.base.select_prg_page(1, ((bank << 2) | 1) as i16);
            self.base.select_prg_page(2, ((bank << 2) | 2) as i16);
            self.base.select_prg_page(3, ((bank << 2) | 3) as i16);
        } else {
            // 16KB mode: $8000 and $C000 are independent 16KB windows
            let mode = self.mode as usize;
            self.base.select_prg_page(0, ((bank << 2) | mode) as i16);
            self.base
                .select_prg_page(1, (((bank << 2) | mode) + 1) as i16);
            self.base.select_prg_page(2, ((bank << 2) | 0x0E) as i16);
            self.base.select_prg_page(3, ((bank << 2) | 0x0F) as i16);
        }

        self.base.set_mirroring_hv(self.mode == 0x03);
    }
}

impl Mapper for Mapper51 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                self.mode = Self::decode_mode(value);
                self.update_banks();
            }
            0xC000..=0xDFFF => {
                self.bank = value & 0x0F;
                self.mode = (Self::decode_mode(value) & 0x02) | (self.mode & 0x01);
                self.update_banks();
            }
            0x8000..=0xBFFF | 0xE000..=0xFFFF => {
                self.bank = value & 0x0F;
                self.update_banks();
            }
            _ => {}
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x0000..=0x5FFF => open_bus,
            _ => self.read_prg(addr),
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.bank, self.mode]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.bank = data[0];
            self.mode = data[1];
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.bank = 0;
        self.mode = 1;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::Mapper51;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // 48 banks × 8 KiB = 384 KiB (non-power-of-two to prevent modulo wrap false-passes)
    const PRG_BANKS: usize = 48;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, 1);
        create_mapper(MapperContext::new_for_test(
            51,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 51 should be implemented")
    }

    fn make_mapper_direct() -> Mapper51 {
        // Use Horizontal header so mirroring tests verify mode-derived value, not header passthrough
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, 1);
        Mapper51::new(MapperContext::new_for_test(
            51,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    fn make_mapper_with_submapper(submapper: u8, chr: Vec<u8>, crc32: u32) -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let mut ctx = MapperContext::new_for_test(51, prg, chr, NametableLayout::Vertical)
            .with_submapper(submapper);
        ctx.crc32 = crc32;
        create_mapper(ctx).expect("Mapper 51 should be implemented")
    }

    // --- Factory ---

    #[test]
    fn mapper_51_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            51,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(8 * 1024, 1),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 51 must be registered in the factory"
        );
    }

    // --- Default state (bank=0, mode=1 → 32KB mode) ---

    #[test]
    fn default_6000_window_is_fixed_bank_35() {
        // mode=1 (32KB), bank=0: 0x23 | (0 << 2) = 35
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x6000),
            35,
            "$6000 should read bank 35 by default"
        );
    }

    #[test]
    fn default_32kb_mode_maps_8000_to_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should read bank 0 in default 32KB mode"
        );
    }

    #[test]
    fn default_32kb_mode_maps_a000_to_bank_1() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xA000),
            1,
            "$A000 should read bank 1 in default 32KB mode"
        );
    }

    #[test]
    fn default_32kb_mode_maps_c000_to_bank_2() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            2,
            "$C000 should read bank 2 in default 32KB mode"
        );
    }

    #[test]
    fn default_32kb_mode_maps_e000_to_bank_3() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            3,
            "$E000 should read bank 3 in default 32KB mode"
        );
    }

    // --- PRG register ($8000-$FFFF) ---

    #[test]
    fn prg_register_selects_32kb_bank() {
        // bank=1, mode=1 (32KB): 8KB sub-banks 4, 5, 6, 7
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 1);
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "$8000 should read bank 4 after prg=1"
        );
        assert_eq!(
            mapper.read_prg(0xA000),
            5,
            "$A000 should read bank 5 after prg=1"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "$C000 should read bank 6 after prg=1"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            7,
            "$E000 should read bank 7 after prg=1"
        );
    }

    #[test]
    fn prg_register_updates_6000_bank_in_32kb_mode() {
        // bank=1, mode=1 (32KB): 0x23 | (1 << 2) = 35 | 4 = 39
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 1);
        assert_eq!(
            mapper.read_prg(0x6000),
            39,
            "$6000 should read bank 39 when prg=1 in 32KB mode"
        );
    }

    // --- Mode register ($6000-$7FFF) ---

    #[test]
    fn mode_bit1_clear_switches_to_16kb_mode() {
        // Write 0x00: mode = ((0>>3)&0x02)|((0>>1)&0x01) = 0 (16KB, Vertical)
        // bank=0: $8000=(0<<2)|0=0, $A000=1, $C000=(0<<2)|0x0E=14, $E000=15
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 0, "16KB mode $8000 reads bank 0");
        assert_eq!(mapper.read_prg(0xA000), 1, "16KB mode $A000 reads bank 1");
        assert_eq!(mapper.read_prg(0xC000), 14, "16KB mode $C000 reads bank 14");
        assert_eq!(mapper.read_prg(0xE000), 15, "16KB mode $E000 reads bank 15");
    }

    #[test]
    fn mode_bit1_clear_6000_window_is_bank_47() {
        // Write 0x00: mode=0, bank=0: 0x2F | (0 << 2) = 47
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x00);
        assert_eq!(
            mapper.read_prg(0x6000),
            47,
            "$6000 in 16KB mode with prg=0 should read bank 47"
        );
    }

    // --- Mirroring (derived from mode register, not header) ---

    #[test]
    fn default_mirroring_is_vertical_regardless_of_header() {
        // Initial mode=1, mode != 0x03 → Vertical
        // Constructor uses Horizontal header to ensure result is from mode, not header
        let mapper = make_mapper_direct();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Default mode=1 must produce Vertical mirroring"
        );
    }

    #[test]
    fn mode_bit4_set_gives_horizontal_mirroring() {
        // Write 0x12: mode = ((0x12>>3)&0x02)|((0x12>>1)&0x01) = 2|1 = 3 → Horizontal
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x6000, 0x12);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "mode bit 4 set must produce Horizontal mirroring"
        );
    }

    #[test]
    fn mode_bit4_clear_gives_vertical_mirroring() {
        // Write 0x00: mode=0, not 0x03 → Vertical
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x6000, 0x00);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "mode bit 4 clear must produce Vertical mirroring"
        );
    }

    // --- CHR ---

    #[test]
    fn chr_reads_from_fixed_bank_0() {
        // CHR is always fixed at bank 0; use 2-bank CHR so bank 1 would differ
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, 2); // bank 0 → 0, bank 1 → 1
        let mut mapper = Mapper51::new(MapperContext::new_for_test(
            51,
            prg,
            chr,
            NametableLayout::Vertical,
        ));
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR must read from bank 0");
        assert_eq!(mapper.read_chr(0x1FFF), 0, "CHR end must still be bank 0");
    }

    #[test]
    fn submapper0_with_chr_rom_header_data_still_uses_chr_ram() {
        // Compatibility case from issue #777 ROM: submapper 0 can appear with
        // bogus 8KB CHR-ROM data in bad dumps even though hardware uses CHR-RAM.
        let mut mapper = make_mapper_with_submapper(0, vec![0xAA; 8 * 1024], 0xA912_B064);
        assert_eq!(mapper.read_chr(0x0010), 0xAA);
        mapper.write_chr(0x0010, 0x5C);
        assert_eq!(
            mapper.read_chr(0x0010),
            0x5C,
            "submapper 0 compatibility should keep CHR writable"
        );
    }

    #[test]
    fn submapper0_non_compat_crc_with_chr_rom_stays_read_only() {
        // Mesen2 parity path: regular Mapper 51 behavior treats CHR-ROM as read-only.
        let mut mapper = make_mapper_with_submapper(0, vec![0xAA; 8 * 1024], 0x1234_5678);
        assert_eq!(mapper.read_chr(0x0010), 0xAA);
        mapper.write_chr(0x0010, 0x5C);
        assert_eq!(
            mapper.read_chr(0x0010),
            0xAA,
            "non-compat CRC should follow normal read-only CHR-ROM behavior"
        );
    }

    // --- Reset ---

    #[test]
    fn reset_restores_default_state() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x8000, 5);
        mapper.write_prg(0x6000, 0x00);
        mapper.reset();
        // After reset: bank=0, mode=1 → same as power-on
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "After reset, $8000 should read bank 0"
        );
        assert_eq!(
            mapper.read_prg(0x6000),
            35,
            "After reset, $6000 should read bank 35"
        );
    }

    // --- Mesen2-spec: $C000-$DFFF write behavior ---

    #[test]
    fn c000_write_updates_bank_register() {
        // $C000 write updates bank (same as $8000 write for bank)
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 1); // bank=1 from $C000 write
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "$C000 write with value 1 must set bank=1"
        );
    }

    #[test]
    fn c000_write_updates_mode_bit1_from_value_bit4() {
        // $C000 write: bank = value & 0x0F; mode = ((value>>3)&0x02) | (mode & 0x01)
        // Write 0x12 to $C000: bank=2, mode bit1 set from value bit4 (0x12 has bit4=1)
        // With default mode=1 (bit0=1): new mode = ((0x12>>3)&0x02) | (1&0x01) = 2|1 = 3 → Horizontal
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0xC000, 0x12); // value bit4=1 → mode bit1 set
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "$C000 write with bit4 set must update mode bit1 → Horizontal"
        );
    }

    #[test]
    fn c000_write_preserves_mode_bit0() {
        // $C000 write preserves existing mode bit0
        // Switch to 16KB mode first (bit0=0): write 0x00 to $6000 → mode bit0=0
        // Then write 0x10 to $C000: mode bit1 from value bit4, mode bit0 stays 0
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x6000, 0x00); // mode = 0 (bit0=0 → 16KB)
        mapper.write_prg(0xC000, 0x10); // value bit4=1 → mode bit1 set, bit0 unchanged (stays 0)
        // mode = 2, 16KB mode; Horizontal requires mode==3, so still Vertical
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "$C000 write with only bit4 set must not produce Horizontal (needs both bits)"
        );
    }

    #[test]
    fn mode_bit4_alone_without_mode_bit0_is_not_horizontal() {
        // Write 0x10 to $6000: mode = ((0x10>>3)&0x02)|((0x10>>1)&0x01) = 2|0 = 2
        // mode != 3 → Vertical (not Horizontal, even though bit4 of value is set)
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x6000, 0x10);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Bit 4 alone in mode write must not produce Horizontal mirroring"
        );
    }

    #[test]
    fn bank_register_is_masked_to_4_bits() {
        // Write 0x1F to $8000: bank = 0x1F & 0x0F = 15 (not 31)
        // With 48 banks: 15*4=60, 60%48=12
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x1F);
        assert_eq!(mapper.read_prg(0x8000), 12, "Bank must be masked to 4 bits");
    }

    #[test]
    fn mode_write_bit4_selects_8000_offset_in_16kb_mode() {
        // Write 0x10 to $6000: mode=2 (16KB, bit0=0). bank stays 0.
        // $8000 = (bank<<2)|mode = (0<<2)|2 = 2. $A000 = 3.
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x10); // mode=2 in Mesen2
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "16KB mode with mode=2: $8000 must be page 2"
        );
        assert_eq!(
            mapper.read_prg(0xA000),
            3,
            "16KB mode with mode=2: $A000 must be page 3"
        );
    }

    // --- Snapshot ---

    #[test]
    fn registers_snapshot_and_restore() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x8000, 2); // bank=2
        mapper.write_prg(0x6000, 0x00); // mode=0 (16KB)
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper_direct();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "Restored PRG bank must match"
        );
        assert_eq!(
            restored.read_prg(0x6000),
            mapper.read_prg(0x6000),
            "Restored $6000 bank must match"
        );
        assert_eq!(
            restored.get_mirroring(),
            mapper.get_mirroring(),
            "Restored mirroring must match"
        );
    }
}
