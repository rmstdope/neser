use super::cartridge::GbCartridge;

/// MBC5 cartridge (types 0x19–0x1E).
///
/// Supports up to 8 MiB ROM (512 banks × 16 KiB) and up to 128 KiB RAM (16 banks × 8 KiB).
///
/// Registers (write-only, mapped to ROM area):
/// - $0000–$1FFF: RAM enable (bottom nibble 0xA → enable; any other value → disable)
/// - $2000–$2FFF: lower 8 bits of ROM bank number
/// - $3000–$3FFF: bit 8 (9th bit) of ROM bank number
/// - $4000–$5FFF: RAM bank number
///   - Non-rumble types (0x19–0x1B): 4-bit bank, 0x00–0x0F
///   - Rumble types (0x1C–0x1E): 3-bit bank, 0x00–0x07; bit 3 drives the rumble
///     motor line and is masked out before updating the bank register
///
/// Unlike MBC1/MBC2, writing 0 to the ROM bank register genuinely selects bank 0;
/// there is no bank-0 → bank-1 promotion.
pub struct Mbc5 {
    rom: Vec<u8>,
    ram: Vec<u8>,
    /// 9-bit ROM bank number (0x000–0x1FF). Lower 8 bits come from writes to
    /// $2000–$2FFF; bit 8 comes from the lowest bit of writes to $3000–$3FFF.
    rom_bank: u16,
    /// RAM bank number (3- or 4-bit depending on `has_rumble`).
    ram_bank: u8,
    /// Whether cartridge RAM is enabled.
    ram_enabled: bool,
    /// True for rumble cart types (0x1C–0x1E). Bit 3 of writes to $4000–$5FFF
    /// controls the rumble motor and is masked out of the RAM bank register.
    has_rumble: bool,
}

impl Mbc5 {
    pub fn new(rom: Vec<u8>, ram: Vec<u8>, has_rumble: bool) -> Self {
        Self {
            rom,
            ram,
            // MBC5 powers on with bank 1 in the switchable window — same as
            // other MBCs.  "Writing 0 gives bank 0" describes write behaviour,
            // not the power-on state.
            rom_bank: 1,
            ram_bank: 0,
            ram_enabled: false,
            has_rumble,
        }
    }

    fn rom_bank_count(&self) -> usize {
        (self.rom.len() / 0x4000).max(1)
    }

    fn ram_bank_count(&self) -> usize {
        (self.ram.len() / 0x2000).max(1)
    }

    /// Effective ROM bank for the $4000–$7FFF window.
    /// Masked to available banks; bank 0 is valid (no promotion).
    fn effective_rom_bank(&self) -> usize {
        (self.rom_bank as usize) & (self.rom_bank_count() - 1)
    }

    /// Effective RAM bank index.
    fn effective_ram_bank(&self) -> usize {
        (self.ram_bank as usize) & (self.ram_bank_count() - 1)
    }

    fn read_rom(&self, addr: u16) -> u8 {
        let (bank, offset) = if addr < 0x4000 {
            (0, addr as usize)
        } else {
            (self.effective_rom_bank(), addr as usize - 0x4000)
        };
        let idx = bank * 0x4000 + offset;
        self.rom.get(idx).copied().unwrap_or(0xFF)
    }

    fn read_ram(&self, addr: u16) -> u8 {
        if !self.ram_enabled || self.ram.is_empty() {
            return 0xFF;
        }
        let offset = addr as usize - 0xA000;
        let idx = self.effective_ram_bank() * 0x2000 + offset;
        self.ram.get(idx).copied().unwrap_or(0xFF)
    }

    fn write_registers(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.ram_enabled = (val & 0x0F) == 0x0A;
            }
            0x2000..=0x2FFF => {
                self.rom_bank = (self.rom_bank & 0x0100) | (val as u16);
            }
            0x3000..=0x3FFF => {
                self.rom_bank = (self.rom_bank & 0x00FF) | (((val & 0x01) as u16) << 8);
            }
            0x4000..=0x5FFF => {
                let mask = if self.has_rumble { 0x07 } else { 0x0F };
                self.ram_bank = val & mask;
            }
            _ => {}
        }
    }

    fn write_ram(&mut self, addr: u16, val: u8) {
        if !self.ram_enabled || self.ram.is_empty() {
            return;
        }
        let offset = addr as usize - 0xA000;
        let idx = self.effective_ram_bank() * 0x2000 + offset;
        if let Some(byte) = self.ram.get_mut(idx) {
            *byte = val;
        }
    }
}

