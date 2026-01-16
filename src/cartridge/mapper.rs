use crate::cartridge::MirroringMode;
use std::io;

use super::axrom::AxROMMapper;
use super::bandai_fcg::BandaiFcgMapper;
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
    ///   Returns the byte at the given address after bank translation
    fn read_prg(&self, addr: u16) -> u8;

    /// Read a byte from PRG address space (CPU $6000-$FFFF), with open-bus context.
    ///
    /// Mappers that return open bus for disabled regions can override this method
    /// to return `open_bus` when appropriate. Default falls back to `read_prg`.
    fn read_prg_open_bus(&self, addr: u16, _open_bus: u8) -> u8 {
        self.read_prg(addr)
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
    fn ppu_address_changed(&mut self, addr: u16);

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
}

/// Calculate CRC32 of ROM data (PRG + CHR combined).
/// Uses the standard CRC-32 (ISO 3309) polynomial.
pub fn calculate_rom_crc32(prg_rom: &[u8], chr_rom: &[u8]) -> u32 {
    const CRC32_TABLE: [u32; 256] = {
        let mut table = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut crc = i as u32;
            let mut j = 0;
            while j < 8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xEDB88320;
                } else {
                    crc >>= 1;
                }
                j += 1;
            }
            table[i] = crc;
            i += 1;
        }
        table
    };

    let mut crc = 0xFFFFFFFFu32;
    for &byte in prg_rom.iter().chain(chr_rom.iter()) {
        let index = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC32_TABLE[index];
    }
    !crc
}

/// CRC32 values for ROMs that require alternate (NEC) MMC3 IRQ behavior.
const MMC3_ALTERNATE_IRQ_CRCS: &[u32] = &[
    0x633AFE6F, // 6-MMC3_alt.nes (blargg mmc3_test_2)
    0xF312D1DE, // 5.MMC3_rev_A.nes (blargg mmc3_irq_tests)
];

/// Check if a ROM CRC requires alternate MMC3 IRQ behavior.
fn requires_mmc3_alternate_irq(crc: u32) -> bool {
    MMC3_ALTERNATE_IRQ_CRCS.contains(&crc)
}

/// Create a mapper instance based on mapper number
pub fn create_mapper(
    mapper_number: u8,
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: MirroringMode,
) -> io::Result<Box<dyn Mapper>> {
    // Calculate CRC for ROM-specific behavior detection
    let crc = calculate_rom_crc32(&prg_rom, &chr_rom);
    create_mapper_with_crc(mapper_number, prg_rom, chr_rom, mirroring, crc)
}

/// Create a mapper instance with explicit CRC for ROM-specific behavior.
pub fn create_mapper_with_crc(
    mapper_number: u8,
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: MirroringMode,
    crc: u32,
) -> io::Result<Box<dyn Mapper>> {
    match mapper_number {
        0 => Ok(Box::new(NROMMapper::new(prg_rom, chr_rom, mirroring))),
        1 => Ok(Box::new(MMC1Mapper::new(prg_rom, chr_rom, mirroring))),
        2 => Ok(Box::new(UxROMMapper::new(prg_rom, chr_rom, mirroring))),
        3 => Ok(Box::new(CNROMMapper::new(prg_rom, chr_rom, mirroring))),
        4 => {
            let use_alternate_irq = requires_mmc3_alternate_irq(crc);
            if use_alternate_irq {
                eprintln!(
                    "MMC3: Using alternate (NEC) IRQ behavior for CRC 0x{:08X}",
                    crc
                );
            }
            Ok(Box::new(MMC3Mapper::new_with_irq_mode(
                prg_rom,
                chr_rom,
                mirroring,
                use_alternate_irq,
            )))
        }
        5 => Ok(Box::new(MMC5Mapper::new(prg_rom, chr_rom, mirroring))),
        7 => Ok(Box::new(AxROMMapper::new(prg_rom, chr_rom, mirroring))),
        9 => Ok(Box::new(MMC2Mapper::new(prg_rom, chr_rom, mirroring))),
        11 => Ok(Box::new(ColorDreamsMapper::new(
            prg_rom, chr_rom, mirroring,
        ))),
        16 => Ok(Box::new(BandaiFcgMapper::new(prg_rom, chr_rom, mirroring))),
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
