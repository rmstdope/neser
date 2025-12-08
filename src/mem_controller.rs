use crate::cartridge::Cartridge;
use crate::ppu_modules;
use std::cell::RefCell;
use std::rc::Rc;

/// NES Memory (64KB address space)
pub struct MemController {
    cpu_ram: Vec<u8>,
    cartridge: Option<Cartridge>,
    ppu: Rc<RefCell<ppu_modules::PPUModular>>,
    oam_dma_page: Option<u8>, // Stores the page for pending OAM DMA
}

impl MemController {
    /// Create a new memory instance with 64KB of RAM initialized to 0
    pub fn new(ppu: Rc<RefCell<ppu_modules::PPUModular>>) -> Self {
        Self {
            cpu_ram: vec![0; 0x10000],
            cartridge: None,
            ppu,
            oam_dma_page: None,
        }
    }

    /// Map a cartridge into memory
    pub fn map_cartridge(&mut self, cartridge: Cartridge) {
        // Extract CHR data from mapper (8KB)
        let mut chr_data = Vec::with_capacity(8192);
        for addr in 0..8192 {
            chr_data.push(cartridge.mapper().read_chr(addr));
        }

        let mut ppu = self.ppu.borrow_mut();
        ppu.load_chr_rom(chr_data);
        ppu.set_mirroring(cartridge.mapper().get_mirroring());
        self.cartridge = Some(cartridge);
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

            // PRG-RAM ($6000-$7FFF)
            0x6000..=0x7FFF => {
                if let Some(ref cartridge) = self.cartridge {
                    cartridge.mapper().read_prg(addr)
                } else {
                    eprintln!(
                        "Warning: Read from PRG-RAM {:04X} without cartridge, returning 0",
                        addr
                    );
                    0
                }
            }

            // PRG ROM ($8000-$FFFF)
            0x8000..=0xFFFF => {
                if let Some(ref cartridge) = self.cartridge {
                    cartridge.mapper().read_prg(addr)
                } else {
                    panic!("No cartridge mapped, cannot read from {:04X}", addr);
                }
            }

            // Everything else
            _ => {
                eprintln!(
                    "Warning: Read from unimplemented address {:04X}, returning 0",
                    addr
                );
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
                    eprintln!(
                        "Warning: Write to unimplemented APU/IO register {:04X} ignored",
                        addr
                    );
                }
            },

            // PRG-RAM ($6000-$7FFF)
            0x6000..=0x7FFF => {
                if let Some(ref mut cartridge) = self.cartridge {
                    cartridge.mapper_mut().write_prg(addr, value);
                } else {
                    eprintln!(
                        "Warning: Write to PRG-RAM {:04X} without cartridge, ignored",
                        addr
                    );
                }
            }

            // PRG ROM area ($8000-$FFFF)
            // Writes here are typically mapper register writes, not ROM writes
            0x8000..=0xFFFF => {
                if let Some(ref mut cartridge) = self.cartridge {
                    cartridge.mapper_mut().write_prg(addr, value);
                } else {
                    eprintln!(
                        "Warning: Write to PRG ROM area {:04X} without cartridge, ignored",
                        addr
                    );
                }
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
        let ppu = Rc::new(RefCell::new(ppu_modules::PPUModular::new(
            crate::nes::TvSystem::Ntsc,
        )));
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

        let cartridge =
            Cartridge::from_parts(prg_rom, vec![], crate::cartridge::MirroringMode::Horizontal);

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

        let cartridge =
            Cartridge::from_parts(prg_rom, vec![], crate::cartridge::MirroringMode::Horizontal);

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

        let cartridge = Cartridge::from_parts(
            vec![0; 0x4000],
            vec![],
            crate::cartridge::MirroringMode::Horizontal,
        );

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

        // Write to OAMADDR register (use address 0x40 to avoid attribute byte)
        memory.write(0x2003, 0x40);

        // Verify by writing to OAMDATA and checking the address incremented
        memory.write(0x2004, 0xAA);
        memory.write(0x2004, 0xBB);

        // Reset OAM address and read back
        memory.write(0x2003, 0x40);
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
        // Attribute byte: 0x33 with masking = 0x33 & 0xE3 = 0x23
        assert_eq!(memory.read(0x2004), 0x23);
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
        memory.write(0x2004, 0xE3); // Attributes (valid value with all implemented bits set)
        memory.write(0x2004, 0x40); // X position

        // Read back the sprite data
        memory.write(0x2003, 0x00);
        assert_eq!(memory.read(0x2004), 0x10);
        memory.write(0x2003, 0x01);
        assert_eq!(memory.read(0x2004), 0x20);
        memory.write(0x2003, 0x02);
        assert_eq!(memory.read(0x2004), 0xE3);
        memory.write(0x2003, 0x03);
        assert_eq!(memory.read(0x2004), 0x40);
    }

