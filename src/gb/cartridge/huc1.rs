//! HuC1 mapper implementation (Hudson Soft, type 0xFF).
//!
//! The HuC1 is an MBC1-like mapper used by various Hudson Soft games.
//! It supports up to 1 MB ROM (64 banks × 16 KB) and up to 32 KB RAM (4 banks × 8 KB).
//!
//! The HuC1 includes an IR port for infrared communication, but this is not commonly used
//! and is not implemented here.
//!
//! Registers (write-only, mapped to ROM area):
//! - $0000–$1FFF: RAM enable (write 0x0A to enable; any other value disables)
//! - $2000–$3FFF: ROM bank number (7-bit; write 0 selects bank 1)
//! - $4000–$5FFF: Secondary bank register (2-bit; RAM bank or upper ROM bits)
//! - $6000–$7FFF: Banking mode (0 = mode 0, 1 = mode 1)

use super::cartridge::GbCartridge;

/// HuC1 cartridge mapper (type 0xFF).
pub struct Huc1 {
    rom: Vec<u8>,
    ram: Vec<u8>,
    /// Lower 7 bits of the ROM bank register (raw; 0 is corrected to 1 on access).
    rom_bank: u8,
    /// 2-bit secondary bank register.
    secondary_bank: u8,
    /// Banking mode: false = mode 0, true = mode 1.
    mode: bool,
    /// Whether cartridge RAM is enabled.
    ram_enabled: bool,
}

impl Huc1 {
    /// Creates a new HuC1 mapper.
    pub fn new(rom: Vec<u8>, ram: Vec<u8>) -> Self {
        Self {
            rom,
            ram,
            rom_bank: 0,
            secondary_bank: 0,
            mode: false,
            ram_enabled: false,
        }
    }

    fn rom_bank_count(&self) -> usize {
        (self.rom.len() / 0x4000).max(1)
    }

    fn ram_bank_count(&self) -> usize {
        (self.ram.len() / 0x2000).max(1)
    }

    /// Effective lower ROM bank number: writes of 0 select bank 1.
    fn effective_rom_bank(&self) -> usize {
        let raw = (self.rom_bank & 0x7F) as usize;
        if raw == 0 { 1 } else { raw }
    }

    /// ROM bank for the $4000–$7FFF window (applying mode and secondary bank).
    fn effective_rom_bank_high(&self) -> usize {
        let mut bank = self.effective_rom_bank();
        if self.mode {
            // Mode 1: secondary bank selects upper bits
            bank = (bank & 0x1F) | ((self.secondary_bank as usize & 0x03) << 5);
        }
        bank % self.rom_bank_count()
    }

    /// Secondary bank for RAM (when in mode 1).
    fn effective_secondary_bank(&self) -> usize {
        (self.secondary_bank as usize & 0x03) % self.ram_bank_count()
    }

    fn read_rom(&self, addr: u16) -> u8 {
        let (bank, offset) = if addr < 0x4000 {
            (0, addr as usize)
        } else {
            (self.effective_rom_bank_high(), addr as usize - 0x4000)
        };
        let idx = bank * 0x4000 + offset;
        self.rom.get(idx).copied().unwrap_or(0xFF)
    }

    fn read_ram(&self, addr: u16) -> u8 {
        if !self.ram_enabled || self.ram.is_empty() {
            return 0xFF;
        }
        let offset = addr as usize - 0xA000;
        let bank = if self.mode {
            self.effective_secondary_bank()
        } else {
            0
        };
        let idx = bank * 0x2000 + offset;
        self.ram.get(idx).copied().unwrap_or(0xFF)
    }

    fn write_registers(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.ram_enabled = (val & 0x0F) == 0x0A;
            }
            0x2000..=0x3FFF => {
                self.rom_bank = val & 0x7F;
            }
            0x4000..=0x5FFF => {
                self.secondary_bank = val & 0x03;
            }
            0x6000..=0x7FFF => {
                self.mode = (val & 0x01) != 0;
            }
            _ => {}
        }
    }

    fn write_ram(&mut self, addr: u16, val: u8) {
        if !self.ram_enabled || self.ram.is_empty() {
            return;
        }
        let offset = addr as usize - 0xA000;
        let bank = if self.mode {
            self.effective_secondary_bank()
        } else {
            0
        };
        let idx = bank * 0x2000 + offset;
        if let Some(slot) = self.ram.get_mut(idx) {
            *slot = val;
        }
    }
}

impl GbCartridge for Huc1 {
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

    fn ram_snapshot(&self) -> Vec<u8> {
        self.ram.clone()
    }

    fn restore_ram(&mut self, data: &[u8]) {
        if data.len() <= self.ram.len() {
            self.ram[..data.len()].copy_from_slice(data);
        }
    }

    fn mbc_state_snapshot(&self) -> Vec<u8> {
        vec![
            self.rom_bank,
            self.secondary_bank,
            self.mode as u8,
            self.ram_enabled as u8,
        ]
    }

    fn restore_mbc_state(&mut self, data: &[u8]) {
        if data.len() >= 4 {
            self.rom_bank = data[0];
            self.secondary_bank = data[1];
            self.mode = data[2] != 0;
            self.ram_enabled = data[3] != 0;
        }
    }

