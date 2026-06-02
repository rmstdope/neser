use crate::nes::console::{ExpansionPort, HardwareMode, TimingMode};
use crate::nes::ppu::NesPalette;

pub fn emulator_timing_toast_message(tv_system: TimingMode) -> String {
    format!("Emulator timing: {}", tv_system_toast_label(tv_system))
}

/// Toast message shown when the preset system palette changes (F8).
pub fn palette_toast_message(palette: NesPalette) -> String {
    format!("Palette: {}", palette.display_name())
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
            ExpansionPort::Playchoice10 => "Hardware: PlayChoice-10".to_string(),
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
    fn emulator_timing_toast_uses_ntsc_label() {
        let message = emulator_timing_toast_message(TimingMode::Ntsc);
        assert_eq!(message, "Emulator timing: NTSC");
    }

    #[test]
    fn palette_toast_uses_display_name() {
        assert_eq!(
            palette_toast_message(NesPalette::Default),
            "Palette: Default"
        );
        assert_eq!(palette_toast_message(NesPalette::NesDev), "Palette: NesDev");
        assert_eq!(
            palette_toast_message(NesPalette::CompositeDirect),
            "Palette: Composite Direct"
        );
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
}
