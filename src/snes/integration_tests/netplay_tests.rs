//! Two SNES consoles in lockstep over a simulated lossy link (nr-ayk5.2) run the same frames on
//! the same pads, and a state mismatch injected into one is detected by both.
//! See [`crate::platform::netplay::simulation`].

use crate::platform::determinism::{
    assert_netplay_hash_ignores_undrained_audio, assert_netplay_hash_matches_after_a_resync,
};
use crate::platform::emulator::SystemType;
use crate::platform::netplay::simulation::{
    assert_consoles_stay_in_step, assert_injected_console_mismatch_is_detected,
};

const ROM: &str = "roms/snes/automated_tests/snes_test_roms/tepples-tellinglys/tellinglys.sfc";

#[test]
fn two_consoles_stay_in_step_over_a_lossy_link() {
    assert_consoles_stay_in_step(SystemType::Snes, ROM);
}

#[test]
fn an_injected_state_mismatch_is_detected() {
    assert_injected_console_mismatch_is_detected(SystemType::Snes, ROM);
}

#[test]
fn the_netplay_hash_ignores_audio_the_page_has_not_played_yet() {
    assert_netplay_hash_ignores_undrained_audio(SystemType::Snes, ROM);
}

#[test]
fn the_netplay_hash_matches_from_the_first_frame_after_a_resync() {
    assert_netplay_hash_matches_after_a_resync(SystemType::Snes, ROM);
}
