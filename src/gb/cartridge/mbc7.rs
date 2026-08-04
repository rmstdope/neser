//! MBC7 mapper implementation (Accelerometer + EEPROM, type 0x22).
//!
//! The MBC7 is used by games with accelerometer and EEPROM support, notably Kirby Tilt n Tumble.
//! It supports up to 2 MB ROM (128 banks × 16 KB) and a 256-byte EEPROM (93LC56 equivalent).
//!
//! The accelerometer provides 2-axis input via a special register interface. This implementation
//! returns neutral values (0x80 for both X and Y axes) rather than simulating physics.
//!
//! Registers (write-only, mapped to ROM area):
//! - $0000–$1FFF: ROM/RAM enable (write 0x0A to enable; any other value disables)
//! - $2000–$3FFF: ROM bank number (8-bit; bank 0 is valid)
//! - $4000–$5FFF: RAM bank / Accelerometer / EEPROM select
//! - $A000–$BFFF: EEPROM data (256 bytes, mirrored across the range)

use super::cartridge::GbCartridge;

/// EEPROM state machine states
#[derive(Debug, Clone, Copy, PartialEq)]
enum EepromState {
    Idle,
    ReadCommand,
    WriteCommand,
    WriteData,
}

/// MBC7 cartridge mapper (type 0x22).
pub struct Mbc7 {
    rom: Vec<u8>,
    /// 256-byte EEPROM
    eeprom: [u8; 256],
    /// 8-bit ROM bank number (0–127)
    rom_bank: u8,
    /// Whether ROM/cartridge access is enabled
    access_enabled: bool,
    /// Current EEPROM state machine state
    eeprom_state: EepromState,
    /// EEPROM address for current operation
    eeprom_addr: u8,
}

impl Mbc7 {
    /// Creates a new MBC7 mapper.
    pub fn new(rom: Vec<u8>) -> Self {
        Self {
            rom,
            eeprom: [0xFF; 256],
            rom_bank: 1,
            access_enabled: false,
            eeprom_state: EepromState::Idle,
            eeprom_addr: 0,
        }
    }

    fn rom_bank_count(&self) -> usize {
        (self.rom.len() / 0x4000).max(1)
    }

    /// Effective ROM bank, masked to available banks.
    fn effective_rom_bank(&self) -> usize {
        (self.rom_bank as usize) % self.rom_bank_count()
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

    /// Read from EEPROM or accelerometer based on register state.
    fn read_eeprom_or_accel(&self, _addr: u16) -> u8 {
        // For now, return a fixed accelerometer value (neutral position)
        // Accelerometer reads typically come from specific registers within the $A000–$BFFF range
        // Both axes return 0x80 (center/neutral position)
        match self.eeprom_state {
            EepromState::Idle => {
                // Return accelerometer data (fixed neutral values)
                0x80
            }
            EepromState::ReadCommand => {
                // Return EEPROM data
                self.eeprom[self.eeprom_addr as usize]
            }
            EepromState::WriteCommand | EepromState::WriteData => {
                // Undefined during write operations
                0xFF
            }
        }
    }

    /// Write to EEPROM register ($4000–$5FFF).
    /// This implements a simplified EEPROM state machine.
    fn write_eeprom_register(&mut self, val: u8) {
        // Bits: 7=chip_select, 6=clock, 5=data_in, 0=unknown
        // Simplified implementation: treat writes as commands
        let cs = (val & 0x80) != 0;

        if !cs {
            // Chip not selected, reset state
            self.eeprom_state = EepromState::Idle;
            return;
        }

        // For simplicity, interpret val as a command
        // Actual EEPROM has a bit-serial protocol, but we'll support basic read/write
        match val {
            0x80..=0xFF => {
                // High bit set: treat as a read command
                // Bits 6–0 form an address (0–127, masked to 0–255 range)
                if (val & 0x40) != 0 {
                    // Read command
                    self.eeprom_addr = val & 0x7F;
                    self.eeprom_state = EepromState::ReadCommand;
                } else {
                    // Write command
                    self.eeprom_addr = val & 0x7F;
                    self.eeprom_state = EepromState::WriteCommand;
                }
            }
            0x00..=0x7F => {
                // Low bit clear: potential write data
                if self.eeprom_state == EepromState::WriteCommand {
                    self.eeprom_state = EepromState::WriteData;
                } else if self.eeprom_state == EepromState::WriteData {
                    // Commit write
                    self.eeprom[self.eeprom_addr as usize] = val;
                    self.eeprom_state = EepromState::Idle;
                }
            }
        }
    }

    fn write_registers(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.access_enabled = (val & 0x0F) == 0x0A;
            }
            0x2000..=0x3FFF => {
                self.rom_bank = val & 0x7F;
            }
            0x4000..=0x5FFF => {
                self.write_eeprom_register(val);
            }
            _ => {}
        }
    }

    fn write_eeprom_data(&mut self, addr: u16, val: u8) {
        if !self.access_enabled {
            return;
        }
        // Writes to $A000–$BFFF are treated as EEPROM data writes
        let offset = addr as usize - 0xA000;
        if offset < 256 {
            self.eeprom[offset] = val;
        }
    }
}

