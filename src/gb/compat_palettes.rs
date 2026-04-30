//! CGB DMG compatibility palette selection.
//!
//! When a DMG-only game runs on CGB hardware, the CGB selects colorization
//! palettes based on the game's title checksum and licensee code. This module
//! implements the palette lookup algorithm as used by the original CGB boot ROM.
//!
//! Reference: SameBoy `cgb_boot.asm` and Pan Docs Power_Up_Sequence.

/// ROM header offsets (relative to header start at $0100)
const TITLE_START: usize = 0x34;
const TITLE_END: usize = 0x43;
const FOURTH_LETTER_OFFSET: usize = 0x37;
const NEW_LICENSEE_HIGH: usize = 0x44;
const NEW_LICENSEE_LOW: usize = 0x45;
const OLD_LICENSEE: usize = 0x4B;

/// Index into TITLE_CHECKSUMS where duplicates requiring tie-breaker begin.
const FIRST_DUPLICATE_INDEX: usize = 65;

/// Title checksums for games with known palettes.
/// Index 0 is the default palette (checksum $00).
/// Indices 0-64 are unique checksums; indices 65+ have duplicates requiring tie-breaker.
#[rustfmt::skip]
const TITLE_CHECKSUMS: [u8; 79] = [
    0x00, // Default
    0x88, // ALLEY WAY
    0x16, // YAKUMAN
    0x36, // BASEBALL
    0xD1, // TENNIS
    0xDB, // TETRIS
    0xF2, // QIX
    0x3C, // DR.MARIO
    0x8C, // RADARMISSION
    0x92, // F1RACE
    0x3D, // YOSSY NO TAMAGO
    0x5C, //
    0x58, // X
    0xC9, // MARIOLAND2
    0x3E, // YOSSY NO COOKIE
    0x70, // ZELDA
    0x1D, //
    0x59, //
    0x69, // TETRIS FLASH
    0x19, // DONKEY KONG
    0x35, // MARIO'S PICROSS
    0xA8, //
    0x14, // POKEMON RED
    0xAA, // POKEMON GREEN
    0x75, // PICROSS 2
    0x95, // YOSSY NO PANEPON
    0x99, // KIRAKIRA KIDS
    0x34, // GAMEBOY GALLERY
    0x6F, // POCKETCAMERA
    0x15, //
    0xFF, // BALLOON KID
    0x97, // KINGOFTHEZOO
    0x4B, // DMG FOOTBALL
    0x90, // WORLD CUP
    0x17, // OTHELLO
    0x10, // SUPER RC PRO-AM
    0x39, // DYNABLASTER
    0xF7, // BOY AND BLOB GB2
    0xF6, // MEGAMAN
    0xA2, // STAR WARS-NOA
    0x49, //
    0x4E, // WAVERACE
    0x43, //
    0x68, // LOLO2
    0xE0, // YOSHI'S COOKIE
    0x8B, // MYSTIC QUEST
    0xF0, //
    0xCE, // TOPRANKINGTENNIS
    0x0C, // MANSELL
    0x29, // MEGAMAN3
    0xE8, // SPACE INVADERS
    0xB7, // GAME&WATCH
    0x86, // DONKEYKONGLAND95
    0x9A, // ASTEROIDS/MISCMD
    0x52, // STREET FIGHTER 2
    0x01, // DEFENDER/JOUST
    0x9D, // KILLERINSTINCT95
    0x71, // TETRIS BLAST
    0x9C, // PINOCCHIO
    0xBD, //
    0x5D, // BA.TOSHINDEN
    0x6D, // NETTOU KOF 95
    0x67, //
    0x3F, // TETRIS PLUS
    0x6B, // DONKEYKONGLAND 3
    // Duplicates requiring fourth-letter tie-breaker (indices 65-78)
    0xB3, // ???[B]???????? / MOGURANYA[U] / TETRIS ATTACK[R]
    0x46, // SUPER[E] MARIOLAND / ???[R]
    0x28, // GOLF[F] / GALAGA[A]&GALAXIAN
    0xA5, // SOLARSTRIKER[A] / BT2RAGNAROKWORLD[R]
    0xC6, // GBWARS[A] / KEN GRIFFEY[ ]JR
    0xD3, // KAERUNOTAMENI[R] / ???[I]
    0x27, // ???[B] / MAGNETIC SOCCER[N]
    0x61, // POKEMON[E] BLUE / VEGAS[A] STAKES
    0x18, // DONKEYKONGLAND[K] / ???[I]
    0x66, // GAMEBOY[E] GALLERY2 / MILLI/CENTI/PEDE[L]
    0x6A, // DONKEYKONGLAND[K] 2 / MARIO[I] & YOSHI
    0xBF, // KID ICARUS[ ] / SOCCER[C]
    0x0D, // TETRIS2[R] / POKEBOM[E]
    0xF4, // ???[-] / G&W GALLERY[ ]
];

