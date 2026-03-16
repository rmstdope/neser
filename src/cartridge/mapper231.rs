//! Mapper 231 - 20-in-1 multicart
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_231>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 231;

/// Mapper 231 - 20-in-1 multicart
///
/// Hardware: Dual 16KB PRG bank switching controlled by write address bits.
/// CHR uses 8KB CHR-RAM.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_231>
/// - PRG-ROM: Up to 512KB (32 x 16KB banks)
/// - CHR: 8KB CHR-RAM
/// - Mirroring: Switchable (controlled by write address bit 7)
///
/// Register: Any write to $8000-$FFFF latches bits from the write ADDRESS:
///   A~[.... .... M.LP PPP.]
///   - A7 = M: Mirroring (0=Vertical, 1=Horizontal)
///   - A5 = L: Low bit of PRG bank
///   - A4..A1 = P: High bits of PRG bank
///
/// PRG Banking:
///   - $8000-$BFFF: 16KB bank = write_addr & 0x1E  (L forced to 0)
///   - $C000-$FFFF: 16KB bank = (write_addr & 0x1E) | ((write_addr >> 5) & 1)
pub struct Mapper231 {
    base: BaseMapper,
    /// Last register write address used to compute banking and mirroring
    reg_addr: u16,
}

impl Mapper231 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        let mut mapper = Self { base, reg_addr: 0 };
        mapper.apply_banks();
        mapper
    }

    fn apply_banks(&mut self) {
        let lo = (self.reg_addr & 0x1E) as i16;
        let hi = lo | (((self.reg_addr >> 5) & 1) as i16);
        self.base.select_prg_page(0, lo);
        self.base.select_prg_page(1, hi);
        let horizontal = (self.reg_addr >> 7) & 1 == 1;
        self.base.set_mirroring_hv(horizontal);
    }
}

impl Mapper for Mapper231 {
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
        let _ = value;
        if (0x8000..=0xFFFF).contains(&addr) {
            self.reg_addr = addr;
            self.apply_banks();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.base.banking_snapshot();
        snap.push((self.reg_addr & 0xFF) as u8);
        snap.push(((self.reg_addr >> 8) & 0xFF) as u8);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        // Determine the expected size of the base banking snapshot so we can
        // safely distinguish it from the trailing reg_addr bytes.
        let expected_banking_len = self.base.banking_snapshot().len();

        if data.len() >= expected_banking_len + 2 {
            // Full snapshot: banking state plus 2-byte reg_addr.
            self.base.restore_banking(&data[..expected_banking_len]);
            self.reg_addr =
                (data[expected_banking_len] as u16) | ((data[expected_banking_len + 1] as u16) << 8);
            self.apply_banks();
        } else {
            // Legacy/truncated/corrupt snapshot: treat all bytes as banking data.
            // Do not attempt to parse reg_addr or apply banks from potentially
            // misaligned data.
            self.base.restore_banking(data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 32;

    fn create_mapper231(prg_rom: Vec<u8>) -> Mapper231 {
        Mapper231::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            prg_rom,
            vec![],
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_231_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(16 * 1024, PRG_BANKS),
            vec![],
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 231 should be registered in factory");
    }

    #[test]
    fn power_on_both_windows_start_at_bank_0() {
        let mapper = create_mapper231(banked_data(16 * 1024, PRG_BANKS));
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 window should start at bank 0"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 window should start at bank 0"
        );
    }

    /// Writing to an address with P=0, L=0 (addr=0x8000):
    /// - First window:  0x8000 & 0x1E = 0 → bank 0
    /// - Second window: 0 | ((0x8000 >> 5) & 1) = 0 | 0 = 0 → bank 0
    #[test]
    fn write_8000_both_windows_select_bank_0() {
        let mut mapper = create_mapper231(banked_data(16 * 1024, PRG_BANKS));
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 0);
    }

    /// Writing to P=1, L=0 (addr=0x8002 → A1=1, others 0):
    /// - First window:  0x8002 & 0x1E = 0x02 → bank 2
    /// - Second window: 0x02 | ((0x8002 >> 5) & 1) = 0x02 | 0 = 0x02 → bank 2
    #[test]
    fn write_8002_both_windows_select_bank_2() {
        let mut mapper = create_mapper231(banked_data(16 * 1024, PRG_BANKS));
        mapper.write_prg(0x8002, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xC000), 2);
    }

