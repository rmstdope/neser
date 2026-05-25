use super::cartridge::GbCartridge;

const HEADER_LOGO_LEN: usize = 48;
const CANONICAL_HEADER_LOGO_CRC32: u32 = 0x4619_5417;

/// Return true if `rom` is an MBC1M multi-game compilation cart.
///
/// Detection criteria (from Pan Docs):
/// - ROM is exactly 1 MiB (64 × 16 KiB banks).
/// - Bank $10 contains a valid Nintendo logo at the standard header offset ($0104).
fn is_multicart_rom(rom: &[u8]) -> bool {
    if rom.len() != 64 * 0x4000 {
        return false;
    }
    let logo_offset = 0x10 * 0x4000 + 0x104;
    rom.get(logo_offset..logo_offset + HEADER_LOGO_LEN)
        .is_some_and(|slice| crate::platform::crc32::crc32(&[slice]) == CANONICAL_HEADER_LOGO_CRC32)
}

/// MBC1 cartridge (types 0x01 = MBC1, 0x02 = MBC1+RAM, 0x03 = MBC1+RAM+BATTERY).
///
/// Supports up to 2 MB ROM (128 banks × 16 KB) and 32 KB RAM (4 banks × 8 KB).
///
/// MBC1M variant: 1 MiB multi-game compilation carts detected at construction by
/// a Nintendo logo in bank $10. In MBC1M mode the secondary register applies to
/// bits 4-5 of the ROM bank number (not 5-6), effectively giving each sub-game
/// its own 256 KiB region.
///
/// Registers (write-only, mapped to ROM area):
/// - $0000–$1FFF: RAM enable (write 0x0A to enable; any other value disables)
/// - $2000–$3FFF: ROM bank number (lower 5 bits)
/// - $4000–$5FFF: secondary bank register (2 bits; upper ROM bits or RAM bank)
/// - $6000–$7FFF: banking mode (0 = mode 0, 1 = mode 1)
pub struct Mbc1 {
    rom: Vec<u8>,
    ram: Vec<u8>,
    /// Lower 5 bits of the ROM bank register (raw; 0 is corrected to 1 on access).
    rom_bank: u8,
    /// 2-bit secondary bank register (upper ROM bits in mode 0; RAM bank in mode 1).
    secondary_bank: u8,
    /// Banking mode: false = mode 0, true = mode 1.
    mode: bool,
    /// Whether cartridge RAM is enabled.
    ram_enabled: bool,
    /// True when the ROM is an MBC1M multi-game compilation cart.
    multicart: bool,
    /// True when the cartridge has battery-backed RAM.
    battery: bool,
}

impl Mbc1 {
    pub fn new(rom: Vec<u8>, ram: Vec<u8>, battery: bool) -> Self {
        let multicart = is_multicart_rom(&rom);
        Self {
            multicart,
            rom,
            ram,
            rom_bank: 0,
            secondary_bank: 0,
            mode: false,
            ram_enabled: false,
            battery,
        }
    }

    fn rom_bank_count(&self) -> usize {
        // Valid GB ROMs are always a multiple of 16 KB (0x4000).
        // A count of 0 would indicate a malformed ROM; callers guard via load_cartridge.
        (self.rom.len() / 0x4000).max(1)
    }

    /// Effective lower ROM bank number: writes of 0 select bank 1.
    fn effective_rom_bank(&self) -> usize {
        let raw = (self.rom_bank & 0x1F) as usize;
        if raw == 0 { 1 } else { raw }
    }

    /// Full bank index for the $4000–$7FFF window.
    ///
    /// MBC1M: secondary register applies to bits 4-5; top bit of rom_bank ignored.
    /// Standard MBC1: secondary register applies to bits 5-6.
    fn bank1_index(&self) -> usize {
        if self.multicart {
            let upper = (self.secondary_bank as usize & 0x03) << 4;
            (upper | (self.effective_rom_bank() & 0x0F)) & (self.rom_bank_count() - 1)
        } else {
            let upper = (self.secondary_bank as usize & 0x03) << 5;
            (upper | self.effective_rom_bank()) & (self.rom_bank_count() - 1)
        }
    }

    /// Bank index for the $0000–$3FFF window.
    ///
    /// In mode 0: always bank 0.
    /// In mode 1 (standard): `secondary_bank << 5`, masked to rom_bank_count.
    /// In mode 1 (MBC1M): `secondary_bank << 4`, masked to rom_bank_count.
    fn bank0_index(&self) -> usize {
        if self.mode {
            let shift = if self.multicart { 4 } else { 5 };
            ((self.secondary_bank as usize & 0x03) << shift) & (self.rom_bank_count() - 1)
        } else {
            0
        }
    }

