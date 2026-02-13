//! # Mapper 16 - Bandai FCG (FCG-1, FCG-2, LZ93D50 with 24C01/24C02 EEPROM)
//!
//! Hardware: Bandai's mapper with CPU-driven IRQ counter and optional EEPROM
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_016>
//! - Submappers: <https://www.nesdev.org/wiki/INES_Mapper_016#Submappers>
//! - EEPROM: <https://www.nesdev.org/wiki/INES_Mapper_016#24C01_and_24C02_EEPROM>
//! - PRG-ROM: Up to 256KB (16KB switchable at $8000-$BFFF, last bank fixed at $C000-$FFFF)
//! - PRG-RAM: None
//! - CHR: Up to 128KB (8×1KB switchable banks) or CHR-RAM
//! - Mirroring: Programmable (horizontal, vertical, one-screen A/B)
//!
//! Common boards: Bandai FCG-1, FCG-2, LZ93D50
//!
//! Notes:
//! - Three submapper variants (0=Both, 4=FCG-1/2, 5=LZ93D50)
//! - Submapper 4: Registers at $6000-$7FFF, direct IRQ counter writes
//! - Submapper 5: Registers at $8000-$800F, latched IRQ counter
//! - CPU-cycle driven IRQ counter (counts down from 16-bit value)
//! - Used in Dragon Ball series, SD Gundam series
//!
//! Limitations:
//! - **EEPROM not implemented**: 24C02 EEPROM (register $800D) used for save data
//!   in some games (Dragon Ball Z II/III, SD Gundam Gaiden) is not supported
//! - Games requiring EEPROM cannot save progress
use crate::trace_mapper;

use crate::cartridge::cartridge::MirroringMode;
use crate::cartridge::common::{BankedRom, ChrMemory};
use crate::cartridge::mapper::Mapper;

/// Submapper variants for Bandai FCG
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandaiFcgVariant {
    /// Submapper 0: Unspecified - respond to both $6000-$7FFF and $8000-$FFFF
    Both,
    /// Submapper 4: FCG-1/2 ASIC - registers at $6000-$7FFF, direct counter writes
    Fcg1_2,
    /// Submapper 5: LZ93D50 ASIC - registers at $8000-$800F, latched counter
    Lz93d50,
}

pub struct BandaiFcgMapper {
    prg_rom: BankedRom,
    chr_memory: ChrMemory,
    mirroring: MirroringMode,
    variant: BandaiFcgVariant,

    // PRG banking
    prg_bank: u8, // 16KB bank at $8000-$BFFF

    // CHR banking (8 x 1KB)
    chr_banks: [u8; 8],

    // IRQ
    irq_enabled: bool,
    irq_counter: u16,
    irq_latch: u16, // Only used by LZ93D50
    irq_pending: bool,
}

impl BandaiFcgMapper {
    const PRG_BANK_SIZE: usize = 16 * 1024; // 16KB
    const CHR_BANK_SIZE: usize = 1024; // 1KB

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        // Default to Both variant for submapper 0 (unspecified) compatibility
        Self::new_with_variant(prg_rom, chr_rom, mirroring, BandaiFcgVariant::Both)
    }

    pub fn new_with_variant(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: MirroringMode,
        variant: BandaiFcgVariant,
    ) -> Self {
        Self {
            prg_rom: BankedRom::new(prg_rom, Self::PRG_BANK_SIZE),
            chr_memory: ChrMemory::new(chr_rom),
            mirroring,
            variant,
            prg_bank: 0,
            chr_banks: [0; 8],
            irq_enabled: false,
            irq_counter: 0,
            irq_latch: 0,
            irq_pending: false,
        }
    }

    fn prg_bank_count(&self) -> usize {
        self.prg_rom.num_banks()
    }

    fn last_prg_bank(&self) -> usize {
        let count = self.prg_bank_count();
        if count == 0 { 0 } else { count - 1 }
    }

    fn chr_bank_count(&self) -> usize {
        self.chr_memory.size() / Self::CHR_BANK_SIZE
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
}

