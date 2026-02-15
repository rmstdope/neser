use crate::cartridge::{Cartridge, MirroringMode};
use std::cell::RefCell;
use std::rc::Rc;

#[allow(dead_code)]
const DEFAULT_PALETTE_RAM: [u8; 32] = [
    0x09, 0x01, 0x00, 0x01, 0x00, 0x02, 0x02, 0x0D, 0x08, 0x10, 0x08, 0x24, 0x00, 0x00, 0x04, 0x2C,
    0x09, 0x01, 0x34, 0x03, 0x00, 0x04, 0x00, 0x14, 0x08, 0x3A, 0x00, 0x02, 0x00, 0x20, 0x2C, 0x08,
];
/// Manages PPU memory including VRAM, palette RAM, and CHR ROM
pub struct Memory {
    /// Nametables - 4KB (supports all four nametables for FourScreen mode)
    ppu_ram: [u8; 4096],
    /// Palette RAM - 32 bytes
    palette_ram: [u8; 32],
    /// Cached palette entry (mirrored index) for hot-path reads
    last_palette_index: Option<u8>,
    /// Cached palette value for last_palette_index
    last_palette_value: u8,
    /// Mirroring mode
    mirroring_mode: MirroringMode,
}

impl Default for Memory {
    fn default() -> Self {
        // Default to Zero mode for tests that don't specify
        Self::new(crate::console::RamInitMode::Zero)
    }
}

impl Memory {
    /// Create a new Memory instance
    pub fn new(ram_init_mode: crate::console::RamInitMode) -> Self {
        let mut ppu_ram = [0u8; 4096];
        let mut palette_ram = [0u8; 32];

        // Initialize nametable RAM based on mode
        crate::console::initialize_ram(&mut ppu_ram, ram_init_mode);

        // Initialize palette RAM based on mode
        crate::console::initialize_ram(&mut palette_ram, ram_init_mode);

        Self {
            ppu_ram,
            palette_ram,
            last_palette_index: None,
            last_palette_value: 0,
            mirroring_mode: MirroringMode::Horizontal,
        }
    }

    /// Reset memory-related state (cache only).
    ///
    /// This method is called by Ppu::reset() which is invoked on both hard and soft resets.
    /// It only clears the palette cache - RAM contents are NOT modified here.
    /// RAM is initialized on power-on (Memory::new) and re-initialized on hard reset
    /// only (via reinitialize()). Soft resets preserve RAM.
    pub fn reset(&mut self) {
        // Only clear the cache - do NOT clear RAM
        self.last_palette_index = None;
        self.last_palette_value = 0;
    }

    /// Re-initialize RAM (nametable and palette) based on the given mode.
    ///
    /// This should be called on hard reset only. Soft resets preserve RAM contents.
    pub fn reinitialize(&mut self, mode: crate::console::RamInitMode) {
        // Initialize nametable RAM
        crate::console::initialize_ram(&mut self.ppu_ram, mode);

        // Initialize palette RAM
        crate::console::initialize_ram(&mut self.palette_ram, mode);

        // Clear cache
        self.last_palette_index = None;
        self.last_palette_value = 0;
    }

    /// Set mirroring mode
    pub fn set_mirroring(&mut self, mirroring: MirroringMode) {
        self.mirroring_mode = mirroring;
    }

    /// Read from CHR ROM/RAM at the specified address through the mapper
    ///
    /// CHR (Character ROM/RAM) contains pattern tables for tiles and sprites.
    /// The address is masked to 13 bits (0x0000-0x1FFF, 8KB range).
    ///
    /// This method routes the read through the cartridge mapper, which allows:
    /// - Bank switching (e.g., CNROM, MMC1, MMC3)
    /// - CHR-RAM support for mappers with writable pattern tables
    /// - Proper hardware-accurate memory access
    ///
    /// Returns 0 if no cartridge is loaded.
    pub fn read_chr(&self, addr: u16, cartridge: &Option<Rc<RefCell<Cartridge>>>) -> u8 {
        let masked_addr = addr & 0x1FFF;
        if let Some(cart) = cartridge {
            let mut cart = cart.borrow_mut();
            let mapper = cart.mapper_mut();
            mapper.ppu_address_changed(masked_addr);
            mapper.read_chr(masked_addr)
        } else {
            0 // No cartridge loaded, return 0
        }
    }