/// Fourth letter tie-breaker for duplicate checksums.
/// For indices >= 65, the character at ROM[$0137] disambiguates between games.
#[rustfmt::skip]
const FOURTH_LETTER_TIEBREAKER: [u8; 29] = *b"BEFAARBEKEK R-URAR INAILICE R";

/// Palette ID for each checksum index (0-93).
/// Bit 7 ($80) indicates game requires DMG boot tilemap (logo) - we ignore this flag.
#[rustfmt::skip]
const PALETTE_PER_CHECKSUM: [u8; 94] = [
     0,  4,  5, 35, 34,  3, 31, 15, 10,  5, // 0-9
    19, 36,  7, 37, 30, 44, 21, 32, 31, 20, // 10-19 (12: X has $80 flag, masked)
     5, 33, 13, 14,  5, 29,  5, 18,  9,  3, // 20-29
     2, 26, 25, 25, 41, 42, 26, 45, 42, 45, // 30-39
    36, 38, 26, 42, 30, 41, 34, 34,  5, 42, // 40-49 (42: has $80 flag, masked)
     6,  5, 33, 25, 42, 42, 40,  2, 16, 25, // 50-59
    42, 42,  5,  0, 39, 36, 22, 25,  6, 32, // 60-69
    12, 36, 11, 39, 18, 39, 24, 31, 50, 17, // 70-79
    46,  6, 27,  0, 47, 41, 41,  0,  0, 19, // 80-89
    34, 23, 18, 29,                         // 90-93
];

/// Palette combinations: [OBJ0 index, OBJ1 index, BG index].
/// Each index is multiplied by 8 in the boot ROM to get byte offset; we store raw indices.
#[rustfmt::skip]
const PALETTE_COMBINATIONS: [[u8; 3]; 55] = [
    [ 4,  4, 29], //  0, Right + A
    [18, 18, 18], //  1, Right
    [20, 20, 20], //  2
    [24, 24, 24], //  3, Down + A
    [ 9,  9,  9], //  4
    [ 0,  0,  0], //  5, Up
    [27, 27, 27], //  6, Right + B
    [ 5,  5,  5], //  7, Left + B
    [12, 12, 12], //  8, Down
    [26, 26, 26], //  9
    [16,  8,  8], // 10
    [ 4, 28, 28], // 11
    [ 4,  2,  2], // 12
    [ 3,  4,  4], // 13
    [ 4, 29, 29], // 14
    [28,  4, 28], // 15
    [ 2, 17,  2], // 16
    [16, 16,  8], // 17
    [ 4,  4,  7], // 18
    [ 4,  4, 18], // 19
    [ 4,  4, 20], // 20
    [19, 19,  9], // 21
    [15, 15, 11], // 22 (raw_palette_comb 4*4-1, 4*4-1, 11*4 → indices 15, 15, 11)
    [17, 17,  2], // 23
    [ 4,  4,  2], // 24
    [ 4,  4,  3], // 25
    [28, 28,  0], // 26
    [ 3,  3,  0], // 27
    [ 0,  0,  1], // 28, Up + B
    [18, 22, 18], // 29
    [20, 22, 20], // 30
    [24, 22, 24], // 31
    [16, 22,  8], // 32
    [17,  4, 13], // 33
    [27,  0, 14], // 34 (raw_palette_comb 28*4-1, 0*4, 14*4 → indices 27, 0, 14)
    [27,  4, 15], // 35 (raw_palette_comb 28*4-1, 4*4, 15*4 → indices 27, 4, 15)
    [19, 23,  9], // 36 (raw_palette_comb 19*4, 23*4-1, 9*4 → indices 19, 23, 9 - but 23*4-1=91 so /4=22.75→22)
    [16, 28, 10], // 37
    [ 4, 23, 28], // 38
    [17, 22,  2], // 39
    [ 4,  0,  2], // 40, Left + A
    [ 4, 28,  3], // 41
    [28,  3,  0], // 42
    [ 3, 28,  4], // 43, Up + A
    [21, 28,  4], // 44
    [ 3, 28,  0], // 45
    [25,  3, 28], // 46
    [ 0, 28,  8], // 47
    [ 4,  3, 28], // 48, Left
    [28,  3,  6], // 49, Down + B
    [ 4, 28, 29], // 50
    // SameBoy "Exclusives" (not in original CGB boot ROM)
    [30, 30, 30], // 51, Right + A + B, CGA
    [31, 31, 31], // 52, Left + A + B, DMG LCD
    [28,  4,  1], // 53, Up + A + B
    [ 0,  0,  2], // 54, Down + A + B
];