impl Mapper for BandaiFcgMapper {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => {
                // Switchable 16KB bank
                self.prg_rom
                    .read_with_base(self.prg_bank as usize, 0x8000, addr)
            }
            0xC000..=0xFFFF => {
                // Fixed last 16KB bank
                let bank_index = self.last_prg_bank();
                self.prg_rom.read_with_base(bank_index, 0xC000, addr)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // Determine which address range this write falls into and if it's valid
        let is_6000_range = (0x6000..=0x7FFF).contains(&addr);
        let is_8000_range = (0x8000..=0xFFFF).contains(&addr);

        let in_range = match self.variant {
            BandaiFcgVariant::Both => is_6000_range || is_8000_range,
            BandaiFcgVariant::Fcg1_2 => is_6000_range,
            BandaiFcgVariant::Lz93d50 => is_8000_range,
        };

        if !in_range {
            return;
        }

        // For "Both" variant, use FCG-1/2 behavior for $6000 range, LZ93D50 for $8000 range
        let use_latch_behavior = match self.variant {
            BandaiFcgVariant::Both => is_8000_range,
            BandaiFcgVariant::Fcg1_2 => false,
            BandaiFcgVariant::Lz93d50 => true,
        };

        let reg = addr & 0x000F;
        match reg {
            0x00..=0x07 => {
                // CHR bank select
                self.chr_banks[reg as usize] = value;
            }
            0x08 => {
                // PRG bank select
                self.prg_bank = value & 0x0F;
            }
            0x09 => {
                // Mirroring
                self.mirroring = match value & 0x03 {
                    0 => MirroringMode::Vertical,
                    1 => MirroringMode::Horizontal,
                    2 => MirroringMode::SingleScreenLower,
                    3 => MirroringMode::SingleScreenUpper,
                    _ => unreachable!(),
                };
            }
            0x0A => {
                // IRQ control
                // Writing acknowledges pending IRQ
                self.irq_pending = false;

                if use_latch_behavior {
                    // LZ93D50 behavior: copy latch to counter
                    self.irq_counter = self.irq_latch;
                }
                // FCG-1/2: counter was written directly, no latch copy

                // Enable/disable
                self.irq_enabled = (value & 0x01) != 0;
                // If enabled while counter is 0, trigger immediately
                if self.irq_enabled && self.irq_counter == 0 {
                    self.irq_pending = true;
                }
            }
            0x0B => {
                // IRQ counter/latch low byte
                if use_latch_behavior {
                    self.irq_latch = (self.irq_latch & 0xFF00) | (value as u16);
                } else {
                    // Direct counter write
                    self.irq_counter = (self.irq_counter & 0xFF00) | (value as u16);
                }
            }
            0x0C => {
                // IRQ counter/latch high byte
                if use_latch_behavior {
                    self.irq_latch = (self.irq_latch & 0x00FF) | ((value as u16) << 8);
                } else {
                    // Direct counter write
                    self.irq_counter = (self.irq_counter & 0x00FF) | ((value as u16) << 8);
                }
            }
            0x0D => {
                // EEPROM control (not implemented yet)
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
        trace_mapper!(5; "[bandai_fcg] cpu_cycle");
        // IRQ counter decrements every CPU cycle when enabled
        if self.irq_enabled && self.irq_counter > 0 {
            self.irq_counter -= 1;
            if self.irq_counter == 0 {
                self.irq_pending = true;
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
        16
    }

    fn wram_size(&self) -> usize {
        // Mapper 16 does not have traditional PRG-RAM.
        // Save data is stored in EEPROM (not yet implemented).
        0
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        // No WRAM to snapshot - EEPROM save data is separate
        Vec::new()
    }

    fn load_wram_snapshot(&mut self, _data: &[u8]) {
        // No WRAM to restore - EEPROM save data is separate
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Serialize Bandai FCG internal registers:
        // [0]: prg_bank
        // [1-8]: chr_banks[0-7]
        // [9]: flags (irq_enabled, irq_pending)
        // [10-11]: irq_counter (little endian)
        // [12-13]: irq_latch (little endian)
        // [14]: mirroring
        let mut snapshot = Vec::with_capacity(15);
        snapshot.push(self.prg_bank);
        snapshot.extend_from_slice(&self.chr_banks);
        let flags = (self.irq_enabled as u8) | ((self.irq_pending as u8) << 1);
        snapshot.push(flags);
        snapshot.push((self.irq_counter & 0xFF) as u8);
        snapshot.push((self.irq_counter >> 8) as u8);
        snapshot.push((self.irq_latch & 0xFF) as u8);
        snapshot.push((self.irq_latch >> 8) as u8);
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
        if data.len() >= 15 {
            self.prg_bank = data[0];
            self.chr_banks.copy_from_slice(&data[1..9]);
            let flags = data[9];
            self.irq_enabled = (flags & 1) != 0;
            self.irq_pending = (flags & 2) != 0;
            self.irq_counter = (data[10] as u16) | ((data[11] as u16) << 8);
            self.irq_latch = (data[12] as u16) | ((data[13] as u16) << 8);
            self.mirroring = match data[14] {
                0 => MirroringMode::Horizontal,
                1 => MirroringMode::Vertical,
                2 => MirroringMode::SingleScreenLower,
                3 => MirroringMode::SingleScreenUpper,
                4 => MirroringMode::FourScreen,
                _ => MirroringMode::Horizontal,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    #[test]
    fn test_mapper_16_is_wired_in_factory() {
        let prg_rom = banked_data(16 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        let mapper = create_mapper(MapperContext::new(
            16,
            prg_rom,
            chr_rom,
            MirroringMode::Horizontal,
        ));
        assert!(mapper.is_ok(), "Mapper 16 should be implemented");
    }

    #[test]
    fn test_prg_banking_switchable_and_fixed() {
        let prg_rom = banked_data(16 * 1024, 4); // 4 x 16KB banks
        let chr_rom = banked_data(1024, 8);

        let mut mapper = BandaiFcgMapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Initially bank 0 at $8000, last bank (3) at $C000
        assert_eq!(mapper.read_prg(0x8000), 0, "Bank 0 at $8000");
        assert_eq!(mapper.read_prg(0xC000), 3, "Last bank at $C000");

        // Switch to bank 2 at $8000
        mapper.write_prg(0x8008, 2);
        assert_eq!(mapper.read_prg(0x8000), 2, "Bank 2 at $8000 after switch");
        assert_eq!(mapper.read_prg(0xC000), 3, "Last bank still at $C000");
    }

    #[test]
    fn test_chr_banking_8x1kb() {
        let prg_rom = banked_data(16 * 1024, 2);
        let chr_rom = banked_data(1024, 16); // 16 x 1KB banks

        let mut mapper = BandaiFcgMapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Set different banks for each 1KB slot
        for i in 0..8 {
            mapper.write_prg(0x8000 + i as u16, i as u8 + 1);
        }

        // Verify each slot reads from correct bank
        for i in 0..8 {
            let addr = (i as u16) * 0x400; // Start of each 1KB slot
            assert_eq!(
                mapper.read_chr(addr),
                (i + 1) as u8,
                "CHR slot {} should read from bank {}",
                i,
                i + 1
            );
        }
    }

    #[test]
    fn test_mirroring_control() {
        let prg_rom = banked_data(16 * 1024, 2);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = BandaiFcgMapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Test all mirroring modes
        mapper.write_prg(0x8009, 0);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Vertical);

        mapper.write_prg(0x8009, 1);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal);

        mapper.write_prg(0x8009, 2);
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenLower);

        mapper.write_prg(0x8009, 3);
        assert_eq!(mapper.get_mirroring(), MirroringMode::SingleScreenUpper);
    }

    #[test]
    fn test_irq_counter_triggers_at_zero() {
        let prg_rom = banked_data(16 * 1024, 2);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = BandaiFcgMapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Set latch to 3
        mapper.write_prg(0x800B, 3); // Low byte
        mapper.write_prg(0x800C, 0); // High byte

        // Enable IRQ (copies latch to counter)
        mapper.write_prg(0x800A, 1);
        assert!(!mapper.irq_pending(), "IRQ should not be pending yet");

        // Clock 3 cycles - counter goes 3 -> 2 -> 1 -> 0
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending());
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending());
        mapper.cpu_cycle();
        assert!(
            mapper.irq_pending(),
            "IRQ should trigger when counter reaches 0"
        );
    }

    #[test]
    fn test_irq_acknowledge_clears_pending() {
        let prg_rom = banked_data(16 * 1024, 2);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = BandaiFcgMapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Trigger an IRQ
        mapper.write_prg(0x800B, 1);
        mapper.write_prg(0x800C, 0);
        mapper.write_prg(0x800A, 1); // Enable
        mapper.cpu_cycle(); // Counter 1 -> 0, triggers IRQ

        assert!(mapper.irq_pending());

        // Writing to $800A acknowledges IRQ
        mapper.write_prg(0x800A, 0);
        assert!(!mapper.irq_pending(), "IRQ should be acknowledged");
    }

    #[test]
    fn test_irq_immediate_if_enabled_with_zero_counter() {
        let prg_rom = banked_data(16 * 1024, 2);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = BandaiFcgMapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);

        // Set latch to 0
        mapper.write_prg(0x800B, 0);
        mapper.write_prg(0x800C, 0);

        // Enable IRQ - should trigger immediately since counter will be 0
        mapper.write_prg(0x800A, 1);
        assert!(
            mapper.irq_pending(),
            "IRQ should trigger immediately when enabled with 0 counter"
        );
    }

    // =====================================================
    // Submapper 4 (FCG-1/2) tests
    // =====================================================

    #[test]
    fn test_fcg1_2_registers_at_6000() {
        let prg_rom = banked_data(16 * 1024, 4);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = BandaiFcgMapper::new_with_variant(
            prg_rom,
            chr_rom,
            MirroringMode::Horizontal,
            BandaiFcgVariant::Fcg1_2,
        );

        // PRG bank switch via $6008
        mapper.write_prg(0x6008, 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "PRG bank should switch via $6008"
        );

        // CHR bank switch via $6000-$6007
        mapper.write_prg(0x6000, 5);
        assert_eq!(
            mapper.read_chr(0x0000),
            5,
            "CHR bank 0 should switch via $6000"
        );

        // Mirroring via $6009
        mapper.write_prg(0x6009, 0);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Vertical);
    }

    #[test]
    fn test_fcg1_2_ignores_8000_writes() {
        let prg_rom = banked_data(16 * 1024, 4);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = BandaiFcgMapper::new_with_variant(
            prg_rom,
            chr_rom,
            MirroringMode::Horizontal,
            BandaiFcgVariant::Fcg1_2,
        );

        // Writing to $8000 should have no effect on FCG-1/2
        mapper.write_prg(0x8008, 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "FCG-1/2 should ignore writes to $8000 range"
        );
    }

    #[test]
    fn test_fcg1_2_direct_irq_counter() {
        let prg_rom = banked_data(16 * 1024, 2);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = BandaiFcgMapper::new_with_variant(
            prg_rom,
            chr_rom,
            MirroringMode::Horizontal,
            BandaiFcgVariant::Fcg1_2,
        );

        // FCG-1/2 writes directly to counter (not latch)
        mapper.write_prg(0x600B, 3); // Counter low byte = 3
        mapper.write_prg(0x600C, 0); // Counter high byte = 0

        // Enable IRQ - does NOT copy latch to counter on FCG-1/2
        mapper.write_prg(0x600A, 1);
        assert!(!mapper.irq_pending(), "IRQ should not be pending yet");

        // Counter should already be 3 from direct writes
        // Clock 3 cycles - counter goes 3 -> 2 -> 1 -> 0
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending());
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending());
        mapper.cpu_cycle();
        assert!(
            mapper.irq_pending(),
            "IRQ should trigger when counter reaches 0"
        );
    }

    #[test]
    fn test_lz93d50_ignores_6000_writes() {
        let prg_rom = banked_data(16 * 1024, 4);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = BandaiFcgMapper::new_with_variant(
            prg_rom,
            chr_rom,
            MirroringMode::Horizontal,
            BandaiFcgVariant::Lz93d50,
        );

        // Writing to $6000 should have no effect on LZ93D50
        mapper.write_prg(0x6008, 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "LZ93D50 should ignore writes to $6000 range"
        );

        // But $8000 should work
        mapper.write_prg(0x8008, 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "LZ93D50 should accept $8000 writes"
        );
    }

    // =====================================================
    // Submapper 0 (Both) tests - responds to both ranges
    // =====================================================

    #[test]
    fn test_both_variant_accepts_6000_and_8000_writes() {
        let prg_rom = banked_data(16 * 1024, 4);
        let chr_rom = banked_data(1024, 16);

        // Default constructor uses Both variant
        let mut mapper = BandaiFcgMapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);

        // $6000 range should work
        mapper.write_prg(0x6008, 1);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "Both should accept $6000 writes"
        );

        // $8000 range should also work
        mapper.write_prg(0x8008, 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "Both should accept $8000 writes"
        );
    }

    #[test]
    fn test_both_variant_uses_correct_irq_behavior_per_range() {
        let prg_rom = banked_data(16 * 1024, 2);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = BandaiFcgMapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);

        // $6000 range uses FCG-1/2 behavior (direct counter writes)
        mapper.write_prg(0x600B, 5); // Direct counter low = 5
        mapper.write_prg(0x600C, 0); // Direct counter high = 0
        mapper.write_prg(0x600A, 1); // Enable - no latch copy

        // Counter should be 5, tick down
        for _ in 0..4 {
            assert!(!mapper.irq_pending());
            mapper.cpu_cycle();
        }
        mapper.cpu_cycle(); // 5th cycle should trigger IRQ
        assert!(mapper.irq_pending(), "IRQ should trigger after 5 cycles");

        // Acknowledge IRQ
        mapper.write_prg(0x600A, 0);
        assert!(!mapper.irq_pending());

        // Now test $8000 range uses LZ93D50 behavior (latched counter)
        mapper.write_prg(0x800B, 3); // Latch low = 3
        mapper.write_prg(0x800C, 0); // Latch high = 0
        // Counter is currently 0 from previous test
        mapper.write_prg(0x800A, 1); // Enable - copies latch to counter

        // Counter should now be 3 (copied from latch)
        for _ in 0..2 {
            assert!(!mapper.irq_pending());
            mapper.cpu_cycle();
        }
        mapper.cpu_cycle(); // 3rd cycle should trigger IRQ
        assert!(
            mapper.irq_pending(),
            "IRQ should trigger after 3 cycles (latch behavior)"
        );
    }

    #[test]
    fn test_bandai_fcg_registers_snapshot_restores_state() {
        let prg_rom = banked_data(16 * 1024, 4);
        let chr_rom = vec![]; // CHR-RAM path

        let mut mapper = BandaiFcgMapper::new(prg_rom.clone(), chr_rom, MirroringMode::Horizontal);

        mapper.write_prg(0x8008, 2); // PRG bank
        mapper.write_prg(0x8000, 3); // CHR bank 0
        mapper.write_prg(0x8009, 3); // mirroring upper

        mapper.write_chr(0x0000, 0xAB);

        mapper.write_prg(0x800B, 0x34);
        mapper.write_prg(0x800C, 0x12);
        mapper.write_prg(0x800A, 1);

        let regs = mapper.registers_snapshot();
        let chr = mapper.chr_ram_snapshot();

        let mut restored = BandaiFcgMapper::new(prg_rom, vec![], MirroringMode::Vertical);
        restored.restore_registers(&regs);
        restored.restore_chr_ram(&chr);

        assert_eq!(restored.read_prg(0x8000), 2);
        assert_eq!(restored.read_chr(0x0000), 0xAB);
        assert_eq!(restored.get_mirroring(), MirroringMode::SingleScreenUpper);
        assert_eq!(restored.irq_pending(), mapper.irq_pending());
    }

    #[test]
    fn test_bandai_fcg_banked_rom_replacement() {
        use crate::cartridge::common::BankedRom;
        use crate::cartridge::test_helpers::banked_data;

        const PRG_BANK_SIZE: usize = 16 * 1024; // 16KB
        const CHR_BANK_SIZE: usize = 1024; // 1KB

        let prg_rom = banked_data(PRG_BANK_SIZE, 16);
        let chr_rom = banked_data(CHR_BANK_SIZE, 128);

        let prg_banked = BankedRom::new(prg_rom, PRG_BANK_SIZE);
        let chr_banked = BankedRom::new(chr_rom, CHR_BANK_SIZE);

        // Test PRG banks
        assert_eq!(prg_banked.read(0, 0), 0);
        assert_eq!(prg_banked.read(15, 0), 15);

        // Test CHR banks
        assert_eq!(chr_banked.read(0, 0), 0);
        assert_eq!(chr_banked.read(127, 0), 127);

        // Test wrapping
        assert_eq!(prg_banked.read(16, 0), 0);
        assert_eq!(chr_banked.read(128, 0), 0);
    }
}
