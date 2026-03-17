//! Mapper 029 - Sealie Computing board
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_029>
//!
//! Register format (`$8000-$FFFF` writes): `[..PPP PCC]`
//! - `PPP` (bits 4-2): 16KB PRG bank at `$8000-$BFFF`
//! - fixed last 16KB bank at `$C000-$FFFF`
//! - `CC` (bits 1-0): 8KB CHR-RAM bank at `$0000-$1FFF`
//! - WRAM: 8KB at `$6000-$7FFF`
//! - Mirroring: fixed from iNES header

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::common::ChrMemory;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const CHR_RAM_SIZE: usize = 32 * 1024;

pub struct Mapper29 {
    base: BaseMapper,
    register: u8,
}

impl Mapper29 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        if ctx.chr_rom.is_empty() {
            base.set_chr_memory(ChrMemory::new_ram(CHR_RAM_SIZE));
        }
        base.configure_prg_banking(16 * 1024);
        base.configure_chr_banking(8 * 1024);
        base.select_prg_page(0, 0);
        base.select_prg_page(1, -1);
        base.select_chr_page(0, 0);

        Self { base, register: 0 }
    }

    fn apply_register(&mut self, value: u8) {
        self.register = value;
        self.base.select_chr_page(0, ((value >> 3) & 0b11) as i16); // bits 4:3
        self.base.select_prg_page(0, (value & 0b111) as i16); // bits 2:0
        self.base.select_prg_page(1, -1);
    }
}

impl Mapper for Mapper29 {
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
        if (0x8000..=0xFFFF).contains(&addr) {
            self.apply_register(value);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.register]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&value) = data.first() {
            self.apply_register(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_mapper29() -> Box<dyn Mapper> {
        let prg_rom = banked_data(16 * 1024, 8);
        create_mapper(MapperContext::new_for_test(
            29,
            prg_rom,
            Vec::new(),
            NametableLayout::Vertical,
        ))
        .expect("mapper 29 should be implemented")
    }

    #[test]
    fn mapper_29_is_registered() {
        let mapper = create_mapper(MapperContext::new_for_test(
            29,
            banked_data(16 * 1024, 8),
            Vec::new(),
            NametableLayout::Vertical,
        ));
        assert!(mapper.is_ok(), "mapper 29 must be available in factory");
    }

    #[test]
    fn write_register_selects_switchable_prg_and_chr_ram_bank() {
        let mut mapper = create_mapper29();

        mapper.write_prg(0x8000, 0b0000_1010); // PRG=2, CHR=1

        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xC000), 7);
        assert_eq!(mapper.read_chr(0x0000), 0);
    }

    #[test]
    fn chr_ram_is_32kb_and_bank_switchable() {
        let mut mapper = create_mapper29();

        assert_eq!(mapper.chr_ram_snapshot().len(), 32 * 1024);

        mapper.write_prg(0x8000, 0b0001_1000); // CHR=3
        mapper.write_chr(0x0100, 0xA5);

        mapper.write_prg(0x8000, 0b0000_0000); // CHR=0
        assert_eq!(mapper.read_chr(0x0100), 0x00);

        mapper.write_prg(0x8000, 0b0001_1000); // CHR=3
        assert_eq!(mapper.read_chr(0x0100), 0xA5);
    }

    #[test]
    fn prg_ram_at_6000_roundtrips() {
        let mut mapper = create_mapper29();

        mapper.write_prg(0x6000, 0x5A);
        mapper.write_prg(0x7FFF, 0xA5);

        assert_eq!(mapper.read_prg(0x6000), 0x5A);
        assert_eq!(mapper.read_prg(0x7FFF), 0xA5);
        assert_eq!(mapper.wram_size(), 8 * 1024);
    }

    #[test]
    fn mirroring_remains_header_controlled() {
        let mut mapper = create_mapper29();

        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0x8000, 0b0001_1111);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn registers_snapshot_restore_roundtrip() {
        let mut mapper = create_mapper29();
        mapper.write_prg(0x8000, 0b0001_1010); // PRG=6, CHR=2

        let snapshot = mapper.registers_snapshot();

        let mut restored = create_mapper29();
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.read_prg(0xC000), mapper.read_prg(0xC000));
        assert_eq!(restored.read_chr(0x0000), mapper.read_chr(0x0000));
    }
}
