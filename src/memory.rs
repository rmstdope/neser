use crate::cartridge::Cartridge;

/// NES Memory (64KB address space)
pub struct Memory {
    data: Vec<u8>,
    cartridge: Option<Cartridge>,
}

impl Memory {
    /// Create a new memory instance with 64KB of RAM initialized to 0
    pub fn new() -> Self {
        Self {
            data: vec![0; 0x10000],
            cartridge: None,
        }
    }

    /// Map a cartridge into memory
    pub fn map_cartridge(&mut self, cartridge: Cartridge) {
        self.cartridge = Some(cartridge);
    }

    /// Map address to physical memory location accounting for mirroring
    fn map_address(&self, addr: u16) -> usize {
        match addr {
            // 2KB internal RAM ($0000-$07FF) mirrored 4 times up to $1FFF
            0x0000..=0x1FFF => (addr & 0x07FF) as usize,
            // PPU registers ($2000-$2007) mirrored up to $3FFF
            0x2000..=0x3FFF => (0x2000 + (addr & 0x0007)) as usize,
            // OK to write to the reset vector
            0xFFFC..=0xFFFF => addr as usize,
            // Everything else maps directly, but prints error
            _ => {
                eprintln!("Warning: Accessing unmapped memory address {:04X}", addr);
                addr as usize
            }
        }
    }

    /// Read a byte from memory
    pub fn read(&self, addr: u16) -> u8 {
        // Check if address is in PRG ROM range ($8000-$FFFF) and cartridge is mapped
        if addr >= 0x8000 {
            if let Some(ref cartridge) = self.cartridge {
                let prg_size = cartridge.prg_rom.len();
                if prg_size > 0 {
                    // Calculate offset into PRG ROM
                    let offset = (addr - 0x8000) as usize;
                    // Handle mirroring for 16KB ROMs (mirror at $C000)
                    let index = if prg_size == 0x4000 {
                        // 16KB ROM: mirror the same 16KB at both $8000 and $C000
                        offset % 0x4000
                    } else {
                        // 32KB or larger ROM: direct mapping
                        offset % prg_size
                    };
                    return cartridge.prg_rom[index];
                }
            }
        }

        self.data[self.map_address(addr)]
    }

    /// Write a byte to memory
    pub fn write(&mut self, addr: u16, value: u8) {
        // ROM addresses ($8000-$FFFF) are read-only when cartridge is mapped
        if addr >= 0x8000 && self.cartridge.is_some() {
            // Silently ignore writes to ROM
            return;
        }

        let mapped_addr = self.map_address(addr);
        self.data[mapped_addr] = value;
    }

    /// Read a 16-bit word from memory (little-endian)
    pub fn read_u16(&self, addr: u16) -> u16 {
        let lo = self.read(addr) as u16;
        let hi = self.read(addr.wrapping_add(1)) as u16;
        (hi << 8) | lo
    }

