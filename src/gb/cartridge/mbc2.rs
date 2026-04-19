use super::cartridge::GbCartridge;

/// MBC2 cartridge (types 0x05 = MBC2, 0x06 = MBC2+BATTERY).
///
/// Supports up to 16 ROM banks (256 KB) and has 512 × 4-bit built-in RAM
/// at $A000–$A1FF (mirrored across $A000–$BFFF).
///
/// Register writes ($0000–$3FFF):
/// - Bit 8 of address **clear**: RAM enable gate (lower nibble 0xA → enable; else disable)
/// - Bit 8 of address **set**: ROM bank register (lower 4 bits; bank 0 promoted to bank 1)
pub struct Mbc2 {
    rom: Vec<u8>,
    /// 512-byte built-in RAM (each slot stores a full byte; upper nibble masked on reads).
    ram: [u8; 512],
    /// Raw ROM bank register (0 is promoted to 1 on access).
    rom_bank: u8,
    /// Whether cartridge RAM is enabled.
    ram_enabled: bool,
}

impl Mbc2 {
    pub fn new(rom: Vec<u8>) -> Self {
        Self {
            rom,
            ram: [0u8; 512],
            rom_bank: 0,
            ram_enabled: false,
        }
    }

    fn rom_bank_count(&self) -> usize {
        (self.rom.len() / 0x4000).max(1)
    }

    /// Effective ROM bank for the $4000–$7FFF window.
    /// Lower 4 bits; bank 0 promotes to bank 1; masked to available banks.
    fn effective_rom_bank(&self) -> usize {
        let raw = (self.rom_bank & 0x0F) as usize;
        let promoted = if raw == 0 { 1 } else { raw };
        promoted & (self.rom_bank_count() - 1)
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
        if !self.ram_enabled {
            return 0xFF;
        }
        let idx = (addr as usize - 0xA000) & 0x1FF;
        self.ram[idx] | 0xF0
    }

    fn write_registers(&mut self, addr: u16, val: u8) {
        if addr & 0x0100 == 0 {
            // Bit 8 clear → RAM enable gate
            self.ram_enabled = (val & 0x0F) == 0x0A;
        } else {
            // Bit 8 set → ROM bank select (lower 4 bits)
            self.rom_bank = val & 0x0F;
        }
    }

    fn write_ram(&mut self, addr: u16, val: u8) {
        if !self.ram_enabled {
            return;
        }
        let idx = (addr as usize - 0xA000) & 0x1FF;
        self.ram[idx] = val;
    }
}

