//! Mapper 251 - SRAM-based register mapper
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_251>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::NametableLayout;
use crate::cartridge::common::{BankedRom, ChrMemory};
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 251 - SRAM-based register mapper
///
/// Hardware: Custom mapper with obfuscated CHR register writes via SRAM buffer.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_251>
/// - PRG-ROM: Up to 128KB (32KB banks, selected via register R0 bits 3:2)
/// - CHR-ROM: Up to 64KB (2KB banks, 4 slots)
/// - Mirroring: Fixed from header
///
/// Register interface:
/// - $8000 (even): Bank select (bits 0-2 select target register)
/// - $8001 (odd): Bank data for selected register
/// - $A001 (odd): SRAM mode (bit 7: enable SRAM writes; clear: disable + reset buffer)
///
/// When SRAM mode enabled, writes to $6000-$6003 fill a 4-byte buffer.
/// When $6003 is written, the buffer is decoded into 4 CHR bank registers:
/// - CHR banks are assembled from scrambled bits across buffer bytes
///
/// PRG bank = (reg[0] >> 2) & 3 (32KB)
/// CHR banks = decoded from buffer using cross-byte bit extraction
pub struct Mapper251 {
    prg_rom: BankedRom,
    chr_memory: ChrMemory,
    mirroring: NametableLayout,
    bank_select: u8,
    regs: [u8; 8],
    sram_mode: bool,
    buffer: [u8; 4],
    chr_banks: [u8; 4], // 4 × 2KB CHR bank registers
}

impl Mapper251 {
    const MAPPER_NUMBER: u8 = 251;
    const PRG_BANK_SIZE: usize = 32 * 1024;
    const CHR_BANK_SIZE: usize = 2 * 1024;

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: NametableLayout) -> Self {
        Self {
            prg_rom: BankedRom::new(prg_rom, Self::PRG_BANK_SIZE),
            chr_memory: ChrMemory::new(chr_rom),
            mirroring,
            bank_select: 0,
            regs: [0; 8],
            sram_mode: false,
            buffer: [0; 4],
            chr_banks: [0; 4],
        }
    }

    fn prg_bank(&self) -> usize {
        let bank = ((self.regs[0] >> 2) & 3) as usize;
        let num = self.prg_rom.num_banks();
        if num == 0 { 0 } else { bank % num }
    }

    fn resolve_chr_bank(&self, slot: usize) -> usize {
        let bank = self.chr_banks[slot] as usize;
        let num = self.chr_memory.size() / Self::CHR_BANK_SIZE;
        if num == 0 { 0 } else { bank % num }
    }

    /// Decode buffer bytes into CHR bank registers.
    /// The bit mapping is an obfuscation/copy-protection scheme.
    fn decode_buffer(&mut self) {
        let b0 = self.buffer[0];
        let b1 = self.buffer[1];
        let b2 = self.buffer[2];
        let b3 = self.buffer[3];

        self.chr_banks[0] = (b0 & 0x07) | ((b3 & 0x01) << 4) | ((b1 & 0x10) >> 1);
        self.chr_banks[1] = ((b0 & 0x70) >> 4) | (b3 & 0x10) | ((b1 & 0x20) >> 2);
        self.chr_banks[2] = (b2 & 0x0F) | ((b3 & 0x02) << 3) | ((b1 & 0x40) >> 2);
        self.chr_banks[3] = ((b2 & 0xF0) >> 4) | ((b3 & 0x20) >> 1) | ((b1 & 0x80) >> 3);
    }
}

impl Mapper for Mapper251 {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let bank = self.prg_bank();
                let offset = (addr - 0x8000) as usize;
                self.prg_rom.read(bank, offset)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr & 0xE001 {
            0x8000 => {
                // Bank select
                self.bank_select = value;
            }
            0x8001 => {
                // Bank data
                let reg = (self.bank_select & 7) as usize;
                self.regs[reg] = value;
            }
            0xA001 => {
                // SRAM mode control
                if value & 0x80 != 0 {
                    self.sram_mode = true;
                } else {
                    self.sram_mode = false;
                    self.buffer = [0; 4];
                }
            }
            _ => {}
        }

