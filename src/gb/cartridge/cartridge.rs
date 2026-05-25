use super::huc1::Huc1;
/// Game Boy cartridge interface.
///
/// Each cartridge type (MBC0, MBC1, …) implements this trait.
/// The `DmgBus` holds a `Box<dyn GbCartridge>` and delegates ROM/RAM
/// accesses to it.
use super::mbc0::{Mbc0, RomRam};
use super::mbc1::Mbc1;
use super::mbc2::Mbc2;
use super::mbc3::Mbc3;
use super::mbc5::Mbc5;
use super::mbc7::Mbc7;

/// Canonical 48-byte Nintendo logo stored at ROM header addresses $0104-$0133.
pub const NINTENDO_LOGO: [u8; 48] = [
    0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B, 0x03, 0x73, 0x00, 0x83, 0x00, 0x0C, 0x00, 0x0D,
    0x00, 0x08, 0x11, 0x1F, 0x88, 0x89, 0x00, 0x0E, 0xDC, 0xCC, 0x6E, 0xE6, 0xDD, 0xDD, 0xD9, 0x99,
    0xBB, 0xBB, 0x67, 0x63, 0x6E, 0x0E, 0xEC, 0xCC, 0xDD, 0xDC, 0x99, 0x9F, 0xBB, 0xB9, 0x33, 0x3E,
];

pub trait GbCartridge {
    /// Read a byte from the cartridge address space.
    ///
    /// Addresses $0000–$7FFF map to ROM; $A000–$BFFF map to cartridge RAM.
    /// Reads outside those ranges return 0xFF.
    fn read(&self, addr: u16) -> u8;

    /// Write a byte to the cartridge address space.
    ///
    /// Writes to ROM ($0000–$7FFF) are interpreted as MBC register writes.
    /// Writes to cartridge RAM ($A000–$BFFF) store data when RAM is enabled.
    fn write(&mut self, addr: u16, val: u8);

    /// Returns `true` when the ROM header indicates CGB compatibility.
    ///
    /// Checks byte 0x0143: values 0x80 (CGB+DMG) or 0xC0 (CGB-only)
    /// indicate a CGB-compatible cartridge. This gates CGB-specific
    /// hardware behavior (e.g., APU length counter rules).
    fn is_cgb(&self) -> bool {
        let flag = self.read(0x0143);
        flag == 0x80 || flag == 0xC0
    }

    /// Snapshot all cartridge RAM (battery-backed SRAM) as a byte vector.
    ///
    /// Returns an empty vector for cartridges without RAM (e.g., MBC0).
    fn ram_snapshot(&self) -> Vec<u8> {
        Vec::new()
    }

    /// Restore cartridge RAM from a previously saved snapshot.
    ///
    /// Truncates or ignores extra bytes silently. Does nothing for
    /// cartridges without RAM.
    fn restore_ram(&mut self, _data: &[u8]) {}

    /// Snapshot the MBC-internal register state as opaque bytes.
    ///
    /// Used by save states to capture bank-select registers, mode flags,
    /// and other MBC-specific state that isn't part of the address space.
    /// Returns an empty vector for cartridges without MBC registers.
    fn mbc_state_snapshot(&self) -> Vec<u8> {
        Vec::new()
    }

    /// Restore MBC-internal register state from a previously saved snapshot.
    ///
    /// Does nothing for cartridges without MBC registers or if data is
    /// malformed/empty.
    fn restore_mbc_state(&mut self, _data: &[u8]) {}

    /// Returns `true` when the cartridge has battery-backed RAM.
    ///
    /// Used to decide whether to load/save `.sav` files.
    fn has_battery(&self) -> bool {
        false
    }

    /// Tick the cartridge by the given number of M-cycles.
    ///
    /// Used by cartridges with real-time clocks (RTC) like MBC3 to track
    /// elapsed time. Does nothing by default for most cartridge types.
    fn tick(&mut self, _cycles: u32) {}
}