    /// Write to CHR memory at the specified address through the mapper
    ///
    /// This method routes the write through the cartridge mapper.
    /// The mapper determines whether the write is allowed:
    /// - CHR-RAM: Write is performed
    /// - CHR-ROM: Write is typically ignored (hardware characteristic)
    ///
    /// The address is masked to 13 bits (0x0000-0x1FFF, 8KB range).
    /// No-op if no cartridge is loaded.
    pub fn write_chr(&mut self, addr: u16, value: u8, cartridge: &Option<Rc<RefCell<Cartridge>>>) {
        let masked_addr = addr & 0x1FFF;
        if let Some(cart) = cartridge {
            let mut cart = cart.borrow_mut();
            let mapper = cart.mapper_mut();
            mapper.ppu_address_changed(masked_addr);
            mapper.write_chr(masked_addr, value);
        }
    }

    /// Read from nametable at the specified address (with mirroring)
    pub fn read_nametable(&self, addr: u16) -> u8 {
        let mirrored = self.mirror_vram_address(addr);
        self.ppu_ram[mirrored as usize]
    }

    /// Read from nametable, allowing the cartridge mapper to override the value.
    ///
    /// This is needed for mappers that provide alternative nametable sources (e.g., MMC5 ExRAM/fill).
    ///
    pub fn read_nametable_mapped(
        &self,
        addr: u16,
        cartridge: &Option<Rc<RefCell<Cartridge>>>,
    ) -> u8 {
        let masked_addr = addr & 0x2FFF;
        debug_assert!(
            masked_addr >= 0x2000,
            "nametable reads must be in $2000-$2FFF after mirroring"
        );

        if let Some(cart) = cartridge {
            let mut cart = cart.borrow_mut();
            let mapper = cart.mapper_mut();
            if let Some(value) = mapper.read_nametable(masked_addr) {
                return value;
            }
        }

        self.read_nametable(masked_addr)
    }

    /// Write to nametable at the specified address (with mirroring)
    pub fn write_nametable(&mut self, addr: u16, value: u8) {
        let mirrored = self.mirror_vram_address(addr);
        self.ppu_ram[mirrored as usize] = value;
    }

    /// Write to nametable, allowing the cartridge mapper to handle the write.
    ///
    pub fn write_nametable_mapped(
        &mut self,
        addr: u16,
        value: u8,
        cartridge: &Option<Rc<RefCell<Cartridge>>>,
    ) {
        let masked_addr = addr & 0x2FFF;
        debug_assert!(
            masked_addr >= 0x2000,
            "nametable writes must be in $2000-$2FFF after mirroring"
        );

        if let Some(cart) = cartridge {
            let mut cart = cart.borrow_mut();
            let mapper = cart.mapper_mut();
            if mapper.write_nametable(masked_addr, value) {
                return;
            }
        }

        self.write_nametable(masked_addr, value);
    }

    /// Read from palette at the specified address (with mirroring)
    pub fn read_palette(&mut self, addr: u16) -> u8 {
        let mirrored = self.mirror_palette_address(addr) as u8;
        if self.last_palette_index == Some(mirrored) {
            return self.last_palette_value;
        }

        let value = self.palette_ram[mirrored as usize];
        self.last_palette_index = Some(mirrored);
        self.last_palette_value = value;
        value
    }

    /// Write to palette at the specified address (with mirroring)
    /// Palette RAM only stores 6 bits (0-5), bits 6-7 are ignored
    pub fn write_palette(&mut self, addr: u16, value: u8) {
        let mirrored = self.mirror_palette_address(addr);
        let masked = value & 0x3F; // Only store bits 5-0
        self.palette_ram[mirrored] = masked;
        if self.last_palette_index == Some(mirrored as u8) {
            self.last_palette_value = masked;
        }
    }