        // Also handle SRAM buffer writes when address is in $6000-$7FFF
        if (0x6000..=0x7FFF).contains(&addr) && self.sram_mode {
            let index = (addr & 3) as usize;
            self.buffer[index] = value;
            if index == 3 {
                self.decode_buffer();
            }
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let slot = ((addr >> 11) & 0x03) as usize; // 2KB slots
        let bank = self.resolve_chr_bank(slot);
        let offset = (addr & 0x07FF) as usize;
        let index = bank * Self::CHR_BANK_SIZE + offset;
        self.chr_memory.read_at_index(index)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.chr_memory.write(addr, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        Self::MAPPER_NUMBER
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = Vec::with_capacity(18);
        snap.push(self.bank_select);
        snap.extend_from_slice(&self.regs);
        snap.push(self.sram_mode as u8);
        snap.extend_from_slice(&self.buffer);
        snap.extend_from_slice(&self.chr_banks);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 17 {
            self.bank_select = data[0];
            self.regs.copy_from_slice(&data[1..9]);
            self.sram_mode = data[9] != 0;
            self.buffer.copy_from_slice(&data[10..14]);
            self.chr_banks.copy_from_slice(&data[14..18]);
        }
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.chr_memory.initialize(mode);
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_chr_banking: true,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 2,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_mapper251(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new(251, prg_rom, chr_rom, mirroring))
    }

    #[test]
    fn test_factory_creates_mapper_251() {
        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(2 * 1024, 32);
        let mapper = create_mapper251(prg_rom, chr_rom, NametableLayout::Vertical);
        assert!(mapper.is_ok());
    }

    #[test]
    fn test_prg_bank_switching() {
        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(2 * 1024, 16);
        let mut mapper = create_mapper251(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Set reg[0]: bank_select=0, then write value with bits 3:2 = 2
        mapper.write_prg(0x8000, 0); // Select register 0
        mapper.write_prg(0x8001, 0b00001000); // bits 3:2 = 0b10 = 2

        assert_eq!(mapper.read_prg(0x8000), 2);
    }

    #[test]
    fn test_sram_mode_enable_disable() {
        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(2 * 1024, 32);
        let mut mapper = create_mapper251(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Enable SRAM mode
        mapper.write_prg(0xA001, 0x80);

        // Write buffer values
        mapper.write_prg(0x6000, 0x07); // buf[0]
        mapper.write_prg(0x6001, 0x00); // buf[1]
        mapper.write_prg(0x6002, 0x00); // buf[2]
        mapper.write_prg(0x6003, 0x00); // buf[3] triggers decode

        // CHR bank 0 = buf[0] & 0x07 = 7
        assert_eq!(mapper.read_chr(0x0000), 7);

        // Disable SRAM mode → buffer should be cleared
        mapper.write_prg(0xA001, 0x00);

        // Further writes should not update CHR banks
        mapper.write_prg(0x6000, 0x01);
        mapper.write_prg(0x6003, 0x00);

        // CHR bank 0 should still be 7 (buffer not functioning)
        assert_eq!(mapper.read_chr(0x0000), 7);
    }

    #[test]
    fn test_chr_buffer_decode_slot0() {
        let prg_rom = banked_data(32 * 1024, 1);
        let chr_rom = banked_data(2 * 1024, 32);
        let mut mapper = create_mapper251(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        mapper.write_prg(0xA001, 0x80);

        // Set CHR bank 0 = 0b10111 = 23
        // chr_banks[0] = (buf[0] & 0x07) | ((buf[3] & 0x01) << 4) | ((buf[1] & 0x10) >> 1)
        // = 0x07 | (1 << 4) | (0x10 >> 1) = 7 | 16 | 8 = 31
        mapper.write_prg(0x6000, 0x07); // bits 0-2
        mapper.write_prg(0x6001, 0x10); // bit 4 → contributes bit 3
        mapper.write_prg(0x6002, 0x00);
        mapper.write_prg(0x6003, 0x01); // bit 0 → contributes bit 4

        assert_eq!(mapper.read_chr(0x0000), 31);
    }

    #[test]
    fn test_chr_buffer_decode_slot1() {
        let prg_rom = banked_data(32 * 1024, 1);
        let chr_rom = banked_data(2 * 1024, 32);
        let mut mapper = create_mapper251(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        mapper.write_prg(0xA001, 0x80);

        // chr_banks[1] = ((buf[0] & 0x70) >> 4) | ((buf[3] & 0x10)) | ((buf[1] & 0x20) >> 2)
        // buf[0] = 0x70 → bits 4-6 set → (0x70 >> 4) = 7
        // buf[1] = 0x20 → bit 5 → (0x20 >> 2) = 8
        // buf[3] = 0x10 → bit 4 → 0x10 = 16
        // = 7 | 16 | 8 = 31
        mapper.write_prg(0x6000, 0x70);
        mapper.write_prg(0x6001, 0x20);
        mapper.write_prg(0x6002, 0x00);
        mapper.write_prg(0x6003, 0x10);

        assert_eq!(mapper.read_chr(0x0800), 31);
    }

    #[test]
    fn test_chr_buffer_decode_all_slots() {
        let prg_rom = banked_data(32 * 1024, 1);
        let chr_rom = banked_data(2 * 1024, 32);
        let mut mapper = create_mapper251(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        mapper.write_prg(0xA001, 0x80);

        // Set specific banks in each slot using carefully crafted buffer
        // buf[0] = 0x12, buf[1] = 0x00, buf[2] = 0x34, buf[3] = 0x00
        // chr_banks[0] = (0x12 & 0x07) = 2
        // chr_banks[1] = ((0x12 & 0x70) >> 4) = 1
        // chr_banks[2] = (0x34 & 0x0F) = 4
        // chr_banks[3] = ((0x34 & 0xF0) >> 4) = 3
        mapper.write_prg(0x6000, 0x12);
        mapper.write_prg(0x6001, 0x00);
        mapper.write_prg(0x6002, 0x34);
        mapper.write_prg(0x6003, 0x00);

        assert_eq!(mapper.read_chr(0x0000), 2); // slot 0
        assert_eq!(mapper.read_chr(0x0800), 1); // slot 1
        assert_eq!(mapper.read_chr(0x1000), 4); // slot 2
        assert_eq!(mapper.read_chr(0x1800), 3); // slot 3
    }

    #[test]
    fn test_registers_snapshot_and_restore() {
        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(2 * 1024, 32);
        let mut mapper =
            create_mapper251(prg_rom.clone(), chr_rom.clone(), NametableLayout::Vertical).unwrap();

        // Enable SRAM and set some CHR banks
        mapper.write_prg(0xA001, 0x80);
        mapper.write_prg(0x6000, 0x05);
        mapper.write_prg(0x6001, 0x00);
        mapper.write_prg(0x6002, 0x00);
        mapper.write_prg(0x6003, 0x00);

        let regs = mapper.registers_snapshot();

        let mut restored = create_mapper251(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();
        restored.restore_registers(&regs);

        // chr_banks[0] = 5 & 0x07 = 5
        assert_eq!(restored.read_chr(0x0000), 5);
    }
}
