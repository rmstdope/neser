//! The words a player reads about SNES peripherals. `Snes::load_rom` raises the connected pair;
//! the desktop and web frontends raise the released pair and the Turbo words.

/// Shown each time a game loads with a Super Scope connected.
pub const SUPER_SCOPE_CONNECTED: &str = "Super Scope connected — click to aim with the mouse";

/// Shown when the mouse stops aiming the Super Scope (Escape, or the capture was lost).
pub const SUPER_SCOPE_MOUSE_RELEASED: &str = "Mouse released — click the game to aim again";

/// Shown each time a game loads with an SNES Mouse connected.
pub const SNES_MOUSE_CONNECTED: &str = "SNES Mouse connected — click the game to use the mouse";

/// Shown when the mouse stops driving the SNES Mouse (Escape, or the capture was lost).
pub const SNES_MOUSE_RELEASED: &str = "Mouse released — click the game to use it again";

/// Shown when the player flips the Super Scope's Turbo switch.
pub fn super_scope_turbo_toast(on: bool) -> &'static str {
    if on { "Turbo on" } else { "Turbo off" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snes_mouse_words_are_the_agreed_ones() {
        assert_eq!(
            SNES_MOUSE_CONNECTED,
            "SNES Mouse connected — click the game to use the mouse"
        );
        assert_eq!(
            SNES_MOUSE_RELEASED,
            "Mouse released — click the game to use it again"
        );
    }

    #[test]
    fn super_scope_words_are_the_agreed_ones() {
        assert_eq!(
            SUPER_SCOPE_CONNECTED,
            "Super Scope connected — click to aim with the mouse"
        );
        assert_eq!(
            SUPER_SCOPE_MOUSE_RELEASED,
            "Mouse released — click the game to aim again"
        );
        assert_eq!(super_scope_turbo_toast(true), "Turbo on");
        assert_eq!(super_scope_turbo_toast(false), "Turbo off");
    }
}
