//! SameSuite DMA test ROM integration tests.
//!
//! SameSuite tests signal completion using the Mooneye-compatible convention:
//! `LD B,B` (opcode 0x40) breakpoint with Fibonacci register values.
//! Pass: B=3, C=5, D=8, E=13, H=21, L=34.
//! Fail: any register deviates from the pattern above.
//!
//! ROMs are located at:
//! `roms/gb/automated_tests/SameSuite/dma/`

use crate::gb::bus::{CgbBus, GbBus};
use crate::gb::cartridge::load_cartridge;
use crate::gb::console::Gb;
use crate::gb::model::CgbModel;

/// Outcome of running a Mooneye-compatible test ROM to completion.
#[derive(Debug, PartialEq)]
pub enum MooneyeResult {
    /// B=3, C=5, D=8, E=13, H=21, L=34 at the `LD B,B` breakpoint.
    Pass,
    /// The `LD B,B` breakpoint was hit but registers did not match the Fibonacci pattern.
    Fail {
        b: u8,
        c: u8,
        d: u8,
        e: u8,
        h: u8,
        l: u8,
    },
    /// The ROM did not hit the breakpoint within the M-cycle budget.
    Timeout,
}

/// Mooneye pass: Fibonacci register values at `LD B,B` breakpoint.
const FIBO_B: u8 = 3;
const FIBO_C: u8 = 5;
const FIBO_D: u8 = 8;
const FIBO_E: u8 = 13;
const FIBO_H: u8 = 21;
const FIBO_L: u8 = 34;

/// Generous per-test M-cycle timeout used as a safety budget to avoid hangs.
const MOONEYE_CYCLE_LIMIT: u64 = 15_000_000;

/// LD B,B opcode used as a Mooneye software breakpoint.
const LD_B_B: u8 = 0x40;

/// Load a GB ROM from `path` and return a ready-to-step `Gb<CgbBus>` (CGB-E model).
fn load_cgb_rom(path: &str) -> Gb<CgbBus> {
    let rom = std::fs::read(path).expect("SameSuite ROM file should be present");
    let cart = load_cartridge(&rom).expect("valid GB ROM");
    let mut gb = Gb::new(CgbBus::new(cart, CgbModel::default()));
    // Set CGB post-boot-ROM CPU register state (A=$11 = CGB hardware identifier).
    gb.cpu.reset_registers_cgb();
    gb
}

/// Step `gb` until the Mooneye breakpoint fires or `cycle_limit` M-cycles elapse (CGB).
///
/// Detects the `LD B,B` (0x40) breakpoint by peeking at the next opcode
/// before each step. For these tests, peeking at the opcode at `PC` is safe
/// because execution is in cartridge/boot ROM space in our bus implementation.
fn detect_mooneye_result_cgb_with_limit(gb: &mut Gb<CgbBus>, cycle_limit: u64) -> MooneyeResult {
    let start = gb.cycles();
    loop {
        let opcode = gb.cpu.bus.read(gb.cpu.regs.pc);
        if opcode == LD_B_B {
            let r = &gb.cpu.regs;
            if r.b == FIBO_B
                && r.c == FIBO_C
                && r.d == FIBO_D
                && r.e == FIBO_E
                && r.h == FIBO_H
                && r.l == FIBO_L
            {
                return MooneyeResult::Pass;
            } else {
                return MooneyeResult::Fail {
                    b: r.b,
                    c: r.c,
                    d: r.d,
                    e: r.e,
                    h: r.h,
                    l: r.l,
                };
            }
        }
        if gb.cycles().saturating_sub(start) >= cycle_limit {
            return MooneyeResult::Timeout;
        }
        gb.step();
    }
}

/// Step `gb` until the Mooneye breakpoint fires or the default cycle limit is reached (CGB).
fn detect_mooneye_result_cgb(gb: &mut Gb<CgbBus>) -> MooneyeResult {
    detect_mooneye_result_cgb_with_limit(gb, MOONEYE_CYCLE_LIMIT)
}

// ============================================================================
// SameSuite DMA tests
// ============================================================================

const DMA_BASE: &str = "roms/gb/automated_tests/SameSuite/dma";

/// Test `gdma_addr_mask.gb`: validates GDMA address masking behavior.
/// ROM requires CGB hardware.
#[test]
fn test_samesuite_gdma_addr_mask() {
    let mut gb = load_cgb_rom(&format!("{DMA_BASE}/gdma_addr_mask.gb"));
    let result = detect_mooneye_result_cgb(&mut gb);
    assert_eq!(
        result,
        MooneyeResult::Pass,
        "SameSuite gdma_addr_mask test failed: {:?}",
        result
    );
}

/// Test `gbc_dma_cont.gb`: validates GBC DMA continuation behavior.
/// ROM requires CGB hardware.
#[test]
fn test_samesuite_gbc_dma_cont() {
    let mut gb = load_cgb_rom(&format!("{DMA_BASE}/gbc_dma_cont.gb"));
    let result = detect_mooneye_result_cgb(&mut gb);
    assert_eq!(
        result,
        MooneyeResult::Pass,
        "SameSuite gbc_dma_cont test failed: {:?}",
        result
    );
}

/// Test `hdma_lcd_off.gb`: validates HDMA behavior when LCD is off.
/// ROM requires CGB hardware.
#[test]
fn test_samesuite_hdma_lcd_off() {
    let mut gb = load_cgb_rom(&format!("{DMA_BASE}/hdma_lcd_off.gb"));
    let result = detect_mooneye_result_cgb(&mut gb);
    assert_eq!(
        result,
        MooneyeResult::Pass,
        "SameSuite hdma_lcd_off test failed: {:?}",
        result
    );
}

/// Test `hdma_mode0.gb`: validates HDMA mode 0 (HBlank DMA) behavior.
/// ROM requires CGB hardware.
#[test]
fn test_samesuite_hdma_mode0() {
    let mut gb = load_cgb_rom(&format!("{DMA_BASE}/hdma_mode0.gb"));
    let result = detect_mooneye_result_cgb(&mut gb);
    assert_eq!(
        result,
        MooneyeResult::Pass,
        "SameSuite hdma_mode0 test failed: {:?}",
        result
    );
}
