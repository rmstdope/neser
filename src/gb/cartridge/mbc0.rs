use super::cartridge::GbCartridge;

/// ROM-only cartridge (MBC type 0x00).
///
/// No bank switching. Bank 0 at $0000–$3FFF, bank 1 at $4000–$7FFF.
/// Writes to the ROM area are silently ignored.
pub struct Mbc0 {
    rom: Vec<u8>,
}

impl Mbc0 {
    pub fn new(rom: Vec<u8>) -> Self {
        Self { rom }
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
}
