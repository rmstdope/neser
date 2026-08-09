//! End-to-end coverage for the SNES power-on RAM pattern (issue #3128).
//!
//! NESER used to zero-fill every SNES volatile memory unconditionally, while
//! both reference emulators randomise by default (Mesen2's SNES
//! `RamPowerOnState` is `RamState::Random`; ares fills WRAM at power-on via
//! `random.array()`). That is an accuracy gap, and it also produced a phantom
//! bug: `test_dmatiming/demo.smc` displays uninitialised WRAM at
//! `$7EC004`/`$7EC006`, so two default Mesen2 captures of it differ from *each
//! other* by 1.06% -- recorded in #3063 as a 0.93% DMA-timing divergence that
//! did not exist.
//!
//! The SNES core now honours the same generic `ram_init_mode` setting the NES
//! core uses, covering WRAM, VRAM, CGRAM, OAM, ARAM and SA-1 I-RAM. Every test
//! here drives the *real* `Snes::load_rom` path with the setting placed in
//! `FrontendConfig`, so it pins the config-to-hardware wiring and not just the
//! fill helper (which `platform::ram_init` tests on its own).
//!
//! Cartridge RAM is covered too, but *only* when it is not battery-backed: a
//! `.sav`-backed save is restored from disk, so filling it would either be
//! overwritten or corrupt a save on a cartridge with no `.sav` yet. The NES core
//! draws the line in the same place (`nes/cartridge/cartridge.rs`,
//! `test_initialize_ram_preserves_battery_backed_save_ram`), and the
//! battery-vs-volatile split itself is pinned by
//! `system_bus::tests::power_on_fills_sram_only_when_it_is_not_battery_backed`.
//!
//! Fixtures are synthetic in-code images, so this suite reads no ROM assets.

#[cfg(test)]
mod tests {
    use super::super::cartridge_fixtures::CartFixture;
    use crate::platform::app_context::AppContext;
    use crate::platform::config::RamInitMode;
    use crate::platform::emulator::Emulator;
    use crate::snes::bus::SnesBus;
    use crate::snes::cartridge::Mapping;
    use crate::snes::console::Snes;
    use crate::snes::test_support::snes_test_config;

    /// A WRAM address no fixture program ever writes, used as the hard/soft
    /// reset sentinel.
    const SENTINEL_ADDR: u32 = 0x7E_1234;

    /// An SA-1 LoROM image with 32 KiB of battery-backed SRAM.
    ///
    /// SA-1 so the cartridge brings I-RAM along -- without it only five of the
    /// six memories exist and a gap in the fill would go unnoticed. SRAM so the
    /// "save RAM is not filled" assertion has something to assert about.
    fn sa1_fixture_with_sram() -> Vec<u8> {
        CartFixture::new(Mapping::LoRom)
            .chipset(0x35) // SA-1 + RAM + battery
            .ram_size_field(0x05) // 32 KiB
            .country(0x01) // North America (NTSC)
            .build()
    }

    fn power_on(mode: RamInitMode) -> Snes {
        let mut config = snes_test_config();
        config.frontend.ram_init_mode = mode;
        let mut snes = Snes::new(AppContext::new_with_config(config));
        snes.load_rom(&sa1_fixture_with_sram(), "ram_init_fixture.sfc")
            .expect("load SA-1 fixture");
        snes
    }

    /// Every volatile RAM the console owns, named so a memory that is not wired
    /// into the power-on fill is identified by the failure message instead of
    /// disappearing into a combined assertion.
    fn volatile_rams(snes: &Snes) -> Vec<(&'static str, Vec<u8>)> {
        let bus = snes.bus_for_tests().expect("ROM loaded");
        let bus_state = bus.capture_state();
        let ppu_state = bus.ppu_capture_state();
        let sa1 = bus_state.sa1.expect("fixture is an SA-1 cartridge");
        vec![
            ("WRAM", bus_state.wram),
            ("VRAM", ppu_state.vram),
            ("CGRAM", ppu_state.cgram),
            ("OAM", ppu_state.oam),
            ("ARAM", bus_state.apu.aram),
            ("SA-1 I-RAM", sa1.iram),
        ]
    }

    fn save_ram(snes: &Snes) -> Vec<u8> {
        snes.bus_for_tests()
            .expect("ROM loaded")
            .capture_state()
            .sram
    }

    /// Given `ram_init_mode=zero`, when the console powers on, then every
    /// volatile RAM reads back as `$00`.
    #[test]
    fn zero_mode_powers_on_with_every_volatile_ram_cleared() {
        let snes = power_on(RamInitMode::Zero);

        for (name, contents) in volatile_rams(&snes) {
            assert!(!contents.is_empty(), "{name} should exist on this fixture");
            assert!(
                contents.iter().all(|&byte| byte == 0x00),
                "{name} should be zero-filled under ram_init_mode=zero"
            );
        }
    }

