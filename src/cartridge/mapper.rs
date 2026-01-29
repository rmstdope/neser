use crate::cartridge::MirroringMode;
use std::io;

use super::axrom::AxROMMapper;
use super::bandai_fcg::BandaiFcgMapper;
use super::bnrom_nina::BnromNinaMapper;
use super::camerica::CamericaMapper;
use super::cnrom::CNROMMapper;
use super::colordreams::ColorDreamsMapper;
use super::cprom::CpromMapper;
use super::gxrom::GxROMMapper;
use super::mmc1::MMC1Mapper;
use super::mmc2::MMC2Mapper;
use super::mmc3::MMC3Mapper;
use super::mmc4::MMC4Mapper;
use super::mmc5::MMC5Mapper;
use super::multicart_15::Multicart15Mapper;
use super::namco118::Namco118Mapper;
use super::namco163::Namco163Mapper;
use super::nina_tengen::NinaTengenMapper;
use super::nrom::NROMMapper;
use super::rom_db;
use super::sunsoft_4::Sunsoft4Mapper;
use super::sunsoft_fme7::SunsoftFme7Mapper;
use super::uxrom::UxROMMapper;
use super::vrc2_vrc4::Vrc2Vrc4Mapper;
use super::vrc6::VRC6Mapper;

/// Metadata for constructing a mapper, containing cartridge header details and
/// derived values (e.g., CRC32) used by the factory.
#[derive(Debug)]
#[allow(dead_code)]
pub struct MapperContext {
    /// iNES/NES 2.0 mapper number. Submapper is kept separately.
    pub mapper: u16,
    /// NES 2.0 submapper id (0 when not specified).
    pub submapper: u8,
    /// PPU nametable mirroring mode from the header.
    pub mirroring: MirroringMode,
    /// PRG ROM bytes.
    pub prg_rom: Vec<u8>,
    /// CHR ROM bytes (empty when CHR-RAM).
    pub chr_rom: Vec<u8>,
    /// PRG-RAM size in 8KB units (minimum 1).
    pub prg_ram_banks_8k: u8,
    /// Whether PRG-RAM is battery backed.
    pub battery_backed_prg_ram: bool,
    /// CRC32 of concatenated PRG/CHR; may be overridden for tests.
    pub crc32: u32,
}

#[allow(dead_code)]
impl MapperContext {
    /// Create mapper metadata with default submapper 0, 1×8KB PRG-RAM (not battery-backed),
    /// and CRC32 computed from PRG+CHR data.
    pub fn new(mapper: u8, prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        let crc32 = rom_db::calculate_rom_crc32(&prg_rom, &chr_rom);
        Self {
            mapper: mapper as u16,
            submapper: 0,
            mirroring,
            prg_rom,
            chr_rom,
            prg_ram_banks_8k: 1,
            battery_backed_prg_ram: false,
            crc32,
        }
    }

    /// Set NES 2.0 submapper id.
    pub fn with_submapper(mut self, submapper: u8) -> Self {
        self.submapper = submapper;
        self
    }

    /// Override PRG-RAM size in 8KB units (clamped to at least one bank).
    pub fn with_prg_ram_banks(mut self, prg_ram_banks_8k: u8) -> Self {
        self.prg_ram_banks_8k = prg_ram_banks_8k.max(1);
        self
    }

    /// Mark PRG-RAM as battery backed.
    pub fn with_battery_backed_prg_ram(mut self, battery_backed_prg_ram: bool) -> Self {
        self.battery_backed_prg_ram = battery_backed_prg_ram;
        self
    }

    /// Override CRC32 value (useful for tests with synthetic ROM data).
    pub fn with_crc32(mut self, crc32: u32) -> Self {
        self.crc32 = crc32;
        self
    }

    fn mapper_u8(&self) -> u8 {
        self.mapper as u8
    }

    fn into_parts(self) -> (Vec<u8>, Vec<u8>, MirroringMode) {
        (self.prg_rom, self.chr_rom, self.mirroring)
    }
}

