//! Mapper 68 - Sunsoft-4
//!
//! Hardware: Sunsoft's mapper with CHR-ROM nametable support
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/Sunsoft-4>
//! - PRG-ROM: Up to 256KB (16KB switchable at $8000-$BFFF, last 16KB fixed at $C000-$FFFF)
//! - PRG-RAM: Up to 64KB at $6000-$7FFF with write protection
//! - CHR: Up to 256KB (four 2KB switchable banks) or CHR-RAM
//! - Mirroring: Programmable (horizontal, vertical, one-screen A/B)
//! - Nametable: Optional CHR-ROM mapping to nametables ($2000-$2FFF)
//!
//! Common boards: SUNSOFT-4
//!
//! Notes:
//! - Register $8000-$8003: Select 2KB CHR banks
//! - Register $8008: Select 16KB PRG bank at $8000-$BFFF
//! - Register $8009: Mirroring control
//! - Register $800A: CHR-ROM nametable mode enable
//! - Register $800B-$800C: 1KB nametable banks (when CHR-ROM nametable mode)
//! - Register $800D: PRG-RAM enable/write-protect
//! - Used in After Burner II, Nantettatte!! Baseball
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::common::{DEFAULT_PRG_RAM_SIZE, PrgRam};
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};

const PRG_BANK_SIZE: usize = 0x4000; // 16KB
const CHR_BANK_SIZE_2K: usize = 0x0800; // 2KB
const NAMETABLE_BANK_SIZE_1K: usize = 0x0400; // 1KB

pub struct Sunsoft4Mapper {
    base: BaseMapper,
    prg_ram: PrgRam,
    prg_bank: u8,
    chr_banks_2k: [u8; 4],
    nametable_banks_1k: [u8; 2],
    nametable_rom_mode: bool,
    prg_ram_enabled: bool,
}

impl Sunsoft4Mapper {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_ram_banks_8k = ctx.prg_ram_banks_8k;
        let caps = MapperCapabilities {
            has_irq: false,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 0, // PRG-RAM managed separately with enable gating
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 2,
            trainer_jsr: false,
            ..Default::default()
        };
        let base = BaseMapper::new(&ctx, caps);
        let prg_ram_size = (prg_ram_banks_8k.max(1) as usize) * DEFAULT_PRG_RAM_SIZE;
        Self {
            base,
            prg_ram: PrgRam::new(prg_ram_size),
            prg_bank: 0,
            chr_banks_2k: [0; 4],
            nametable_banks_1k: [0; 2],
            nametable_rom_mode: false,
            prg_ram_enabled: false,
        }
    }

    #[cfg(test)]
    pub fn new_with_prg_ram_banks(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
        prg_ram_banks_8k: u8,
    ) -> Self {
        let ctx = super::mapper::MapperContext {
            mapper: 68,
            submapper: 0,
            mirroring,
            console_type: crate::cartridge::ConsoleType::NesFamicom,
            prg_rom,
            chr_rom,
            prg_ram_banks_8k,
            prg_ram_size_specified: true,
            battery_backed_prg_ram: false,
            crc32: 0,
        };
        Self::new(ctx)
    }

    fn read_chr_indexed(&self, bank: u8, offset: usize, bank_size: usize) -> u8 {
        let total_banks = self.base.chr_size() / bank_size;
        if total_banks == 0 {
            return 0;
        }
        let bank_index = (bank as usize) % total_banks;
        let index = bank_index * bank_size + offset;
        self.base.read_chr_at_index(index)
    }

    fn write_chr_indexed(&mut self, bank: u8, offset: usize, bank_size: usize, value: u8) {
        let total_banks = self.base.chr_size() / bank_size;
        if total_banks == 0 {
            return;
        }
        let bank_index = (bank as usize) % total_banks;
        let index = bank_index * bank_size + offset;
        self.base.write_chr_at_index(index, value);
    }

    fn map_nametable_to_bank(&self, addr: u16) -> (usize, usize) {
        let addr = addr & 0x2FFF;
        let table = ((addr - 0x2000) / 0x0400) as usize;
        let offset = (addr & 0x03FF) as usize;

        let use_upper = match self.base.mirroring() {
            NametableLayout::Vertical => table == 1 || table == 3,
            NametableLayout::Horizontal => table == 2 || table == 3,
            NametableLayout::SingleScreenLower => false,
            NametableLayout::SingleScreenUpper => true,
            NametableLayout::FourScreen => table == 1 || table == 3,
            NametableLayout::SingleScreen => false,
        };

        let bank_index = if use_upper { 1 } else { 0 };
        (bank_index, offset)
    }

    fn chr_bank_for_addr(&self, addr: u16) -> (u8, usize) {
        let bank_index = ((addr & 0x1FFF) / CHR_BANK_SIZE_2K as u16) as usize;
        let offset = (addr as usize) & (CHR_BANK_SIZE_2K - 1);
        (self.chr_banks_2k[bank_index], offset)
    }

    fn nametable_bank_for_addr(&self, addr: u16) -> (u8, usize) {
        let (bank_index, offset) = self.map_nametable_to_bank(addr);
        let bank = (self.nametable_banks_1k[bank_index] & 0x7F) | 0x80;
        (bank, offset)
    }
}

