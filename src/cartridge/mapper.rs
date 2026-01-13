use crate::cartridge::MirroringMode;
use std::io;

use super::axrom::AxROMMapper;
use super::cnrom::CNROMMapper;
use super::colordreams::ColorDreamsMapper;
use super::gxrom::GxROMMapper;
use super::mmc1::MMC1Mapper;
use super::mmc2::MMC2Mapper;
use super::mmc3::MMC3Mapper;
use super::mmc5::MMC5Mapper;
use super::nrom::NROMMapper;
use super::uxrom::UxROMMapper;
use super::vrc6::VRC6Mapper;

pub trait Mapper {
    /// Read a byte from PRG address space (CPU $6000-$FFFF)
    /// - $6000-$7FFF: PRG-RAM (8KB, battery-backed on some cartridges)
    /// - $8000-$FFFF: PRG-ROM (with bank switching on advanced mappers)
    /// Returns the byte at the given address after bank translation
    fn read_prg(&self, addr: u16) -> u8;

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
    fn ppu_address_changed(&mut self, addr: u16);

    /// Set the current PPU CHR fetch kind.
    ///
    /// Some mappers (e.g., MMC5) need to distinguish between background and sprite CHR fetches.
    /// Default implementation is a no-op.
    fn ppu_set_chr_fetch_is_sprite(&mut self, _is_sprite: bool) {}

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
}

/// Create a mapper instance based on mapper number
pub fn create_mapper(
    mapper_number: u8,
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: MirroringMode,
) -> io::Result<Box<dyn Mapper>> {
    match mapper_number {
        0 => Ok(Box::new(NROMMapper::new(prg_rom, chr_rom, mirroring))),
        1 => Ok(Box::new(MMC1Mapper::new(prg_rom, chr_rom, mirroring))),
        2 => Ok(Box::new(UxROMMapper::new(prg_rom, chr_rom, mirroring))),
        3 => Ok(Box::new(CNROMMapper::new(prg_rom, chr_rom, mirroring))),
        4 => Ok(Box::new(MMC3Mapper::new(prg_rom, chr_rom, mirroring))),
        5 => Ok(Box::new(MMC5Mapper::new(prg_rom, chr_rom, mirroring))),
        7 => Ok(Box::new(AxROMMapper::new(prg_rom, chr_rom, mirroring))),
        9 => Ok(Box::new(MMC2Mapper::new(prg_rom, chr_rom, mirroring))),
        11 => Ok(Box::new(ColorDreamsMapper::new(
            prg_rom, chr_rom, mirroring,
        ))),
        24 => Ok(Box::new(VRC6Mapper::new(24, prg_rom, chr_rom, mirroring))),
        26 => Ok(Box::new(VRC6Mapper::new(26, prg_rom, chr_rom, mirroring))),
        66 => Ok(Box::new(GxROMMapper::new(prg_rom, chr_rom, mirroring))),
        _ => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("Mapper {} not implemented", mapper_number),
        )),
    }
}

/// Create a mapper instance using cartridge metadata.
///
/// `prg_ram_banks_8k` is PRG-RAM size in 8KB units (iNES v1 header byte 8).
///
/// Currently only MMC5 uses PRG-RAM sizing metadata.
pub fn create_mapper_with_prg_ram_size(
    mapper_number: u8,
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: MirroringMode,
    prg_ram_banks_8k: u8,
) -> io::Result<Box<dyn Mapper>> {
    match mapper_number {
        5 => Ok(Box::new(MMC5Mapper::new_with_prg_ram_size(
            prg_rom,
            chr_rom,
            mirroring,
            prg_ram_banks_8k,
        ))),
        _ => create_mapper(mapper_number, prg_rom, chr_rom, mirroring),
    }
}
