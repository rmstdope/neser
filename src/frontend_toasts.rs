use crate::nes::console::{ExpansionPort, HardwareMode, TimingMode};

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

pub fn emulator_timing_toast_message(tv_system: TimingMode) -> String {
    format!("Emulator timing: {}", tv_system_toast_label(tv_system))
}

pub fn hardware_mode_toast_message(
    mode: HardwareMode,
    model: crate::nes::console::HardwareModel,
    expansion: ExpansionPort,
    four_score_enabled: bool,
) -> String {
    match mode {
        HardwareMode::Nes => {
            let timing = match model {
                crate::nes::console::HardwareModel::NesNtsc => "NTSC",
                crate::nes::console::HardwareModel::NesPal => "PAL",
                crate::nes::console::HardwareModel::Dendy => "Dendy",
            };
            if four_score_enabled {
                format!("Hardware: NES {} (Four Score)", timing)
            } else {
                format!("Hardware: NES {}", timing)
            }
        }
        HardwareMode::Famicom => match expansion {
            ExpansionPort::FamicomFourPlayers => {
                "Hardware: Famicom (4-player expansion)".to_string()
            }
            ExpansionPort::ArkanoidFamicom => "Hardware: Famicom (Arkanoid expansion)".to_string(),
            ExpansionPort::ZapperFamicom => "Hardware: Famicom (Zapper expansion)".to_string(),
            ExpansionPort::PowerPadFamicom => "Hardware: Famicom (Power Pad expansion)".to_string(),
            ExpansionPort::VsSystem => "Hardware: VS System".to_string(),
            ExpansionPort::None => "Hardware: Famicom".to_string(),
        },
    }
}

fn tv_system_toast_label(tv_system: TimingMode) -> &'static str {
    match tv_system {
        TimingMode::Ntsc => "NTSC",
        TimingMode::Pal => "PAL",
        TimingMode::Dendy => "Dendy",
        TimingMode::MultiRegion | TimingMode::Unknown(_) => "NTSC",
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
    fn emulator_timing_toast_uses_ntsc_label() {
        let message = emulator_timing_toast_message(TimingMode::Ntsc);
        assert_eq!(message, "Emulator timing: NTSC");
    }

    #[test]
    fn emulator_timing_toast_uses_pal_label() {
        let message = emulator_timing_toast_message(TimingMode::Pal);
        assert_eq!(message, "Emulator timing: PAL");
    }

    #[test]
    fn emulator_timing_toast_uses_dendy_label() {
        let message = emulator_timing_toast_message(TimingMode::Dendy);
        assert_eq!(message, "Emulator timing: Dendy");
    }

    #[test]
    fn hardware_mode_toast_nes_dendy() {
        use crate::nes::console::HardwareModel;
        let message = hardware_mode_toast_message(
            HardwareMode::Nes,
            HardwareModel::Dendy,
            ExpansionPort::None,
            false,
        );
        assert_eq!(message, "Hardware: NES Dendy");
    }

    #[test]
    fn hardware_mode_toast_nes_dendy_with_four_score() {
        use crate::nes::console::HardwareModel;
        let message = hardware_mode_toast_message(
            HardwareMode::Nes,
            HardwareModel::Dendy,
            ExpansionPort::None,
            true,
        );
        assert_eq!(message, "Hardware: NES Dendy (Four Score)");
    }

    #[test]
    fn hardware_mode_toast_nes_ntsc_default() {
        use crate::nes::console::HardwareModel;
        let message = hardware_mode_toast_message(
            HardwareMode::Nes,
            HardwareModel::NesNtsc,
            ExpansionPort::None,
            false,
        );
        assert_eq!(message, "Hardware: NES NTSC");
    }

    #[test]
    fn hardware_mode_toast_nes_pal() {
        use crate::nes::console::HardwareModel;
        let message = hardware_mode_toast_message(
            HardwareMode::Nes,
            HardwareModel::NesPal,
            ExpansionPort::None,
            false,
        );
        assert_eq!(message, "Hardware: NES PAL");
    }

    #[test]
    fn hardware_mode_toast_nes_ntsc_with_four_score() {
        use crate::nes::console::HardwareModel;
        let message = hardware_mode_toast_message(
            HardwareMode::Nes,
            HardwareModel::NesNtsc,
            ExpansionPort::None,
            true,
        );
        assert_eq!(message, "Hardware: NES NTSC (Four Score)");
    }

    #[test]
    fn hardware_mode_toast_famicom_no_expansion() {
        use crate::nes::console::HardwareModel;
        let message = hardware_mode_toast_message(
            HardwareMode::Famicom,
            HardwareModel::NesNtsc,
            ExpansionPort::None,
            false,
        );
        assert_eq!(message, "Hardware: Famicom");
    }

    #[test]
    fn hardware_mode_toast_famicom_with_four_players() {
        use crate::nes::console::HardwareModel;
        let message = hardware_mode_toast_message(
            HardwareMode::Famicom,
            HardwareModel::NesNtsc,
            ExpansionPort::FamicomFourPlayers,
            false,
        );
        assert_eq!(message, "Hardware: Famicom (4-player expansion)");
    }

    #[test]
    fn hardware_mode_toast_famicom_with_power_pad() {
        use crate::nes::console::HardwareModel;
        let message = hardware_mode_toast_message(
            HardwareMode::Famicom,
            HardwareModel::NesNtsc,
            ExpansionPort::PowerPadFamicom,
            false,
        );
        assert_eq!(message, "Hardware: Famicom (Power Pad expansion)");
    }

    // ── Gamepad hot-plug toast messages ───────────────────────────────────────

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
