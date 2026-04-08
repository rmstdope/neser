//! Mapper 238 – MMC3 variant with security register at $4020–$7FFF
//!
//! # Specifications
//! - Primary source: NesDev Wiki `INES_Mapper_238`
//!   (mirror: <https://nesdev-wiki.nes.science/wikipages/INES_Mapper_238.xhtml>)
//! - Supplemental: Mesen2 `Core/NES/Mappers/Mmc3Variants/MMC3_238.h`
//!
//! ## Hardware overview
//! Used by *Contra Fighter* (a hack of *G.I. Joe* with Street Fighter II characters).
//! The board is an MMC3 variant that adds a security register accessible in the
//! `$4020–$7FFF` address range.
//!
//! ## Extra register – `$4020–$7FFF`
//!
//! **Reads** from `$4020–$7FFF` return the current value of the security register.
//!
//! **Writes** to `$4020–$7FFF` update the security register via a lookup table:
//!
//! ```text
//! value & 0x03 │  0x00  0x01  0x02  0x03
//! exReg        │  0x00  0x02  0x02  0x03
//! ```
//!
//! Power-on value: `$00`.
//!
//! ## MMC3-compatible registers (`$8000–$FFFF`)
//! All standard MMC3 register writes are forwarded to the MMC3 core unchanged.
//! Register reads from `$8000–$FFFF` are not enabled (write-only range).

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mmc3::MMC3Mapper;
use crate::nes::cartridge::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 238;

/// Security LUT: index = `value & 0x03`, output = `exReg` value.
const SECURITY_LUT: [u8; 4] = [0x00, 0x02, 0x02, 0x03];

/// Mapper 238 – MMC3 variant with security register.
///
/// An MMC3-based board used by *Contra Fighter*. A security register lives at
/// `$4020–$7FFF` and returns a LUT-mapped value on reads and writes.
pub struct Mapper238 {
    pub(crate) mmc3: MMC3Mapper,
    ex_reg: u8,
}

impl Mapper238 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
            ex_reg: 0,
        }
    }
}

