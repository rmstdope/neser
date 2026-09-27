//! The games NESER recognises as SNES Mouse games, so the mouse can be plugged into port 1
//! for them without the player configuring anything.
//!
//! Matched on the cartridge header title, as for the Super Scope. Only the games that cannot
//! be played without the mouse are listed (fullsnes "SNES Controllers Mouse Games"; Mario &
//! Wario is the only other mouse-only game): most mouse games also take a pad and switch the
//! pad off when they sense a mouse, so plugging it in for them would take the pad away.
//! Titles come from superfamicom.org's ROM information pages.

/// Header titles with their trailing padding removed.
const SNES_MOUSE_TITLES: &[&[u8]] = &[
    // Mario Paint: the same title in the Japan/USA and Europe cartridges.
    b"MARIOPAINT",
    // Mario & Wario (Japan).
    b"Mario&Wario",
];

/// Whether `title_bytes`, a cartridge's raw header title, names a game that needs the
/// SNES Mouse.
pub fn is_snes_mouse_game(title_bytes: &[u8]) -> bool {
    SNES_MOUSE_TITLES.contains(&super::super_scope_games::trim_header_title(title_bytes))
}

#[cfg(test)]
mod tests {
    use super::is_snes_mouse_game;

    fn padded(title: &[u8]) -> Vec<u8> {
        let mut bytes = title.to_vec();
        bytes.resize(21, b' ');
        bytes
    }

    #[test]
    fn recognises_mario_paint_and_mario_and_wario() {
        assert!(is_snes_mouse_game(&padded(b"MARIOPAINT")));
        assert!(is_snes_mouse_game(&padded(b"Mario&Wario")));
    }

    #[test]
    fn ignores_trailing_nul_padding() {
        let mut title = b"MARIOPAINT".to_vec();
        title.resize(21, 0);
        assert!(is_snes_mouse_game(&title));
    }

    #[test]
    fn does_not_recognise_other_games() {
        for title in [
            &b"SUPER MARIOWORLD"[..],
            // SD Gundam GX moves its menu arrow with the d-pad and never reads a mouse.
            b"SD\xB6\xDE\xDD\xC0\xDE\xD1GX",
            // Games where the mouse is optional keep their pad.
            b"JURASSIC PARK",
            b"Lemmings 2,The Tribes",
            // Close, but not the cartridge's title.
            b"MARIO PAINT",
            b"MARIOPAINT 2",
        ] {
            assert!(
                !is_snes_mouse_game(&padded(title)),
                "{} should not be recognised",
                String::from_utf8_lossy(title)
            );
        }
    }
}