/// RGB555 palettes (4 colors each).
/// Each color is a little-endian `u16` in RGB555 format: 0b0BBBBBGGGGGRRRRR
/// (bits 0-4 = R, bits 5-9 = G, bits 10-14 = B; bit 15 unused).
#[rustfmt::skip]
const PALETTES: [[u16; 4]; 32] = [
    [0x7FFF, 0x32BF, 0x00D0, 0x0000], //  0
    [0x639F, 0x4279, 0x15B0, 0x04CB], //  1
    [0x7FFF, 0x6E31, 0x454A, 0x0000], //  2
    [0x7FFF, 0x1BEF, 0x0200, 0x0000], //  3
    [0x7FFF, 0x421F, 0x1CF2, 0x0000], //  4
    [0x7FFF, 0x5294, 0x294A, 0x0000], //  5
    [0x7FFF, 0x03FF, 0x012F, 0x0000], //  6
    [0x7FFF, 0x03EF, 0x01D6, 0x0000], //  7
    [0x7FFF, 0x42B5, 0x3DC8, 0x0000], //  8
    [0x7E74, 0x03FF, 0x0180, 0x0000], //  9
    [0x67FF, 0x77AC, 0x1A13, 0x2D6B], // 10
    [0x7ED6, 0x4BFF, 0x2175, 0x0000], // 11
    [0x53FF, 0x4A5F, 0x7E52, 0x0000], // 12
    [0x4FFF, 0x7ED2, 0x3A4C, 0x1CE0], // 13
    [0x03ED, 0x7FFF, 0x255F, 0x0000], // 14
    [0x036A, 0x021F, 0x03FF, 0x7FFF], // 15
    [0x7FFF, 0x01DF, 0x0112, 0x0000], // 16
    [0x231F, 0x035F, 0x00F2, 0x0009], // 17
    [0x7FFF, 0x03EA, 0x011F, 0x0000], // 18
    [0x299F, 0x001A, 0x000C, 0x0000], // 19
    [0x7FFF, 0x027F, 0x001F, 0x0000], // 20
    [0x7FFF, 0x03E0, 0x0206, 0x0120], // 21
    [0x7FFF, 0x7EEB, 0x001F, 0x7C00], // 22
    [0x7FFF, 0x3FFF, 0x7E00, 0x001F], // 23
    [0x7FFF, 0x03FF, 0x001F, 0x0000], // 24
    [0x03FF, 0x001F, 0x000C, 0x0000], // 25
    [0x7FFF, 0x033F, 0x0193, 0x0000], // 26
    [0x0000, 0x4200, 0x037F, 0x7FFF], // 27
    [0x7FFF, 0x7E8C, 0x7C00, 0x0000], // 28
    [0x7FFF, 0x1BEF, 0x6180, 0x0000], // 29
    // SameBoy "Exclusives"
    [0x7FFF, 0x7FEA, 0x7D5F, 0x0000], // 30, CGA
    [0x4778, 0x3290, 0x1D87, 0x0861], // 31, DMG LCD
];

