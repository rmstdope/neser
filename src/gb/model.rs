use serde::{Deserialize, Serialize};

/// Game Boy hardware target selection.
///
/// Determines which Game Boy hardware variant to emulate: original DMG,
/// Game Boy Color (CGB), or Game Boy Advance in GBC mode (GBA).
///
/// This enum is used in configuration to explicitly select the hardware
/// mode, or can be `None` to enable smart default behavior (auto-detect
/// based on ROM compatibility flags).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GbHardware {
    /// Original Game Boy (DMG) hardware.
    Dmg,
    /// Game Boy Color (CGB) hardware.
    Cgb,
    /// Game Boy Advance in GBC compatibility mode.
    Gba,
}

impl GbHardware {
    /// Parse a GB hardware target from a string value.
    ///
    /// Accepts `dmg`, `cgb`, `gba` (case-insensitive).
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "dmg" => Some(Self::Dmg),
            "cgb" => Some(Self::Cgb),
            "gba" => Some(Self::Gba),
            _ => None,
        }
    }
}

/// Game Boy hardware model variant.
///
/// Distinguishes between the first-generation DMG-0 and the
/// individual production DMG-A, DMG-B, DMG-C models. The DMG-0
/// variant differs from the production variants in boot ROM content,
/// post-boot CPU register values, and the DIV counter phase at boot
/// exit. DMG-A, DMG-B, and DMG-C share identical software-visible
/// behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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
        match value.trim().to_ascii_lowercase().as_str() {
            "dmg-0" => Some(Self::Dmg0),
            "dmg-a" => Some(Self::DmgA),
            "dmg-b" => Some(Self::DmgB),
            "dmg-c" => Some(Self::DmgC),
            _ => None,
        }
    }
}

/// Game Boy Color (CGB) hardware model variant.
///
/// Distinguishes between different CGB production revisions (CGB-0 through CGB-E).
/// Each revision may differ in boot ROM content, post-boot CPU register values,
/// DIV counter initial value, and other hardware quirks. CGB-0 is the first
/// revision (rare), while CGB-A through CGB-E are production variants with
/// CGB-E being the latest and most common.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CgbModel {
    /// CGB-0 hardware (first revision, rare).
    ///
    /// Post-boot CPU registers (same as Production CGB):
    /// A=$11 F=$80 B=$00 C=$00 D=$00 E=$08 H=$00 L=$7C SP=$FFFE.
    /// DIV=$04 at cartridge entry.
    ///
    /// **Key difference from Production CGB**: CGB-0 does NOT initialize wave RAM
    /// ($FF30-$FF3F), leaving it at all zeros. Production CGB initializes wave RAM
    /// to an alternating 0x00/0xFF pattern.
    Cgb0,

    /// CGB-A hardware (early production).
    ///
    /// Post-boot CPU registers (Mooneye-verified):
    /// A=$11 F=$80 B=$00 C=$00 D=$00 E=$08 H=$00 L=$7C SP=$FFFE.
    /// DIV=$04 at cartridge entry (same as CGB-B through E).
    CgbA,

    /// CGB-B hardware (production variant).
    ///
    /// Post-boot CPU registers (Mooneye-verified):
    /// A=$11 F=$80 B=$00 C=$00 D=$00 E=$08 H=$00 L=$7C SP=$FFFE.
    /// DIV=$04 at cartridge entry (same as CGB-A, C, D, E).
    CgbB,

    /// CGB-C hardware (production variant).
    ///
    /// Post-boot CPU registers (Mooneye-verified):
    /// A=$11 F=$80 B=$00 C=$00 D=$00 E=$08 H=$00 L=$7C SP=$FFFE.
    /// DIV=$04 at cartridge entry (same as CGB-A, B, D, E).
    CgbC,

    /// CGB-D hardware (production variant).
    ///
    /// Post-boot CPU registers (Mooneye-verified):
    /// A=$11 F=$80 B=$00 C=$00 D=$00 E=$08 H=$00 L=$7C SP=$FFFE.
    /// DIV=$04 at cartridge entry (same as CGB-A, B, C, E).
    CgbD,

    /// CGB-E hardware (latest production, most common).
    ///
    /// Post-boot CPU registers (Mooneye-verified):
    /// A=$11 F=$80 B=$00 C=$00 D=$00 E=$08 H=$00 L=$7C SP=$FFFE.
    /// DIV=$04 at cartridge entry (same as CGB-A, B, C, D).
    #[default]
    CgbE,
}

/// Boot ROM behavior group for CGB revisions.
///
/// CGB-0 uses a distinct boot ROM from production variants (CGB-A through E),
/// which share a common boot ROM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CgbBootVariant {
    /// CGB-0 boot ROM (first revision).
    Cgb0,
    /// Production CGB boot ROM (used by CGB-A, CGB-B, CGB-C, CGB-D, CGB-E).
    Production,
}

impl CgbModel {
    /// Returns the boot ROM variant for this hardware model.
    pub fn boot_variant(self) -> CgbBootVariant {
        match self {
            Self::Cgb0 => CgbBootVariant::Cgb0,
            Self::CgbA | Self::CgbB | Self::CgbC | Self::CgbD | Self::CgbE => {
                CgbBootVariant::Production
            }
        }
    }