/// Returns whether the cartridge header contains the canonical Nintendo logo.
pub fn has_canonical_nintendo_logo(cart: &dyn GbCartridge) -> bool {
    NINTENDO_LOGO
        .iter()
        .enumerate()
        .all(|(offset, &byte)| cart.read(0x0104 + offset as u16) == byte)
}

/// Errors returned by [`load_cartridge`].
#[derive(Debug, PartialEq)]
pub enum RomError {
    /// The supplied byte slice is shorter than a valid GB ROM (minimum 32 KB).
    TooShort,
    /// The header checksum byte does not match the computed value.
    BadHeaderChecksum { expected: u8, actual: u8 },
    /// The cartridge type byte at 0x0147 maps to an MBC not yet supported.
    UnsupportedMbc(u8),
}

/// Compute the GB header checksum over bytes 0x0134–0x014C.
///
/// The algorithm is: `x = 0; for each byte b in range: x = x - b - 1`.
/// The result (mod 256) is stored at 0x014D.
fn compute_header_checksum(bytes: &[u8]) -> u8 {
    bytes[0x0134..=0x014C]
        .iter()
        .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1))
}

/// Derive the RAM size (in bytes) from the RAM size byte at 0x0149.
fn ram_size_from_byte(byte: u8) -> usize {
    match byte {
        0x01 => 2 * 1024,
        0x02 => 8 * 1024,
        0x03 => 32 * 1024,
        0x04 => 128 * 1024,
        0x05 => 64 * 1024,
        _ => 0,
    }
}

/// Parse raw `.gb` ROM bytes and return the appropriate cartridge implementation.
///
/// Validations performed:
/// 1. Length must be at least 32 KB (0x8000) — returns [`RomError::TooShort`].
/// 2. Header checksum at 0x014D must be correct — returns [`RomError::BadHeaderChecksum`].
/// 3. MBC type at 0x0147 must be supported (0x00–0x03, 0x05–0x06, 0x08–0x09, 0x19–0x1E, 0x22, 0xFF) — returns [`RomError::UnsupportedMbc`].
pub fn load_cartridge(bytes: &[u8]) -> Result<Box<dyn GbCartridge>, RomError> {
    if bytes.len() < 0x8000 {
        return Err(RomError::TooShort);
    }
    // Valid GB ROMs are always a multiple of 16 KB. Passing a non-aligned slice
    // is harmless — the MBC implementations compute bank count as `len / 0x4000`,
    // so any trailing partial bank simply becomes unreachable (reads return 0xFF
    // via slice::get + unwrap_or).

    let expected = compute_header_checksum(bytes);
    let actual = bytes[0x014D];
    if expected != actual {
        return Err(RomError::BadHeaderChecksum { expected, actual });
    }

    let mbc_type = bytes[0x0147];
    match mbc_type {
        0x00 => Ok(Box::new(Mbc0::new(bytes.to_vec()))),
        0x08..=0x09 => {
            let ram_size = ram_size_from_byte(bytes[0x0149]);
            let has_battery = mbc_type == 0x09;
            Ok(Box::new(RomRam::new(
                bytes.to_vec(),
                vec![0u8; ram_size],
                has_battery,
            )))
        }
        0x01..=0x03 => {
            let ram_size = ram_size_from_byte(bytes[0x0149]);
            let has_battery = mbc_type == 0x03;
            Ok(Box::new(Mbc1::new(
                bytes.to_vec(),
                vec![0u8; ram_size],
                has_battery,
            )))
        }
        0x05..=0x06 => {
            let has_battery = mbc_type == 0x06;
            Ok(Box::new(Mbc2::new(bytes.to_vec(), has_battery)))
        }
        0x0F..=0x13 => {
            let ram_size = ram_size_from_byte(bytes[0x0149]);
            let has_rtc = matches!(mbc_type, 0x0F | 0x10);
            let has_battery = matches!(mbc_type, 0x0F | 0x10 | 0x13);
            Ok(Box::new(Mbc3::new(
                bytes.to_vec(),
                ram_size,
                has_rtc,
                has_battery,
            )))
        }
        0x19..=0x1E => {
            let ram_size = ram_size_from_byte(bytes[0x0149]);
            let has_rumble = mbc_type >= 0x1C;
            let has_battery = matches!(mbc_type, 0x1B | 0x1E);
            Ok(Box::new(Mbc5::new(
                bytes.to_vec(),
                vec![0u8; ram_size],
                has_rumble,
                has_battery,
            )))
        }
        0x22 => Ok(Box::new(Mbc7::new(bytes.to_vec()))),
        0xFF => {
            let ram_size = ram_size_from_byte(bytes[0x0149]);
            Ok(Box::new(Huc1::new(bytes.to_vec(), vec![0u8; ram_size])))
        }
        n => Err(RomError::UnsupportedMbc(n)),
    }
}