    /// Given `ram_init_mode=seeded-random:SEED`, when two consoles power on with
    /// the same seed, then every volatile RAM is filled and byte-identical; with
    /// a different seed, every one of them differs.
    ///
    /// This is the test that fails if a memory is added to the SNES but never
    /// wired into the power-on fill: a forgotten buffer stays all-zero and trips
    /// the "was left zeroed" assertion by name.
    #[test]
    fn seeded_mode_is_reproducible_per_seed_across_every_volatile_ram() {
        let first = volatile_rams(&power_on(RamInitMode::SeededRandom(42)));
        let same_seed = volatile_rams(&power_on(RamInitMode::SeededRandom(42)));
        let other_seed = volatile_rams(&power_on(RamInitMode::SeededRandom(43)));

        assert_eq!(first.len(), 6, "an SA-1 console has six volatile RAMs");
        for (((name, first), (_, same)), (_, other)) in
            first.iter().zip(same_seed.iter()).zip(other_seed.iter())
        {
            assert!(
                first.iter().any(|&byte| byte != 0x00),
                "{name} was left zeroed -- it is not wired into the power-on fill"
            );
            assert_eq!(first, same, "{name} must be reproducible for the same seed");
            assert_ne!(first, other, "{name} must differ for a different seed");
        }
    }

    /// Given `ram_init_mode=random`, when two consoles power on, then their RAM
    /// differs -- the setting reaches the hardware rather than being accepted
    /// and ignored.
    #[test]
    fn random_mode_powers_on_differently_on_every_run() {
        let first = volatile_rams(&power_on(RamInitMode::Random));
        let second = volatile_rams(&power_on(RamInitMode::Random));

        for ((name, first), (_, second)) in first.iter().zip(second.iter()) {
            assert_ne!(
                first, second,
                "{name} must differ between two random power-ons"
            );
        }
    }

    /// Given a powered-on console, then battery-backed save RAM stays clear
    /// regardless of the mode -- it is restored from `.sav`. (The fixture's
    /// chipset `$35` has battery bit set; the non-battery half of this rule is
    /// covered at the bus level.)
    #[test]
    fn save_ram_is_never_filled_by_the_power_on_pattern() {
        let random = power_on(RamInitMode::Random);
        assert!(
            !save_ram(&random).is_empty(),
            "the fixture must declare SRAM for this exclusion to mean anything"
        );

        for mode in [
            RamInitMode::Zero,
            RamInitMode::Random,
            RamInitMode::SeededRandom(42),
        ] {
            let snes = power_on(mode);
            assert!(
                save_ram(&snes).iter().all(|&byte| byte == 0x00),
                "battery-backed save RAM must not be filled ({mode:?})"
            );
        }
    }

    /// Given a running console, when a *hard* reset happens, then every volatile
    /// RAM is re-initialised from the configured pattern -- a hard reset models a
    /// power cycle, as it does on the NES side (`nes/bus/bus.rs::reset`).
    #[test]
    fn hard_reset_reapplies_the_power_on_pattern() {
        let mut snes = power_on(RamInitMode::SeededRandom(42));
        let at_power_on = volatile_rams(&snes);
        let sentinel_before = snes
            .read_bus_for_debugger_for_tests(SENTINEL_ADDR)
            .expect("ROM loaded");
        let sentinel = sentinel_before.wrapping_add(1);
        snes.bus_mut_for_tests()
            .expect("ROM loaded")
            .write(SENTINEL_ADDR, sentinel);
        assert_eq!(
            snes.read_bus_for_debugger_for_tests(SENTINEL_ADDR),
            Some(sentinel),
            "sentinel write must land, or the test proves nothing"
        );

        snes.reset(false);

        assert_eq!(
            snes.read_bus_for_debugger_for_tests(SENTINEL_ADDR),
            Some(sentinel_before),
            "a hard reset must wipe RAM written since power-on"
        );
        for ((name, before), (_, after)) in at_power_on.iter().zip(volatile_rams(&snes).iter()) {
            assert_eq!(
                before, after,
                "{name} must be re-initialised to the same seeded pattern by a hard reset"
            );
        }
    }

    /// Given a running console, when a *soft* reset happens, then RAM survives.
    /// A real /RES does not clear WRAM, and the blargg ROM handshake in
    /// `rom_runner` depends on this.
    #[test]
    fn soft_reset_preserves_ram() {
        let mut snes = power_on(RamInitMode::SeededRandom(42));
        let sentinel = snes
            .read_bus_for_debugger_for_tests(SENTINEL_ADDR)
            .expect("ROM loaded")
            .wrapping_add(1);
        snes.bus_mut_for_tests()
            .expect("ROM loaded")
            .write(SENTINEL_ADDR, sentinel);
        let before = volatile_rams(&snes);

        snes.reset(true);

        assert_eq!(
            snes.read_bus_for_debugger_for_tests(SENTINEL_ADDR),
            Some(sentinel),
            "a soft reset must preserve RAM written since power-on"
        );
        for ((name, before), (_, after)) in before.iter().zip(volatile_rams(&snes).iter()) {
            assert_eq!(before, after, "{name} must survive a soft reset");
        }
    }
}