/// DMG compatibility palette data for CGB mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmgCompatPalette {
    /// Background palette 0 (4 RGB555 colors)
    pub bg0: [u16; 4],
    /// Object palette 0 (4 RGB555 colors)
    pub obj0: [u16; 4],
    /// Object palette 1 (4 RGB555 colors)
    pub obj1: [u16; 4],
}

impl Default for DmgCompatPalette {
    fn default() -> Self {
        // Default grayscale palette (palette combination 5 = palette 0 for all)
        Self {
            bg0: PALETTES[0],
            obj0: PALETTES[0],
            obj1: PALETTES[0],
        }
    }
}

/// Computes the title checksum from ROM header bytes.
///
/// The checksum is the sum of bytes at $0134-$0143 (title area), masked to 8 bits.
/// `header` should start at ROM address $0100 (the entry point).
pub fn compute_title_checksum(header: &[u8]) -> u8 {
    if header.len() <= TITLE_END {
        return 0;
    }
    header[TITLE_START..=TITLE_END]
        .iter()
        .fold(0u8, |acc, &b| acc.wrapping_add(b))
}

/// Checks if the cartridge is from Nintendo (old licensee $01 or new licensee "01").
///
/// `header` should start at ROM address $0100.
pub fn is_nintendo_licensee(header: &[u8]) -> bool {
    if header.len() <= OLD_LICENSEE {
        return false;
    }

    // Old licensee code $01 = Nintendo
    if header[OLD_LICENSEE] == 0x01 {
        return true;
    }

    // New licensee code "01" = Nintendo (when old licensee is $33)
    if header[OLD_LICENSEE] == 0x33 && header.len() > NEW_LICENSEE_LOW {
        return header[NEW_LICENSEE_HIGH] == b'0' && header[NEW_LICENSEE_LOW] == b'1';
    }

    false
}

/// Gets the palette combination index for a cartridge header.
///
/// Returns the index into `PALETTE_COMBINATIONS` (0-54).
/// Non-Nintendo games return 5 (the "Up button" default palette).
pub fn get_palette_id(header: &[u8]) -> u8 {
    // Non-Nintendo games get the default palette (combination 5)
    if !is_nintendo_licensee(header) {
        return 5; // Palette combination 5 = Up button
    }

    let checksum = compute_title_checksum(header);

    // Find the checksum in the table
    let index = TITLE_CHECKSUMS
        .iter()
        .position(|&c| c == checksum)
        .unwrap_or(0);

    // Handle duplicates requiring fourth-letter tie-breaker
    let final_index = if index >= FIRST_DUPLICATE_INDEX {
        resolve_duplicate(header, index)
    } else {
        index
    };

    // Get palette ID from the mapping table
    if final_index < PALETTE_PER_CHECKSUM.len() {
        // Mask off the $80 flag (logo tilemap indicator)
        PALETTE_PER_CHECKSUM[final_index] & 0x7F
    } else {
        5 // Default fallback
    }
}

/// Resolves duplicate checksums using the fourth letter of the title.
fn resolve_duplicate(header: &[u8], initial_index: usize) -> usize {
    if header.len() <= FOURTH_LETTER_OFFSET {
        return initial_index;
    }

    let fourth_letter = header[FOURTH_LETTER_OFFSET];
    let tiebreaker_offset = initial_index - FIRST_DUPLICATE_INDEX;

    // Check if the fourth letter matches the expected tie-breaker character
    if tiebreaker_offset < FOURTH_LETTER_TIEBREAKER.len()
        && fourth_letter == FOURTH_LETTER_TIEBREAKER[tiebreaker_offset]
    {
        // First variant (use the initial index as-is for palette lookup)
        initial_index
    } else {
        // Second variant (add 14 to get the alternate palette)
        // The duplicate entries are 14 apart in PALETTE_PER_CHECKSUM
        initial_index + 14
    }
}

