//! Mapper 69 - Sunsoft FME-7 (Sunsoft 5A/5B)
//!
//! Hardware: Sunsoft's advanced mapper with IRQ counter and optional expansion audio
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/Sunsoft_FME-7>
//! - Audio: <https://www.nesdev.org/wiki/Sunsoft_5B_audio> (5B variant only)
//! - PRG-ROM: Up to 512KB with 8KB banking
//! - PRG-RAM: Up to 512KB (unusual, can be banked at $6000-$7FFF)
//! - CHR: Up to 256KB (eight 1KB switchable banks)
//! - Mirroring: Programmable (horizontal, vertical, one-screen A/B)
//! - IRQ: 16-bit CPU-cycle countdown timer
//!
//! Common boards: Sunsoft FME-7 (5A without audio, 5B with audio)
//!
//! Memory Map:
//! - $6000-$7FFF: Bank 0 (can be PRG-RAM or PRG-ROM)
//! - $8000-$9FFF: Bank 1 (PRG-ROM)
//! - $A000-$BFFF: Bank 2 (PRG-ROM)
//! - $C000-$DFFF: Bank 3 (PRG-ROM)
//! - $E000-$FFFF: Bank 4 (PRG-ROM, usually fixed to last bank)
//!
//! Registers (two-step access):
//! 1. Write command number to $8000-$9FFF
//! 2. Write parameter to $A000-$BFFF
//!
//! Commands:
//! - $00-$07: CHR bank select (1KB each)
//! - $08-$0B: PRG bank select (8KB each at $6000, $8000, $A000, $C000)
//! - $0C: Mirroring control
//! - $0D: IRQ control (enable/disable, acknowledge)
//! - $0E: IRQ counter low byte
//! - $0F: IRQ counter high byte
//!
//! Notes:
//! - Used in Gimmick! (with 5B audio), Batman: Return of the Joker, Hebereke
//! - 5B variant adds YM2149F-compatible expansion audio (3 square waves)
//!
//! Limitations:
//! - **Expansion audio not implemented** (5B audio chip)

use crate::cartridge::common::ChrMemory;
use crate::cartridge::{Mapper, MapperCapabilities, MirroringMode};
use crate::trace_mapper;

pub struct SunsoftFme7Mapper {
    prg_rom: Vec<u8>,
    prg_ram: Vec<u8>,
    chr_memory: ChrMemory,
    mirroring: MirroringMode,

    // Register selection
    command: u8,

    // PRG banking (4 x 8KB switchable banks)
    prg_banks: [u8; 4], // Banks for $6000-$7FFF, $8000-$9FFF, $A000-$BFFF, $C000-$DFFF
    prg_ram_enabled: bool,
    prg_ram_readonly: bool,

    // CHR banking (8 x 1KB banks)
    chr_banks: [u8; 8],

    // IRQ
    irq_counter: u16,
    irq_enabled: bool,
    irq_counter_enabled: bool,
    irq_pending: bool,
}