    /// Mirror VRAM address based on nametable mirroring mode
    fn mirror_vram_address(&self, addr: u16) -> u16 {
        // Mirror down $3000-$3EFF to the range $2000-$2EFF
        // Map $2000-$2FFF to 0x0000-0x0FFF
        let vram_index = (addr & 0x2FFF) - 0x2000;

        match self.mirroring_mode {
            MirroringMode::Vertical => {
                // Vertical mirroring: A, A, B, B
                // $2000/$2800 map to first 1KB, $2400/$2C00 map to second 1KB
                // Use modulo 0x0800 to map tables 0,2 together and 1,3 together
                vram_index % 0x0800
            }
            MirroringMode::Horizontal => {
                // Horizontal mirroring: A, A, B, B (left-right mirrored)
                // $2000/$2400 map to A (first 1KB), $2800/$2C00 map to B (second 1KB)
                // Tables 0&1 share first 1KB, tables 2&3 share second 1KB
                let table = vram_index / 0x0400;
                let offset = vram_index % 0x0400;
                let mirrored_table = match table {
                    0 | 1 => 0, // Tables 0 ($2000) and 1 ($2400) map to physical table 0
                    2 | 3 => 1, // Tables 2 ($2800) and 3 ($2C00) map to physical table 1
                    _ => unreachable!(),
                };
                mirrored_table * 0x0400 + offset
            }
            MirroringMode::SingleScreen | MirroringMode::SingleScreenLower => {
                // SingleScreen/Lower mirroring: all nametables map to first 1KB (lower bank)
                vram_index % 0x0400
            }
            MirroringMode::SingleScreenUpper => {
                // SingleScreenUpper mirroring: all nametables map to second 1KB (upper bank)
                0x0400 + (vram_index % 0x0400)
            }
            MirroringMode::FourScreen => {
                // FourScreen: no mirroring, direct mapping (needs 4KB VRAM)
                vram_index
            }
        }
    }

    /// Mirror palette address
    /// Addresses $3F10, $3F14, $3F18, $3F1C mirror to $3F00, $3F04, $3F08, $3F0C
    fn mirror_palette_address(&self, addr: u16) -> usize {
        let offset = (addr - 0x3F00) as usize % 32;
        // Mirror addresses $10, $14, $18, $1C to $00, $04, $08, $0C
        if offset & 0x13 == 0x10 {
            offset & 0x0F
        } else {
            offset
        }
    }

    /// Create a snapshot of VRAM for save-state.
    pub fn vram_snapshot(&self) -> Vec<u8> {
        self.ppu_ram.to_vec()
    }

    /// Create a snapshot of palette for save-state.
    pub fn palette_snapshot(&self) -> Vec<u8> {
        self.palette_ram.to_vec()
    }

    /// Restore VRAM from a save-state.
    pub fn restore_vram(&mut self, data: &[u8]) {
        let len = data.len().min(self.ppu_ram.len());
        self.ppu_ram[..len].copy_from_slice(&data[..len]);
    }

    /// Restore palette from a save-state.
    pub fn restore_palette(&mut self, data: &[u8]) {
        let len = data.len().min(self.palette_ram.len());
        self.palette_ram[..len].copy_from_slice(&data[..len]);
        self.last_palette_index = None; // Invalidate cache
    }

    pub fn last_palette_index(&self) -> Option<u8> {
        self.last_palette_index
    }

    pub fn last_palette_value(&self) -> u8 {
        self.last_palette_value
    }

    pub fn mirroring_mode(&self) -> MirroringMode {
        self.mirroring_mode
    }

