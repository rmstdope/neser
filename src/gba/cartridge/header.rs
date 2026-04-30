//! GBA cartridge ROM header parsing.
//!
//! The 192-byte GBA cartridge header lives at offset `0x000` of the ROM image
//! and is documented at <https://problemkaputt.de/gbatek.htm#gbacartridgeheader>.
//!
//! The fields exposed here cover what the emulator currently needs:
//!
//! * Game title (12 ASCII bytes at `0x0A0`)
//! * Game code (4 ASCII bytes at `0x0AC`)
//! * Maker code (2 ASCII bytes at `0x0B0`)
//! * Fixed byte at `0x0B2` (must be `0x96`)
//! * Header complement check at `0x0BD`
//!
//! Validation is intentionally lenient: a bad fixed byte or a wrong
//! complement check is reported through [`GbaHeader::is_valid`] but the
//! caller decides whether to refuse to boot the ROM.

/// Required size of the GBA cartridge header (the area covered by the
/// complement check spans `0x0A0..=0x0BC`, but the full ROM-internal header
/// is 192 bytes).
pub const HEADER_SIZE: usize = 0xC0;

/// Offset of the game title (12 bytes).
pub const TITLE_OFFSET: usize = 0xA0;
pub const TITLE_LEN: usize = 12;

/// Offset of the 4-byte game code.
pub const GAME_CODE_OFFSET: usize = 0xAC;
pub const GAME_CODE_LEN: usize = 4;

/// Offset of the 2-byte maker code.
pub const MAKER_CODE_OFFSET: usize = 0xB0;
pub const MAKER_CODE_LEN: usize = 2;

/// Offset of the fixed byte (must equal `0x96`).
pub const FIXED_BYTE_OFFSET: usize = 0xB2;
pub const FIXED_BYTE_VALUE: u8 = 0x96;

/// Offset of the header complement check byte.
pub const COMPLEMENT_CHECK_OFFSET: usize = 0xBD;
/// Range over which the complement check is computed (inclusive).
pub const COMPLEMENT_CHECK_RANGE: std::ops::RangeInclusive<usize> = 0xA0..=0xBC;

/// Parsed GBA cartridge header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GbaHeader {
    /// Game title, ASCII, NUL-padded (12 bytes raw).
    pub title: String,
    /// 4-byte ASCII game code (e.g. `"AGBE"`).
    pub game_code: String,
    /// 2-byte ASCII maker code (e.g. `"01"` for Nintendo).
    pub maker_code: String,
    /// Fixed byte at `0x0B2`. Should be `0x96` on a real cartridge.
    pub fixed_byte: u8,
    /// Header complement check byte read from the ROM at `0x0BD`.
    pub complement_check: u8,
    /// Computed expected complement check value.
    pub computed_complement: u8,
}

impl GbaHeader {
    /// Returns `true` when both the fixed byte and the complement check
    /// match the values mandated by the GBA boot ROM.
    pub fn is_valid(&self) -> bool {
        self.fixed_byte == FIXED_BYTE_VALUE && self.complement_check == self.computed_complement
    }
}

/// Errors that can occur while parsing a [`GbaHeader`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderError {
    /// ROM is shorter than [`HEADER_SIZE`].
    TooShort,
}

/// Parse the GBA cartridge header from the start of `rom`.
///
/// The header is parsed unconditionally — a bad fixed byte or complement
/// check is surfaced through the returned [`GbaHeader::is_valid`] helper
/// rather than refused outright, matching how real GBAs accept many
/// homebrew ROMs that do not bother to set those fields.
pub fn parse_header(rom: &[u8]) -> Result<GbaHeader, HeaderError> {
    if rom.len() < HEADER_SIZE {
        return Err(HeaderError::TooShort);
    }

    let title = ascii_string(&rom[TITLE_OFFSET..TITLE_OFFSET + TITLE_LEN]);
    let game_code = ascii_string(&rom[GAME_CODE_OFFSET..GAME_CODE_OFFSET + GAME_CODE_LEN]);
    let maker_code = ascii_string(&rom[MAKER_CODE_OFFSET..MAKER_CODE_OFFSET + MAKER_CODE_LEN]);

    let fixed_byte = rom[FIXED_BYTE_OFFSET];
    let complement_check = rom[COMPLEMENT_CHECK_OFFSET];
    let computed_complement = compute_complement_check(rom);

    Ok(GbaHeader {
        title,
        game_code,
        maker_code,
        fixed_byte,
        complement_check,
        computed_complement,
    })
}