    #[test]
    fn test_prg_ram_write_and_read() {
        // Test basic PRG-RAM read/write at $6000-$7FFF
        let mut memory = create_test_memory();

        // Load a simple NROM cartridge with PRG-RAM
        let rom_data = create_nrom_rom_with_prg_ram();
        let cartridge = Cartridge::new(&rom_data).expect("Failed to create cartridge");
        memory.map_cartridge(cartridge);

        // Write to PRG-RAM
        memory.write(0x6000, 0x42);
        memory.write(0x6001, 0x43);
        memory.write(0x7FFF, 0xFF);

        // Read back from PRG-RAM
        assert_eq!(
            memory.read(0x6000),
            0x42,
            "PRG-RAM at $6000 should return written value"
        );
        assert_eq!(
            memory.read(0x6001),
            0x43,
            "PRG-RAM at $6001 should return written value"
        );
        assert_eq!(
            memory.read(0x7FFF),
            0xFF,
            "PRG-RAM at $7FFF should return written value"
        );
    }

    #[test]
    fn test_prg_ram_persistence() {
        // Test that PRG-RAM persists across multiple reads
        let mut memory = create_test_memory();

        let rom_data = create_nrom_rom_with_prg_ram();
        let cartridge = Cartridge::new(&rom_data).expect("Failed to create cartridge");
        memory.map_cartridge(cartridge);

        memory.write(0x6100, 0xAB);

        // Multiple reads should return the same value
        assert_eq!(memory.read(0x6100), 0xAB);
        assert_eq!(memory.read(0x6100), 0xAB);
        assert_eq!(memory.read(0x6100), 0xAB);
    }

    #[test]
    fn test_prg_ram_8kb_size() {
        // Test that PRG-RAM is 8KB ($6000-$7FFF = 8192 bytes)
        let mut memory = create_test_memory();

        let rom_data = create_nrom_rom_with_prg_ram();
        let cartridge = Cartridge::new(&rom_data).expect("Failed to create cartridge");
        memory.map_cartridge(cartridge);

        // Write to first and last byte of 8KB range
        memory.write(0x6000, 0x01);
        memory.write(0x7FFF, 0xFF);

        assert_eq!(memory.read(0x6000), 0x01);
        assert_eq!(memory.read(0x7FFF), 0xFF);

        // They should be different addresses (not mirrored)
        assert_ne!(memory.read(0x6000), memory.read(0x7FFF));
    }

    #[test]
    fn test_prg_ram_initialized_to_zero() {
        // Test that PRG-RAM starts with all zeros
        let mut memory = create_test_memory();

        let rom_data = create_nrom_rom_with_prg_ram();
        let cartridge = Cartridge::new(&rom_data).expect("Failed to create cartridge");
        memory.map_cartridge(cartridge);

        // Check various addresses are initialized to 0
        assert_eq!(memory.read(0x6000), 0x00);
        assert_eq!(memory.read(0x6100), 0x00);
        assert_eq!(memory.read(0x7000), 0x00);
        assert_eq!(memory.read(0x7FFF), 0x00);
    }

    /// Helper function to create a minimal NROM ROM with PRG-RAM support
    fn create_nrom_rom_with_prg_ram() -> Vec<u8> {
        let mut rom = Vec::new();

        // iNES header
        rom.extend_from_slice(b"NES\x1A"); // Signature
        rom.push(2); // 2 * 16KB PRG ROM
        rom.push(1); // 1 * 8KB CHR ROM
        rom.push(0x02); // Flags 6: Battery-backed PRG-RAM present (bit 1)
        rom.push(0x00); // Flags 7: Mapper 0 (NROM)
        rom.extend_from_slice(&[0; 8]); // Unused padding

        // 32KB PRG ROM (2 * 16KB) - filled with NOPs
        rom.extend_from_slice(&[0xEA; 32768]);

        // 8KB CHR ROM - filled with zeros
        rom.extend_from_slice(&[0x00; 8192]);

        rom
    }
}