/// Gets the DMG compatibility palette colors for a cartridge.
///
/// `header` should start at ROM address $0100 (at least 76 bytes, up to $014B).
pub fn get_palette_colors(header: &[u8]) -> DmgCompatPalette {
    let palette_id = get_palette_id(header) as usize;
    palette_colors_from_combination_id(palette_id)
}

/// Shared implementation for palette lookup by combination ID.
fn palette_colors_from_combination_id(palette_id: usize) -> DmgCompatPalette {
    if palette_id >= PALETTE_COMBINATIONS.len() {
        return DmgCompatPalette::default();
    }

    let combination = PALETTE_COMBINATIONS[palette_id];
    let palette_at = |idx: u8| PALETTES.get(idx as usize).copied().unwrap_or(PALETTES[0]);

    DmgCompatPalette {
        obj0: palette_at(combination[0]),
        obj1: palette_at(combination[1]),
        bg0: palette_at(combination[2]),
    }
}

/// Gets the DMG compatibility palette colors by palette combination ID.
///
/// This is used for manual palette selection during the CGB boot animation.
/// Returns the default palette if `palette_id` is out of bounds.
pub fn get_palette_colors_by_id(palette_id: u8) -> DmgCompatPalette {
    palette_colors_from_combination_id(palette_id as usize)
}

