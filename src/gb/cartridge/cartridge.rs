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

    /// Returns `true` when the ROM header indicates CGB compatibility.
    ///
    /// Checks byte 0x0143: values 0x80 (CGB+DMG) or 0xC0 (CGB-only)
    /// indicate a CGB-compatible cartridge. This gates CGB-specific
    /// hardware behavior (e.g., APU length counter rules).
    fn is_cgb(&self) -> bool {
        let flag = self.read(0x0143);
        flag == 0x80 || flag == 0xC0
    }

    /// Capture cartridge state (mapper registers + RAM) as opaque bytes.
    fn save_state(&self) -> Vec<u8> {
        vec![]
    }

    /// Restore cartridge state from previously saved bytes.
    fn load_state(&mut self, _data: &[u8]) -> Result<(), String> {
        Ok(())
    }
}
