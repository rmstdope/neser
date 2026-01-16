use crate::cartridge::{Mapper, MirroringMode};
use crate::trace_mapper;

/// MMC3 mapper (Mapper 4)
///
/// This implementation focuses on PRG/CHR banking + mirroring control.
/// It also includes basic MMC3 scanline IRQ counter support.
pub struct MMC3Mapper {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    chr_ram: Vec<u8>,
    prg_ram: Vec<u8>,

    prg_ram_enabled: bool,
    prg_ram_write_protected: bool,

    mirroring: MirroringMode,

    bank_select: u8,
    regs: [u8; 8],

    // --- MMC3 scanline IRQ ---
    irq_latch: u8,
    irq_counter: u8,
    irq_reload: bool,
    irq_enabled: bool,
    irq_asserted: bool,

    prev_a12: bool,
    a12_low_cycles: u8,
}

impl MMC3Mapper {
    const PRG_BANK_SIZE: usize = 0x2000; // 8KB
    const CHR_BANK_SIZE: usize = 0x0400; // 1KB
    const PRG_RAM_SIZE: usize = 0x2000; // 8KB
    const DEFAULT_CHR_RAM_SIZE: usize = 0x2000; // 8KB

    const A12_LOW_CYCLES_REQUIRED: u8 = 8;

    const PRG_RAM_ENABLE_MASK: u8 = 0b1000_0000;
    const PRG_RAM_WRITE_PROTECT_MASK: u8 = 0b0100_0000;

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        let chr_ram = if chr_rom.is_empty() {
            vec![0; Self::DEFAULT_CHR_RAM_SIZE]
        } else {
            Vec::new()
        };