impl Mapper for Sunsoft4Mapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if self.prg_ram_enabled
            && let Some(value) = self.prg_ram.try_read(addr)
        {
            return value;
        }

        let prg = self.base.prg_rom();
        let num_banks = prg.len() / PRG_BANK_SIZE;
        match addr {
            0x8000..=0xBFFF => {
                let bank = (self.prg_bank as usize) % num_banks.max(1);
                let offset = (addr - 0x8000) as usize;
                prg.get(bank * PRG_BANK_SIZE + offset).copied().unwrap_or(0)
            }
            0xC000..=0xFFFF => {
                let last_bank = num_banks.saturating_sub(1);
                let offset = (addr - 0xC000) as usize;
                prg.get(last_bank * PRG_BANK_SIZE + offset)
                    .copied()
                    .unwrap_or(0)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.prg_ram_enabled && self.prg_ram.try_write(addr, value) {
            return;
        }

        match addr {
            0x8000..=0x8FFF => {
                self.chr_banks_2k[0] = value;
            }
            0x9000..=0x9FFF => {
                self.chr_banks_2k[1] = value;
            }
            0xA000..=0xAFFF => {
                self.chr_banks_2k[2] = value;
            }
            0xB000..=0xBFFF => {
                self.chr_banks_2k[3] = value;
            }
            0xC000..=0xCFFF => {
                self.nametable_banks_1k[0] = value;
            }
            0xD000..=0xDFFF => {
                self.nametable_banks_1k[1] = value;
            }
            0xE000..=0xEFFF => {
                let new_mirroring = match value & 0x03 {
                    0 => NametableLayout::Vertical,
                    1 => NametableLayout::Horizontal,
                    2 => NametableLayout::SingleScreenLower,
                    3 => NametableLayout::SingleScreenUpper,
                    _ => NametableLayout::Horizontal,
                };
                self.base.set_mirroring(new_mirroring);
                self.nametable_rom_mode = (value & 0x10) != 0;
            }
            0xF000..=0xFFFF => {
                self.prg_bank = value & 0x0F;
                self.prg_ram_enabled = (value & 0x10) != 0;
            }
            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let (bank, offset) = self.chr_bank_for_addr(addr);
        self.read_chr_indexed(bank, offset, CHR_BANK_SIZE_2K)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let (bank, offset) = self.chr_bank_for_addr(addr);
        self.write_chr_indexed(bank, offset, CHR_BANK_SIZE_2K, value);
    }

    fn read_nametable(&mut self, addr: u16) -> Option<u8> {
        if !self.nametable_rom_mode {
            return None;
        }

        if !(0x2000..=0x2FFF).contains(&addr) {
            return None;
        }

        let (bank, offset) = self.nametable_bank_for_addr(addr);
        Some(self.read_chr_indexed(bank, offset, NAMETABLE_BANK_SIZE_1K))
    }

    fn write_nametable(&mut self, addr: u16, value: u8) -> bool {
        if !self.nametable_rom_mode {
            return false;
        }

        if !(0x2000..=0x2FFF).contains(&addr) {
            return false;
        }

        let (bank, offset) = self.nametable_bank_for_addr(addr);
        self.write_chr_indexed(bank, offset, NAMETABLE_BANK_SIZE_1K, value);
        true
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.base.mirroring()
    }

    fn mapper_number(&self) -> u8 {
        self.base.mapper_number()
    }

    fn wram_size(&self) -> usize {
        self.prg_ram.size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram.snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.prg_ram.load_snapshot(data);
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.base.chr_ram_snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.base.restore_chr_ram(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snapshot = Vec::with_capacity(10);
        snapshot.push(self.prg_bank);
        snapshot.extend_from_slice(&self.chr_banks_2k);
        snapshot.extend_from_slice(&self.nametable_banks_1k);
        snapshot.push(self.nametable_rom_mode as u8);
        snapshot.push(self.prg_ram_enabled as u8);
        let mirroring_bits = match self.base.mirroring() {
            NametableLayout::Vertical => 0,
            NametableLayout::Horizontal => 1,
            NametableLayout::SingleScreenLower => 2,
            NametableLayout::SingleScreenUpper => 3,
            NametableLayout::FourScreen | NametableLayout::SingleScreen => 0,
        };
        snapshot.push(mirroring_bits);
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 10 {
            return;
        }
        self.prg_bank = data[0];
        self.chr_banks_2k.copy_from_slice(&data[1..5]);
        self.nametable_banks_1k.copy_from_slice(&data[5..7]);
        self.nametable_rom_mode = data[7] != 0;
        self.prg_ram_enabled = data[8] != 0;
        self.base.set_mirroring(match data[9] & 0x03 {
            0 => NametableLayout::Vertical,
            1 => NametableLayout::Horizontal,
            2 => NametableLayout::SingleScreenLower,
            3 => NametableLayout::SingleScreenUpper,
            _ => self.base.mirroring(),
        });
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.chr_banks_2k = [0; 4];
        self.nametable_banks_1k = [0; 2];
        self.nametable_rom_mode = false;
        self.prg_ram_enabled = false;
    }

    fn capabilities(&self) -> MapperCapabilities {
        let mut caps = self.base.capabilities();
        caps.max_prg_ram_kb = self.prg_ram.size() / 1024;
        caps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prg_bank_switching_selects_16k_bank() {
        let mut prg_rom = vec![0u8; 4 * PRG_BANK_SIZE];
        for bank in 0..4 {
            let start = bank * PRG_BANK_SIZE;
            let end = start + PRG_BANK_SIZE;
            for byte in &mut prg_rom[start..end] {
                *byte = bank as u8;
            }
        }

        let mut mapper = Sunsoft4Mapper::new_with_prg_ram_banks(
            prg_rom,
            vec![0u8; 8 * 1024],
            NametableLayout::Horizontal,
            1,
        );

        // Default power-on PRG bank is 0 in the switchable $8000-$BFFF window.
        assert_eq!(mapper.read_prg(0x8000), 0);
        // The fixed $C000-$FFFF window always maps the last 16KB PRG bank.
        assert_eq!(mapper.read_prg(0xC000), 3);

        mapper.write_prg(0xF000, 0x02);
        // Writing to $F000 selects the 16KB PRG bank for $8000-$BFFF.
        assert_eq!(mapper.read_prg(0x8000), 2);

        mapper.write_prg(0xFFFF, 0x0F);
        // Bank selection wraps by available banks (0x0F -> bank 3 for 4 banks).
        assert_eq!(mapper.read_prg(0x8000), 3);
    }

    #[test]
    fn chr_bank_switching_uses_2k_banks() {
        let mut chr_rom = vec![0u8; 4 * 2 * 1024];
        for bank in 0..4 {
            let start = bank * 2 * 1024;
            let end = start + 2 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = bank as u8;
            }
        }

        let mut mapper = Sunsoft4Mapper::new_with_prg_ram_banks(
            vec![0u8; 2 * PRG_BANK_SIZE],
            chr_rom,
            NametableLayout::Horizontal,
            1,
        );
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x9000, 0x01);
        mapper.write_prg(0xA000, 0x02);
        mapper.write_prg(0xB000, 0x03);

        // Each 2KB CHR bank maps to a 2KB PPU window in order.
        assert_eq!(mapper.read_chr(0x0000), 0);
        // Bank 1 maps to $0800-$0FFF.
        assert_eq!(mapper.read_chr(0x0800), 1);
        // Bank 2 maps to $1000-$17FF.
        assert_eq!(mapper.read_chr(0x1000), 2);
        // Bank 3 maps to $1800-$1FFF.
        assert_eq!(mapper.read_chr(0x1800), 3);
    }

    #[test]
    fn nametable_rom_mode_follows_mirroring() {
        let chr_bank_count = 256;
        let mut chr_rom = vec![0u8; chr_bank_count * 1024];
        for bank in 0..chr_bank_count {
            let start = bank * 1024;
            let end = start + 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = bank as u8;
            }
        }

        let mut mapper = Sunsoft4Mapper::new_with_prg_ram_banks(
            vec![0u8; 2 * PRG_BANK_SIZE],
            chr_rom,
            NametableLayout::Horizontal,
            1,
        );

        mapper.write_prg(0xC000, 0x01);
        mapper.write_prg(0xD000, 0x02);

        mapper.write_prg(0xE000, 0x10);
        // ROM nametable mode uses CHR-ROM banks for $2000/$2800 based on mirroring.
        assert_eq!(mapper.read_nametable(0x2000), Some(0x81));
        // Vertical mirroring: $2000 and $2800 share the lower bank.
        assert_eq!(mapper.read_nametable(0x2800), Some(0x81));

        mapper.write_prg(0xE000, 0x11);
        // Horizontal mirroring: $2000 and $2400 share the lower bank.
        assert_eq!(mapper.read_nametable(0x2000), Some(0x81));
        assert_eq!(mapper.read_nametable(0x2400), Some(0x81));
        // Horizontal mirroring: $2800 uses the upper bank.
        assert_eq!(mapper.read_nametable(0x2800), Some(0x82));
    }

    #[test]
    fn test_sunsoft4_registers_snapshot_roundtrip() {
        // Test comprehensive state save/restore for Sunsoft-4
        let mut prg_rom = vec![0u8; 4 * PRG_BANK_SIZE];
        let mut chr_rom = vec![0u8; 256 * 1024]; // 256KB CHR-ROM

        // Fill PRG ROM with bank-specific data
        for bank in 0..4 {
            let start = bank * PRG_BANK_SIZE;
            let end = start + PRG_BANK_SIZE;
            for byte in &mut prg_rom[start..end] {
                *byte = bank as u8;
            }
        }

        // Fill CHR ROM with bank-specific data (each 2KB bank)
        for bank in 0..128 {
            let start = bank * 2048;
            let end = start + 2048;
            for byte in &mut chr_rom[start..end] {
                *byte = bank as u8;
            }
        }

        let mut mapper = Sunsoft4Mapper::new_with_prg_ram_banks(
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Vertical,
            1,
        );

        // Configure complex state
        mapper.write_prg(0x8000, 10); // CHR bank 0 (addresses $8000-$8FFF)
        mapper.write_prg(0x9000, 20); // CHR bank 1 (addresses $9000-$9FFF)
        mapper.write_prg(0xA000, 30); // CHR bank 2 (addresses $A000-$AFFF)
        mapper.write_prg(0xB000, 40); // CHR bank 3 (addresses $B000-$BFFF)
        mapper.write_prg(0xF000, 2); // PRG bank at $8000 (addresses $F000-$FFFF)
        mapper.write_prg(0xE000, 1); // Mirroring control (addresses $E000-$EFFF): 1 = horizontal
        mapper.write_prg(0xC000, 50); // Nametable bank 0 (addresses $C000-$CFFF)
        mapper.write_prg(0xD000, 60); // Nametable bank 1 (addresses $D000-$DFFF)

        // Verify state before snapshot
        assert_eq!(mapper.read_chr(0x0000), 10);
        assert_eq!(mapper.read_chr(0x0800), 20);
        assert_eq!(mapper.read_chr(0x1000), 30);
        assert_eq!(mapper.read_chr(0x1800), 40);
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        // Take snapshot
        let registers = mapper.registers_snapshot();
        let prg_ram = mapper.wram_snapshot();

        // Create fresh mapper and restore
        let mut restored =
            Sunsoft4Mapper::new_with_prg_ram_banks(prg_rom, chr_rom, NametableLayout::Vertical, 1);
        restored.restore_registers(&registers);
        restored.load_wram_snapshot(&prg_ram);

        // Verify all state is restored correctly
        assert_eq!(restored.read_chr(0x0000), 10);
        assert_eq!(restored.read_chr(0x0800), 20);
        assert_eq!(restored.read_chr(0x1000), 30);
        assert_eq!(restored.read_chr(0x1800), 40);
        assert_eq!(restored.read_prg(0x8000), 2);
        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
    }
}
