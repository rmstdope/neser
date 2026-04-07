//! `BaseMapper` — shared state and default behavior for all NES mapper implementations.
//!
//! Mappers embed `BaseMapper` via composition and delegate boilerplate to it,
//! only implementing the truly mapper-specific logic themselves.

use super::common::{ChrMemory, DEFAULT_CHR_RAM_SIZE, DEFAULT_PRG_RAM_SIZE, PrgRam};
use super::mapper::{MapperCapabilities, MapperContext};
use crate::cartridge::NametableLayout;

/// Shared mapper infrastructure holding common state
/// (PRG-ROM, PRG-RAM, CHR memory, mirroring, mapper number) and providing
/// default implementations for the repetitive `Mapper` trait methods.
///
/// Mappers embed `BaseMapper` via composition and delegate boilerplate to it,
/// only implementing the truly mapper-specific logic themselves.
///
/// # Usage
///
/// ```rust,ignore
/// use neser::cartridge::{BaseMapper, Mapper, NametableLayout, MapperCapabilities};
///
/// pub struct MyMapper {
///     base: BaseMapper,
///     // ... mapper-specific fields
/// }
///
/// impl Mapper for MyMapper {
///     fn base(&self) -> &BaseMapper { &self.base }
///     fn base_mut(&mut self) -> &mut BaseMapper { &mut self.base }
///     fn read_prg(&self, addr: u16) -> u8 {
///         if let Some(v) = self.base.try_read_prg_ram(addr) { return v; }
///         self.base.read_prg_banked(addr)
///     }
///     fn write_prg(&mut self, addr: u16, value: u8) {
///         if self.base.try_write_prg_ram(addr, value) { return; }
///         // mapper-specific register write logic
///     }
/// }
/// ```
pub struct BaseMapper {
    prg_rom: Vec<u8>,
    prg_ram: Option<PrgRam>,
    chr_memory: ChrMemory,
    mirroring: NametableLayout,
    mapper_number: u16,
    capabilities: MapperCapabilities,
    prg_page_size: usize,
    chr_page_size: usize,
    /// Page table mapping PRG slots to bank numbers.
    /// Length = 32KB / prg_page_size. Each entry is a bank number (after wrapping).
    prg_pages: Vec<usize>,
    /// Page table mapping CHR slots to bank numbers.
    /// Length = 8KB / chr_page_size. Each entry is a bank number (after wrapping).
    chr_pages: Vec<usize>,
    /// Whether bus conflicts are enabled (write value ANDed with ROM at same address).
    bus_conflicts: bool,
    /// Optional PRG-ROM bank resolved for the $6000-$7FFF region.
    /// `None` means $6000 banking is not configured (default: PRG-RAM or open bus).
    prg_6000_bank: Option<usize>,
}

/// Resolve a signed bank number to an unsigned index, wrapping to available banks.
///
/// Negative values count from the end: -1 = last bank, -2 = second-to-last, etc.
fn resolve_bank(bank: i16, bank_count: usize) -> usize {
    if bank < 0 {
        (bank_count as i32 + bank as i32) as usize % bank_count
    } else {
        bank as usize % bank_count
    }
}

/// Compute the flat byte index for a banked memory access.
///
/// `offset` is the byte offset within the banked window (e.g. `addr - 0x8000` for PRG).
/// `page_size` is the size of each bank in bytes.
/// `pages` maps slot numbers to bank numbers.
fn resolve_banked_index(offset: usize, page_size: usize, pages: &[usize]) -> usize {
    let slot = offset / page_size;
    let bank = pages[slot];
    bank * page_size + (offset % page_size)
}

#[allow(dead_code)]
impl BaseMapper {
    /// Create a new `BaseMapper` from a `MapperContext`.
    ///
    /// PRG-RAM is created only when the header explicitly specifies a non-zero size.
    /// CHR memory is ROM when `chr_rom` is non-empty, otherwise CHR-RAM is allocated.
    pub fn new(ctx: &MapperContext, capabilities: MapperCapabilities) -> Self {
        let prg_ram = if capabilities.max_prg_ram_kb > 0
            && ctx.prg_ram_size_specified
            && ctx.prg_ram_banks_8k > 0
        {
            Some(PrgRam::new(
                ctx.prg_ram_banks_8k as usize * DEFAULT_PRG_RAM_SIZE,
            ))
        } else {
            None
        };
        Self {
            prg_rom: ctx.prg_rom.clone(),
            prg_ram,
            chr_memory: if ctx.chr_rom.is_empty() {
                let size = ctx
                    .chr_ram_size_bytes
                    .unwrap_or(DEFAULT_CHR_RAM_SIZE)
                    .max(DEFAULT_CHR_RAM_SIZE);
                ChrMemory::new_ram(size)
            } else {
                ChrMemory::new(ctx.chr_rom.clone())
            },
            mirroring: ctx.mirroring,
            mapper_number: ctx.mapper,
            capabilities,
            prg_page_size: 0,
            chr_page_size: 0,
            prg_pages: Vec::new(),
            chr_pages: Vec::new(),
            bus_conflicts: false,
            prg_6000_bank: None,
        }
    }

    // --- PRG-ROM access ---

    /// Get a reference to the PRG-ROM data.
    #[inline]
    pub fn prg_rom(&self) -> &[u8] {
        &self.prg_rom
    }

    /// Read a byte from PRG-ROM at $8000-$FFFF.
    ///
    /// Auto-detects banking: if `configure_prg_banking` has been called,
    /// delegates to `read_prg_banked`; otherwise uses `read_prg_rom_fixed`.
    #[inline]
    pub fn read_prg_rom(&self, addr: u16) -> u8 {
        if !(0x8000..=0xFFFF).contains(&addr) {
            return 0;
        }
        if self.prg_page_size > 0 {
            self.read_prg_banked(addr)
        } else {
            self.read_prg_rom_fixed(addr)
        }
    }

    /// Read a byte from fixed PRG-ROM at $8000-$FFFF with automatic mirroring.
    ///
    /// For mappers with no PRG banking (e.g., NROM), this maps the full
    /// 32KB window using `offset % prg_rom.len()` which naturally handles
    /// 16KB mirroring.
    #[inline]
    pub fn read_prg_rom_fixed(&self, addr: u16) -> u8 {
        if (0x8000..=0xFFFF).contains(&addr) {
            let index = (addr - 0x8000) as usize % self.prg_rom.len();
            self.prg_rom.get(index).copied().unwrap_or(0)
        } else {
            0
        }
    }

    // --- PRG-RAM access ---