pub trait Mapper {
    /// Read a byte from PRG address space (CPU $6000-$FFFF)
    /// - $6000-$7FFF: PRG-RAM (8KB, battery-backed on some cartridges)
    /// - $8000-$FFFF: PRG-ROM (with bank switching on advanced mappers)
    ///   Returns the byte at the given address after bank translation
    fn read_prg(&self, addr: u16) -> u8;

    /// Read a byte from PRG address space (CPU $6000-$FFFF), with open-bus context.
    ///
    /// Mappers that return open bus for disabled regions can override this method
    /// to return `open_bus` when appropriate. Default falls back to `read_prg`.
    fn read_prg_open_bus(&self, addr: u16, _open_bus: u8) -> u8 {
        if addr < 0x6000 {
            _open_bus
        } else {
            self.read_prg(addr)
        }
    }

    /// Write a byte to PRG address space (CPU $6000-$FFFF)
    /// - $6000-$7FFF: PRG-RAM (8KB, writable)
    /// - $8000-$FFFF: Mapper control registers (PRG-ROM is read-only)
    fn write_prg(&mut self, addr: u16, value: u8);

    /// Read a byte from CHR address space (PPU $0000-$1FFF)
    /// Returns the byte at the given address after bank translation
    fn read_chr(&self, addr: u16) -> u8;

    /// Write a byte to CHR address space (PPU $0000-$1FFF)
    /// Only works for CHR-RAM, CHR-ROM is read-only
    fn write_chr(&mut self, addr: u16, value: u8);

    /// Notify mapper of PPU address bus changes
    /// Used for detecting A12 rising edges (for MMC3 IRQ)
    /// Default implementation is a no-op.
    fn ppu_address_changed(&mut self, _addr: u16) {}

    /// Set the current PPU CHR fetch kind.
    ///
    /// Some mappers (e.g., MMC5) need to distinguish between background and sprite CHR fetches.
    /// Default implementation is a no-op.
    fn ppu_set_chr_fetch_is_sprite(&mut self, _is_sprite: bool) {}

    /// Notify mapper that PPUDATA ($2007) is about to read from CHR.
    ///
    /// Some mappers (e.g., MMC5) need to distinguish rendering fetches from PPUDATA reads.
    /// Default implementation is a no-op.
    fn ppu_set_chr_fetch_is_ppudata(&mut self) {}

    /// Notify mapper of a write to PPUCTRL ($2000).
    ///
    /// The MMC5 monitors this to detect 8x16 sprite mode (bit 5).
    /// Default implementation is a no-op.
    fn ppu_write_ctrl(&mut self, _value: u8) {}

    /// Notify mapper of a write to PPUMASK ($2001).
    ///
    /// The MMC5 monitors this to detect rendering enable (bits 3-4).
    /// Default implementation is a no-op.
    fn ppu_write_mask(&mut self, _value: u8) {}

    /// Notify mapper of a CPU write to $4014 (OAMDMA).
    ///
    /// Some mappers (e.g., MMC5) reset scanline counters on OAMDMA writes.
    /// Default implementation is a no-op.
    fn on_oam_dma(&mut self) {}

    /// Notify mapper of a CPU read from an interrupt vector ($FFFA-$FFFF).
    ///
    /// The `_addr` argument indicates which vector was read:
    ///   - $FFFA/$FFFB: NMI
    ///   - $FFFC/$FFFD: Reset
    ///   - $FFFE/$FFFF: IRQ/BRK
    ///
    /// Some mappers (e.g., MMC5) reset scanline/in-frame tracking on interrupt
    /// vector reads. Default implementation is a no-op.
    fn on_irq_vector_read(&mut self, _addr: u16) {}

    /// Notify mapper about the current scanline (during rendering) for PPU-driven IRQ systems.
    /// Default implementation is a no-op.
    fn ppu_scanline(&mut self, _scanline: u16, _rendering_enabled: bool) {}

    /// Notify mapper that a frame has ended.
    /// Default implementation is a no-op.
    fn ppu_end_frame(&mut self) {}