    /// Parse a CGB model variant from a string value.
    ///
    /// Accepts `cgb-0`, `cgb-a`, `cgb-b`, `cgb-c`, `cgb-d`, `cgb-e` (case-insensitive).
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "cgb-0" => Some(Self::Cgb0),
            "cgb-a" => Some(Self::CgbA),
            "cgb-b" => Some(Self::CgbB),
            "cgb-c" => Some(Self::CgbC),
            "cgb-d" => Some(Self::CgbD),
            "cgb-e" => Some(Self::CgbE),
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

    // GbHardware tests
    #[test]
    fn test_gb_hardware_parse_dmg() {
        assert_eq!(GbHardware::parse("dmg"), Some(GbHardware::Dmg));
    }

    #[test]
    fn test_gb_hardware_parse_cgb() {
        assert_eq!(GbHardware::parse("cgb"), Some(GbHardware::Cgb));
    }

    #[test]
    fn test_gb_hardware_parse_gba() {
        assert_eq!(GbHardware::parse("gba"), Some(GbHardware::Gba));
    }

    #[test]
    fn test_gb_hardware_parse_case_insensitive() {
        assert_eq!(GbHardware::parse("DMG"), Some(GbHardware::Dmg));
        assert_eq!(GbHardware::parse("Cgb"), Some(GbHardware::Cgb));
        assert_eq!(GbHardware::parse("GBA"), Some(GbHardware::Gba));
    }

    #[test]
    fn test_gb_hardware_parse_invalid_returns_none() {
        assert_eq!(GbHardware::parse("invalid"), None);
        assert_eq!(GbHardware::parse(""), None);
        assert_eq!(GbHardware::parse("gameboy"), None);
    }

    #[test]
    fn test_gb_hardware_parse_handles_whitespace() {
        assert_eq!(GbHardware::parse(" cgb "), Some(GbHardware::Cgb));
        assert_eq!(GbHardware::parse("\tdmg\n"), Some(GbHardware::Dmg));
        assert_eq!(GbHardware::parse("  gba  "), Some(GbHardware::Gba));
    }

    #[test]
    fn test_dmg_model_parse_handles_whitespace() {
        assert_eq!(DmgModel::parse(" dmg-b "), Some(DmgModel::DmgB));
        assert_eq!(DmgModel::parse("\tdmg-0\n"), Some(DmgModel::Dmg0));
    }

    // CgbModel tests
    #[test]
    fn test_cgb_boot_variant_cgb0_is_cgb0() {
        assert_eq!(CgbModel::Cgb0.boot_variant(), CgbBootVariant::Cgb0);
    }

    #[test]
    fn test_cgb_boot_variant_cgb_a_is_production() {
        assert_eq!(CgbModel::CgbA.boot_variant(), CgbBootVariant::Production);
    }

    #[test]
    fn test_cgb_boot_variant_cgb_b_is_production() {
        assert_eq!(CgbModel::CgbB.boot_variant(), CgbBootVariant::Production);
    }

    #[test]
    fn test_cgb_boot_variant_cgb_c_is_production() {
        assert_eq!(CgbModel::CgbC.boot_variant(), CgbBootVariant::Production);
    }

    #[test]
    fn test_cgb_boot_variant_cgb_d_is_production() {
        assert_eq!(CgbModel::CgbD.boot_variant(), CgbBootVariant::Production);
    }

    #[test]
    fn test_cgb_boot_variant_cgb_e_is_production() {
        assert_eq!(CgbModel::CgbE.boot_variant(), CgbBootVariant::Production);
    }

    #[test]
    fn test_default_cgb_is_cgb_e() {
        assert_eq!(CgbModel::default(), CgbModel::CgbE);
    }

    #[test]
    fn test_cgb_parse_cgb_0() {
        assert_eq!(CgbModel::parse("cgb-0"), Some(CgbModel::Cgb0));
    }

    #[test]
    fn test_cgb_parse_cgb_a() {
        assert_eq!(CgbModel::parse("cgb-a"), Some(CgbModel::CgbA));
    }

    #[test]
    fn test_cgb_parse_cgb_b() {
        assert_eq!(CgbModel::parse("cgb-b"), Some(CgbModel::CgbB));
    }

    #[test]
    fn test_cgb_parse_cgb_c() {
        assert_eq!(CgbModel::parse("cgb-c"), Some(CgbModel::CgbC));
    }

    #[test]
    fn test_cgb_parse_cgb_d() {
        assert_eq!(CgbModel::parse("cgb-d"), Some(CgbModel::CgbD));
    }

    #[test]
    fn test_cgb_parse_cgb_e() {
        assert_eq!(CgbModel::parse("cgb-e"), Some(CgbModel::CgbE));
    }

    #[test]
    fn test_cgb_parse_case_insensitive() {
        assert_eq!(CgbModel::parse("CGB-0"), Some(CgbModel::Cgb0));
        assert_eq!(CgbModel::parse("Cgb-A"), Some(CgbModel::CgbA));
        assert_eq!(CgbModel::parse("CGB-E"), Some(CgbModel::CgbE));
    }

    #[test]
    fn test_cgb_parse_handles_whitespace() {
        assert_eq!(CgbModel::parse(" cgb-b "), Some(CgbModel::CgbB));
        assert_eq!(CgbModel::parse("\tcgb-0\n"), Some(CgbModel::Cgb0));
    }

    #[test]
    fn test_cgb_parse_invalid_returns_none() {
        assert_eq!(CgbModel::parse("cgb"), None);
        assert_eq!(CgbModel::parse("cgb-f"), None);
        assert_eq!(CgbModel::parse("invalid"), None);
        assert_eq!(CgbModel::parse(""), None);
    }
}