impl GbCartridge for Mbc7 {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.read_rom(addr),
            0xA000..=0xBFFF if self.access_enabled => self.read_eeprom_or_accel(addr),
            _ => 0xFF,
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x7FFF => self.write_registers(addr, val),
            0xA000..=0xBFFF => self.write_eeprom_data(addr, val),
            _ => {}
        }
    }

    fn mbc_state_snapshot(&self) -> Vec<u8> {
        let mut snapshot = vec![self.rom_bank, self.access_enabled as u8];
        snapshot.extend_from_slice(&self.eeprom);
        snapshot
    }

    fn restore_mbc_state(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.rom_bank = data[0];
            self.access_enabled = data[1] != 0;
        }
        if data.len() >= 2 + 256 {
            self.eeprom.copy_from_slice(&data[2..258]);
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

    #[test]
    fn test_rom_bank_selection() {
        let rom = make_rom(128 * 0x4000); // 128 banks
        let mut cart = Mbc7::new(rom);

        // Default bank 1
        assert_eq!(cart.effective_rom_bank(), 1);

        // Change to bank 0
        cart.write_registers(0x2000, 0x00);
        assert_eq!(cart.effective_rom_bank(), 0);

        // Change to bank 127
        cart.write_registers(0x2000, 0x7F);
        assert_eq!(cart.effective_rom_bank(), 127);

        // Bank wraps at available bank count
        cart.write_registers(0x2000, 0x80); // Bit 7 masked out
        assert_eq!(cart.effective_rom_bank(), 0);
    }

    #[test]
    fn test_access_enable_disable() {
        let rom = make_rom(2 * 0x4000);
        let mut cart = Mbc7::new(rom);

        // Access disabled by default
        assert!(!cart.access_enabled);
        assert_eq!(cart.read(0xA000), 0xFF);

        // Enable with 0x0A
        cart.write_registers(0x0000, 0x0A);
        assert!(cart.access_enabled);

        // Reads from EEPROM range now succeed
        let val = cart.read(0xA000);
        assert_ne!(val, 0xFF); // Should read something (accelerometer or EEPROM)

        // Disable with any other value
        cart.write_registers(0x0000, 0x00);
        assert!(!cart.access_enabled);
        assert_eq!(cart.read(0xA000), 0xFF);
    }

    #[test]
    fn test_eeprom_write_read() {
        let rom = make_rom(2 * 0x4000);
        let mut cart = Mbc7::new(rom);

        cart.write_registers(0x0000, 0x0A); // enable access

        // Write to EEPROM at address 0
        cart.write_eeprom_data(0xA000, 0x42);
        assert_eq!(cart.eeprom[0], 0x42);

        // Write to EEPROM at address 255
        cart.write_eeprom_data(0xA000 + 255, 0x99);
        assert_eq!(cart.eeprom[255], 0x99);
    }

    #[test]
    fn test_eeprom_data_persistence() {
        let rom = make_rom(2 * 0x4000);
        let mut cart = Mbc7::new(rom);

        cart.write_registers(0x0000, 0x0A);

        // Fill EEPROM with pattern
        for i in 0..256 {
            cart.write_eeprom_data(0xA000 + i as u16, (i as u8).wrapping_mul(3));
        }

        // Verify pattern
        for i in 0..256 {
            assert_eq!(cart.eeprom[i], (i as u8).wrapping_mul(3));
        }
    }

    #[test]
    fn test_accelerometer_neutral_values() {
        let rom = make_rom(2 * 0x4000);
        let cart = Mbc7::new(rom);

        // Accelerometer should return neutral values when idle
        assert_eq!(cart.read_eeprom_or_accel(0xA000), 0x80);
        assert_eq!(cart.read_eeprom_or_accel(0xA001), 0x80);
    }

    #[test]
    fn test_state_snapshot_restore() {
        let rom = make_rom(4 * 0x4000);
        let mut cart = Mbc7::new(rom.clone());

        // Set up state
        cart.write_registers(0x0000, 0x0A);
        cart.write_registers(0x2000, 0x10);
        cart.write_eeprom_data(0xA000, 0x42);
        cart.write_eeprom_data(0xA001, 0x99);

        // Snapshot
        let snapshot = cart.mbc_state_snapshot();

        // Create new cart and restore
        let mut cart2 = Mbc7::new(rom);
        cart2.restore_mbc_state(&snapshot);

        assert_eq!(cart2.rom_bank, 0x10);
        assert!(cart2.access_enabled);
        assert_eq!(cart2.eeprom[0], 0x42);
        assert_eq!(cart2.eeprom[1], 0x99);
    }

    #[test]
    fn test_rom_read() {
        let mut rom_data = make_rom(4 * 0x4000);
        // Mark first bank
        rom_data[0x0100] = 0xAA;
        // Mark second bank
        rom_data[0x4100] = 0xBB;
        // Mark third bank
        rom_data[0x8100] = 0xCC;

        let mut cart = Mbc7::new(rom_data);

        // Read from bank 0 (fixed)
        assert_eq!(cart.read(0x0100), 0xAA);

        // Default effective bank is 1
        assert_eq!(cart.read(0x4100), 0xBB);

        // Change to bank 2
        cart.write_registers(0x2000, 0x02);
        assert_eq!(cart.read(0x4100), 0xCC);
    }

    #[test]
    fn test_out_of_range_reads() {
        let rom = make_rom(2 * 0x4000);
        let cart = Mbc7::new(rom);

        // Reads outside ROM/RAM ranges should return 0xFF
        assert_eq!(cart.read(0x8000), 0xFF);
        assert_eq!(cart.read(0x9000), 0xFF);
        assert_eq!(cart.read(0xC000), 0xFF);
        assert_eq!(cart.read(0xE000), 0xFF);
    }

    #[test]
    fn test_disabled_writes() {
        let rom = make_rom(2 * 0x4000);
        let mut cart = Mbc7::new(rom);

        // EEPROM writes should be ignored when access is disabled
        cart.write_eeprom_data(0xA000, 0x42);
        assert_eq!(cart.eeprom[0], 0xFF); // Unchanged

        // Enable access
        cart.write_registers(0x0000, 0x0A);
        cart.write_eeprom_data(0xA000, 0x42);
        assert_eq!(cart.eeprom[0], 0x42); // Now it works
    }
}
