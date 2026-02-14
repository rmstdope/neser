use crate::console::TvSystem;

pub fn gamepad_init_toast_message(gamepads_enabled: bool, detected_controllers: usize) -> String {
    if !gamepads_enabled {
        return "Gamepads disabled: using keyboard controls".to_string();
    }

    let used_controllers = detected_controllers.min(2);

    match used_controllers {
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

pub fn emulator_timing_toast_message(tv_system: TvSystem) -> String {
    format!("Emulator timing: {}", tv_system_toast_label(tv_system))
}

fn tv_system_toast_label(tv_system: TvSystem) -> &'static str {
    match tv_system {
        TvSystem::Ntsc => "NTSC",
        TvSystem::Pal => "PAL",
    }
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
    fn gamepad_init_toast_caps_reported_gamepads_to_two() {
        let message = gamepad_init_toast_message(true, 3);
        assert_eq!(message, "Gamepads found: using 2 gamepads");
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
    fn emulator_timing_toast_uses_ntsc_label() {
        let message = emulator_timing_toast_message(TvSystem::Ntsc);
        assert_eq!(message, "Emulator timing: NTSC");
    }

    #[test]
    fn emulator_timing_toast_uses_pal_label() {
        let message = emulator_timing_toast_message(TvSystem::Pal);
        assert_eq!(message, "Emulator timing: PAL");
    }
}
