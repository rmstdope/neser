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
//! Known Limitations:
//! - **Expansion audio not implemented** (5B audio chip)

use crate::cartridge::BaseMapper;
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};
use crate::trace_mapper;

use super::cpu_cycle_irq::{CpuCycleIrq, CpuCycleIrqMode};

pub struct SunsoftFme7Mapper {
    base: BaseMapper,
    prg_ram: Vec<u8>,

    // Register selection
    command: u8,

    // PRG banking (4 x 8KB switchable banks)
    prg_banks: [u8; 4], // Banks for $6000-$7FFF, $8000-$9FFF, $A000-$BFFF, $C000-$DFFF
    prg_ram_enabled: bool,
    prg_ram_readonly: bool,

    // CHR banking (8 x 1KB banks)
    chr_banks: [u8; 8],

    // IRQ
    irq: CpuCycleIrq,
    irq_counter_enabled: bool,
}

impl SunsoftFme7Mapper {
    const PRG_RAM_SIZE: usize = 8 * 1024; // 8KB

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(0x2000);
        base.configure_chr_banking(0x0400);
        let mut mapper = Self {
            base,
            prg_ram: vec![0u8; Self::PRG_RAM_SIZE],
            command: 0,
            prg_banks: [0, 0, 0, 0],
            prg_ram_enabled: false,
            prg_ram_readonly: false,
            chr_banks: [0, 1, 2, 3, 4, 5, 6, 7],
            irq: CpuCycleIrq::new(CpuCycleIrqMode::DownUnderflow),
            irq_counter_enabled: false,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        // PRG: $8000-$FFFF = 4 x 8KB slots
        self.base.select_prg_page(0, self.prg_banks[1] as i16);
        self.base.select_prg_page(1, self.prg_banks[2] as i16);
        self.base.select_prg_page(2, self.prg_banks[3] as i16);
        self.base.select_prg_page(3, -1); // fixed last
        // CHR: 8 x 1KB slots
        for i in 0..8 {
            self.base.select_chr_page(i, self.chr_banks[i] as i16);
        }
    }

    fn read_prg_6000(&self, addr: u16) -> u8 {
        let prg_rom = self.base.prg_rom();
        let bank_size = 0x2000;
        let bank_count = prg_rom.len() / bank_size;
        if bank_count == 0 {
            return 0;
        }
        let bank = (self.prg_banks[0] as usize) % bank_count;
        let offset = (addr as usize) - 0x6000;
        prg_rom.get(bank * bank_size + offset).copied().unwrap_or(0)
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
                self.base.set_mirroring(match value & 0x03 {
                    0 => NametableLayout::Vertical,
                    1 => NametableLayout::Horizontal,
                    2 => NametableLayout::SingleScreenLower,
                    3 => NametableLayout::SingleScreenUpper,
                    _ => unreachable!(),
                });
            }
            0x0D => {
                // IRQ control
                self.irq.set_enabled((value & 0x01) != 0);
                self.irq_counter_enabled = (value & 0x80) != 0;

                if self.irq.enabled() {
                    self.irq.acknowledge();
                }

                trace_mapper!(1; "[fme7] IRQ enabled={}, counter_enabled={}", 
                    self.irq.enabled(), self.irq_counter_enabled);
            }
            0x0E => {
                // IRQ counter low byte
                self.irq
                    .set_counter((self.irq.counter() & 0xFF00) | (value as u16));
                trace_mapper!(1; "[fme7] IRQ counter low <- ${:02X}, counter now ${:04X}", 
                    value, self.irq.counter());
            }
            0x0F => {
                // IRQ counter high byte
                self.irq
                    .set_counter((self.irq.counter() & 0x00FF) | ((value as u16) << 8));
                trace_mapper!(1; "[fme7] IRQ counter high <- ${:02X}, counter now ${:04X}", 
                    value, self.irq.counter());
            }
            _ => {}
        }
        self.update_banks();
    }
}

impl Mapper for SunsoftFme7Mapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                if self.prg_ram_enabled {
                    let offset = (addr - 0x6000) as usize;
                    self.prg_ram.get(offset).copied().unwrap_or(0)
                } else {
                    self.read_prg_6000(addr)
                }
            }
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
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

    fn cpu_cycle(&mut self) {
        // IRQ counter decrements every CPU cycle when counter enable (bit 7) is set
        // IRQ triggers on underflow only if IRQ enable (bit 0) is also set
        if self.irq_counter_enabled {
            self.irq.tick();
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq.is_pending()
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

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        crate::console::initialize_ram(&mut self.prg_ram, mode);
        self.base.initialize_ram(mode);
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
            | ((self.irq.enabled() as u8) << 2)
            | ((self.irq_counter_enabled as u8) << 3)
            | ((self.irq.is_pending() as u8) << 4);
        snapshot.push(flags);
        snapshot.push((self.irq.counter() & 0xFF) as u8);
        snapshot.push((self.irq.counter() >> 8) as u8);
        let mirroring = match self.base.mirroring() {
            NametableLayout::Horizontal => 0,
            NametableLayout::Vertical => 1,
            NametableLayout::SingleScreenLower => 2,
            NametableLayout::SingleScreenUpper => 3,
            NametableLayout::SingleScreen => 2,
            NametableLayout::FourScreen => 4,
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
            self.irq.set_enabled((flags & 4) != 0);
            self.irq_counter_enabled = (flags & 8) != 0;
            self.irq.set_pending((flags & 16) != 0);
            self.irq
                .set_counter((data[14] as u16) | ((data[15] as u16) << 8));
            self.base.set_mirroring(match data[16] {
                0 => NametableLayout::Horizontal,
                1 => NametableLayout::Vertical,
                2 => NametableLayout::SingleScreenLower,
                3 => NametableLayout::SingleScreenUpper,
                4 => NametableLayout::FourScreen,
                _ => NametableLayout::Horizontal,
            });
            self.update_banks();
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
        let mapper = create_mapper(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));
        assert!(mapper.is_ok(), "Mapper 69 should be implemented");
    }

    #[test]
    fn test_prg_banking() {
        let prg_rom = banked_data(8 * 1024, 16); // 16 x 8KB banks
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

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
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

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
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

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
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

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
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        // Change to vertical
        mapper.write_prg(0x8000, 0x0C); // Command C = mirroring
        mapper.write_prg(0xA000, 0x00); // 0 = vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // Change to single screen lower
        mapper.write_prg(0x8000, 0x0C);
        mapper.write_prg(0xA000, 0x02); // 2 = single screen lower
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
    }

    #[test]
    fn test_irq_countdown() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

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
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

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
        let mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // $E000-$FFFF should read from last bank (bank 15)
        assert_eq!(mapper.read_prg(0xE000), 15);
    }

    #[test]
    fn test_fme7_registers_snapshot_restores_mirroring_and_banks() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
        ));

        // Set PRG bank 1 and CHR bank 0.
        mapper.write_prg(0x8000, 0x09);
        mapper.write_prg(0xA000, 0x03);
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0xA000, 0x04);

        // Set mirroring to single screen upper.
        mapper.write_prg(0x8000, 0x0C);
        mapper.write_prg(0xA000, 0x03);

        let regs = mapper.registers_snapshot();

        let mut restored = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));
        restored.restore_registers(&regs);

        assert_eq!(restored.get_mirroring(), NametableLayout::SingleScreenUpper);
        assert_eq!(restored.read_prg(0x8000), 3);
        assert_eq!(restored.read_chr(0x0000), 4);
    }
}