impl GbCartridge for Mbc5 {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.read_rom(addr),
            0xA000..=0xBFFF => self.read_ram(addr),
            _ => 0xFF,
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x5FFF => self.write_registers(addr, val),
            0xA000..=0xBFFF => self.write_ram(addr, val),
            _ => {}
        }
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
            (self.rom_bank & 0xFF) as u8,
            (self.rom_bank >> 8) as u8,
            self.ram_bank,
            self.ram_enabled as u8,
            self.has_rumble as u8,
        ]
    }

    fn restore_mbc_state(&mut self, data: &[u8]) {
        if data.len() >= 5 {
            self.rom_bank = data[0] as u16 | ((data[1] as u16) << 8);
            self.ram_bank = data[2];
            self.ram_enabled = data[3] != 0;
            self.has_rumble = data[4] != 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an N-bank ROM where bank K is uniformly filled with `K as u8`.
    fn make_rom(bank_count: usize) -> Vec<u8> {
        let mut rom = vec![0u8; bank_count * 0x4000];
        for bank in 0..bank_count {
            let start = bank * 0x4000;
            rom[start..start + 0x4000].fill(bank as u8);
        }
        rom
    }

    fn make_ram(bank_count: usize) -> Vec<u8> {
        vec![0u8; bank_count * 0x2000]
    }

    // -----------------------------------------------------------------------
    // ROM banking — fixed window ($0000–$3FFF)
    // -----------------------------------------------------------------------

    #[test]
    fn test_mbc5_fixed_window_always_reads_bank0() {
        // Given: 4-bank ROM; bank 0 filled with 0x00
        let cart = Mbc5::new(make_rom(4), make_ram(0), false);
        // Then: $0000 and $3FFF both return bank 0 fill byte
        assert_eq!(cart.read(0x0000), 0x00);
        assert_eq!(cart.read(0x3FFF), 0x00);
    }

    #[test]
    fn test_mbc5_fixed_window_unchanged_after_bank_select() {
        // Given: bank 2 is selected
        let mut cart = Mbc5::new(make_rom(4), make_ram(0), false);
        cart.write(0x2000, 0x02);
        // Then: $0000 still returns bank 0 data
        assert_eq!(cart.read(0x0000), 0x00);
    }

    // -----------------------------------------------------------------------
    // ROM banking — switchable window ($4000–$7FFF)
    // -----------------------------------------------------------------------

    #[test]
    fn test_mbc5_switchable_window_initially_maps_bank1() {
        // MBC5 powers on with bank 1 in the switchable window (same as other MBCs).
        // "Writing 0 gives bank 0" applies only to explicit writes, not power-on state.
        let cart = Mbc5::new(make_rom(4), make_ram(0), false);
        assert_eq!(cart.read(0x4000), 0x01);
        assert_eq!(cart.read(0x7FFF), 0x01);
    }

    #[test]
    fn test_mbc5_writing_zero_to_bank_reg_selects_bank0() {
        // Unlike MBC1/MBC2, explicitly writing 0 selects bank 0 (no promotion).
        let mut cart = Mbc5::new(make_rom(4), make_ram(0), false);
        cart.write(0x2000, 0x00);
        assert_eq!(cart.read(0x4000), 0x00);
    }

    #[test]
    fn test_mbc5_rom_bank_switch_low_byte_selects_correct_bank() {
        // Given: 4-bank ROM; write 0x02 to $2000
        let mut cart = Mbc5::new(make_rom(4), make_ram(0), false);
        cart.write(0x2000, 0x02);
        // Then: $4000 returns bank 2 fill byte
        assert_eq!(cart.read(0x4000), 0x02);
        assert_eq!(cart.read(0x7FFF), 0x02);
    }

    #[test]
    fn test_mbc5_rom_bank_switch_to_bank3() {
        let mut cart = Mbc5::new(make_rom(4), make_ram(0), false);
        cart.write(0x2000, 0x03);
        assert_eq!(cart.read(0x4000), 0x03);
    }

    #[test]
    fn test_mbc5_9th_bit_selects_upper_bank() {
        // Given: 512-bank ROM; write 0xFF to $2000 and 0x01 to $3000 → bank 0x1FF
        // Bank 0x1FF = 511; fill byte = 511u8 = 0xFF
        let mut cart = Mbc5::new(make_rom(512), make_ram(0), false);
        cart.write(0x2000, 0xFF); // lower 8 bits = 0xFF
        cart.write(0x3000, 0x01); // bit 8 set → bank = 0x1FF
        assert_eq!(cart.read(0x4000), 0xFF);
    }

    #[test]
    fn test_mbc5_9th_bit_only_lowest_bit_matters() {
        // Writing 0xFE to $3000 (bit 0 = 0) should not set bit 8
        let mut cart = Mbc5::new(make_rom(4), make_ram(0), false);
        cart.write(0x2000, 0x02); // bank 2
        cart.write(0x3000, 0xFE); // bit 0 = 0 → bit 8 stays 0
        assert_eq!(cart.read(0x4000), 0x02); // still bank 2
    }

    #[test]
    fn test_mbc5_9th_bit_can_be_cleared() {
        // Set bit 8, then clear it by writing 0x00 to $3000
        let mut cart = Mbc5::new(make_rom(512), make_ram(0), false);
        cart.write(0x2000, 0x01); // bank 1
        cart.write(0x3000, 0x01); // bit 8 set → bank 0x101 = 257; fill byte = 1 (wraps)
        cart.write(0x3000, 0x00); // clear bit 8 → bank 1
        assert_eq!(cart.read(0x4000), 0x01); // bank 1 fill byte
    }

    #[test]
    fn test_mbc5_low_byte_write_preserves_high_bit() {
        // Set bit 8, then change low byte; bit 8 should be preserved
        let mut cart = Mbc5::new(make_rom(512), make_ram(0), false);
        cart.write(0x3000, 0x01); // bit 8 set
        cart.write(0x2000, 0x05); // low byte = 5 → bank = 0x105 = 261
        // 261 & 511 = 261; fill byte = 261u8 = 5 (261 % 256 wraps, and fill = bank as u8)
        assert_eq!(cart.read(0x4000), 5u8);
    }

    #[test]
    fn test_mbc5_bank_masking_wraps_to_available_banks() {
        // 4-bank ROM; requesting bank 5 → 5 & 3 = 1; fill byte = 1
        let mut cart = Mbc5::new(make_rom(4), make_ram(0), false);
        cart.write(0x2000, 0x05);
        assert_eq!(cart.read(0x4000), 0x01);
    }

    // -----------------------------------------------------------------------
    // RAM enable / disable
    // -----------------------------------------------------------------------

    #[test]
    fn test_mbc5_ram_disabled_by_default_returns_0xff() {
        let cart = Mbc5::new(make_rom(2), make_ram(1), false);
        assert_eq!(cart.read(0xA000), 0xFF);
    }

    #[test]
    fn test_mbc5_ram_enable_with_0x0a() {
        let mut cart = Mbc5::new(make_rom(2), make_ram(1), false);
        cart.write(0x0000, 0x0A); // enable RAM
        cart.write(0xA000, 0x42);
        assert_eq!(cart.read(0xA000), 0x42);
    }

    #[test]
    fn test_mbc5_ram_enable_with_any_lower_nibble_a() {
        // $1A has lower nibble 0xA → should also enable
        let mut cart = Mbc5::new(make_rom(2), make_ram(1), false);
        cart.write(0x0000, 0x1A);
        cart.write(0xA000, 0x55);
        assert_eq!(cart.read(0xA000), 0x55);
    }

    #[test]
    fn test_mbc5_ram_disable_with_0x00() {
        let mut cart = Mbc5::new(make_rom(2), make_ram(1), false);
        cart.write(0x0000, 0x0A); // enable
        cart.write(0xA000, 0x42);
        cart.write(0x0000, 0x00); // disable
        assert_eq!(cart.read(0xA000), 0xFF);
    }

    #[test]
    fn test_mbc5_ram_disable_non_0xa_lower_nibble() {
        // Lower nibble 0x0B ≠ 0x0A → disable
        let mut cart = Mbc5::new(make_rom(2), make_ram(1), false);
        cart.write(0x0000, 0x0A); // enable first
        cart.write(0xA000, 0x42);
        cart.write(0x0000, 0x0B); // disable
        assert_eq!(cart.read(0xA000), 0xFF);
    }

    #[test]
    fn test_mbc5_ram_write_ignored_when_disabled() {
        let mut cart = Mbc5::new(make_rom(2), make_ram(1), false);
        // RAM disabled; write should be a no-op
        cart.write(0xA000, 0x42);
        // Enable and verify the write did nothing
        cart.write(0x0000, 0x0A);
        assert_eq!(cart.read(0xA000), 0x00); // initial RAM = 0
    }

    #[test]
    fn test_mbc5_empty_ram_returns_0xff_even_when_enabled() {
        let mut cart = Mbc5::new(make_rom(2), make_ram(0), false);
        cart.write(0x0000, 0x0A); // enable
        assert_eq!(cart.read(0xA000), 0xFF);
    }

    // -----------------------------------------------------------------------
    // RAM banking
    // -----------------------------------------------------------------------

    #[test]
    fn test_mbc5_ram_bank_switching() {
        // Given: 2 RAM banks; write distinct bytes to each bank
        let mut cart = Mbc5::new(make_rom(2), make_ram(2), false);
        cart.write(0x0000, 0x0A); // enable RAM

        cart.write(0x4000, 0x00); // select RAM bank 0
        cart.write(0xA000, 0xAA);

        cart.write(0x4000, 0x01); // select RAM bank 1
        cart.write(0xA000, 0xBB);

        // Then: bank 1 returns 0xBB
        assert_eq!(cart.read(0xA000), 0xBB);
        // Switching back returns 0xAA
        cart.write(0x4000, 0x00);
        assert_eq!(cart.read(0xA000), 0xAA);
    }

    #[test]
    fn test_mbc5_ram_bank_register_masked_to_4_bits() {
        // Writing 0xFF → lower 4 bits = 0xF = 15; 1 RAM bank → wraps to 0
        let mut cart = Mbc5::new(make_rom(2), make_ram(1), false);
        cart.write(0x0000, 0x0A);
        cart.write(0x4000, 0xFF); // 4-bit mask → ram_bank = 0xF; 1 bank → wraps to 0
        cart.write(0xA000, 0x77);
        assert_eq!(cart.read(0xA000), 0x77);
    }

    #[test]
    fn test_mbc5_ram_bank_masking_wraps_to_available_banks() {
        // 2 RAM banks; select bank 3 → 3 & 1 = 1 (wraps to bank 1)
        let mut cart = Mbc5::new(make_rom(2), make_ram(2), false);
        cart.write(0x0000, 0x0A);
        cart.write(0x4000, 0x01); // bank 1
        cart.write(0xA000, 0xCC);
        cart.write(0x4000, 0x03); // bank 3 → wraps to bank 1
        assert_eq!(cart.read(0xA000), 0xCC);
    }

    // -----------------------------------------------------------------------
    // Rumble cart RAM bank masking
    // -----------------------------------------------------------------------

    #[test]
    fn test_mbc5_rumble_bit3_does_not_affect_ram_bank() {
        // For rumble carts, bit 3 drives the rumble motor; writing 0x08 (only
        // bit 3 set) should leave the RAM bank at 0, not switch to bank 8.
        let mut cart = Mbc5::new(make_rom(2), make_ram(2), true);
        cart.write(0x0000, 0x0A);
        cart.write(0x4000, 0x00); // bank 0
        cart.write(0xA000, 0xAA);
        cart.write(0x4000, 0x08); // bit 3 = rumble; bank bits = 0 → still bank 0
        assert_eq!(cart.read(0xA000), 0xAA);
    }

    #[test]
    fn test_mbc5_rumble_ram_bank_masked_to_3_bits() {
        // For rumble carts, the RAM bank is only 3 bits (0x00–0x07).
        // Writing 0xFF → 0xFF & 0x07 = 7, not 0x0F = 15.
        let mut cart = Mbc5::new(make_rom(2), make_ram(8), true);
        cart.write(0x0000, 0x0A);
        cart.write(0x4000, 0x07); // bank 7
        cart.write(0xA000, 0xBB);
        cart.write(0x4000, 0xFF); // 0xFF & 0x07 = 7; still bank 7
        assert_eq!(cart.read(0xA000), 0xBB);
    }

    #[test]
    fn test_mbc5_non_rumble_ram_bank_uses_full_4_bits() {
        // Non-rumble carts allow 4-bit bank (0x00–0x0F). Bit 3 is a valid bank bit.
        let mut cart = Mbc5::new(make_rom(2), make_ram(16), false);
        cart.write(0x0000, 0x0A);
        cart.write(0x4000, 0x08); // bank 8 — valid for non-rumble
        cart.write(0xA000, 0xCC);
        cart.write(0x4000, 0x08);
        assert_eq!(cart.read(0xA000), 0xCC);
    }
}
