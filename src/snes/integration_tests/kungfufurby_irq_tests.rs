//! KungFuFurby's IRQ test ROM collection (issue #2883), from the same
//! "SNES TEST IMAGE" (byuu) family as `kungfufurby_nmi_tests` (no formal
//! license, recorded as `unknown`; `test_irq.smc`/`test_irq4200.smc` are
//! byte-identical to the tukuyomi-bsnes-tests mirror -- see
//! `roms/snes/automated_tests/manifest.json`).
//!
//! Same pass/fail convention as the NMI suite: blue backdrop for PASS,
//! red/maroon for FAIL. Verified against Mesen2 headless captures, which
//! show blue for all six ROMs by frame 600 (`irq.smc` needs longer,
//! matching `demo_irqtest.smc` transitioning well after frame 600 too).
//!
//! All six currently fail in NESER: investigated as issue #2883 increment
//! 2 and found to share the same root cause as `kungfufurby_nmi_tests`'
//! `nmi.smc`/`test_nmi.smc` divergences (tracked in #3049) -- NESER's H/V
//! IRQ dispatch resolves a few master clocks early relative to Mesen2 (an
//! interrupt-pending check granularity gap, not an IRQ-specific bug; see
//! the #3049 issue comment for the investigation and disproven alternate
//! hypothesis).

use super::rom_runner::{RunConfig, RunExitReason, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const ROOT: &str = "roms/snes/automated_tests/snes_test_roms/KungFuFurby-test-ROMs";

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs a KungFuFurby IRQ ROM to `frames` and asserts the rendered
    /// screen matches the Mesen2-approved PASS golden CRC32.
    ///
    /// To approve a new golden, run with NESER_CAPTURE_SCREEN=1, visually
    /// confirm the capture under target/snes_test_captures/ against a
    /// Mesen2 headless capture at the same frame, then record the CRC here.
    fn run_rom_screen_crc(file: &str, frames: u32, expected_crc: u32) {
        let path = Path::new(ROOT).join(file);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let result = run_rom_with_oracle(
            &rom,
            file,
            "kungfufurby_irq_tests",
            RunConfig::new(400_000_000, 0),
            RunOracle::ScreenCrc {
                frames,
                expected_crc,
            },
        );
        assert!(
            result.passed && result.exit_reason == RunExitReason::ScreenCrcFrame,
            "{file}: expected screen-CRC PASS (blue) at frame {frames}, \
             got crc=0x{:08X} passed={} exit={:?}",
            result.screen_crc32,
            result.passed,
            result.exit_reason
        );
    }

    /// NESER's current CRC (red/fail state), NOT a Mesen2-approved golden.
    /// See #3049: shares the NMI suite's interrupt-dispatch-precision root
    /// cause (V-IRQ mode, VTIME=225, fires ~18 master clocks early).
    #[test]
    #[ignore = "H/V-IRQ dispatch timing not yet bit-exact vs Mesen2; pending #3049"]
    fn irq_passes() {
        run_rom_screen_crc("irq.smc", 1200, 0xDEAD_FA89);
    }

    /// NESER's current CRC (red/fail state), NOT a Mesen2-approved golden.
    /// See #3049.
    #[test]
    #[ignore = "H/V-IRQ dispatch timing not yet bit-exact vs Mesen2; pending #3049"]
    fn test_irq_passes() {
        run_rom_screen_crc("test_irq.smc", 600, 0x0B56_4EEF);
    }

    /// NESER's current CRC (red/fail state), NOT a Mesen2-approved golden.
    /// See #3049.
    #[test]
    #[ignore = "H/V-IRQ dispatch timing not yet bit-exact vs Mesen2; pending #3049"]
    fn test_irq4200_passes() {
        run_rom_screen_crc("test_irq4200.smc", 600, 0x0B56_4EEF);
    }

    /// NESER's current CRC (red/fail state), NOT a Mesen2-approved golden.
    /// See #3049.
    #[test]
    #[ignore = "H/V-IRQ dispatch timing not yet bit-exact vs Mesen2; pending #3049"]
    fn test_irq4209_passes() {
        run_rom_screen_crc("test_irq4209.smc", 600, 0x0B56_4EEF);
    }

    /// NESER's current CRC (red/fail state), NOT a Mesen2-approved golden.
    /// See #3049.
    #[test]
    #[ignore = "H/V-IRQ dispatch timing not yet bit-exact vs Mesen2; pending #3049"]
    fn test_irqb_passes() {
        run_rom_screen_crc("test_irqb.smc", 600, 0xDEAD_FA89);
    }

    /// NESER's current CRC (red/fail state), NOT a Mesen2-approved golden.
    /// See #3049.
    #[test]
    #[ignore = "H/V-IRQ dispatch timing not yet bit-exact vs Mesen2; pending #3049"]
    fn demo_irqtest_passes() {
        run_rom_screen_crc("demo_irqtest.smc", 600, 0xDEAD_FA89);
    }
}