impl SunsoftFme7Mapper {
    const PRG_BANK_SIZE: usize = 8 * 1024; // 8KB
    const CHR_BANK_SIZE: usize = 1024; // 1KB
    const PRG_RAM_SIZE: usize = 8 * 1024; // 8KB (can be larger, but 8KB is standard)

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        Self {
            prg_rom,
            prg_ram: vec![0u8; Self::PRG_RAM_SIZE],
            chr_memory: ChrMemory::new(chr_rom),
            mirroring,
            command: 0,
            prg_banks: [0, 0, 0, 0],
            prg_ram_enabled: false,
            prg_ram_readonly: false,
            chr_banks: [0, 1, 2, 3, 4, 5, 6, 7],
            irq_counter: 0,
            irq_enabled: false,
            irq_counter_enabled: false,
            irq_pending: false,
        }
    }

    fn prg_bank_count(&self) -> usize {
        self.prg_rom.len() / Self::PRG_BANK_SIZE
    }

    fn chr_bank_count(&self) -> usize {
        self.chr_memory.size() / Self::CHR_BANK_SIZE
    }

    fn read_prg_bank(&self, bank: u8, offset: usize) -> u8 {
        let bank_count = self.prg_bank_count();
        if bank_count == 0 {
            return 0;
        }
        let bank_index = (bank as usize) % bank_count;
        let addr = bank_index * Self::PRG_BANK_SIZE + offset;
        self.prg_rom.get(addr).copied().unwrap_or(0)
    }

    fn read_chr_byte(&self, bank: u8, offset: usize) -> u8 {
        let bank_count = self.chr_bank_count();
        if bank_count == 0 {
            return 0;
        }
        let bank_index = (bank as usize) % bank_count;
        let addr = bank_index * Self::CHR_BANK_SIZE + offset;
        self.chr_memory.read_at_index(addr)
    }

    fn write_chr_byte(&mut self, bank: u8, offset: usize, value: u8) {
        let bank_count = self.chr_bank_count();
        if bank_count == 0 {
            return;
        }
        let bank_index = (bank as usize) % bank_count;
        let addr = bank_index * Self::CHR_BANK_SIZE + offset;
        self.chr_memory.write_at_index(addr, value);
    }

    fn write_command(&mut self, value: u8) {
        self.command = value & 0x0F;
        trace_mapper!(1; "[fme7] Command set to ${:02X}", self.command);
    }

    fn write_parameter(&mut self, value: u8) {
        trace_mapper!(1; "[fme7] Command ${:02X} <- ${:02X}", self.command, value);

        match self.command {
            0x00..=0x07 => {
                // CHR bank select (1KB each)
                self.chr_banks[self.command as usize] = value;
            }
            0x08 => {
                // PRG bank 0 ($6000-$7FFF)
                // Bit 7: RAM enable
                // Bit 6: RAM write protect (1 = readonly)
                // Bits 5-0: Bank number
                self.prg_ram_enabled = (value & 0x80) != 0;
                self.prg_ram_readonly = (value & 0x40) != 0;
                self.prg_banks[0] = value & 0x3F;
            }
            0x09..=0x0B => {
                // PRG banks 1-3 ($8000-$9FFF, $A000-$BFFF, $C000-$DFFF)
                let bank_index = (self.command - 0x09 + 1) as usize;
                self.prg_banks[bank_index] = value & 0x3F;
            }
            0x0C => {
                // Mirroring
                self.mirroring = match value & 0x03 {
                    0 => MirroringMode::Vertical,
                    1 => MirroringMode::Horizontal,
                    2 => MirroringMode::SingleScreenLower,
                    3 => MirroringMode::SingleScreenUpper,
                    _ => unreachable!(),
                };
            }
            0x0D => {
                // IRQ control
                self.irq_enabled = (value & 0x01) != 0;
                self.irq_counter_enabled = (value & 0x80) != 0;

                if self.irq_enabled {
                    self.irq_pending = false;
                }

                trace_mapper!(1; "[fme7] IRQ enabled={}, counter_enabled={}", 
                    self.irq_enabled, self.irq_counter_enabled);
            }
            0x0E => {
                // IRQ counter low byte
                self.irq_counter = (self.irq_counter & 0xFF00) | (value as u16);
                trace_mapper!(1; "[fme7] IRQ counter low <- ${:02X}, counter now ${:04X}", 
                    value, self.irq_counter);
            }
            0x0F => {
                // IRQ counter high byte
                self.irq_counter = (self.irq_counter & 0x00FF) | ((value as u16) << 8);
                trace_mapper!(1; "[fme7] IRQ counter high <- ${:02X}, counter now ${:04X}", 
                    value, self.irq_counter);
            }
            _ => {}
        }
    }
}

