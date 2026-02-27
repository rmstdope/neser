//! Reusable mapper template types to reduce boilerplate code.
//!
//! These generic mapper implementations provide standard patterns used across multiple
//! simple mappers, ensuring consistent behavior and reducing duplicated code.
//!
//! # Benefits
//!
//! Using mapper templates significantly reduces code duplication:
//! - **CNROM**: Reduced from ~110 lines to ~25 lines (77% reduction)
//! - **UxROM**: Reduced from ~130 lines to ~30 lines (77% reduction)
//! - **GxROM**: Reduced from ~125 lines to ~25 lines (80% reduction)
//! - **ColorDreams**: Reduced from ~125 lines to ~25 lines (80% reduction)
//!
//! Total: Eliminated ~400+ lines of boilerplate while ensuring consistent behavior.
//!
//! # Available Templates
//!
//! - [`SimpleFixedPrgMapper`] - Fixed PRG bank + bank-selectable CHR-ROM (CNROM pattern)
//! - [`SimpleBankedPrgMapper`] - Bank-selectable PRG + CHR-RAM (UxROM pattern)
//! - [`DualBank32Mapper`] - 32KB PRG bank + 8KB CHR bank selection (GxROM/ColorDreams pattern)
//!
//! # When to Use
//!
//! Use these templates when creating a new mapper that matches one of the standard patterns.
//! If your mapper has special behavior (IRQ counters, custom mirroring control, expansion
//! audio, etc.), you'll need a custom implementation instead.
//!
//! ## SimpleFixedPrgMapper
//!
//! Use for mappers with:
//! - Fixed PRG-ROM (no PRG banking)
//! - Bank-switchable CHR-ROM
//! - Simple register at $8000-$FFFF that selects CHR bank
//!
//! **Examples:** CNROM (mapper 3)
//!
//! **Not suitable for:**
//! - CPROM (mapper 13) - has CHR-RAM with special banking (lower 4KB switchable, upper 4KB fixed)
//!
//! ```rust,ignore
//! // Example: Define a new mapper similar to CNROM
//! pub type MyFixedPrgMapper = SimpleFixedPrgMapper<8, 185>;
//!
//! let mapper = MyFixedPrgMapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);
//! ```
//!
//! ## SimpleBankedPrgMapper
//!
//! Use for mappers with:
//! - Bank-switchable PRG-ROM (typically 16KB banks)
//! - Fixed last PRG bank
//! - CHR-RAM (not CHR-ROM)
//! - Simple register at $8000-$FFFF that selects PRG bank
//!
//! **Examples:** UxROM (mapper 2)
//!
//! **Not suitable for:**
//! - AxROM (mapper 7) - has programmable mirroring control
//! - Camerica (mapper 71) - has separate mirroring register
//!
//! ```rust,ignore
//! // Example: Define a new UxROM-style mapper with 16KB banks
//! pub type MyBankedPrgMapper = SimpleBankedPrgMapper<16, 94>;
//!
//! let mapper = MyBankedPrgMapper::new(prg_rom, chr_rom, MirroringMode::Vertical);
//! ```
//!
//! ## DualBank32Mapper
//!
//! Use for mappers with:
//! - 32KB PRG bank selection
//! - 8KB CHR bank selection
//! - Single register that controls both banks
//! - Different bit masks for PRG and CHR bank selection
//!
//! **Examples:** GxROM (mapper 66), ColorDreams (mapper 11)
//!
//! ```rust,ignore
//! // Example: GxROM uses PRG bits 4-5, CHR bits 0-1
//! pub type MyGxROMStyle = DualBank32Mapper<0b0011, 4, 0b0011, 0, 66>;
//!
//! // Example: ColorDreams uses PRG bits 4-7, CHR bits 0-3
//! pub type MyColorDreamsStyle = DualBank32Mapper<0b1111, 4, 0b1111, 0, 11>;
//!
//! let mapper = MyGxROMStyle::new(prg_rom, chr_rom, MirroringMode::Horizontal);
//! ```

