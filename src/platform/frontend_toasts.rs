//! System-agnostic toast message formatters for frontend events.
//!
//! These generate human-readable strings for transient on-screen messages
//! (gamepad connection, ROM loading, etc.) that are the same regardless of
//! which emulated system is running.

pub fn gamepad_connected_toast_message(player_num: u8) -> String {
    format!("Gamepad connected: Player {player_num}")
}

pub fn gamepad_disconnected_toast_message(player_num: u8) -> String {
    format!("Gamepad disconnected: was Player {player_num}")
}

pub fn gamepad_init_toast_message(gamepads_enabled: bool, detected_controllers: usize) -> String {
    if !gamepads_enabled {
        return "Gamepads disabled: using keyboard controls".to_string();
    }

    match detected_controllers {
        0 => "No gamepads found: using keyboard controls".to_string(),
        1 => "Gamepad found: using 1 gamepad".to_string(),
        count => format!("Gamepads found: using {} gamepads", count),
    }
}

pub fn cartridge_load_toast_message(rom_path: &str, success: bool) -> String {
    if success {
        return format!("Cartridge loaded: {}", rom_path);
    }
    format!("Cartridge load failed: {}", rom_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gamepad_init_toast_when_gamepads_disabled_uses_keyboard_message() {
        let message = gamepad_init_toast_message(false, 0);
        assert_eq!(message, "Gamepads disabled: using keyboard controls");
    }

    #[test]
    fn gamepad_init_toast_when_enabled_and_none_found_uses_fallback_message() {
        let message = gamepad_init_toast_message(true, 0);
        assert_eq!(message, "No gamepads found: using keyboard controls");
    }

    #[test]
    fn gamepad_init_toast_when_one_found_reports_single_gamepad() {
        let message = gamepad_init_toast_message(true, 1);
        assert_eq!(message, "Gamepad found: using 1 gamepad");
    }

    #[test]
    fn gamepad_init_toast_reports_three_gamepads_in_four_score() {
        let message = gamepad_init_toast_message(true, 3);
        assert_eq!(message, "Gamepads found: using 3 gamepads");
    }

    #[test]
    fn cartridge_load_success_toast_includes_rom_path() {
        let message = cartridge_load_toast_message("roms/games/mario.nes", true);
        assert_eq!(message, "Cartridge loaded: roms/games/mario.nes");
    }

    #[test]
    fn cartridge_load_failure_toast_includes_rom_path() {
        let message = cartridge_load_toast_message("roms/games/mario.nes", false);
        assert_eq!(message, "Cartridge load failed: roms/games/mario.nes");
    }

    #[test]
    fn gamepad_connected_toast_player_1() {
        let message = gamepad_connected_toast_message(1);
        assert_eq!(message, "Gamepad connected: Player 1");
    }

    #[test]
    fn gamepad_connected_toast_player_2() {
        let message = gamepad_connected_toast_message(2);
        assert_eq!(message, "Gamepad connected: Player 2");
    }

    #[test]
    fn gamepad_disconnected_toast_player_1() {
        let message = gamepad_disconnected_toast_message(1);
        assert_eq!(message, "Gamepad disconnected: was Player 1");
    }

    #[test]
    fn gamepad_disconnected_toast_player_2() {
        let message = gamepad_disconnected_toast_message(2);
        assert_eq!(message, "Gamepad disconnected: was Player 2");
    }
}
