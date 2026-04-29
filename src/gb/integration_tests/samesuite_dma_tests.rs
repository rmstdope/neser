//! SameSuite DMA test ROM integration tests.
//!
//! SameSuite tests signal completion using the Mooneye-compatible convention:
//! `LD B,B` (opcode 0x40) breakpoint with Fibonacci register values.
//! Pass: B=3, C=5, D=8, E=13, H=21, L=34.
//! Fail: any register deviates from the pattern above.
//!
//! ROMs are located at:
//! `roms/gb/automated_tests/SameSuite/dma/`

use super::helpers::{MooneyeResult, run_and_detect_cgb};
use crate::gb::model::CgbModel;

const BASE: &str = "roms/gb/automated_tests/SameSuite/dma";
const SAMESUITE_CYCLE_LIMIT: u64 = 15_000_000;

macro_rules! assert_samesuite_pass {
    ($path:expr) => {{
        let result = run_and_detect_cgb($path, CgbModel::default(), SAMESUITE_CYCLE_LIMIT);
        assert_eq!(
            result,
            MooneyeResult::Pass,
            "SameSuite DMA test failed: {:?}",
            result
        );
    }};
}

// ============================================================================
// SameSuite DMA tests
// ============================================================================

/// Test `gdma_addr_mask.gb`: validates GDMA address masking behavior.
/// ROM requires CGB hardware.
#[test]
#[ignore = "HDMA emulation incomplete - test timeouts tracked in issue #2226"]
fn test_samesuite_gdma_addr_mask() {
    assert_samesuite_pass!(&format!("{}/gdma_addr_mask.gb", BASE));
}

/// Test `gbc_dma_cont.gb`: validates GBC DMA continuation behavior.
/// ROM requires CGB hardware.
#[test]
#[ignore = "HDMA emulation incomplete - test timeouts tracked in issue #2226"]
fn test_samesuite_gbc_dma_cont() {
    assert_samesuite_pass!(&format!("{}/gbc_dma_cont.gb", BASE));
}

/// Test `hdma_lcd_off.gb`: validates HDMA behavior when LCD is off.
/// ROM requires CGB hardware.
#[test]
#[ignore = "HDMA emulation incomplete - test timeouts tracked in issue #2226"]
fn test_samesuite_hdma_lcd_off() {
    assert_samesuite_pass!(&format!("{}/hdma_lcd_off.gb", BASE));
}

/// Test `hdma_mode0.gb`: validates HDMA mode 0 (HBlank DMA) behavior.
/// ROM requires CGB hardware.
#[test]
#[ignore = "HDMA emulation incomplete - test timeouts tracked in issue #2226"]
fn test_samesuite_hdma_mode0() {
    assert_samesuite_pass!(&format!("{}/hdma_mode0.gb", BASE));
}