/// Maps a button combination to a manual palette selection ID.
///
/// Button bits follow NES platform convention:
/// - Bit 0: A
/// - Bit 1: B
/// - Bit 4: Up
/// - Bit 5: Down
/// - Bit 6: Left
/// - Bit 7: Right
///
/// Returns `Some(palette_id)` for valid manual selection combos, `None` otherwise.
/// The 12 valid combinations are documented in the CGB boot ROM behavior.
///
/// # Button Combinations
///
/// | Buttons   | Palette ID | Description       |
/// |-----------|------------|-------------------|
/// | Up        | 5          | Light green/brown |
/// | Up+A      | 43         | Red/blue          |
/// | Up+B      | 28         | Dark brown        |
/// | Down      | 8          | Pastel            |
/// | Down+A    | 3          | Orange            |
/// | Down+B    | 49         | Yellow            |
/// | Left      | 48         | Dark green        |
/// | Left+A    | 40         | Gray              |
/// | Left+B    | 7          | Dark gray         |
/// | Right     | 1          | Sepia             |
/// | Right+A   | 0          | Green             |
/// | Right+B   | 6          | Reverse           |
pub fn button_combo_to_palette_id(buttons: u8) -> Option<u8> {
    // Extract relevant bits
    let a = buttons & 0x01 != 0;
    let b = buttons & 0x02 != 0;
    let up = buttons & 0x10 != 0;
    let down = buttons & 0x20 != 0;
    let left = buttons & 0x40 != 0;
    let right = buttons & 0x80 != 0;

    // Must have exactly one D-pad direction (no diagonals, no multiple directions)
    let dpad_count = up as u8 + down as u8 + left as u8 + right as u8;
    if dpad_count != 1 {
        return None;
    }

    // Cannot have both A and B pressed
    if a && b {
        return None;
    }

    // Map button combinations to palette IDs per CGB boot ROM behavior
    match (up, down, left, right, a, b) {
        // Up combinations
        (true, false, false, false, false, false) => Some(5), // Up only
        (true, false, false, false, true, false) => Some(43), // Up + A
        (true, false, false, false, false, true) => Some(28), // Up + B
        // Down combinations
        (false, true, false, false, false, false) => Some(8), // Down only
        (false, true, false, false, true, false) => Some(3),  // Down + A
        (false, true, false, false, false, true) => Some(49), // Down + B
        // Left combinations
        (false, false, true, false, false, false) => Some(48), // Left only
        (false, false, true, false, true, false) => Some(40),  // Left + A
        (false, false, true, false, false, true) => Some(7),   // Left + B
        // Right combinations
        (false, false, false, true, false, false) => Some(1), // Right only
        (false, false, false, true, true, false) => Some(0),  // Right + A
        (false, false, false, true, false, true) => Some(6),  // Right + B
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates a minimal header with title and licensee bytes.
    fn make_header(title: &[u8; 16], old_licensee: u8) -> [u8; 0x4C] {
        let mut header = [0u8; 0x4C];
        header[TITLE_START..TITLE_START + 16].copy_from_slice(title);
        header[OLD_LICENSEE] = old_licensee;
        header
    }

    /// Creates a header with new licensee code.
    fn make_header_new_licensee(title: &[u8; 16], new_licensee: &[u8; 2]) -> [u8; 0x4C] {
        let mut header = [0u8; 0x4C];
        header[TITLE_START..TITLE_START + 16].copy_from_slice(title);
        header[NEW_LICENSEE_HIGH] = new_licensee[0];
        header[NEW_LICENSEE_LOW] = new_licensee[1];
        header[OLD_LICENSEE] = 0x33; // Indicates new licensee code
        header
    }

    #[test]
    fn test_compute_title_checksum_tetris() {
        // "TETRIS" padded with zeros
        let title: [u8; 16] = *b"TETRIS\0\0\0\0\0\0\0\0\0\0";
        let header = make_header(&title, 0x01);
        let checksum = compute_title_checksum(&header);
        // T=0x54, E=0x45, T=0x54, R=0x52, I=0x49, S=0x53 = 0xDB + zeros = 0xDB
        assert_eq!(checksum, 0xDB, "TETRIS checksum should be 0xDB");
    }

    #[test]
    fn test_compute_title_checksum_empty() {
        let title: [u8; 16] = [0u8; 16];
        let header = make_header(&title, 0x01);
        assert_eq!(compute_title_checksum(&header), 0x00);
    }

    #[test]
    fn test_is_nintendo_licensee_old() {
        let title: [u8; 16] = [0u8; 16];
        let header = make_header(&title, 0x01);
        assert!(is_nintendo_licensee(&header));
    }

    #[test]
    fn test_is_nintendo_licensee_new() {
        let title: [u8; 16] = [0u8; 16];
        let header = make_header_new_licensee(&title, b"01");
        assert!(is_nintendo_licensee(&header));
    }

    #[test]
    fn test_is_nintendo_licensee_non_nintendo() {
        let title: [u8; 16] = [0u8; 16];
        let header = make_header(&title, 0x00);
        assert!(!is_nintendo_licensee(&header));
    }

    #[test]
    fn test_get_palette_id_non_nintendo() {
        let title: [u8; 16] = [0u8; 16];
        let header = make_header(&title, 0x00);
        // Non-Nintendo games get palette combination 5 (grayscale)
        assert_eq!(get_palette_id(&header), 5);
    }

    #[test]
    fn test_get_palette_id_tetris() {
        // TETRIS has checksum 0xDB which is at index 5 in TITLE_CHECKSUMS
        let title: [u8; 16] = *b"TETRIS\0\0\0\0\0\0\0\0\0\0";
        let header = make_header(&title, 0x01);
        // PALETTE_PER_CHECKSUM[5] = 3
        assert_eq!(get_palette_id(&header), 3);
    }

    #[test]
    fn test_get_palette_colors_default() {
        let title: [u8; 16] = [0u8; 16];
        let header = make_header(&title, 0x00);
        let palette = get_palette_colors(&header);
        // Non-Nintendo → palette 5 → combination [0, 0, 0] → all palette 0
        assert_eq!(palette.bg0, PALETTES[0]);
        assert_eq!(palette.obj0, PALETTES[0]);
        assert_eq!(palette.obj1, PALETTES[0]);
    }

    #[test]
    fn test_get_palette_colors_tetris() {
        let title: [u8; 16] = *b"TETRIS\0\0\0\0\0\0\0\0\0\0";
        let header = make_header(&title, 0x01);
        let palette = get_palette_colors(&header);
        // TETRIS → palette 3 → combination [24, 24, 24] → all palette 24
        assert_eq!(palette.bg0, PALETTES[24]);
        assert_eq!(palette.obj0, PALETTES[24]);
        assert_eq!(palette.obj1, PALETTES[24]);
    }

    #[test]
    fn test_palette_combination_indices_in_bounds() {
        // Verify all palette combination indices are valid
        for (i, combination) in PALETTE_COMBINATIONS.iter().enumerate() {
            assert!(
                (combination[0] as usize) < PALETTES.len(),
                "OBJ0 index {} out of bounds in combination {}",
                combination[0],
                i
            );
            assert!(
                (combination[1] as usize) < PALETTES.len(),
                "OBJ1 index {} out of bounds in combination {}",
                combination[1],
                i
            );
            assert!(
                (combination[2] as usize) < PALETTES.len(),
                "BG index {} out of bounds in combination {}",
                combination[2],
                i
            );
        }
    }

    #[test]
    fn test_palette_per_checksum_indices_in_bounds() {
        // Verify all palette IDs map to valid combinations
        for (i, &id) in PALETTE_PER_CHECKSUM.iter().enumerate() {
            let masked = (id & 0x7F) as usize;
            assert!(
                masked < PALETTE_COMBINATIONS.len(),
                "Palette ID {} (masked {}) out of bounds at index {}",
                id,
                masked,
                i
            );
        }
    }

    #[test]
    fn test_duplicate_checksum_first_variant() {
        // POKEMON BLUE has checksum 0x61 (index 72) with fourth letter 'E'
        // Fourth letter 'E' at position 72-65=7 in tiebreaker matches
        let title: [u8; 16] = *b"POKEMON BLUE\0\0\0\0";
        let header = make_header(&title, 0x01);
        let checksum = compute_title_checksum(&header);
        assert_eq!(checksum, 0x61);
        // Should get first variant palette
        let palette_id = get_palette_id(&header);
        // PALETTE_PER_CHECKSUM[72] = 11
        assert_eq!(palette_id, 11);
    }

    #[test]
    fn test_duplicate_checksum_second_variant() {
        // VEGAS STAKES has checksum 0x61 but fourth letter 'A' (not 'E')
        let title: [u8; 16] = *b"VEGAS STAKES\0\0\0\0";
        let header = make_header(&title, 0x01);
        let checksum = compute_title_checksum(&header);
        assert_eq!(checksum, 0x61);
        // Fourth letter 'A' doesn't match tiebreaker 'E', so second variant
        let palette_id = get_palette_id(&header);
        // PALETTE_PER_CHECKSUM[72+14] = PALETTE_PER_CHECKSUM[86] = 41
        assert_eq!(palette_id, 41);
    }

    // ── get_palette_colors_by_id tests ─────────────────────────────────────

    #[test]
    fn test_get_palette_colors_by_id_valid() {
        // Palette ID 5 (Up button) = combination [0, 0, 0] = all palette 0
        let palette = get_palette_colors_by_id(5);
        assert_eq!(palette.bg0, PALETTES[0]);
        assert_eq!(palette.obj0, PALETTES[0]);
        assert_eq!(palette.obj1, PALETTES[0]);
    }

    #[test]
    fn test_get_palette_colors_by_id_out_of_bounds() {
        // Out of bounds should return default palette
        let palette = get_palette_colors_by_id(255);
        assert_eq!(palette, DmgCompatPalette::default());
    }

    #[test]
    fn test_get_palette_colors_by_id_all_manual_palettes() {
        // Verify all 12 manual palette IDs are valid
        let manual_ids = [5, 43, 28, 8, 3, 49, 48, 40, 7, 1, 0, 6];
        for &id in &manual_ids {
            let palette = get_palette_colors_by_id(id);
            // Just verify it doesn't panic and returns something
            assert!(
                palette.bg0[0] != 0
                    || palette.bg0[1] != 0
                    || palette.bg0[2] != 0
                    || palette.bg0[3] != 0
            );
        }
    }

    // ── button_combo_to_palette_id tests ───────────────────────────────────

    #[test]
    fn test_button_combo_up_only() {
        // Up = bit 4 = 0x10
        assert_eq!(button_combo_to_palette_id(0x10), Some(5));
    }

    #[test]
    fn test_button_combo_up_a() {
        // Up (0x10) + A (0x01) = 0x11
        assert_eq!(button_combo_to_palette_id(0x11), Some(43));
    }

    #[test]
    fn test_button_combo_up_b() {
        // Up (0x10) + B (0x02) = 0x12
        assert_eq!(button_combo_to_palette_id(0x12), Some(28));
    }

    #[test]
    fn test_button_combo_down_only() {
        // Down = bit 5 = 0x20
        assert_eq!(button_combo_to_palette_id(0x20), Some(8));
    }

    #[test]
    fn test_button_combo_down_a() {
        // Down (0x20) + A (0x01) = 0x21
        assert_eq!(button_combo_to_palette_id(0x21), Some(3));
    }

    #[test]
    fn test_button_combo_down_b() {
        // Down (0x20) + B (0x02) = 0x22
        assert_eq!(button_combo_to_palette_id(0x22), Some(49));
    }

    #[test]
    fn test_button_combo_left_only() {
        // Left = bit 6 = 0x40
        assert_eq!(button_combo_to_palette_id(0x40), Some(48));
    }

    #[test]
    fn test_button_combo_left_a() {
        // Left (0x40) + A (0x01) = 0x41
        assert_eq!(button_combo_to_palette_id(0x41), Some(40));
    }

    #[test]
    fn test_button_combo_left_b() {
        // Left (0x40) + B (0x02) = 0x42
        assert_eq!(button_combo_to_palette_id(0x42), Some(7));
    }

    #[test]
    fn test_button_combo_right_only() {
        // Right = bit 7 = 0x80
        assert_eq!(button_combo_to_palette_id(0x80), Some(1));
    }

    #[test]
    fn test_button_combo_right_a() {
        // Right (0x80) + A (0x01) = 0x81
        assert_eq!(button_combo_to_palette_id(0x81), Some(0));
    }

    #[test]
    fn test_button_combo_right_b() {
        // Right (0x80) + B (0x02) = 0x82
        assert_eq!(button_combo_to_palette_id(0x82), Some(6));
    }

    #[test]
    fn test_button_combo_no_dpad() {
        // No D-pad direction pressed
        assert_eq!(button_combo_to_palette_id(0x00), None);
        assert_eq!(button_combo_to_palette_id(0x01), None); // A only
        assert_eq!(button_combo_to_palette_id(0x02), None); // B only
        assert_eq!(button_combo_to_palette_id(0x03), None); // A+B only
    }

    #[test]
    fn test_button_combo_multiple_dpad() {
        // Multiple D-pad directions (diagonal) = invalid
        assert_eq!(button_combo_to_palette_id(0x30), None); // Up + Down
        assert_eq!(button_combo_to_palette_id(0xC0), None); // Left + Right
        assert_eq!(button_combo_to_palette_id(0x50), None); // Up + Left
        assert_eq!(button_combo_to_palette_id(0xF0), None); // All directions
    }

    #[test]
    fn test_button_combo_a_and_b() {
        // A+B together with D-pad = invalid
        assert_eq!(button_combo_to_palette_id(0x13), None); // Up + A + B
        assert_eq!(button_combo_to_palette_id(0x23), None); // Down + A + B
        assert_eq!(button_combo_to_palette_id(0x43), None); // Left + A + B
        assert_eq!(button_combo_to_palette_id(0x83), None); // Right + A + B
    }

    #[test]
    fn test_button_combo_select_start_ignored() {
        // Select (0x04) and Start (0x08) should be ignored
        // Up + Select = still valid Up
        assert_eq!(button_combo_to_palette_id(0x14), Some(5));
        // Up + Start = still valid Up
        assert_eq!(button_combo_to_palette_id(0x18), Some(5));
        // Up + A + Select + Start = still valid Up + A
        assert_eq!(button_combo_to_palette_id(0x1D), Some(43));
    }
}