        Self {
            prg_rom,
            chr_rom,
            chr_ram,
            prg_ram: vec![0; Self::PRG_RAM_SIZE],
            mirroring,
            prg_ram_enabled: true, // PRG-RAM enabled by default on power-on
            prg_ram_write_protected: false,
            bank_select: 0,
            regs: [0; 8],

            irq_latch: 0,
            irq_counter: 0,
            irq_reload: false,
            irq_enabled: false,
            irq_asserted: false,

            prev_a12: false,
            a12_low_cycles: 0,
        }
    }

    fn prg_bank_count(&self) -> usize {
        self.prg_rom.len() / Self::PRG_BANK_SIZE
    }

    fn chr_bank_count_1k(&self) -> usize {
        let chr_len = if self.chr_rom.is_empty() {
            self.chr_ram.len()
        } else {
            self.chr_rom.len()
        };
        chr_len / Self::CHR_BANK_SIZE
    }

    fn prg_bank_index(&self, bank: u8) -> usize {
        let count = self.prg_bank_count();
        if count == 0 {
            return 0;
        }
        (bank as usize) % count
    }

    fn chr_bank_index_1k(&self, bank: u8) -> usize {
        let count = self.chr_bank_count_1k();
        if count == 0 {
            return 0;
        }
        (bank as usize) % count
    }

    fn prg_mode(&self) -> bool {
        (self.bank_select & 0b0100_0000) != 0
    }

    fn chr_mode(&self) -> bool {
        (self.bank_select & 0b1000_0000) != 0
    }

    fn selected_reg(&self) -> usize {
        (self.bank_select & 0b0000_0111) as usize
    }

    fn read_prg_rom_bank(&self, bank_index: usize, bank_offset: usize) -> u8 {
        let addr = bank_index * Self::PRG_BANK_SIZE + bank_offset;
        self.prg_rom.get(addr).copied().unwrap_or(0)
    }

    fn read_chr_bank_1k(&self, bank_index: usize, bank_offset: usize) -> u8 {
        let addr = bank_index * Self::CHR_BANK_SIZE + bank_offset;
        if self.chr_rom.is_empty() {
            self.chr_ram.get(addr).copied().unwrap_or(0)
        } else {
            self.chr_rom.get(addr).copied().unwrap_or(0)
        }
    }

    fn update_prg_ram_control(&mut self, value: u8) {
        self.prg_ram_enabled = (value & Self::PRG_RAM_ENABLE_MASK) != 0;
        self.prg_ram_write_protected = (value & Self::PRG_RAM_WRITE_PROTECT_MASK) != 0;
    }

    fn a12_rising_edge(&mut self, current_a12: bool) -> bool {
        let rising_edge = !self.prev_a12 && current_a12;
        self.prev_a12 = current_a12;
        rising_edge
    }

    fn track_a12_low_cycles(&mut self, current_a12: bool) {
        if current_a12 {
            self.a12_low_cycles = 0;
        } else {
            self.a12_low_cycles = self.a12_low_cycles.saturating_add(1);
        }
    }

    fn should_clock_irq_on_a12_change(&mut self, addr: u16) -> bool {
        // MMC3 A12 low-pass filter: A12 must be low for at least 8 PPU cycles
        // before a rising edge is allowed to clock the IRQ counter.
        let current_a12 = (addr & 0x1000) != 0;
        let rising_edge = self.a12_rising_edge(current_a12);
        let low_cycles_met = self.a12_low_cycles >= Self::A12_LOW_CYCLES_REQUIRED;
        let should_clock = rising_edge && low_cycles_met;
        
        trace_mapper!(3; "MMC3 A12 check: addr=${:04X}, a12={}, rising_edge={}, low_cycles={}, should_clock={}", 
            addr, current_a12, rising_edge, self.a12_low_cycles, should_clock);
        
        self.track_a12_low_cycles(current_a12);
        should_clock
    }

    fn clock_irq_counter_on_a12_rising_edge(&mut self) {
        // MMC3 IRQ counter behavior (minimal):
        // - On each A12 rising edge, update the counter.
        // - If counter==0 or reload requested: load counter from latch.
        // - Else: decrement counter.
        // - If counter becomes 0 and IRQ is enabled: assert IRQ.
        let old_counter = self.irq_counter;
        let was_reload = self.irq_reload;
        
        if self.irq_counter == 0 || self.irq_reload {
            self.irq_counter = self.irq_latch;
            self.irq_reload = false;
        } else {
            self.irq_counter = self.irq_counter.wrapping_sub(1);
        }

        trace_mapper!(2; "MMC3 IRQ clock: old_counter={}, reload_flag={}, latch={}, new_counter={}, enabled={}", 
            old_counter, was_reload, self.irq_latch, self.irq_counter, self.irq_enabled);

        if self.irq_counter == 0 && self.irq_enabled {
            trace_mapper!(1; "MMC3 IRQ ASSERTED!");
            self.irq_asserted = true;
        }
    }

    fn map_chr_addr_to_bank_1k(&self, chr_addr: usize) -> (usize, usize) {
        let bank_offset = chr_addr & (Self::CHR_BANK_SIZE - 1);

        let r0 = self.regs[0] & 0xFE; // 2KB bank, even-aligned
        let r1 = self.regs[1] & 0xFE; // 2KB bank, even-aligned
        let r2 = self.regs[2];
        let r3 = self.regs[3];
        let r4 = self.regs[4];
        let r5 = self.regs[5];

        let bank_1k = if !self.chr_mode() {
            // CHR mode 0
            match chr_addr {
                0x0000..=0x03FF => r0,
                0x0400..=0x07FF => r0.wrapping_add(1),
                0x0800..=0x0BFF => r1,
                0x0C00..=0x0FFF => r1.wrapping_add(1),
                0x1000..=0x13FF => r2,
                0x1400..=0x17FF => r3,
                0x1800..=0x1BFF => r4,
                0x1C00..=0x1FFF => r5,
                _ => 0,
            }
        } else {
            // CHR mode 1
            match chr_addr {
                0x0000..=0x03FF => r2,
                0x0400..=0x07FF => r3,
                0x0800..=0x0BFF => r4,
                0x0C00..=0x0FFF => r5,
                0x1000..=0x13FF => r0,
                0x1400..=0x17FF => r0.wrapping_add(1),
                0x1800..=0x1BFF => r1,
                0x1C00..=0x1FFF => r1.wrapping_add(1),
                _ => 0,
            }
        };

        (self.chr_bank_index_1k(bank_1k), bank_offset)
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::cartridge::MirroringMode;
    use crate::cartridge::mapper::create_mapper;

    fn banked_data(bank_size: usize, num_banks: usize) -> Vec<u8> {
        let mut data = vec![0u8; bank_size * num_banks];
        for bank in 0..num_banks {
            let start = bank * bank_size;
            let end = start + bank_size;
            data[start..end].fill(bank as u8);
        }
        data
    }

    #[test]
    fn test_mmc3_irq_asserts_after_a12_rising_edges() {
        // Minimal MMC3 scanline IRQ spec (A12 rising edge counter):
        // With latch=1, reload requested, and IRQ enabled, the IRQ should assert
        // on the second A12 rising edge.

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1 * 1024, 16);

        let mut mapper = create_mapper(4, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC3 (mapper 4) should be implemented");

        // Set IRQ latch to 1
        mapper.write_prg(0xC000, 1);
        // Request reload
        mapper.write_prg(0xC001, 0);
        // Enable IRQ
        mapper.write_prg(0xE001, 0);

        // First A12 rising edge ($0xxx -> $1xxx). MMC3 requires A12 low for 8 PPU cycles.
        for _ in 0..8 {
            mapper.ppu_address_changed(0x0FFF);
        }
        mapper.ppu_address_changed(0x1000);
        assert_eq!(mapper.irq_pending(), false);

        // Second A12 rising edge
        for _ in 0..8 {
            mapper.ppu_address_changed(0x0FFF);
        }
        mapper.ppu_address_changed(0x1000);
        assert_eq!(mapper.irq_pending(), true);

        // Acknowledge/disable should clear IRQ
        mapper.write_prg(0xE000, 0);
        assert_eq!(mapper.irq_pending(), false);
    }

    #[test]
    fn test_mmc3_irq_a12_rising_edge_requires_8_ppu_cycles_low() {
        // MMC3 has a simple A12 low-pass filter: a rising edge should only clock
        // the IRQ counter if A12 has been low for at least 8 PPU cycles.

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1 * 1024, 16);

        let mut mapper = create_mapper(4, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC3 (mapper 4) should be implemented");

        mapper.write_prg(0xC000, 1); // latch=1
        mapper.write_prg(0xC001, 0); // reload
        mapper.write_prg(0xE001, 0); // enable

        // A12 low for 1 PPU cycle, then rising edge: should be ignored.
        mapper.ppu_address_changed(0x0FFF);
        mapper.ppu_address_changed(0x1000);
        assert_eq!(mapper.irq_pending(), false);

        // Now hold A12 low for 8 PPU cycles, then rising edge: this clocks once.
        for _ in 0..8 {
            mapper.ppu_address_changed(0x0FFF);
        }
        mapper.ppu_address_changed(0x1000);
        assert_eq!(mapper.irq_pending(), false);

        // Another valid edge after 8 low cycles: second clock should assert IRQ.
        for _ in 0..8 {
            mapper.ppu_address_changed(0x0FFF);
        }
        mapper.ppu_address_changed(0x1000);
        assert_eq!(mapper.irq_pending(), true);
    }

    #[test]
    fn test_mmc3_prg_bank_switching_modes() {
        // MMC3 PRG banking (no IRQ): four 8KB CPU banks at $8000-$FFFF.
        // - PRG mode 0: $8000 = R6 (switch), $A000 = R7 (switch), $C000 = fixed second-last, $E000 = fixed last
        // - PRG mode 1: $8000 = fixed second-last, $A000 = R7 (switch), $C000 = R6 (switch), $E000 = fixed last

        let prg_rom = banked_data(8 * 1024, 8); // 8 x 8KB banks; last=7, second-last=6
        let chr_rom = banked_data(1 * 1024, 16); // Enough to satisfy mapper creation; not used in this test

        let mut mapper = create_mapper(4, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC3 (mapper 4) should be implemented");

        // PRG mode 0, set R6=$8000 bank 2, R7=$A000 bank 3
        mapper.write_prg(0x8000, 0b0000_0110); // bank select: register 6, PRG mode 0, CHR mode 0
        mapper.write_prg(0x8001, 2); // R6 = 2

        mapper.write_prg(0x8000, 0b0000_0111); // bank select: register 7
        mapper.write_prg(0x8001, 3); // R7 = 3

        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xA000), 3);
        assert_eq!(mapper.read_prg(0xC000), 6);
        assert_eq!(mapper.read_prg(0xE000), 7);

        // Switch to PRG mode 1, set R6=$C000 bank 1
        mapper.write_prg(0x8000, 0b0100_0110); // bank select: register 6, PRG mode 1
        mapper.write_prg(0x8001, 1); // R6 = 1

        assert_eq!(mapper.read_prg(0x8000), 6);
        assert_eq!(mapper.read_prg(0xA000), 3);
        assert_eq!(mapper.read_prg(0xC000), 1);
        assert_eq!(mapper.read_prg(0xE000), 7);
    }

    #[test]
    fn test_mmc3_chr_bank_switching_mode0() {
        // MMC3 CHR banking (CHR mode 0):
        // - R0: 2KB bank @ $0000 (even-aligned)
        // - R1: 2KB bank @ $0800 (even-aligned)
        // - R2..R5: 1KB banks @ $1000, $1400, $1800, $1C00

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1 * 1024, 16); // 16 x 1KB banks

        let mut mapper = create_mapper(4, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC3 (mapper 4) should be implemented");

        // Ensure CHR mode 0.
        mapper.write_prg(0x8000, 0b0000_0000); // bank select: register 0, CHR mode 0

        // R0 = 5 -> should map banks 4 and 5 at $0000-$07FF
        mapper.write_prg(0x8000, 0b0000_0000); // R0
        mapper.write_prg(0x8001, 5);
        assert_eq!(mapper.read_chr(0x0000), 4);
        assert_eq!(mapper.read_chr(0x0400), 5);

        // R1 = 2 -> should map banks 2 and 3 at $0800-$0FFF
        mapper.write_prg(0x8000, 0b0000_0001); // R1
        mapper.write_prg(0x8001, 2);
        assert_eq!(mapper.read_chr(0x0800), 2);
        assert_eq!(mapper.read_chr(0x0C00), 3);

        // R2..R5: 1KB banks
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 7);
        mapper.write_prg(0x8000, 0b0000_0011); // R3
        mapper.write_prg(0x8001, 8);
        mapper.write_prg(0x8000, 0b0000_0100); // R4
        mapper.write_prg(0x8001, 9);
        mapper.write_prg(0x8000, 0b0000_0101); // R5
        mapper.write_prg(0x8001, 10);

        assert_eq!(mapper.read_chr(0x1000), 7);
        assert_eq!(mapper.read_chr(0x1400), 8);
        assert_eq!(mapper.read_chr(0x1800), 9);
        assert_eq!(mapper.read_chr(0x1C00), 10);
    }

    #[test]
    fn test_mmc3_prg_ram_enable_and_write_protect() {
        // MMC3 PRG-RAM control ($A001, odd address):
        // - bit 7: PRG-RAM enable (0 = disabled)
        // - bit 6: PRG-RAM write protect (1 = write-protected)
        // PRG-RAM is enabled by default on power-on (hardware behavior).

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1 * 1024, 16);

        let mut mapper = create_mapper(4, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC3 (mapper 4) should be implemented");

        // Default: PRG-RAM enabled (can read and write)
        mapper.write_prg(0x6000, 0xAA);
        assert_eq!(mapper.read_prg(0x6000), 0xAA);

        // Disable PRG-RAM: reads return 0, writes are ignored
        mapper.write_prg(0xA001, 0b0000_0000);
        mapper.write_prg(0x6000, 0xBB);
        assert_eq!(mapper.read_prg(0x6000), 0x00);

        // Enable PRG-RAM and allow writes
        mapper.write_prg(0xA001, 0b1000_0000);
        mapper.write_prg(0x6000, 0xCC);
        assert_eq!(mapper.read_prg(0x6000), 0xCC);

        // Enable + write-protect (writes ignored, reads still work)
        mapper.write_prg(0xA001, 0b1100_0000);
        mapper.write_prg(0x6000, 0xDD);
        assert_eq!(mapper.read_prg(0x6000), 0xCC);
    }

    #[test]
    fn test_mmc3_chr_bank_switching_mode1() {
        // MMC3 CHR banking (CHR mode 1):
        // - R2..R5: 1KB banks @ $0000, $0400, $0800, $0C00
        // - R0: 2KB bank @ $1000 (even-aligned)
        // - R1: 2KB bank @ $1800 (even-aligned)

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1 * 1024, 16); // 16 x 1KB banks

        let mut mapper = create_mapper(4, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC3 (mapper 4) should be implemented");

        // Enable CHR mode 1 (bit7=1) and program registers.
        mapper.write_prg(0x8000, 0b1000_0000 | 0); // R0, CHR mode 1
        mapper.write_prg(0x8001, 5); // R0 -> banks 4+5 at $1000-$17FF

        mapper.write_prg(0x8000, 0b1000_0000 | 1); // R1
        mapper.write_prg(0x8001, 2); // R1 -> banks 2+3 at $1800-$1FFF

        mapper.write_prg(0x8000, 0b1000_0000 | 2); // R2
        mapper.write_prg(0x8001, 9);
        mapper.write_prg(0x8000, 0b1000_0000 | 3); // R3
        mapper.write_prg(0x8001, 10);
        mapper.write_prg(0x8000, 0b1000_0000 | 4); // R4
        mapper.write_prg(0x8001, 11);
        mapper.write_prg(0x8000, 0b1000_0000 | 5); // R5
        mapper.write_prg(0x8001, 12);

        // R2..R5 mapping at $0000-$0FFF
        assert_eq!(mapper.read_chr(0x0000), 9);
        assert_eq!(mapper.read_chr(0x0400), 10);
        assert_eq!(mapper.read_chr(0x0800), 11);
        assert_eq!(mapper.read_chr(0x0C00), 12);

        // R0 2KB mapping at $1000-$17FF (even aligned: 4 then 5)
        assert_eq!(mapper.read_chr(0x1000), 4);
        assert_eq!(mapper.read_chr(0x1400), 5);

        // R1 2KB mapping at $1800-$1FFF (even aligned: 2 then 3)
        assert_eq!(mapper.read_chr(0x1800), 2);
        assert_eq!(mapper.read_chr(0x1C00), 3);
    }

    #[test]
    fn test_mmc3_mirroring_control_via_a000() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1 * 1024, 16);

        let mut mapper = create_mapper(4, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC3 (mapper 4) should be implemented");

        // Starts with the cartridge-provided mirroring.
        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal);

        // $A000 even: mirroring control (bit 0)
        // 0 => Vertical
        mapper.write_prg(0xA000, 0);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Vertical);

        // $A001 odd: PRG-RAM protect; must not affect mirroring
        mapper.write_prg(0xA001, 1);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Vertical);

        // 1 => Horizontal
        mapper.write_prg(0xA000, 1);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal);
    }

    #[test]
    fn test_mmc3_chr_ram_writes_are_banked() {
        // If the cartridge has CHR-RAM (no CHR-ROM), writes should go to the currently mapped CHR bank.
        // Switching the bank should change what is visible at the same PPU address.

        let prg_rom = banked_data(8 * 1024, 8);
        let mut mapper = create_mapper(4, prg_rom, vec![], MirroringMode::Horizontal)
            .expect("MMC3 (mapper 4) should be implemented");

        // CHR mode 0: $1000-$13FF uses R2 (1KB bank).
        mapper.write_prg(0x8000, 0b0000_0010); // select R2
        mapper.write_prg(0x8001, 1); // map bank 1 at $1000

        mapper.write_chr(0x1000, 0xAA);
        assert_eq!(mapper.read_chr(0x1000), 0xAA);
        // Switch to bank 2: should not see the value written into bank 1.
        mapper.write_prg(0x8000, 0b0000_0010); // select R2
        mapper.write_prg(0x8001, 2); // map bank 2 at $1000
        assert_eq!(mapper.read_chr(0x1000), 0x00);

        // Switch back to bank 1: value should still be there.
        mapper.write_prg(0x8000, 0b0000_0010); // select R2
        mapper.write_prg(0x8001, 1); // map bank 1 at $1000
        assert_eq!(mapper.read_chr(0x1000), 0xAA);
    }

    #[test]
    fn test_mmc3_chr_r1_even_aligned_when_odd_bank_written() {
        // R1 controls a 2KB CHR bank and must be even-aligned (odd values map the previous even bank).
        // In CHR mode 0, R1 maps to $0800-$0FFF (two 1KB banks).

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1 * 1024, 16);

        let mut mapper = create_mapper(4, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC3 (mapper 4) should be implemented");

        // Ensure CHR mode 0 and select R1.
        mapper.write_prg(0x8000, 0b0000_0001); // R1, CHR mode 0
        mapper.write_prg(0x8001, 7); // odd; should map banks 6 and 7

        assert_eq!(mapper.read_chr(0x0800), 6);
        assert_eq!(mapper.read_chr(0x0C00), 7);
    }

    #[test]
    fn test_mmc3_two_bank_prg_rom() {
        // Test MMC3 with only 2 x 8KB PRG banks (like the Blargg MMC3 test ROMs)
        // This is the minimum configuration and should work correctly.
        
        let mut prg_rom = vec![0u8; 16 * 1024]; // 16KB = 2 x 8KB banks
        // Fill bank 0 with 0xAA
        prg_rom[0..8192].fill(0xAA);
        // Fill bank 1 with 0xBB
        prg_rom[8192..16384].fill(0xBB);
        
        let chr_rom = vec![];  // No CHR-ROM (uses CHR-RAM)
        
        let mapper = create_mapper(4, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC3 (mapper 4) should be implemented");
        
        // With 2 banks and default configuration (PRG mode 0):
        // $8000-$9FFF: R6 (bank 0) = 0xAA
        // $A000-$BFFF: R7 (bank 0) = 0xAA  
        // $C000-$DFFF: fixed second-to-last (bank 0) = 0xAA
        // $E000-$FFFF: fixed last (bank 1) = 0xBB
        
        assert_eq!(mapper.read_prg(0x8000), 0xAA);
        assert_eq!(mapper.read_prg(0xA000), 0xAA);
        assert_eq!(mapper.read_prg(0xC000), 0xAA);
        assert_eq!(mapper.read_prg(0xE000), 0xBB);
        
        // The reset vector at $FFFC-$FFFD should be readable from bank 1
        assert_eq!(mapper.read_prg(0xFFFC), 0xBB);
        assert_eq!(mapper.read_prg(0xFFFD), 0xBB);
    }
}