impl Mapper for Mapper238 {
    fn base(&self) -> &BaseMapper {
        &self.mmc3.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.mmc3.base
    }

    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        Some(&self.mmc3)
    }

    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        Some(&mut self.mmc3)
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x4020..=0x7FFF => self.ex_reg,
            _ => self.mmc3.read_prg(addr),
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x4020..=0x7FFF => self.ex_reg,
            _ => self.mmc3.read_prg_open_bus(addr, open_bus),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x4020..=0x7FFF => {
                self.ex_reg = SECURITY_LUT[(value & 0x03) as usize];
            }
            0x8000..=0xFFFF => {
                self.mmc3.write_prg(addr, value);
            }
            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.mmc3.read_chr(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.mmc3.write_chr(addr, value);
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.ex_reg);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 17 {
            self.mmc3.restore_registers(&data[..16]);
            self.ex_reg = data[16];
        } else {
            self.mmc3.restore_registers(data);
            self.ex_reg = 0;
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    fn create_mapper238(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(
            238, prg_rom, chr_rom, mirroring,
        ))
    }

    #[test]
    fn test_factory_creates_mapper_238() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mapper = create_mapper238(prg_rom, chr_rom, NametableLayout::Vertical);
        assert!(mapper.is_ok(), "Mapper 238 should be creatable via factory");
    }

    #[test]
    fn test_ex_reg_initial_value_is_zero() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mapper = create_mapper238(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();
        // Power-on: exReg = 0x00
        assert_eq!(mapper.read_prg(0x4020), 0x00);
        assert_eq!(mapper.read_prg(0x7FFF), 0x00);
    }

    #[test]
    fn test_security_lut_write_and_read() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper238(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // LUT: 0->0x00, 1->0x02, 2->0x02, 3->0x03
        mapper.write_prg(0x6000, 0x00);
        assert_eq!(mapper.read_prg(0x6000), 0x00);

        mapper.write_prg(0x6000, 0x01);
        assert_eq!(mapper.read_prg(0x6000), 0x02);

        mapper.write_prg(0x6000, 0x02);
        assert_eq!(mapper.read_prg(0x6000), 0x02);

        mapper.write_prg(0x6000, 0x03);
        assert_eq!(mapper.read_prg(0x6000), 0x03);
    }

    #[test]
    fn test_security_lut_uses_lower_two_bits_only() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper238(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Upper bits should be masked out
        mapper.write_prg(0x6000, 0xFF); // 0xFF & 0x03 = 3 → 0x03
        assert_eq!(mapper.read_prg(0x6000), 0x03);

        mapper.write_prg(0x6000, 0xFC); // 0xFC & 0x03 = 0 → 0x00
        assert_eq!(mapper.read_prg(0x6000), 0x00);

        mapper.write_prg(0x6000, 0xFD); // 0xFD & 0x03 = 1 → 0x02
        assert_eq!(mapper.read_prg(0x6000), 0x02);
    }

    #[test]
    fn test_security_register_mirrors_across_4020_7fff() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper238(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        mapper.write_prg(0x4020, 0x03); // exReg = 0x03
        assert_eq!(mapper.read_prg(0x4020), 0x03);
        assert_eq!(mapper.read_prg(0x5FFF), 0x03);
        assert_eq!(mapper.read_prg(0x6000), 0x03);
        assert_eq!(mapper.read_prg(0x7FFF), 0x03);
    }

    #[test]
    fn test_mmc3_prg_banking_works_normally() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper238(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Standard MMC3: select register R6, set to bank 3
        mapper.write_prg(0x8000, 6); // bank select: R6
        mapper.write_prg(0x8001, 3); // R6 = 3
        assert_eq!(mapper.read_prg(0x8000), 3);
    }

    #[test]
    fn test_mmc3_chr_banking_works_normally() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper238(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Standard MMC3: select R0, set to bank 2 (2KB CHR, selects banks 2 and 3)
        mapper.write_prg(0x8000, 0); // bank select: R0
        mapper.write_prg(0x8001, 2); // R0 = 2
        assert_eq!(mapper.read_chr(0x0000), 2);
        assert_eq!(mapper.read_chr(0x03FF), 2);
    }

    #[test]
    fn test_security_write_does_not_affect_mmc3_registers() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper238(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Set a PRG bank via MMC3
        mapper.write_prg(0x8000, 6);
        mapper.write_prg(0x8001, 5);

        // Write to security register should not change PRG bank
        mapper.write_prg(0x6000, 0x03);
        assert_eq!(mapper.read_prg(0x8000), 5);
    }

    #[test]
    fn test_mapper_number() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mapper = create_mapper238(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();
        assert_eq!(mapper.mapper_number(), 238);
    }

    #[test]
    fn test_registers_snapshot_and_restore() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper =
            create_mapper238(prg_rom.clone(), chr_rom.clone(), NametableLayout::Vertical).unwrap();

        // Set MMC3 state and security register
        mapper.write_prg(0x8000, 6);
        mapper.write_prg(0x8001, 3); // R6=3
        mapper.write_prg(0x6000, 0x03); // exReg = 0x03

        let snap = mapper.registers_snapshot();

        let mut restored = create_mapper238(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();
        restored.restore_registers(&snap);

        // MMC3 PRG state restored
        assert_eq!(restored.read_prg(0x8000), 3);
        // Security register restored
        assert_eq!(restored.read_prg(0x6000), 0x03);
    }

    #[test]
    fn test_registers_snapshot_legacy_restore() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper =
            create_mapper238(prg_rom.clone(), chr_rom.clone(), NametableLayout::Vertical).unwrap();

        mapper.write_prg(0x8000, 6);
        mapper.write_prg(0x8001, 5); // R6=5

        // Grab only the MMC3 portion (16 bytes, no exReg)
        let snap = mapper.registers_snapshot();
        let legacy = snap[..16].to_vec();

        let mut restored = create_mapper238(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();
        // Set exReg to a non-zero value before restoring to confirm it gets reset
        restored.write_prg(0x6000, 0x03);
        assert_eq!(restored.read_prg(0x6000), 0x03);

        restored.restore_registers(&legacy);

        // MMC3 banking is restored
        assert_eq!(restored.read_prg(0x8000), 5);
        // exReg must be reset to 0 on legacy restore (not retain previous value)
        assert_eq!(restored.read_prg(0x6000), 0x00);
    }
}
