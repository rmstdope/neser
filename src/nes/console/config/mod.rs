//! Configuration for the NES emulator.
//!
//! The `Config` struct holds all configurable options for the emulator instance.
//! Configuration values are loaded with the following priority (highest to lowest):
//! 1. Command-line arguments
//! 2. Config file (neser.conf)
//! 3. Default values

use crate::nes::console::TimingMode;
use crate::nes::input::ControllerType;
use crate::nes::ppu::NesPalette;
use bitflags::bitflags;

pub mod cli;
pub mod defaults;

pub(crate) use cli::CLI_FLAGS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareModel {
    NesNtsc,
    NesPal,
    Dendy,
}

impl HardwareModel {
    pub const fn from_timing_mode(timing_mode: TimingMode) -> Self {
        match timing_mode {
            TimingMode::Pal => Self::NesPal,
            TimingMode::Dendy => Self::Dendy,
            TimingMode::Ntsc | TimingMode::MultiRegion | TimingMode::Unknown(_) => Self::NesNtsc,
        }
    }

    pub const fn timing_mode(self) -> TimingMode {
        match self {
            Self::NesNtsc => TimingMode::Ntsc,
            Self::NesPal => TimingMode::Pal,
            Self::Dendy => TimingMode::Dendy,
        }
    }

