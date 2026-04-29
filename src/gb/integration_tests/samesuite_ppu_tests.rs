//! SameSuite GB test ROM integration tests.
//!
//! SameSuite tests signal completion by executing `LD B,B` (opcode 0x40).
//! Pass: B=3, C=5, D=8, E=13, H=21, L=34 (Fibonacci sequence).
//! Fail: any register deviates from the pattern above.
//!
//! ROMs are located at:
//! `roms/gb/automated_tests/SameSuite/ppu/`
//!
//! ## CGB PPU Test Status
//!
//! These tests target CGB-specific PPU behavior and require a fully functional
//! CGB PPU implementation. Currently some tests are ignored due to incomplete
//! CGB PPU emulation (particularly BGPI/BGPD register behavior).

use super::helpers::{MooneyeResult, run_and_detect_cgb};
use crate::gb::model::CgbModel;

/// Generous per-test M-cycle timeout used as a safety budget to avoid hangs.
const SAMESUITE_CYCLE_LIMIT: u64 = 150_000_000;

// ============================================================================
// Helper macro to produce a single-line pass assertion.
// ============================================================================

macro_rules! assert_samesuite_pass {
    ($path:expr) => {
        let result = run_and_detect_cgb($path, CgbModel::default(), SAMESUITE_CYCLE_LIMIT);
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
// SameSuite PPU tests
// ============================================================================

const BASE: &str = "roms/gb/automated_tests/SameSuite/ppu";

#[test]
#[ignore = "CGB PPU BGPI/BGPD blocking behavior not yet implemented — see issue #XXXX"]
fn test_samesuite_ppu_blocking_bgpi_increase() {
    assert_samesuite_pass!(&format!("{BASE}/blocking_bgpi_increase.gb"));
}
