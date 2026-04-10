/// Game Boy cartridge interface.
///
/// Each cartridge type (MBC0, MBC1, …) implements this trait.
/// The `DmgBus` holds a `Box<dyn GbCartridge>` and delegates ROM/RAM
/// accesses to it.
pub trait GbCartridge {
    /// Read a byte from the cartridge address space.
    ///
    /// Addresses $0000–$7FFF map to ROM; $A000–$BFFF map to cartridge RAM.
    /// Reads outside those ranges return 0xFF.
    fn read(&self, addr: u16) -> u8;

    /// Write a byte to the cartridge address space.
    ///
    /// Writes to ROM ($0000–$7FFF) are interpreted as MBC register writes.
    /// Writes to cartridge RAM ($A000–$BFFF) store data when RAM is enabled.
    fn write(&mut self, addr: u16, val: u8);
}