    /// Try to read from PRG-RAM if the address is in $6000-$7FFF.
    /// Returns `Some(value)` if PRG-RAM exists and address is in range, else `None`.
    #[inline]
    pub fn try_read_prg_ram(&self, addr: u16) -> Option<u8> {
        self.prg_ram.as_ref().and_then(|ram| ram.try_read(addr))
    }

    /// Try to write to PRG-RAM if the address is in $6000-$7FFF.
    /// Returns `true` if the write was handled.
    #[inline]
    pub fn try_write_prg_ram(&mut self, addr: u16, value: u8) -> bool {
        if let Some(prg_ram) = &mut self.prg_ram {
            prg_ram.try_write(addr, value)
        } else {
            false
        }
    }

    /// Whether PRG-RAM is present.
    #[inline]
    pub fn has_prg_ram(&self) -> bool {
        self.prg_ram.is_some()
    }

    /// Read a byte from PRG-RAM at an absolute byte offset.
    /// Used for banked RAM access where the caller computes `bank * bank_size + page_offset`.
    /// Returns 0 (open bus) if no PRG-RAM is present or if `offset` is beyond the RAM size.
    #[inline]
    pub fn read_prg_ram_at_offset(&self, offset: usize) -> u8 {
        self.prg_ram
            .as_ref()
            .map_or(0, |ram| ram.read_at_offset(offset))
    }

    /// Write a byte to PRG-RAM at an absolute byte offset.
    /// No-op if no RAM is present or if the offset is past the end.
    #[inline]
    pub fn write_prg_ram_at_offset(&mut self, offset: usize, value: u8) {
        if let Some(ram) = &mut self.prg_ram {
            ram.write_at_offset(offset, value);
        }
    }

    // --- CHR memory access ---

    /// Read a byte from CHR memory (ROM or RAM) at $0000-$1FFF.
    ///
    /// Auto-detects banking: if `configure_chr_banking` has been called,
    /// delegates to `read_chr_banked`; otherwise uses direct unbanked access.
    #[inline]
    pub fn read_chr(&self, addr: u16) -> u8 {
        if self.chr_page_size > 0 {
            self.read_chr_banked(addr)
        } else {
            self.chr_memory.read(addr)
        }
    }

    /// Write a byte to CHR memory. Only succeeds for CHR-RAM.
    ///
    /// Auto-detects banking: if `configure_chr_banking` has been called,
    /// delegates to `write_chr_banked`; otherwise uses direct unbanked access.
    #[inline]
    pub fn write_chr(&mut self, addr: u16, value: u8) {
        if self.chr_page_size > 0 {
            self.write_chr_banked(addr, value);
        } else {
            self.chr_memory.write(addr, value);
        }
    }

    /// Read a byte from CHR memory at a raw byte index (not limited to $0000-$1FFF).
    ///
    /// Used by MMC3-based multicart mappers that compute their own effective
    /// bank and offset, bypassing the page table.
    #[inline]
    pub fn read_chr_at_index(&self, index: usize) -> u8 {
        self.chr_memory.read_at_index(index)
    }

    /// Write a byte to CHR memory at a raw byte index (not limited to $0000-$1FFF).
    ///
    /// Only succeeds for CHR-RAM. Used by MMC3-based multicart mappers.
    #[inline]
    pub fn write_chr_at_index(&mut self, index: usize, value: u8) {
        self.chr_memory.write_at_index(index, value);
    }

    // --- Mirroring ---

    /// Get the nametable mirroring layout.
    #[inline]
    pub fn mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    /// Set the nametable mirroring layout (for mappers with dynamic mirroring).
    #[inline]
    pub fn set_mirroring(&mut self, mirroring: NametableLayout) {
        self.mirroring = mirroring;
    }

    /// Convenience: set horizontal (true) or vertical (false) mirroring.
    #[inline]
    pub fn set_mirroring_hv(&mut self, horizontal: bool) {
        let mirroring = if horizontal {
            NametableLayout::Horizontal
        } else {
            NametableLayout::Vertical
        };
        self.set_mirroring(mirroring);
    }

    // --- Mapper identification ---

    /// Get the mapper number.
    #[inline]
    pub fn mapper_number(&self) -> u16 {
        self.mapper_number
    }

    // --- Save-state / WRAM support ---

    /// Get the WRAM (PRG-RAM) size in bytes.
    pub fn wram_size(&self) -> usize {
        self.prg_ram.as_ref().map_or(0, PrgRam::size)
    }