    /// Writing to P=1, L=1 (addr=0x8022 → A5=1, A1=1):
    /// - First window:  0x8022 & 0x1E = 0x02 → bank 2 (L forced to 0)
    /// - Second window: 0x02 | ((0x8022 >> 5) & 1) = 0x02 | 1 = 0x03 → bank 3
    #[test]
    fn write_8022_first_window_bank_2_second_window_bank_3() {
        let mut mapper = create_mapper231(banked_data(16 * 1024, PRG_BANKS));
        mapper.write_prg(0x8022, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "First window should be bank 2 (L=0)"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "Second window should be bank 3 (L=1)"
        );
    }

    /// Writing to P=0, L=1 (addr=0x8020 → A5=1):
    /// - First window:  0x8020 & 0x1E = 0x00 → bank 0
    /// - Second window: 0x00 | ((0x8020 >> 5) & 1) = 0x00 | 1 = 0x01 → bank 1
    #[test]
    fn write_8020_first_window_bank_0_second_window_bank_1() {
        let mut mapper = create_mapper231(banked_data(16 * 1024, PRG_BANKS));
        mapper.write_prg(0x8020, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 0, "First window bank 0 (L=0)");
        assert_eq!(mapper.read_prg(0xC000), 1, "Second window bank 1 (L=1)");
    }

    /// Writing to P=15, L=1 (addr = 0x8000 | (15<<1) | (1<<5) = 0x8000 | 0x1E | 0x20 = 0x803E):
    /// - First window:  0x803E & 0x1E = 0x1E → bank 30
    /// - Second window: 0x1E | ((0x803E >> 5) & 1) = 0x1E | 1 = 0x1F → bank 31
    #[test]
    fn write_max_bank_both_windows_at_top_of_rom() {
        let mut mapper = create_mapper231(banked_data(16 * 1024, PRG_BANKS));
        mapper.write_prg(0x803E, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            30,
            "First window should be bank 30"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            31,
            "Second window should be bank 31"
        );
    }

    /// Mirroring bit A7: 0 = Vertical, 1 = Horizontal
    #[test]
    fn mirroring_bit_a7_zero_selects_vertical() {
        let mut mapper = create_mapper231(banked_data(16 * 1024, PRG_BANKS));
        mapper.write_prg(0x8000, 0x00); // A7=0
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_bit_a7_one_selects_horizontal() {
        let mut mapper = create_mapper231(banked_data(16 * 1024, PRG_BANKS));
        mapper.write_prg(0x8080, 0x00); // A7=1
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_can_be_toggled_via_writes() {
        let mut mapper = create_mapper231(banked_data(16 * 1024, PRG_BANKS));
        mapper.write_prg(0x8080, 0x00); // horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        mapper.write_prg(0x8000, 0x00); // vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn chr_ram_is_readable_and_writable() {
        let mut mapper = create_mapper231(banked_data(16 * 1024, PRG_BANKS));
        mapper.write_chr(0x0000, 0xAB);
        mapper.write_chr(0x1FFF, 0xCD);
        assert_eq!(mapper.read_chr(0x0000), 0xAB);
        assert_eq!(mapper.read_chr(0x1FFF), 0xCD);
    }

    /// Write data value should be ignored; only the address matters
    #[test]
    fn write_data_value_is_ignored() {
        let mut mapper = create_mapper231(banked_data(16 * 1024, PRG_BANKS));
        // Write different data values to same address - bank should not change
        mapper.write_prg(0x8002, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 2);
        mapper.write_prg(0x8002, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 2);
    }

    #[test]
    fn registers_snapshot_and_restore() {
        let prg_rom = banked_data(16 * 1024, PRG_BANKS);
        let mut mapper = create_mapper231(prg_rom.clone());
        mapper.write_prg(0x8022, 0x00); // P=1, L=1

        let snap = mapper.registers_snapshot();

        let mut restored = create_mapper231(prg_rom);
        restored.restore_registers(&snap);
        assert_eq!(restored.read_prg(0x8000), 2);
        assert_eq!(restored.read_prg(0xC000), 3);
    }
}
