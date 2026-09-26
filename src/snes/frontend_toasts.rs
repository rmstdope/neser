//! The words a player reads about SNES peripherals, shared by the desktop and web frontends.

/// Shown each time a game loads with a Super Scope connected.
pub const SUPER_SCOPE_CONNECTED: &str = "Super Scope connected — click to aim with the mouse";

/// Shown when the mouse stops aiming the Super Scope (Escape, or the capture was lost).
pub const SUPER_SCOPE_MOUSE_RELEASED: &str = "Mouse released — click the game to aim again";

/// Shown when the player flips the Super Scope's Turbo switch.
pub fn super_scope_turbo_toast(on: bool) -> &'static str {
    if on { "Turbo on" } else { "Turbo off" }
}

#[cfg(test)]
mod tests {
    use super::*;

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