    /// Write a 16-bit word to memory (little-endian)
    pub fn write_u16(&mut self, addr: u16, value: u16) {
        let lo = (value & 0xFF) as u8;
        let hi = (value >> 8) as u8;
        self.write(addr, lo);
        self.write(addr.wrapping_add(1), hi);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_memory_is_zeroed() {
        let memory = Memory::new();
        assert_eq!(memory.read(0x0000), 0);
        assert_eq!(memory.read(0x1234), 0);
        assert_eq!(memory.read(0x3FFF), 0);
    }

    #[test]
    fn test_write_and_read_byte() {
        let mut memory = Memory::new();
        memory.write(0x1234, 0x42);
        assert_eq!(memory.read(0x1234), 0x42);
    }

    #[test]
    fn test_write_u16_little_endian() {
        let mut memory = Memory::new();
        memory.write_u16(0x1234, 0xABCD);
        assert_eq!(memory.read(0x1234), 0xCD); // Low byte
        assert_eq!(memory.read(0x1235), 0xAB); // High byte
    }

    #[test]
    fn test_read_u16_little_endian() {
        let mut memory = Memory::new();
        memory.write(0x1234, 0xCD); // Low byte
        memory.write(0x1235, 0xAB); // High byte
        let result = memory.read_u16(0x1234);
        assert_eq!(result, 0xABCD);
    }

    #[test]
    fn test_write_and_read_u16_round_trip() {
        let mut memory = Memory::new();
        memory.write_u16(0x3000, 0x1234);
        let result = memory.read_u16(0x3000);
        assert_eq!(result, 0x1234);
    }

    #[test]
    fn test_ram_mirror_0800() {
        let mut memory = Memory::new();
        memory.write(0x0000, 0x42);
        assert_eq!(memory.read(0x0800), 0x42);
        assert_eq!(memory.read(0x1000), 0x42);
        assert_eq!(memory.read(0x1800), 0x42);
    }

    #[test]
    fn test_ram_mirror_write_to_mirror() {
        let mut memory = Memory::new();
        memory.write(0x0800, 0x55);
        assert_eq!(memory.read(0x0000), 0x55);
        assert_eq!(memory.read(0x1000), 0x55);
        assert_eq!(memory.read(0x1800), 0x55);
    }

    #[test]
    fn test_ram_mirror_different_addresses() {
        let mut memory = Memory::new();
        memory.write(0x01FF, 0xAA);
        assert_eq!(memory.read(0x09FF), 0xAA);
        assert_eq!(memory.read(0x11FF), 0xAA);
        assert_eq!(memory.read(0x19FF), 0xAA);
    }

    #[test]
    fn test_ppu_register_mirror() {
        let mut memory = Memory::new();
        memory.write(0x2000, 0x33);
        assert_eq!(memory.read(0x2008), 0x33);
        assert_eq!(memory.read(0x2010), 0x33);
        assert_eq!(memory.read(0x3FF8), 0x33);
    }

    #[test]
    fn test_ppu_register_mirror_different_register() {
        let mut memory = Memory::new();
        memory.write(0x2007, 0x77);
        assert_eq!(memory.read(0x200F), 0x77);
        assert_eq!(memory.read(0x2017), 0x77);
        assert_eq!(memory.read(0x3FFF), 0x77);
    }

    #[test]
    fn test_cartridge_prg_rom_16kb_read() {
        use crate::cartridge::Cartridge;

        let mut memory = Memory::new();

        // Create a simple 16KB PRG ROM cartridge
        let mut prg_rom = vec![0; 0x4000]; // 16KB
        prg_rom[0] = 0xAA; // First byte
        prg_rom[0x3FFF] = 0xBB; // Last byte of 16KB

        let cartridge = Cartridge {
            prg_rom,
            chr_rom: vec![],
        };

        memory.map_cartridge(cartridge);

        // Read from $8000 (start of PRG ROM)
        assert_eq!(memory.read(0x8000), 0xAA);
        // Read from $BFFF (end of first 16KB)
        assert_eq!(memory.read(0xBFFF), 0xBB);
        // Read from $C000 (should mirror to $8000)
        assert_eq!(memory.read(0xC000), 0xAA);
        // Read from $FFFF (should mirror to $BFFF)
        assert_eq!(memory.read(0xFFFF), 0xBB);
    }

    #[test]
    fn test_cartridge_prg_rom_32kb_read() {
        use crate::cartridge::Cartridge;

        let mut memory = Memory::new();

        // Create a 32KB PRG ROM cartridge
        let mut prg_rom = vec![0; 0x8000]; // 32KB
        prg_rom[0] = 0xAA; // First byte at $8000
        prg_rom[0x4000] = 0xCC; // First byte at $C000
        prg_rom[0x7FFF] = 0xDD; // Last byte at $FFFF

        let cartridge = Cartridge {
            prg_rom,
            chr_rom: vec![],
        };

        memory.map_cartridge(cartridge);

        // Read from $8000
        assert_eq!(memory.read(0x8000), 0xAA);
        // Read from $C000 (different from $8000 in 32KB ROM)
        assert_eq!(memory.read(0xC000), 0xCC);
        // Read from $FFFF
        assert_eq!(memory.read(0xFFFF), 0xDD);
    }

    #[test]
    fn test_cartridge_rom_is_read_only() {
        use crate::cartridge::Cartridge;

        let mut memory = Memory::new();

        // Create a 16KB PRG ROM cartridge
        let prg_rom = vec![0x42; 0x4000]; // All bytes set to 0x42

        let cartridge = Cartridge {
            prg_rom,
            chr_rom: vec![],
        };

        memory.map_cartridge(cartridge);

        // Verify initial value
        assert_eq!(memory.read(0x8000), 0x42);

        // Try to write to ROM (should be ignored)
        memory.write(0x8000, 0x99);

        // Value should remain unchanged
        assert_eq!(memory.read(0x8000), 0x42);

        // Also test at $C000 (mirror)
        assert_eq!(memory.read(0xC000), 0x42);
        memory.write(0xC000, 0x88);
        assert_eq!(memory.read(0xC000), 0x42);
    }

    #[test]
    fn test_ram_still_writable_with_cartridge() {
        use crate::cartridge::Cartridge;

        let mut memory = Memory::new();

        let cartridge = Cartridge {
            prg_rom: vec![0; 0x4000],
            chr_rom: vec![],
        };

        memory.map_cartridge(cartridge);

        // RAM should still be writable
        memory.write(0x0000, 0x55);
        assert_eq!(memory.read(0x0000), 0x55);

        // PPU registers should still be writable
        memory.write(0x2000, 0x66);
        assert_eq!(memory.read(0x2000), 0x66);
    }
}
