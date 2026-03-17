use crate::cartridge::ines::TimingMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum HardwareType {
    NesNtsc,
    NesPal,
    Famicom,
    VsSystem,
    Dendy,
    Playchoice10,
    NesMultiRegion,
}

impl HardwareType {
    #[allow(dead_code)]
    pub fn from_db_value(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::NesNtsc),
            1 => Some(Self::NesPal),
            2 => Some(Self::Famicom),
            3 => Some(Self::VsSystem),
            4 => Some(Self::Dendy),
            5 => Some(Self::Playchoice10),
            6 => Some(Self::NesMultiRegion),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn timing_mode(self) -> TimingMode {
        match self {
            Self::NesNtsc | Self::Famicom | Self::VsSystem | Self::Playchoice10 => TimingMode::Ntsc,
            Self::NesPal => TimingMode::Pal,
            Self::Dendy => TimingMode::Dendy,
            Self::NesMultiRegion => TimingMode::MultiRegion,
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
    fn from_db_value_7_returns_none() {
        assert_eq!(HardwareType::from_db_value(7), None);
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
}
