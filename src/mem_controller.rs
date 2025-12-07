use crate::cartridge::Cartridge;
use crate::ppu_modules;
use std::cell::RefCell;
use std::rc::Rc;

/// NES Memory (64KB address space)
pub struct MemController {
    cpu_ram: Vec<u8>,
    prg_rom: Vec<u8>,
    ppu: Rc<RefCell<ppu_modules::PPUModular>>,
    oam_dma_page: Option<u8>, // Stores the page for pending OAM DMA
}

impl MemController {
    /// Create a new memory instance with 64KB of RAM initialized to 0
    pub fn new(ppu: Rc<RefCell<ppu_modules::PPUModular>>) -> Self {
        Self {
            cpu_ram: vec![0; 0x10000],
            prg_rom: Vec::new(),
            ppu,
            oam_dma_page: None,
        }
    }

    /// Map a cartridge into memory
    pub fn map_cartridge(&mut self, cartridge: Cartridge) {
        self.prg_rom = cartridge.prg_rom;
        let mut ppu = self.ppu.borrow_mut();
        ppu.load_chr_rom(cartridge.chr_rom);
        ppu.set_mirroring(cartridge.mirroring);
    }

    /// Read a byte from memory
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            // RAM ($0000-$1FFF) with mirroring
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x07FF) as usize],

            // PPU registers ($2000-$3FFF) with mirroring every 8 bytes
            0x2000..=0x3FFF => match addr & 0x2007 {
                0x2000 => panic!("Cannot read from write-only PPU register PPUCTRL (0x2000)"),
                0x2001 => panic!("Cannot read from write-only PPU register PPUMASK (0x2001)"),
                0x2002 => self.ppu.borrow_mut().get_status(),
                0x2003 => panic!("Cannot read from write-only PPU register OAMADDR (0x2003)"),
                0x2004 => self.ppu.borrow().read_oam_data(),
                0x2005 => panic!("Cannot read from write-only PPU register PPUSCROLL (0x2005)"),
                0x2006 => panic!("Cannot read from write-only PPU register PPUADDR (0x2006)"),
                0x2007 => self.ppu.borrow_mut().read_data(),
                _ => panic!("Should never happen!"),
            },

            // PRG ROM ($8000-$FFFF)
            0x8000..=0xFFFF => {
                let prg_size = self.prg_rom.len();
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
                    self.prg_rom[index]
                } else {
                    panic!("No PRG ROM mapped, cannot read from {:04X}", addr);
                }
            }

            // Everything else
            _ => {
                // eprintln!(
                //     "Warning: Read from unimplemented address {:04X}, returning 0",
                //     addr
                // );
                0
            }
        }
    }

    /// Write a byte to memory
    /// Returns true if an OAM DMA was triggered (at $4014)
    pub fn write(&mut self, addr: u16, value: u8) -> bool {
        match addr {
            // RAM ($0000-$1FFF) with mirroring
            0x0000..=0x1FFF => {
                self.cpu_ram[(addr & 0x07FF) as usize] = value;
            }

            // PPU registers ($2000-$3FFF) with mirroring every 8 bytes
            0x2000..=0x3FFF => match addr & 0x2007 {
                0x2000 => self.ppu.borrow_mut().write_control(value),
                0x2001 => self.ppu.borrow_mut().write_mask(value),
                0x2002 => panic!("Cannot write to read-only PPU register PPUSTATUS (0x2002)"),
                0x2003 => self.ppu.borrow_mut().write_oam_address(value),
                0x2004 => self.ppu.borrow_mut().write_oam_data(value),
                0x2005 => self.ppu.borrow_mut().write_scroll(value),
                0x2006 => self.ppu.borrow_mut().write_address(value),
                0x2007 => self.ppu.borrow_mut().write_data(value),
                _ => panic!("Should never happen!"),
            },

            // APU and I/O registers ($4000-$4017)
            0x4000..=0x4017 => match addr {
                0x4014 => {
                    // OAMDMA - Store the page for later DMA execution
                    self.oam_dma_page = Some(value);
                    return true; // Signal that OAM DMA should occur
                }
                _ => {
                    // eprintln!(
                    //     "Warning: Write to unimplemented APU/IO register {:04X} ignored",
                    //     addr
                    // );
                }
            },

            // PRG ROM ($8000-$FFFF) are read-only when ROM is loaded
            0x8000..=0xFFFF => {
                panic!("Cannot write to PRG ROM address {:04X}", addr);
            }

            // Everything else
            _ => {
                eprintln!(
                    "Warning: Write to unimplemented address {:04X} ignored",
                    addr
                );
            }
        }
        false // No DMA triggered
    }

    /// Read a 16-bit word from memory (little-endian)
    pub fn read_u16(&self, addr: u16) -> u16 {
        let lo = self.read(addr) as u16;
        let hi = self.read(addr.wrapping_add(1)) as u16;
        (hi << 8) | lo
    }

    /// Write a 16-bit word to memory (little-endian)
    #[cfg(test)]
    pub fn write_u16(&mut self, addr: u16, value: u16) {
        let lo = (value & 0xFF) as u8;
        let hi = (value >> 8) as u8;
        self.write(addr, lo);
        self.write(addr.wrapping_add(1), hi);
    }

    /// Check if an OAM DMA is pending and get the page value
    pub fn take_oam_dma_page(&mut self) -> Option<u8> {
        self.oam_dma_page.take()
    }

    /// Execute an OAM DMA transfer from the specified page to OAM
    /// Returns the number of bytes transferred (always 256)
    pub fn execute_oam_dma(&mut self, page: u8) {
        let source_page = (page as u16) << 8;
        for i in 0..256u16 {
            let byte = self.read(source_page + i);
            self.ppu.borrow_mut().write_oam_data(byte);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_memory() -> MemController {
        let ppu = Rc::new(RefCell::new(ppu_modules::PPUModular::new(crate::nes::TvSystem::Ntsc)));
        MemController::new(ppu)
    }

    #[test]
    fn test_new_memory_is_initialized() {
        let memory = create_test_memory();
        assert_eq!(memory.read(0x0000), 0);
        assert_eq!(memory.read(0x1234), 0);
        assert_eq!(memory.read(0x3FFF), 0);
    }

    #[test]
    fn test_write_and_read_byte() {
        let mut memory = create_test_memory();
        let dma = memory.write(0x1234, 0x42);
        assert_eq!(dma, false);
        assert_eq!(memory.read(0x1234), 0x42);
    }

    #[test]
    fn test_write_u16_little_endian() {
        let mut memory = create_test_memory();
        memory.write_u16(0x1234, 0xABCD);
        assert_eq!(memory.read(0x1234), 0xCD); // Low byte
        assert_eq!(memory.read(0x1235), 0xAB); // High byte
    }

    #[test]
    fn test_read_u16_little_endian() {
        let mut memory = create_test_memory();
        memory.write(0x1234, 0xCD); // Low byte
        memory.write(0x1235, 0xAB); // High byte
        let result = memory.read_u16(0x1234);
        assert_eq!(result, 0xABCD);
    }

    #[test]
    fn test_write_and_read_u16_round_trip() {
        let mut memory = create_test_memory();
        memory.write_u16(0x1000, 0x1234);
        let result = memory.read_u16(0x1000);
        assert_eq!(result, 0x1234);
    }

    #[test]
    fn test_ram_mirror_0800() {
        let mut memory = create_test_memory();
        memory.write(0x0000, 0x42);
        assert_eq!(memory.read(0x0800), 0x42);
        assert_eq!(memory.read(0x1000), 0x42);
        assert_eq!(memory.read(0x1800), 0x42);
    }

    #[test]
    fn test_ram_mirror_write_to_mirror() {
        let mut memory = create_test_memory();
        memory.write(0x0800, 0x55);
        assert_eq!(memory.read(0x0000), 0x55);
        assert_eq!(memory.read(0x1000), 0x55);
        assert_eq!(memory.read(0x1800), 0x55);
    }

    #[test]
    fn test_ram_mirror_different_addresses() {
        let mut memory = create_test_memory();
        memory.write(0x01FF, 0xAA);
        assert_eq!(memory.read(0x09FF), 0xAA);
        assert_eq!(memory.read(0x11FF), 0xAA);
        assert_eq!(memory.read(0x19FF), 0xAA);
    }

    #[test]
    fn test_cartridge_prg_rom_16kb_read() {
        use crate::cartridge::Cartridge;

        let mut memory = create_test_memory();

        // Create a simple 16KB PRG ROM cartridge
        let mut prg_rom = vec![0; 0x4000]; // 16KB
        prg_rom[0] = 0xAA; // First byte
        prg_rom[0x3FFF] = 0xBB; // Last byte of 16KB

        let cartridge = Cartridge {
            prg_rom,
            chr_rom: vec![],
            mirroring: crate::cartridge::MirroringMode::Horizontal,
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

        let mut memory = create_test_memory();

        // Create a 32KB PRG ROM cartridge
        let mut prg_rom = vec![0; 0x8000]; // 32KB
        prg_rom[0] = 0xAA; // First byte at $8000
        prg_rom[0x4000] = 0xCC; // First byte at $C000
        prg_rom[0x7FFF] = 0xDD; // Last byte at $FFFF

        let cartridge = Cartridge {
            prg_rom,
            chr_rom: vec![],
            mirroring: crate::cartridge::MirroringMode::Horizontal,
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
    fn test_ram_still_writable_with_cartridge() {
        use crate::cartridge::Cartridge;

        let mut memory = create_test_memory();

        let cartridge = Cartridge {
            prg_rom: vec![0; 0x4000],
            chr_rom: vec![],
            mirroring: crate::cartridge::MirroringMode::Horizontal,
        };

        memory.map_cartridge(cartridge);

        // RAM should still be writable
        memory.write(0x0000, 0x55);
        assert_eq!(memory.read(0x0000), 0x55);

        // Another RAM location should still be writable
        memory.write(0x0100, 0x66);
        assert_eq!(memory.read(0x0100), 0x66);
    }

    #[test]
    fn test_write_to_ppudata_writes_to_ppu() {
        let mut memory = create_test_memory();

        // Set PPU address to nametable ($2000)
        memory.write(0x2006, 0x20);
        memory.write(0x2006, 0x00);

        // Write data to PPUDATA register
        memory.write(0x2007, 0x42);

        // Verify the data was written to PPU memory by reading it back
        // Reset PPU address
        memory.write(0x2006, 0x20);
        memory.write(0x2006, 0x00);

        // Read from PPUDATA (first read returns buffer, second returns actual value)
        memory.read(0x2007); // Skip buffered read
        assert_eq!(memory.read(0x2007), 0x42);
    }

    #[test]
    fn test_write_to_oamaddr_sets_oam_address() {
        let mut memory = create_test_memory();

        // Write to OAMADDR register
        memory.write(0x2003, 0x42);

        // Verify by writing to OAMDATA and checking the address incremented
        memory.write(0x2004, 0xAA);
        memory.write(0x2004, 0xBB);

        // Reset OAM address and read back
        memory.write(0x2003, 0x42);
        assert_eq!(memory.read(0x2004), 0xAA);
        assert_eq!(memory.read(0x2004), 0xAA); // Reading doesn't increment
    }

    #[test]
    fn test_write_to_oamdata_writes_and_increments() {
        let mut memory = create_test_memory();

        // Set OAM address to 0
        memory.write(0x2003, 0x00);

        // Write sequence of values
        memory.write(0x2004, 0x11);
        memory.write(0x2004, 0x22);
        memory.write(0x2004, 0x33);

        // Reset OAM address and read back
        memory.write(0x2003, 0x00);
        assert_eq!(memory.read(0x2004), 0x11);

        memory.write(0x2003, 0x01);
        assert_eq!(memory.read(0x2004), 0x22);

        memory.write(0x2003, 0x02);
        assert_eq!(memory.read(0x2004), 0x33);
    }

    #[test]
    fn test_oamdata_write_wraps_at_256() {
        let mut memory = create_test_memory();

        // Set OAM address to 0xFF
        memory.write(0x2003, 0xFF);
        memory.write(0x2004, 0xAA);

        // Address should wrap to 0x00
        memory.write(0x2004, 0xBB);

        // Verify wrap
        memory.write(0x2003, 0xFF);
        assert_eq!(memory.read(0x2004), 0xAA);

        memory.write(0x2003, 0x00);
        assert_eq!(memory.read(0x2004), 0xBB);
    }

    #[test]
    fn test_read_from_oamdata_does_not_increment() {
        let mut memory = create_test_memory();

        // Set OAM address and write data
        memory.write(0x2003, 0x10);
        memory.write(0x2004, 0x88);

        // Reset address and read multiple times
        memory.write(0x2003, 0x10);
        assert_eq!(memory.read(0x2004), 0x88);
        assert_eq!(memory.read(0x2004), 0x88);
        assert_eq!(memory.read(0x2004), 0x88);
    }

    #[test]
    fn test_oam_full_sprite_write() {
        let mut memory = create_test_memory();

        // Write a full sprite (4 bytes) to OAM
        memory.write(0x2003, 0x00);
        memory.write(0x2004, 0x10); // Y position
        memory.write(0x2004, 0x20); // Tile index
        memory.write(0x2004, 0x30); // Attributes
        memory.write(0x2004, 0x40); // X position

        // Read back the sprite data
        memory.write(0x2003, 0x00);
        assert_eq!(memory.read(0x2004), 0x10);
        memory.write(0x2003, 0x01);
        assert_eq!(memory.read(0x2004), 0x20);
        memory.write(0x2003, 0x02);
        assert_eq!(memory.read(0x2004), 0x30);
        memory.write(0x2003, 0x03);
        assert_eq!(memory.read(0x2004), 0x40);
    }
}
