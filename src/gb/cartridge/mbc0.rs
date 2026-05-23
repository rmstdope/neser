use super::cartridge::GbCartridge;

/// ROM-only cartridge (MBC type 0x00).
///
/// No bank switching. Bank 0 at $0000–$3FFF, bank 1 at $4000–$7FFF.
/// Writes to the ROM area are silently ignored.
pub struct Mbc0 {
    rom: Vec<u8>,
}

/// ROM+RAM cartridge (types 0x08 and 0x09).
///
/// Pan Docs lists these cartridge types but notes that exact licensed-hardware
/// behavior is unknown. This implements the convention expected by daid's
/// `rom_and_ram.gb`: fixed 32 KiB ROM, external RAM gated by the standard
/// low-nibble `0x0A` enable write, and RAM addressing wrapped to the fitted
/// RAM size.
pub struct RomRam {
    rom: Vec<u8>,
    ram: Vec<u8>,
    ram_enabled: bool,
    battery: bool,
}

impl Mbc0 {
    pub fn new(rom: Vec<u8>) -> Self {
        Self { rom }
    }
}

impl RomRam {
    pub fn new(rom: Vec<u8>, ram: Vec<u8>, battery: bool) -> Self {
        Self {
            rom,
            ram,
            ram_enabled: false,
            battery,
        }
    }

    fn read_ram(&self, addr: u16) -> u8 {
        if !self.ram_enabled || self.ram.is_empty() {
            return 0xFF;
        }

        let offset = (addr as usize - 0xA000) % self.ram.len();
        self.ram[offset]
    }

    fn write_ram(&mut self, addr: u16, val: u8) {
        if !self.ram_enabled || self.ram.is_empty() {
            return;
        }

        let offset = (addr as usize - 0xA000) % self.ram.len();
        self.ram[offset] = val;
    }
}

impl GbCartridge for Mbc0 {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.rom.get(addr as usize).copied().unwrap_or(0xFF),
            _ => 0xFF,
        }
    }

    fn write(&mut self, _addr: u16, _val: u8) {
        // ROM-only: writes are silently ignored.
    }
}

impl GbCartridge for RomRam {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.rom.get(addr as usize).copied().unwrap_or(0xFF),
            0xA000..=0xBFFF => self.read_ram(addr),
            _ => 0xFF,
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.ram_enabled = (val & 0x0F) == 0x0A;
            }
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
        vec![self.ram_enabled as u8]
    }

    fn restore_mbc_state(&mut self, data: &[u8]) {
        if let Some(&ram_enabled) = data.first() {
            self.ram_enabled = ram_enabled != 0;
        }
    }

    fn has_battery(&self) -> bool {
        self.battery
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a 32 KB ROM: bank 0 ($0000–$3FFF) filled with 0x01,
    /// bank 1 ($4000–$7FFF) filled with 0x02.
    fn make_mbc0_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0000..0x4000].fill(0x01);
        rom[0x4000..0x8000].fill(0x02);
        rom
    }

    #[test]
    fn test_mbc0_reads_bank0_from_low_region() {
        let cart = Mbc0::new(make_mbc0_rom());
        assert_eq!(cart.read(0x0000), 0x01);
        assert_eq!(cart.read(0x3FFF), 0x01);
    }

    #[test]
    fn test_mbc0_reads_bank1_from_high_region() {
        let cart = Mbc0::new(make_mbc0_rom());
        assert_eq!(cart.read(0x4000), 0x02);
        assert_eq!(cart.read(0x7FFF), 0x02);
    }

    #[test]
    fn test_mbc0_writes_to_rom_are_ignored() {
        let mut cart = Mbc0::new(make_mbc0_rom());
        cart.write(0x0000, 0xFF);
        assert_eq!(cart.read(0x0000), 0x01);
    }

    #[test]
    fn test_mbc0_reads_outside_rom_return_0xff() {
        let cart = Mbc0::new(make_mbc0_rom());
        assert_eq!(cart.read(0x8000), 0xFF);
        assert_eq!(cart.read(0xA000), 0xFF);
    }

    #[test]
    fn test_rom_ram_requires_enable_before_external_ram_writes() {
        let mut cart = RomRam::new(make_mbc0_rom(), vec![0; 2 * 1024], false);

        cart.write(0xA000, 0x42);
        assert_eq!(cart.read(0xA000), 0xFF);

        cart.write(0x0000, 0x0A);
        cart.write(0xA000, 0x42);
        assert_eq!(cart.read(0xA000), 0x42);
    }

    #[test]
    fn test_rom_ram_disable_hides_external_ram() {
        let mut cart = RomRam::new(make_mbc0_rom(), vec![0; 2 * 1024], false);

        cart.write(0x0000, 0x0A);
        cart.write(0xA000, 0x42);
        cart.write(0x0000, 0x00);

        assert_eq!(cart.read(0xA000), 0xFF);
    }

    #[test]
    fn test_rom_ram_wraps_external_ram_to_available_size() {
        let mut cart = RomRam::new(make_mbc0_rom(), vec![0; 2 * 1024], false);

        cart.write(0x0000, 0x0A);
        cart.write(0xA000, 0x12);
        cart.write(0xA800, 0x34);

        assert_eq!(cart.read(0xA000), 0x34);
        assert_eq!(cart.read(0xA800), 0x34);
    }

    #[test]
    fn test_rom_ram_battery_flag_reflects_cartridge_type() {
        let cart = RomRam::new(make_mbc0_rom(), vec![0; 2 * 1024], true);

        assert!(cart.has_battery());
    }
}
