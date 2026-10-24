//! Two NES consoles in lockstep over a simulated lossy link (nr-ayk5.2) run the same frames on
//! the same pads, and a state mismatch injected into one is detected by both.
//! See [`crate::platform::netplay::simulation`].

use crate::platform::emulator::SystemType;
use crate::platform::netplay::simulation::{
    assert_consoles_stay_in_step, assert_injected_console_mismatch_is_detected,
};

const ROM: &str = "roms/nes/automated_tests/allpads-r9/allpads.nes";

#[test]
fn two_consoles_stay_in_step_over_a_lossy_link() {
    assert_consoles_stay_in_step(SystemType::Nes, ROM);
}

#[test]
fn an_injected_state_mismatch_is_detected() {
    assert_injected_console_mismatch_is_detected(SystemType::Nes, ROM);
}
