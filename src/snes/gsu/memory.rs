//! Super FX cartridge address decoding, for both sides of the Game Pak bus.
//!
//! fullsnes "SNES Cart GSU-n Memory Map": the ROM is visible twice, as LoROM (32 KB banks at
//! `$8000-$FFFF`) and as HiROM (linear 64 KB banks from `$40`), and the GSU additionally sees the
//! LoROM banks mirrored into their own lower halves (for "GETB R15" vectors). Game Pak RAM sits
//! in banks `$70-$71`, and the S-CPU also sees its first 8 KB at `$6000-$7FFF` of the system
//! banks. Indices returned here are linear offsets into the ROM or RAM image; callers wrap them
//! to the physical size.

/// Where an S-CPU address lands on a Super FX cartridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SnesTarget {
    /// The GSU register file and code-cache window, `$3000-$34FF` of the system banks, given as
    /// the offset within the bank.
    Registers(u16),
    /// A linear Game Pak ROM offset.
    Rom(usize),
    /// A linear Game Pak RAM offset.
    Ram(usize),
}

/// Decodes an S-CPU address on a Super FX cartridge. `None` is open bus (or, in the system banks,
/// something the S-CPU itself owns, such as WRAM or its I/O).
pub(crate) fn decode_snes(addr: u32) -> Option<SnesTarget> {
    let bank = ((addr >> 16) & 0xFF) as u8;
    let offset = (addr & 0xFFFF) as u16;
    match bank {
        0x00..=0x3F | 0x80..=0xBF => match offset {
            0x3000..=0x34FF => Some(SnesTarget::Registers(offset)),
            0x6000..=0x7FFF => Some(SnesTarget::Ram(usize::from(offset - 0x6000))),
            0x8000..=0xFFFF => Some(SnesTarget::Rom(lorom_index(bank, offset))),
            _ => None,
        },
        0x40..=0x5F | 0xC0..=0xDF => Some(SnesTarget::Rom(hirom_index(bank, offset))),
        0x70..=0x71 | 0xF0..=0xF1 => Some(SnesTarget::Ram(ram_index(bank, offset))),
        _ => None,
    }
}

/// The GSU's own view of ROM: `$00-$3F` (both halves, the lower half mirroring the upper) as
/// LoROM, `$40-$5F` as HiROM. `None` outside the ROM banks.
pub(crate) fn gsu_rom_index(bank: u8, offset: u16) -> Option<usize> {
    match bank {
        0x00..=0x3F => Some(lorom_index(bank, offset)),
        0x40..=0x5F => Some(hirom_index(bank, offset)),
        _ => None,
    }
}

/// The GSU's own view of Game Pak RAM, banks `$70-$71`. `None` outside them.
pub(crate) fn gsu_ram_index(bank: u8, offset: u16) -> Option<usize> {
    matches!(bank, 0x70..=0x71).then(|| ram_index(bank, offset))
}

/// The byte the S-CPU reads from any ROM address while the GSU owns the ROM bus (GO and RON both
/// set). fullsnes "GSU Interrupt Vectors": ROM is replaced by fixed words that depend only on
/// address bits 0-3, so the exception vectors point into WRAM at `$0100`-`$010C` (COP `$0104`,
/// NMI `$0108`, IRQ `$010C`, everything else `$0100`).
pub(crate) fn rom_bus_blocked_byte(addr: u32) -> u8 {
    if addr & 0x01 != 0 {
        return 0x01;
    }
    match addr & 0x0E {
        0x04 => 0x04,
        0x0A => 0x08,
        0x0E => 0x0C,
        _ => 0x00,
    }
}

fn lorom_index(bank: u8, offset: u16) -> usize {
    usize::from(bank & 0x3F) * 0x8000 + usize::from(offset & 0x7FFF)
}

fn hirom_index(bank: u8, offset: u16) -> usize {
    (usize::from(bank & 0x1F) << 16) | usize::from(offset)
}

fn ram_index(bank: u8, offset: u16) -> usize {
    (usize::from(bank & 0x01) << 16) | usize::from(offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lorom_and_hirom_views_of_the_same_byte_agree() {
        assert_eq!(decode_snes(0x03_8123), Some(SnesTarget::Rom(0x1_8123)));
        assert_eq!(decode_snes(0x41_8123), Some(SnesTarget::Rom(0x1_8123)));
        assert_eq!(
            gsu_rom_index(0x03, 0x0123),
            Some(0x1_8123),
            "GSU lower-half mirror"
        );
        assert_eq!(gsu_rom_index(0x60, 0x0000), None);
    }

    #[test]
    fn fixed_vectors_match_the_fullsnes_table() {
        let word = |addr: u32| {
            u16::from_le_bytes([rom_bus_blocked_byte(addr), rom_bus_blocked_byte(addr + 1)])
        };
        assert_eq!(word(0x00_FFE4), 0x0104, "COP");
        assert_eq!(word(0x00_FFE6), 0x0100, "BRK");
        assert_eq!(word(0x00_FFEA), 0x0108, "NMI");
        assert_eq!(word(0x00_FFEE), 0x010C, "IRQ");
        assert_eq!(word(0x00_FFFC), 0x0100, "reset / any other");
    }
}
