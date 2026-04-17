//! Configuration for the Game Boy emulator.
//!
//! [`GbConfig`] holds Game Boy-specific configuration options such as the
//! emulated hardware variant (DMG or CGB).

/// Game Boy hardware variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GbHardware {
    /// Original Game Boy (DMG).
    Dmg,
    /// Game Boy Color (CGB).
    Cgb,
}

impl GbHardware {
    /// Parse a hardware variant from a string value.
    pub fn parse(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("dmg") {
            Some(Self::Dmg)
        } else if value.eq_ignore_ascii_case("cgb") {
            Some(Self::Cgb)
        } else {
            None
        }
    }
}

/// Game Boy-specific hardware configuration.
#[derive(Debug, Clone)]
pub struct GbConfig {
    /// Emulated hardware variant.
    pub hardware: GbHardware,
    /// Whether hardware was explicitly configured (vs. auto-detected from ROM).
    pub hardware_explicit: bool,
}

impl Default for GbConfig {
    fn default() -> Self {
        Self {
            hardware: GbHardware::Dmg,
            hardware_explicit: false,
        }
    }
}
