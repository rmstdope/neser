//! Configuration for the NES emulator.
//!
//! The `Config` struct holds all configurable options for the emulator instance.
//! Configuration values are loaded with the following priority (highest to lowest):
//! 1. Command-line arguments
//! 2. Config file (neser.conf)
//! 3. Default values

use crate::gba::console::config::GBA_FILTER_NAMES;
use crate::nes::console::TimingMode;
use crate::nes::input::ControllerType;
use crate::platform::config::CliFlag;
use crate::platform::config::ParseResult;
use crate::platform::config::{OPTIONAL_BOOL_FLAGS, parse_bool, parse_hex_u8};
use crate::platform::shaders::SHADER_PRESETS;
use bitflags::bitflags;
use std::fs;
use std::path::Path;

/// All supported NES-specific CLI flags.
pub(crate) const CLI_FLAGS: &[CliFlag] = &[
    CliFlag {
        flag: "--trace-nestest",
        help: Some("Enable CPU trace output (nestest.log format)"),
        has_value: false,
    },
    CliFlag {
        flag: "--trace-ppu",
        help: Some("Enable PPU trace output"),
        has_value: false,
    },
    CliFlag {
        flag: "--trace-apu",
        help: Some("Enable APU trace output"),
        has_value: false,
    },
    CliFlag {
        flag: "--trace-mapper",
        help: Some("Enable mapper trace output"),
        has_value: false,
    },
    CliFlag {
        flag: "--nes-filter",
        help: Some("NES shader filter: crt, ntsc, smooth, pal, or none"),
        has_value: true,
    },
    CliFlag {
        flag: "--nes-controller-port1",
        help: Some(
            "Controller type for port 1: joypad, snes-controller, snes-mouse, zapper, arkanoid, power-pad",
        ),
        has_value: true,
    },
    CliFlag {
        flag: "--nes-controller-port2",
        help: Some(
            "Controller type for port 2: joypad, snes-controller, snes-mouse, zapper, arkanoid, power-pad",
        ),
        has_value: true,
    },
    CliFlag {
        flag: "--nes-hardware",
        help: Some(
            "NES hardware mode: nes-ntsc, nes-pal, famicom, dendy, or playchoice10/playchoice-10 (default: nes-ntsc)",
        ),
        has_value: true,
    },
    CliFlag {
        flag: "--nes-expansion-port",
        help: Some(
            "Expansion port controller: none, famicom-four-players, arkanoid, zapper, power-pad, vs-system, or playchoice10 (default: none)",
        ),
        has_value: true,
    },
    CliFlag {
        flag: "--nes-zapper-detection-size",
        help: Some(
            "Zapper light detection square radius in pixels (0..=255, default: 0; higher values are more tolerant but slower)",
        ),
        has_value: true,
    },
    CliFlag {
        flag: "--nes-oam-dram-decay",
        help: Some("Enable OAM DRAM decay emulation (true/false, default: false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--nes-enable-4-score",
        help: Some(
            "Enable Four Score mode (optionally: true/false, default when flag present: true)",
        ),
        has_value: false,
    },
    CliFlag {
        flag: "--no-nes-4-score",
        help: Some("Disable Four Score mode (equivalent to --nes-enable-4-score false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-nes-4-score",
        help: Some("Disable Four Score mode (equivalent to --nes-enable-4-score false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--nes-pulse1",
        help: Some(
            "Enable pulse 1 channel (optionally: true/false, default when flag present: true)",
        ),
        has_value: false,
    },
    CliFlag {
        flag: "--no-nes-pulse1",
        help: Some("Disable pulse 1 channel (equivalent to --nes-pulse1 false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-nes-pulse1",
        help: Some("Disable pulse 1 channel (equivalent to --nes-pulse1 false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--nes-pulse2",
        help: Some(
            "Enable pulse 2 channel (optionally: true/false, default when flag present: true)",
        ),
        has_value: false,
    },
    CliFlag {
        flag: "--no-nes-pulse2",
        help: Some("Disable pulse 2 channel (equivalent to --nes-pulse2 false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-nes-pulse2",
        help: Some("Disable pulse 2 channel (equivalent to --nes-pulse2 false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--nes-triangle",
        help: Some(
            "Enable triangle channel (optionally: true/false, default when flag present: true)",
        ),
        has_value: false,
    },
    CliFlag {
        flag: "--no-nes-triangle",
        help: Some("Disable triangle channel (equivalent to --nes-triangle false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-nes-triangle",
        help: Some("Disable triangle channel (equivalent to --nes-triangle false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--nes-noise",
        help: Some(
            "Enable noise channel (optionally: true/false, default when flag present: true)",
        ),
        has_value: false,
    },
    CliFlag {
        flag: "--no-nes-noise",
        help: Some("Disable noise channel (equivalent to --nes-noise false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-nes-noise",
        help: Some("Disable noise channel (equivalent to --nes-noise false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--nes-dmc",
        help: Some("Enable DMC channel (optionally: true/false, default when flag present: true)"),
        has_value: false,
    },
    CliFlag {
        flag: "--no-nes-dmc",
        help: Some("Disable DMC channel (equivalent to --nes-dmc false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--disable-nes-dmc",
        help: Some("Disable DMC channel (equivalent to --nes-dmc false)"),
        has_value: false,
    },
    CliFlag {
        flag: "--nes-horizontal-overscan",
        help: Some(
            "Horizontal overscan removal in pixels (0..=8, default: 0; removed from left and right)",
        ),
        has_value: true,
    },
    CliFlag {
        flag: "--nes-vertical-overscan",
        help: Some(
            "Vertical overscan removal in pixels (0..=16, default: 8; removed from top and bottom)",
        ),
        has_value: true,
    },
];

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
        }
    }
}

impl NesConfig {
    /// Apply command-line arguments to NES configuration.
    ///
    /// Parses NES-specific CLI flags (hardware, controllers, expansion,  4-score, APU channels, zapper, OAM, overscan).
    pub(crate) fn apply_args(&mut self, args: &[String]) -> Result<(), String> {
        use crate::platform::config::{has_negation_flag, parse_bool_arg, parse_u32_arg};

        // OAM DRAM decay: --nes-oam-dram-decay true/false
        if let Some(oam_dram_decay) = parse_bool_arg(args, "--nes-oam-dram-decay")? {
            self.oam_dram_decay_enabled = oam_dram_decay;
        }

        // Four Score: --nes-enable-4-score true/false, --no-nes-4-score, --disable-nes-4-score
        if let Some(four_score) = parse_bool_arg(args, "--nes-enable-4-score")? {
            self.four_score_enabled = four_score;
            self.four_score_enabled_explicit = true;
        }
        if has_negation_flag(args, &["--no-nes-4-score", "--disable-nes-4-score"]) {
            self.four_score_enabled = false;
            self.four_score_enabled_explicit = true;
        }

        // APU channel enable/disable flags (support both value-based and prefix negation)
        // Pulse1: --nes-pulse1 true/false, --no-nes-pulse1, --disable-nes-pulse1
        if let Some(pulse1) = parse_bool_arg(args, "--nes-pulse1")? {
            if pulse1 {
                self.apu_channels.insert(ApuChannels::PULSE1);
            } else {
                self.apu_channels.remove(ApuChannels::PULSE1);
            }
        }
        if has_negation_flag(args, &["--no-nes-pulse1", "--disable-nes-pulse1"]) {
            self.apu_channels.remove(ApuChannels::PULSE1);
        }

        // Pulse2: --nes-pulse2 true/false, --no-nes-pulse2, --disable-nes-pulse2
        if let Some(pulse2) = parse_bool_arg(args, "--nes-pulse2")? {
            if pulse2 {
                self.apu_channels.insert(ApuChannels::PULSE2);
            } else {
                self.apu_channels.remove(ApuChannels::PULSE2);
            }
        }
        if has_negation_flag(args, &["--no-nes-pulse2", "--disable-nes-pulse2"]) {
            self.apu_channels.remove(ApuChannels::PULSE2);
        }

        // Triangle: --nes-triangle true/false, --no-nes-triangle, --disable-nes-triangle
        if let Some(triangle) = parse_bool_arg(args, "--nes-triangle")? {
            if triangle {
                self.apu_channels.insert(ApuChannels::TRIANGLE);
            } else {
                self.apu_channels.remove(ApuChannels::TRIANGLE);
            }
        }
        if has_negation_flag(args, &["--no-nes-triangle", "--disable-nes-triangle"]) {
            self.apu_channels.remove(ApuChannels::TRIANGLE);
        }

        // Noise: --nes-noise true/false, --no-nes-noise, --disable-nes-noise
        if let Some(noise) = parse_bool_arg(args, "--nes-noise")? {
            if noise {
                self.apu_channels.insert(ApuChannels::NOISE);
            } else {
                self.apu_channels.remove(ApuChannels::NOISE);
            }
        }
        if has_negation_flag(args, &["--no-nes-noise", "--disable-nes-noise"]) {
            self.apu_channels.remove(ApuChannels::NOISE);
        }

        // DMC: --nes-dmc true/false, --no-nes-dmc, --disable-nes-dmc
        if let Some(dmc) = parse_bool_arg(args, "--nes-dmc")? {
            if dmc {
                self.apu_channels.insert(ApuChannels::DMC);
            } else {
                self.apu_channels.remove(ApuChannels::DMC);
            }
        }
        if has_negation_flag(args, &["--no-nes-dmc", "--disable-nes-dmc"]) {
            self.apu_channels.remove(ApuChannels::DMC);
        }

        // Zapper detection size
        if let Some(size) = parse_u32_arg(args, "--nes-zapper-detection-size")? {
            let size_u8 = u8::try_from(size).map_err(|_| {
                format!(
                    "Invalid --nes-zapper-detection-size value: {} (must be between 0 and 255)",
                    size
                )
            })?;
            self.zapper_detection_size = size_u8;

            if size_u8 > 10 {
                eprintln!(
                    "Warning: --nes-zapper-detection-size={} may cause performance issues. \
                     Large values sample (2*size + 1)^2 = {} pixels per controller read. \
                     Consider values <= 10 for better performance.",
                    size_u8,
                    (2 * size_u8 as u32 + 1).pow(2)
                );
            }
        }

        // Overscan
        if let Some(v) = parse_u32_arg(args, "--nes-horizontal-overscan")? {
            self.horizontal_overscan = v.min(8) as u8;
        }
        if let Some(v) = parse_u32_arg(args, "--nes-vertical-overscan")? {
            self.vertical_overscan = v.min(16) as u8;
        }

        Ok(())
    }

    /// Apply a single config file key-value pair to NES configuration.
    ///
    /// Handles NES-specific config keys (nes_hardware, nes_controller_port1, etc.).
    pub(crate) fn apply_config_value(&mut self, key: &str, value: &str) -> Result<(), String> {
        use crate::platform::config::parse_bool;
        let key = key.replace('-', "_");

        match key.as_str() {
            "nes_enable_4_score" => {
                if let Ok(b) = parse_bool(value) {
                    self.four_score_enabled = b;
                    self.four_score_enabled_explicit = true;
                }
            }
            "nes_pulse1" => {
                if let Ok(b) = parse_bool(value) {
                    if b {
                        self.apu_channels.insert(ApuChannels::PULSE1);
                    } else {
                        self.apu_channels.remove(ApuChannels::PULSE1);
                    }
                }
            }
            "nes_pulse2" => {
                if let Ok(b) = parse_bool(value) {
                    if b {
                        self.apu_channels.insert(ApuChannels::PULSE2);
                    } else {
                        self.apu_channels.remove(ApuChannels::PULSE2);
                    }
                }
            }
            "nes_triangle" => {
                if let Ok(b) = parse_bool(value) {
                    if b {
                        self.apu_channels.insert(ApuChannels::TRIANGLE);
                    } else {
                        self.apu_channels.remove(ApuChannels::TRIANGLE);
                    }
                }
            }
            "nes_noise" => {
                if let Ok(b) = parse_bool(value) {
                    if b {
                        self.apu_channels.insert(ApuChannels::NOISE);
                    } else {
                        self.apu_channels.remove(ApuChannels::NOISE);
                    }
                }
            }
            "nes_dmc" => {
                if let Ok(b) = parse_bool(value) {
                    if b {
                        self.apu_channels.insert(ApuChannels::DMC);
                    } else {
                        self.apu_channels.remove(ApuChannels::DMC);
                    }
                }
            }
            "nes_zapper_detection_size" => {
                if let Ok(size) = value.parse::<u8>() {
                    self.zapper_detection_size = size;
                    if size > 10 {
                        eprintln!(
                            "Warning: nes-zapper_detection_size={} may cause performance issues. \
                             Large values sample (2*size + 1)² = {} pixels per controller read. \
                             Consider using values ≤ 10 for better performance.",
                            size,
                            (2 * size as u32 + 1).pow(2)
                        );
                    }
                } else {
                    eprintln!(
                        "Warning: invalid value '{}' for 'nes-zapper_detection_size' in configuration; \
                         ignoring. Must be a number between 0 and 255.",
                        value
                    );
                }
            }
            "nes_oam_dram_decay" | "nes_oam_dram_decay_enabled" => {
                if let Ok(b) = parse_bool(value) {
                    self.oam_dram_decay_enabled = b;
                } else {
                    eprintln!(
                        "Warning: invalid value '{}' for 'nes-oam_dram_decay'; keeping current value. \
                         Valid values: true/false/yes/no/1/0",
                        value
                    );
                }
            }
            "nes_horizontal_overscan" => {
                if let Ok(v) = value.parse::<u8>() {
                    self.horizontal_overscan = v.min(8);
                }
            }
            "nes_vertical_overscan" => {
                if let Ok(v) = value.parse::<u8>() {
                    self.vertical_overscan = v.min(16);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

use crate::platform::config::Config;

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

impl Config {
    pub(crate) fn parse_hardware_value(value: &str) -> Option<(HardwareMode, HardwareModel)> {
        if value.eq_ignore_ascii_case("nes-ntsc") {
            Some((HardwareMode::Nes, HardwareModel::NesNtsc))
        } else if value.eq_ignore_ascii_case("nes-pal") {
            Some((HardwareMode::Nes, HardwareModel::NesPal))
        } else if value.eq_ignore_ascii_case("famicom") {
            Some((HardwareMode::Famicom, HardwareModel::NesNtsc))
        } else if value.eq_ignore_ascii_case("dendy") {
            Some((HardwareMode::Nes, HardwareModel::Dendy))
        } else if value.eq_ignore_ascii_case("playchoice10")
            || value.eq_ignore_ascii_case("playchoice-10")
        {
            Some((HardwareMode::Nes, HardwareModel::NesNtsc))
        } else {
            None
        }
    }

    fn parse_hardware_arg(
        args: &[String],
    ) -> Result<Option<(HardwareMode, HardwareModel)>, String> {
        if let Some(hardware) = Self::parse_string_arg(args, "--nes-hardware") {
            if let Some(parsed) = Self::parse_hardware_value(&hardware) {
                Ok(Some(parsed))
            } else {
                Err(format!(
                    "Invalid --nes-hardware value: '{}'. Valid options are: nes-ntsc, nes-pal, famicom, dendy, playchoice10/playchoice-10",
                    hardware
                ))
            }
        } else {
            Ok(None)
        }
    }

    fn parse_expansion_port_arg(args: &[String]) -> Result<Option<ExpansionPort>, String> {
        if let Some(expansion_port) = Self::parse_string_arg(args, "--nes-expansion-port") {
            let parsed = ExpansionPort::parse(&expansion_port).ok_or_else(|| {
                format!(
                    "Invalid --nes-expansion-port value: '{}'. Valid options are: none, famicom-four-players, arkanoid, zapper, power-pad, vs-system, playchoice10",
                    expansion_port
                )
            })?;
            Ok(Some(parsed))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn apply_hardware_value(&mut self, value: &str) -> Result<(), String> {
        let (hardware_mode, hardware_model) =
            Self::parse_hardware_value(value).ok_or_else(|| {
                format!(
                    "Invalid nes-hardware value: '{}'. Valid options are: nes-ntsc, nes-pal, famicom, dendy, playchoice10/playchoice-10",
                    value
                )
            })?;
        self.nes.hardware_mode = hardware_mode;
        self.nes.hardware_mode_explicit = true;
        self.nes.hardware_model = hardware_model;
        self.nes.hardware_model_explicit = true;

        if value.eq_ignore_ascii_case("playchoice10") || value.eq_ignore_ascii_case("playchoice-10")
        {
            self.nes.expansion_port = ExpansionPort::Playchoice10;
            self.nes.expansion_port_explicit = true;
        }

        Ok(())
    }

    pub(crate) fn apply_expansion_port_value(&mut self, value: &str) -> Result<(), String> {
        self.nes.expansion_port = ExpansionPort::parse(value)
            .ok_or_else(|| format!("Invalid nes-expansion_port value: '{}'", value))?;
        self.nes.expansion_port_explicit = true;
        Ok(())
    }

    fn valid_controller_values() -> &'static str {
        "joypad, snes-controller, snes-mouse, zapper, arkanoid, power-pad"
    }

    fn parse_controller_arg(flag: &str, value: &str) -> Result<ControllerType, String> {
        ControllerType::parse(value).ok_or_else(|| {
            format!(
                "Invalid value '{}' for '{}'. Valid options are: {}",
                value,
                flag,
                Self::valid_controller_values()
            )
        })
    }

    /// Create a new Config with only default values (no config files or args).
    #[cfg(test)]
    pub fn with_defaults() -> Self {
        Self::default()
    }

    /// Create a new Config from command-line arguments.
    ///
    /// Configuration is loaded in the following order (later overrides earlier):
    /// 1. Default values
    /// 2. ~/.neser/neser.conf (user-wide config, if it exists)
    /// 3. ./neser.conf (project-specific config, if it exists)
    /// 4. --config <file> (explicit config file, if specified)
    /// 5. Command-line arguments
    ///
    /// If --config is specified with a non-existent file, an error is returned.
    ///
    /// # Arguments
    /// * `args` - Command-line arguments (including program name at index 0).
    ///
    /// # Returns
    /// - `Ok(ParseResult::Help)` if --help or -h was specified
    /// - `Ok(ParseResult::Config(config))` on successful parse
    /// - `Err(message)` on validation error
    #[allow(clippy::new_ret_no_self)]
    pub fn new(args: &[String]) -> Result<ParseResult, String> {
        // Check for help first
        if args.iter().any(|a| a == "--help" || a == "-h") {
            return Ok(ParseResult::Help);
        }

        // Validate arguments
        crate::platform::config::validate_args(args)?;

        // Step 1: Start with defaults
        let mut config = Self::default();

        // Step 2: Load config files in priority order
        // Check if --config was specified
        if let Some(config_path) = Self::parse_config_arg(args) {
            // Explicit config file - must exist
            let path = Path::new(&config_path);
            if !path.exists() {
                return Err(format!("Config file not found: {}", config_path));
            }
            config.load_from_file(path)?;
        } else {
            // Load from default locations (later overrides earlier)
            // First: ~/.neser/neser.conf
            if let Some(home) = std::env::var_os("HOME") {
                let home_config = Path::new(&home).join(".neser").join(Self::CONFIG_FILE_NAME);
                config.load_from_file(&home_config)?;
            }
            // Second: ./neser.conf (overrides user config)
            config.load_from_file(Path::new(Self::CONFIG_FILE_NAME))?;
        }

        // Step 3: Apply command-line arguments (override config file and defaults)
        config.apply_args(args)?;

        config.validate_controller_ports()?;

        Ok(ParseResult::Config(Box::new(config)))
    }

    /// Print help text to stdout.
    pub fn print_help() {
        crate::platform::config::print_help();
    }

    /// Apply command-line arguments to the config.
    /// Arguments override any values set by defaults or config file.
    fn apply_args(&mut self, args: &[String]) -> Result<(), String> {
        // Delegate to sub-config apply_args() methods
        self.frontend.apply_args(args)?;
        self.nes.apply_args(args)?;

        // Parse hardware mode (TODO: move to NesConfig in task 8)
        let hardware_arg = Self::parse_string_arg(args, "--nes-hardware");
        if let Some((hardware_mode, hardware_model)) = Self::parse_hardware_arg(args)? {
            self.nes.hardware_mode = hardware_mode;
            self.nes.hardware_mode_explicit = true;
            self.nes.hardware_model = hardware_model;
            self.nes.hardware_model_explicit = true;

            if let Some(hardware) = hardware_arg.as_deref()
                && (hardware.eq_ignore_ascii_case("playchoice10")
                    || hardware.eq_ignore_ascii_case("playchoice-10"))
            {
                self.nes.expansion_port = ExpansionPort::Playchoice10;
                self.nes.expansion_port_explicit = true;
            }
        }

        // Parse expansion port (TODO: move to NesConfig in task 8)
        if let Some(expansion_port) = Self::parse_expansion_port_arg(args)? {
            self.nes.expansion_port = expansion_port;
            self.nes.expansion_port_explicit = true;
        }

        // Controller ports (TODO: move to NesConfig in task 8)
        if let Some(controller_port1) = Self::parse_string_arg(args, "--nes-controller-port1") {
            self.nes.controller_port1 =
                Self::parse_controller_arg("--nes-controller-port1", &controller_port1)?;
            self.nes.controller_port1_explicit = true;
        }
        if let Some(controller_port2) = Self::parse_string_arg(args, "--nes-controller-port2") {
            self.nes.controller_port2 =
                Self::parse_controller_arg("--nes-controller-port2", &controller_port2)?;
            self.nes.controller_port2_explicit = true;
        }

        // Display argument (only applies if fullscreen is set)
        // TODO: Move to FrontendConfig in task 8
        if self.frontend.fullscreen
            && let Some(display) = Self::parse_display_arg(args)?
        {
            self.frontend.fullscreen_display = Some(display);
        }

        // Shader paths (NES and GB filter names)
        // TODO: Move to FrontendConfig in task 8
        if let Some(filter_name) = Self::parse_named_arg(args, "--nes-filter") {
            self.frontend.shader_path = Some(Self::map_filter_name_for(
                &filter_name,
                &["none", "crt", "smooth", "ntsc", "pal"],
            )?);
        }

        if let Some(filter_name) = Self::parse_named_arg(args, "--gb-filter") {
            self.frontend.shader_path =
                Some(Self::map_filter_name_for(&filter_name, &["none", "dmg"])?);
        }

        if let Some(filter_name) = Self::parse_named_arg(args, "--gba-filter") {
            self.frontend.shader_path =
                Some(Self::map_filter_name_for(&filter_name, GBA_FILTER_NAMES)?);
        }

        // ROM path from positional argument
        // TODO: Move to FrontendConfig in task 8
        if let Some(path) = Self::parse_rom_arg(args)? {
            self.frontend.rom_path = Some(path);
        }

        // GB hardware (parsed by GB config module)
        self.gb.apply_args(args)?;

        // GBA hardware (parsed by GBA config module)
        self.gba.apply_args(args)?;

        Ok(())
    }

    /// Parse the --display argument from command-line args.
    fn parse_display_arg(args: &[String]) -> Result<Option<i32>, String> {
        for i in 0..args.len() {
            if args[i] == "--display" {
                if i + 1 >= args.len() {
                    return Err("Missing value for --display".to_string());
                }
                let value = &args[i + 1];
                let parsed: i32 = value
                    .parse()
                    .map_err(|_| format!("Invalid --display value: {value}"))?;
                if parsed < 0 {
                    return Err("--display must be >= 0".to_string());
                }
                return Ok(Some(parsed));
            }
        }
        Ok(None)
    }

    /// Parse a named flag argument (e.g., `--nes-filter`) from command-line args.
    fn parse_named_arg(args: &[String], flag: &str) -> Option<String> {
        for i in 0..args.len() {
            if args[i] == flag && i + 1 < args.len() {
                return Some(args[i + 1].clone());
            }
        }
        None
    }

    /// Parse the --config argument from command-line args.
    fn parse_config_arg(args: &[String]) -> Option<String> {
        for i in 0..args.len() {
            if args[i] == "--config" && i + 1 < args.len() {
                return Some(args[i + 1].clone());
            }
        }
        None
    }

    /// Parse a positional ROM path from command-line args.
    fn parse_rom_arg(args: &[String]) -> Result<Option<String>, String> {
        let mut i = 1; // Skip program name
        let mut rom_path: Option<String> = None;
        while i < args.len() {
            let arg = &args[i];

            if let Some(flag) = crate::platform::config::all_cli_flags().find(|f| f.flag == arg) {
                if flag.has_value {
                    i += 2;
                }
                // For optional boolean flags, check if next arg is a boolean value
                else if OPTIONAL_BOOL_FLAGS.contains(&arg.as_str()) {
                    i += 1;
                    // Peek at next argument to see if it's a boolean value
                    if i < args.len() && parse_bool(&args[i]).is_ok() {
                        i += 1; // Skip the boolean value
                    }
                } else {
                    i += 1;
                }
                continue;
            }

            if let Some((flag_part, _)) = arg.split_once('=')
                && crate::platform::config::all_cli_flags().any(|f| f.flag == flag_part)
            {
                i += 1;
                continue;
            }

            if arg.starts_with('-') {
                i += 1;
                continue;
            }

            if rom_path.is_some() {
                return Err(format!(
                    "Unexpected positional argument: {arg}\nTry --help for usage."
                ));
            }

            rom_path = Some(arg.clone());
            i += 1;
        }

        Ok(rom_path)
    }

    /// Parse a string argument from command-line args.
    ///
    /// Supports both `--flag value` and `--flag=value` forms.
    fn parse_string_arg(args: &[String], flag: &str) -> Option<String> {
        crate::platform::config::parse_cli_string_arg(args, flag)
    }

    /// Default config file name.
    const CONFIG_FILE_NAME: &'static str = "neser.conf";

    /// Load configuration from a config file.
    ///
    /// The config file uses a simple key=value format, one setting per line.
    /// Lines starting with '#' are treated as comments.
    /// Unknown keys are ignored.
    ///
    /// # Example config file:
    /// ```text
    /// # Hardware mode: nes-ntsc, nes-pal, famicom, or dendy
    /// hardware=nes-ntsc
    ///
    /// # Expansion port: none or famicom-four-players
    /// expansion_port=none
    ///
    /// # Audio settings
    /// audio=true
    /// vsync=true
    ///
    /// # Fullscreen settings
    /// fullscreen=false
    /// display=0
    ///
    /// # Window settings (windowed mode only)
    /// window_height=896
    ///
    /// # Shader/filter
    /// # NES valid values: crt, ntsc, smooth, pal, none
    /// nes-filter=crt
    /// # GB valid values: dmg, none
    /// gb-filter=dmg
    ///
    /// # APU channel toggles
    /// pulse1=true
    /// pulse2=true
    /// triangle=true
    /// noise=true
    /// dmc=true
    /// ```
    fn load_from_file(&mut self, path: &Path) -> Result<(), String> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Ok(()), // File doesn't exist or can't be read - silently ignore
        };

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse key=value
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                self.apply_config_value(key, value)?;
            }
        }
        Ok(())
    }

    /// Map a filter name to a shader path, validating against an allowed list.
    ///
    /// `allowed` must be a subset of names defined in [`crate::platform::shaders::SHADER_PRESETS`].
    fn map_filter_name_for(name: &str, allowed: &[&str]) -> Result<String, String> {
        if !allowed.contains(&name) {
            return Err(format!(
                "Invalid filter name: '{}'. Valid options are: {}",
                name,
                allowed.join(", ")
            ));
        }
        SHADER_PRESETS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, path)| (*path).to_string())
            .ok_or_else(|| format!("Filter '{}' has no shader path defined", name))
    }

    /// Apply a single config file key-value pair.
    ///
    /// Keys are normalized: dashes are treated as underscores, so both
    /// `nes-hardware` and `nes_hardware` are accepted.
    fn apply_config_value(&mut self, key: &str, value: &str) -> Result<(), String> {
        let key = key.replace('-', "_");
        // Delegate to sub-configs first
        self.frontend.apply_config_value(&key, value)?;
        self.nes.apply_config_value(&key, value)?;

        // Handle keys that need Config-level coordination or haven't been moved yet.
        match key.as_str() {
            "nes_hardware" => self.apply_hardware_value(value)?,
            "nes_expansion_port" => self.apply_expansion_port_value(value)?,
            "nes_vs_dip_switches" => {
                self.nes.vs_dip_switches = parse_hex_u8(value).map_err(|_| {
                    format!(
                        "Invalid nes_vs_dip_switches value: '{}'. Expected hex (0x00-0xFF) or decimal (0-255)",
                        value
                    )
                })?;
            }
            "nes_vs_controllers_swapped" => {
                if let Ok(b) = parse_bool(value) {
                    self.nes.vs_controllers_swapped = b;
                }
            }
            "nes_filter" => {
                if !value.is_empty() {
                    self.frontend.shader_path = Some(Self::map_filter_name_for(
                        value,
                        &["none", "crt", "smooth", "ntsc", "pal"],
                    )?);
                }
            }
            "gb_filter" => {
                if !value.is_empty() {
                    self.frontend.shader_path =
                        Some(Self::map_filter_name_for(value, &["none", "dmg"])?);
                }
            }
            "gba_filter" => {
                if !value.is_empty() {
                    self.frontend.shader_path =
                        Some(Self::map_filter_name_for(value, GBA_FILTER_NAMES)?);
                }
            }
            "nes_controller_port1" => {
                self.nes.controller_port1 =
                    Self::parse_controller_arg("nes_controller_port1", value)?;
                self.nes.controller_port1_explicit = true;
            }
            "nes_controller_port2" => {
                self.nes.controller_port2 =
                    Self::parse_controller_arg("nes_controller_port2", value)?;
                self.nes.controller_port2_explicit = true;
            }
            "gb_dmg_variant" => {
                self.gb.apply_config_value("gb_dmg_variant", value)?;
            }
            "gb_hardware" => {
                self.gb.apply_config_value("gb_hardware", value)?;
            }
            "gb_cgb_variant" => {
                self.gb.apply_config_value("gb_cgb_variant", value)?;
            }
            "gb_boot_animation" => {
                self.gb.apply_config_value("gb_boot_animation", value)?;
            }
            "gba_hardware" => {
                self.gba.apply_config_value("gba_hardware", value)?;
            }
            "gba_bios_path" => {
                self.gba.apply_config_value("gba_bios_path", value)?;
            }
            "skip_bios_intro" => {
                self.gba.apply_config_value("skip_bios_intro", value)?;
            }
            "gba_color_correction" => {
                self.gba.apply_config_value("gba_color_correction", value)?;
            }
            "gba_trace_cpu" | "gba_trace_bus" | "gba_trace_dma" | "gba_trace_swi"
            | "gba_trace_mgba_log" => {
                self.gba.apply_config_value(&key, value)?;
            }
            _ => {} // Unknown keys are silently ignored (may have been handled by sub-configs)
        }
        Ok(())
    }

    pub fn apply_rom_timing_mode(
        &mut self,
        rom_timing_mode: crate::nes::cartridge::TimingMode,
    ) -> bool {
        if self.nes.hardware_model_explicit {
            return false;
        }

        if rom_timing_mode.is_ntsc_or_pal()
            || matches!(rom_timing_mode, crate::nes::cartridge::TimingMode::Dendy)
        {
            self.nes.hardware_model = HardwareModel::from_timing_mode(rom_timing_mode);
            true
        } else {
            false
        }
    }

    pub fn apply_rom_db_famicom_four_players_hint(&mut self, has_hint: bool) -> bool {
        if !has_hint {
            return false;
        }

        let mut changed = false;

        if !self.nes.hardware_mode_explicit && self.nes.hardware_mode != HardwareMode::Famicom {
            self.nes.hardware_mode = HardwareMode::Famicom;
            changed = true;
        }

        if !self.nes.hardware_model_explicit && self.nes.hardware_model != HardwareModel::NesNtsc {
            self.nes.hardware_model = HardwareModel::NesNtsc;
            changed = true;
        }

        if !self.nes.expansion_port_explicit
            && self.nes.hardware_mode == HardwareMode::Famicom
            && self.nes.expansion_port != ExpansionPort::FamicomFourPlayers
        {
            self.nes.expansion_port = ExpansionPort::FamicomFourPlayers;
            changed = true;
        }

        changed
    }

    pub fn apply_rom_db_arkanoid_famicom_hint(&mut self, has_hint: bool) -> bool {
        if !has_hint {
            return false;
        }

        let mut changed = false;

        if !self.nes.hardware_mode_explicit && self.nes.hardware_mode != HardwareMode::Famicom {
            self.nes.hardware_mode = HardwareMode::Famicom;
            changed = true;
        }

        if !self.nes.hardware_model_explicit && self.nes.hardware_model != HardwareModel::NesNtsc {
            self.nes.hardware_model = HardwareModel::NesNtsc;
            changed = true;
        }

        if !self.nes.expansion_port_explicit
            && self.nes.hardware_mode == HardwareMode::Famicom
            && self.nes.expansion_port != ExpansionPort::ArkanoidFamicom
        {
            self.nes.expansion_port = ExpansionPort::ArkanoidFamicom;
            changed = true;
        }

        changed
    }

    pub fn apply_rom_db_zapper_famicom_hint(&mut self, has_hint: bool) -> bool {
        if !has_hint {
            return false;
        }

        // Only set expansion port if hardware mode is already Famicom.
        // For NES mode, the Zapper is handled via standard controller ports.
        if !self.nes.expansion_port_explicit
            && self.nes.hardware_mode == HardwareMode::Famicom
            && self.nes.expansion_port != ExpansionPort::ZapperFamicom
        {
            self.nes.expansion_port = ExpansionPort::ZapperFamicom;
            return true;
        }

        false
    }

    /// Apply ROM DB hint for Famicom Power Pad (Family Trainer) expansion.
    ///
    /// Only sets the expansion port if hardware mode is already Famicom.
    /// For NES mode, the Power Pad is handled via standard controller port 2.
    pub fn apply_rom_db_power_pad_famicom_hint(&mut self, has_hint: bool) -> bool {
        if !has_hint {
            return false;
        }

        if !self.nes.expansion_port_explicit
            && self.nes.hardware_mode == HardwareMode::Famicom
            && self.nes.expansion_port != ExpansionPort::PowerPadFamicom
        {
            self.nes.expansion_port = ExpansionPort::PowerPadFamicom;
            return true;
        }

        false
    }

    pub fn apply_rom_db_famicom_region_hint(&mut self, is_japan: bool) -> bool {
        if !is_japan {
            return false;
        }

        if !self.nes.hardware_mode_explicit && self.nes.hardware_mode != HardwareMode::Famicom {
            self.nes.hardware_mode = HardwareMode::Famicom;
            return true;
        }

        false
    }

    pub fn apply_rom_db_vs_system_hint(&mut self, is_vs: bool) -> bool {
        if !is_vs {
            return false;
        }

        if !self.nes.expansion_port_explicit && self.nes.expansion_port != ExpansionPort::VsSystem {
            self.nes.expansion_port = ExpansionPort::VsSystem;
            return true;
        }

        false
    }

    pub fn apply_rom_db_playchoice10_hint(&mut self, is_playchoice10: bool) -> bool {
        if !is_playchoice10 {
            return false;
        }

        if !self.nes.expansion_port_explicit
            && self.nes.expansion_port != ExpansionPort::Playchoice10
        {
            self.nes.expansion_port = ExpansionPort::Playchoice10;
            return true;
        }

        false
    }

    /// Apply ROM DB hint for VS System swapped controller wiring.
    pub fn apply_rom_db_vs_controllers_swapped_hint(&mut self, swapped: bool) {
        self.nes.vs_controllers_swapped = swapped;
    }

    /// Apply ROM DB hint for NES Four Score adapter.
    ///
    /// Unlike other hints, this method also **resets** `four_score_enabled` to `false`
    /// when `has_hint` is `false` and not explicitly configured, so that swapping from a
    /// Four Score ROM to a standard ROM correctly disables Four Score mode.
    pub fn apply_rom_db_nes_four_score_hint(&mut self, has_hint: bool) -> bool {
        if self.nes.four_score_enabled_explicit {
            return false;
        }

        if has_hint && !self.nes.four_score_enabled {
            self.nes.four_score_enabled = true;
            return true;
        }

        if !has_hint && self.nes.four_score_enabled {
            self.nes.four_score_enabled = false;
            return true;
        }

        false
    }

    /// Build a human-readable summary of the emulated hardware configuration.
    ///
    /// Includes hardware mode/model, controller port types, and expansion port.
    /// Intended to be logged at ROM load time so the user can see what
    /// peripherals are active.
    pub fn hardware_summary(&self) -> String {
        self.hardware_summary_with(self.nes.controller_port1, self.nes.controller_port2)
    }

    /// Like [`hardware_summary`] but uses the supplied effective controller types instead of
    /// those stored in the config. Useful when auto-detection overrides the config values for the
    /// current cartridge without permanently mutating them.
    pub fn hardware_summary_with(
        &self,
        port1: crate::nes::input::ControllerType,
        port2: crate::nes::input::ControllerType,
    ) -> String {
        let mut parts = vec![format!(
            "Hardware: {} ({}) | Port 1: {} | Port 2: {}",
            self.nes.hardware_mode.display_label(),
            self.nes.hardware_model.display_label(),
            port1.display_label(),
            port2.display_label(),
        )];

        if self.nes.four_score_enabled {
            let joypad_label = crate::nes::input::ControllerType::Joypad.display_label();
            parts.push(format!(
                "Port 3: {} | Port 4: {} | Four Score: enabled",
                joypad_label, joypad_label
            ));
        }

        if let Some(label) = self.nes.expansion_port.display_label() {
            parts.push(format!("Expansion: {}", label));
        }

        parts.join(" | ")
    }

    fn validate_controller_ports(&self) -> Result<(), String> {
        if self.nes.hardware_mode == HardwareMode::Famicom
            && (self.nes.controller_port1_explicit || self.nes.controller_port2_explicit)
        {
            return Err(
                "In Famicom mode, --controller-port1 and --controller-port2 are not allowed because ports 1 and 2 are hardwired joypads".to_string(),
            );
        }

        if self.nes.hardware_mode == HardwareMode::Nes && self.nes.expansion_port.is_famicom_only()
        {
            return Err("famicom expansion_port requires hardware=famicom".to_string());
        }

        let mouse_emulated_controller_count =
            [self.nes.controller_port1, self.nes.controller_port2]
                .iter()
                .filter(|controller| {
                    matches!(
                        **controller,
                        ControllerType::Arkanoid
                            | ControllerType::Zapper
                            | ControllerType::SnesMouse
                    )
                })
                .count();

        if mouse_emulated_controller_count > 1 {
            return Err(
                "No more than one mouse-emulated controller can be configured (Arkanoid/Zapper)"
                    .to_string(),
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::autorun::{AutorunFormat, AutorunMode};
    use crate::platform::config::RamInitMode;

    fn config_new(mut args: Vec<String>) -> Result<ParseResult, String> {
        use std::io::Write;
        use tempfile::NamedTempFile;

        if args.iter().any(|a| a == "--config") {
            return Config::new(&args);
        }

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"").unwrap();

        args.push("--config".to_string());
        args.push(file.path().to_string_lossy().to_string());

        Config::new(&args)
    }

    fn parse_config(args: Vec<String>) -> Config {
        match config_new(args).unwrap() {
            ParseResult::Config(c) => *c,
            ParseResult::Help => panic!("Expected Config, got Help"),
        }
    }

    #[test]
    fn test_config_default_values() {
        let config = Config::with_defaults();
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
        assert!(config.frontend.audio_enabled);
        assert!(config.frontend.vsync_enabled);
        assert!(config.frontend.gamepads_enabled);
        assert!(!config.frontend.fullscreen);
        assert_eq!(config.frontend.fullscreen_display, None);
        assert_eq!(config.frontend.shader_path, None);
        assert!(!config.frontend.debugger_enabled);
        assert!(!config.frontend.load_state);
        assert!(config.nes.apu_channels.contains(ApuChannels::PULSE1));
        assert!(config.nes.apu_channels.contains(ApuChannels::PULSE2));
        assert!(config.nes.apu_channels.contains(ApuChannels::TRIANGLE));
        assert!(config.nes.apu_channels.contains(ApuChannels::NOISE));
        assert!(config.nes.apu_channels.contains(ApuChannels::DMC));
        assert_eq!(config.frontend.window_height, 896);
        assert_eq!(config.frontend.rom_path, None);
        assert_eq!(config.nes.controller_port1, ControllerType::Joypad);
        assert_eq!(config.nes.controller_port2, ControllerType::Joypad);
    }

    #[test]
    fn test_config_new_defaults() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"").unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_string_lossy().to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
        assert!(config.frontend.audio_enabled);
        assert!(config.frontend.vsync_enabled);
        assert!(config.frontend.gamepads_enabled);
        assert!(!config.frontend.fullscreen);
        assert_eq!(config.frontend.window_height, 896);
        assert_eq!(config.nes.controller_port1, ControllerType::Joypad);
        assert_eq!(config.nes.controller_port2, ControllerType::Joypad);
    }

    #[test]
    fn test_config_hardware_nes_pal() {
        let args = vec![
            "neser".to_string(),
            "--nes-hardware".to_string(),
            "nes-pal".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesPal);
        assert!(config.nes.hardware_model_explicit);
    }

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
    fn test_config_apply_rom_timing_mode_when_not_explicit() {
        let mut config = Config::default();
        let applied = config.apply_rom_timing_mode(crate::nes::cartridge::TimingMode::Pal);
        assert!(applied);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesPal);
    }

    #[test]
    fn test_config_apply_rom_timing_mode_does_not_override_explicit() {
        let mut config = Config {
            nes: NesConfig {
                hardware_model: HardwareModel::NesPal,
                hardware_model_explicit: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let applied = config.apply_rom_timing_mode(crate::nes::cartridge::TimingMode::Ntsc);
        assert!(!applied);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesPal);
    }

    #[test]
    fn test_config_apply_rom_timing_mode_unknown_keeps_default() {
        let mut config = Config::default();
        let applied = config.apply_rom_timing_mode(crate::nes::cartridge::TimingMode::Unknown(0));
        assert!(!applied);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
    }

    #[test]
    fn test_config_apply_rom_timing_mode_dendy_sets_dendy_model() {
        let mut config = Config::default();
        let applied = config.apply_rom_timing_mode(crate::nes::cartridge::TimingMode::Dendy);
        assert!(applied);
        assert_eq!(config.nes.hardware_model, HardwareModel::Dendy);
    }

    #[test]
    fn test_parse_hardware_value_dendy_returns_nes_dendy() {
        let result = Config::parse_hardware_value("dendy");
        assert_eq!(result, Some((HardwareMode::Nes, HardwareModel::Dendy)));
    }

    #[test]
    fn test_parse_hardware_value_dendy_is_case_insensitive() {
        assert_eq!(
            Config::parse_hardware_value("DENDY"),
            Some((HardwareMode::Nes, HardwareModel::Dendy))
        );
        assert_eq!(
            Config::parse_hardware_value("Dendy"),
            Some((HardwareMode::Nes, HardwareModel::Dendy))
        );
    }

    #[test]
    fn test_parse_hardware_value_playchoice10_returns_nes_ntsc() {
        assert_eq!(
            Config::parse_hardware_value("playchoice10"),
            Some((HardwareMode::Nes, HardwareModel::NesNtsc))
        );
        assert_eq!(
            Config::parse_hardware_value("playchoice-10"),
            Some((HardwareMode::Nes, HardwareModel::NesNtsc))
        );
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
    fn test_hardware_arg_dendy_sets_hardware_model() {
        let args = vec![
            "neser".to_string(),
            "--nes-hardware".to_string(),
            "dendy".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.hardware_model, HardwareModel::Dendy);
        assert!(config.nes.hardware_model_explicit);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Nes);
    }

    #[test]
    fn test_config_apply_rom_timing_mode_dendy_does_not_override_explicit() {
        let mut config = Config {
            nes: NesConfig {
                hardware_model: HardwareModel::NesNtsc,
                hardware_model_explicit: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let applied = config.apply_rom_timing_mode(crate::nes::cartridge::TimingMode::Dendy);
        assert!(!applied);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
    }

    #[test]
    fn test_config_apply_rom_db_famicom_four_players_hint_sets_hardware_and_expansion() {
        let mut config = Config::default();
        let changed = config.apply_rom_db_famicom_four_players_hint(true);

        assert!(changed);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Famicom);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
        assert_eq!(config.nes.expansion_port, ExpansionPort::FamicomFourPlayers);
    }

    #[test]
    fn test_config_apply_rom_db_famicom_four_players_hint_respects_explicit_nes_hardware_override()
    {
        let mut config = Config {
            nes: NesConfig {
                hardware_mode: HardwareMode::Nes,
                hardware_mode_explicit: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let changed = config.apply_rom_db_famicom_four_players_hint(true);

        assert!(!changed);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Nes);
        assert_eq!(config.nes.expansion_port, ExpansionPort::None);
    }

    #[test]
    fn test_config_apply_rom_db_famicom_four_players_hint_sets_expansion_when_hardware_explicit_famicom()
     {
        let mut config = Config {
            nes: NesConfig {
                hardware_mode: HardwareMode::Famicom,
                hardware_mode_explicit: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let changed = config.apply_rom_db_famicom_four_players_hint(true);

        assert!(changed);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Famicom);
        assert_eq!(config.nes.expansion_port, ExpansionPort::FamicomFourPlayers);
    }

    #[test]
    fn test_config_apply_rom_db_arkanoid_famicom_hint_sets_hardware_and_expansion() {
        let mut config = Config::default();
        let changed = config.apply_rom_db_arkanoid_famicom_hint(true);

        assert!(changed);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Famicom);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
        assert_eq!(config.nes.expansion_port, ExpansionPort::ArkanoidFamicom);
    }

    #[test]
    fn test_config_apply_rom_db_arkanoid_famicom_hint_respects_explicit_expansion_override() {
        let mut config = Config {
            nes: NesConfig {
                expansion_port: ExpansionPort::None,
                expansion_port_explicit: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let changed = config.apply_rom_db_arkanoid_famicom_hint(true);

        // Hardware mode changes but expansion stays explicit
        assert!(changed);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Famicom);
        assert_eq!(config.nes.expansion_port, ExpansionPort::None);
    }

    #[test]
    fn test_config_apply_rom_db_arkanoid_famicom_hint_false_is_noop() {
        let mut config = Config::default();
        let changed = config.apply_rom_db_arkanoid_famicom_hint(false);

        assert!(!changed);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Nes);
        assert_eq!(config.nes.expansion_port, ExpansionPort::None);
    }

    #[test]
    fn test_config_apply_rom_db_famicom_region_hint_sets_famicom_mode() {
        let mut config = Config::default();
        let changed = config.apply_rom_db_famicom_region_hint(true);

        assert!(changed);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Famicom);
    }

    #[test]
    fn test_config_apply_rom_db_famicom_region_hint_respects_explicit_hardware() {
        let mut config = Config {
            nes: NesConfig {
                hardware_mode: HardwareMode::Nes,
                hardware_mode_explicit: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let changed = config.apply_rom_db_famicom_region_hint(true);

        assert!(!changed);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Nes);
    }

    #[test]
    fn test_config_apply_rom_db_famicom_region_hint_false_is_noop() {
        let mut config = Config::default();
        let changed = config.apply_rom_db_famicom_region_hint(false);

        assert!(!changed);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Nes);
    }

    #[test]
    fn test_config_vsync_false() {
        let args = vec![
            "neser".to_string(),
            "--vsync".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.frontend.vsync_enabled);
    }

    #[test]
    fn test_config_fullscreen_true() {
        let args = vec![
            "neser".to_string(),
            "--fullscreen".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.frontend.fullscreen);
        assert_eq!(config.frontend.fullscreen_display, None);
    }

    #[test]
    fn test_config_fullscreen_with_display() {
        let args = vec![
            "neser".to_string(),
            "--fullscreen".to_string(),
            "true".to_string(),
            "--display".to_string(),
            "1".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.frontend.fullscreen);
        assert_eq!(config.frontend.fullscreen_display, Some(1));
    }

    #[test]
    fn test_config_display_missing_value_errors() {
        let args = vec![
            "neser".to_string(),
            "--fullscreen".to_string(),
            "--display".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_display_negative_value_errors() {
        let args = vec![
            "neser".to_string(),
            "--fullscreen".to_string(),
            "--display".to_string(),
            "-1".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_debugger_enabled() {
        let args = vec![
            "neser".to_string(),
            "--debugger".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.frontend.debugger_enabled);
    }

    #[test]
    fn test_config_pulse1_false() {
        let args = vec![
            "neser".to_string(),
            "--nes-pulse1".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.nes.apu_channels.contains(ApuChannels::PULSE1));
        assert!(config.nes.apu_channels.contains(ApuChannels::PULSE2));
    }

    #[test]
    fn test_config_pulse2_false() {
        let args = vec![
            "neser".to_string(),
            "--nes-pulse2".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.nes.apu_channels.contains(ApuChannels::PULSE1));
        assert!(!config.nes.apu_channels.contains(ApuChannels::PULSE2));
    }

    #[test]
    fn test_config_triangle_false() {
        let args = vec![
            "neser".to_string(),
            "--nes-triangle".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.nes.apu_channels.contains(ApuChannels::TRIANGLE));
    }

    #[test]
    fn test_config_noise_false() {
        let args = vec![
            "neser".to_string(),
            "--nes-noise".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.nes.apu_channels.contains(ApuChannels::NOISE));
    }

    #[test]
    fn test_config_dmc_false() {
        let args = vec![
            "neser".to_string(),
            "--nes-dmc".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.nes.apu_channels.contains(ApuChannels::DMC));
    }

    #[test]
    fn test_config_positional_argument_is_rom_path() {
        let args = vec!["neser".to_string(), "somefile.nes".to_string()];
        let config = parse_config(args);
        assert_eq!(config.frontend.rom_path.as_deref(), Some("somefile.nes"));
    }

    #[test]
    fn test_config_tracing_nestest() {
        let args = vec!["neser".to_string(), "--trace-nestest".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.tracing.enabled);
        assert!(config.frontend.tracing.nestest);
    }

    #[test]
    fn test_config_tracing_ppu() {
        let args = vec!["neser".to_string(), "--trace-ppu".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.ppu, 1);
    }

    #[test]
    fn test_config_tracing_mapper() {
        let args = vec!["neser".to_string(), "--trace-mapper".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.mapper, 1);
    }

    #[test]
    fn test_config_tracing_ppu_with_level() {
        let args = vec!["neser".to_string(), "--trace-ppu=3".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.ppu, 3);
    }

    #[test]
    fn test_config_tracing_apu_with_level() {
        let args = vec!["neser".to_string(), "--trace-apu=4".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.apu, 4);
    }

    #[test]
    fn test_config_tracing_mapper_level_is_capped_at_five() {
        let args = vec!["neser".to_string(), "--trace-mapper=9".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.mapper, 5);
    }

    #[test]
    fn test_config_window_height_invalid_errors() {
        let args = vec![
            "neser".to_string(),
            "--window-height".to_string(),
            "not_a_number".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_zapper_detection_size_cli_space_syntax() {
        let args = vec![
            "neser".to_string(),
            "--nes-zapper-detection-size".to_string(),
            "3".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.zapper_detection_size, 3);
    }

    #[test]
    fn test_config_zapper_detection_size_cli_equals_syntax() {
        let args = vec![
            "neser".to_string(),
            "--nes-zapper-detection-size=7".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.zapper_detection_size, 7);
    }

    #[test]
    fn test_config_zapper_detection_size_cli_out_of_range_errors() {
        let args = vec![
            "neser".to_string(),
            "--nes-zapper-detection-size".to_string(),
            "300".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_default_horizontal_overscan_is_8() {
        let config = Config::default();
        assert_eq!(config.nes.horizontal_overscan, 0);
    }

    #[test]
    fn test_config_default_vertical_overscan_is_8() {
        let config = Config::default();
        assert_eq!(config.nes.vertical_overscan, 8);
    }

    #[test]
    fn test_config_file_horizontal_overscan() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-horizontal_overscan", "4")
            .unwrap();
        assert_eq!(config.nes.horizontal_overscan, 4);
    }

    #[test]
    fn test_config_file_vertical_overscan() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-vertical_overscan", "12")
            .unwrap();
        assert_eq!(config.nes.vertical_overscan, 12);
    }

    #[test]
    fn test_config_file_overscan_zero() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-horizontal_overscan", "0")
            .unwrap();
        config
            .apply_config_value("nes-vertical_overscan", "0")
            .unwrap();
        assert_eq!(config.nes.horizontal_overscan, 0);
        assert_eq!(config.nes.vertical_overscan, 0);
    }

    #[test]
    fn test_config_file_horizontal_overscan_max_is_8() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-horizontal_overscan", "8")
            .unwrap();
        assert_eq!(config.nes.horizontal_overscan, 8);
    }

    #[test]
    fn test_config_file_vertical_overscan_max_is_16() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-vertical_overscan", "16")
            .unwrap();
        assert_eq!(config.nes.vertical_overscan, 16);
    }

    #[test]
    fn test_config_file_horizontal_overscan_above_max_is_clamped_to_8() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-horizontal_overscan", "9")
            .unwrap();
        assert_eq!(config.nes.horizontal_overscan, 8);
    }

    #[test]
    fn test_config_file_vertical_overscan_above_max_is_clamped_to_16() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-vertical_overscan", "17")
            .unwrap();
        assert_eq!(config.nes.vertical_overscan, 16);
    }

    #[test]
    fn test_config_cli_horizontal_overscan() {
        let args = vec![
            "neser".to_string(),
            "--nes-horizontal-overscan".to_string(),
            "4".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.horizontal_overscan, 4);
    }

    #[test]
    fn test_config_cli_vertical_overscan() {
        let args = vec![
            "neser".to_string(),
            "--nes-vertical-overscan".to_string(),
            "0".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.vertical_overscan, 0);
    }

    #[test]
    fn test_config_video_scale_flag_is_unknown() {
        let args = vec![
            "neser".to_string(),
            "--video-scale".to_string(),
            "2.5".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    // Config file tests

    #[test]
    fn test_config_file_hardware_nes_pal() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-hardware", "nes-pal")
            .unwrap();
        assert_eq!(config.nes.hardware_model, HardwareModel::NesPal);
        assert!(config.nes.hardware_model_explicit);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Nes);
        assert!(config.nes.hardware_mode_explicit);
    }

    #[test]
    fn test_config_file_hardware_nes_ntsc() {
        let mut config = Config {
            nes: NesConfig {
                hardware_model: HardwareModel::NesPal,
                ..Default::default()
            },
            ..Default::default()
        };
        config
            .apply_config_value("nes-hardware", "nes-ntsc")
            .unwrap();
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
        assert!(config.nes.hardware_model_explicit);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Nes);
        assert!(config.nes.hardware_mode_explicit);
    }

    #[test]
    fn test_config_file_hardware_case_insensitive() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-hardware", "NES-PAL")
            .unwrap();
        assert_eq!(config.nes.hardware_model, HardwareModel::NesPal);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Nes);

        config
            .apply_config_value("nes-hardware", "NES-NTSC")
            .unwrap();
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Nes);
    }

    #[test]
    fn test_config_file_hardware_playchoice10_forces_playchoice_expansion() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-hardware", "playchoice10")
            .unwrap();

        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
        assert!(config.nes.hardware_model_explicit);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Nes);
        assert!(config.nes.hardware_mode_explicit);
        assert_eq!(config.nes.expansion_port, ExpansionPort::Playchoice10);
        assert!(config.nes.expansion_port_explicit);
    }

    #[test]
    fn test_config_file_hardware_famicom_sets_mode_and_model() {
        let mut config = Config {
            nes: NesConfig {
                hardware_model: HardwareModel::NesPal,
                ..Default::default()
            },
            ..Default::default()
        };
        config
            .apply_config_value("nes-hardware", "famicom")
            .unwrap();
        assert_eq!(config.nes.hardware_mode, HardwareMode::Famicom);
        assert!(config.nes.hardware_mode_explicit);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
        assert!(config.nes.hardware_model_explicit);
    }

    #[test]
    fn test_config_file_audio() {
        let mut config = Config::default();
        config.apply_config_value("audio", "false").unwrap();
        assert!(!config.frontend.audio_enabled);

        config.apply_config_value("audio", "true").unwrap();
        assert!(config.frontend.audio_enabled);
    }

    #[test]
    fn test_config_file_vsync() {
        let mut config = Config::default();
        config.apply_config_value("vsync", "false").unwrap();
        assert!(!config.frontend.vsync_enabled);

        config.apply_config_value("vsync", "true").unwrap();
        assert!(config.frontend.vsync_enabled);
    }

    #[test]
    fn test_config_file_gamepads() {
        let mut config = Config::default();
        config.apply_config_value("gamepads", "false").unwrap();
        assert!(!config.frontend.gamepads_enabled);

        config.apply_config_value("gamepads", "true").unwrap();
        assert!(config.frontend.gamepads_enabled);
    }

    #[test]
    fn test_config_file_fullscreen() {
        let mut config = Config::default();
        config.apply_config_value("fullscreen", "true").unwrap();
        assert!(config.frontend.fullscreen);

        config.apply_config_value("fullscreen", "false").unwrap();
        assert!(!config.frontend.fullscreen);
    }

    #[test]
    fn test_config_file_display() {
        let mut config = Config::default();
        config.apply_config_value("display", "1").unwrap();
        assert_eq!(config.frontend.fullscreen_display, Some(1));

        config.apply_config_value("display", "0").unwrap();
        assert_eq!(config.frontend.fullscreen_display, Some(0));
    }

    #[test]
    fn test_config_file_display_negative_ignored() {
        let mut config = Config::default();
        config.apply_config_value("display", "-1").unwrap();
        assert_eq!(config.frontend.fullscreen_display, None);
    }

    #[test]
    fn test_config_file_filter_invalid_errors() {
        let mut config = Config::default();
        let result = config.apply_config_value("nes-filter", "invalid-filter");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Invalid filter name: 'invalid-filter'. Valid options are: none, crt, smooth, ntsc, pal"
        );
    }

    #[test]
    fn test_config_file_filter_empty_ignored() {
        let mut config = Config::default();
        config.apply_config_value("nes-filter", "").unwrap();
        assert_eq!(config.frontend.shader_path, None);
    }

    #[test]
    fn test_config_file_filter_crt() {
        let mut config = Config::default();
        config.apply_config_value("nes-filter", "crt").unwrap();
        assert_eq!(
            config.frontend.shader_path,
            Some("vendor/slang-shaders/crt/crt-lottes.slangp".to_string())
        );
    }

    #[test]
    fn test_config_file_filter_ntsc() {
        let mut config = Config::default();
        config.apply_config_value("nes-filter", "ntsc").unwrap();
        assert_eq!(
            config.frontend.shader_path,
            Some("vendor/slang-shaders/ntsc/ntsc-256px-composite.slangp".to_string())
        );
    }

    #[test]
    fn test_config_file_filter_smooth() {
        let mut config = Config::default();
        config.apply_config_value("nes-filter", "smooth").unwrap();
        assert_eq!(
            config.frontend.shader_path,
            Some(
                "vendor/slang-shaders/edge-smoothing/xbrz/xbrz-freescale-multipass.slangp"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_config_file_filter_none() {
        let mut config = Config::default();
        config.apply_config_value("nes-filter", "none").unwrap();
        assert_eq!(
            config.frontend.shader_path,
            Some("shaders/stock.slangp".to_string())
        );
    }

    #[test]
    fn test_config_cmdline_filter_crt() {
        let args = vec![
            "neser".to_string(),
            "--nes-filter".to_string(),
            "crt".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.shader_path,
            Some("vendor/slang-shaders/crt/crt-lottes.slangp".to_string())
        );
    }

    #[test]
    fn test_config_cmdline_filter_ntsc() {
        let args = vec![
            "neser".to_string(),
            "--nes-filter".to_string(),
            "ntsc".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.shader_path,
            Some("vendor/slang-shaders/ntsc/ntsc-256px-composite.slangp".to_string())
        );
    }

    #[test]
    fn test_config_cmdline_filter_smooth() {
        let args = vec![
            "neser".to_string(),
            "--nes-filter".to_string(),
            "smooth".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.shader_path,
            Some(
                "vendor/slang-shaders/edge-smoothing/xbrz/xbrz-freescale-multipass.slangp"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_config_cmdline_filter_none() {
        let args = vec![
            "neser".to_string(),
            "--nes-filter".to_string(),
            "none".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.shader_path,
            Some("shaders/stock.slangp".to_string())
        );
    }

    #[test]
    fn test_config_file_debugger() {
        let mut config = Config::default();
        config.apply_config_value("debugger", "true").unwrap();
        assert!(config.frontend.debugger_enabled);
    }

    #[test]
    fn test_config_file_apu_channels() {
        let mut config = Config::default();
        config.apply_config_value("nes-pulse1", "false").unwrap();
        config.apply_config_value("nes-pulse2", "false").unwrap();
        config.apply_config_value("nes-triangle", "false").unwrap();
        config.apply_config_value("nes-noise", "false").unwrap();
        config.apply_config_value("nes-dmc", "false").unwrap();

        assert!(!config.nes.apu_channels.contains(ApuChannels::PULSE1));
        assert!(!config.nes.apu_channels.contains(ApuChannels::PULSE2));
        assert!(!config.nes.apu_channels.contains(ApuChannels::TRIANGLE));
        assert!(!config.nes.apu_channels.contains(ApuChannels::NOISE));
        assert!(!config.nes.apu_channels.contains(ApuChannels::DMC));
    }

    #[test]
    fn test_config_file_window_height() {
        let mut config = Config::default();
        config.apply_config_value("window_height", "720").unwrap();
        assert_eq!(config.frontend.window_height, 720);
    }

    #[test]
    fn test_config_file_controller_ports() {
        let mut config = Config::default();
        let _ = config.apply_config_value("nes-controller_port1", "arkanoid");
        let _ = config.apply_config_value("nes-controller_port2", "joypad");

        assert_eq!(config.nes.controller_port1, ControllerType::Arkanoid);
        assert_eq!(config.nes.controller_port2, ControllerType::Joypad);
        assert!(config.nes.controller_port1_explicit);
        assert!(config.nes.controller_port2_explicit);
    }

    #[test]
    fn test_config_file_controller_port_invalid_value_errors() {
        let mut config = Config::default();
        let result = config.apply_config_value("nes-controller_port1", "unknown");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Invalid value 'unknown' for 'nes_controller_port1'. Valid options are: joypad, snes-controller, snes-mouse, zapper, arkanoid, power-pad"
        );
    }

    #[test]
    fn test_config_controller_port1_flag_zapper() {
        let args = vec![
            "neser".to_string(),
            "--nes-controller-port1=zapper".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.controller_port1, ControllerType::Zapper);
    }

    #[test]
    fn test_config_controller_port1_flag_arkanoid() {
        let args = vec![
            "neser".to_string(),
            "--nes-controller-port1=arkanoid".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.controller_port1, ControllerType::Arkanoid);
    }

    #[test]
    fn test_config_controller_port1_flag_snes_controller() {
        let args = vec![
            "neser".to_string(),
            "--nes-controller-port1=snes-controller".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.controller_port1, ControllerType::SnesController);
        assert!(config.nes.controller_port1_explicit);
    }

    #[test]
    fn test_config_controller_port1_flag_snes_mouse() {
        let args = vec![
            "neser".to_string(),
            "--nes-controller-port1=snes-mouse".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.controller_port1, ControllerType::SnesMouse);
        assert!(config.nes.controller_port1_explicit);
    }

    #[test]
    fn test_config_controller_port2_flag_zapper() {
        let args = vec![
            "neser".to_string(),
            "--nes-controller-port2=zapper".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.controller_port2, ControllerType::Zapper);
    }

    #[test]
    fn test_config_controller_port1_flag_power_pad() {
        let args = vec![
            "neser".to_string(),
            "--nes-controller-port1=power-pad".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.controller_port1, ControllerType::PowerPad);
        assert!(config.nes.controller_port1_explicit);
    }

    #[test]
    fn test_config_controller_port_cli_overrides_config_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = "nes-controller_port1=zapper\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_string_lossy().to_string(),
            "--nes-controller-port1=joypad".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.controller_port1, ControllerType::Joypad);
    }

    #[test]
    fn test_config_controller_port_flag_invalid_value_errors() {
        let args = vec![
            "neser".to_string(),
            "--nes-controller-port1=unknown".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Invalid value 'unknown' for '--nes-controller-port1'. Valid options are: joypad, snes-controller, snes-mouse, zapper, arkanoid, power-pad"
        );
    }

    #[test]
    fn test_config_controller_port_invalid_cli_value_does_not_override_config_file_and_errors() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = "nes-controller_port1=zapper\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_string_lossy().to_string(),
            "--nes-controller-port1=unknown".to_string(),
        ];
        let result = Config::new(&args);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Invalid value 'unknown' for '--nes-controller-port1'. Valid options are: joypad, snes-controller, snes-mouse, zapper, arkanoid, power-pad"
        );
    }

    #[test]
    fn test_config_controller_port_flag_snes_adapter_is_rejected() {
        let args = vec![
            "neser".to_string(),
            "--nes-controller-port1=snes-adapter".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Invalid value 'snes-adapter' for '--nes-controller-port1'. Valid options are: joypad, snes-controller, snes-mouse, zapper, arkanoid, power-pad"
        );
    }

    #[test]
    fn test_config_controller_port_flags_two_mouse_controllers_error() {
        let args = vec![
            "neser".to_string(),
            "--nes-controller-port1=zapper".to_string(),
            "--nes-controller-port2=arkanoid".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_zapper_detection_size() {
        let mut config = Config::default();
        let _ = config.apply_config_value("nes-zapper_detection_size", "2");
        assert_eq!(config.nes.zapper_detection_size, 2);
    }

    #[test]
    fn test_config_zapper_detection_size_invalid() {
        let mut config = Config::default();
        let _ = config.apply_config_value("nes-zapper_detection_size", "invalid");
        assert_eq!(config.nes.zapper_detection_size, 0); // Should remain default
    }

    #[test]
    fn test_config_zapper_detection_size_from_file() {
        let config_content = "nes-zapper_detection_size=1\n";
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("neser.conf");
        std::fs::write(&config_path, config_content).unwrap();

        let mut config = Config::default();
        config.load_from_file(&config_path).unwrap();

        assert_eq!(config.nes.zapper_detection_size, 1);
    }

    #[test]
    fn test_config_file_trace_cpu() {
        let mut config = Config::default();
        config.apply_config_value("trace-cpu", "2").unwrap();
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.cpu, 2);
    }

    #[test]
    fn test_config_file_trace_ppu() {
        let mut config = Config::default();
        config.apply_config_value("trace-ppu", "3").unwrap();
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.ppu, 3);
    }

    #[test]
    fn test_config_file_trace_apu() {
        let mut config = Config::default();
        config.apply_config_value("trace-apu", "1").unwrap();
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.apu, 1);
    }

    #[test]
    fn test_config_file_trace_mapper() {
        let mut config = Config::default();
        config.apply_config_value("trace-mapper", "4").unwrap();
        assert!(config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.mapper, 4);
    }

    #[test]
    fn test_config_file_trace_nestest() {
        let mut config = Config::default();
        config.apply_config_value("trace-nestest", "true").unwrap();
        assert!(config.frontend.tracing.enabled);
        assert!(config.frontend.tracing.nestest);
    }

    #[test]
    fn test_config_file_gba_trace_channels() {
        let mut config = Config::default();

        config.apply_config_value("gba-trace-cpu", "1").unwrap();
        config.apply_config_value("gba-trace-bus", "2").unwrap();
        config.apply_config_value("gba-trace-dma", "3").unwrap();
        config.apply_config_value("gba-trace-swi", "4").unwrap();
        config
            .apply_config_value("gba-trace-mgba-log", "9")
            .unwrap();

        assert_eq!(config.gba.tracing.cpu, 1);
        assert_eq!(config.gba.tracing.bus, 2);
        assert_eq!(config.gba.tracing.dma, 3);
        assert_eq!(config.gba.tracing.swi, 4);
        assert_eq!(config.gba.tracing.mgba_log, 5);
    }

    #[test]
    fn test_config_file_trace_zero_does_not_enable() {
        let mut config = Config::default();
        config.apply_config_value("trace-cpu", "0").unwrap();
        assert!(!config.frontend.tracing.enabled);
        assert_eq!(config.frontend.tracing.cpu, 0);
    }

    #[test]
    fn test_config_file_bool_formats() {
        let mut config = Config::default();

        // Test "yes"/"no"
        config.apply_config_value("audio", "no").unwrap();
        assert!(!config.frontend.audio_enabled);
        config.apply_config_value("audio", "yes").unwrap();
        assert!(config.frontend.audio_enabled);

        // Test "1"/"0"
        config.apply_config_value("audio", "0").unwrap();
        assert!(!config.frontend.audio_enabled);
        config.apply_config_value("audio", "1").unwrap();
        assert!(config.frontend.audio_enabled);
    }

    #[test]
    fn test_config_file_unknown_key_ignored() {
        let mut config = Config::default();
        // Should not panic
        config
            .apply_config_value("unknown_key", "some_value")
            .unwrap();
        // Config should remain unchanged
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
    }

    #[test]
    fn test_config_file_load_from_string_content() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = r#"
# Test config file
nes-hardware=nes-pal
audio=false
fullscreen=true
display=2
nes-filter=crt
nes-pulse1=false
"#;

        // Create a temporary file with config content
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let mut config = Config::default();
        config.load_from_file(file.path()).unwrap();

        assert_eq!(config.nes.hardware_model, HardwareModel::NesPal);
        assert!(!config.frontend.audio_enabled);
        assert!(config.frontend.fullscreen);
        assert_eq!(config.frontend.fullscreen_display, Some(2));
        assert_eq!(
            config.frontend.shader_path,
            Some("vendor/slang-shaders/crt/crt-lottes.slangp".to_string())
        );
        assert!(!config.nes.apu_channels.contains(ApuChannels::PULSE1));
        // Other values should remain default
        assert!(config.frontend.vsync_enabled);
        assert!(config.nes.apu_channels.contains(ApuChannels::PULSE2));
    }

    #[test]
    fn test_config_file_accepts_dashes_as_underscores() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = "window-height=600\ntrace-cpu=2\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let mut config = Config::default();
        config.load_from_file(file.path()).unwrap();

        assert_eq!(config.frontend.window_height, 600);
        assert_eq!(config.frontend.tracing.cpu, 2);
        assert!(config.frontend.tracing.enabled);
    }

    #[test]
    fn test_config_args_override_config_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create config file that sets PAL and disables audio
        let content = r#"
    nes-hardware=nes-pal
audio=false
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        // Start with default config
        let mut config = Config::default();

        // Load from config file
        config.load_from_file(file.path()).unwrap();
        assert_eq!(config.nes.hardware_model, HardwareModel::NesPal);
        assert!(!config.frontend.audio_enabled);

        // Apply args - no args means config file values should remain
        let args = vec!["neser".to_string()];
        config.apply_args(&args).unwrap();

        // Config file values should persist since args don't override them
        assert_eq!(config.nes.hardware_model, HardwareModel::NesPal);
        assert!(!config.frontend.audio_enabled);
    }

    #[test]
    fn test_config_hardware_flag_overrides_config_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = "nes-hardware=nes-pal\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_string_lossy().to_string(),
            "--nes-hardware".to_string(),
            "nes-ntsc".to_string(),
        ];

        let config = parse_config(args);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
        assert!(config.nes.hardware_model_explicit);
    }

    #[test]
    fn test_config_file_two_arkanoid_controllers_errors() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = r#"