use super::mapper::MapperContext;
use crate::cartridge::common::{BankSwitch, BankedRom, ChrMemory, DEFAULT_PRG_RAM_SIZE, PrgRam};
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};

/// Simple mapper with fixed PRG-ROM and bank-selectable CHR-ROM.
///
/// This template implements the CNROM pattern used by several simple mappers:
/// - Fixed PRG-ROM (no banking)
/// - CHR-ROM with configurable bank size
/// - Any write to $8000-$FFFF selects CHR bank
///
/// # Type Parameters
///
/// - `CHR_BANK_KB`: Size of CHR banks in kilobytes (typically 8)
/// - `MAPPER_NUM`: Mapper number for identification
///
/// # Example
///
/// ```rust,ignore
/// // CNROM (Mapper 3) with 8KB CHR banks
/// type CNROMMapper = SimpleFixedPrgMapper<8, 3>;
///
/// let mapper = CNROMMapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);
/// ```
pub struct SimpleFixedPrgMapper<const CHR_BANK_KB: usize, const MAPPER_NUM: u8> {
    prg_rom: Vec<u8>,
    prg_ram: Option<PrgRam>,
    chr_rom: BankedRom,
    mirroring: NametableLayout,
    chr_bank: BankSwitch,
    bus_conflicts: bool,
}

impl<const CHR_BANK_KB: usize, const MAPPER_NUM: u8> SimpleFixedPrgMapper<CHR_BANK_KB, MAPPER_NUM> {
    /// Create a new SimpleFixedPrgMapper.
    ///
    /// # Arguments
    ///
    /// * `prg_rom` - PRG-ROM data (will be fixed at $8000-$FFFF)
    /// * `chr_rom` - CHR-ROM data (will be bank-switched)
    /// * `mirroring` - Nametable mirroring mode
    pub fn new(ctx: MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        let chr_bank_size = CHR_BANK_KB * 1024;
        let chr_bank = BankSwitch::from_rom(&chr_rom, chr_bank_size);
        let prg_ram = if ctx.prg_ram_banks_8k > 0 && ctx.prg_ram_size_specified {
            Some(PrgRam::new(
                ctx.prg_ram_banks_8k as usize * DEFAULT_PRG_RAM_SIZE,
            ))
        } else {
            None
        };
        // Submapper 1 = explicitly no bus conflicts; all others (including 0 = original
        // CNROM hardware) emulate AND-type bus conflicts.
        let bus_conflicts = ctx.submapper != 1;

        Self {
            prg_rom,
            prg_ram,
            chr_rom: BankedRom::new(chr_rom, chr_bank_size),
            mirroring,
            chr_bank,
            bus_conflicts,
        }
    }
}

