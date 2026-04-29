//! SameSuite GB test ROM integration tests.
//!
//! SameSuite tests signal completion by executing `LD B,B` (opcode 0x40).
//! Pass: B=3, C=5, D=8, E=13, H=21, L=34 (Fibonacci sequence).
//! Fail: any register deviates from the pattern above.
//!
//! ROMs are located at:
//! `roms/gb/automated_tests/SameSuite/`
//!
//! SameSuite project: https://github.com/LIJI32/SameSuite

use super::mooneye_tests::{MooneyeResult, detect_mooneye_result};
use crate::gb::bus::DmgBus;
use crate::gb::cartridge::load_cartridge;
use crate::gb::console::Gb;
use crate::gb::model::DmgModel;

/// Load a GB ROM from `path` and return a ready-to-step `Gb<DmgBus>` (DMG-B model).
fn load_gb_rom(path: &str) -> Gb<DmgBus> {
    let rom = std::fs::read(path).expect("SameSuite ROM file should be present");
    let cart = load_cartridge(&rom).expect("valid GB ROM");
    Gb::new(DmgBus::new(cart, DmgModel::DmgB))
}

/// Run a SameSuite test ROM to completion and return the result.
fn run_samesuite_rom(path: &str) -> MooneyeResult {
    let mut gb = load_gb_rom(path);
    detect_mooneye_result(&mut gb)
}

// ============================================================================
// Helper macro to produce a single-line pass assertion.
// ============================================================================

macro_rules! assert_samesuite_pass {
    ($path:expr) => {
        let result = run_samesuite_rom($path);
        assert_eq!(
            result,
            MooneyeResult::Pass,
            "SameSuite test failed: {:?} — ROM: {}",
            result,
            $path
        );
    };
}

// ============================================================================
// interrupt/ tests
// ============================================================================

const BASE: &str = "roms/gb/automated_tests/SameSuite";

#[test]
fn test_samesuite_interrupt_ei_delay_halt() {
    assert_samesuite_pass!(&format!("{BASE}/interrupt/ei_delay_halt.gb"));
}
