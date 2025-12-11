use crate::apu;
use crate::cartridge::Cartridge;
use crate::input::Joypad;
use crate::ppu;
use std::cell::RefCell;
use std::rc::Rc;

/// NES Memory (64KB address space)
pub struct MemController {
    cpu_ram: Vec<u8>,
    cartridge: Option<Cartridge>,
    ppu: Rc<RefCell<ppu::Ppu>>,
    apu: Rc<RefCell<apu::Apu>>,
    oam_dma_page: Option<u8>, // Stores the page for pending OAM DMA
    joypad1: RefCell<Joypad>,
    joypad2: RefCell<Joypad>,
    open_bus: RefCell<u8>, // Last value on the data bus for open bus behavior
}

impl MemController {
    /// Create a new memory instance with 64KB of RAM initialized to 0
    pub fn new(ppu: Rc<RefCell<ppu::Ppu>>, apu: Rc<RefCell<apu::Apu>>) -> Self {
        Self {
            cpu_ram: vec![0; 0x10000],
            cartridge: None,
            ppu,
            apu,
            oam_dma_page: None,
            joypad1: RefCell::new(Joypad::new()),
            joypad2: RefCell::new(Joypad::new()),
            open_bus: RefCell::new(0xFF), // Initialize to 0xFF (common power-on state)
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
        let value = match addr {
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

            // APU registers ($4000-$4017)
            // Most APU registers are write-only and return open bus when read
            0x4000..=0x4013 => *self.open_bus.borrow(), // APU write-only registers
            0x4014 => *self.open_bus.borrow(),          // OAM DMA (write-only)
            0x4015 => self.apu.borrow_mut().read_status(*self.open_bus.borrow()),
            0x4016 => self.joypad1.borrow_mut().read(),
            0x4017 => self.joypad2.borrow_mut().read(),

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
        };

        // Update open bus with the value read
        *self.open_bus.borrow_mut() = value;
        value
    }

    /// Write a byte to memory
    /// Returns true if an OAM DMA was triggered (at $4014)
    pub fn write(&mut self, addr: u16, value: u8) -> bool {
        // Update open bus with the value being written
        *self.open_bus.borrow_mut() = value;

        match addr {
            // RAM ($0000-$1FFF) with mirroring
            0x0000..=0x1FFF => {
                self.cpu_ram[(addr & 0x07FF) as usize] = value;
            }

            // PPU registers ($2000-$3FFF) with mirroring every 8 bytes
            0x2000..=0x3FFF => match addr & 0x2007 {
                0x2000 => self.ppu.borrow_mut().write_control(value),
                0x2001 => self.ppu.borrow_mut().write_mask(value),
                0x2002 => { /* PPUSTATUS is read-only, ignore writes */ }
                0x2003 => self.ppu.borrow_mut().write_oam_address(value),
                0x2004 => self.ppu.borrow_mut().write_oam_data(value),
                0x2005 => self.ppu.borrow_mut().write_scroll(value),
                0x2006 => self.ppu.borrow_mut().write_address(value),
                0x2007 => self.ppu.borrow_mut().write_data(value),
                _ => panic!("Should never happen!"),
            },

            // APU and I/O registers ($4000-$4017)
            0x4000..=0x4017 => match addr {
                // Pulse 1 registers
                0x4000 => self.apu.borrow_mut().pulse1_mut().write_control(value),
                0x4001 => self.apu.borrow_mut().pulse1_mut().write_sweep(value),
                0x4002 => self.apu.borrow_mut().pulse1_mut().write_timer_low(value),
                0x4003 => self
                    .apu
                    .borrow_mut()
                    .pulse1_mut()
                    .write_length_counter_timer_high(value),

                // Pulse 2 registers
                0x4004 => self.apu.borrow_mut().pulse2_mut().write_control(value),
                0x4005 => self.apu.borrow_mut().pulse2_mut().write_sweep(value),
                0x4006 => self.apu.borrow_mut().pulse2_mut().write_timer_low(value),
                0x4007 => self
                    .apu
                    .borrow_mut()
                    .pulse2_mut()
                    .write_length_counter_timer_high(value),

                // Triangle registers
                0x4008 => self
                    .apu
                    .borrow_mut()
                    .triangle_mut()
                    .write_linear_counter(value),
                0x400A => self.apu.borrow_mut().triangle_mut().write_timer_low(value),
                0x400B => self
                    .apu
                    .borrow_mut()
                    .triangle_mut()
                    .write_length_counter_timer_high(value),

                // Noise registers
                0x400C => self.apu.borrow_mut().noise_mut().write_envelope(value),
                0x400E => self.apu.borrow_mut().noise_mut().write_period(value),
                0x400F => self.apu.borrow_mut().noise_mut().write_length(value),

                // DMC registers
                0x4010 => self.apu.borrow_mut().dmc_mut().write_flags_and_rate(value),
                0x4011 => self.apu.borrow_mut().dmc_mut().write_direct_load(value),
                0x4012 => self.apu.borrow_mut().dmc_mut().write_sample_address(value),
                0x4013 => self.apu.borrow_mut().dmc_mut().write_sample_length(value),

                0x4014 => {
                    // OAMDMA - Store the page for later DMA execution
                    self.oam_dma_page = Some(value);
                    return true; // Signal that OAM DMA should occur
                }

                // APU Control registers
                0x4015 => self.apu.borrow_mut().write_enable(value),
                0x4016 => {
                    // Controller strobe - write to both controllers
                    self.joypad1.borrow_mut().write_strobe(value);
                    self.joypad2.borrow_mut().write_strobe(value);
                }
                0x4017 => self.apu.borrow_mut().write_frame_counter(value),

                // Unused APU registers
                0x4009 | 0x400D => {
                    // $4009 is unused (triangle register 1)
                    // $400D is unused (noise register 1)
                    // Writes to these addresses have no effect
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
                if addr == 0x6000 {
                    // For debugging, print writes to $6000
                    println!("Debug: Write to $6000 PRG-RAM: {:02X}", value);
                }
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

    /// Set button state for a controller
    pub fn set_button(&mut self, controller: u8, button: crate::input::Button, pressed: bool) {
        match controller {
            1 => self.joypad1.borrow_mut().set_button(button, pressed),
            2 => self.joypad2.borrow_mut().set_button(button, pressed),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_memory() -> MemController {
        let ppu = Rc::new(RefCell::new(ppu::Ppu::new(crate::nes::TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(apu::Apu::new()));
        MemController::new(ppu, apu)
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

    #[test]
    fn test_read_apu_status_register() {
        // Test reading from $4015 returns APU status
        let memory = create_test_memory();

        // Reading $4015 should return the APU status register
        let status = memory.read(0x4015);

        // Initially all channels should be disabled, so status should be 0
        // except for bit 5 which returns the current open bus value (0xFF at power-on)
        assert_eq!(status & 0b1101_1111, 0x00); // Mask out bit 5 (open bus)
        assert_eq!(status & 0b0010_0000, 0x20); // Bit 5 should be set from open bus
    }

    #[test]
    fn test_read_apu_status_after_enable() {
        // Test that reading $4015 returns the APU's status
        let memory = create_test_memory();

        // Directly configure pulse 1 through the APU to test reading
        {
            let mut apu = memory.apu.borrow_mut();
            apu.write_enable(0b0000_0001); // Enable pulse 1
            // Set length counter to non-zero by writing to register 3
            apu.pulse1_mut()
                .write_length_counter_timer_high(0b1111_1000);
        }

        // Read status through memory controller - pulse 1 bit should be set
        let status = memory.read(0x4015);
        assert_eq!(status & 0b0000_0001, 0b0000_0001);
    }

    #[test]
    fn test_apu_status_register_mirrored() {
        // Test that $4015 is not mirrored (only accessible at exact address)
        let memory = create_test_memory();

        // Reading exactly $4015 should work
        let status = memory.read(0x4015);
        // Bit 5 is open bus, so mask it out
        assert_eq!(status & 0b1101_1111, 0x00);

        // Note: $4015 is not mirrored, so other addresses in APU range
        // should not return the status register
    }

    #[test]
    fn test_write_pulse1_registers() {
        // Test writing to pulse 1 registers ($4000-$4003)
        let mut memory = create_test_memory();

        // Enable pulse 1 first
        memory.write(0x4015, 0b00000001);

        // Write to $4000 (control register)
        memory.write(0x4000, 0b10111111);

        // Write to $4001 (sweep register)
        memory.write(0x4001, 0b10101010);

        // Write to $4002 (timer low)
        memory.write(0x4002, 0xAB);

        // Write to $4003 (length/timer high)
        memory.write(0x4003, 0b11111000);

        // Verify writes reached the APU by checking pulse1 length counter
        let apu = memory.apu.borrow();
        assert!(apu.pulse1().get_length_counter() > 0);
    }

    #[test]
    fn test_write_pulse2_registers() {
        // Test writing to pulse 2 registers ($4004-$4007)
        let mut memory = create_test_memory();

        // Enable pulse 2 first
        memory.write(0x4015, 0b00000010);

        // Write to $4004 (control register)
        memory.write(0x4004, 0b11001111);

        // Write to $4007 (length/timer high)
        memory.write(0x4007, 0b11110000);

        // Verify writes reached the APU
        let apu = memory.apu.borrow();
        assert!(apu.pulse2().get_length_counter() > 0);
    }

    #[test]
    fn test_write_triangle_registers() {
        // Test writing to triangle registers ($4008-$400B)
        let mut memory = create_test_memory();

        // Enable triangle first
        memory.write(0x4015, 0b00000100);

        // Write to $4008 (linear counter)
        memory.write(0x4008, 0b11111111);

        // Write to $400B (length/timer high)
        memory.write(0x400B, 0b11110000);

        // Verify writes reached the APU
        let apu = memory.apu.borrow();
        assert!(apu.triangle().get_length_counter() > 0);
    }

    #[test]
    fn test_write_noise_registers() {
        // Test writing to noise registers ($400C-$400F)
        let mut memory = create_test_memory();

        // Enable noise first
        memory.write(0x4015, 0b00001000);

        // Write to $400C (control)
        memory.write(0x400C, 0b00111111);

        // Write to $400F (length counter load)
        memory.write(0x400F, 0b11110000);

        // Verify writes reached the APU
        let apu = memory.apu.borrow();
        assert!(apu.noise().get_length_counter() > 0);
    }

    #[test]
    fn test_write_dmc_registers() {
        // Test writing to DMC registers ($4010-$4013)
        let mut memory = create_test_memory();

        // Write to $4010 (flags and rate)
        memory.write(0x4010, 0b00001111);

        // Write to $4011 (direct load)
        memory.write(0x4011, 0x40);

        // Write to $4012 (sample address)
        memory.write(0x4012, 0xC0);

        // Write to $4013 (sample length)
        memory.write(0x4013, 0xFF);

        // Verify write reached the APU (no panic means success)
    }

    #[test]
    fn test_write_apu_enable_register() {
        // Test writing to $4015 (enable register)
        let mut memory = create_test_memory();

        // Enable pulse 1 and pulse 2
        memory.write(0x4015, 0b00000011);

        // Write length counters to make them non-zero
        memory.write(0x4003, 0b11110000);
        memory.write(0x4007, 0b11110000);

        // Read status to verify both are enabled
        let status = memory.read(0x4015);
        assert_eq!(status & 0b00000011, 0b00000011);
    }

    #[test]
    fn test_write_frame_counter_register() {
        // Test writing to $4017 (frame counter)
        let mut memory = create_test_memory();

        // Write to frame counter register - 5-step mode (bit 7 set)
        memory.write(0x4017, 0b10000000);

        // Verify write reached the APU
        let apu = memory.apu.borrow();
        assert_eq!(apu.frame_counter().get_mode(), true);
    }
}