impl GbCartridge for Mbc2 {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.read_rom(addr),
            0xA000..=0xBFFF => self.read_ram(addr),
            _ => 0xFF,
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x3FFF => self.write_registers(addr, val),
            0xA000..=0xBFFF => self.write_ram(addr, val),
            _ => {}
        }
    }

    fn ram_snapshot(&self) -> Vec<u8> {
        self.ram.to_vec()
    }

    fn restore_ram(&mut self, data: &[u8]) {
        let len = data.len().min(self.ram.len());
        self.ram[..len].copy_from_slice(&data[..len]);
    }

    fn mbc_state_snapshot(&self) -> Vec<u8> {
        vec![self.rom_bank, self.ram_enabled as u8]
    }

    fn restore_mbc_state(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.rom_bank = data[0];
            self.ram_enabled = data[1] != 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a test ROM with `bank_count` banks, where each bank's bytes are
    /// all set to `bank_index as u8`.
    fn make_rom(bank_count: usize) -> Vec<u8> {
        let mut rom = vec![0u8; bank_count * 0x4000];
        for bank in 0..bank_count {
            let fill = bank as u8;
            let start = bank * 0x4000;
            rom[start..start + 0x4000].fill(fill);
        }
        rom
    }

    // -----------------------------------------------------------------------
    // ROM banking
    // -----------------------------------------------------------------------

    #[test]
    fn test_mbc2_initial_rom_bank0_readable() {
        // Bank 0 is always at $0000–$3FFF; fill byte = 0
        let cart = Mbc2::new(make_rom(4));
        assert_eq!(cart.read(0x0000), 0x00);
        assert_eq!(cart.read(0x3FFF), 0x00);
    }

    #[test]
    fn test_mbc2_initial_high_region_maps_bank1() {
        // rom_bank = 0 → promotes to 1; fill byte = 1
        let cart = Mbc2::new(make_rom(4));
        assert_eq!(cart.read(0x4000), 0x01);
        assert_eq!(cart.read(0x7FFF), 0x01);
    }

    #[test]
    fn test_mbc2_rom_bank_switch_selects_bank() {
        // Write 0x02 to addr with bit 8 set (e.g. $0100) → bank 2
        let mut cart = Mbc2::new(make_rom(4));
        cart.write(0x0100, 0x02);
        assert_eq!(cart.read(0x4000), 0x02);
    }

    #[test]
    fn test_mbc2_writing_zero_bank_reg_promotes_to_bank1() {
        // Explicitly write 0 to bank reg → still maps to bank 1
        let mut cart = Mbc2::new(make_rom(4));
        cart.write(0x0100, 0x00);
        assert_eq!(cart.read(0x4000), 0x01);
    }

    #[test]
    fn test_mbc2_bank_reg_masked_to_4_bits() {
        // Writing 0x1F → lower 4 bits = 0xF = 15
        let mut cart = Mbc2::new(make_rom(16));
        cart.write(0x0100, 0x1F);
        assert_eq!(cart.read(0x4000), 0x0F);
    }

    #[test]
    fn test_mbc2_bank_wraps_to_available_banks() {
        // 4-bank ROM; requesting bank 5 → 5 & 3 = 1 (effective_rom_bank &
        // (bank_count - 1) masking); fill byte = 1
        let mut cart = Mbc2::new(make_rom(4));
        cart.write(0x0100, 0x05); // lower 4 bits = 5; 5 & (4-1) = 1
        assert_eq!(cart.read(0x4000), 0x01);
    }

    // -----------------------------------------------------------------------
    // RAM gate
    // -----------------------------------------------------------------------

    #[test]
    fn test_mbc2_ram_disabled_by_default_returns_0xff() {
        let cart = Mbc2::new(make_rom(2));
        assert_eq!(cart.read(0xA000), 0xFF);
    }

    #[test]
    fn test_mbc2_ram_enable_any_lower_nibble_0xa() {
        // 0x2A has lower nibble 0xA → enables; addr has bit 8 clear (0x0000)
        let mut cart = Mbc2::new(make_rom(2));
        cart.write(0x0000, 0x2A);
        cart.write(0xA000, 0x05);
        assert_eq!(cart.read(0xA000), 0x05 | 0xF0);
    }

    #[test]
    fn test_mbc2_ram_disable_non_0xa_lower_nibble() {
        // Enable first, then write 0x00 (lower nibble 0) → disabled
        let mut cart = Mbc2::new(make_rom(2));
        cart.write(0x0000, 0x0A); // enable
        cart.write(0xA000, 0x07);
        cart.write(0x0000, 0x00); // disable
        assert_eq!(cart.read(0xA000), 0xFF);
    }

    #[test]
    fn test_mbc2_ram_write_and_read_when_enabled() {
        let mut cart = Mbc2::new(make_rom(2));
        cart.write(0x0000, 0x0A);
        cart.write(0xA000, 0x03);
        assert_eq!(cart.read(0xA000), 0x03 | 0xF0);
    }

    #[test]
    fn test_mbc2_ram_read_returns_upper_nibble_0xf() {
        // Stored value 0x05; read must return 0xF5
        let mut cart = Mbc2::new(make_rom(2));
        cart.write(0x0000, 0x0A);
        cart.write(0xA001, 0x05);
        assert_eq!(cart.read(0xA001), 0xF5);
    }

    #[test]
    fn test_mbc2_ram_mirrors_a200_to_bfff() {
        // Write to $A000; read back at $A200 (512 bytes later, same slot)
        let mut cart = Mbc2::new(make_rom(2));
        cart.write(0x0000, 0x0A);
        cart.write(0xA000, 0x09);
        assert_eq!(cart.read(0xA200), 0x09 | 0xF0);
    }

    #[test]
    fn test_mbc2_writes_to_bit8_clear_addr_do_not_change_bank() {
        // Bit 8 clear → RAM gate write; should NOT change current ROM bank
        let mut cart = Mbc2::new(make_rom(4));
        cart.write(0x0100, 0x03); // select bank 3 first
        cart.write(0x0000, 0x0A); // RAM enable (bit 8 clear) — must not reset bank
        assert_eq!(cart.read(0x4000), 0x03);
    }

    #[test]
    fn test_mbc2_writes_to_bit8_set_addr_do_not_enable_ram() {
        // Bit 8 set → ROM bank select; should NOT change RAM enabled state
        let mut cart = Mbc2::new(make_rom(4));
        // RAM disabled by default; write bank select with 0x0A value but bit 8 set
        cart.write(0x0100, 0x0A);
        // RAM should still be disabled
        assert_eq!(cart.read(0xA000), 0xFF);
    }
}
