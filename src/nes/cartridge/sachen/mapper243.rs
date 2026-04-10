//! Mapper 243 - Sachen 74LS374N (SA-020A)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_243>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 243 - Sachen 74LS374N (SA-020A)
///
/// Hardware: Sachen latch-based register select/data write interface.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_243>
/// - PRG-ROM: Up to 64KB (32KB banks, selected by bit 0)
/// - CHR-ROM: Up to 64KB (8KB banks, selected by 3 bits from registers)
/// - Mirroring: Software-controlled (Horizontal/Vertical)
///
/// Address decoding:
/// - Write to $4100 (A0=0): Set command register (bits 0-2)
/// - Write to $4101 (A0=1): Write data to selected register
///
/// Register effects (when data written to $4101):
/// - Reg 2: bit 1 → mirroring flag, bit 3 → CHR bit 0
/// - Reg 4: bit 1 → CHR bit 1
/// - Reg 5: bit 0 → PRG bank (32KB)
/// - Reg 6: bit 1 → CHR bit 2
/// - Reg 7: Triggers mirroring update from stored flag
///
/// CHR bank = chr_bit0 | (chr_bit1 << 1) | (chr_bit2 << 2)
/// Mirroring: flag 0 = Vertical, flag 1 = Horizontal
pub struct Mapper243 {
    base: BaseMapper,
    command: u8,
    prg_bank: u8,
    chr_bits: [u8; 3], // chr_bit0, chr_bit1, chr_bit2
    mirror_flag: u8,
}

impl Mapper243 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(32 * 1024);
        base.configure_chr_banking(8 * 1024);
        Self {
            base,
            command: 0,
            prg_bank: 0,
            chr_bits: [0; 3],
            mirror_flag: 0,
        }
    }

    fn chr_bank(&self) -> usize {
        self.chr_bits[0] as usize
            | ((self.chr_bits[1] as usize) << 1)
            | ((self.chr_bits[2] as usize) << 2)
    }

    fn update_mirroring(&mut self) {
        self.base.set_mirroring_hv(self.mirror_flag != 0);
    }

    fn update_banks(&mut self) {
        self.base.select_prg_page(0, self.prg_bank as i16);
        self.base.select_chr_page(0, self.chr_bank() as i16);
    }
}

