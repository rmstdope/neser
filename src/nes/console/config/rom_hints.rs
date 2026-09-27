//! What the ROM database and the cartridge header say about the hardware a game expects,
//! and the one transform that writes it into the [`Config`].
//!
//! The console reads its configuration once, when it builds its hardware, so every hint is
//! resolved into the `Config` before that happens (see `Nes::insert_cartridge`). Adding a
//! hint touches two places in this file: a field filled by [`RomHints::resolve`] from the
//! database, and a line in [`Config::apply_rom_hints`]. Nothing downstream re-syncs.

use crate::nes::cartridge::{Cartridge, HardwareType, RomDb};
use crate::nes::console::{ExpansionPort, HardwareMode, HardwareModel};
use crate::platform::config::Config;

/// Every configuration hint known for one cartridge.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RomHints {
    /// The database classes the ROM as a Japan-region (Famicom) release.
    pub japan_region: bool,
    /// The game expects the Famicom four-players adapter on the expansion port.
    pub famicom_four_players: bool,
    /// The game expects the Famicom Arkanoid controller on the expansion port.
    pub arkanoid_famicom: bool,
    /// The game expects a Zapper on the Famicom expansion port.
    pub zapper_famicom: bool,
    /// The game expects the Family Trainer mat on the Famicom expansion port.
    pub power_pad_famicom: bool,
    /// The game supports the NES Four Score adapter.
    pub nes_four_score: bool,
    /// The header marks a VS System board.
    pub vs_system: bool,
    /// The header marks a PlayChoice-10 board.
    pub playchoice10: bool,
    /// The VS board swaps the two controller ports.
    pub vs_controllers_swapped: bool,
}

impl RomHints {
    /// Every configuration hint the ROM database and the header give for `cartridge`.
    pub fn resolve(cartridge: &Cartridge, rom_db: &RomDb) -> Self {
        let crc32 = cartridge.crc32();
        Self {
            japan_region: rom_db.is_japan_region(crc32),
            famicom_four_players: rom_db.has_famicom_four_players_expansion(crc32),
            arkanoid_famicom: rom_db.has_arkanoid_famicom_expansion(crc32),
            zapper_famicom: rom_db.has_zapper_famicom_expansion(crc32),
            power_pad_famicom: rom_db.has_power_pad_famicom_expansion(crc32),
            nes_four_score: rom_db.has_nes_four_score_expansion(crc32),
            vs_system: cartridge.vs_ppu_type().is_some() || cartridge.vs_hardware_type().is_some(),
            playchoice10: matches!(cartridge.hardware_type(), HardwareType::Playchoice10),
            vs_controllers_swapped: rom_db.has_vs_swapped_controllers(crc32),
        }
    }
}

impl Config {
    /// Write every hint into the configuration, leaving any setting the user made explicit.
    ///
    /// The order matters: the region hint makes the console a Famicom, which the Zapper and
    /// Power Pad expansion hints require before they act.
    pub fn apply_rom_hints(&mut self, hints: &RomHints) {
        self.apply_rom_db_famicom_region_hint(hints.japan_region);
        self.apply_rom_db_famicom_four_players_hint(hints.famicom_four_players);
        self.apply_rom_db_arkanoid_famicom_hint(hints.arkanoid_famicom);
        self.apply_rom_db_zapper_famicom_hint(hints.zapper_famicom);
        self.apply_rom_db_power_pad_famicom_hint(hints.power_pad_famicom);
        self.apply_rom_db_nes_four_score_hint(hints.nes_four_score);
        self.apply_rom_db_vs_system_hint(hints.vs_system);
        self.apply_rom_db_playchoice10_hint(hints.playchoice10);
        self.apply_rom_db_vs_controllers_swapped_hint(hints.vs_controllers_swapped);
    }