    /// Optional mapper override for PPU nametable reads ($2000-$3EFF).
    ///
    /// If the mapper wishes to supply the byte (e.g., MMC5 ExRAM/fill), return `Some(value)`.
    /// Return `None` to fall back to the PPU's internal nametable VRAM.
    fn read_nametable(&mut self, _addr: u16) -> Option<u8> {
        None
    }

    /// Optional mapper override for PPU nametable writes ($2000-$3EFF).
    ///
    /// Return `true` if the mapper handled the write, `false` to fall back to internal VRAM.
    fn write_nametable(&mut self, _addr: u16, _value: u8) -> bool {
        false
    }

    /// Notify mapper that a CPU cycle has elapsed.
    ///
    /// Some mappers implement CPU-cycle-driven IRQ systems (e.g., Konami VRC IRQ).
    /// Default implementation is a no-op.
    fn cpu_cycle(&mut self) {}

    /// Reset the mapper to its power-on state.
    ///
    /// This is called when the NES is reset. Mappers should reset their internal
    /// state (bank registers, IRQ counters, etc.) but typically preserve PRG-RAM contents.
    /// Default implementation is a no-op for simple mappers.
    fn reset(&mut self) {}

    /// Whether the mapper is currently asserting IRQ.
    ///
    /// This is used to model mapper-generated IRQs (e.g., MMC3 scanline IRQ).
    /// Default implementation returns false for mappers without IRQ support.
    fn irq_pending(&self) -> bool {
        false
    }

    /// Current expansion-audio output sample contributed by the mapper.
    ///
    /// Some mappers provide additional sound channels (e.g., Konami VRC6).
    /// The returned value should be a linear sample intended to be added to the
    /// base APU mix (typically in a small range like 0.0..~0.5).
    ///
    /// Default implementation returns 0.0 for mappers without expansion audio.
    fn expansion_audio_sample(&self) -> f32 {
        0.0
    }

    /// Get the current nametable mirroring mode
    /// Some mappers can change mirroring dynamically
    fn get_mirroring(&self) -> MirroringMode;

    /// Get the size of cartridge WRAM (PRG-RAM) in bytes.
    ///
    /// Returns the total size of battery-backed PRG-RAM that should be persisted to disk.
    /// Default implementation returns 8KB (0x2000 bytes), the standard PRG-RAM size.
    fn wram_size(&self) -> usize {
        0x2000
    }

    /// Create a snapshot of all cartridge WRAM (PRG-RAM) for persistence.
    ///
    /// This reads the raw WRAM independent of:
    /// - Enable/disable state (e.g., MMC3 PRG-RAM enable)
    /// - Write-protect state
    /// - Current bank mapping (for mappers with banked WRAM like MMC5)
    ///
    /// Returns a Vec containing the complete WRAM contents.
    /// Default implementation reads via read_prg at $6000-$7FFF (8KB window).
    /// **Mappers with >8KB WRAM MUST override this method to capture all banks.**
    fn wram_snapshot(&self) -> Vec<u8> {
        let size = self.wram_size().min(0x2000);
        let mut snapshot = Vec::with_capacity(size);
        for i in 0..size {
            snapshot.push(self.read_prg(0x6000 + i as u16));
        }
        snapshot
    }

    /// Load a WRAM snapshot from persistence.
    ///
    /// This writes the raw WRAM independent of:
    /// - Enable/disable state (e.g., MMC3 PRG-RAM enable)
    /// - Write-protect state
    /// - Current bank mapping (for mappers with banked WRAM like MMC5)
    ///
    /// The data slice should contain the complete WRAM contents to restore.
    /// Default implementation writes via write_prg at $6000-$7FFF (8KB window).
    /// **Mappers with >8KB WRAM MUST override this method to restore all banks.**
    fn load_wram_snapshot(&mut self, data: &[u8]) {
        let to_copy = data.len().min(0x2000).min(self.wram_size());
        for (i, &byte) in data.iter().take(to_copy).enumerate() {
            self.write_prg(0x6000 + i as u16, byte);
        }
    }