impl Mapper for MMC3Mapper {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                if !self.prg_ram_enabled {
                    return 0;
                }
                let offset = (addr - 0x6000) as usize;
                self.prg_ram.get(offset).copied().unwrap_or(0)
            }
            0x8000..=0xFFFF => {
                let prg_count = self.prg_bank_count();
                if prg_count == 0 {
                    return 0;
                }

                let bank_offset = (addr as usize) & (Self::PRG_BANK_SIZE - 1);

                let fixed_last = prg_count.saturating_sub(1);
                let fixed_second_last = prg_count.saturating_sub(2);

                // Registers R6 and R7 are PRG 8KB bank selectors.
                let r6 = self.prg_bank_index(self.regs[6]);
                let r7 = self.prg_bank_index(self.regs[7]);

                let bank_index = match addr {
                    0x8000..=0x9FFF => {
                        if self.prg_mode() {
                            fixed_second_last
                        } else {
                            r6
                        }
                    }
                    0xA000..=0xBFFF => r7,
                    0xC000..=0xDFFF => {
                        if self.prg_mode() {
                            r6
                        } else {
                            fixed_second_last
                        }
                    }
                    0xE000..=0xFFFF => fixed_last,
                    _ => 0,
                };

                self.read_prg_rom_bank(bank_index, bank_offset)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                if !self.prg_ram_enabled || self.prg_ram_write_protected {
                    return;
                }
                let offset = (addr - 0x6000) as usize;
                if let Some(byte) = self.prg_ram.get_mut(offset) {
                    *byte = value;
                }
            }
            0x8000..=0x9FFF => {
                if (addr & 1) == 0 {
                    // Bank select
                    trace_mapper!(1; "MMC3 bank_select=${:02X}", value);
                    self.bank_select = value;
                } else {
                    // Bank data
                    let reg = self.selected_reg();
                    trace_mapper!(1; "MMC3 reg[{}]=${:02X}", reg, value);
                    self.regs[reg] = value;
                }
            }
            0xA000..=0xBFFF => {
                if (addr & 1) == 0 {
                    // Mirroring
                    // MMC3: bit0 selects mirroring.
                    let new_mirroring = if (value & 1) == 0 {
                        MirroringMode::Vertical
                    } else {
                        MirroringMode::Horizontal
                    };
                    trace_mapper!(1; "MMC3 mirroring={:?}", new_mirroring);
                    self.mirroring = new_mirroring;
                } else {
                    // PRG RAM protect
                    // - bit 7: PRG-RAM enable
                    // - bit 6: PRG-RAM write protect
                    self.update_prg_ram_control(value);
                }
            }
            0xC000..=0xDFFF => {
                if (addr & 1) == 0 {
                    // IRQ latch
                    trace_mapper!(1; "MMC3 IRQ_latch=${:02X}", value);
                    self.irq_latch = value;
                } else {
                    // IRQ reload
                    trace_mapper!(1; "MMC3 IRQ_reload");
                    self.irq_reload = true;
                }
            }
            0xE000..=0xFFFF => {
                if (addr & 1) == 0 {
                    // IRQ disable + acknowledge
                    trace_mapper!(1; "MMC3 IRQ_disable");
                    self.irq_enabled = false;
                    self.irq_asserted = false;
                } else {
                    // IRQ enable
                    trace_mapper!(1; "MMC3 IRQ_enable");
                    self.irq_enabled = true;
                }
            }
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        // MMC3 uses CHR banking in 1KB units (with two 2KB banks depending on CHR mode).
        let chr_addr = (addr & 0x1FFF) as usize;
        let (bank_index, bank_offset) = self.map_chr_addr_to_bank_1k(chr_addr);
        self.read_chr_bank_1k(bank_index, bank_offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if !self.chr_rom.is_empty() {
            return;
        }

        // CHR-RAM writes must respect the same bank mapping as reads.
        let chr_addr = (addr & 0x1FFF) as usize;
        let (bank_index, bank_offset) = self.map_chr_addr_to_bank_1k(chr_addr);
        let mapped_addr = bank_index * Self::CHR_BANK_SIZE + bank_offset;
        if let Some(byte) = self.chr_ram.get_mut(mapped_addr) {
            *byte = value;
        }
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        if self.should_clock_irq_on_a12_change(addr) {
            self.clock_irq_counter_on_a12_rising_edge();
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_asserted
    }

    fn get_mirroring(&self) -> MirroringMode {
        self.mirroring
    }

    fn wram_size(&self) -> usize {
        self.prg_ram.len()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        // Return a direct copy of PRG-RAM, bypassing enable/protect state
        self.prg_ram.clone()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        // Write directly to PRG-RAM, bypassing enable/protect state
        let to_copy = data.len().min(self.prg_ram.len());
        self.prg_ram[..to_copy].copy_from_slice(&data[..to_copy]);
    }
}
