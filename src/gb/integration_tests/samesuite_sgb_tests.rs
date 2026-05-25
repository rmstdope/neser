//! SameSuite SGB command test ROM integration tests.
//!
//! These ROMs use SGB packets over `$FF00` and signal completion using the
//! Mooneye-compatible `LD B,B` breakpoint with Fibonacci register values.

use super::helpers::{MooneyeResult, run_and_detect_sgb};
use crate::gb::model::DmgModel;

const BASE: &str = "roms/gb/automated_tests/SameSuite/sgb";
const SAMESUITE_CYCLE_LIMIT: u64 = 15_000_000;

macro_rules! assert_samesuite_sgb_pass {
    ($path:expr) => {{
        let result = run_and_detect_sgb($path, DmgModel::DmgB, SAMESUITE_CYCLE_LIMIT);
        assert_eq!(
            result,
            MooneyeResult::Pass,
            "SameSuite SGB test failed: {:?}",
            result
        );
    }};
}

/// Test `command_mlt_req.gb`: validates SGB MLT_REQ player-count selection.
#[test]
fn test_samesuite_command_mlt_req() {
    assert_samesuite_sgb_pass!(&format!("{}/command_mlt_req.gb", BASE));
}

/// Test `command_mlt_req_1_incrementing.gb`: validates MLT_REQ P15 incrementing.
#[test]
fn test_samesuite_command_mlt_req_1_incrementing() {
    assert_samesuite_sgb_pass!(&format!("{}/command_mlt_req_1_incrementing.gb", BASE));
}