nes-controller_port1=arkanoid
nes-controller_port2=arkanoid
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_string_lossy().to_string(),
        ];

        let result = Config::new(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_file_nonexistent_silently_ignored() {
        let mut config = Config::default();
        config
            .load_from_file(Path::new("/nonexistent/path/neser.conf"))
            .unwrap();
        // Should not panic, config should remain default
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
        assert!(config.frontend.audio_enabled);
    }

    #[test]
    fn test_config_flag_loads_specified_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = "nes-hardware=nes-pal\naudio=false\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_str().unwrap().to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesPal);
        assert!(!config.frontend.audio_enabled);
    }

    #[test]
    fn test_config_file_invalid_filter_errors() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let content = r#"
    hardware=nes-pal
nes-filter=invalid-shader
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            file.path().to_str().unwrap().to_string(),
        ];
        let result = Config::new(&args);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Invalid filter name: 'invalid-shader'. Valid options are: none, crt, smooth, ntsc, pal"
        );
    }

    #[test]
    fn test_config_flag_invalid_file_errors() {
        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            "/nonexistent/path/config.conf".to_string(),
        ];
        let result = Config::new(&args);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("/nonexistent/path/config.conf"));
    }

    #[test]
    fn test_config_flag_missing_value_errors() {
        let args = vec!["neser".to_string(), "--config".to_string()];
        let result = Config::new(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_flag_overrides_default_locations() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create a config file with --config that sets PAL
        let content = "nes-hardware=nes-pal\n";
        let mut explicit_file = NamedTempFile::new().unwrap();
        explicit_file.write_all(content.as_bytes()).unwrap();

        // The --config file should be used
        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            explicit_file.path().to_str().unwrap().to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesPal);
    }

    #[test]
    fn test_parse_config_arg() {
        let args = vec![
            "neser".to_string(),
            "--config".to_string(),
            "my_config.conf".to_string(),
        ];
        let result = Config::parse_config_arg(&args);
        assert_eq!(result, Some("my_config.conf".to_string()));
    }

    #[test]
    fn test_parse_config_arg_not_present() {
        let args = vec!["neser".to_string()];
        let result = Config::parse_config_arg(&args);
        assert_eq!(result, None);
    }

    // Tests for aligned command line arguments

    #[test]
    fn test_config_audio_flag_true() {
        let args = vec![
            "neser".to_string(),
            "--audio".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.frontend.audio_enabled);
    }

    #[test]
    fn test_config_audio_flag_yes() {
        let args = vec![
            "neser".to_string(),
            "--audio".to_string(),
            "yes".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.frontend.audio_enabled);
    }

    #[test]
    fn test_config_vsync_flag_true() {
        let args = vec![
            "neser".to_string(),
            "--vsync".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.frontend.vsync_enabled);
    }

    #[test]
    fn test_config_gamepads_flag_true() {
        let args = vec![
            "neser".to_string(),
            "--gamepads".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.frontend.gamepads_enabled);
    }

    #[test]
    fn test_config_pulse1_flag_true() {
        let args = vec![
            "neser".to_string(),
            "--nes-pulse1".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.nes.apu_channels.contains(ApuChannels::PULSE1));
    }

    #[test]
    fn test_config_pulse2_flag_true() {
        let args = vec![
            "neser".to_string(),
            "--nes-pulse2".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.nes.apu_channels.contains(ApuChannels::PULSE2));
    }

    #[test]
    fn test_config_triangle_flag_true() {
        let args = vec![
            "neser".to_string(),
            "--nes-triangle".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.nes.apu_channels.contains(ApuChannels::TRIANGLE));
    }

    #[test]
    fn test_config_noise_flag_true() {
        let args = vec![
            "neser".to_string(),
            "--nes-noise".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.nes.apu_channels.contains(ApuChannels::NOISE));
    }

    #[test]
    fn test_config_dmc_flag_true() {
        let args = vec![
            "neser".to_string(),
            "--nes-dmc".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.nes.apu_channels.contains(ApuChannels::DMC));
    }

    #[test]
    fn test_config_debugger_flag_true() {
        let args = vec![
            "neser".to_string(),
            "--debugger".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.frontend.debugger_enabled);
    }

    #[test]
    fn test_config_hardware_flag_nes_pal() {
        let args = vec![
            "neser".to_string(),
            "--nes-hardware".to_string(),
            "nes-pal".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesPal);
    }

    #[test]
    fn test_config_hardware_flag_nes_ntsc() {
        let args = vec![
            "neser".to_string(),
            "--nes-hardware".to_string(),
            "nes-ntsc".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
    }

    #[test]
    fn test_config_hardware_flag_invalid_timing_value_errors() {
        let args = vec![
            "neser".to_string(),
            "--nes-hardware".to_string(),
            "pal".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_hardware_flag_famicom_is_accepted() {
        let args = vec![
            "neser".to_string(),
            "--nes-hardware".to_string(),
            "famicom".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_hardware_flag_nes_ntsc_is_accepted() {
        let args = vec![
            "neser".to_string(),
            "--nes-hardware".to_string(),
            "nes-ntsc".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_hardware_flag_invalid_value_errors() {
        let args = vec![
            "neser".to_string(),
            "--nes-hardware".to_string(),
            "pal".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_expansion_port_flag_famicom_four_players_is_accepted() {
        let args = vec![
            "neser".to_string(),
            "--nes-hardware".to_string(),
            "famicom".to_string(),
            "--nes-expansion-port".to_string(),
            "famicom-four-players".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_expansion_port_flag_arkanoid_is_accepted() {
        let args = vec![
            "neser".to_string(),
            "--nes-hardware".to_string(),
            "famicom".to_string(),
            "--nes-expansion-port".to_string(),
            "arkanoid".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.expansion_port, ExpansionPort::ArkanoidFamicom);
    }

    #[test]
    fn test_config_expansion_port_flag_power_pad_is_accepted() {
        let args = vec![
            "neser".to_string(),
            "--nes-hardware".to_string(),
            "famicom".to_string(),
            "--nes-expansion-port".to_string(),
            "power-pad".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.expansion_port, ExpansionPort::PowerPadFamicom);
    }

    #[test]
    fn test_config_expansion_port_flag_arkanoid_invalid_value_errors() {
        let args = vec![
            "neser".to_string(),
            "--nes-expansion-port".to_string(),
            "invalid-arkanoid".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_famicom_mode_rejects_controller_port_overrides() {
        let args = vec![
            "neser".to_string(),
            "--nes-hardware".to_string(),
            "famicom".to_string(),
            "--nes-controller-port1".to_string(),
            "zapper".to_string(),
            "--nes-controller-port2".to_string(),
            "arkanoid".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_file_tv_system_key_is_ignored() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-tv_system", "pal")
            .expect("legacy key should be ignored gracefully");
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
    }

    #[test]
    fn test_config_tv_system_flag_is_unknown() {
        let args = vec![
            "neser".to_string(),
            "--tv-system".to_string(),
            "pal".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_bool_flag_invalid_value_treated_as_positional() {
        // When an invalid (non-boolean) value follows a boolean flag, it's treated as a positional arg
        // and the flag defaults to true
        let args = vec![
            "neser".to_string(),
            "--audio".to_string(),
            "game.nes".to_string(),
        ];
        let config = parse_config(args);
        // Flag defaults to true, and "game.nes" is treated as ROM path
        assert!(config.frontend.audio_enabled);
        assert_eq!(config.frontend.rom_path.as_deref(), Some("game.nes"));
    }

    #[test]
    fn test_config_bool_flag_no_value_at_end() {
        // When a boolean flag is the last argument, it defaults to true
        let args = vec!["neser".to_string(), "--audio".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.audio_enabled);
    }

    #[test]
    fn test_config_file_load_state() {
        let mut config = Config::default();
        config.apply_config_value("load_state", "true").unwrap();
        assert!(config.frontend.load_state);
    }

    // Tests for negation flags (--no-*, --disable-*)

    #[test]
    fn test_config_no_audio_disables_audio() {
        let args = vec!["neser".to_string(), "--no-audio".to_string()];
        let config = parse_config(args);
        assert!(!config.frontend.audio_enabled);
    }

    #[test]
    fn test_config_disable_audio_disables_audio() {
        let args = vec!["neser".to_string(), "--disable-audio".to_string()];
        let config = parse_config(args);
        assert!(!config.frontend.audio_enabled);
    }

    #[test]
    fn test_config_no_vsync_disables_vsync() {
        let args = vec!["neser".to_string(), "--no-vsync".to_string()];
        let config = parse_config(args);
        assert!(!config.frontend.vsync_enabled);
    }

    #[test]
    fn test_config_disable_vsync_disables_vsync() {
        let args = vec!["neser".to_string(), "--disable-vsync".to_string()];
        let config = parse_config(args);
        assert!(!config.frontend.vsync_enabled);
    }

    #[test]
    fn test_config_no_gamepads_disables_gamepads() {
        let args = vec!["neser".to_string(), "--no-gamepads".to_string()];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_disable_gamepads_is_rejected() {
        let args = vec!["neser".to_string(), "--disable-gamepads".to_string()];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_enable_4_score_enables_four_score() {
        let args = vec!["neser".to_string(), "--nes-enable-4-score".to_string()];
        let config = parse_config(args);
        assert!(config.nes.four_score_enabled);
    }

    #[test]
    fn test_config_enable_4_score_with_value_false_disables_four_score() {
        let args = vec![
            "neser".to_string(),
            "--nes-enable-4-score".to_string(),
            "false".to_string(),
            "game.nes".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.nes.four_score_enabled);
        assert_eq!(config.frontend.rom_path.as_deref(), Some("game.nes"));
    }

    #[test]
    fn test_config_no_4_score_disables_four_score() {
        let args = vec!["neser".to_string(), "--no-nes-4-score".to_string()];
        let config = parse_config(args);
        assert!(!config.nes.four_score_enabled);
    }

    #[test]
    fn test_config_disable_4_score_disables_four_score() {
        let args = vec!["neser".to_string(), "--disable-nes-4-score".to_string()];
        let config = parse_config(args);
        assert!(!config.nes.four_score_enabled);
    }

    #[test]
    fn test_config_no_4_score_overrides_enable_4_score() {
        let args = vec![
            "neser".to_string(),
            "--nes-enable-4-score".to_string(),
            "--no-nes-4-score".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.nes.four_score_enabled);
    }

    #[test]
    fn test_config_disable_pulse1_removes_channel() {
        let args = vec!["neser".to_string(), "--disable-nes-pulse1".to_string()];
        let config = parse_config(args);
        assert!(!config.nes.apu_channels.contains(ApuChannels::PULSE1));
        assert!(config.nes.apu_channels.contains(ApuChannels::PULSE2));
    }

    #[test]
    fn test_config_no_pulse2_removes_channel() {
        let args = vec!["neser".to_string(), "--no-nes-pulse2".to_string()];
        let config = parse_config(args);
        assert!(config.nes.apu_channels.contains(ApuChannels::PULSE1));
        assert!(!config.nes.apu_channels.contains(ApuChannels::PULSE2));
    }

    #[test]
    fn test_config_disable_triangle_removes_channel() {
        let args = vec!["neser".to_string(), "--disable-nes-triangle".to_string()];
        let config = parse_config(args);
        assert!(!config.nes.apu_channels.contains(ApuChannels::TRIANGLE));
    }

    #[test]
    fn test_config_audio_value_equals_syntax() {
        let args = vec!["neser".to_string(), "--audio=0".to_string()];
        let config = parse_config(args);
        assert!(!config.frontend.audio_enabled);
    }

    #[test]
    fn test_config_audio_value_equals_syntax_true() {
        let args = vec!["neser".to_string(), "--audio=1".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.audio_enabled);
    }

    #[test]
    fn test_config_mixed_value_and_negation() {
        let args = vec![
            "neser".to_string(),
            "--audio".to_string(),
            "true".to_string(),
            "--no-vsync".to_string(),
            "--disable-nes-pulse1".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.frontend.audio_enabled);
        assert!(!config.frontend.vsync_enabled);
        assert!(!config.nes.apu_channels.contains(ApuChannels::PULSE1));
    }

    // Tests for valueless boolean flags (defaults to true)

    #[test]
    fn test_config_audio_no_value_defaults_true() {
        let args = vec!["neser".to_string(), "--audio".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.audio_enabled);
    }

    #[test]
    fn test_config_vsync_no_value_defaults_true() {
        let args = vec!["neser".to_string(), "--vsync".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.vsync_enabled);
    }

    #[test]
    fn test_config_debugger_no_value_defaults_true() {
        let args = vec!["neser".to_string(), "--debugger".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.debugger_enabled);
    }

    #[test]
    fn test_config_audio_no_value_with_rom() {
        let args = vec![
            "neser".to_string(),
            "--audio".to_string(),
            "game.nes".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.frontend.audio_enabled);
        assert_eq!(config.frontend.rom_path.as_deref(), Some("game.nes"));
    }

    #[test]
    fn test_config_fullscreen_no_value_defaults_true() {
        let args = vec!["neser".to_string(), "--fullscreen".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.fullscreen);
    }

    #[test]
    fn test_config_load_state_no_value_defaults_true() {
        let args = vec!["neser".to_string(), "--load-state".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.load_state);
    }

    #[test]
    fn test_config_no_load_state_disables_load_state() {
        let args = vec!["neser".to_string(), "--no-load-state".to_string()];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_disable_load_state_disables_load_state() {
        let args = vec!["neser".to_string(), "--disable-load-state".to_string()];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_load_state_false_disables() {
        let args = vec![
            "neser".to_string(),
            "--load-state".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.frontend.load_state);
    }

    #[test]
    fn test_config_load_state_equals_zero() {
        let args = vec!["neser".to_string(), "--load-state=0".to_string()];
        let config = parse_config(args);
        assert!(!config.frontend.load_state);
    }

    #[test]
    fn test_config_load_state_with_rom() {
        let args = vec![
            "neser".to_string(),
            "--load-state".to_string(),
            "game.nes".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.frontend.load_state);
        assert_eq!(config.frontend.rom_path.as_deref(), Some("game.nes"));
    }

    #[test]
    fn test_config_no_load_state_with_rom() {
        let args = vec![
            "neser".to_string(),
            "--no-load-state".to_string(),
            "game.nes".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_pulse1_no_value_defaults_true() {
        let args = vec!["neser".to_string(), "--nes-pulse1".to_string()];
        let config = parse_config(args);
        assert!(config.nes.apu_channels.contains(ApuChannels::PULSE1));
    }

    #[test]
    fn test_config_audio_with_another_flag() {
        let args = vec![
            "neser".to_string(),
            "--audio".to_string(),
            "--vsync".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.frontend.audio_enabled);
        assert!(config.frontend.vsync_enabled);
    }

    #[test]
    fn test_config_ram_init_mode_default() {
        let config = Config::with_defaults();
        #[cfg(target_arch = "wasm32")]
        assert_eq!(config.frontend.ram_init_mode, RamInitMode::Zero);
        #[cfg(not(target_arch = "wasm32"))]
        assert_eq!(config.frontend.ram_init_mode, RamInitMode::Random);
    }

    #[test]
    fn test_config_file_ram_init_mode_zero() {
        let mut config = Config::default();
        config.apply_config_value("ram_init_mode", "zero").unwrap();
        assert_eq!(config.frontend.ram_init_mode, RamInitMode::Zero);
    }

    #[test]
    fn test_config_file_ram_init_mode_random() {
        let mut config = Config::default();
        config
            .apply_config_value("ram_init_mode", "random")
            .unwrap();
        assert_eq!(config.frontend.ram_init_mode, RamInitMode::Random);
    }

    #[test]
    fn test_config_file_ram_init_mode_seeded_random() {
        let mut config = Config::default();
        config
            .apply_config_value("ram_init_mode", "seeded-random:42")
            .unwrap();
        assert_eq!(config.frontend.ram_init_mode, RamInitMode::SeededRandom(42));
    }

    #[test]
    fn test_config_file_ram_init_mode_seeded_random_underscore() {
        let mut config = Config::default();
        config
            .apply_config_value("ram_init_mode", "seeded_random:12345")
            .unwrap();
        assert_eq!(
            config.frontend.ram_init_mode,
            RamInitMode::SeededRandom(12345)
        );
    }

    #[test]
    fn test_config_cmdline_ram_init_mode_zero() {
        let args = vec![
            "neser".to_string(),
            "--ram-init-mode".to_string(),
            "zero".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.frontend.ram_init_mode, RamInitMode::Zero);
    }

    #[test]
    fn test_config_cmdline_ram_init_mode_seeded() {
        let args = vec![
            "neser".to_string(),
            "--ram-init-mode".to_string(),
            "seeded-random:999".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.ram_init_mode,
            RamInitMode::SeededRandom(999)
        );
    }

    #[test]
    fn test_config_oam_dram_decay_default_disabled() {
        let config = Config::with_defaults();
        assert!(!config.nes.oam_dram_decay_enabled);
    }

    #[test]
    fn test_config_file_oam_dram_decay_enabled() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-oam_dram_decay", "true")
            .unwrap();
        assert!(config.nes.oam_dram_decay_enabled);
    }

    #[test]
    fn test_config_cmdline_oam_dram_decay_enabled() {
        let args = vec![
            "neser".to_string(),
            "--nes-oam-dram-decay".to_string(),
            "true".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.nes.oam_dram_decay_enabled);
    }

    #[test]
    fn test_config_cmdline_oam_dram_decay_disabled() {
        let args = vec![
            "neser".to_string(),
            "--nes-oam-dram-decay".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.nes.oam_dram_decay_enabled);
    }

    #[test]
    fn test_config_cmdline_oam_dram_decay_with_rom() {
        let args = vec![
            "neser".to_string(),
            "--nes-oam-dram-decay".to_string(),
            "true".to_string(),
            "game.nes".to_string(),
        ];
        let config = parse_config(args);
        assert!(config.nes.oam_dram_decay_enabled);
        assert_eq!(config.frontend.rom_path.as_deref(), Some("game.nes"));
    }

    #[test]
    fn test_cli_breakpoint_pc_flag_adds_pc_breakpoint() {
        use crate::platform::debugging::breakpoints::BreakpointKind;
        let args = vec![
            "neser".to_string(),
            "--breakpoint".to_string(),
            "pc=C000".to_string(),
        ];
        let config = parse_config(args);
        assert!(
            config
                .frontend
                .breakpoints
                .contains(&BreakpointKind::Pc(0xC000)),
            "expected PC breakpoint at 0xC000"
        );
    }

    #[test]
    fn test_cli_breakpoint_cycle_flag_adds_cycle_breakpoint() {
        use crate::platform::debugging::breakpoints::BreakpointKind;
        let args = vec![
            "neser".to_string(),
            "--breakpoint".to_string(),
            "cycle=12345".to_string(),
        ];
        let config = parse_config(args);
        assert!(
            config
                .frontend
                .breakpoints
                .contains(&BreakpointKind::Cycle(12345)),
            "expected Cycle breakpoint at 12345"
        );
    }

    #[test]
    fn test_cli_frame_flag_adds_frame_breakpoint() {
        use crate::platform::debugging::breakpoints::BreakpointKind;
        let args = vec![
            "neser".to_string(),
            "--breakpoint".to_string(),
            "frame=42".to_string(),
        ];
        let config = parse_config(args);
        assert!(
            config
                .frontend
                .breakpoints
                .contains(&BreakpointKind::Frame(42)),
            "expected Frame breakpoint at frame 42"
        );
    }

    #[test]
    fn test_cli_breakpoint_frame_flag_adds_frame_breakpoint() {
        use crate::platform::debugging::breakpoints::BreakpointKind;
        let args = vec![
            "neser".to_string(),
            "--breakpoint".to_string(),
            "frame=42".to_string(),
        ];
        let config = parse_config(args);
        assert!(
            config
                .frontend
                .breakpoints
                .contains(&BreakpointKind::Frame(42)),
            "expected Frame breakpoint at frame 42"
        );
    }

    #[test]
    fn test_cli_breakpoint_write_flag_adds_write_breakpoint() {
        use crate::platform::debugging::breakpoints::BreakpointKind;
        let args = vec![
            "neser".to_string(),
            "--breakpoint".to_string(),
            "write=2006".to_string(),
        ];
        let config = parse_config(args);
        assert!(
            config
                .frontend
                .breakpoints
                .contains(&BreakpointKind::WriteAddress(0x2006)),
            "expected WriteAddress breakpoint at 0x2006"
        );
    }

    #[test]
    fn test_cli_multiple_breakpoints_are_all_added() {
        let args = vec![
            "neser".to_string(),
            "--breakpoint".to_string(),
            "pc=C000,write=2006".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.breakpoints.len(),
            2,
            "expected 2 breakpoints from comma-separated list"
        );
    }

    #[test]
    fn test_cli_invalid_breakpoint_kind_errors() {
        let args = vec![
            "neser".to_string(),
            "--breakpoint".to_string(),
            "cycfdsfd".to_string(),
        ];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "expected error for invalid breakpoint spec"
        );
        assert!(
            result.unwrap_err().contains("breakpoint"),
            "error message should mention 'breakpoint'"
        );
    }

    #[test]
    fn test_cli_invalid_breakpoint_pc_address_errors() {
        let args = vec![
            "neser".to_string(),
            "--breakpoint".to_string(),
            "pc=ZZZZ".to_string(),
        ];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "expected error for invalid PC address in breakpoint"
        );
    }

    #[test]
    fn test_cli_invalid_breakpoint_cycle_value_errors() {
        let args = vec![
            "neser".to_string(),
            "--breakpoint".to_string(),
            "cycle=notanumber".to_string(),
        ];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "expected error for invalid cycle value in breakpoint"
        );
    }

    // --- Issue #649: Config argument cleanup ---

    // --frame should no longer be a standalone flag
    #[test]
    fn test_cli_frame_standalone_flag_is_rejected() {
        let args = vec!["neser".to_string(), "--frame".to_string(), "42".to_string()];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--frame should be rejected; use --breakpoint frame=N instead"
        );
    }

    // --no-debugger and --disable-debugger should be removed
    #[test]
    fn test_cli_no_debugger_flag_is_rejected() {
        let args = vec!["neser".to_string(), "--no-debugger".to_string()];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--no-debugger should be rejected; use --debugger false instead"
        );
    }

    #[test]
    fn test_cli_disable_debugger_flag_is_rejected() {
        let args = vec!["neser".to_string(), "--disable-debugger".to_string()];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--disable-debugger should be rejected; use --debugger false instead"
        );
    }

    // Old autorunner flags should be rejected
    #[test]
    fn test_cli_record_standalone_flag_is_rejected() {
        let args = vec!["neser".to_string(), "--record".to_string()];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--record should be rejected; use --create-recording or --extend-recording instead"
        );
    }

    #[test]
    fn test_cli_extend_standalone_flag_is_rejected() {
        let args = vec!["neser".to_string(), "--extend".to_string()];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--extend should be rejected; use --extend-recording instead"
        );
    }

    #[test]
    fn test_cli_headless_standalone_flag_is_rejected() {
        let args = vec!["neser".to_string(), "--headless".to_string()];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--headless should be rejected; use --playback-headless instead"
        );
    }

    #[test]
    fn test_cli_overwrite_recording_standalone_flag_is_rejected() {
        let args = vec!["neser".to_string(), "--overwrite-recording".to_string()];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--overwrite-recording should be rejected; use --create-recording instead"
        );
    }

    // New autorunner flags should work
    #[test]
    fn test_cli_create_recording_sets_record_mode_and_overwrite() {
        let args = vec!["neser".to_string(), "--create-recording".to_string()];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.autorun_mode,
            AutorunMode::Record,
            "--create-recording should set Record mode"
        );
        assert!(
            config.frontend.autorun_overwrite,
            "--create-recording should set autorun_overwrite"
        );
        assert!(
            !config.frontend.autorun_extend,
            "--create-recording should not set autorun_extend"
        );
    }

    #[test]
    fn test_cli_extend_recording_sets_record_mode_and_extend() {
        let args = vec!["neser".to_string(), "--extend-recording".to_string()];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.autorun_mode,
            AutorunMode::Record,
            "--extend-recording should set Record mode"
        );
        assert!(
            config.frontend.autorun_extend,
            "--extend-recording should set autorun_extend"
        );
        assert!(
            !config.frontend.autorun_overwrite,
            "--extend-recording should not set autorun_overwrite"
        );
    }

    #[test]
    fn test_cli_playback_headless_sets_playback_mode_and_headless() {
        let args = vec!["neser".to_string(), "--playback-headless".to_string()];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.autorun_mode,
            AutorunMode::Playback,
            "--playback-headless should set Playback mode"
        );
        assert!(
            config.frontend.autorun_headless,
            "--playback-headless should set autorun_headless"
        );
    }

    #[test]
    fn test_cli_playback_still_works() {
        let args = vec!["neser".to_string(), "--playback".to_string()];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.autorun_mode,
            AutorunMode::Playback,
            "--playback should still set Playback mode"
        );
        assert!(
            !config.frontend.autorun_headless,
            "--playback alone should not set autorun_headless"
        );
    }

    #[test]
    fn test_cli_create_recording_and_playback_are_mutually_exclusive() {
        let args = vec![
            "neser".to_string(),
            "--create-recording".to_string(),
            "--playback".to_string(),
        ];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--create-recording and --playback should be mutually exclusive"
        );
    }

    #[test]
    fn test_cli_extend_recording_and_playback_are_mutually_exclusive() {
        let args = vec![
            "neser".to_string(),
            "--extend-recording".to_string(),
            "--playback".to_string(),
        ];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--extend-recording and --playback should be mutually exclusive"
        );
    }

    #[test]
    fn test_cli_create_recording_and_extend_recording_are_mutually_exclusive() {
        let args = vec![
            "neser".to_string(),
            "--create-recording".to_string(),
            "--extend-recording".to_string(),
        ];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--create-recording and --extend-recording should be mutually exclusive"
        );
    }

    #[test]
    fn test_cli_playback_from_checkpoint_sets_autorun_from_checkpoint() {
        let args = vec![
            "neser".to_string(),
            "--playback".to_string(),
            "game".to_string(),
            "--playback-from-checkpoint".to_string(),
            "3".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.frontend.autorun_from_checkpoint, Some(3));
    }

    #[test]
    fn test_cli_trim_checkpoints_sets_autorun_trim_checkpoints() {
        let args = vec![
            "neser".to_string(),
            "--trim-checkpoints".to_string(),
            "2".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.frontend.autorun_trim_checkpoints, Some(2));
    }

    #[test]
    fn test_cli_playback_from_checkpoint_equals_syntax() {
        let args = vec![
            "neser".to_string(),
            "--playback".to_string(),
            "game".to_string(),
            "--playback-from-checkpoint=4".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.autorun_from_checkpoint,
            Some(4),
            "--playback-from-checkpoint=N (equals syntax) should be parsed"
        );
    }

    #[test]
    fn test_cli_trim_checkpoints_equals_syntax() {
        let args = vec!["neser".to_string(), "--trim-checkpoints=3".to_string()];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.autorun_trim_checkpoints,
            Some(3),
            "--trim-checkpoints=N (equals syntax) should be parsed"
        );
    }

    #[test]
    fn test_cli_convert_autorun_sets_autorun_convert_true() {
        let args = vec!["neser".to_string(), "--convert-autorun".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.autorun_convert);
    }

    #[test]
    fn test_cli_convert_autorun_equals_syntax_true() {
        let args = vec!["neser".to_string(), "--convert-autorun=true".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.autorun_convert);
    }

    #[test]
    fn test_cli_convert_autorun_equals_syntax_false() {
        let args = vec!["neser".to_string(), "--convert-autorun=false".to_string()];
        let config = parse_config(args);
        assert!(!config.frontend.autorun_convert);
    }

    #[test]
    fn test_cli_recalculate_autorun_sets_autorun_recalculate_true() {
        let args = vec!["neser".to_string(), "--recalculate-autorun".to_string()];
        let config = parse_config(args);
        assert!(config.frontend.autorun_recalculate);
    }

    #[test]
    fn test_cli_recalculate_autorun_equals_syntax_false() {
        let args = vec![
            "neser".to_string(),
            "--recalculate-autorun=false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.frontend.autorun_recalculate);
    }

    #[test]
    fn test_cli_trim_checkpoints_and_convert_autorun_are_mutually_exclusive() {
        let args = vec![
            "neser".to_string(),
            "--trim-checkpoints".to_string(),
            "1".to_string(),
            "--convert-autorun".to_string(),
        ];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--trim-checkpoints and --convert-autorun should be mutually exclusive"
        );
    }

    #[test]
    fn test_cli_recalculate_autorun_and_convert_autorun_are_mutually_exclusive() {
        let args = vec![
            "neser".to_string(),
            "--recalculate-autorun".to_string(),
            "--convert-autorun".to_string(),
        ];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--recalculate-autorun and --convert-autorun should be mutually exclusive"
        );
    }

    #[test]
    fn test_cli_recalculate_autorun_and_playback_are_mutually_exclusive() {
        let args = vec![
            "neser".to_string(),
            "--recalculate-autorun".to_string(),
            "--playback".to_string(),
        ];
        let result = config_new(args);
        assert!(
            result.is_err(),
            "--recalculate-autorun and --playback should be mutually exclusive"
        );
    }

    #[test]
    fn test_cli_playback_from_checkpoint_implies_playback_mode() {
        // --playback-from-checkpoint alone (without --playback) should enable Playback mode
        let args = vec![
            "neser".to_string(),
            "--playback-from-checkpoint".to_string(),
            "4".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.autorun_mode,
            AutorunMode::Playback,
            "--playback-from-checkpoint should imply Playback mode"
        );
        assert_eq!(config.frontend.autorun_from_checkpoint, Some(4));
    }

    #[test]
    fn test_cli_playback_from_checkpoint_negative_value() {
        // Negative value -1 means second-to-last checkpoint
        let args = vec![
            "neser".to_string(),
            "--playback-from-checkpoint".to_string(),
            "-1".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.autorun_from_checkpoint,
            Some(-1),
            "--playback-from-checkpoint=-1 should parse as -1"
        );
        assert_eq!(config.frontend.autorun_mode, AutorunMode::Playback);
    }

    #[test]
    fn test_cli_playback_from_checkpoint_negative_equals_syntax() {
        let args = vec![
            "neser".to_string(),
            "--playback-from-checkpoint=-2".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.frontend.autorun_from_checkpoint, Some(-2));
    }

    #[test]
    fn test_cli_playback_headless_from_checkpoint_sets_playback_headless_and_checkpoint() {
        let args = vec![
            "neser".to_string(),
            "--playback-headless-from-checkpoint".to_string(),
            "3".to_string(),
            "game".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.autorun_mode,
            AutorunMode::Playback,
            "--playback-headless-from-checkpoint should set Playback mode"
        );
        assert!(
            config.frontend.autorun_headless,
            "--playback-headless-from-checkpoint should set headless mode"
        );
        assert_eq!(
            config.frontend.autorun_from_checkpoint,
            Some(3),
            "--playback-headless-from-checkpoint should set checkpoint index"
        );
    }

    #[test]
    fn test_cli_playback_headless_from_checkpoint_equals_syntax() {
        let args = vec![
            "neser".to_string(),
            "--playback-headless-from-checkpoint=5".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.frontend.autorun_mode, AutorunMode::Playback);
        assert!(config.frontend.autorun_headless);
        assert_eq!(config.frontend.autorun_from_checkpoint, Some(5));
    }

    #[test]
    fn test_cli_playback_headless_from_checkpoint_negative_value() {
        let args = vec![
            "neser".to_string(),
            "--playback-headless-from-checkpoint=-1".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.frontend.autorun_mode, AutorunMode::Playback);
        assert!(config.frontend.autorun_headless);
        assert_eq!(config.frontend.autorun_from_checkpoint, Some(-1));
    }

    #[test]
    fn test_cli_create_recording_forces_zero_ram_init_mode() {
        let args = vec!["neser".to_string(), "--create-recording".to_string()];
        let config = parse_config(args);
        assert_eq!(config.frontend.autorun_mode, AutorunMode::Record);
        assert_eq!(config.frontend.ram_init_mode, RamInitMode::Zero);
    }

    #[test]
    fn test_cli_playback_forces_zero_ram_init_mode() {
        let args = vec!["neser".to_string(), "--playback".to_string()];
        let config = parse_config(args);
        assert_eq!(config.frontend.autorun_mode, AutorunMode::Playback);
        assert_eq!(config.frontend.ram_init_mode, RamInitMode::Zero);
    }

    #[test]
    fn test_cli_playback_from_checkpoint_forces_zero_ram_init_mode() {
        let args = vec![
            "neser".to_string(),
            "--playback-from-checkpoint=1".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.frontend.autorun_mode, AutorunMode::Playback);
        assert_eq!(config.frontend.ram_init_mode, RamInitMode::Zero);
    }

    #[test]
    fn test_cli_create_recording_rejects_non_zero_ram_init_mode() {
        let args = vec![
            "neser".to_string(),
            "--create-recording".to_string(),
            "--ram-init-mode".to_string(),
            "random".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
        assert!(
            result
                .err()
                .unwrap()
                .contains("Autorun recording/playback requires --ram-init-mode zero")
        );
    }

    #[test]
    fn test_cli_playback_rejects_non_zero_ram_init_mode() {
        let args = vec![
            "neser".to_string(),
            "--playback".to_string(),
            "--ram-init-mode=seeded-random:42".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
        assert!(
            result
                .err()
                .unwrap()
                .contains("Autorun recording/playback requires --ram-init-mode zero")
        );
    }

    #[test]
    fn given_cartridge_search_paths_cli_when_parsed_then_paths_are_configured() {
        let args = vec![
            "neser".to_string(),
            "--cartridge-search-paths".to_string(),
            "roms,games/custom".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.cartridge_search_paths,
            vec!["roms".to_string(), "games/custom".to_string()]
        );
    }

    #[test]
    fn given_no_cartridge_paths_when_using_defaults_then_paths_are_empty() {
        let config = Config::with_defaults();
        assert!(config.frontend.cartridge_search_paths.is_empty());
    }

    #[test]
    fn given_no_scan_cartridges_cli_when_parsed_then_startup_scan_is_disabled() {
        let args = vec!["neser".to_string(), "--no-scan-cartridges".to_string()];
        let config = parse_config(args);
        assert!(!config.frontend.scan_cartridges);
    }

    #[test]
    fn given_rebuild_catalog_config_key_when_loaded_then_rebuild_is_enabled() {
        let mut config = Config::default();
        config
            .apply_config_value("rebuild_cartridge_catalog", "true")
            .unwrap();
        assert!(config.frontend.rebuild_cartridge_catalog);
    }

    #[test]
    fn test_config_apply_rom_db_zapper_famicom_hint_sets_expansion_when_already_famicom() {
        let mut config = Config {
            nes: NesConfig {
                hardware_mode: HardwareMode::Famicom,
                ..Default::default()
            },
            ..Default::default()
        };

        let changed = config.apply_rom_db_zapper_famicom_hint(true);

        assert!(changed);
        assert_eq!(config.nes.expansion_port, ExpansionPort::ZapperFamicom);
    }

    #[test]
    fn test_config_apply_rom_db_zapper_famicom_hint_no_change_when_nes_mode() {
        let mut config = Config::default(); // Default is NES mode

        let changed = config.apply_rom_db_zapper_famicom_hint(true);

        assert!(!changed);
        assert_eq!(config.nes.expansion_port, ExpansionPort::None);
    }

    #[test]
    fn test_config_apply_rom_db_zapper_famicom_hint_respects_explicit_expansion_override() {
        let mut config = Config {
            nes: NesConfig {
                hardware_mode: HardwareMode::Famicom,
                expansion_port: ExpansionPort::None,
                expansion_port_explicit: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let changed = config.apply_rom_db_zapper_famicom_hint(true);

        assert!(!changed);
        assert_eq!(config.nes.expansion_port, ExpansionPort::None);
    }

    #[test]
    fn test_config_apply_rom_db_zapper_famicom_hint_false_is_noop() {
        let mut config = Config {
            nes: NesConfig {
                hardware_mode: HardwareMode::Famicom,
                ..Default::default()
            },
            ..Default::default()
        };

        let changed = config.apply_rom_db_zapper_famicom_hint(false);

        assert!(!changed);
        assert_eq!(config.nes.expansion_port, ExpansionPort::None);
    }

    // --- Power Pad Famicom hint tests ---

    #[test]
    fn test_config_apply_rom_db_power_pad_famicom_hint_sets_expansion_when_already_famicom() {
        let mut config = Config {
            nes: NesConfig {
                hardware_mode: HardwareMode::Famicom,
                ..Default::default()
            },
            ..Default::default()
        };

        let changed = config.apply_rom_db_power_pad_famicom_hint(true);

        assert!(changed);
        assert_eq!(config.nes.expansion_port, ExpansionPort::PowerPadFamicom);
    }

    #[test]
    fn test_config_apply_rom_db_power_pad_famicom_hint_no_change_when_nes_mode() {
        let mut config = Config::default();
        assert_eq!(config.nes.hardware_mode, HardwareMode::Nes);

        let changed = config.apply_rom_db_power_pad_famicom_hint(true);

        assert!(!changed);
        assert_eq!(config.nes.expansion_port, ExpansionPort::None);
    }

    #[test]
    fn test_config_apply_rom_db_power_pad_famicom_hint_respects_explicit_expansion_override() {
        let mut config = Config {
            nes: NesConfig {
                hardware_mode: HardwareMode::Famicom,
                expansion_port: ExpansionPort::FamicomFourPlayers,
                expansion_port_explicit: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let changed = config.apply_rom_db_power_pad_famicom_hint(true);

        assert!(!changed);
        assert_eq!(config.nes.expansion_port, ExpansionPort::FamicomFourPlayers);
    }

    #[test]
    fn test_config_apply_rom_db_power_pad_famicom_hint_false_is_noop() {
        let mut config = Config {
            nes: NesConfig {
                hardware_mode: HardwareMode::Famicom,
                ..Default::default()
            },
            ..Default::default()
        };

        let changed = config.apply_rom_db_power_pad_famicom_hint(false);

        assert!(!changed);
        assert_eq!(config.nes.expansion_port, ExpansionPort::None);
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
    fn test_autorun_format_default_is_binary() {
        let config = Config::default();
        assert_eq!(config.frontend.autorun_format, AutorunFormat::Binary);
    }

    #[test]
    fn test_autorun_format_cli_binary() {
        let args = vec![
            "neser".to_string(),
            "--autorun-format".to_string(),
            "binary".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.frontend.autorun_format, AutorunFormat::Binary);
    }

    #[test]
    fn test_autorun_format_cli_json() {
        let args = vec![
            "neser".to_string(),
            "--autorun-format".to_string(),
            "json".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.frontend.autorun_format, AutorunFormat::Json);
    }

    #[test]
    fn test_autorun_format_cli_unknown_value_returns_error() {
        let args = vec![
            "neser".to_string(),
            "--autorun-format".to_string(),
            "xml".to_string(),
        ];
        let result = Config::new(&args);
        assert!(result.is_err(), "unknown format should return an error");
        assert!(
            result.unwrap_err().contains("xml"),
            "error should mention the unknown format"
        );
    }

    #[test]
    fn test_tui_mode_default_is_false() {
        let config = Config::with_defaults();
        assert!(
            !config.frontend.tui_mode,
            "tui_mode should default to false"
        );
    }

    #[cfg(feature = "tui")]
    #[test]
    fn test_tui_flag_sets_tui_mode_true() {
        let config = parse_config(vec!["neser".to_string(), "--tui".to_string()]);
        assert!(
            config.frontend.tui_mode,
            "--tui flag should set tui_mode to true"
        );
    }

    #[cfg(not(feature = "tui"))]
    #[test]
    fn test_tui_flag_errors_without_tui_feature() {
        let result = Config::new(&["neser".to_string(), "--tui".to_string()]);
        assert!(
            result.is_err(),
            "--tui should return an error when tui feature is not enabled"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("tui"),
            "error message should mention 'tui': {err}"
        );
    }

    #[test]
    fn test_no_tui_flag_leaves_tui_mode_false() {
        let config = parse_config(vec!["neser".to_string()]);
        assert!(
            !config.frontend.tui_mode,
            "tui_mode should remain false without --tui"
        );
    }

    // ── VS System config tests ───────────────────────────────────────────────

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
    fn test_config_vs_dip_switches_default() {
        let config = Config::default();
        assert_eq!(config.nes.vs_dip_switches, 0x00);
    }

    #[test]
    fn test_config_vs_dip_switches_hex_parse() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-vs_dip_switches", "0xFF")
            .unwrap();
        assert_eq!(config.nes.vs_dip_switches, 0xFF);
    }

    #[test]
    fn test_config_vs_dip_switches_decimal_parse() {
        let mut config = Config::default();
        config
            .apply_config_value("nes-vs_dip_switches", "42")
            .unwrap();
        assert_eq!(config.nes.vs_dip_switches, 42);
    }

    #[test]
    fn test_config_vs_dip_switches_invalid_parse() {
        let mut config = Config::default();
        assert!(
            config
                .apply_config_value("nes-vs_dip_switches", "xyz")
                .is_err()
        );
    }

    #[test]
    fn test_config_apply_rom_db_vs_system_hint_sets_expansion_port() {
        let mut config = Config::default();

        let changed = config.apply_rom_db_vs_system_hint(true);

        assert!(changed);
        assert_eq!(config.nes.expansion_port, ExpansionPort::VsSystem);
    }

    #[test]
    fn test_config_apply_rom_db_vs_system_hint_respects_explicit_expansion() {
        let mut config = Config {
            nes: NesConfig {
                expansion_port: ExpansionPort::None,
                expansion_port_explicit: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let changed = config.apply_rom_db_vs_system_hint(true);

        assert!(!changed);
        assert_eq!(config.nes.expansion_port, ExpansionPort::None);
    }

    #[test]
    fn test_config_apply_rom_db_vs_system_hint_false_is_noop() {
        let mut config = Config::default();

        let changed = config.apply_rom_db_vs_system_hint(false);

        assert!(!changed);
        assert_eq!(config.nes.expansion_port, ExpansionPort::None);
    }

    #[test]
    fn test_config_apply_rom_db_playchoice10_hint_sets_expansion_port() {
        let mut config = Config::default();

        let changed = config.apply_rom_db_playchoice10_hint(true);

        assert!(changed);
        assert_eq!(config.nes.expansion_port, ExpansionPort::Playchoice10);
    }

    #[test]
    fn test_config_apply_rom_db_playchoice10_hint_respects_explicit_expansion() {
        let mut config = Config {
            nes: NesConfig {
                expansion_port: ExpansionPort::None,
                expansion_port_explicit: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let changed = config.apply_rom_db_playchoice10_hint(true);

        assert!(!changed);
        assert_eq!(config.nes.expansion_port, ExpansionPort::None);
    }

    #[test]
    fn test_config_apply_rom_db_playchoice10_hint_false_is_noop() {
        let mut config = Config::default();

        let changed = config.apply_rom_db_playchoice10_hint(false);

        assert!(!changed);
        assert_eq!(config.nes.expansion_port, ExpansionPort::None);
    }

    #[test]
    fn test_config_vs_system_not_famicom_only() {
        assert!(
            !ExpansionPort::VsSystem.is_famicom_only(),
            "VS System should not be classified as Famicom-only"
        );
    }

    // --- hardware_summary tests ---

    #[test]
    fn test_hardware_summary_default_nes_ntsc() {
        let config = Config::default();
        let summary = config.hardware_summary();
        assert!(
            summary.contains("NES"),
            "Summary should mention NES hardware mode: {summary}"
        );
        assert!(
            summary.contains("NTSC"),
            "Summary should mention NTSC timing: {summary}"
        );
        assert!(
            summary.contains("Joypad"),
            "Summary should mention Joypad controllers: {summary}"
        );
    }

    #[test]
    fn test_hardware_summary_famicom_with_power_pad_expansion() {
        let config = Config {
            nes: NesConfig {
                hardware_mode: HardwareMode::Famicom,
                expansion_port: ExpansionPort::PowerPadFamicom,
                ..Default::default()
            },
            ..Config::default()
        };
        let summary = config.hardware_summary();
        assert!(
            summary.contains("Famicom"),
            "Summary should mention Famicom hardware mode: {summary}"
        );
        assert!(
            summary.contains("Power Pad"),
            "Summary should mention Power Pad expansion: {summary}"
        );
    }

    #[test]
    fn test_hardware_summary_nes_pal_with_zapper() {
        let config = Config {
            nes: NesConfig {
                hardware_model: HardwareModel::NesPal,
                controller_port2: crate::nes::input::ControllerType::Zapper,
                ..Default::default()
            },
            ..Config::default()
        };
        let summary = config.hardware_summary();
        assert!(
            summary.contains("PAL"),
            "Summary should mention PAL timing: {summary}"
        );
        assert!(
            summary.contains("Zapper"),
            "Summary should mention Zapper on port 2: {summary}"
        );
    }

    #[test]
    fn test_hardware_summary_power_pad_on_port2() {
        let config = Config {
            nes: NesConfig {
                controller_port2: crate::nes::input::ControllerType::PowerPad,
                ..Default::default()
            },
            ..Config::default()
        };
        let summary = config.hardware_summary();
        assert!(
            summary.contains("Power Pad"),
            "Summary should mention Power Pad on port 2: {summary}"
        );
    }

    #[test]
    fn test_hardware_summary_no_expansion_omits_expansion_line() {
        let config = Config::default();
        let summary = config.hardware_summary();
        assert!(
            !summary.contains("Expansion"),
            "Summary should not mention expansion when None: {summary}"
        );
    }

    #[test]
    fn test_hardware_summary_includes_four_score_when_enabled() {
        let config = Config {
            nes: NesConfig {
                four_score_enabled: true,
                ..Default::default()
            },
            ..Config::default()
        };
        let summary = config.hardware_summary();
        assert!(
            summary.contains("Four Score"),
            "Summary should mention Four Score when enabled: {summary}"
        );
        assert!(
            summary.contains("Port 3"),
            "Summary should show Port 3 when Four Score enabled: {summary}"
        );
        assert!(
            summary.contains("Port 4"),
            "Summary should show Port 4 when Four Score enabled: {summary}"
        );
    }

    #[test]
    fn test_hardware_summary_omits_ports_3_4_when_four_score_disabled() {
        let config = Config::default();
        let summary = config.hardware_summary();
        assert!(
            !summary.contains("Port 3"),
            "Summary should not show Port 3 when Four Score disabled: {summary}"
        );
        assert!(
            !summary.contains("Port 4"),
            "Summary should not show Port 4 when Four Score disabled: {summary}"
        );
    }

    #[test]
    fn test_hardware_summary_omits_four_score_when_disabled() {
        let config = Config::default();
        let summary = config.hardware_summary();
        assert!(
            !summary.contains("Four Score"),
            "Summary should not mention Four Score when disabled: {summary}"
        );
    }

    #[test]
    fn test_hardware_summary_with_overrides_controller_types() {
        let config = Config::default(); // port1=Joypad, port2=Joypad
        let summary = config.hardware_summary_with(
            crate::nes::input::ControllerType::Joypad,
            crate::nes::input::ControllerType::PowerPad,
        );
        assert!(
            summary.contains("Power Pad"),
            "hardware_summary_with should use the supplied port2 type: {summary}"
        );
        // The config itself should be unchanged.
        assert_eq!(
            config.nes.controller_port2,
            crate::nes::input::ControllerType::Joypad,
            "hardware_summary_with must not mutate config.nes.controller_port2"
        );
    }

    // --- apply_rom_db_nes_four_score_hint ---

    #[test]
    fn test_config_apply_rom_db_nes_four_score_hint_enables_four_score_when_hint_true() {
        // Given: default config with four_score_enabled = false
        let mut config = Config::default();
        assert!(!config.nes.four_score_enabled);

        // When: hint is true (ROM DB says this ROM uses NES Four Score)
        let changed = config.apply_rom_db_nes_four_score_hint(true);

        // Then: four_score_enabled is set to true and changed is true
        assert!(config.nes.four_score_enabled);
        assert!(changed);
    }

    #[test]
    fn test_config_apply_rom_db_nes_four_score_hint_disables_four_score_when_hint_false() {
        // Given: config where four_score was previously auto-enabled
        let mut config = Config {
            nes: NesConfig {
                four_score_enabled: true,
                ..Default::default()
            },
            ..Config::default()
        };

        // When: hint is false (new ROM has no Four Score entry)
        let changed = config.apply_rom_db_nes_four_score_hint(false);

        // Then: four_score_enabled is reset to false (ROM swap resets auto-detection)
        assert!(!config.nes.four_score_enabled);
        assert!(changed);
    }

    #[test]
    fn test_config_apply_rom_db_nes_four_score_hint_no_change_when_already_enabled_and_hint_true() {
        // Given: config already has four_score_enabled = true (not via explicit)
        let mut config = Config {
            nes: NesConfig {
                four_score_enabled: true,
                ..Default::default()
            },
            ..Config::default()
        };

        // When: hint is true
        let changed = config.apply_rom_db_nes_four_score_hint(true);

        // Then: still enabled, but changed is false (no mutation needed)
        assert!(config.nes.four_score_enabled);
        assert!(!changed);
    }

    #[test]
    fn test_config_apply_rom_db_nes_four_score_hint_respects_explicit_true_when_hint_false() {
        // Given: user explicitly enabled four_score
        let mut config = Config {
            nes: NesConfig {
                four_score_enabled: true,
                four_score_enabled_explicit: true,
                ..Default::default()
            },
            ..Config::default()
        };

        // When: hint is false (ROM has no Four Score entry)
        let changed = config.apply_rom_db_nes_four_score_hint(false);

        // Then: explicit config wins — four_score stays enabled, no change
        assert!(config.nes.four_score_enabled);
        assert!(!changed);
    }

    #[test]
    fn test_config_apply_rom_db_nes_four_score_hint_respects_explicit_false_when_hint_true() {
        // Given: user explicitly disabled four_score
        let mut config = Config {
            nes: NesConfig {
                four_score_enabled: false,
                four_score_enabled_explicit: true,
                ..Default::default()
            },
            ..Config::default()
        };

        // When: hint is true (ROM DB says Four Score)
        let changed = config.apply_rom_db_nes_four_score_hint(true);

        // Then: explicit config wins — four_score stays disabled, no change
        assert!(!config.nes.four_score_enabled);
        assert!(!changed);
    }

    // === Tests for issue #2074: separated config ===

    #[test]
    fn test_nes_hardware_arg_uses_nes_prefix() {
        let args = vec![
            "neser".to_string(),
            "--nes-hardware".to_string(),
            "famicom".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Famicom);
        assert!(config.nes.hardware_mode_explicit);
    }

    #[test]
    fn test_nes_hardware_playchoice10_arg_forces_playchoice_expansion() {
        let args = vec![
            "neser".to_string(),
            "--nes-hardware".to_string(),
            "playchoice10".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.hardware_mode, HardwareMode::Nes);
        assert_eq!(config.nes.hardware_model, HardwareModel::NesNtsc);
        assert_eq!(config.nes.expansion_port, ExpansionPort::Playchoice10);
        assert!(config.nes.expansion_port_explicit);
    }

    #[test]
    fn test_nes_pulse1_arg_uses_nes_prefix() {
        let args = vec!["neser".to_string(), "--no-nes-pulse1".to_string()];
        let config = parse_config(args);
        assert!(!config.nes.apu_channels.contains(ApuChannels::PULSE1));
    }

    #[test]
    fn test_nes_expansion_port_arg_uses_nes_prefix() {
        let args = vec![
            "neser".to_string(),
            "--nes-hardware".to_string(),
            "famicom".to_string(),
            "--nes-expansion-port".to_string(),
            "arkanoid".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.expansion_port, ExpansionPort::ArkanoidFamicom);
        assert!(config.nes.expansion_port_explicit);
    }

    #[test]
    fn test_nes_controller_port1_arg_uses_nes_prefix() {
        let args = vec![
            "neser".to_string(),
            "--nes-controller-port1".to_string(),
            "zapper".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.controller_port1, ControllerType::Zapper);
        assert!(config.nes.controller_port1_explicit);
    }

    #[test]
    fn test_nes_horizontal_overscan_arg_uses_nes_prefix() {
        let args = vec![
            "neser".to_string(),
            "--nes-horizontal-overscan".to_string(),
            "4".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.horizontal_overscan, 4);
    }

    #[test]
    fn test_nes_vertical_overscan_arg_uses_nes_prefix() {
        let args = vec![
            "neser".to_string(),
            "--nes-vertical-overscan".to_string(),
            "12".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.vertical_overscan, 12);
    }

    #[test]
    fn test_nes_oam_dram_decay_arg_uses_nes_prefix() {
        let args = vec!["neser".to_string(), "--nes-oam-dram-decay".to_string()];
        let config = parse_config(args);
        assert!(config.nes.oam_dram_decay_enabled);
    }

    #[test]
    fn test_nes_enable_4_score_arg_uses_nes_prefix() {
        let args = vec!["neser".to_string(), "--nes-enable-4-score".to_string()];
        let config = parse_config(args);
        assert!(config.nes.four_score_enabled);
        assert!(config.nes.four_score_enabled_explicit);
    }

    #[test]
    fn test_nes_zapper_detection_size_arg_uses_nes_prefix() {
        let args = vec![
            "neser".to_string(),
            "--nes-zapper-detection-size".to_string(),
            "5".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.nes.zapper_detection_size, 5);
    }

    #[test]
    fn test_config_file_nes_hardware_key_uses_prefix() {
        let mut config = Config::with_defaults();
        config
            .apply_config_value("nes-hardware", "famicom")
            .unwrap();
        assert_eq!(config.nes.hardware_mode, HardwareMode::Famicom);
        assert!(config.nes.hardware_mode_explicit);
    }

    #[test]
    fn test_config_file_nes_pulse1_key_uses_prefix() {
        let mut config = Config::with_defaults();
        config.apply_config_value("nes-pulse1", "false").unwrap();
        assert!(!config.nes.apu_channels.contains(ApuChannels::PULSE1));
    }

    #[test]
    fn test_config_file_nes_expansion_port_key_uses_prefix() {
        let mut config = Config::with_defaults();
        config
            .apply_config_value("nes-expansion_port", "zapper")
            .unwrap();
        assert_eq!(config.nes.expansion_port, ExpansionPort::ZapperFamicom);
        assert!(config.nes.expansion_port_explicit);
    }

    #[test]
    fn test_config_file_nes_oam_dram_decay_key_uses_prefix() {
        let mut config = Config::with_defaults();
        config
            .apply_config_value("nes-oam_dram_decay", "true")
            .unwrap();
        assert!(config.nes.oam_dram_decay_enabled);
    }

    #[test]
    fn test_config_file_nes_horizontal_overscan_key_uses_prefix() {
        let mut config = Config::with_defaults();
        config
            .apply_config_value("nes-horizontal_overscan", "6")
            .unwrap();
        assert_eq!(config.nes.horizontal_overscan, 6);
    }

    #[test]
    fn test_ram_init_mode_is_in_frontend_config() {
        let config = Config::with_defaults();
        assert_eq!(config.frontend.ram_init_mode, RamInitMode::Random);
    }

    #[test]
    fn test_breakpoints_are_in_frontend_config() {
        let config = Config::with_defaults();
        assert!(config.frontend.breakpoints.is_empty());
    }

    #[test]
    fn test_ram_init_mode_cli_sets_frontend_config() {
        let args = vec![
            "neser".to_string(),
            "--ram-init-mode".to_string(),
            "zero".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.frontend.ram_init_mode, RamInitMode::Zero);
    }

    #[test]
    fn test_breakpoint_cli_sets_frontend_config() {
        use crate::platform::debugging::breakpoints::BreakpointKind;

        let args = vec![
            "neser".to_string(),
            "--breakpoint".to_string(),
            "frame=100".to_string(),
        ];
        let config = parse_config(args);
        assert!(
            config
                .frontend
                .breakpoints
                .contains(&BreakpointKind::Frame(100))
        );
    }

    #[test]
    fn test_gb_dmg_variant_arg_sets_gb_config() {
        let args = vec![
            "neser".to_string(),
            "--gb-dmg-variant".to_string(),
            "dmg-0".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.gb.dmg_variant, crate::gb::model::DmgModel::Dmg0);
    }

    #[test]
    fn test_gb_dmg_variant_default_is_dmg_b() {
        let config = Config::with_defaults();
        assert_eq!(config.gb.dmg_variant, crate::gb::model::DmgModel::DmgB);
    }

    #[test]
    fn test_config_file_gb_dmg_variant_key() {
        let mut config = Config::with_defaults();
        config
            .apply_config_value("gb-dmg-variant", "dmg-c")
            .unwrap();
        assert_eq!(config.gb.dmg_variant, crate::gb::model::DmgModel::DmgC);
    }

    #[test]
    fn test_config_file_gb_hardware_dmg_sets_hardware() {
        let mut config = Config::with_defaults();
        config.apply_config_value("gb-hardware", "dmg").unwrap();
        assert_eq!(config.gb.hardware, Some(crate::gb::model::GbHardware::Dmg));
    }

    #[test]
    fn test_config_file_gb_hardware_cgb_sets_hardware() {
        let mut config = Config::with_defaults();
        config.apply_config_value("gb-hardware", "cgb").unwrap();
        assert_eq!(config.gb.hardware, Some(crate::gb::model::GbHardware::Cgb));
    }

    #[test]
    fn test_config_file_gb_hardware_invalid_value_returns_error() {
        let mut config = Config::with_defaults();
        let result = config.apply_config_value("gb-hardware", "dmg-a");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid gb_hardware value"));
    }

    #[test]
    fn test_config_file_gba_bios_path_sets_gba_config() {
        let mut config = Config::with_defaults();
        config
            .apply_config_value("gba-bios-path", "/tmp/gba_bios.bin")
            .unwrap();
        assert_eq!(config.gba.bios_path.as_deref(), Some("/tmp/gba_bios.bin"));
    }

    #[test]
    fn test_gb_dmg_variant_invalid_value_returns_error() {
        let args = vec![
            "neser".to_string(),
            "--gb-dmg-variant".to_string(),
            "invalid".to_string(),
        ];
        let result = Config::new(&args);
        assert!(result.is_err(), "Invalid variant should produce an error");
    }

    #[test]
    fn test_gb_dmg_variant_all_values() {
        for (input, expected) in [
            ("dmg-0", crate::gb::model::DmgModel::Dmg0),
            ("dmg-a", crate::gb::model::DmgModel::DmgA),
            ("dmg-b", crate::gb::model::DmgModel::DmgB),
            ("dmg-c", crate::gb::model::DmgModel::DmgC),
        ] {
            let args = vec![
                "neser".to_string(),
                "--gb-dmg-variant".to_string(),
                input.to_string(),
            ];
            let config = parse_config(args);
            assert_eq!(config.gb.dmg_variant, expected, "variant={input}");
        }
    }

    #[test]
    fn test_config_file_nes_vs_dip_switches_key_uses_prefix() {
        let mut config = Config::with_defaults();
        config
            .apply_config_value("nes-vs_dip_switches", "0xFF")
            .unwrap();
        assert_eq!(config.nes.vs_dip_switches, 0xFF);
    }

    #[test]
    fn test_gb_hardware_arg_is_valid() {
        let args = vec![
            "neser".to_string(),
            "--gb-hardware".to_string(),
            "dmg".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_ok());
        match result.unwrap() {
            ParseResult::Config(config) => {
                assert_eq!(config.gb.hardware, Some(crate::gb::model::GbHardware::Dmg));
            }
            ParseResult::Help => panic!("Expected Config, got Help"),
        }
    }

    #[test]
    fn test_old_hardware_arg_is_rejected() {
        let args = vec![
            "neser".to_string(),
            "--hardware".to_string(),
            "famicom".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_old_pulse1_arg_is_rejected() {
        let args = vec!["neser".to_string(), "--pulse1".to_string()];
        let result = config_new(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_old_expansion_port_arg_is_rejected() {
        let args = vec![
            "neser".to_string(),
            "--expansion-port".to_string(),
            "arkanoid".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
    }

    // --nes-filter CLI flag tests

    #[test]
    fn test_config_cmdline_nes_filter_crt_sets_shader_path() {
        let args = vec![
            "neser".to_string(),
            "--nes-filter".to_string(),
            "crt".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.shader_path,
            Some("vendor/slang-shaders/crt/crt-lottes.slangp".to_string())
        );
    }

    #[test]
    fn test_config_cmdline_nes_filter_rejects_dmg_shader() {
        let args = vec![
            "neser".to_string(),
            "--nes-filter".to_string(),
            "dmg".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("dmg"),
            "Error should mention the invalid value: {msg}"
        );
    }

    #[test]
    fn test_config_cmdline_nes_filter_accepts_smooth() {
        let args = vec![
            "neser".to_string(),
            "--nes-filter".to_string(),
            "smooth".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.shader_path,
            Some(
                "vendor/slang-shaders/edge-smoothing/xbrz/xbrz-freescale-multipass.slangp"
                    .to_string()
            )
        );
    }

    // --gb-filter CLI flag tests

    #[test]
    fn test_config_cmdline_gb_filter_dmg_sets_shader_path() {
        let args = vec![
            "neser".to_string(),
            "--gb-filter".to_string(),
            "dmg".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.shader_path,
            Some("vendor/slang-shaders/handheld/gameboy.slangp".to_string())
        );
    }

    #[test]
    fn test_config_cmdline_gb_filter_rejects_crt_shader() {
        let args = vec![
            "neser".to_string(),
            "--gb-filter".to_string(),
            "crt".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("crt"),
            "Error should mention the invalid value: {msg}"
        );
    }

    #[test]
    fn test_config_cmdline_gb_filter_none_sets_stock_shader() {
        let args = vec![
            "neser".to_string(),
            "--gb-filter".to_string(),
            "none".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.shader_path,
            Some("shaders/stock.slangp".to_string())
        );
    }

    // nes-filter= config file key tests

    #[test]
    fn test_config_file_nes_filter_ntsc_sets_shader_path() {
        let mut config = Config::default();
        config.apply_config_value("nes-filter", "ntsc").unwrap();
        assert_eq!(
            config.frontend.shader_path,
            Some("vendor/slang-shaders/ntsc/ntsc-256px-composite.slangp".to_string())
        );
    }

    #[test]
    fn test_config_file_nes_filter_rejects_dmg_shader() {
        let mut config = Config::default();
        let result = config.apply_config_value("nes-filter", "dmg");
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("dmg"),
            "Error should mention the invalid value: {msg}"
        );
    }

    #[test]
    fn test_config_file_nes_filter_empty_ignored() {
        let mut config = Config::default();
        config.apply_config_value("nes-filter", "").unwrap();
        assert_eq!(config.frontend.shader_path, None);
    }

    // gb-filter= config file key tests

    #[test]
    fn test_config_file_gb_filter_dmg_sets_shader_path() {
        let mut config = Config::default();
        config.apply_config_value("gb-filter", "dmg").unwrap();
        assert_eq!(
            config.frontend.shader_path,
            Some("vendor/slang-shaders/handheld/gameboy.slangp".to_string())
        );
    }

    #[test]
    fn test_config_file_gb_filter_rejects_crt_shader() {
        let mut config = Config::default();
        let result = config.apply_config_value("gb-filter", "crt");
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("crt"),
            "Error should mention the invalid value: {msg}"
        );
    }

    #[test]
    fn test_config_file_gb_filter_empty_ignored() {
        let mut config = Config::default();
        config.apply_config_value("gb-filter", "").unwrap();
        assert_eq!(config.frontend.shader_path, None);
    }

    // --gba-filter CLI flag tests

    #[test]
    fn test_config_cmdline_gba_filter_agb001_sets_shader_path() {
        let args = vec![
            "neser".to_string(),
            "--gba-filter".to_string(),
            "agb001".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.shader_path,
            Some("vendor/slang-shaders/handheld/agb001.slangp".to_string())
        );
    }

    #[test]
    fn test_config_cmdline_gba_filter_nso_gba_color_sets_shader_path() {
        let args = vec![
            "neser".to_string(),
            "--gba-filter".to_string(),
            "nso-gba-color".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.shader_path,
            Some("vendor/slang-shaders/handheld/color-mod/NSO-gba-color.slangp".to_string())
        );
    }

    #[test]
    fn test_config_cmdline_gba_filter_sp101_color_sets_shader_path() {
        let args = vec![
            "neser".to_string(),
            "--gba-filter".to_string(),
            "sp101-color".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.shader_path,
            Some("vendor/slang-shaders/handheld/color-mod/SP101-color.slangp".to_string())
        );
    }

    #[test]
    fn test_config_cmdline_gba_filter_gba_lcd_grid_sets_shader_path() {
        let args = vec![
            "neser".to_string(),
            "--gba-filter".to_string(),
            "gba-lcd-grid".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(
            config.frontend.shader_path,
            Some("vendor/slang-shaders/handheld/console-border/gba-lcd-grid-v2.slangp".to_string())
        );
    }

    #[test]
    fn test_config_cmdline_gba_filter_rejects_bogus_shader_with_valid_options() {
        let args = vec![
            "neser".to_string(),
            "--gba-filter".to_string(),
            "bogus".to_string(),
        ];
        let result = config_new(args);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("bogus"));
        assert!(msg.contains("none, gba-lcd, agb001, nso-gba-color, sp101-color, gba-lcd-grid"));
    }

    // gba-filter= config file key tests

    #[test]
    fn test_config_file_gba_filter_agb001_sets_shader_path() {
        let mut config = Config::default();
        config.apply_config_value("gba-filter", "agb001").unwrap();
        assert_eq!(
            config.frontend.shader_path,
            Some("vendor/slang-shaders/handheld/agb001.slangp".to_string())
        );
    }

    #[test]
    fn test_config_file_gba_filter_nso_gba_color_sets_shader_path() {
        let mut config = Config::default();
        config
            .apply_config_value("gba-filter", "nso-gba-color")
            .unwrap();
        assert_eq!(
            config.frontend.shader_path,
            Some("vendor/slang-shaders/handheld/color-mod/NSO-gba-color.slangp".to_string())
        );
    }

    #[test]
    fn test_config_file_gba_filter_sp101_color_sets_shader_path() {
        let mut config = Config::default();
        config
            .apply_config_value("gba-filter", "sp101-color")
            .unwrap();
        assert_eq!(
            config.frontend.shader_path,
            Some("vendor/slang-shaders/handheld/color-mod/SP101-color.slangp".to_string())
        );
    }

    #[test]
    fn test_config_file_gba_filter_gba_lcd_grid_sets_shader_path() {
        let mut config = Config::default();
        config
            .apply_config_value("gba-filter", "gba-lcd-grid")
            .unwrap();
        assert_eq!(
            config.frontend.shader_path,
            Some("vendor/slang-shaders/handheld/console-border/gba-lcd-grid-v2.slangp".to_string())
        );
    }

    #[test]
    fn test_config_file_gba_filter_rejects_bogus_shader_with_valid_options() {
        let mut config = Config::default();
        let result = config.apply_config_value("gba-filter", "bogus");
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("bogus"));
        assert!(msg.contains("none, gba-lcd, agb001, nso-gba-color, sp101-color, gba-lcd-grid"));
    }
}
