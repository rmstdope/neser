//! Mapper 102 - Deprecated NROM alias
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_102>
//!
//! Known Limitations:
//! - Mapper 102 is treated as a deprecated alias of mapper 0 (NROM).

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 102 - Deprecated NROM alias.
///
/// Hardware behavior is equivalent to NROM:
/// - PRG-ROM fixed 16KB/32KB at $8000-$FFFF (16KB mirrored when present)
/// - CHR fixed 8KB (ROM or RAM)
/// - Mirroring fixed from header
/// - No mapper registers, IRQ, or expansion audio
pub struct Mapper102 {
    base: BaseMapper,
}

impl Mapper102 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_irq: false,
            has_chr_banking: false,
            has_dynamic_mirroring: false,
            has_expansion_audio: false,
            // Like mapper 0, PRG-RAM availability/size is header-defined.
            max_prg_ram_kb: if ctx.prg_ram_size_specified && ctx.prg_ram_banks_8k > 0 {
                ctx.prg_ram_banks_8k as usize * 8
            } else {
                0
            },
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };

        Self {
            base: BaseMapper::new(&ctx, capabilities),
        }
    }
}

impl Mapper for Mapper102 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // Only PRG-RAM writes at $6000-$7FFF are meaningful.
        // Mapper 102 has no bank-switch registers.
        self.base.try_write_prg_ram(addr, value);
    }
}

#[cfg(test)]
mod tests {
    use super::Mapper102;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};

    #[test]
    fn mapper_102_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            102,
            vec![0; 32 * 1024],
            vec![0; 8 * 1024],
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 102 must be creatable via factory");
    }

    #[test]
    fn prg_is_fixed_and_16kb_is_mirrored() {
        let mut prg_rom = vec![0; 16 * 1024];
        prg_rom[0x0000] = 0x12;
        prg_rom[0x3FFF] = 0x34;

        let mapper = Mapper102::new(MapperContext::new_for_test(
            102,
            prg_rom,
            vec![0; 8 * 1024],
            NametableLayout::Vertical,
        ));

        assert_eq!(mapper.read_prg(0x8000), 0x12);
        assert_eq!(mapper.read_prg(0xBFFF), 0x34);
        assert_eq!(mapper.read_prg(0xC000), 0x12);
        assert_eq!(mapper.read_prg(0xFFFF), 0x34);
    }

    #[test]
    fn prg_register_writes_are_ignored() {
        let mut mapper = Mapper102::new(MapperContext::new_for_test(
            102,
            vec![0xA5; 32 * 1024],
            vec![0x5A; 8 * 1024],
            NametableLayout::Horizontal,
        ));

        mapper.write_prg(0x8000, 0xFF);
        mapper.write_prg(0xE000, 0x00);

        assert_eq!(mapper.read_prg(0x8000), 0xA5);
        assert_eq!(mapper.read_chr(0x0000), 0x5A);
    }

    #[test]
    fn mirroring_is_fixed_from_header() {
        let mapper_h = Mapper102::new(MapperContext::new_for_test(
            102,
            vec![0; 32 * 1024],
            vec![0; 8 * 1024],
            NametableLayout::Horizontal,
        ));
        assert_eq!(mapper_h.get_mirroring(), NametableLayout::Horizontal);

        let mapper_v = Mapper102::new(MapperContext::new_for_test(
            102,
            vec![0; 32 * 1024],
            vec![0; 8 * 1024],
            NametableLayout::Vertical,
        ));
        assert_eq!(mapper_v.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn capabilities_match_nrom_class_hardware() {
        let mapper = Mapper102::new(MapperContext::new_for_test(
            102,
            vec![0; 32 * 1024],
            vec![0; 8 * 1024],
            NametableLayout::Horizontal,
        ));

        let caps = mapper.capabilities();
        assert!(!caps.has_irq);
        assert!(!caps.has_chr_banking);
        assert!(!caps.has_dynamic_mirroring);
        assert!(!caps.has_expansion_audio);
    }
}