    /// Get the mapper number (iNES mapper ID).
    ///
    /// Returns the iNES mapper number for this mapper (e.g., 0 for NROM, 1 for MMC1).
    /// Default implementation returns 0.
    fn mapper_number(&self) -> u8 {
        0
    }

    /// Create a snapshot of PRG-RAM for save-state.
    ///
    /// Default implementation uses wram_snapshot.
    fn prg_ram_snapshot(&self) -> Vec<u8> {
        self.wram_snapshot()
    }

    /// Create a snapshot of CHR-RAM for save-state.
    ///
    /// Default implementation returns empty (CHR-ROM has no state to save).
    fn chr_ram_snapshot(&self) -> Vec<u8> {
        Vec::new()
    }

    /// Create a snapshot of mapper-specific registers for save-state.
    ///
    /// Default implementation returns empty (for simple mappers with no internal state).
    fn registers_snapshot(&self) -> Vec<u8> {
        Vec::new()
    }

    /// Restore PRG-RAM from a save-state.
    ///
    /// Default implementation uses load_wram_snapshot.
    fn restore_prg_ram(&mut self, data: &[u8]) {
        self.load_wram_snapshot(data);
    }

    /// Restore CHR-RAM from a save-state.
    ///
    /// Default implementation is a no-op.
    fn restore_chr_ram(&mut self, _data: &[u8]) {}

    /// Restore mapper-specific registers from a save-state.
    ///
    /// Default implementation is a no-op.
    fn restore_registers(&mut self, _data: &[u8]) {}
}

