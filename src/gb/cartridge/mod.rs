#[allow(clippy::module_inception)]
mod cartridge;
mod mbc0;
mod mbc1;
mod mbc2;
mod mbc3;
mod mbc5;

pub use cartridge::GbCartridge;
use mbc0::Mbc0;
use mbc1::Mbc1;
use mbc2::Mbc2;
use mbc3::Mbc3;
use mbc5::Mbc5;

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
/// 3. MBC type at 0x0147 must be supported (0x00–0x03, 0x05–0x06, 0x19–0x1E) — returns [`RomError::UnsupportedMbc`].
pub fn load_cartridge(bytes: &[u8]) -> Result<Box<dyn GbCartridge>, RomError> {
    if bytes.len() < 0x8000 {
        return Err(RomError::TooShort);
    }
    // Valid GB ROMs are always a multiple of 16 KB.  Passing a non-aligned slice
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
        n => Err(RomError::UnsupportedMbc(n)),
    }
}

#[cfg(test)]
mod tests {
    use super::{RomError, compute_header_checksum, load_cartridge};

    /// Build a syntactically valid ROM of the given MBC type and ROM-size byte.
    fn make_valid_rom(mbc_type: u8, rom_size_byte: u8) -> Vec<u8> {
        let bank_count: usize = 2 << (rom_size_byte as usize);
        let mut rom = vec![0u8; bank_count * 0x4000];
        rom[0x0147] = mbc_type;
        rom[0x0148] = rom_size_byte;
        rom[0x0149] = 0x00; // no RAM
        let checksum = compute_header_checksum(&rom);
        rom[0x014D] = checksum;
        rom
    }

    #[test]
    fn test_load_returns_error_for_too_short_input() {
        // Given: an input shorter than 32 KB
        let short = vec![0u8; 0x100];
        // Then: TooShort error
        assert!(matches!(load_cartridge(&short), Err(RomError::TooShort)));
    }

    #[test]
    fn test_load_returns_error_for_bad_header_checksum() {
        // Given: a valid 32 KB ROM with a corrupted checksum byte
        let mut rom = make_valid_rom(0x00, 0x00);
        rom[0x014D] = rom[0x014D].wrapping_add(1); // corrupt
        // Then: BadHeaderChecksum error
        let result = load_cartridge(&rom);
        assert!(matches!(result, Err(RomError::BadHeaderChecksum { .. })));
    }

    #[test]
    fn test_load_returns_ok_for_valid_mbc0_rom() {
        // Given: a valid ROM-only cartridge
        let rom = make_valid_rom(0x00, 0x00);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_valid_mbc1_rom() {
        // Given: a valid MBC1 cartridge (64 KB)
        let rom = make_valid_rom(0x01, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc2_rom() {
        // Given: a valid MBC2 cartridge (type 0x05)
        let rom = make_valid_rom(0x05, 0x00);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc2_battery_rom() {
        // Given: a valid MBC2+BATTERY cartridge (type 0x06)
        let rom = make_valid_rom(0x06, 0x00);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc5_rom() {
        // Given: a valid MBC5 cartridge (type 0x19 = MBC5)
        let rom = make_valid_rom(0x19, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc5_ram_battery_rom() {
        // Given: a valid MBC5+RAM+BATTERY cartridge (type 0x1B)
        let rom = make_valid_rom(0x1B, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc5_rumble_rom() {
        // Given: a valid MBC5+RUMBLE cartridge (type 0x1C)
        let rom = make_valid_rom(0x1C, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc5_rumble_ram_battery_rom() {
        // Given: a valid MBC5+RUMBLE+RAM+BATTERY cartridge (type 0x1E)
        let rom = make_valid_rom(0x1E, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc3_timer_battery_rom() {
        // Given: a valid MBC3+TIMER+BATTERY cartridge (type 0x0F)
        let rom = make_valid_rom(0x0F, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc3_timer_ram_battery_rom() {
        // Given: a valid MBC3+TIMER+RAM+BATTERY cartridge (type 0x10)
        let rom = make_valid_rom(0x10, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc3_rom() {
        // Given: a valid MBC3 cartridge (type 0x11)
        let rom = make_valid_rom(0x11, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc3_ram_rom() {
        // Given: a valid MBC3+RAM cartridge (type 0x12)
        let rom = make_valid_rom(0x12, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_ok_for_mbc3_ram_battery_rom() {
        // Given: a valid MBC3+RAM+BATTERY cartridge (type 0x13)
        let rom = make_valid_rom(0x13, 0x01);
        assert!(load_cartridge(&rom).is_ok());
    }

    #[test]
    fn test_load_returns_error_for_unsupported_mbc_type() {
        // Given: a ROM with an unrecognised MBC type byte (0xFF)
        let rom = make_valid_rom(0xFF, 0x00);
        assert!(matches!(
            load_cartridge(&rom),
            Err(RomError::UnsupportedMbc(0xFF))
        ));
    }
}