    /// Number of 8 KiB RAM banks present in this cartridge.
    fn ram_bank_count(&self) -> usize {
        (self.ram.len() / 0x2000).max(1)
    }

    /// RAM bank index.
    ///
    /// In mode 0: always bank 0.
    /// In mode 1: secondary_bank masked to the actual number of RAM banks.
    fn ram_bank_index(&self) -> usize {
        if self.mode {
            (self.secondary_bank & 0x03) as usize & (self.ram_bank_count() - 1)
        } else {
            0
        }
    }

    fn read_rom(&self, addr: u16) -> u8 {
        let (bank, offset) = if addr < 0x4000 {
            (self.bank0_index(), addr as usize)
        } else {
            (self.bank1_index(), addr as usize - 0x4000)
        };
        let idx = bank * 0x4000 + offset;
        self.rom.get(idx).copied().unwrap_or(0xFF)
    }

    fn read_ram(&self, addr: u16) -> u8 {
        if !self.ram_enabled || self.ram.is_empty() {
            return 0xFF;
        }
        let offset = addr as usize - 0xA000;
        let idx = self.ram_bank_index() * 0x2000 + offset;
        self.ram.get(idx).copied().unwrap_or(0xFF)
    }

    fn write_registers(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.ram_enabled = (val & 0x0F) == 0x0A;
            }
            0x2000..=0x3FFF => {
                self.rom_bank = val & 0x1F;
            }
            0x4000..=0x5FFF => {
                self.secondary_bank = val & 0x03;
            }
            0x6000..=0x7FFF => {
                self.mode = val & 0x01 != 0;
            }
            _ => {}
        }
    }

    fn write_ram(&mut self, addr: u16, val: u8) {
        if !self.ram_enabled || self.ram.is_empty() {
            return;
        }
        let offset = addr as usize - 0xA000;
        let idx = self.ram_bank_index() * 0x2000 + offset;
        if let Some(byte) = self.ram.get_mut(idx) {
            *byte = val;
        }
    }
}