    fn apply_rom_db_famicom_four_players_hint(&mut self, has_hint: bool) -> bool {
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

    fn apply_rom_db_arkanoid_famicom_hint(&mut self, has_hint: bool) -> bool {
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

    fn apply_rom_db_zapper_famicom_hint(&mut self, has_hint: bool) -> bool {
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
    fn apply_rom_db_power_pad_famicom_hint(&mut self, has_hint: bool) -> bool {
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

    fn apply_rom_db_famicom_region_hint(&mut self, is_japan: bool) -> bool {
        if !is_japan {
            return false;
        }

        if !self.nes.hardware_mode_explicit && self.nes.hardware_mode != HardwareMode::Famicom {
            self.nes.hardware_mode = HardwareMode::Famicom;
            return true;
        }

        false
    }

    fn apply_rom_db_vs_system_hint(&mut self, is_vs: bool) -> bool {
        if !is_vs {
            return false;
        }

        if !self.nes.expansion_port_explicit && self.nes.expansion_port != ExpansionPort::VsSystem {
            self.nes.expansion_port = ExpansionPort::VsSystem;
            return true;
        }

        false
    }

    fn apply_rom_db_playchoice10_hint(&mut self, is_playchoice10: bool) -> bool {
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
    fn apply_rom_db_vs_controllers_swapped_hint(&mut self, swapped: bool) {
        self.nes.vs_controllers_swapped = swapped;
    }

    /// Apply ROM DB hint for NES Four Score adapter.
    ///
    /// Unlike other hints, this method also **resets** `four_score_enabled` to `false`
    /// when `has_hint` is `false` and not explicitly configured, so that swapping from a
    /// Four Score ROM to a standard ROM correctly disables Four Score mode.
    fn apply_rom_db_nes_four_score_hint(&mut self, has_hint: bool) -> bool {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::{VsHardwareType, VsPpuType};
    use crate::nes::console::NesConfig;
    use crate::platform::test_roms::minimal_nes_rom;

    const CRC: u32 = 0x2459_8791;

    fn cartridge(rom: &[u8]) -> Cartridge {
        let mut cartridge = Cartridge::load_from_file(rom, "test.nes", None).expect("load cart");
        cartridge.set_crc32_for_test(CRC);
        cartridge
    }

    fn db_with_row(rom_class: &str, expansion_type: &str) -> RomDb {
        RomDb::from_csv_content(&format!(
            "1,Game,,{CRC:08X},,{rom_class},,,,,,,,,,,,,,,{expansion_type}\n"
        ))
    }

    #[test]
    fn resolve_unknown_crc_gives_no_hints() {
        let hints = RomHints::resolve(&cartridge(&minimal_nes_rom(false)), &RomDb::default());
        assert_eq!(hints, RomHints::default());
    }

    #[test]
    fn resolve_reads_the_region_and_expansion_from_the_database() {
        let hints = RomHints::resolve(
            &cartridge(&minimal_nes_rom(false)),
            &db_with_row("Licensed Japan", "3"),
        );
        assert_eq!(
            hints,
            RomHints {
                japan_region: true,
                famicom_four_players: true,
                ..RomHints::default()
            }
        );
    }

    #[test]
    fn resolve_reads_playchoice10_from_the_header() {
        let mut rom = minimal_nes_rom(false);
        rom[7] = 0x02; // flags7: PlayChoice-10
        let hints = RomHints::resolve(&cartridge(&rom), &RomDb::default());
        assert!(hints.playchoice10);
        assert!(!hints.vs_system);
    }

    #[test]
    fn resolve_reads_vs_system_from_an_nes2_vs_header() {
        let mut rom = minimal_nes_rom(false);
        rom[7] = 0x09; // flags7: NES 2.0 identifier, VS System console
        rom[13] = 0x10; // VS hardware type 1, VS PPU type 0
        let hints = RomHints::resolve(&cartridge(&rom), &RomDb::default());
        assert!(hints.vs_system);
        assert!(!hints.playchoice10);
    }

    #[test]
    fn resolve_reads_vs_system_from_the_vs_ppu_type_alone() {
        let mut cartridge = cartridge(&minimal_nes_rom(false));
        cartridge.set_vs_ppu_type_for_test(Some(VsPpuType::Rp2c03b));
        assert!(RomHints::resolve(&cartridge, &RomDb::default()).vs_system);
    }

    #[test]
    fn resolve_reads_vs_system_from_the_vs_hardware_type_alone() {
        let mut cartridge = cartridge(&minimal_nes_rom(false));
        cartridge.set_vs_hardware_type_for_test(Some(VsHardwareType::Unisystem));
        assert!(RomHints::resolve(&cartridge, &RomDb::default()).vs_system);
    }

    #[test]
    fn resolve_reads_swapped_vs_controllers_from_the_database() {
        let hints = RomHints::resolve(&cartridge(&minimal_nes_rom(false)), &db_with_row("", "5"));
        assert_eq!(
            hints,
            RomHints {
                vs_controllers_swapped: true,
                ..RomHints::default()
            }
        );
    }

    #[test]
    fn apply_rom_hints_region_runs_before_the_zapper_expansion() {
        let mut config = Config::default();
        config.apply_rom_hints(&RomHints {
            japan_region: true,
            zapper_famicom: true,
            ..RomHints::default()
        });
        assert_eq!(config.nes.hardware_mode, HardwareMode::Famicom);
        assert_eq!(config.nes.expansion_port, ExpansionPort::ZapperFamicom);
    }

    #[test]
    fn apply_rom_hints_without_hints_leaves_a_default_config_unchanged() {
        let mut config = Config::default();
        config.apply_rom_hints(&RomHints::default());
        assert_eq!(
            config.nes.hardware_mode,
            Config::default().nes.hardware_mode
        );
        assert_eq!(
            config.nes.expansion_port,
            Config::default().nes.expansion_port
        );
        assert!(!config.nes.four_score_enabled);
    }

    #[test]
    fn apply_rom_hints_without_four_score_turns_a_previous_games_four_score_off() {
        let mut config = Config::default();
        config.apply_rom_hints(&RomHints {
            nes_four_score: true,
            ..RomHints::default()
        });
        assert!(config.nes.four_score_enabled);
        config.apply_rom_hints(&RomHints::default());
        assert!(!config.nes.four_score_enabled);
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
}