    #[allow(dead_code)] // Used by the wasm frontend
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NesNtsc => "nes-ntsc",
            Self::NesPal => "nes-pal",
            Self::Dendy => "dendy",
        }
    }

    /// Human-readable label for display in logs and summaries.
    pub const fn display_label(self) -> &'static str {
        match self {
            Self::NesNtsc => "NTSC",
            Self::NesPal => "PAL",
            Self::Dendy => "Dendy",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareMode {
    Nes,
    Famicom,
}

impl HardwareMode {
    /// Human-readable label for display in logs and summaries.
    pub const fn display_label(self) -> &'static str {
        match self {
            Self::Nes => "NES",
            Self::Famicom => "Famicom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpansionPort {
    None,
    FamicomFourPlayers,
    ArkanoidFamicom,
    ZapperFamicom,
    PowerPadFamicom,
    VsSystem,
    Playchoice10,
}

impl ExpansionPort {
    /// Human-readable label for display in logs and summaries.
    /// Returns `None` for `ExpansionPort::None`.
    pub fn display_label(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::FamicomFourPlayers => Some("Famicom Four Players"),
            Self::ArkanoidFamicom => Some("Arkanoid"),
            Self::ZapperFamicom => Some("Zapper"),
            Self::PowerPadFamicom => Some("Power Pad"),
            Self::VsSystem => Some("VS System"),
            Self::Playchoice10 => Some("PlayChoice-10"),
        }
    }

    fn parse(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("none") {
            Some(Self::None)
        } else if value.eq_ignore_ascii_case("famicom-four-players") {
            Some(Self::FamicomFourPlayers)
        } else if value.eq_ignore_ascii_case("arkanoid") {
            Some(Self::ArkanoidFamicom)
        } else if value.eq_ignore_ascii_case("zapper") {
            Some(Self::ZapperFamicom)
        } else if value.eq_ignore_ascii_case("power-pad") || value.eq_ignore_ascii_case("powerpad")
        {
            Some(Self::PowerPadFamicom)
        } else if value.eq_ignore_ascii_case("vs-system") || value.eq_ignore_ascii_case("vssystem")
        {
            Some(Self::VsSystem)
        } else if value.eq_ignore_ascii_case("playchoice10")
            || value.eq_ignore_ascii_case("playchoice-10")
        {
            Some(Self::Playchoice10)
        } else {
            None
        }
    }

    fn is_famicom_only(self) -> bool {
        matches!(
            self,
            Self::FamicomFourPlayers
                | Self::ArkanoidFamicom
                | Self::ZapperFamicom
                | Self::PowerPadFamicom
        )
    }
}

/// NES-specific hardware configuration.
///
/// These settings control NES hardware behavior: CPU/PPU model, expansion
/// ports, controller types, RAM initialization, APU channel enables, etc.
#[derive(Debug, Clone)]
pub struct NesConfig {
    /// Emulated hardware family mode.
    pub hardware_mode: HardwareMode,
    /// Whether hardware mode was explicitly configured.
    pub hardware_mode_explicit: bool,
    /// Controller type connected to expansion port.
    pub expansion_port: ExpansionPort,
    /// Whether expansion port type was explicitly configured.
    pub expansion_port_explicit: bool,
    /// VS System DIP switch value (8-bit, one bit per switch).
    pub vs_dip_switches: u8,
    /// VS System swapped controller wiring (VsSystem4017 / VsSystemSwapped).
    /// When true, the arcade cabinet reads P1 from $4017 (left stick) and
    /// P2 from $4016 (right stick), so d-pad/A/B are swapped between ports.
    pub vs_controllers_swapped: bool,
    /// Emulated hardware model.
    pub hardware_model: HardwareModel,
    /// Whether the hardware model was explicitly configured.
    pub hardware_model_explicit: bool,
    /// Whether Four Score mode is enabled.
    pub four_score_enabled: bool,
    /// Whether Four Score mode was explicitly configured.
    pub four_score_enabled_explicit: bool,
    /// APU channel enable flags.
    pub apu_channels: ApuChannels,
    /// Controller type connected to port 1.
    pub controller_port1: ControllerType,
    /// Controller type connected to port 2.
    pub controller_port2: ControllerType,
    /// Whether controller_port1 was explicitly configured (not default).
    pub controller_port1_explicit: bool,
    /// Whether controller_port2 was explicitly configured (not default).
    pub controller_port2_explicit: bool,
    /// Zapper light detection size (config key: `zapper_detection_size`).
    pub zapper_detection_size: u8,
    /// Enable dynamic OAM DRAM decay emulation.
    pub oam_dram_decay_enabled: bool,
    /// Horizontal overscan removal in pixels (removed from both left and right edges).
    pub horizontal_overscan: u8,
    /// Vertical overscan removal in pixels (removed from both top and bottom edges).
    pub vertical_overscan: u8,
    /// Selected preset system (RGB) palette used for composite NTSC output.
    pub palette: NesPalette,
}

impl Default for NesConfig {
    fn default() -> Self {
        Self {
            hardware_mode: HardwareMode::Nes,
            hardware_mode_explicit: false,
            expansion_port: ExpansionPort::None,
            expansion_port_explicit: false,
            vs_dip_switches: 0x00,
            vs_controllers_swapped: false,
            hardware_model: HardwareModel::NesNtsc,
            hardware_model_explicit: false,
            four_score_enabled: false,
            four_score_enabled_explicit: false,
            apu_channels: ApuChannels::ALL,
            controller_port1: ControllerType::Joypad,
            controller_port2: ControllerType::Joypad,
            controller_port1_explicit: false,
            controller_port2_explicit: false,
            zapper_detection_size: 0,
            oam_dram_decay_enabled: false,
            horizontal_overscan: 0,
            vertical_overscan: 8,
            palette: NesPalette::default(),
        }
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ApuChannels: u8 {
        const PULSE1 = 0b00001;
        const PULSE2 = 0b00010;
        const TRIANGLE = 0b00100;
        const NOISE = 0b01000;
        const DMC = 0b10000;
        const ALL = 0b11111;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timing_mode_values_match_expected_cpu_clock_and_scanlines() {
        let ntsc_timing = HardwareModel::NesNtsc.timing_mode();
        assert_eq!(ntsc_timing.cpu_clock_hz(), 1_789_773.0);
        assert_eq!(ntsc_timing.scanlines_per_frame(), 262);

        let pal_timing = HardwareModel::NesPal.timing_mode();
        assert_eq!(pal_timing.cpu_clock_hz(), 1_662_607.0);
        assert_eq!(pal_timing.scanlines_per_frame(), 312);
    }

    #[test]
    fn test_hardware_model_dendy_timing_mode_is_dendy() {
        assert_eq!(
            HardwareModel::Dendy.timing_mode(),
            crate::nes::cartridge::TimingMode::Dendy
        );
    }

    #[test]
    fn test_hardware_model_dendy_display_label_is_dendy() {
        assert_eq!(HardwareModel::Dendy.display_label(), "Dendy");
    }

    #[test]
    fn test_hardware_model_dendy_as_str_is_dendy() {
        assert_eq!(HardwareModel::Dendy.as_str(), "dendy");
    }

    #[test]
    fn test_timing_mode_dendy_cpu_clock_and_scanlines_via_hardware_model() {
        let dendy_timing = HardwareModel::Dendy.timing_mode();
        assert_eq!(dendy_timing.cpu_clock_hz(), 1_773_448.0);
        assert_eq!(dendy_timing.scanlines_per_frame(), 312);
    }

    #[test]
    fn test_config_expansion_port_parse_zapper() {
        assert_eq!(
            ExpansionPort::parse("zapper"),
            Some(ExpansionPort::ZapperFamicom)
        );
    }

    #[test]
    fn test_config_expansion_port_parse_power_pad() {
        assert_eq!(
            ExpansionPort::parse("power-pad"),
            Some(ExpansionPort::PowerPadFamicom)
        );
        assert_eq!(
            ExpansionPort::parse("powerpad"),
            Some(ExpansionPort::PowerPadFamicom)
        );
    }

    #[test]
    fn test_config_expansion_port_parse_vs_system() {
        assert_eq!(
            ExpansionPort::parse("vs-system"),
            Some(ExpansionPort::VsSystem)
        );
        assert_eq!(
            ExpansionPort::parse("vssystem"),
            Some(ExpansionPort::VsSystem)
        );
    }

    #[test]
    fn test_config_expansion_port_parse_playchoice10() {
        assert_eq!(
            ExpansionPort::parse("playchoice10"),
            Some(ExpansionPort::Playchoice10)
        );
        assert_eq!(
            ExpansionPort::parse("playchoice-10"),
            Some(ExpansionPort::Playchoice10)
        );
    }

    #[test]
    fn test_config_vs_system_not_famicom_only() {
        assert!(
            !ExpansionPort::VsSystem.is_famicom_only(),
            "VS System should not be classified as Famicom-only"
        );
    }
}