#[cfg(test)]
mod tests {
    use super::{RomError, compute_header_checksum, load_cartridge};

    /// Build a syntactically valid ROM of the given MBC type and ROM-size byte.
    fn make_valid_rom(mbc_type: u8, rom_size_byte: u8) -> Vec<u8> {
        make_valid_rom_with_ram(mbc_type, rom_size_byte, 0x00)
    }

    fn make_valid_rom_with_ram(mbc_type: u8, rom_size_byte: u8, ram_size_byte: u8) -> Vec<u8> {
        let bank_count: usize = 2 << (rom_size_byte as usize);
        let mut rom = vec![0u8; bank_count * 0x4000];
        rom[0x0147] = mbc_type;
        rom[0x0148] = rom_size_byte;
        rom[0x0149] = ram_size_byte;
        let checksum = compute_header_checksum(&rom);
        rom[0x014D] = checksum;
        rom
    }

    #[test]
    fn test_load_returns_error_for_too_short_input() {
        let short = vec![0u8; 0x100];
        assert!(matches!(load_cartridge(&short), Err(RomError::TooShort)));
    }

    #[test]
    fn test_load_returns_error_for_bad_header_checksum() {
        let mut rom = make_valid_rom(0x00, 0x00);
        rom[0x014D] = rom[0x014D].wrapping_add(1);
        let result = load_cartridge(&rom);
        assert!(matches!(result, Err(RomError::BadHeaderChecksum { .. })));
    }

    #[test]
    fn test_load_returns_ok_for_valid_mbc0_rom() {
        let rom = make_valid_rom(0x00, 0x00);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_valid_mbc1_rom() {
        let rom = make_valid_rom(0x01, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_rom_ram_rom() {
        let rom = make_valid_rom_with_ram(0x08, 0x00, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_rom_ram_battery_rom() {
        let rom = make_valid_rom_with_ram(0x09, 0x00, 0x01);
        let cart = load_cartridge(&rom).expect("ROM+RAM+BATTERY should load");
        assert!(cart.has_battery());
    }

    #[test]
    fn test_load_returns_ok_for_mbc2_rom() {
        let rom = make_valid_rom(0x05, 0x00);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc2_battery_rom() {
        let rom = make_valid_rom(0x06, 0x00);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc5_rom() {
        let rom = make_valid_rom(0x19, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc5_ram_battery_rom() {
        let rom = make_valid_rom(0x1B, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc5_rumble_rom() {
        let rom = make_valid_rom(0x1C, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc5_rumble_ram_battery_rom() {
        let rom = make_valid_rom(0x1E, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc3_timer_battery_rom() {
        let rom = make_valid_rom(0x0F, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc3_timer_ram_battery_rom() {
        let rom = make_valid_rom(0x10, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc3_rom() {
        let rom = make_valid_rom(0x11, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc3_ram_rom() {
        let rom = make_valid_rom(0x12, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc3_ram_battery_rom() {
        let rom = make_valid_rom(0x13, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc7_rom() {
        let rom = make_valid_rom(0x22, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_huc1_rom() {
        let rom = make_valid_rom(0xFF, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_error_for_unsupported_mbc_type() {
        let rom = make_valid_rom(0x07, 0x00); // Use 0x07 which is not supported
        assert!(matches!(
            load_cartridge(&rom),
            Err(RomError::UnsupportedMbc(0x07))
        ));
    }
}
