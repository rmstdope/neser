//! Mapper 244 - Decathlon (C-N22M)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_244>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 244 - Decathlon (C-N22M)
///
/// Hardware: Simple PRG+CHR bank switching with lookup tables.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_244>
/// - PRG-ROM: Up to 128KB (4 x 32KB banks)
/// - CHR-ROM: Up to 64KB (8 x 8KB banks)
/// - Mirroring: Fixed from header
///
/// Write to $8000-$FFFF:
/// - If bit 3 == 0: bits 0-1 select PRG bank (through lookup table)
/// - If bit 3 == 1: bits 0-2 select CHR bank (through lookup table)
///
/// The lookup tables implement a simple copy-protection scramble:
/// - PRG: Identity mapping [0, 1, 2, 3]
/// - CHR: Swapped pairs [0, 2, 1, 3, 4, 6, 5, 7]
pub struct Mapper244 {
    base: BaseMapper,
}

impl Mapper244 {
    /// PRG bank lookup table (identity)
    const PRG_LUT: [u8; 4] = [0, 1, 2, 3];
    /// CHR bank lookup table (pairs 1/2 and 5/6 swapped)
    const CHR_LUT: [u8; 8] = [0, 2, 1, 3, 4, 6, 5, 7];

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(32 * 1024);
        base.configure_chr_banking(8 * 1024);
        Self { base }
    }
}

impl Mapper for Mapper244 {
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
            if value & 0x08 == 0 {
                // PRG bank select via LUT
                let index = (value & 0x03) as usize;
                self.base.select_prg_page(0, Self::PRG_LUT[index] as i16);
            } else {
                // CHR bank select via LUT
                let index = (value & 0x07) as usize;
                self.base.select_chr_page(0, Self::CHR_LUT[index] as i16);
            }
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.base.prg_page(0) as u8, self.base.chr_page(0) as u8]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&value) = data.first() {
            self.base.select_prg_page(0, value as i16);
        }
        if let Some(&value) = data.get(1) {
            self.base.select_chr_page(0, value as i16);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_mapper244(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(
            244, prg_rom, chr_rom, mirroring,
        ))
    }

    #[test]
    fn test_factory_creates_mapper_244() {
        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 8);
        let mapper = create_mapper244(prg_rom, chr_rom, NametableLayout::Vertical);
        assert!(mapper.is_ok(), "Mapper 244 should be creatable via factory");
    }

    #[test]
    fn test_prg_bank_select_via_lut() {
        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 8);
        let mut mapper = create_mapper244(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // PRG LUT is identity: [0, 1, 2, 3]
        // value bit 3 == 0, so bits 0-1 select PRG bank via LUT
        mapper.write_prg(0x8000, 0); // index 0 -> bank 0
        assert_eq!(mapper.read_prg(0x8000), 0);

        mapper.write_prg(0x8000, 1); // index 1 -> bank 1
        assert_eq!(mapper.read_prg(0x8000), 1);

        mapper.write_prg(0x8000, 2); // index 2 -> bank 2
        assert_eq!(mapper.read_prg(0x8000), 2);

        mapper.write_prg(0x8000, 3); // index 3 -> bank 3
        assert_eq!(mapper.read_prg(0x8000), 3);
    }

    #[test]
    fn test_chr_bank_select_via_scrambled_lut() {
        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 8);
        let mut mapper = create_mapper244(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // CHR LUT: [0, 2, 1, 3, 4, 6, 5, 7]
        // value bit 3 == 1, so bits 0-2 select CHR bank via LUT
        mapper.write_prg(0x8000, 0x08); // index 0 -> bank 0
        assert_eq!(mapper.read_chr(0x0000), 0);

        mapper.write_prg(0x8000, 0x09); // index 1 -> bank 2 (scrambled!)
        assert_eq!(mapper.read_chr(0x0000), 2);

        mapper.write_prg(0x8000, 0x0A); // index 2 -> bank 1 (scrambled!)
        assert_eq!(mapper.read_chr(0x0000), 1);

        mapper.write_prg(0x8000, 0x0B); // index 3 -> bank 3
        assert_eq!(mapper.read_chr(0x0000), 3);

        mapper.write_prg(0x8000, 0x0C); // index 4 -> bank 4
        assert_eq!(mapper.read_chr(0x0000), 4);

        mapper.write_prg(0x8000, 0x0D); // index 5 -> bank 6 (scrambled!)
        assert_eq!(mapper.read_chr(0x0000), 6);

        mapper.write_prg(0x8000, 0x0E); // index 6 -> bank 5 (scrambled!)
        assert_eq!(mapper.read_chr(0x0000), 5);

        mapper.write_prg(0x8000, 0x0F); // index 7 -> bank 7
        assert_eq!(mapper.read_chr(0x0000), 7);
    }

    #[test]
    fn test_mirroring_is_fixed() {
        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 8);
        let mut mapper = create_mapper244(prg_rom, chr_rom, NametableLayout::Horizontal).unwrap();

        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_prg_and_chr_banks_are_independent() {
        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 8);
        let mut mapper = create_mapper244(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Set PRG bank 2
        mapper.write_prg(0x8000, 2);
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_chr(0x0000), 0); // CHR unchanged

        // Set CHR bank (index 1 -> bank 2)
        mapper.write_prg(0x8000, 0x09);
        assert_eq!(mapper.read_prg(0x8000), 2); // PRG unchanged
        assert_eq!(mapper.read_chr(0x0000), 2);
    }

    #[test]
    fn test_registers_snapshot_and_restore() {
        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 8);
        let mut mapper =
            create_mapper244(prg_rom.clone(), chr_rom.clone(), NametableLayout::Vertical).unwrap();

        mapper.write_prg(0x8000, 2); // PRG bank 2
        mapper.write_prg(0x8000, 0x0D); // CHR bank 6 (LUT index 5)

        let regs = mapper.registers_snapshot();

        let mut restored = create_mapper244(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();
        restored.restore_registers(&regs);
        assert_eq!(restored.read_prg(0x8000), 2);
        assert_eq!(restored.read_chr(0x0000), 6);
    }
}
