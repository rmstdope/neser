use crate::cartridge::ines::{ConsoleType, TimingMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareType {
    NesNtsc,
    NesPal,
    Famicom,
    VsSystem,
    Dendy,
    Playchoice10,
    NesMultiRegion,
    Vt01Monochrome,
    Vt01Stn,
    Vt02,
    Vt03,
    Vt09,
    Vt32,
    Vt369,
    UmcUm6578,
    FamicomNetworkSystem,
}

impl HardwareType {
    pub fn from_db_value(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::NesNtsc),
            1 => Some(Self::NesPal),
            2 => Some(Self::Famicom),
            3 => Some(Self::VsSystem),
            4 => Some(Self::Dendy),
            5 => Some(Self::Playchoice10),
            6 => Some(Self::NesMultiRegion),
            7 => Some(Self::Vt01Monochrome),
            8 => Some(Self::Vt01Stn),
            9 => Some(Self::Vt02),
            10 => Some(Self::Vt03),
            11 => Some(Self::Vt09),
            12 => Some(Self::Vt32),
            13 => Some(Self::Vt369),
            14 => Some(Self::UmcUm6578),
            15 => Some(Self::FamicomNetworkSystem),
            _ => None,
        }
    }

    pub fn timing_mode(self) -> TimingMode {
        match self {
            Self::NesNtsc
            | Self::Famicom
            | Self::VsSystem
            | Self::Playchoice10
            | Self::Vt01Monochrome
            | Self::Vt01Stn
            | Self::Vt02
            | Self::Vt03
            | Self::Vt09
            | Self::Vt32
            | Self::Vt369
            | Self::UmcUm6578
            | Self::FamicomNetworkSystem => TimingMode::Ntsc,
            Self::NesPal => TimingMode::Pal,
            Self::Dendy => TimingMode::Dendy,
            Self::NesMultiRegion => TimingMode::MultiRegion,
        }
    }

    pub fn from_console_type_and_timing(
        console_type: ConsoleType,
        timing_mode: TimingMode,
    ) -> Self {
        match console_type {
            ConsoleType::VsSystem => Self::VsSystem,
            ConsoleType::Playchoice10 => Self::Playchoice10,
            ConsoleType::Extended(ext) => match ext {
                3 => Self::Vt01Monochrome,
                4 => Self::Vt01Stn,
                5 => Self::Vt02,
                6 => Self::Vt03,
                7 => Self::Vt09,
                8 => Self::Vt32,
                9 => Self::Vt369,
                10 => Self::UmcUm6578,
                11 => Self::FamicomNetworkSystem,
                _ => Self::NesNtsc,
            },
            ConsoleType::NesFamicom => match timing_mode {
                TimingMode::Pal => Self::NesPal,
                TimingMode::Dendy => Self::Dendy,
                TimingMode::MultiRegion => Self::NesMultiRegion,
                TimingMode::Ntsc | TimingMode::Unknown(_) => Self::NesNtsc,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_db_value_0_returns_nes_ntsc() {
        assert_eq!(HardwareType::from_db_value(0), Some(HardwareType::NesNtsc));
    }

    #[test]
    fn from_db_value_1_returns_nes_pal() {
        assert_eq!(HardwareType::from_db_value(1), Some(HardwareType::NesPal));
    }

    #[test]
    fn from_db_value_2_returns_famicom() {
        assert_eq!(HardwareType::from_db_value(2), Some(HardwareType::Famicom));
    }

    #[test]
    fn from_db_value_3_returns_vs_system() {
        assert_eq!(HardwareType::from_db_value(3), Some(HardwareType::VsSystem));
    }

    #[test]
    fn from_db_value_4_returns_dendy() {
        assert_eq!(HardwareType::from_db_value(4), Some(HardwareType::Dendy));
    }

    #[test]
    fn from_db_value_5_returns_playchoice10() {
        assert_eq!(
            HardwareType::from_db_value(5),
            Some(HardwareType::Playchoice10)
        );
    }

    #[test]
    fn from_db_value_6_returns_nes_multi_region() {
        assert_eq!(
            HardwareType::from_db_value(6),
            Some(HardwareType::NesMultiRegion)
        );
    }

    #[test]
    fn from_db_value_255_returns_none() {
        assert_eq!(HardwareType::from_db_value(255), None);
    }

    #[test]
    fn timing_mode_nes_ntsc_returns_ntsc() {
        assert_eq!(HardwareType::NesNtsc.timing_mode(), TimingMode::Ntsc);
    }

    #[test]
    fn timing_mode_nes_pal_returns_pal() {
        assert_eq!(HardwareType::NesPal.timing_mode(), TimingMode::Pal);
    }

    #[test]
    fn timing_mode_famicom_returns_ntsc() {
        assert_eq!(HardwareType::Famicom.timing_mode(), TimingMode::Ntsc);
    }

    #[test]
    fn timing_mode_vs_system_returns_ntsc() {
        assert_eq!(HardwareType::VsSystem.timing_mode(), TimingMode::Ntsc);
    }

    #[test]
    fn timing_mode_dendy_returns_dendy() {
        assert_eq!(HardwareType::Dendy.timing_mode(), TimingMode::Dendy);
    }

    #[test]
    fn timing_mode_playchoice10_returns_ntsc() {
        assert_eq!(HardwareType::Playchoice10.timing_mode(), TimingMode::Ntsc);
    }

    #[test]
    fn timing_mode_nes_multi_region_returns_multi_region() {
        assert_eq!(
            HardwareType::NesMultiRegion.timing_mode(),
            TimingMode::MultiRegion
        );
    }

    #[test]
    fn from_console_nes_famicom_ntsc_returns_nes_ntsc() {
        assert_eq!(
            HardwareType::from_console_type_and_timing(ConsoleType::NesFamicom, TimingMode::Ntsc),
            HardwareType::NesNtsc
        );
    }

    #[test]
    fn from_console_nes_famicom_pal_returns_nes_pal() {
        assert_eq!(
            HardwareType::from_console_type_and_timing(ConsoleType::NesFamicom, TimingMode::Pal),
            HardwareType::NesPal
        );
    }

    #[test]
    fn from_console_nes_famicom_dendy_returns_dendy() {
        assert_eq!(
            HardwareType::from_console_type_and_timing(ConsoleType::NesFamicom, TimingMode::Dendy),
            HardwareType::Dendy
        );
    }

    #[test]
    fn from_console_nes_famicom_multi_region_returns_nes_multi_region() {
        assert_eq!(
            HardwareType::from_console_type_and_timing(
                ConsoleType::NesFamicom,
                TimingMode::MultiRegion
            ),
            HardwareType::NesMultiRegion
        );
    }

    #[test]
    fn from_console_vs_system_returns_vs_system() {
        assert_eq!(
            HardwareType::from_console_type_and_timing(ConsoleType::VsSystem, TimingMode::Ntsc),
            HardwareType::VsSystem
        );
    }

    #[test]
    fn from_console_playchoice10_returns_playchoice10() {
        assert_eq!(
            HardwareType::from_console_type_and_timing(ConsoleType::Playchoice10, TimingMode::Ntsc),
            HardwareType::Playchoice10
        );
    }

    #[test]
    fn from_console_extended_3_returns_vt01_monochrome() {
        assert_eq!(
            HardwareType::from_console_type_and_timing(ConsoleType::Extended(3), TimingMode::Ntsc),
            HardwareType::Vt01Monochrome
        );
    }

    #[test]
    fn from_console_extended_5_returns_vt02() {
        assert_eq!(
            HardwareType::from_console_type_and_timing(ConsoleType::Extended(5), TimingMode::Ntsc),
            HardwareType::Vt02
        );
    }

    #[test]
    fn from_console_extended_unknown_defaults_to_nes_ntsc() {
        assert_eq!(
            HardwareType::from_console_type_and_timing(ConsoleType::Extended(99), TimingMode::Ntsc),
            HardwareType::NesNtsc
        );
    }

    #[test]
    fn from_db_value_7_returns_vt01_monochrome() {
        assert_eq!(
            HardwareType::from_db_value(7),
            Some(HardwareType::Vt01Monochrome)
        );
    }

    #[test]
    fn from_db_value_15_returns_famicom_network_system() {
        assert_eq!(
            HardwareType::from_db_value(15),
            Some(HardwareType::FamicomNetworkSystem)
        );
    }

    #[test]
    fn from_db_value_16_returns_none() {
        assert_eq!(HardwareType::from_db_value(16), None);
    }
}