    /// Create a snapshot of PRG-RAM for save persistence.
    pub fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram
            .as_ref()
            .map_or_else(Vec::new, PrgRam::snapshot)
    }

    /// Load a PRG-RAM snapshot from save persistence.
    pub fn load_wram_snapshot(&mut self, data: &[u8]) {
        if let Some(prg_ram) = &mut self.prg_ram {
            prg_ram.load_snapshot(data);
        }
    }

    /// Create a snapshot of CHR-RAM for save-state.
    pub fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    /// Return the total size of the CHR memory in bytes.
    pub fn chr_size(&self) -> usize {
        self.chr_memory.size()
    }

    /// Restore CHR-RAM from a save-state.
    pub fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    /// Replace the CHR memory with a custom `ChrMemory` instance.
    ///
    /// Useful for mappers that need non-standard CHR-RAM sizes (e.g. CPROM with 16KB CHR-RAM).
    pub fn set_chr_memory(&mut self, chr: ChrMemory) {
        self.chr_memory = chr;
    }

    /// Re-initialize all RAM (PRG-RAM + CHR-RAM) for cartridge insertion / hard reset.
    pub fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        if let Some(prg_ram) = &mut self.prg_ram {
            prg_ram.initialize(mode);
        }
        self.chr_memory.initialize(mode);
    }

    /// Initialize only CHR-RAM, leaving PRG-RAM untouched.
    ///
    /// Used when inserting a battery-backed cartridge: PRG-RAM was already
    /// loaded from disk and must not be overwritten.
    pub fn initialize_chr_ram(&mut self, mode: crate::console::RamInitMode) {
        self.chr_memory.initialize(mode);
    }

    /// Read PRG with open-bus handling.
    ///
    /// Returns `open_bus` for addresses below $6000 and for $6000-$7FFF when
    /// neither PRG-RAM nor mapper-controlled PRG-ROM banking is present there.
    /// Otherwise delegates to the provided `read_prg` function.
    pub fn read_prg_open_bus(&self, addr: u16, open_bus: u8, read_prg: impl Fn(u16) -> u8) -> u8 {
        match addr {
            0x0000..=0x5FFF => open_bus,
            0x6000..=0x7FFF if self.prg_6000_bank.is_some() => read_prg(addr),
            0x6000..=0x7FFF if self.prg_ram.is_none() => open_bus,
            _ => read_prg(addr),
        }
    }

    /// Get the mapper capabilities.
    pub fn capabilities(&self) -> MapperCapabilities {
        self.capabilities.clone()
    }

    // --- Banking configuration ---

    /// Configure PRG-ROM banking with the given page size in bytes.
    ///
    /// Sets up the page table with `32KB / page_size` slots.
    /// Initially all slots are mapped to bank 0.
    /// Common page sizes: 0x2000 (8KB), 0x4000 (16KB), 0x8000 (32KB).
    pub fn configure_prg_banking(&mut self, page_size: usize) {
        self.prg_page_size = page_size;
        let slot_count = 0x8000 / page_size;
        self.prg_pages = vec![0; slot_count];
    }

    /// Configure CHR memory banking with the given page size in bytes.
    ///
    /// Sets up the page table with `8KB / page_size` slots.
    /// Initially all slots are mapped to bank 0.
    /// Common page sizes: 0x0400 (1KB), 0x0800 (2KB), 0x1000 (4KB), 0x2000 (8KB).
    pub fn configure_chr_banking(&mut self, page_size: usize) {
        self.chr_page_size = page_size;
        let slot_count = 0x2000 / page_size;
        self.chr_pages = vec![0; slot_count];
    }

    /// Select a PRG bank for the given slot.
    ///
    /// Negative values count from the end: -1 = last bank, -2 = second-to-last, etc.
    /// Bank numbers are automatically wrapped to the available number of banks.
    pub fn select_prg_page(&mut self, slot: usize, bank: i16) {
        self.prg_pages[slot] = resolve_bank(bank, self.prg_bank_count());
    }

    /// Apply NROM-128 or NROM-256 PRG banking to two 16KB slots.
    ///
    /// - NROM-128 (`nrom_128 = true`): maps the same 16KB bank to both $8000 and $C000.
    /// - NROM-256 (`nrom_128 = false`): maps a consecutive 16KB bank pair (even/odd).
    ///   Bit 0 of `bank` is cleared for slot 0 and set for slot 1.
    pub fn apply_nrom_prg_banking(&mut self, bank: u8, nrom_128: bool) {
        if nrom_128 {
            self.select_prg_page(0, bank as i16);
            self.select_prg_page(1, bank as i16);
        } else {
            let even = (bank & !1) as i16;
            self.select_prg_page(0, even);
            self.select_prg_page(1, even | 1);
        }
    }

    /// Enable 8KB PRG-ROM banking at $6000-$7FFF.
    ///
    /// After calling this, `select_prg_6000_page()` can be used to choose
    /// which 8KB bank of PRG-ROM appears at $6000-$7FFF, and
    /// `try_read_prg_6000()` will resolve reads from that region.
    ///
    /// Initially maps bank 0.
    pub fn configure_prg_6000_banking(&mut self) {
        self.prg_6000_bank = Some(0);
    }

    /// Select which 8KB PRG-ROM bank maps to $6000-$7FFF.
    ///
    /// Requires `configure_prg_6000_banking()` to have been called first.
    /// Negative values count from the end: -1 = last 8KB bank, etc.
    pub fn select_prg_6000_page(&mut self, bank: i16) {
        let bank_count = self.prg_rom.len() / 0x2000;
        if bank_count > 0 {
            self.prg_6000_bank = Some(resolve_bank(bank, bank_count));
        }
    }

    /// Try to read a byte from the $6000-$7FFF PRG-ROM banking region.
    ///
    /// Returns `Some(byte)` if $6000 banking is configured and `addr` is in
    /// $6000-$7FFF. Returns `None` otherwise (so the caller can fall through
    /// to PRG-RAM or open bus).
    pub fn try_read_prg_6000(&self, addr: u16) -> Option<u8> {
        let bank = self.prg_6000_bank?;
        if !(0x6000..=0x7FFF).contains(&addr) {
            return None;
        }
        let offset = (addr as usize) - 0x6000;
        Some(
            self.prg_rom
                .get(bank * 0x2000 + offset)
                .copied()
                .unwrap_or(0),
        )
    }

    /// Returns `true` if $6000-$7FFF PRG-ROM banking is configured.
    pub fn has_prg_6000_banking(&self) -> bool {
        self.prg_6000_bank.is_some()
    }

    /// Select a CHR bank for the given slot.
    ///
    /// Negative values count from the end: -1 = last bank, -2 = second-to-last, etc.
    /// Bank numbers are automatically wrapped to the available number of banks.
    pub fn select_chr_page(&mut self, slot: usize, bank: i16) {
        self.chr_pages[slot] = resolve_bank(bank, self.chr_bank_count());
    }

    /// Read a byte from banked PRG-ROM at $8000-$FFFF.
    ///
    /// Uses the page table to resolve the address to the correct bank and offset.
    /// Requires `configure_prg_banking` to have been called first.
    pub fn read_prg_banked(&self, addr: u16) -> u8 {
        let offset = (addr - 0x8000) as usize;
        let index = resolve_banked_index(offset, self.prg_page_size, &self.prg_pages);
        self.prg_rom.get(index).copied().unwrap_or(0)
    }

    /// Read a byte from banked CHR memory at $0000-$1FFF.
    ///
    /// Uses the page table to resolve the address to the correct bank and offset.
    /// Requires `configure_chr_banking` to have been called first.
    pub fn read_chr_banked(&self, addr: u16) -> u8 {
        let offset = (addr & 0x1FFF) as usize;
        let index = resolve_banked_index(offset, self.chr_page_size, &self.chr_pages);
        self.chr_memory.read_at_index(index)
    }

    /// Write a byte to banked CHR memory at $0000-$1FFF.
    ///
    /// Only succeeds for CHR-RAM. Uses the page table to resolve the address.
    /// Requires `configure_chr_banking` to have been called first.
    pub fn write_chr_banked(&mut self, addr: u16, value: u8) {
        let offset = (addr & 0x1FFF) as usize;
        let index = resolve_banked_index(offset, self.chr_page_size, &self.chr_pages);
        self.chr_memory.write_at_index(index, value);
    }

    /// Enable or disable bus conflicts.
    ///
    /// When enabled, write values are ANDed with the ROM byte at the write address.
    pub fn set_bus_conflicts(&mut self, enabled: bool) {
        self.bus_conflicts = enabled;
    }

    /// Whether bus conflicts are enabled.
    pub fn has_bus_conflicts(&self) -> bool {
        self.bus_conflicts
    }

    /// Apply bus conflict: AND the write value with the PRG-ROM byte at the same address.
    ///
    /// Returns the original value if bus conflicts are disabled.
    pub fn apply_bus_conflict(&self, addr: u16, value: u8) -> u8 {
        if self.bus_conflicts {
            value & self.read_prg_banked(addr)
        } else {
            value
        }
    }

    /// Get the number of PRG banks for the configured page size.
    pub fn prg_bank_count(&self) -> usize {
        self.prg_rom.len() / self.prg_page_size
    }

    /// Get the number of CHR banks for the configured page size.
    pub fn chr_bank_count(&self) -> usize {
        self.chr_memory.size() / self.chr_page_size
    }

    /// Get the current PRG bank selected for a given slot.
    pub fn prg_page(&self, slot: usize) -> usize {
        self.prg_pages[slot]
    }

    /// Get the current CHR bank selected for a given slot.
    pub fn chr_page(&self, slot: usize) -> usize {
        self.chr_pages[slot]
    }

    /// Snapshot all banking page tables (PRG + CHR) and optionally mirroring state.
    ///
    /// Format: [prg_page_0, ..., prg_page_N, chr_page_0, ..., chr_page_M, mirroring?]
    /// Mirroring byte is included only when `has_dynamic_mirroring` is set in capabilities.
    /// Mirroring byte uses `NametableLayout::to_snapshot_byte()` encoding.
    ///
    /// # Panics
    /// Panics if any bank index exceeds 255.
    pub fn banking_snapshot(&self) -> Vec<u8> {
        let mut data: Vec<u8> = self
            .prg_pages
            .iter()
            .chain(self.chr_pages.iter())
            .map(|&page| {
                u8::try_from(page)
                    .expect("banking_snapshot: bank index does not fit in u8 (max 255)")
            })
            .collect();
        if self.capabilities.has_dynamic_mirroring {
            data.push(self.mirroring.to_snapshot_byte());
        }
        data
    }

    /// Restore banking page tables (PRG + CHR) and optionally mirroring state
    /// from a snapshot produced by `banking_snapshot()`.
    pub fn restore_banking(&mut self, data: &[u8]) {
        let mut idx = 0;
        for slot in 0..self.prg_pages.len() {
            if let Some(&bank) = data.get(idx) {
                self.select_prg_page(slot, bank as i16);
                idx += 1;
            }
        }
        for slot in 0..self.chr_pages.len() {
            if let Some(&bank) = data.get(idx) {
                self.select_chr_page(slot, bank as i16);
                idx += 1;
            }
        }
        if self.capabilities.has_dynamic_mirroring
            && let Some(&mir) = data.get(idx)
        {
            self.set_mirroring(NametableLayout::from_snapshot_byte(mir));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_base_mapper_with_prg_ram(prg_ram_banks: u8) -> BaseMapper {
        let ctx = MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![0; 8192],
            NametableLayout::Horizontal,
        )
        .with_prg_ram_banks(prg_ram_banks);
        let capabilities = MapperCapabilities {
            max_prg_ram_kb: if prg_ram_banks > 0 { 8 } else { 0 },
            ..MapperCapabilities::default()
        };
        BaseMapper::new(&ctx, capabilities)
    }

    #[test]
    fn test_base_mapper_read_prg_rom_fixed_32kb() {
        let mut prg_rom = vec![0; 0x8000];
        prg_rom[0x0000] = 0xAA;
        prg_rom[0x4000] = 0xBB;
        prg_rom[0x7FFF] = 0xCC;
        let ctx =
            MapperContext::new_for_test(0, prg_rom, vec![0; 8192], NametableLayout::Horizontal);
        let base = BaseMapper::new(&ctx, MapperCapabilities::default());

        assert_eq!(base.read_prg_rom_fixed(0x8000), 0xAA);
        assert_eq!(base.read_prg_rom_fixed(0xC000), 0xBB);
        assert_eq!(base.read_prg_rom_fixed(0xFFFF), 0xCC);
    }

    #[test]
    fn test_base_mapper_read_prg_rom_fixed_16kb_mirrors() {
        let mut prg_rom = vec![0; 0x4000];
        prg_rom[0x0000] = 0xAA;
        prg_rom[0x3FFF] = 0xBB;
        let ctx =
            MapperContext::new_for_test(0, prg_rom, vec![0; 8192], NametableLayout::Horizontal);
        let base = BaseMapper::new(&ctx, MapperCapabilities::default());

        assert_eq!(base.read_prg_rom_fixed(0x8000), 0xAA);
        assert_eq!(base.read_prg_rom_fixed(0xBFFF), 0xBB);
        assert_eq!(base.read_prg_rom_fixed(0xC000), 0xAA); // mirrored
        assert_eq!(base.read_prg_rom_fixed(0xFFFF), 0xBB); // mirrored
    }

    #[test]
    fn test_base_mapper_read_prg_rom_unconfigured_uses_fixed() {
        let mut prg_rom = vec![0; 0x4000];
        prg_rom[0x0000] = 0xAB;
        prg_rom[0x3FFF] = 0xCD;
        let ctx =
            MapperContext::new_for_test(0, prg_rom, vec![0; 8192], NametableLayout::Horizontal);
        let base = BaseMapper::new(&ctx, MapperCapabilities::default());

        assert_eq!(base.read_prg_rom(0x8000), 0xAB);
        assert_eq!(base.read_prg_rom(0xC000), 0xAB); // mirrored in fixed mode
        assert_eq!(base.read_prg_rom(0xFFFF), 0xCD);
    }

    #[test]
    fn test_base_mapper_read_prg_rom_configured_uses_banked() {
        let mut base = make_prg_banked_mapper(4, 0x2000);
        base.select_prg_page(0, 3);
        base.select_prg_page(1, 1);
        base.select_prg_page(2, 0);
        base.select_prg_page(3, 2);

        assert_eq!(base.read_prg_rom(0x8000), 3);
        assert_eq!(base.read_prg_rom(0xA000), 1);
        assert_eq!(base.read_prg_rom(0xC000), 0);
        assert_eq!(base.read_prg_rom(0xE000), 2);
    }

    #[test]
    fn test_base_mapper_read_prg_rom_out_of_range_returns_zero() {
        let mut base = make_prg_banked_mapper(2, 0x4000);
        base.select_prg_page(0, 1);
        assert_eq!(base.read_prg_rom(0x7FFF), 0);
    }

    #[test]
    fn test_base_mapper_prg_ram_read_write() {
        let mut base = make_base_mapper_with_prg_ram(1);

        assert_eq!(base.try_read_prg_ram(0x6000), Some(0));
        base.try_write_prg_ram(0x6000, 0x42);
        assert_eq!(base.try_read_prg_ram(0x6000), Some(0x42));

        // Out of range
        assert_eq!(base.try_read_prg_ram(0x5FFF), None);
        assert_eq!(base.try_read_prg_ram(0x8000), None);
    }

    #[test]
    fn test_base_mapper_no_prg_ram() {
        let base = make_base_mapper_with_prg_ram(0);

        assert!(!base.has_prg_ram());
        assert_eq!(base.try_read_prg_ram(0x6000), None);
        assert_eq!(base.wram_size(), 0);
        assert!(base.wram_snapshot().is_empty());
    }

    #[test]
    fn test_base_mapper_chr_read_write_ram() {
        let ctx = MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![], // empty = CHR-RAM
            NametableLayout::Horizontal,
        );
        let mut base = BaseMapper::new(&ctx, MapperCapabilities::default());

        base.write_chr(0x0000, 0xAA);
        assert_eq!(base.read_chr(0x0000), 0xAA);
    }

    #[test]
    fn test_base_mapper_chr_read_rom() {
        let ctx = MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![0x55; 8192],
            NametableLayout::Horizontal,
        );
        let mut base = BaseMapper::new(&ctx, MapperCapabilities::default());

        assert_eq!(base.read_chr(0x0000), 0x55);
        base.write_chr(0x0000, 0xAA); // should be ignored (ROM)
        assert_eq!(base.read_chr(0x0000), 0x55);
    }

    #[test]
    fn test_base_mapper_chr_read_write_auto_dispatch_with_banking() {
        let ctx =
            MapperContext::new_for_test(0, vec![0; 0x8000], vec![], NametableLayout::Horizontal);
        let mut base = BaseMapper::new(&ctx, MapperCapabilities::default());
        base.configure_chr_banking(0x1000); // 2x 4KB slots over 8KB CHR-RAM
        base.select_chr_page(0, 1);
        base.select_chr_page(1, 0);

        base.write_chr(0x0000, 0x3C); // Should write through banked dispatch to bank 1
        assert_eq!(base.read_chr(0x0000), 0x3C);
        assert_eq!(base.read_chr_at_index(0x1000), 0x3C);
        assert_eq!(base.read_chr_at_index(0x0000), 0x00);
    }

    #[test]
    fn test_base_mapper_mirroring() {
        let ctx = MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![0; 8192],
            NametableLayout::Vertical,
        );
        let mut base = BaseMapper::new(&ctx, MapperCapabilities::default());

        assert_eq!(base.mirroring(), NametableLayout::Vertical);
        base.set_mirroring(NametableLayout::Horizontal);
        assert_eq!(base.mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_set_mirroring_hv_true_sets_horizontal() {
        let ctx = MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![0; 8192],
            NametableLayout::Vertical,
        );
        let mut base = BaseMapper::new(&ctx, MapperCapabilities::default());

        base.set_mirroring_hv(true);
        assert_eq!(base.mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_set_mirroring_hv_false_sets_vertical() {
        let ctx = MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![0; 8192],
            NametableLayout::Horizontal,
        );
        let mut base = BaseMapper::new(&ctx, MapperCapabilities::default());

        base.set_mirroring_hv(false);
        assert_eq!(base.mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn test_base_mapper_wram_snapshot_restore() {
        let mut base = make_base_mapper_with_prg_ram(1);
        base.try_write_prg_ram(0x6000, 0x42);
        base.try_write_prg_ram(0x7FFF, 0xAB);

        let snapshot = base.wram_snapshot();
        assert_eq!(snapshot.len(), 8192);

        let mut base2 = make_base_mapper_with_prg_ram(1);
        base2.load_wram_snapshot(&snapshot);
        assert_eq!(base2.try_read_prg_ram(0x6000), Some(0x42));
        assert_eq!(base2.try_read_prg_ram(0x7FFF), Some(0xAB));
    }

    #[test]
    fn test_base_mapper_chr_ram_snapshot_restore() {
        let ctx =
            MapperContext::new_for_test(0, vec![0; 0x8000], vec![], NametableLayout::Horizontal);
        let mut base = BaseMapper::new(&ctx, MapperCapabilities::default());
        base.write_chr(0x0000, 0xAA);
        base.write_chr(0x1FFF, 0xBB);

        let snapshot = base.chr_ram_snapshot();

        let ctx2 =
            MapperContext::new_for_test(0, vec![0; 0x8000], vec![], NametableLayout::Horizontal);
        let mut base2 = BaseMapper::new(&ctx2, MapperCapabilities::default());
        base2.restore_chr_ram(&snapshot);
        assert_eq!(base2.read_chr(0x0000), 0xAA);
        assert_eq!(base2.read_chr(0x1FFF), 0xBB);
    }

    #[test]
    fn test_base_mapper_open_bus_no_prg_ram() {
        let base = make_base_mapper_with_prg_ram(0);

        // Below $6000: open bus
        assert_eq!(base.read_prg_open_bus(0x5000, 0x42, |_| 0xFF), 0x42);
        // $6000-$7FFF with no PRG-RAM: open bus
        assert_eq!(base.read_prg_open_bus(0x6000, 0x42, |_| 0xFF), 0x42);
        // $8000+: delegates to read_prg
        assert_eq!(base.read_prg_open_bus(0x8000, 0x42, |_| 0xAA), 0xAA);
    }

    #[test]
    fn test_base_mapper_open_bus_with_prg_ram() {
        let base = make_base_mapper_with_prg_ram(1);

        // $6000-$7FFF with PRG-RAM: delegates to read_prg
        assert_eq!(base.read_prg_open_bus(0x6000, 0x42, |_| 0xBB), 0xBB);
    }

    #[test]
    fn test_base_mapper_open_bus_uses_prg_6000_mapping_without_prg_ram() {
        let base = make_prg_6000_mapper(6);

        assert_eq!(
            base.read_prg_open_bus(0x6000, 0x42, |addr| {
                base.try_read_prg_6000(addr).unwrap_or(0x99)
            }),
            0,
            "$6000 banking must override open bus even when no PRG-RAM exists"
        );
    }

    #[test]
    fn test_base_mapper_mapper_number() {
        let ctx = MapperContext::new_for_test(
            7,
            vec![0; 0x8000],
            vec![0; 8192],
            NametableLayout::Horizontal,
        );
        let base = BaseMapper::new(&ctx, MapperCapabilities::default());
        assert_eq!(base.mapper_number(), 7);
    }

    #[test]
    fn test_base_mapper_capabilities() {
        let caps = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            ..Default::default()
        };
        let ctx = MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![0; 8192],
            NametableLayout::Horizontal,
        );
        let base = BaseMapper::new(&ctx, caps.clone());
        assert_eq!(base.capabilities(), caps);
    }

    // ========================================================================
    // PRG Banking Tests
    // ========================================================================

    /// Build ROM data where each bank is filled with its index as a marker byte.
    fn make_bank_marked_rom(num_banks: usize, bank_size: usize) -> Vec<u8> {
        (0..num_banks)
            .flat_map(|bank| std::iter::repeat_n(bank as u8, bank_size))
            .collect()
    }

    /// Helper: create a BaseMapper with identifiable PRG-ROM banks.
    fn make_prg_banked_mapper(num_banks: usize, page_size: usize) -> BaseMapper {
        let prg_rom = make_bank_marked_rom(num_banks, page_size);
        let ctx =
            MapperContext::new_for_test(0, prg_rom, vec![0; 8192], NametableLayout::Horizontal);
        let mut base = BaseMapper::new(&ctx, MapperCapabilities::default());
        base.configure_prg_banking(page_size);
        base
    }

    #[test]
    fn test_configure_prg_banking_sets_page_count() {
        let base = make_prg_banked_mapper(4, 0x2000);
        assert_eq!(base.prg_bank_count(), 4);
    }

    #[test]
    fn test_configure_prg_banking_16kb_pages() {
        let base = make_prg_banked_mapper(4, 0x4000);
        assert_eq!(base.prg_bank_count(), 4);
    }

    #[test]
    fn test_select_prg_page_and_read() {
        let mut base = make_prg_banked_mapper(4, 0x2000);
        base.select_prg_page(0, 2);
        assert_eq!(base.read_prg_banked(0x8000), 2);
        assert_eq!(base.read_prg_banked(0x9FFF), 2);
    }

    #[test]
    fn test_select_prg_page_multiple_slots() {
        let mut base = make_prg_banked_mapper(4, 0x2000);
        base.select_prg_page(0, 3);
        base.select_prg_page(1, 1);
        base.select_prg_page(2, 0);
        base.select_prg_page(3, 2);

        assert_eq!(base.read_prg_banked(0x8000), 3);
        assert_eq!(base.read_prg_banked(0xA000), 1);
        assert_eq!(base.read_prg_banked(0xC000), 0);
        assert_eq!(base.read_prg_banked(0xE000), 2);
    }

    #[test]
    fn test_select_prg_page_negative_bank() {
        let mut base = make_prg_banked_mapper(4, 0x2000);
        base.select_prg_page(3, -1);
        assert_eq!(base.read_prg_banked(0xE000), 3);
        base.select_prg_page(2, -2);
        assert_eq!(base.read_prg_banked(0xC000), 2);
    }

    #[test]
    fn test_select_prg_page_bank_wraps() {
        let mut base = make_prg_banked_mapper(4, 0x2000);
        base.select_prg_page(0, 5);
        assert_eq!(base.read_prg_banked(0x8000), 1); // 5 % 4 = 1
    }

    #[test]
    fn test_prg_page_query() {
        let mut base = make_prg_banked_mapper(4, 0x2000);
        base.select_prg_page(1, 3);
        assert_eq!(base.prg_page(1), 3);
    }

    #[test]
    fn test_read_prg_banked_16kb_pages() {
        let mut base = make_prg_banked_mapper(2, 0x4000);
        base.select_prg_page(0, 0);
        base.select_prg_page(1, 1);
        assert_eq!(base.read_prg_banked(0x8000), 0);
        assert_eq!(base.read_prg_banked(0xBFFF), 0);
        assert_eq!(base.read_prg_banked(0xC000), 1);
        assert_eq!(base.read_prg_banked(0xFFFF), 1);
    }

    #[test]
    fn test_read_prg_banked_32kb_page() {
        let mut base = make_prg_banked_mapper(2, 0x8000);
        base.select_prg_page(0, 1);
        assert_eq!(base.read_prg_banked(0x8000), 1);
        assert_eq!(base.read_prg_banked(0xFFFF), 1);
    }

    // ========================================================================
    // NROM PRG Banking Tests
    // ========================================================================

    #[test]
    fn test_nrom128_mirrors_same_bank_to_both_slots() {
        // 6 banks avoids power-of-two wrapping false-passes
        let mut base = make_prg_banked_mapper(6, 0x4000);
        base.apply_nrom_prg_banking(3, true);
        assert_eq!(base.read_prg_banked(0x8000), 3);
        assert_eq!(base.read_prg_banked(0xC000), 3);
    }

    #[test]
    fn test_nrom256_selects_consecutive_bank_pair() {
        let mut base = make_prg_banked_mapper(6, 0x4000);
        base.apply_nrom_prg_banking(2, false);
        assert_eq!(base.read_prg_banked(0x8000), 2);
        assert_eq!(base.read_prg_banked(0xC000), 3);
    }

    #[test]
    fn test_nrom256_clears_bit0_for_slot0() {
        let mut base = make_prg_banked_mapper(6, 0x4000);
        // Passing odd bank 3: slot 0 should get bank 2 (bit 0 cleared),
        // slot 1 should get bank 3 (bit 0 set).
        base.apply_nrom_prg_banking(3, false);
        assert_eq!(base.read_prg_banked(0x8000), 2);
        assert_eq!(base.read_prg_banked(0xC000), 3);
    }

    #[test]
    fn test_nrom128_bank_zero() {
        let mut base = make_prg_banked_mapper(6, 0x4000);
        base.apply_nrom_prg_banking(0, true);
        assert_eq!(base.read_prg_banked(0x8000), 0);
        assert_eq!(base.read_prg_banked(0xC000), 0);
    }

    #[test]
    fn test_nrom256_bank_zero() {
        let mut base = make_prg_banked_mapper(6, 0x4000);
        base.apply_nrom_prg_banking(0, false);
        assert_eq!(base.read_prg_banked(0x8000), 0);
        assert_eq!(base.read_prg_banked(0xC000), 1);
    }

    #[test]
    fn test_nrom128_wraps_bank_to_available() {
        // 6 banks → bank 7 should wrap to bank 1 (7 % 6 = 1)
        let mut base = make_prg_banked_mapper(6, 0x4000);
        base.apply_nrom_prg_banking(7, true);
        assert_eq!(base.read_prg_banked(0x8000), 1);
        assert_eq!(base.read_prg_banked(0xC000), 1);
    }

    #[test]
    fn test_nrom256_wraps_bank_to_available() {
        // 6 banks → bank 4: slot 0 = 4 % 6 = 4, slot 1 = 5 % 6 = 5
        let mut base = make_prg_banked_mapper(6, 0x4000);
        base.apply_nrom_prg_banking(4, false);
        assert_eq!(base.read_prg_banked(0x8000), 4);
        assert_eq!(base.read_prg_banked(0xC000), 5);
    }

    // ========================================================================
    // $6000 PRG-ROM Banking Tests
    // ========================================================================

    /// Helper: create a BaseMapper with identifiable 8KB PRG-ROM banks and $6000 banking.
    fn make_prg_6000_mapper(num_8k_banks: usize) -> BaseMapper {
        let prg_rom = make_bank_marked_rom(num_8k_banks, 0x2000);
        let ctx =
            MapperContext::new_for_test(0, prg_rom, vec![0; 8192], NametableLayout::Horizontal);
        let mut base = BaseMapper::new(&ctx, MapperCapabilities::default());
        base.configure_prg_6000_banking();
        base
    }

    #[test]
    fn test_prg_6000_default_maps_bank_0() {
        let base = make_prg_6000_mapper(6);
        assert_eq!(base.try_read_prg_6000(0x6000), Some(0));
        assert_eq!(base.try_read_prg_6000(0x7FFF), Some(0));
    }

    #[test]
    fn test_prg_6000_select_bank() {
        let mut base = make_prg_6000_mapper(6);
        base.select_prg_6000_page(3);
        assert_eq!(base.try_read_prg_6000(0x6000), Some(3));
        assert_eq!(base.try_read_prg_6000(0x7FFF), Some(3));
    }

    #[test]
    fn test_prg_6000_bank_wraps() {
        // 6 banks → bank 7 wraps to 1 (7 % 6 = 1)
        let mut base = make_prg_6000_mapper(6);
        base.select_prg_6000_page(7);
        assert_eq!(base.try_read_prg_6000(0x6000), Some(1));
    }

    #[test]
    fn test_prg_6000_negative_bank() {
        // 6 banks → -1 = last bank = 5
        let mut base = make_prg_6000_mapper(6);
        base.select_prg_6000_page(-1);
        assert_eq!(base.try_read_prg_6000(0x6000), Some(5));
    }

    #[test]
    fn test_prg_6000_returns_none_when_not_configured() {
        // No configure_prg_6000_banking() call
        let prg_rom = make_bank_marked_rom(4, 0x2000);
        let ctx =
            MapperContext::new_for_test(0, prg_rom, vec![0; 8192], NametableLayout::Horizontal);
        let base = BaseMapper::new(&ctx, MapperCapabilities::default());
        assert_eq!(base.try_read_prg_6000(0x6000), None);
    }

    #[test]
    fn test_prg_6000_returns_none_outside_range() {
        let base = make_prg_6000_mapper(6);
        assert_eq!(base.try_read_prg_6000(0x5FFF), None);
        assert_eq!(base.try_read_prg_6000(0x8000), None);
    }

    #[test]
    fn test_has_prg_6000_banking_false_by_default() {
        let ctx = MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![0; 8192],
            NametableLayout::Horizontal,
        );
        let base = BaseMapper::new(&ctx, MapperCapabilities::default());
        assert!(!base.has_prg_6000_banking());
    }

    #[test]
    fn test_has_prg_6000_banking_true_after_configure() {
        let base = make_prg_6000_mapper(4);
        assert!(base.has_prg_6000_banking());
    }

    // ========================================================================
    // CHR Banking Tests
    // ========================================================================

    fn make_chr_banked_mapper(num_chr_banks: usize, chr_page_size: usize) -> BaseMapper {
        let chr_rom = make_bank_marked_rom(num_chr_banks, chr_page_size);
        let ctx =
            MapperContext::new_for_test(0, vec![0; 0x8000], chr_rom, NametableLayout::Horizontal);
        let mut base = BaseMapper::new(&ctx, MapperCapabilities::default());
        base.configure_chr_banking(chr_page_size);
        base
    }

    #[test]
    fn test_configure_chr_banking_sets_bank_count() {
        let base = make_chr_banked_mapper(8, 0x0400);
        assert_eq!(base.chr_bank_count(), 8);
    }

    #[test]
    fn test_select_chr_page_and_read() {
        let mut base = make_chr_banked_mapper(8, 0x0400);
        base.select_chr_page(0, 5);
        assert_eq!(base.read_chr_banked(0x0000), 5);
        assert_eq!(base.read_chr_banked(0x03FF), 5);
    }

    #[test]
    fn test_chr_banking_multiple_slots() {
        let mut base = make_chr_banked_mapper(16, 0x0400);
        for slot in 0..8 {
            base.select_chr_page(slot, slot as i16 + 4);
        }
        for slot in 0..8 {
            let addr = (slot * 0x0400) as u16;
            assert_eq!(base.read_chr_banked(addr), (slot + 4) as u8);
        }
    }

    #[test]
    fn test_chr_banking_negative_bank() {
        let mut base = make_chr_banked_mapper(8, 0x0400);
        base.select_chr_page(7, -1);
        assert_eq!(base.read_chr_banked(0x1C00), 7);
    }

    #[test]
    fn test_chr_banking_4kb_pages() {
        let mut base = make_chr_banked_mapper(4, 0x1000);
        base.select_chr_page(0, 2);
        base.select_chr_page(1, 3);
        assert_eq!(base.read_chr_banked(0x0000), 2);
        assert_eq!(base.read_chr_banked(0x0FFF), 2);
        assert_eq!(base.read_chr_banked(0x1000), 3);
        assert_eq!(base.read_chr_banked(0x1FFF), 3);
    }

    #[test]
    fn test_write_chr_banked_ram() {
        let ctx =
            MapperContext::new_for_test(0, vec![0; 0x8000], vec![], NametableLayout::Horizontal);
        let mut base = BaseMapper::new(&ctx, MapperCapabilities::default());
        base.configure_chr_banking(0x2000);
        base.select_chr_page(0, 0);
        base.write_chr_banked(0x0000, 0xAB);
        assert_eq!(base.read_chr_banked(0x0000), 0xAB);
    }

    #[test]
    fn test_write_chr_rom_is_noop_unbanked() {
        let chr_rom = vec![0x42; 0x2000];
        let ctx =
            MapperContext::new_for_test(0, vec![0; 0x8000], chr_rom, NametableLayout::Horizontal);
        let mut base = BaseMapper::new(&ctx, MapperCapabilities::default());
        // Attempt to write to CHR-ROM — should be silently ignored
        base.write_chr(0x0000, 0xFF);
        assert_eq!(base.read_chr(0x0000), 0x42);
    }

    #[test]
    fn test_write_chr_rom_is_noop_banked() {
        let chr_rom = vec![0x42; 0x2000];
        let ctx =
            MapperContext::new_for_test(0, vec![0; 0x8000], chr_rom, NametableLayout::Horizontal);
        let mut base = BaseMapper::new(&ctx, MapperCapabilities::default());
        base.configure_chr_banking(0x2000);
        base.select_chr_page(0, 0);
        // Attempt to write to CHR-ROM via banked path — should be silently ignored
        base.write_chr_banked(0x0000, 0xFF);
        assert_eq!(base.read_chr_banked(0x0000), 0x42);
    }

    #[test]
    fn test_chr_page_query() {
        let mut base = make_chr_banked_mapper(8, 0x0400);
        base.select_chr_page(3, 7);
        assert_eq!(base.chr_page(3), 7);
    }

    // ========================================================================
    // Banking Snapshot/Restore Tests
    // ========================================================================

    #[test]
    fn test_banking_snapshot_prg_only() {
        let mut base = make_prg_banked_mapper(4, 0x4000);
        base.select_prg_page(0, 2);
        base.select_prg_page(1, 3);
        let snapshot = base.banking_snapshot();
        assert_eq!(snapshot, vec![2, 3]);
    }

    #[test]
    fn test_banking_snapshot_prg_and_chr() {
        let prg_rom = vec![0; 0x10000]; // 64KB = 4 banks of 16KB
        let chr_rom = make_bank_marked_rom(8, 0x0400);
        let ctx = MapperContext::new_for_test(0, prg_rom, chr_rom, NametableLayout::Horizontal);
        let mut base = BaseMapper::new(&ctx, MapperCapabilities::default());
        base.configure_prg_banking(0x4000);
        base.configure_chr_banking(0x0400);
        base.select_prg_page(0, 1);
        base.select_prg_page(1, 2);
        base.select_chr_page(0, 5);
        base.select_chr_page(1, 6);
        let snapshot = base.banking_snapshot();
        // PRG pages first, then CHR pages
        assert_eq!(snapshot, vec![1, 2, 5, 6, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn test_banking_snapshot_with_dynamic_mirroring() {
        let caps = MapperCapabilities {
            has_dynamic_mirroring: true,
            ..Default::default()
        };
        let ctx =
            MapperContext::new_for_test(0, vec![0; 0x8000], vec![], NametableLayout::Horizontal);
        let mut base = BaseMapper::new(&ctx, caps);
        base.configure_prg_banking(0x8000);
        base.select_prg_page(0, 0);
        base.set_mirroring_hv(true); // Horizontal
        let snapshot = base.banking_snapshot();
        // [prg_page0, mirroring(H=0 via to_snapshot_byte)]
        assert_eq!(snapshot, vec![0, 0]);
    }

    #[test]
    fn test_banking_snapshot_without_dynamic_mirroring_excludes_mirroring() {
        let mut base = make_prg_banked_mapper(2, 0x8000);
        base.select_prg_page(0, 1);
        let snapshot = base.banking_snapshot();
        assert_eq!(snapshot, vec![1]); // No mirroring byte
    }

    #[test]
    fn test_restore_banking_prg_only() {
        let mut base = make_prg_banked_mapper(4, 0x4000);
        base.select_prg_page(0, 0);
        base.select_prg_page(1, 0);
        base.restore_banking(&[2, 3]);
        assert_eq!(base.prg_page(0), 2);
        assert_eq!(base.prg_page(1), 3);
    }

    #[test]
    fn test_restore_banking_prg_and_chr() {
        let prg_rom = vec![0; 0x10000]; // 64KB = 4 banks of 16KB
        let chr_rom = make_bank_marked_rom(8, 0x0400);
        let ctx = MapperContext::new_for_test(0, prg_rom, chr_rom, NametableLayout::Horizontal);
        let mut base = BaseMapper::new(&ctx, MapperCapabilities::default());
        base.configure_prg_banking(0x4000);
        base.configure_chr_banking(0x0400);
        base.restore_banking(&[1, 2, 5, 6, 0, 0, 0, 0, 0, 0]);
        assert_eq!(base.prg_page(0), 1);
        assert_eq!(base.prg_page(1), 2);
        assert_eq!(base.chr_page(0), 5);
        assert_eq!(base.chr_page(1), 6);
    }

    #[test]
    fn test_restore_banking_with_mirroring() {
        let caps = MapperCapabilities {
            has_dynamic_mirroring: true,
            ..Default::default()
        };
        let ctx =
            MapperContext::new_for_test(0, vec![0; 0x8000], vec![], NametableLayout::Vertical);
        let mut base = BaseMapper::new(&ctx, caps);
        base.configure_prg_banking(0x8000);
        base.restore_banking(&[0, 0]); // page0=0, mirroring=Horizontal (H=0 via from_snapshot_byte)
        assert_eq!(base.prg_page(0), 0);
        assert_eq!(base.mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_banking_snapshot_restore_roundtrip() {
        let prg_rom = vec![0; 0x10000]; // 64KB = 4 banks of 16KB
        let chr_rom = make_bank_marked_rom(8, 0x0400);
        let ctx = MapperContext::new_for_test(0, prg_rom, chr_rom, NametableLayout::Horizontal);
        let mut base = BaseMapper::new(&ctx, MapperCapabilities::default());
        base.configure_prg_banking(0x4000);
        base.configure_chr_banking(0x0400);
        base.select_prg_page(0, 1);
        base.select_prg_page(1, 3);
        base.select_chr_page(0, 7);
        base.select_chr_page(3, 2);
        let snapshot = base.banking_snapshot();
        // Reset all pages
        base.select_prg_page(0, 0);
        base.select_prg_page(1, 0);
        base.select_chr_page(0, 0);
        base.select_chr_page(3, 0);
        base.restore_banking(&snapshot);
        assert_eq!(base.prg_page(0), 1);
        assert_eq!(base.prg_page(1), 3);
        assert_eq!(base.chr_page(0), 7);
        assert_eq!(base.chr_page(3), 2);
    }

    // ========================================================================
    // Bus Conflict Tests
    // ========================================================================

    #[test]
    fn test_bus_conflicts_disabled_returns_original() {
        let base = make_prg_banked_mapper(2, 0x4000);
        assert!(!base.has_bus_conflicts());
        assert_eq!(base.apply_bus_conflict(0x8000, 0xFF), 0xFF);
    }

    #[test]
    fn test_bus_conflicts_enabled_ands_with_rom() {
        let mut base = make_prg_banked_mapper(2, 0x4000);
        base.set_bus_conflicts(true);
        base.select_prg_page(0, 0);
        assert_eq!(base.apply_bus_conflict(0x8000, 0xFF), 0x00);
    }

    #[test]
    fn test_bus_conflicts_with_nonzero_rom() {
        let mut base = make_prg_banked_mapper(4, 0x2000);
        base.set_bus_conflicts(true);
        base.select_prg_page(0, 1);
        assert_eq!(base.apply_bus_conflict(0x8000, 0xFF), 0x01);
    }
}
