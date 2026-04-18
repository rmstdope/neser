/// Game Boy hardware model variant.
///
/// Distinguishes between the first-generation DMG-0 and the
/// individual production DMG-A, DMG-B, DMG-C models. The DMG-0
/// variant differs from the production variants in boot ROM content,
/// post-boot CPU register values, and the DIV counter phase at boot
/// exit. DMG-A, DMG-B, and DMG-C share identical software-visible
/// behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DmgModel {
    /// Production DMG-A hardware.
    ///
    /// Post-boot CPU registers: A=$01 F=$B0 B=$00 C=$13 D=$00 E=$D8 H=$01 L=$4D SP=$FFFE.
    /// DIV=$AB at cartridge entry.
    DmgA,

    /// Production DMG-B hardware (most common revision).
    ///
    /// Post-boot CPU registers: A=$01 F=$B0 B=$00 C=$13 D=$00 E=$D8 H=$01 L=$4D SP=$FFFE.
    /// DIV=$AB at cartridge entry.
    #[default]
    DmgB,

    /// Production DMG-C hardware.
    ///
    /// Post-boot CPU registers: A=$01 F=$B0 B=$00 C=$13 D=$00 E=$D8 H=$01 L=$4D SP=$FFFE.
    /// DIV=$AB at cartridge entry.
    DmgC,

    /// First-generation DMG-0 hardware.
    ///
    /// Post-boot CPU registers: A=$01 F=$00 B=$FF C=$13 D=$00 E=$C1 H=$84 L=$03 SP=$FFFE.
    /// DIV=$18 at cartridge entry (shorter boot ROM).
    Dmg0,
}

/// Boot ROM behavior group.
///
/// DMG-A, DMG-B, and DMG-C all share the same boot ROM and post-boot
/// state; DMG-0 uses a different boot ROM with different post-boot
/// register values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmgBootVariant {
    /// Production DMG boot ROM (used by DMG-A, DMG-B, DMG-C).
    Production,
    /// First-generation DMG-0 boot ROM.
    Dmg0,
}

impl DmgModel {
    /// Returns the boot ROM variant for this hardware model.
    pub fn boot_variant(self) -> DmgBootVariant {
        match self {
            Self::DmgA | Self::DmgB | Self::DmgC => DmgBootVariant::Production,
            Self::Dmg0 => DmgBootVariant::Dmg0,
        }
    }

    /// Parse a DMG model variant from a string value.
    ///
    /// Accepts `dmg-0`, `dmg-a`, `dmg-b`, `dmg-c` (case-insensitive).
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "dmg-0" => Some(Self::Dmg0),
            "dmg-a" => Some(Self::DmgA),
            "dmg-b" => Some(Self::DmgB),
            "dmg-c" => Some(Self::DmgC),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boot_variant_dmg_a_is_production() {
        assert_eq!(DmgModel::DmgA.boot_variant(), DmgBootVariant::Production);
    }

    #[test]
    fn test_boot_variant_dmg_b_is_production() {
        assert_eq!(DmgModel::DmgB.boot_variant(), DmgBootVariant::Production);
    }

    #[test]
    fn test_boot_variant_dmg_c_is_production() {
        assert_eq!(DmgModel::DmgC.boot_variant(), DmgBootVariant::Production);
    }

    #[test]
    fn test_boot_variant_dmg_0_is_dmg0() {
        assert_eq!(DmgModel::Dmg0.boot_variant(), DmgBootVariant::Dmg0);
    }

    #[test]
    fn test_default_is_dmg_b() {
        assert_eq!(DmgModel::default(), DmgModel::DmgB);
    }

    #[test]
    fn test_parse_dmg_0() {
        assert_eq!(DmgModel::parse("dmg-0"), Some(DmgModel::Dmg0));
    }

    #[test]
    fn test_parse_dmg_a() {
        assert_eq!(DmgModel::parse("dmg-a"), Some(DmgModel::DmgA));
    }

    #[test]
    fn test_parse_dmg_b() {
        assert_eq!(DmgModel::parse("dmg-b"), Some(DmgModel::DmgB));
    }

    #[test]
    fn test_parse_dmg_c() {
        assert_eq!(DmgModel::parse("dmg-c"), Some(DmgModel::DmgC));
    }

    #[test]
    fn test_parse_case_insensitive() {
        assert_eq!(DmgModel::parse("DMG-B"), Some(DmgModel::DmgB));
        assert_eq!(DmgModel::parse("Dmg-A"), Some(DmgModel::DmgA));
    }

    #[test]
    fn test_parse_invalid_returns_none() {
        assert_eq!(DmgModel::parse("dmg"), None);
        assert_eq!(DmgModel::parse("invalid"), None);
        assert_eq!(DmgModel::parse(""), None);
    }
}
