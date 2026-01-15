//! # Mapper 16 (Bandai FCG) Implementation
//!
//! Used by Dragon Ball, SD Gundam, and other Bandai games.
//!
//! ## Banks
//! - PRG: 16KB switchable at $8000-$BFFF, fixed last bank at $C000-$FFFF
//! - CHR: 8 × 1KB switchable banks
//!
//! ## Submappers
//! - **Submapper 4 (FCG-1/2)**: Registers at $6000-$7FFF, direct IRQ counter writes
//! - **Submapper 5 (LZ93D50)**: Registers at $8000-$800F, latched IRQ counter
//!
//! ## Registers
//! - $x000-$x007: CHR bank select (1KB each)
//! - $x008: PRG bank select (16KB at $8000-$BFFF)
//! - $x009: Mirroring (0=V, 1=H, 2=1A, 3=1B)
//! - $x00A: IRQ control (bit 0 = enable)
//! - $x00B: IRQ counter/latch low byte
//! - $x00C: IRQ counter/latch high byte
//!
//! Where x = 6 for submapper 4, x = 8 for submapper 5.
//!
//! ## References
//! - <https://www.nesdev.org/wiki/INES_Mapper_016>

use crate::cartridge::cartridge::MirroringMode;
use crate::cartridge::mapper::Mapper;

/// Submapper variants for Bandai FCG
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
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    chr_ram: Vec<u8>,
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
        let chr_ram = if chr_rom.is_empty() {
            vec![0u8; 8 * 1024]
        } else {
            Vec::new()
        };

        Self {
            prg_rom,
            chr_rom,
            chr_ram,
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
        self.prg_rom.len() / Self::PRG_BANK_SIZE
    }

    fn last_prg_bank(&self) -> usize {
        self.prg_bank_count().saturating_sub(1)
    }

    fn chr_bank_count(&self) -> usize {
        if !self.chr_rom.is_empty() {
            self.chr_rom.len() / Self::CHR_BANK_SIZE
        } else {
            self.chr_ram.len() / Self::CHR_BANK_SIZE
        }
    }

    fn read_chr_byte(&self, bank: u8, offset: usize) -> u8 {
        let bank_count = self.chr_bank_count();
        if bank_count == 0 {
            return 0;
        }
        let bank_index = (bank as usize) % bank_count;
        let addr = bank_index * Self::CHR_BANK_SIZE + offset;

        if !self.chr_rom.is_empty() {
            self.chr_rom.get(addr).copied().unwrap_or(0)
        } else {
            self.chr_ram.get(addr).copied().unwrap_or(0)
        }
    }

    fn write_chr_byte(&mut self, bank: u8, offset: usize, value: u8) {
        if self.chr_rom.is_empty() {
            let bank_count = self.chr_bank_count();
            if bank_count == 0 {
                return;
            }
            let bank_index = (bank as usize) % bank_count;
            let addr = bank_index * Self::CHR_BANK_SIZE + offset;
            if let Some(slot) = self.chr_ram.get_mut(addr) {
                *slot = value;
            }
        }
    }
}

impl Mapper for BandaiFcgMapper {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => {
                // Switchable 16KB bank
                let bank_count = self.prg_bank_count();
                if bank_count == 0 {
                    return 0;
                }
                let bank_index = (self.prg_bank as usize) % bank_count;
                let offset = (addr - 0x8000) as usize;
                self.prg_rom
                    .get(bank_index * Self::PRG_BANK_SIZE + offset)
                    .copied()
                    .unwrap_or(0)
            }
            0xC000..=0xFFFF => {
                // Fixed last 16KB bank
                let bank_index = self.last_prg_bank();
                let offset = (addr - 0xC000) as usize;
                self.prg_rom
                    .get(bank_index * Self::PRG_BANK_SIZE + offset)
                    .copied()
                    .unwrap_or(0)
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

    fn ppu_address_changed(&mut self, _addr: u16) {
        // Not used for this mapper
    }

    fn cpu_cycle(&mut self) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::create_mapper;

    fn banked_data(bank_size: usize, num_banks: usize) -> Vec<u8> {
        let mut data = vec![0u8; bank_size * num_banks];
        for bank in 0..num_banks {
            for i in 0..bank_size {
                data[bank * bank_size + i] = bank as u8;
            }
        }
        data
    }

    #[test]
    fn test_mapper_16_is_wired_in_factory() {
        let prg_rom = banked_data(16 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        let mapper = create_mapper(16, prg_rom, chr_rom, MirroringMode::Horizontal);
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
        assert_eq!(mapper.read_prg(0x8000), 1, "Both should accept $6000 writes");

        // $8000 range should also work
        mapper.write_prg(0x8008, 2);
        assert_eq!(mapper.read_prg(0x8000), 2, "Both should accept $8000 writes");
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
        assert!(mapper.irq_pending(), "IRQ should trigger after 3 cycles (latch behavior)");
    }
}
