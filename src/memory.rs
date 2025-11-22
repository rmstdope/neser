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

    /// Read a byte from memory
    pub fn read(&self, addr: u16) -> u8 {
        self.data[addr as usize]
    }

    /// Write a byte to memory
    pub fn write(&mut self, addr: u16, value: u8) {
        self.data[addr as usize] = value;
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
        assert_eq!(memory.read(0xFFFF), 0);
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
        memory.write_u16(0x5000, 0x1234);
        let result = memory.read_u16(0x5000);
        assert_eq!(result, 0x1234);
    }
}
