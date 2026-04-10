//! Mapper 171 – Kaiser KS-7058
//!
//! Specifications:
//! - Mesen reference: `Core/NES/Mappers/Kaiser/Kaiser7058.h`
//!   <https://raw.githubusercontent.com/sourmesen/mesen2/master/Core/NES/Mappers/Kaiser/Kaiser7058.h>
//!
//! Hardware behavior:
//! - PRG-ROM: 32 KiB, fixed at bank 0.
//! - CHR-ROM: Two 4 KiB switchable banks.
//!   - Writes to `$F000` (where `addr & 0xF080 == 0xF000`) select CHR bank at PPU `$0000–$0FFF`.
//!   - Writes to `$F080` (where `addr & 0xF080 == 0xF080`) select CHR bank at PPU `$1000–$1FFF`.
//! - Mirroring: hardwired (no software control).
//! - No PRG-RAM, no IRQ.
//!
//! Known games: 梁山伯與祝英台 (Liang Shan Bo Yu Zhu Ying Tai)

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 171;
const PRG_BANK_SIZE: usize = 32 * 1024;
const CHR_BANK_SIZE: usize = 4 * 1024;

pub struct Mapper171 {
    base: BaseMapper,
    chr_banks: [u8; 2],
}

impl Mapper171 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            prg_bank_size_kb: PRG_BANK_SIZE / 1024,
            chr_bank_size_kb: CHR_BANK_SIZE / 1024,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);

        let mut mapper = Self {
            base,
            chr_banks: [0; 2],
        };
        mapper.apply_banks();
        mapper
    }

    fn apply_banks(&mut self) {
        self.base.select_prg_page(0, 0);
        self.base.select_chr_page(0, self.chr_banks[0] as i16);
        self.base.select_chr_page(1, self.chr_banks[1] as i16);
    }
}

impl Mapper for Mapper171 {
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
        match addr & 0xF080 {
            0xF000 => {
                self.chr_banks[0] = value;
                self.base.select_chr_page(0, value as i16);
            }
            0xF080 => {
                self.chr_banks[1] = value;
                self.base.select_chr_page(1, value as i16);
            }
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        self.chr_banks.to_vec()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.chr_banks[0] = data[0];
            self.chr_banks[1] = data[1];
            self.apply_banks();
        }
    }

    fn reset(&mut self) {
        self.chr_banks = [0; 2];
        self.apply_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const TEST_PRG_BANKS: usize = 1;
    const TEST_CHR_BANKS: usize = 7;

    fn make_mapper() -> Mapper171 {
        Mapper171::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, TEST_PRG_BANKS),
            banked_data(CHR_BANK_SIZE, TEST_CHR_BANKS),
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_171_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, TEST_PRG_BANKS),
            banked_data(CHR_BANK_SIZE, TEST_CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 171 must be creatable via factory");
    }

    #[test]
    fn prg_is_fixed_at_bank_0() {
        let mapper = make_mapper();
        // PRG bank 0: first byte == 0 (banked_data sets byte 0 = bank index mod 256)
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG must be fixed to bank 0");
    }

    #[test]
    fn power_on_chr_banks_are_zero() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank 0 must start at 0");
        assert_eq!(mapper.read_chr(0x1000), 0, "CHR bank 1 must start at 0");
    }

    #[test]
    fn write_f000_selects_chr_low_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF000, 3);
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "$F000 write must select CHR bank at PPU $0000"
        );
    }

    #[test]
    fn write_f080_selects_chr_high_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF080, 5);
        assert_eq!(
            mapper.read_chr(0x1000),
            5,
            "$F080 write must select CHR bank at PPU $1000"
        );
    }

    #[test]
    fn write_f000_does_not_affect_high_chr() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF080, 4);
        mapper.write_prg(0xF000, 2);
        assert_eq!(
            mapper.read_chr(0x1000),
            4,
            "CHR high bank must be unchanged by $F000 write"
        );
    }

    #[test]
    fn write_f080_does_not_affect_low_chr() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF000, 2);
        mapper.write_prg(0xF080, 4);
        assert_eq!(
            mapper.read_chr(0x0000),
            2,
            "CHR low bank must be unchanged by $F080 write"
        );
    }

    #[test]
    fn addr_mask_0xf080_only_matches_f000_and_f080() {
        let mut mapper = make_mapper();
        // $F001 & 0xF080 = 0xF000 → selects CHR low bank
        mapper.write_prg(0xF001, 6);
        assert_eq!(mapper.read_chr(0x0000), 6, "$F001 must alias to $F000");
        // $F040 & 0xF080 = 0xF000 → also selects CHR low bank
        mapper.write_prg(0xF040, 1);
        assert_eq!(mapper.read_chr(0x0000), 1, "$F040 must also alias to $F000");
        // $F081 & 0xF080 = 0xF080 → selects CHR high bank
        mapper.write_prg(0xF081, 4);
        assert_eq!(
            mapper.read_chr(0x1000),
            4,
            "$F081 must alias to $F080 (CHR high)"
        );
    }

    #[test]
    fn snapshot_restore_round_trips_chr_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF000, 2);
        mapper.write_prg(0xF080, 5);
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.read_chr(0x0000), 2, "restored CHR low must be 2");
        assert_eq!(restored.read_chr(0x1000), 5, "restored CHR high must be 5");
    }

    #[test]
    fn reset_restores_chr_to_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF000, 3);
        mapper.write_prg(0xF080, 6);
        mapper.reset();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "reset must set CHR low to bank 0"
        );
        assert_eq!(
            mapper.read_chr(0x1000),
            0,
            "reset must set CHR high to bank 0"
        );
    }
}
