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

use crate::gb::bus::CgbBus;
use crate::gb::cartridge::load_cartridge;
use crate::gb::console::Gb;
use crate::gb::model::CgbModel;

/// Outcome of running a SameSuite test ROM to completion.
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
const SAMESUITE_CYCLE_LIMIT: u64 = 150_000_000;

/// LD B,B opcode used as a Mooneye software breakpoint.
const LD_B_B: u8 = 0x40;

/// Load a GB ROM from `path` and return a ready-to-step `Gb<CgbBus>` (CGB-E model).
fn load_cgb_rom(path: &str) -> Gb<CgbBus> {
    let rom = std::fs::read(path).expect("SameSuite ROM file should be present");
    let cart = load_cartridge(&rom).expect("valid GB ROM");
    let mut gb = Gb::new(CgbBus::new(cart, CgbModel::CgbE));
    // Set CGB post-boot-ROM CPU register state (A=$11 = CGB hardware identifier).
    gb.cpu.reset_registers_cgb();
    gb
}

/// Step `gb` until the Mooneye breakpoint fires or `cycle_limit` M-cycles elapse.
///
/// Detects the `LD B,B` (0x40) breakpoint by peeking at the next opcode
/// before each step. For these tests, peeking at the opcode at `PC` is safe
/// because execution is in cartridge/boot ROM space in our bus implementation.
pub(crate) fn detect_mooneye_result_with_limit_cgb(
    gb: &mut Gb<CgbBus>,
    cycle_limit: u64,
) -> MooneyeResult {
    let start = gb.cycles();
    loop {
        let opcode = gb.read_for_debugger(gb.cpu.regs.pc);
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

/// Step `gb` until the Mooneye breakpoint fires or the default cycle limit is reached.
pub fn detect_mooneye_result_cgb(gb: &mut Gb<CgbBus>) -> MooneyeResult {
    detect_mooneye_result_with_limit_cgb(gb, SAMESUITE_CYCLE_LIMIT)
}

/// Run a SameSuite test ROM to completion and return the result.
pub fn run_samesuite_rom(path: &str) -> MooneyeResult {
    let mut gb = load_cgb_rom(path);
    detect_mooneye_result_cgb(&mut gb)
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
// SameSuite PPU tests
// ============================================================================

const BASE: &str = "roms/gb/automated_tests/SameSuite/ppu";

#[test]
#[ignore = "CGB PPU BGPI/BGPD blocking behavior not yet implemented — see issue #XXXX"]
fn test_samesuite_ppu_blocking_bgpi_increase() {
    assert_samesuite_pass!(&format!("{BASE}/blocking_bgpi_increase.gb"));
}