impl<const CHR_BANK_KB: usize, const MAPPER_NUM: u8> Mapper
    for SimpleFixedPrgMapper<CHR_BANK_KB, MAPPER_NUM>
{
    fn read_prg(&self, addr: u16) -> u8 {
        // PRG-RAM at $6000-$7FFF (only if present)
        if let Some(prg_ram) = &self.prg_ram
            && let Some(value) = prg_ram.try_read(addr)
        {
            return value;
        }

        // PRG ROM is fixed at $8000-$FFFF (32KB or 16KB)
        match addr {
            0x8000..=0xFFFF => {
                let offset = (addr - 0x8000) as usize;
                let index = offset % self.prg_rom.len();
                self.prg_rom.get(index).copied().unwrap_or(0)
            }
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x0000..=0x5FFF => open_bus,
            0x6000..=0x7FFF if self.prg_ram.is_none() => open_bus,
            _ => self.read_prg(addr),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // PRG-RAM at $6000-$7FFF (only if present)
        if let Some(prg_ram) = &mut self.prg_ram
            && prg_ram.try_write(addr, value)
        {
            return;
        }

        // Any write to $8000-$FFFF sets the CHR bank select.
        // Original CNROM has AND-type bus conflicts: the ROM at the write address
        // also drives the bus, so the effective value is (write & ROM).
        if (0x8000..=0xFFFF).contains(&addr) {
            let effective = if self.bus_conflicts {
                value & self.read_prg(addr)
            } else {
                value
            };
            self.chr_bank.set(effective);
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let offset = (addr & 0x1FFF) as usize;
        self.chr_rom.read(self.chr_bank.current(), offset)
    }

    fn write_chr(&mut self, _addr: u16, _value: u8) {
        // CHR-ROM is read-only, writes are ignored
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        MAPPER_NUM
    }

    fn wram_size(&self) -> usize {
        self.prg_ram.as_ref().map_or(0, PrgRam::size)
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram
            .as_ref()
            .map_or_else(Vec::new, PrgRam::snapshot)
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        if let Some(prg_ram) = &mut self.prg_ram {
            prg_ram.load_snapshot(data);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.chr_bank.raw()]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&value) = data.first() {
            self.chr_bank.set(value);
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: false,
            has_chr_banking: true,
            has_dynamic_mirroring: false,
            has_expansion_audio: false,
            max_prg_ram_kb: if self.prg_ram.is_some() { 8 } else { 0 },
            prg_bank_size_kb: 32,
            chr_bank_size_kb: CHR_BANK_KB,
            trainer_jsr: false,
            ..Default::default()
        }
    }
}

/// Simple mapper with bank-selectable PRG and CHR-RAM.
///
/// This template implements the UxROM pattern used by several simple mappers:
/// - Switchable PRG bank at lower address
/// - Fixed last PRG bank at upper address
/// - CHR-RAM (not CHR-ROM)
/// - Any write to $8000-$FFFF selects PRG bank
///
/// # Type Parameters
///
/// - `PRG_BANK_KB`: Size of PRG banks in kilobytes (typically 16)
/// - `MAPPER_NUM`: Mapper number for identification
///
/// # Example
///
/// ```rust,ignore
/// // UxROM (Mapper 2) with 16KB PRG banks
/// type UxROMMapper = SimpleBankedPrgMapper<16, 2>;
///
/// let mapper = UxROMMapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);
/// ```
pub struct SimpleBankedPrgMapper<const PRG_BANK_KB: usize, const MAPPER_NUM: u8> {
    prg_rom: BankedRom,
    prg_ram: Option<PrgRam>,
    chr_memory: ChrMemory,
    mirroring: NametableLayout,
    bank_select: u8,
}

impl<const PRG_BANK_KB: usize, const MAPPER_NUM: u8>
    SimpleBankedPrgMapper<PRG_BANK_KB, MAPPER_NUM>
{
    /// Create a new SimpleBankedPrgMapper.
    ///
    /// # Arguments
    ///
    /// * `prg_rom` - PRG-ROM data (will be bank-switched)
    /// * `_chr_rom` - Ignored (CHR-RAM is used instead)
    /// * `mirroring` - Nametable mirroring mode
    pub fn new(ctx: MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let mirroring = ctx.mirroring;
        let prg_bank_size = PRG_BANK_KB * 1024;
        let prg_ram = if ctx.prg_ram_size_specified && ctx.prg_ram_banks_8k > 0 {
            Some(PrgRam::new(
                ctx.prg_ram_banks_8k as usize * DEFAULT_PRG_RAM_SIZE,
            ))
        } else {
            None
        };

        Self {
            prg_rom: BankedRom::new(prg_rom, prg_bank_size),
            prg_ram,
            chr_memory: ChrMemory::new_ram(8192),
            mirroring,
            bank_select: 0,
        }
    }
}

impl<const PRG_BANK_KB: usize, const MAPPER_NUM: u8> Mapper
    for SimpleBankedPrgMapper<PRG_BANK_KB, MAPPER_NUM>
{
    fn read_prg(&self, addr: u16) -> u8 {
        // PRG-RAM at $6000-$7FFF (only if present)
        if let Some(prg_ram) = &self.prg_ram
            && let Some(value) = prg_ram.try_read(addr)
        {
            return value;
        }

        // PRG ROM at $8000-$FFFF
        match addr {
            0x8000..=0xBFFF => {
                // Switchable PRG bank
                let bank = self.bank_select as usize;
                self.prg_rom.read_with_base(bank, 0x8000, addr)
            }
            0xC000..=0xFFFF => {
                // Fixed to last PRG bank
                let last_bank = self.prg_rom.num_banks().saturating_sub(1);
                self.prg_rom.read_with_base(last_bank, 0xC000, addr)
            }
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x0000..=0x5FFF => open_bus,
            0x6000..=0x7FFF if self.prg_ram.is_none() => open_bus,
            _ => self.read_prg(addr),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // PRG-RAM at $6000-$7FFF (only if present)
        if let Some(prg_ram) = &mut self.prg_ram
            && prg_ram.try_write(addr, value)
        {
            return;
        }

        // Any write to $8000-$FFFF sets the bank register
        if (0x8000..=0xFFFF).contains(&addr) {
            self.bank_select = value;
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.chr_memory.read(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.chr_memory.write(addr, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        MAPPER_NUM
    }

    fn wram_size(&self) -> usize {
        self.prg_ram.as_ref().map_or(0, PrgRam::size)
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram
            .as_ref()
            .map_or_else(Vec::new, PrgRam::snapshot)
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        if let Some(prg_ram) = &mut self.prg_ram {
            prg_ram.load_snapshot(data);
        }
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.bank_select]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if !data.is_empty() {
            self.bank_select = data[0];
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: false,
            has_chr_banking: false,
            has_dynamic_mirroring: false,
            has_expansion_audio: false,
            max_prg_ram_kb: self.prg_ram.as_ref().map_or(0, |r| r.size() / 1024),
            prg_bank_size_kb: PRG_BANK_KB,
            chr_bank_size_kb: 8,
            trainer_jsr: false,
            ..Default::default()
        }
    }
}

/// Dual 32KB bank mapper with separate PRG and CHR bank selection.
///
/// This template implements the GxROM/ColorDreams pattern:
/// - 32KB PRG bank selection
/// - 8KB CHR bank selection
/// - Single register controls both banks with different bit masks
///
/// # Type Parameters
///
/// - `PRG_MASK`: Bit mask for PRG bank selection (e.g., 0b0011 for 2 bits)
/// - `PRG_SHIFT`: Bit shift for PRG bank (e.g., 4 for bits 4-5)
/// - `CHR_MASK`: Bit mask for CHR bank selection (e.g., 0b0011 for 2 bits)
/// - `CHR_SHIFT`: Bit shift for CHR bank (e.g., 0 for bits 0-1)
/// - `MAPPER_NUM`: Mapper number for identification
///
/// # Example
///
/// ```rust,ignore
/// // GxROM (Mapper 66): PRG bits 4-5, CHR bits 0-1
/// type GxROMMapper = DualBank32Mapper<0b0011, 4, 0b0011, 0, 66>;
///
/// // ColorDreams (Mapper 11): PRG bits 4-7, CHR bits 0-3
/// type ColorDreamsMapper = DualBank32Mapper<0b1111, 4, 0b1111, 0, 11>;
///
/// let mapper = GxROMMapper::new(prg_rom, chr_rom, MirroringMode::Horizontal);
/// ```
pub struct DualBank32Mapper<
    const PRG_MASK: u8,
    const PRG_SHIFT: u8,
    const CHR_MASK: u8,
    const CHR_SHIFT: u8,
    const MAPPER_NUM: u8,
> {
    prg_rom: BankedRom,
    prg_ram: PrgRam,
    chr_rom: BankedRom,
    mirroring: NametableLayout,
    prg_bank: BankSwitch,
    chr_bank: BankSwitch,
}

impl<
    const PRG_MASK: u8,
    const PRG_SHIFT: u8,
    const CHR_MASK: u8,
    const CHR_SHIFT: u8,
    const MAPPER_NUM: u8,
> DualBank32Mapper<PRG_MASK, PRG_SHIFT, CHR_MASK, CHR_SHIFT, MAPPER_NUM>
{
    /// Create a new DualBank32Mapper.
    ///
    /// # Arguments
    ///
    /// * `prg_rom` - PRG-ROM data (32KB banks)
    /// * `chr_rom` - CHR-ROM data (8KB banks)
    /// * `mirroring` - Nametable mirroring mode
    pub fn new(ctx: MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        const PRG_BANK_SIZE: usize = 32 * 1024; // 32KB
        const CHR_BANK_SIZE: usize = 8 * 1024; // 8KB

        let prg_bank = BankSwitch::from_rom(&prg_rom, PRG_BANK_SIZE);
        let chr_bank = BankSwitch::from_rom(&chr_rom, CHR_BANK_SIZE);

        Self {
            prg_rom: BankedRom::new(prg_rom, PRG_BANK_SIZE),
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            chr_rom: BankedRom::new(chr_rom, CHR_BANK_SIZE),
            mirroring,
            prg_bank,
            chr_bank,
        }
    }
}

impl<
    const PRG_MASK: u8,
    const PRG_SHIFT: u8,
    const CHR_MASK: u8,
    const CHR_SHIFT: u8,
    const MAPPER_NUM: u8,
> Mapper for DualBank32Mapper<PRG_MASK, PRG_SHIFT, CHR_MASK, CHR_SHIFT, MAPPER_NUM>
{
    fn read_prg(&self, addr: u16) -> u8 {
        // PRG-RAM at $6000-$7FFF
        if let Some(value) = self.prg_ram.try_read(addr) {
            return value;
        }

        match addr {
            0x8000..=0xFFFF => self
                .prg_rom
                .read_with_base(self.prg_bank.current(), 0x8000, addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // PRG-RAM at $6000-$7FFF
        if self.prg_ram.try_write(addr, value) {
            return;
        }

        if (0x8000..=0xFFFF).contains(&addr) {
            // Extract PRG and CHR banks using configured masks and shifts
            self.chr_bank.set((value >> CHR_SHIFT) & CHR_MASK);
            self.prg_bank.set((value >> PRG_SHIFT) & PRG_MASK);
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let offset = (addr & 0x1FFF) as usize;
        self.chr_rom.read(self.chr_bank.current(), offset)
    }

    fn write_chr(&mut self, _addr: u16, _value: u8) {
        // CHR-ROM is read-only, writes are ignored
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        MAPPER_NUM
    }

    fn wram_size(&self) -> usize {
        self.prg_ram.size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram.snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.prg_ram.load_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_bank.raw(), self.chr_bank.raw()]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&value) = data.first() {
            self.prg_bank.set(value);
        }
        if let Some(&value) = data.get(1) {
            self.chr_bank.set(value);
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: false,
            has_chr_banking: true,
            has_dynamic_mirroring: false,
            has_expansion_audio: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            trainer_jsr: false,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::mapper::MapperContext;
    use super::*;

    // Test helper to create banked data
    fn banked_data(bank_size: usize, num_banks: usize) -> Vec<u8> {
        let mut data = vec![0; bank_size * num_banks];
        for bank in 0..num_banks {
            let start = bank * bank_size;
            let end = start + bank_size;
            for byte in &mut data[start..end] {
                *byte = bank as u8;
            }
        }
        data
    }

    // SimpleFixedPrgMapper tests
    mod simple_fixed_prg {
        use super::*;

        type TestMapper = SimpleFixedPrgMapper<8, 3>;

        #[test]
        fn test_fixed_prg_no_banking() {
            let mut prg_rom = vec![0; 32 * 1024];
            for (i, byte) in prg_rom.iter_mut().enumerate() {
                *byte = (i / 1024) as u8;
            }

            let mapper = TestMapper::new(MapperContext::new_for_test(
                3,
                prg_rom,
                vec![0; 32 * 1024],
                NametableLayout::Horizontal,
            ));

            assert_eq!(mapper.read_prg(0x8000), 0);
            assert_eq!(mapper.read_prg(0x9000), 4);
            assert_eq!(mapper.read_prg(0xC000), 16);
            assert_eq!(mapper.read_prg(0xFFFF), 31);
        }

        #[test]
        fn test_chr_bank_switching() {
            let mut chr_rom = vec![0; 32 * 1024];
            for bank in 0..4 {
                let start = bank * 8 * 1024;
                let end = start + 8 * 1024;
                for byte in &mut chr_rom[start..end] {
                    *byte = (bank * 10) as u8;
                }
            }

            let mut mapper = TestMapper::new(MapperContext::new_for_test(
                3,
                vec![0xFF; 32 * 1024], // 0xFF ensures bus conflicts don't mask the bank select
                chr_rom,
                NametableLayout::Horizontal,
            ));

            assert_eq!(mapper.read_chr(0x0000), 0);

            mapper.write_prg(0x8000, 1);
            assert_eq!(mapper.read_chr(0x0000), 10);

            mapper.write_prg(0x8000, 2);
            assert_eq!(mapper.read_chr(0x0000), 20);

            mapper.write_prg(0x8000, 3);
            assert_eq!(mapper.read_chr(0x0000), 30);
        }

        #[test]
        fn test_chr_bank_wrapping() {
            let mut chr_rom = vec![0; 16 * 1024]; // Only 2 banks
            for bank in 0..2 {
                let start = bank * 8 * 1024;
                let end = start + 8 * 1024;
                for byte in &mut chr_rom[start..end] {
                    *byte = (bank * 50) as u8;
                }
            }

            let mut mapper = TestMapper::new(MapperContext::new_for_test(
                3,
                vec![0xFF; 32 * 1024], // 0xFF ensures bus conflicts don't mask the bank select
                chr_rom,
                NametableLayout::Vertical,
            ));

            mapper.write_prg(0x8000, 0b0000_0001);
            assert_eq!(mapper.read_chr(0x0000), 50);

            // Writing higher bits should wrap (only 2 banks available)
            mapper.write_prg(0x8000, 0b0000_0011); // Bank 3 wraps to bank 1
            assert_eq!(mapper.read_chr(0x0000), 50);
        }

        #[test]
        fn test_registers_snapshot_restore() {
            let mut chr_rom = vec![0; 32 * 1024];
            for bank in 0..4 {
                let start = bank * 8 * 1024;
                let end = start + 8 * 1024;
                for byte in &mut chr_rom[start..end] {
                    *byte = (bank * 7) as u8;
                }
            }

            let mut mapper = TestMapper::new(MapperContext::new_for_test(
                3,
                vec![0xFF; 32 * 1024], // 0xFF ensures bus conflicts don't mask the bank select
                chr_rom.clone(),
                NametableLayout::Horizontal,
            ));
            mapper.write_prg(0x8000, 0b0000_0011);

            let registers = mapper.registers_snapshot();

            let mut restored = TestMapper::new(MapperContext::new_for_test(
                3,
                vec![0xFF; 32 * 1024], // 0xFF ensures bus conflicts don't mask the bank select
                chr_rom,
                NametableLayout::Horizontal,
            ));
            restored.restore_registers(&registers);

            assert_eq!(restored.read_chr(0x0000), 21);
            assert_eq!(restored.read_chr(0x1FFF), 21);
        }

        #[test]
        fn test_mapper_number() {
            let mapper = TestMapper::new(MapperContext::new_for_test(
                3,
                vec![0; 32 * 1024],
                vec![0; 32 * 1024],
                NametableLayout::Horizontal,
            ));
            assert_eq!(mapper.mapper_number(), 3);
        }
    }

    // SimpleBankedPrgMapper tests
    mod simple_banked_prg {
        use super::*;

        type TestMapper = SimpleBankedPrgMapper<16, 2>;

        #[test]
        fn test_prg_bank_switching() {
            let mut prg_rom = vec![0; 128 * 1024]; // 8 banks of 16KB
            for bank in 0..8 {
                let start = bank * 16 * 1024;
                let end = start + 16 * 1024;
                for byte in &mut prg_rom[start..end] {
                    *byte = bank as u8;
                }
            }

            let mut mapper = TestMapper::new(MapperContext::new_for_test(
                2,
                prg_rom,
                vec![],
                NametableLayout::Horizontal,
            ));

            // Initially bank 0 at $8000-$BFFF
            assert_eq!(mapper.read_prg(0x8000), 0);

            // Last bank (7) at $C000-$FFFF
            assert_eq!(mapper.read_prg(0xC000), 7);
            assert_eq!(mapper.read_prg(0xFFFF), 7);

            // Switch to bank 3
            mapper.write_prg(0x8000, 3);
            assert_eq!(mapper.read_prg(0x8000), 3);
            assert_eq!(mapper.read_prg(0xBFFF), 3);

            // Last bank unchanged
            assert_eq!(mapper.read_prg(0xC000), 7);
        }

        #[test]
        fn test_chr_ram() {
            let mut mapper = TestMapper::new(MapperContext::new_for_test(
                2,
                vec![0; 128 * 1024],
                vec![],
                NametableLayout::Horizontal,
            ));

            mapper.write_chr(0x0000, 0xAA);
            mapper.write_chr(0x1000, 0xBB);
            mapper.write_chr(0x1FFF, 0xCC);

            assert_eq!(mapper.read_chr(0x0000), 0xAA);
            assert_eq!(mapper.read_chr(0x1000), 0xBB);
            assert_eq!(mapper.read_chr(0x1FFF), 0xCC);
        }

        #[test]
        fn test_fixed_last_bank() {
            let mut prg_rom = vec![0; 256 * 1024]; // 16 banks
            for bank in 0..16 {
                let start = bank * 16 * 1024;
                let end = start + 16 * 1024;
                for byte in &mut prg_rom[start..end] {
                    *byte = (bank + 100) as u8;
                }
            }

            let mut mapper = TestMapper::new(MapperContext::new_for_test(
                2,
                prg_rom,
                vec![],
                NametableLayout::Horizontal,
            ));

            // Last bank should always be 115 (bank 15 + 100)
            assert_eq!(mapper.read_prg(0xC000), 115);

            mapper.write_prg(0x8000, 0);
            assert_eq!(mapper.read_prg(0xC000), 115);

            mapper.write_prg(0x8000, 5);
            assert_eq!(mapper.read_prg(0xC000), 115);
        }

        #[test]
        fn test_registers_snapshot_restore() {
            let mut prg_rom = vec![0; 128 * 1024];
            for bank in 0..8 {
                let start = bank * 16 * 1024;
                let end = start + 16 * 1024;
                for byte in &mut prg_rom[start..end] {
                    *byte = bank as u8;
                }
            }

            let mut mapper = TestMapper::new(MapperContext::new_for_test(
                2,
                prg_rom.clone(),
                vec![],
                NametableLayout::Horizontal,
            ));
            mapper.write_prg(0x8000, 3);
            mapper.write_chr(0x0000, 0x5A);

            let regs = mapper.registers_snapshot();
            let chr = mapper.chr_ram_snapshot();

            let mut restored = TestMapper::new(MapperContext::new_for_test(
                2,
                prg_rom,
                vec![],
                NametableLayout::Horizontal,
            ));
            restored.restore_registers(&regs);
            restored.restore_chr_ram(&chr);

            assert_eq!(restored.read_prg(0x8000), 3);
            assert_eq!(restored.read_chr(0x0000), 0x5A);
        }

        #[test]
        fn test_mapper_number() {
            let mapper = TestMapper::new(MapperContext::new_for_test(
                2,
                vec![0; 128 * 1024],
                vec![],
                NametableLayout::Horizontal,
            ));
            assert_eq!(mapper.mapper_number(), 2);
        }
    }

    // DualBank32Mapper tests
    mod dual_bank32 {
        use super::*;

        // GxROM: PRG bits 4-5, CHR bits 0-1
        type GxROMTestMapper = DualBank32Mapper<0b0011, 4, 0b0011, 0, 66>;
        // ColorDreams: PRG bits 4-7, CHR bits 0-3
        type ColorDreamsTestMapper = DualBank32Mapper<0b1111, 4, 0b1111, 0, 11>;

        #[test]
        fn test_gxrom_bank_selection() {
            let prg_rom = banked_data(32 * 1024, 4);
            let chr_rom = banked_data(8 * 1024, 4);

            let mut mapper = GxROMTestMapper::new(MapperContext::new_for_test(
                66,
                prg_rom,
                chr_rom,
                NametableLayout::Horizontal,
            ));

            // Initial banks
            assert_eq!(mapper.read_prg(0x8000), 0);
            assert_eq!(mapper.read_chr(0x0000), 0);

            // Select PRG bank 1, CHR bank 2: 0b0001_0010 = 0x12
            mapper.write_prg(0x8000, 0x12);
            assert_eq!(mapper.read_prg(0x8000), 1);
            assert_eq!(mapper.read_chr(0x0000), 2);
        }

        #[test]
        fn test_colordreams_bank_selection() {
            let prg_rom = banked_data(32 * 1024, 16);
            let chr_rom = banked_data(8 * 1024, 16);

            let mut mapper = ColorDreamsTestMapper::new(MapperContext::new_for_test(
                11,
                prg_rom,
                chr_rom,
                NametableLayout::Horizontal,
            ));

            // Select PRG bank 8, CHR bank 5: 0b1000_0101 = 0x85
            mapper.write_prg(0x8000, 0x85);
            assert_eq!(mapper.read_prg(0x8000), 8);
            assert_eq!(mapper.read_chr(0x0000), 5);

            // Select PRG bank 15, CHR bank 15: 0b1111_1111 = 0xFF
            mapper.write_prg(0x8000, 0xFF);
            assert_eq!(mapper.read_prg(0x8000), 15);
            assert_eq!(mapper.read_chr(0x0000), 15);
        }

        #[test]
        fn test_bank_wrapping() {
            let prg_rom = banked_data(32 * 1024, 4); // Only 4 banks
            let chr_rom = banked_data(8 * 1024, 2); // Only 2 banks

            let mut mapper = ColorDreamsTestMapper::new(MapperContext::new_for_test(
                11,
                prg_rom,
                chr_rom,
                NametableLayout::Horizontal,
            ));

            // Select bank 5, should wrap to bank 1 (5 % 4 = 1)
            mapper.write_prg(0x8000, 0x50); // 0b0101_0000
            assert_eq!(mapper.read_prg(0x8000), 1);

            // Select CHR bank 3, should wrap to bank 1 (3 % 2 = 1)
            mapper.write_prg(0x8000, 0x03); // 0b0000_0011
            assert_eq!(mapper.read_chr(0x0000), 1);
        }

        #[test]
        fn test_registers_snapshot_restore() {
            let prg_rom = banked_data(32 * 1024, 4);
            let chr_rom = banked_data(8 * 1024, 4);

            let mut mapper = GxROMTestMapper::new(MapperContext::new_for_test(
                66,
                prg_rom.clone(),
                chr_rom.clone(),
                NametableLayout::Horizontal,
            ));
            mapper.write_prg(0x8000, 0x21); // PRG bank 2, CHR bank 1

            let snapshot = mapper.registers_snapshot();

            let mut restored = GxROMTestMapper::new(MapperContext::new_for_test(
                66,
                prg_rom,
                chr_rom,
                NametableLayout::Horizontal,
            ));
            restored.restore_registers(&snapshot);

            assert_eq!(restored.read_prg(0x8000), 2);
            assert_eq!(restored.read_chr(0x0000), 1);
        }

        #[test]
        fn test_mapper_numbers() {
            let prg_rom = vec![0; 128 * 1024];
            let chr_rom = vec![0; 32 * 1024];

            let gxrom = GxROMTestMapper::new(MapperContext::new_for_test(
                66,
                prg_rom.clone(),
                chr_rom.clone(),
                NametableLayout::Horizontal,
            ));
            assert_eq!(gxrom.mapper_number(), 66);

            let colordreams = ColorDreamsTestMapper::new(MapperContext::new_for_test(
                11,
                prg_rom,
                chr_rom,
                NametableLayout::Horizontal,
            ));
            assert_eq!(colordreams.mapper_number(), 11);
        }
    }
}