    fn has_battery(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rom(size_bytes: usize) -> Vec<u8> {
        vec![0u8; size_bytes]
    }

    fn make_ram(size_bytes: usize) -> Vec<u8> {
        vec![0u8; size_bytes]
    }

    #[test]
    fn test_rom_bank_selection() {
        let rom = make_rom(64 * 0x4000); // 64 banks
        let ram = make_ram(4 * 0x2000); // 4 banks
        let mut cart = Huc1::new(rom, ram);

        // Default bank 0 maps to bank 1
        assert_eq!(cart.effective_rom_bank(), 1);

        // Write bank 1 to register
        cart.write_registers(0x2000, 0x01);
        assert_eq!(cart.effective_rom_bank(), 1);

        // Write bank 2
        cart.write_registers(0x2000, 0x02);
        assert_eq!(cart.effective_rom_bank(), 2);

        // Write 0 still maps to 1 (promotion)
        cart.write_registers(0x2000, 0x00);
        assert_eq!(cart.effective_rom_bank(), 1);
    }

    #[test]
    fn test_ram_enable_disable() {
        let rom = make_rom(2 * 0x4000);
        let ram = make_ram(0x2000);
        let mut cart = Huc1::new(rom, ram);

        // RAM disabled by default
        assert!(!cart.ram_enabled);
        assert_eq!(cart.read_ram(0xA000), 0xFF);

        // Enable RAM with 0x0A
        cart.write_registers(0x0000, 0x0A);
        assert!(cart.ram_enabled);

        // Write to RAM
        cart.write_ram(0xA000, 0x42);
        assert_eq!(cart.read_ram(0xA000), 0x42);

        // Disable with any other value
        cart.write_registers(0x0000, 0x00);
        assert!(!cart.ram_enabled);
        assert_eq!(cart.read_ram(0xA000), 0xFF);
    }

    #[test]
    fn test_mode_0_banking() {
        let rom = make_rom(64 * 0x4000);
        let ram = make_ram(4 * 0x2000);
        let mut cart = Huc1::new(rom, ram);

        // Mode 0: secondary bank only affects upper ROM address bits
        cart.write_registers(0x6000, 0x00); // mode 0
        cart.write_registers(0x2000, 0x01); // ROM bank 1
        cart.write_registers(0x4000, 0x00); // secondary = 0

        // In mode 0, RAM always accesses bank 0
        cart.write_registers(0x0000, 0x0A); // enable RAM
        cart.write_ram(0xA000, 0x11);
        assert_eq!(cart.read_ram(0xA000), 0x11);

        // Change secondary bank
        cart.write_registers(0x4000, 0x01);
        // RAM should still read from bank 0 in mode 0
        assert_eq!(cart.read_ram(0xA000), 0x11);
    }

    #[test]
    fn test_mode_1_banking() {
        let rom = make_rom(64 * 0x4000);
        let ram = make_ram(4 * 0x2000);
        let mut cart = Huc1::new(rom, ram);

        cart.write_registers(0x0000, 0x0A); // enable RAM
        cart.write_registers(0x6000, 0x01); // mode 1

        // In mode 1, secondary bank selects RAM bank
        cart.write_registers(0x4000, 0x00); // secondary = 0
        cart.write_ram(0xA000, 0x11);

        cart.write_registers(0x4000, 0x01); // secondary = 1
        cart.write_ram(0xA000, 0x22);

        // Switch back to bank 0
        cart.write_registers(0x4000, 0x00);
        assert_eq!(cart.read_ram(0xA000), 0x11);

        // Switch to bank 1
        cart.write_registers(0x4000, 0x01);
        assert_eq!(cart.read_ram(0xA000), 0x22);
    }

    #[test]
    fn test_state_snapshot_restore() {
        let rom = make_rom(4 * 0x4000);
        let ram = make_ram(2 * 0x2000);
        let mut cart = Huc1::new(rom.clone(), ram.clone());

        // Set up state
        cart.write_registers(0x2000, 0x03);
        cart.write_registers(0x4000, 0x02);
        cart.write_registers(0x6000, 0x01);
        cart.write_registers(0x0000, 0x0A);

        // Store RAM and MBC state
        let ram_snapshot = cart.ram_snapshot();
        let mbc_snapshot = cart.mbc_state_snapshot();

        // Create new cart and restore
        let mut cart2 = Huc1::new(rom, ram);
        cart2.restore_mbc_state(&mbc_snapshot);
        cart2.restore_ram(&ram_snapshot);

        assert_eq!(cart2.rom_bank, 0x03);
        assert_eq!(cart2.secondary_bank, 0x02);
        assert!(cart2.mode);
        assert!(cart2.ram_enabled);
    }

    #[test]
    fn test_rom_read_boundary() {
        let mut rom_data = make_rom(2 * 0x4000);
        // Mark first bank
        rom_data[0x0100] = 0xAA;
        // Mark second bank
        rom_data[0x4100] = 0xBB;

        let ram = make_ram(0x2000);
        let mut cart = Huc1::new(rom_data, ram);

        // Read from bank 0 (fixed)
        assert_eq!(cart.read(0x0100), 0xAA);

        // Change to bank 1
        cart.write_registers(0x2000, 0x01);
        assert_eq!(cart.read(0x4100), 0xBB);
    }

    #[test]
    fn test_out_of_range_reads() {
        let rom = make_rom(2 * 0x4000);
        let ram = make_ram(0x2000);
        let cart = Huc1::new(rom, ram);

        // Reads outside ROM/RAM ranges should return 0xFF
        assert_eq!(cart.read(0x8000), 0xFF);
        assert_eq!(cart.read(0x9000), 0xFF);
        assert_eq!(cart.read(0xC000), 0xFF);
        assert_eq!(cart.read(0xE000), 0xFF);
    }
}