impl Mapper for Mapper243 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if addr & 0x4101 == 0x4100 {
            self.command = value & 0x07;
        } else if addr & 0x4101 == 0x4101 {
            match self.command {
                2 => {
                    self.mirror_flag = (value >> 1) & 1;
                    self.chr_bits[0] = (value >> 3) & 1;
                    self.update_mirroring();
                    self.update_banks();
                }
                4 => {
                    self.chr_bits[1] = (value >> 1) & 1;
                    self.update_banks();
                }
                5 => {
                    self.prg_bank = value & 1;
                    self.update_banks();
                }
                6 => {
                    self.chr_bits[2] = (value >> 1) & 1;
                    self.update_banks();
                }
                7 => {
                    self.update_mirroring();
                }
                _ => {}
            }
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.command,
            self.prg_bank,
            self.chr_bits[0],
            self.chr_bits[1],
            self.chr_bits[2],
            self.mirror_flag,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 6 {
            self.command = data[0];
            self.prg_bank = data[1];
            self.chr_bits[0] = data[2];
            self.chr_bits[1] = data[3];
            self.chr_bits[2] = data[4];
            self.mirror_flag = data[5];
            self.update_mirroring();
            self.update_banks();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    fn create_mapper243(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(
            243, prg_rom, chr_rom, mirroring,
        ))
    }

    #[test]
    fn test_factory_creates_mapper_243() {
        let prg_rom = banked_data(32 * 1024, 2);
        let chr_rom = banked_data(8 * 1024, 8);
        let mapper = create_mapper243(prg_rom, chr_rom, NametableLayout::Vertical);
        assert!(mapper.is_ok());
    }

    #[test]
    fn test_prg_bank_switching() {
        let prg_rom = banked_data(32 * 1024, 2);
        let chr_rom = banked_data(8 * 1024, 4);
        let mut mapper = create_mapper243(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Default bank 0
        assert_eq!(mapper.read_prg(0x8000), 0);

        // Select register 5 (PRG bank)
        mapper.write_prg(0x4100, 5);
        // Write value with bit 0 = 1 → PRG bank 1
        mapper.write_prg(0x4101, 1);
        assert_eq!(mapper.read_prg(0x8000), 1);

        // Write value with bit 0 = 0 → PRG bank 0
        mapper.write_prg(0x4101, 0);
        assert_eq!(mapper.read_prg(0x8000), 0);
    }

    #[test]
    fn test_chr_bank_switching_via_register_2() {
        let prg_rom = banked_data(32 * 1024, 1);
        let chr_rom = banked_data(8 * 1024, 8);
        let mut mapper = create_mapper243(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Default CHR bank should be 0
        assert_eq!(mapper.read_chr(0x0000), 0);

        // Register 2, bit 3 → CHR bit 0
        mapper.write_prg(0x4100, 2);
        mapper.write_prg(0x4101, 0x08); // bit 3 set → chr_bit0 = 1

        // CHR bank = 1
        assert_eq!(mapper.read_chr(0x0000), 1);
    }

    #[test]
    fn test_chr_bank_all_bits() {
        let prg_rom = banked_data(32 * 1024, 1);
        let chr_rom = banked_data(8 * 1024, 8);
        let mut mapper = create_mapper243(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Set CHR bit 0 via register 2
        mapper.write_prg(0x4100, 2);
        mapper.write_prg(0x4101, 0x08); // bit 3 → chr_bit0 = 1

        // Set CHR bit 1 via register 4
        mapper.write_prg(0x4100, 4);
        mapper.write_prg(0x4101, 0x02); // bit 1 → chr_bit1 = 1

        // Set CHR bit 2 via register 6
        mapper.write_prg(0x4100, 6);
        mapper.write_prg(0x4101, 0x02); // bit 1 → chr_bit2 = 1

        // CHR bank = 1 | (1<<1) | (1<<2) = 7
        assert_eq!(mapper.read_chr(0x0000), 7);
    }

    #[test]
    fn test_mirroring_via_register_2() {
        let prg_rom = banked_data(32 * 1024, 1);
        let chr_rom = banked_data(8 * 1024, 4);
        let mut mapper = create_mapper243(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // Register 2, bit 1 → mirror_flag
        mapper.write_prg(0x4100, 2);
        mapper.write_prg(0x4101, 0x02); // bit 1 set → mirror_flag = 1 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        // Clear mirror flag
        mapper.write_prg(0x4101, 0x00); // bit 1 clear → mirror_flag = 0 → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn test_mirroring_via_register_7() {
        let prg_rom = banked_data(32 * 1024, 1);
        let chr_rom = banked_data(8 * 1024, 4);
        let mut mapper = create_mapper243(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Set mirror flag via register 2
        mapper.write_prg(0x4100, 2);
        mapper.write_prg(0x4101, 0x02); // mirror_flag = 1 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        // Reset mirror flag but don't trigger register 7 yet
        mapper.write_prg(0x4101, 0x00); // mirror_flag = 0 → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // Set mirror flag again
        mapper.write_prg(0x4101, 0x02);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        // Register 7 also applies mirroring from stored flag
        mapper.write_prg(0x4100, 7);
        mapper.write_prg(0x4101, 0x00); // value doesn't matter, uses stored flag
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_command_register_only_uses_low_3_bits() {
        let prg_rom = banked_data(32 * 1024, 2);
        let chr_rom = banked_data(8 * 1024, 4);
        let mut mapper = create_mapper243(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Write 0xFF to command register - only bits 0-2 should be used (= 7)
        mapper.write_prg(0x4100, 0xFF);
        // This is register 7 (mirroring), write should not crash
        mapper.write_prg(0x4101, 0x00);
    }

    #[test]
    fn test_registers_snapshot_and_restore() {
        let prg_rom = banked_data(32 * 1024, 2);
        let chr_rom = banked_data(8 * 1024, 8);
        let mut mapper =
            create_mapper243(prg_rom.clone(), chr_rom.clone(), NametableLayout::Vertical).unwrap();

        // Set up registers
        mapper.write_prg(0x4100, 5);
        mapper.write_prg(0x4101, 1); // PRG bank 1
        mapper.write_prg(0x4100, 2);
        mapper.write_prg(0x4101, 0x0A); // mirror_flag=1, chr_bit0=1
        mapper.write_prg(0x4100, 4);
        mapper.write_prg(0x4101, 0x02); // chr_bit1=1
        mapper.write_prg(0x4100, 6);
        mapper.write_prg(0x4101, 0x02); // chr_bit2=1

        let regs = mapper.registers_snapshot();

        let mut restored = create_mapper243(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();
        restored.restore_registers(&regs);

        // Should have CHR bank 7 (all bits set)
        assert_eq!(restored.read_chr(0x0000), 7);
        assert_eq!(restored.read_prg(0x8000), 1);
        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
    }
}