impl GbCartridge for Mbc1 {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.read_rom(addr),
            0xA000..=0xBFFF => self.read_ram(addr),
            _ => 0xFF,
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x7FFF => self.write_registers(addr, val),
            0xA000..=0xBFFF => self.write_ram(addr, val),
            _ => {}
        }
    }

    fn has_battery(&self) -> bool {
        self.battery
    }

    fn ram_snapshot(&self) -> Vec<u8> {
        self.ram.clone()
    }

    fn restore_ram(&mut self, data: &[u8]) {
        let len = data.len().min(self.ram.len());
        self.ram[..len].copy_from_slice(&data[..len]);
    }

    fn mbc_state_snapshot(&self) -> Vec<u8> {
        vec![
            self.rom_bank,
            self.secondary_bank,
            self.mode as u8,
            self.ram_enabled as u8,
            self.multicart as u8,
        ]
    }

    fn restore_mbc_state(&mut self, data: &[u8]) {
        if data.len() >= 5 {
            self.rom_bank = data[0];
            self.secondary_bank = data[1];
            self.mode = data[2] != 0;
            self.ram_enabled = data[3] != 0;
            self.multicart = data[4] != 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an N-bank ROM where bank K is uniformly filled with `K as u8`.
    fn make_mbc1_rom(bank_count: usize) -> Vec<u8> {
        let mut rom = vec![0u8; bank_count * 0x4000];
        for bank in 0..bank_count {
            let start = bank * 0x4000;
            let end = start + 0x4000;
            rom[start..end].fill(bank as u8);
        }
        rom
    }

    fn make_mbc1_ram(bank_count: usize) -> Vec<u8> {
        vec![0u8; bank_count * 0x2000]
    }

    #[test]
    fn test_mbc1_reads_bank0_data_from_low_region_initially() {
        // Given: 4-bank ROM (banks 0–3 filled with 0x00–0x03)
        let cart = Mbc1::new(make_mbc1_rom(4), make_mbc1_ram(0), false);
        // When: reading from the $0000–$3FFF window (bank 0)
        // Then: returns bank 0 fill byte 0x00
        assert_eq!(cart.read(0x0000), 0x00);
        assert_eq!(cart.read(0x3FFF), 0x00);
    }

    #[test]
    fn test_mbc1_reads_bank1_data_from_high_region_initially() {
        // Given: 4-bank ROM; initial bank register = 0 → effective = 1
        let cart = Mbc1::new(make_mbc1_rom(4), make_mbc1_ram(0), false);
        // When: reading from the $4000–$7FFF window
        // Then: returns bank 1 fill byte 0x01
        assert_eq!(cart.read(0x4000), 0x01);
        assert_eq!(cart.read(0x7FFF), 0x01);
    }

    #[test]
    fn test_mbc1_rom_bank_switch_selects_correct_bank() {
        // Given: 4-bank ROM; select bank 2 by writing to $2000
        let mut cart = Mbc1::new(make_mbc1_rom(4), make_mbc1_ram(0), false);
        // When: writing 0x02 to $2000 and reading from high region
        cart.write(0x2000, 0x02);
        // Then: $4000 returns bank 2 fill byte 0x02
        assert_eq!(cart.read(0x4000), 0x02);
    }

    #[test]
    fn test_mbc1_writing_zero_to_bank_reg_selects_bank_1() {
        // Given: 4-bank ROM; write 0x00 to $2000 (hardware corrects to bank 1)
        let mut cart = Mbc1::new(make_mbc1_rom(4), make_mbc1_ram(0), false);
        // When: writing 0x00
        cart.write(0x2000, 0x00);
        // Then: bank 1 (fill byte 0x01) appears at $4000
        assert_eq!(cart.read(0x4000), 0x01);
    }

    #[test]
    fn test_mbc1_secondary_bank_shifts_to_upper_rom_bits_in_mode0() {
        // Given: 64-bank ROM (1 MB); secondary_bank = 1; rom_bank_reg = 0 → 1
        // Then: effective bank = (1 << 5) | 1 = 33; bank 33 fill = 33u8
        let mut cart = Mbc1::new(make_mbc1_rom(64), make_mbc1_ram(0), false);
        cart.write(0x4000, 0x01); // secondary_bank = 1
        cart.write(0x2000, 0x00); // rom_bank_reg = 0 → corrected to 1
        assert_eq!(cart.read(0x4000), 33u8);
    }

    #[test]
    fn test_mbc1_mode1_bank0_region_uses_secondary_bank_offset() {
        // Given: 64-bank ROM; mode 1; secondary_bank = 1
        // Then: $0000–$3FFF shows bank (1 << 5) = 32; fill byte = 32u8
        let mut cart = Mbc1::new(make_mbc1_rom(64), make_mbc1_ram(0), false);
        cart.write(0x6000, 0x01); // mode = 1
        cart.write(0x4000, 0x01); // secondary_bank = 1
        assert_eq!(cart.read(0x0000), 32u8);
    }

    #[test]
    fn test_mbc1_ram_read_returns_0xff_when_disabled() {
        // Given: cart with 1 RAM bank, RAM disabled (default)
        let cart = Mbc1::new(make_mbc1_rom(4), make_mbc1_ram(1), false);
        // Then: reads from $A000 return 0xFF
        assert_eq!(cart.read(0xA000), 0xFF);
    }

    #[test]
    fn test_mbc1_ram_read_write_when_enabled() {
        // Given: cart with 1 RAM bank; enable RAM
        let mut cart = Mbc1::new(make_mbc1_rom(4), make_mbc1_ram(1), false);
        cart.write(0x0000, 0x0A); // Enable RAM
        // When: writing 0x42 to $A000
        cart.write(0xA000, 0x42);
        // Then: reading $A000 returns 0x42
        assert_eq!(cart.read(0xA000), 0x42);
    }

    #[test]
    fn test_mbc1_ram_bank_switching_in_mode1() {
        // Given: 4 RAM banks, mode 1; write distinct data to banks 0 and 1
        let mut cart = Mbc1::new(make_mbc1_rom(4), make_mbc1_ram(4), false);
        cart.write(0x0000, 0x0A); // Enable RAM
        cart.write(0x6000, 0x01); // Mode 1

        cart.write(0x4000, 0x00); // secondary = 0 → RAM bank 0
        cart.write(0xA000, 0xAA);

        cart.write(0x4000, 0x01); // secondary = 1 → RAM bank 1
        cart.write(0xA000, 0xBB);

        // Then: bank 1 read returns 0xBB
        assert_eq!(cart.read(0xA000), 0xBB);
        // And: switching back to bank 0 returns 0xAA
        cart.write(0x4000, 0x00);
        assert_eq!(cart.read(0xA000), 0xAA);
    }

    // ── MBC1M multicart detection tests ─────────────────────────────────────

    /// Build a 1 MiB (64-bank) MBC1M ROM with a valid Nintendo logo in bank $10.
    /// Bank K is filled with `K as u8` so reads at offset 0 within any bank
    /// return the bank index as a byte.
    fn make_mbc1m_rom() -> Vec<u8> {
        let mut rom = make_mbc1_rom(64);
        let logo_offset = 0x10 * 0x4000 + 0x104;
        rom[logo_offset..logo_offset + HEADER_LOGO_LEN].copy_from_slice(&fixture_header_logo());
        rom
    }

    fn fixture_header_logo() -> [u8; HEADER_LOGO_LEN] {
        let rom =
            std::fs::read("roms/gb/automated_tests/daid/rom_and_ram.gb").expect("fixture ROM");
        rom[0x0104..0x0134].try_into().expect("header logo bytes")
    }

    #[test]
    fn test_is_multicart_rom_returns_true_for_1mb_with_logo_in_bank10() {
        // Given: 1 MiB ROM with valid Nintendo logo at bank $10 offset $104
        let rom = make_mbc1m_rom();
        // Then: detected as MBC1M multicart
        assert!(is_multicart_rom(&rom));
    }

    #[test]
    fn test_is_multicart_rom_returns_false_for_non_1mb_rom() {
        // Given: 2 MiB (128-bank) ROM that happens to have the logo in bank $10
        let mut rom = make_mbc1_rom(128);
        let logo_offset = 0x10 * 0x4000 + 0x104;
        rom[logo_offset..logo_offset + HEADER_LOGO_LEN].copy_from_slice(&fixture_header_logo());
        // Then: not detected as multicart (wrong size)
        assert!(!is_multicart_rom(&rom));
    }

    #[test]
    fn test_is_multicart_rom_returns_false_for_1mb_without_logo() {
        // Given: 1 MiB ROM with no logo at bank $10
        let rom = make_mbc1_rom(64);
        // Then: not detected as multicart (logo absent)
        assert!(!is_multicart_rom(&rom));
    }

    // ── MBC1M banking formula tests ──────────────────────────────────────────

    #[test]
    fn test_mbc1m_bank1_maps_secondary_to_bits_4_5() {
        // Given: MBC1M cart; secondary=1, rom_bank=1
        // Then: bank1_index = (1 << 4) | 1 = 17 → fill byte = 17u8
        // (Standard MBC1 would give (1 << 5) | 1 = 33 → fill = 33u8)
        let mut cart = Mbc1::new(make_mbc1m_rom(), make_mbc1_ram(0), false);
        cart.write(0x4000, 0x01); // secondary = 1
        cart.write(0x2000, 0x01); // rom_bank = 1
        assert_eq!(cart.read(0x4000), 17u8);
    }

    #[test]
    fn test_mbc1m_bank1_ignores_top_bit_of_rom_bank_register() {
        // Given: MBC1M cart; secondary=0, rom_bank_reg=0x11 (17; top bit set)
        // Then: effective bank uses only lower 4 bits → (0 << 4) | 1 = 1 → fill = 1u8
        // (Standard MBC1 would give (0 << 5) | 17 = 17 → fill = 17u8)
        let mut cart = Mbc1::new(make_mbc1m_rom(), make_mbc1_ram(0), false);
        cart.write(0x4000, 0x00); // secondary = 0
        cart.write(0x2000, 0x11); // rom_bank_reg = 0x11 = 17
        assert_eq!(cart.read(0x4000), 1u8);
    }

    #[test]
    fn test_mbc1m_bank0_in_mode1_uses_secondary_shifted_4() {
        // Given: MBC1M cart; mode=1, secondary=1
        // Then: bank0_index = (1 << 4) = 16 → fill byte = 16u8
        // (Standard MBC1 would give (1 << 5) = 32 → fill = 32u8)
        let mut cart = Mbc1::new(make_mbc1m_rom(), make_mbc1_ram(0), false);
        cart.write(0x6000, 0x01); // mode = 1
        cart.write(0x4000, 0x01); // secondary = 1
        assert_eq!(cart.read(0x0000), 16u8);
    }

    // ── RAM bank masking test ────────────────────────────────────────────────

    #[test]
    fn test_mbc1_ram_bank_masked_to_actual_count_with_1_bank() {
        // Given: 1 RAM bank (8 KB, 64 Kbit); mode 1; data written to bank 0
        // When: secondary register is set to 2 (out of range for 1-bank cart)
        // Then: reads return the same data (secondary masked to bank 0)
        let mut cart = Mbc1::new(make_mbc1_rom(4), make_mbc1_ram(1), false);
        cart.write(0x0000, 0x0A); // enable RAM
        cart.write(0x6000, 0x01); // mode 1
        cart.write(0x4000, 0x00); // secondary = 0 → RAM bank 0
        cart.write(0xA000, 0x42); // write sentinel to bank 0
        cart.write(0x4000, 0x02); // secondary = 2 → should still map to bank 0
        assert_eq!(cart.read(0xA000), 0x42);
    }
}
