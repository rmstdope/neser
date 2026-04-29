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

use super::helpers::{MooneyeResult, run_and_detect_dmg};
use crate::gb::model::DmgModel;

/// Default cycle limit for SameSuite interrupt tests.
const SAMESUITE_CYCLE_LIMIT: u64 = 15_000_000;

// ============================================================================
// Helper macro to produce a single-line pass assertion.
// ============================================================================

macro_rules! assert_samesuite_pass {
    ($path:expr) => {
        let result = run_and_detect_dmg($path, DmgModel::DmgB, SAMESUITE_CYCLE_LIMIT);
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