    pub fn set_palette_cache(&mut self, index: Option<u8>, value: u8) {
        self.last_palette_index = index;
        self.last_palette_value = value;
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryDebugState {
    pub ppu_ram: [u8; 4096],
    pub palette: [u8; 32],
    pub last_palette_index: Option<u8>,
    pub last_palette_value: u8,
    pub mirroring_mode: MirroringMode,
}

#[cfg(test)]
impl Memory {
    pub fn debug_state(&self) -> MemoryDebugState {
        MemoryDebugState {
            ppu_ram: self.ppu_ram,
            palette: self.palette_ram,
            last_palette_index: self.last_palette_index,
            last_palette_value: self.last_palette_value,
            mirroring_mode: self.mirroring_mode,
        }
    }

    pub fn set_debug_state(&mut self, state: MemoryDebugState) {
        self.ppu_ram = state.ppu_ram;
        self.palette_ram = state.palette;
        self.last_palette_index = state.last_palette_index;
        self.last_palette_value = state.last_palette_value;
        self.mirroring_mode = state.mirroring_mode;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::Cartridge;

    struct TestNametableOverrideMapper;

    impl crate::cartridge::Mapper for TestNametableOverrideMapper {
        fn read_prg(&self, _addr: u16) -> u8 {
            0
        }

        fn write_prg(&mut self, _addr: u16, _value: u8) {}

        fn read_chr(&self, _addr: u16) -> u8 {
            0
        }

        fn write_chr(&mut self, _addr: u16, _value: u8) {}

        fn ppu_address_changed(&mut self, _addr: u16) {}

        fn read_nametable(&mut self, addr: u16) -> Option<u8> {
            if addr == 0x2000 {
                return Some(0xAB);
            }
            None
        }

        fn get_mirroring(&self) -> MirroringMode {
            MirroringMode::Horizontal
        }
    }

    #[test]
    fn test_memory_new() {
        let mut mem = Memory::default();
        assert_eq!(mem.read_chr(0, &None), 0);
        // Default mode uses Zero initialization, so palette is all zeros
        assert_eq!(mem.read_palette(0x3F00), 0);
    }

    #[test]
    fn test_mapper_can_override_nametable_reads() {
        let mem = Memory::default();
        let cart = Rc::new(RefCell::new(Cartridge::from_mapper_for_test(Box::new(
            TestNametableOverrideMapper,
        ))));
        let cartridge = Some(cart);

        // Expect mapper override to win.
        // This should fail until Memory starts consulting the mapper.
        assert_eq!(mem.read_nametable_mapped(0x2000, &cartridge), 0xAB);
    }

    fn create_mmc3_ines_rom() -> Vec<u8> {
        // Minimal iNES ROM with mapper 4 (MMC3):
        // - 32KB PRG ROM (2 banks)
        // - 8KB CHR ROM (1 bank)
        let prg_rom_banks = 2u8;
        let chr_rom_banks = 1u8;

        // Mapper 4: lower nibble goes in flags6 high nibble.
        let flags6 = 0x40; // mapper=4, horizontal mirroring
        let flags7 = 0x00;

        let mut rom = vec![
            b'N',
            b'E',
            b'S',
            0x1A,          // iNES header magic
            prg_rom_banks, // PRG ROM size (16KB units)
            chr_rom_banks, // CHR ROM size (8KB units)
            flags6,        // Flags 6
            flags7,        // Flags 7
            0,             // Flags 8 (PRG RAM size)
            0,             // Flags 9
            0,             // Flags 10
            0,
            0,
            0,
            0,
            0, // Reserved
        ];

        rom.extend(vec![0u8; prg_rom_banks as usize * 16384]);
        rom.extend(vec![0u8; chr_rom_banks as usize * 8192]);
        rom
    }

    fn clock_one_valid_a12_rising_edge_via_chr_reads(
        mem: &Memory,
        cartridge: &Option<Rc<RefCell<Cartridge>>>,
    ) {
        // MMC3 A12 low-pass filter: requires 3 CPU cycles low.
        mem.read_chr(0x0FFF, cartridge);
        if let Some(cart) = cartridge {
            let mut cart = cart.borrow_mut();
            for _ in 0..3 {
                cart.mapper_mut().cpu_cycle();
            }
        }
        mem.read_chr(0x1000, cartridge);
    }

    #[test]
    fn test_chr_reads_notify_mapper_address_changes_for_mmc3_irq() {
        // RED: MMC3 scanline IRQ relies on seeing PPU address bus activity (A12 rising edges).
        // If PPU CHR reads don't call mapper.ppu_address_changed(addr), MMC3 IRQ can never fire
        // during real rendering.

        let cartridge = Cartridge::new(&create_mmc3_ines_rom()).expect("MMC3 test ROM should load");
        let cartridge_rc = Rc::new(RefCell::new(cartridge));
        let cartridge_opt = Some(cartridge_rc.clone());

        // Program MMC3 IRQ: latch=1, reload requested, IRQ enabled.
        {
            let mut cart = cartridge_rc.borrow_mut();
            let mapper = cart.mapper_mut();
            mapper.write_prg(0xC000, 1);
            mapper.write_prg(0xC001, 0);
            mapper.write_prg(0xE001, 0);
        }

        let mem = Memory::default();

        // First valid A12 rising edge
        clock_one_valid_a12_rising_edge_via_chr_reads(&mem, &cartridge_opt);
        assert!(!cartridge_rc.borrow().mapper().irq_pending());

        // Second valid A12 rising edge should assert IRQ.
        clock_one_valid_a12_rising_edge_via_chr_reads(&mem, &cartridge_opt);

        assert!(cartridge_rc.borrow().mapper().irq_pending());
    }

    #[test]
    fn test_memory_reset() {
        let mut mem = Memory::default();
        mem.write_palette(0x3F00, 0x42);
        // Reset only clears the cache, not RAM
        mem.reset();
        // The palette value should still be there (0x42 & 0x3F = 0x02)
        assert_eq!(mem.read_palette(0x3F00), 0x02);
    }

    #[test]
    fn test_nametable_read_write() {
        let mut mem = Memory::default();
        mem.write_nametable(0x2000, 0x42);
        assert_eq!(mem.read_nametable(0x2000), 0x42);
    }

    #[test]
    fn test_palette_read_write() {
        let mut mem = Memory::default();
        mem.write_palette(0x3F00, 0x42);
        // Palette RAM only stores 6 bits (0x42 & 0x3F = 0x02)
        assert_eq!(mem.read_palette(0x3F00), 0x02);
    }

    #[test]
    fn test_palette_mirroring_3f10_to_3f00() {
        let mut mem = Memory::default();
        mem.write_palette(0x3F00, 0x42);
        // Palette RAM only stores 6 bits (0x42 & 0x3F = 0x02)
        assert_eq!(mem.read_palette(0x3F10), 0x02);
    }

    #[test]
    fn test_palette_mirroring_3f14_to_3f04() {
        let mut mem = Memory::default();
        mem.write_palette(0x3F04, 0x55);
        // Palette RAM only stores 6 bits (0x55 & 0x3F = 0x15)
        assert_eq!(mem.read_palette(0x3F14), 0x15);
    }

    #[test]
    fn test_palette_mirroring_3f18_to_3f08() {
        let mut mem = Memory::default();
        mem.write_palette(0x3F08, 0x66);
        // Palette RAM only stores 6 bits (0x66 & 0x3F = 0x26)
        assert_eq!(mem.read_palette(0x3F18), 0x26);
    }

    #[test]
    fn test_palette_mirroring_3f1c_to_3f0c() {
        let mut mem = Memory::default();
        mem.write_palette(0x3F0C, 0x7F);
        // Palette RAM only stores 6 bits (0x7F & 0x3F = 0x3F)
        assert_eq!(mem.read_palette(0x3F1C), 0x3F);
    }

    #[test]
    fn test_vertical_mirroring() {
        let mut mem = Memory::default();
        mem.set_mirroring(MirroringMode::Vertical);

        // Write to nametable 0
        mem.write_nametable(0x2000, 0x11);
        // Nametable 2 should mirror to nametable 0
        assert_eq!(mem.read_nametable(0x2800), 0x11);
    }

    #[test]
    fn test_horizontal_mirroring() {
        let mut mem = Memory::default();
        mem.set_mirroring(MirroringMode::Horizontal);

        // Write to nametable 0
        mem.write_nametable(0x2000, 0x22);
        // Nametable 1 should mirror to nametable 0 (horizontal mirroring)
        assert_eq!(mem.read_nametable(0x2400), 0x22);

        // But nametable 2 should not mirror to nametable 0
        assert_ne!(mem.read_nametable(0x2800), 0x22);
    }

    #[test]
    fn test_single_screen_mirroring() {
        let mut memory = Memory::default();
        memory.set_mirroring(MirroringMode::SingleScreen);

        // In SingleScreen mode, all four nametables map to the same 1KB
        // Write to $2000 (nametable 0)
        memory.write_nametable(0x2000, 0xAB);

        // All nametables should read the same value
        assert_eq!(memory.read_nametable(0x2000), 0xAB); // Nametable 0
        assert_eq!(memory.read_nametable(0x2400), 0xAB); // Nametable 1
        assert_eq!(memory.read_nametable(0x2800), 0xAB); // Nametable 2
        assert_eq!(memory.read_nametable(0x2C00), 0xAB); // Nametable 3

        // Write to different nametable, should affect all
        memory.write_nametable(0x2800, 0xCD);
        assert_eq!(memory.read_nametable(0x2000), 0xCD);
        assert_eq!(memory.read_nametable(0x2400), 0xCD);
        assert_eq!(memory.read_nametable(0x2800), 0xCD);
        assert_eq!(memory.read_nametable(0x2C00), 0xCD);
    }

    #[test]
    fn test_four_screen_mirroring() {
        let mut memory = Memory::default();
        memory.set_mirroring(MirroringMode::FourScreen);

        // In FourScreen mode, all four nametables are independent (no mirroring)
        // Each nametable gets its own 1KB of VRAM

        // Write different values to each nametable
        memory.write_nametable(0x2000, 0x11); // Nametable 0
        memory.write_nametable(0x2400, 0x22); // Nametable 1
        memory.write_nametable(0x2800, 0x33); // Nametable 2
        memory.write_nametable(0x2C00, 0x44); // Nametable 3

        // Each nametable should retain its own value (no mirroring)
        assert_eq!(memory.read_nametable(0x2000), 0x11);
        assert_eq!(memory.read_nametable(0x2400), 0x22);
        assert_eq!(memory.read_nametable(0x2800), 0x33);
        assert_eq!(memory.read_nametable(0x2C00), 0x44);

        // Verify addresses within each nametable work independently
        memory.write_nametable(0x2100, 0xAA); // Middle of nametable 0
        memory.write_nametable(0x2500, 0xBB); // Middle of nametable 1
        memory.write_nametable(0x2900, 0xCC); // Middle of nametable 2
        memory.write_nametable(0x2D00, 0xDD); // Middle of nametable 3

        assert_eq!(memory.read_nametable(0x2100), 0xAA);
        assert_eq!(memory.read_nametable(0x2500), 0xBB);
        assert_eq!(memory.read_nametable(0x2900), 0xCC);
        assert_eq!(memory.read_nametable(0x2D00), 0xDD);
    }

    #[test]
    fn test_vertical_mirroring_comprehensive() {
        let mut memory = Memory::default();
        memory.set_mirroring(MirroringMode::Vertical);

        // Vertical mirroring: A, A, B, B
        // Nametable 0 ($2000) and 2 ($2800) share memory
        // Nametable 1 ($2400) and 3 ($2C00) share memory

        // Write to nametable 0, should mirror to nametable 2
        memory.write_nametable(0x2000, 0x11);
        assert_eq!(memory.read_nametable(0x2000), 0x11);
        assert_eq!(memory.read_nametable(0x2800), 0x11); // Mirror

        // Write to nametable 1, should mirror to nametable 3
        memory.write_nametable(0x2400, 0x22);
        assert_eq!(memory.read_nametable(0x2400), 0x22);
        assert_eq!(memory.read_nametable(0x2C00), 0x22); // Mirror

        // Verify nametables 0 and 1 are independent
        assert_ne!(memory.read_nametable(0x2000), memory.read_nametable(0x2400));

        // Test with offset addresses
        memory.write_nametable(0x2100, 0x33);
        assert_eq!(memory.read_nametable(0x2900), 0x33); // $2100 mirrors to $2900

        memory.write_nametable(0x2500, 0x44);
        assert_eq!(memory.read_nametable(0x2D00), 0x44); // $2500 mirrors to $2D00
    }

    #[test]
    fn test_horizontal_mirroring_comprehensive() {
        let mut memory = Memory::default();
        memory.set_mirroring(MirroringMode::Horizontal);

        // Horizontal mirroring: A, A, B, B (left-right mirrored)
        // Nametable 0 ($2000) and 1 ($2400) share memory (top row)
        // Nametable 2 ($2800) and 3 ($2C00) share memory (bottom row)

        // Write to nametable 0, should mirror to nametable 1
        memory.write_nametable(0x2000, 0x11);
        assert_eq!(memory.read_nametable(0x2000), 0x11);
        assert_eq!(memory.read_nametable(0x2400), 0x11); // Mirror

        // Write to nametable 2, should mirror to nametable 3
        memory.write_nametable(0x2800, 0x22);
        assert_eq!(memory.read_nametable(0x2800), 0x22);
        assert_eq!(memory.read_nametable(0x2C00), 0x22); // Mirror

        // Verify nametables 0 and 2 are independent
        assert_ne!(memory.read_nametable(0x2000), memory.read_nametable(0x2800));

        // Test with offset addresses
        memory.write_nametable(0x2100, 0x33);
        assert_eq!(memory.read_nametable(0x2500), 0x33); // $2100 mirrors to $2500

        memory.write_nametable(0x2900, 0x44);
        assert_eq!(memory.read_nametable(0x2D00), 0x44); // $2900 mirrors to $2D00
    }

    #[test]
    fn test_dynamic_mirroring_mode_changes() {
        let mut memory = Memory::default();

        // Start with Vertical mirroring (A, A, B, B)
        memory.set_mirroring(MirroringMode::Vertical);
        memory.write_nametable(0x2000, 0xAA);
        assert_eq!(memory.read_nametable(0x2800), 0xAA); // $2000 mirrors to $2800 in vertical

        // Switch to Horizontal mirroring (A, A, B, B)
        memory.set_mirroring(MirroringMode::Horizontal);
        memory.write_nametable(0x2000, 0xBB);
        assert_eq!(memory.read_nametable(0x2400), 0xBB); // $2000 mirrors to $2400 in horizontal
        // And $2800 mirrors to $2C00
        memory.write_nametable(0x2800, 0xDD);
        assert_eq!(memory.read_nametable(0x2C00), 0xDD);

        // Switch to SingleScreen (A, A, A, A)
        memory.set_mirroring(MirroringMode::SingleScreen);
        memory.write_nametable(0x2000, 0xCC);
        assert_eq!(memory.read_nametable(0x2400), 0xCC);
        assert_eq!(memory.read_nametable(0x2800), 0xCC);
        assert_eq!(memory.read_nametable(0x2C00), 0xCC);

        // Switch to FourScreen (A, B, C, D)
        memory.set_mirroring(MirroringMode::FourScreen);
        memory.write_nametable(0x2000, 0x11);
        memory.write_nametable(0x2400, 0x22);
        memory.write_nametable(0x2800, 0x33);
        memory.write_nametable(0x2C00, 0x44);
        // Each should be independent
        assert_eq!(memory.read_nametable(0x2000), 0x11);
        assert_eq!(memory.read_nametable(0x2400), 0x22);
        assert_eq!(memory.read_nametable(0x2800), 0x33);
        assert_eq!(memory.read_nametable(0x2C00), 0x44);
    }

    #[test]
    fn test_mirroring_3000_to_2000() {
        let mut mem = Memory::default();
        mem.write_nametable(0x2000, 0x33);
        // $3000-$3EFF mirrors to $2000-$2EFF
        assert_eq!(mem.read_nametable(0x3000), 0x33);
    }

    #[test]
    fn test_chr_write_no_cartridge() {
        let mut mem = Memory::default();
        // With no cartridge, reads return 0
        assert_eq!(mem.read_chr(0x0000, &None), 0x00);

        // Write to CHR memory (no-op without cartridge)
        mem.write_chr(0x0000, 0xAA, &None);
        mem.write_chr(0x1000, 0xBB, &None);
        mem.write_chr(0x1FFF, 0xCC, &None);

        // Reads still return 0 (no cartridge to store data)
        assert_eq!(mem.read_chr(0x0000, &None), 0x00);
        assert_eq!(mem.read_chr(0x1000, &None), 0x00);
        assert_eq!(mem.read_chr(0x1FFF, &None), 0x00);
    }
}