fn vrc2_vrc4_21(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Vrc2Vrc4Mapper {
    Vrc2Vrc4Mapper::new(21, prg_rom, chr_rom, mirroring)
}

fn vrc2_vrc4_22(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Vrc2Vrc4Mapper {
    Vrc2Vrc4Mapper::new(22, prg_rom, chr_rom, mirroring)
}

fn vrc2_vrc4_23(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Vrc2Vrc4Mapper {
    Vrc2Vrc4Mapper::new(23, prg_rom, chr_rom, mirroring)
}

fn vrc2_vrc4_25(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Vrc2Vrc4Mapper {
    Vrc2Vrc4Mapper::new(25, prg_rom, chr_rom, mirroring)
}

fn vrc6_24(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> VRC6Mapper {
    VRC6Mapper::new(24, prg_rom, chr_rom, mirroring)
}

fn vrc6_26(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> VRC6Mapper {
    VRC6Mapper::new(26, prg_rom, chr_rom, mirroring)
}

macro_rules! mapper_registry {
    ($($id:expr => $ctor:path),+ $(,)?) => {
        fn create_registry_mapper(
            metadata: MapperContext,
        ) -> Option<Box<dyn Mapper>> {
            match metadata.mapper_u8() {
                $(
                    $id => {
                        let (prg_rom, chr_rom, mirroring) = metadata.into_parts();
                        Some(Box::new($ctor(prg_rom, chr_rom, mirroring)))
                    }
                )+
                _ => None,
            }
        }
    };
}

mapper_registry! {
    0 => NROMMapper::new,
    1 => MMC1Mapper::new,
    2 => UxROMMapper::new,
    3 => CNROMMapper::new,
    7 => AxROMMapper::new,
    9 => MMC2Mapper::new,
    10 => MMC4Mapper::new,
    11 => ColorDreamsMapper::new,
    13 => CpromMapper::new,
    15 => Multicart15Mapper::new,
    16 => BandaiFcgMapper::new,
    19 => Namco163Mapper::new,
    21 => vrc2_vrc4_21,
    22 => vrc2_vrc4_22,
    23 => vrc2_vrc4_23,
    24 => vrc6_24,
    25 => vrc2_vrc4_25,
    26 => vrc6_26,
    34 => BnromNinaMapper::new,
    66 => GxROMMapper::new,
    69 => SunsoftFme7Mapper::new,
    71 => CamericaMapper::new,
    78 => NinaTengenMapper::new,
    206 => Namco118Mapper::new,
}

#[cfg(test)]
const SUPPORTED_MAPPERS: &[u8] = &[
    4, // MMC3 is constructed with CRC-specific behavior.
    0, 1, 2, 3, 5, 7, 9, 10, 11, 13, 15, 16, 19, 21, 22, 23, 24, 25, 26, 34, 66, 68, 69, 71, 78,
    206,
];

/// List of supported iNES mapper IDs handled by the factory.
#[cfg(test)]
pub fn supported_mappers() -> &'static [u8] {
    SUPPORTED_MAPPERS
}

/// Create a mapper instance based on mapper metadata.
pub fn create_mapper(metadata: MapperContext) -> io::Result<Box<dyn Mapper>> {
    let mapper_number = metadata.mapper_u8();

    if mapper_number == 4 {
        let crc32 = metadata.crc32;
        let use_alternate_irq = rom_db::requires_mmc3_alternate_irq(crc32);
        let (prg_rom, chr_rom, mirroring) = metadata.into_parts();
        // if use_alternate_irq {
        //     eprintln!(
        //         "MMC3: Using alternate (NEC) IRQ behavior for CRC 0x{:08X}",
        //         crc32
        //     );
        // }
        return Ok(Box::new(MMC3Mapper::new_with_irq_mode(
            prg_rom,
            chr_rom,
            mirroring,
            use_alternate_irq,
        )));
    }

    if mapper_number == 5 {
        let prg_ram_banks_8k = metadata.prg_ram_banks_8k;
        let (prg_rom, chr_rom, mirroring) = metadata.into_parts();
        return Ok(Box::new(MMC5Mapper::new_with_prg_ram_size(
            prg_rom,
            chr_rom,
            mirroring,
            prg_ram_banks_8k,
        )));
    }

    if mapper_number == 68 {
        let prg_ram_banks_8k = metadata.prg_ram_banks_8k;
        let (prg_rom, chr_rom, mirroring) = metadata.into_parts();
        return Ok(Box::new(Sunsoft4Mapper::new_with_prg_ram_banks(
            prg_rom,
            chr_rom,
            mirroring,
            prg_ram_banks_8k,
        )));
    }

    if let Some(mapper) = create_registry_mapper(metadata) {
        return Ok(mapper);
    }

    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        format!("Mapper {} not implemented", mapper_number),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::MirroringMode;

    #[test]
    fn test_supported_mappers_contains_common_ids() {
        let supported = supported_mappers();
        assert!(supported.contains(&0));
        assert!(supported.contains(&1));
        assert!(supported.contains(&2));
        assert!(supported.contains(&3));
        assert!(supported.contains(&4));
        assert!(supported.contains(&5));
        assert!(supported.contains(&7));
    }

    #[test]
    fn create_mapper_uses_metadata_for_mmc5_prg_ram_size() {
        let prg_rom = vec![0u8; 8 * 1024 * 4];
        let chr_rom = vec![0u8; 8 * 1024];
        let metadata = MapperContext::new(5, prg_rom, chr_rom, MirroringMode::Horizontal)
            .with_prg_ram_banks(2);

        let mapper = create_mapper(metadata).expect("MMC5 mapper should be created");
        assert_eq!(mapper.wram_size(), 16 * 1024);
    }

    #[test]
    fn test_ppu_address_changed_default_is_noop() {
        // Test that simple mappers can use the default no-op implementation
        let prg_rom = vec![0u8; 32 * 1024];
        let chr_rom = vec![0u8; 8 * 1024];
        let metadata = MapperContext::new(0, prg_rom, chr_rom, MirroringMode::Horizontal);

        let mut mapper = create_mapper(metadata).expect("NROM mapper should be created");

        // Call ppu_address_changed - should be a no-op and not panic
        mapper.ppu_address_changed(0x0000);
        mapper.ppu_address_changed(0x1000);
        mapper.ppu_address_changed(0x1FFF);
    }
}
