//! The games NESER recognises as Super Scope games, so the scope can be plugged into
//! port 2 for them without the player configuring anything.
//!
//! Matched on the cartridge header title (the 21 bytes at `$FFC0`/`$7FC0`), which survives
//! re-dumps and copier headers. Titles come from fullsnes ("SNES Controllers Detecting
//! Controller Support of ROM-Images") and superfamicom.org's ROM information pages. Only
//! games whose main input is the Super Scope are listed: games where it is an optional
//! extra (Lemmings 2, The Hunt for Red October, Lamborghini American Challenge) would lose
//! the pad on port 2 if the scope were plugged in for them.

/// Header titles with their trailing padding removed.
const SUPER_SCOPE_TITLES: &[&[u8]] = &[
    b"SUPER SCOPE 6",
    b"NINTENDO SCOPE 6",
    b"YOSHI'S SAFARI",
    // Yoshi no Road Hunting (Japan), in half-width katakana.
    &[
        0xD6, 0xAF, 0xBC, 0xB0, 0xC9, 0xDB, 0xB0, 0xC4, 0xDE, 0xCA, 0xDD, 0xC3, 0xA8, 0xDD, 0xB8,
        0xDE,
    ],
    b"BATTLE CLASH",
    b"SPACE BAZOOKA",
    b"METAL COMBAT",
    b"T2 ARCADE",
    b"BAZOOKA BLITZKRIEG",
    b"SFC DESTRUCTIVE",
    b"OPERATION THUNDERBOLT",
    b"TINSTAR",
    b"X ZONE",
];

/// Whether `title_bytes`, a cartridge's raw header title, names a Super Scope game.
pub fn is_super_scope_game(title_bytes: &[u8]) -> bool {
    SUPER_SCOPE_TITLES.contains(&trim_header_title(title_bytes))
}

/// A raw header title without its trailing space or NUL padding.
pub(super) fn trim_header_title(title_bytes: &[u8]) -> &[u8] {
    let end = title_bytes
        .iter()
        .rposition(|&byte| byte != b' ' && byte != 0)
        .map_or(0, |last| last + 1);
    &title_bytes[..end]
}

#[cfg(test)]
mod tests {
    use super::is_super_scope_game;

    fn padded(title: &[u8]) -> Vec<u8> {
        let mut bytes = title.to_vec();
        bytes.resize(21, b' ');
        bytes
    }

    #[test]
    fn recognises_metal_combat_as_read_from_the_cartridge() {
        assert!(is_super_scope_game(b"METAL COMBAT         "));
    }

    #[test]
    fn recognises_every_listed_super_scope_title() {
        for title in [
            &b"SUPER SCOPE 6"[..],
            b"NINTENDO SCOPE 6",
            b"YOSHI'S SAFARI",
            b"BATTLE CLASH",
            b"SPACE BAZOOKA",
            b"METAL COMBAT",
            b"T2 ARCADE",
            b"BAZOOKA BLITZKRIEG",
            b"SFC DESTRUCTIVE",
            b"OPERATION THUNDERBOLT",
            b"TINSTAR",
            b"X ZONE",
        ] {
            assert!(
                is_super_scope_game(&padded(title)),
                "{} should be recognised",
                String::from_utf8_lossy(title)
            );
        }
    }

    #[test]
    fn recognises_yoshi_no_road_hunting_by_its_katakana_title() {
        let title = [
            0xD6, 0xAF, 0xBC, 0xB0, 0xC9, 0xDB, 0xB0, 0xC4, 0xDE, 0xCA, 0xDD, 0xC3, 0xA8, 0xDD,
            0xB8, 0xDE,
        ];
        assert!(is_super_scope_game(&padded(&title)));
    }

    #[test]
    fn ignores_trailing_nul_padding() {
        let mut title = b"BATTLE CLASH".to_vec();
        title.resize(21, 0);
        assert!(is_super_scope_game(&title));
    }

    #[test]
    fn does_not_recognise_other_games() {
        for title in [
            &b"SUPER MARIOWORLD"[..],
            // Terminator 2: Judgment Day is a platformer; the scope game is T2 ARCADE.
            b"TERMINATOR2 THE MOVIE",
            // The scope is only an optional extra in these.
            b"Lemmings 2,The Tribes",
            b"Hunt for Red October",
            b"LAMBORGHINI AMERICAN",
            // A title that merely starts like a listed one.
            b"METAL COMBAT II",
        ] {
            assert!(
                !is_super_scope_game(&padded(title)),
                "{} should not be recognised",
                String::from_utf8_lossy(title)
            );
        }
    }
}