/// Compute the GBA header complement check value over `rom[0xA0..=0xBC]`.
///
/// Per GBATek the algorithm is:
///
/// ```text
/// chk = 0
/// for byte b in rom[0xA0..=0xBC]:
///     chk = chk - b
/// chk = (chk - 0x19) & 0xFF
/// ```
pub fn compute_complement_check(rom: &[u8]) -> u8 {
    if rom.len() <= *COMPLEMENT_CHECK_RANGE.end() {
        return 0;
    }
    let sum = rom[COMPLEMENT_CHECK_RANGE]
        .iter()
        .fold(0u8, |acc, &b| acc.wrapping_sub(b));
    sum.wrapping_sub(0x19)
}

/// Decode an ASCII run that may be NUL-padded into a [`String`].
fn ascii_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a 192-byte header populated with a known title, game code, maker
    /// code, the canonical fixed byte and a correct complement check.
    fn make_valid_header(title: &str, game_code: &str, maker_code: &str) -> Vec<u8> {
        let mut rom = vec![0u8; HEADER_SIZE];
        let n = title.len().min(TITLE_LEN);
        rom[TITLE_OFFSET..TITLE_OFFSET + n].copy_from_slice(&title.as_bytes()[..n]);
        rom[GAME_CODE_OFFSET..GAME_CODE_OFFSET + GAME_CODE_LEN]
            .copy_from_slice(game_code.as_bytes());
        rom[MAKER_CODE_OFFSET..MAKER_CODE_OFFSET + MAKER_CODE_LEN]
            .copy_from_slice(maker_code.as_bytes());
        rom[FIXED_BYTE_OFFSET] = FIXED_BYTE_VALUE;
        rom[COMPLEMENT_CHECK_OFFSET] = compute_complement_check(&rom);
        rom
    }

    #[test]
    fn rejects_rom_shorter_than_header() {
        let rom = vec![0u8; HEADER_SIZE - 1];
        assert_eq!(parse_header(&rom), Err(HeaderError::TooShort));
    }

    #[test]
    fn parses_title_game_and_maker_code() {
        let rom = make_valid_header("POKEMON RED ", "AGBE", "01");
        let header = parse_header(&rom).unwrap();
        // Title is 12 ASCII bytes — no NUL terminator means trailing
        // padding stays put (the ROM filled the whole field).
        assert_eq!(header.title, "POKEMON RED ");
        assert_eq!(header.game_code, "AGBE");
        assert_eq!(header.maker_code, "01");
    }

    #[test]
    fn valid_header_has_canonical_fixed_byte_and_passes_checksum() {
        let rom = make_valid_header("HELLO", "ABCD", "EF");
        let header = parse_header(&rom).unwrap();
        assert_eq!(header.fixed_byte, FIXED_BYTE_VALUE);
        assert_eq!(header.complement_check, header.computed_complement);
        assert!(header.is_valid());
    }

    #[test]
    fn detects_bad_fixed_byte() {
        let mut rom = make_valid_header("HELLO", "ABCD", "EF");
        rom[FIXED_BYTE_OFFSET] = 0x00;
        // The complement check covers 0xA0..=0xBC, including the fixed byte,
        // so changing it also invalidates the checksum unless we recompute.
        rom[COMPLEMENT_CHECK_OFFSET] = compute_complement_check(&rom);
        let header = parse_header(&rom).unwrap();
        assert!(!header.is_valid());
    }

    #[test]
    fn detects_bad_complement_check() {
        let mut rom = make_valid_header("HELLO", "ABCD", "EF");
        rom[COMPLEMENT_CHECK_OFFSET] = rom[COMPLEMENT_CHECK_OFFSET].wrapping_add(1);
        let header = parse_header(&rom).unwrap();
        assert!(!header.is_valid());
    }

    #[test]
    fn null_padded_title_truncates_at_first_zero() {
        let mut rom = vec![0u8; HEADER_SIZE];
        rom[TITLE_OFFSET..TITLE_OFFSET + 4].copy_from_slice(b"GAME");
        rom[FIXED_BYTE_OFFSET] = FIXED_BYTE_VALUE;
        rom[COMPLEMENT_CHECK_OFFSET] = compute_complement_check(&rom);
        let header = parse_header(&rom).unwrap();
        assert_eq!(header.title, "GAME");
    }

    #[test]
    fn complement_check_matches_known_vector() {
        // All-zero header: expected = -(0x19) & 0xFF = 0xE7
        let rom = vec![0u8; HEADER_SIZE];
        assert_eq!(compute_complement_check(&rom), 0xE7);
    }
}
