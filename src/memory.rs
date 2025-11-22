/// NES Memory (64KB address space)
pub struct Memory {
    data: Vec<u8>,
}

impl Memory {
    /// Create a new memory instance with 64KB of RAM initialized to 0
    pub fn new() -> Self {
        Self {
            data: vec![0; 0x10000],
        }
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
        self.data[self.map_address(addr)]
    }

    /// Write a byte to memory
    pub fn write(&mut self, addr: u16, value: u8) {
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
}
