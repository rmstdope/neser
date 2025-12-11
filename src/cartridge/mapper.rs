use crate::cartridge::MirroringMode;
use std::io;

use super::axrom::AxROMMapper;
use super::cnrom::CNROMMapper;
use super::mmc1::MMC1Mapper;
use super::nrom::NROMMapper;
use super::uxrom::UxROMMapper;

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
        7 => Ok(Box::new(AxROMMapper::new(prg_rom, chr_rom, mirroring))),
        _ => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("Mapper {} not implemented", mapper_number),
        )),
    }
}