impl Mapper for SunsoftFme7Mapper {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                // Bank 0 (can be RAM or ROM)
                if self.prg_ram_enabled {
                    // RAM access
                    let offset = (addr - 0x6000) as usize;
                    self.prg_ram.get(offset).copied().unwrap_or(0)
                } else {
                    // ROM access
                    let offset = (addr - 0x6000) as usize;
                    self.read_prg_bank(self.prg_banks[0], offset)
                }
            }
            0x8000..=0x9FFF => {
                // Bank 1
                let offset = (addr - 0x8000) as usize;
                self.read_prg_bank(self.prg_banks[1], offset)
            }
            0xA000..=0xBFFF => {
                // Bank 2
                let offset = (addr - 0xA000) as usize;
                self.read_prg_bank(self.prg_banks[2], offset)
            }
            0xC000..=0xDFFF => {
                // Bank 3
                let offset = (addr - 0xC000) as usize;
                self.read_prg_bank(self.prg_banks[3], offset)
            }
            0xE000..=0xFFFF => {
                // Bank 4: fixed to the last PRG ROM bank ($E000-$FFFF is not switchable on FME-7)
                // Use last bank by default
                let last_bank = self.prg_bank_count().saturating_sub(1) as u8;
                let offset = (addr - 0xE000) as usize;
                self.read_prg_bank(last_bank, offset)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                // PRG-RAM writes (if enabled and not readonly)
                if self.prg_ram_enabled && !self.prg_ram_readonly {
                    let offset = (addr - 0x6000) as usize;
                    if offset < self.prg_ram.len() {
                        self.prg_ram[offset] = value;
                    }
                }
            }
            0x8000..=0x9FFF => {
                // Command register
                self.write_command(value);
            }
            0xA000..=0xBFFF => {
                // Parameter register
                self.write_parameter(value);
            }
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let slot = (addr >> 10) as usize; // Which 1KB slot (0-7)
        let offset = (addr & 0x03FF) as usize; // Offset within 1KB
        let bank = self.chr_banks[slot];
        self.read_chr_byte(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let slot = (addr >> 10) as usize;
        let offset = (addr & 0x03FF) as usize;
        let bank = self.chr_banks[slot];
        self.write_chr_byte(bank, offset, value);
    }

    fn cpu_cycle(&mut self) {
        // IRQ counter decrements every CPU cycle when counter enable (bit 7) is set
        // IRQ triggers on underflow only if IRQ enable (bit 0) is also set
        if self.irq_counter_enabled {
            self.irq_counter = self.irq_counter.wrapping_sub(1);
            if self.irq_counter == 0xFFFF && self.irq_enabled {
                self.irq_pending = true;
                trace_mapper!(2; "[fme7] IRQ triggered on underflow");
            }
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn get_mirroring(&self) -> MirroringMode {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        69
    }

    fn wram_size(&self) -> usize {
        Self::PRG_RAM_SIZE
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram.clone()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        let to_copy = data.len().min(self.prg_ram.len());
        self.prg_ram[..to_copy].copy_from_slice(&data[..to_copy]);
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Serialize Sunsoft FME-7 internal registers:
        // [0]: command
        // [1-4]: prg_banks[0-3]
        // [5-12]: chr_banks[0-7]
        // [13]: flags (prg_ram_enabled, prg_ram_readonly, irq_enabled, irq_counter_enabled, irq_pending)
        // [14-15]: irq_counter (little endian)
        // [16]: mirroring
        let mut snapshot = Vec::with_capacity(17);
        snapshot.push(self.command);
        snapshot.extend_from_slice(&self.prg_banks);
        snapshot.extend_from_slice(&self.chr_banks);
        let flags = (self.prg_ram_enabled as u8)
            | ((self.prg_ram_readonly as u8) << 1)
            | ((self.irq_enabled as u8) << 2)
            | ((self.irq_counter_enabled as u8) << 3)
            | ((self.irq_pending as u8) << 4);
        snapshot.push(flags);
        snapshot.push((self.irq_counter & 0xFF) as u8);
        snapshot.push((self.irq_counter >> 8) as u8);
        let mirroring = match self.mirroring {
            MirroringMode::Horizontal => 0,
            MirroringMode::Vertical => 1,
            MirroringMode::SingleScreenLower => 2,
            MirroringMode::SingleScreenUpper => 3,
            MirroringMode::SingleScreen => 2,
            MirroringMode::FourScreen => 4,
        };
        snapshot.push(mirroring);
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 17 {
            self.command = data[0];
            self.prg_banks.copy_from_slice(&data[1..5]);
            self.chr_banks.copy_from_slice(&data[5..13]);
            let flags = data[13];
            self.prg_ram_enabled = (flags & 1) != 0;
            self.prg_ram_readonly = (flags & 2) != 0;
            self.irq_enabled = (flags & 4) != 0;
            self.irq_counter_enabled = (flags & 8) != 0;
            self.irq_pending = (flags & 16) != 0;
            self.irq_counter = (data[14] as u16) | ((data[15] as u16) << 8);
            self.mirroring = match data[16] {
                0 => MirroringMode::Horizontal,
                1 => MirroringMode::Vertical,
                2 => MirroringMode::SingleScreenLower,
                3 => MirroringMode::SingleScreenUpper,
                4 => MirroringMode::FourScreen,
                _ => MirroringMode::Horizontal,
            };
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    #[test]
    fn test_mapper_69_is_wired_in_factory() {
        let prg_rom = banked_data(8 * 1024, 16);
        let chr_rom = banked_data(1024, 8);
        let mapper = create_mapper(MapperContext::new(
            69,
            prg_rom,
            chr_rom,
            MirroringMode::Horizontal,
        ));
        assert!(mapper.is_ok(), "Mapper 69 should be implemented");
    }

    #[test]
    fn test_prg_banking() {
        let prg_rom = banked_data(8 * 1024, 16); // 16 x 8KB banks
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Set bank 1 ($8000-$9FFF) to bank 5
        mapper.write_prg(0x8000, 0x09); // Command 9 = PRG bank 1
        mapper.write_prg(0xA000, 0x05); // Parameter = bank 5

        // Read from $8000 should return data from bank 5
        assert_eq!(mapper.read_prg(0x8000), 5);
    }

    #[test]
    fn test_chr_banking() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 64); // 64 x 1KB banks
        let mut mapper = SunsoftFme7Mapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Set CHR bank 0 to bank 10
        mapper.write_prg(0x8000, 0x00); // Command 0 = CHR bank 0
        mapper.write_prg(0xA000, 0x0A); // Parameter = bank 10

        // Read from $0000 should return data from bank 10
        assert_eq!(mapper.read_chr(0x0000), 10);
    }

    #[test]
    fn test_prg_ram_access() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Initially RAM is disabled
        mapper.write_prg(0x6000, 0x42);
        assert_eq!(mapper.read_prg(0x6000), 0); // Should read ROM bank 0

        // Enable RAM (command 8, bit 7 = 1)
        mapper.write_prg(0x8000, 0x08); // Command 8 = PRG bank 0
        mapper.write_prg(0xA000, 0x80); // Enable RAM (bit 7)

        // Now writes should work
        mapper.write_prg(0x6000, 0x42);
        assert_eq!(mapper.read_prg(0x6000), 0x42);
    }

    #[test]
    fn test_prg_ram_readonly() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Enable RAM but make it readonly
        mapper.write_prg(0x8000, 0x08); // Command 8 = PRG bank 0
        mapper.write_prg(0xA000, 0xC0); // Enable RAM (bit 7) + readonly (bit 6)

        // Writes should be ignored
        mapper.write_prg(0x6000, 0x42);
        assert_eq!(mapper.read_prg(0x6000), 0);

        // Make it writable
        mapper.write_prg(0x8000, 0x08);
        mapper.write_prg(0xA000, 0x80); // Enable RAM, writable

        mapper.write_prg(0x6000, 0x42);
        assert_eq!(mapper.read_prg(0x6000), 0x42);
    }

    #[test]
    fn test_mirroring() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);

        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal);

        // Change to vertical
        mapper.write_prg(0x8000, 0x0C); // Command C = mirroring
        mapper.write_prg(0xA000, 0x00); // 0 = vertical
        assert_eq!(mapper.get_mirroring(), MirroringMode::Vertical);

        // Change to single screen lower
        mapper.write_prg(0x8000, 0x0C);
        mapper.write_prg(0xA000, 0x02); // 2 = single screen lower
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenLower);
    }

    #[test]
    fn test_irq_countdown() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Set counter to 10
        mapper.write_prg(0x8000, 0x0E); // Command E = IRQ counter low
        mapper.write_prg(0xA000, 0x0A); // Counter = 10
        mapper.write_prg(0x8000, 0x0F); // Command F = IRQ counter high
        mapper.write_prg(0xA000, 0x00); // Counter high = 0

        // Enable IRQ and counter
        mapper.write_prg(0x8000, 0x0D); // Command D = IRQ control
        mapper.write_prg(0xA000, 0x81); // Enable IRQ (bit 0) and counter (bit 7)

        assert!(!mapper.irq_pending());

        // Countdown should decrement each CPU cycle
        // After 10 cycles, counter goes from 10 -> 9 -> ... -> 1 -> 0
        for _ in 0..10 {
            mapper.cpu_cycle();
            assert!(!mapper.irq_pending());
        }

        // After one more cycle, counter underflows from 0 to $FFFF and IRQ triggers
        mapper.cpu_cycle();
        assert!(mapper.irq_pending());
    }

    #[test]
    fn test_irq_disabled() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Set counter to 5
        mapper.write_prg(0x8000, 0x0E);
        mapper.write_prg(0xA000, 0x05);
        mapper.write_prg(0x8000, 0x0F);
        mapper.write_prg(0xA000, 0x00);

        // Enable counter but not IRQ
        mapper.write_prg(0x8000, 0x0D);
        mapper.write_prg(0xA000, 0x80); // Counter enabled (bit 7), IRQ disabled (bit 0 = 0)

        // Counter should count down even with IRQ disabled
        // After 5 cycles: 5->4->3->2->1->0
        for _ in 0..5 {
            mapper.cpu_cycle();
            assert!(!mapper.irq_pending()); // IRQ should not trigger
        }

        // After one more cycle, counter underflows to $FFFF but IRQ still shouldn't trigger
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending());

        // Now enable IRQ - it should trigger immediately since counter already underflowed
        mapper.write_prg(0x8000, 0x0D);
        mapper.write_prg(0xA000, 0x81); // Enable both counter and IRQ

        // IRQ should not trigger immediately on enable (counter is already at $FFFF-1 or similar)
        // But on next underflow it will trigger
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn test_last_prg_bank_fixed() {
        let prg_rom = banked_data(8 * 1024, 16); // 16 x 8KB banks
        let chr_rom = banked_data(1024, 8);
        let mapper = SunsoftFme7Mapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);

        // $E000-$FFFF should read from last bank (bank 15)
        assert_eq!(mapper.read_prg(0xE000), 15);
    }

    #[test]
    fn test_fme7_registers_snapshot_restores_mirroring_and_banks() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper =
            SunsoftFme7Mapper::new(prg_rom.clone(), chr_rom.clone(), MirroringMode::Horizontal);

        // Set PRG bank 1 and CHR bank 0.
        mapper.write_prg(0x8000, 0x09);
        mapper.write_prg(0xA000, 0x03);
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0xA000, 0x04);

        // Set mirroring to single screen upper.
        mapper.write_prg(0x8000, 0x0C);
        mapper.write_prg(0xA000, 0x03);

        let regs = mapper.registers_snapshot();

        let mut restored = SunsoftFme7Mapper::new(prg_rom, chr_rom, MirroringMode::Vertical);
        restored.restore_registers(&regs);

        assert_eq!(restored.get_mirroring(), MirroringMode::SingleScreenUpper);
        assert_eq!(restored.read_prg(0x8000), 3);
        assert_eq!(restored.read_chr(0x0000), 4);
    }
}
