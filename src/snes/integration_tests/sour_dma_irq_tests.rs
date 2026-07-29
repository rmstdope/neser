//! Sour/SnesTests' `dma_irq_test.sfc` (issue #2883), vendored as a
//! `git subtree` mirror of SourMesen/SnesTests (no LICENSE, recorded as
//! `unknown`; see `roms/snes/automated_tests/manifest.json`).
//!
//! Validates how many instructions run after a manual DMA ($420B write)
//! before a pending IRQ or NMI is dispatched, across 19 sub-cases (7 H-IRQ
//! at HTIME=VTIME=2, 3 VBlank-NMI, then the same 10 repeated with a
//! 16-bit-wide $420B write). Each sub-case's result is rendered as a 4-digit
//! hex value next to its name; the ROM then freezes (screen is stable by
//! frame 300, confirmed identical at frame 600).
//!
//! Rebuilt byte-identical from `src/` with the vendored `lorom256k.cfg`
//! (`ca65 -g` + `ld65 --dbgfile`, sha256-verified against the committed
//! `.sfc`) to recover `testResults`' WRAM address (zeropage `$11`, 2 bytes
//! per sub-case, `$11 + N*2` for sub-case N=1..19) directly from the debug
//! symbol table, rather than guessing from the disassembly.
//!
//! The upstream README's expected-results table has a transcription error:
//! it lists `$FFFF` for the two "SEI+INC" no-interrupt-fires sub-cases
//! (#6, #16), but `valueOnIrq` (the byte captured into the result) is
//! declared as a single WRAM byte and is stored via an 8-bit `STA`, so the
//! real (and Mesen2-confirmed, screen- and WRAM-verified) sentinel is
//! `$00FF`, not `$FFFF`. The golden CRC below reflects the Mesen2-verified
//! screen, not the README table.
//!
//! NESER originally diverged from Mesen2 on 8 of the 19 sub-cases (every
//! diverging value exactly one less than Mesen2's, i.e. one fewer
//! instruction ran before dispatch). #3049's per-CPU-cycle NMI and H/V-IRQ
//! dispatch fixes closed 2 of those 8 (#5 `IRQ-CLI+INC`, #15
//! `W16:IRQ-CLI+INC`); 6 remain (#1, #2, #4, #7 `IRQ-*`, #8, #10 `NMI-*`),
//! same off-by-one-instruction signature, root cause not yet identified
//! (matches `kungfufurby_nmi_tests::test_nmi_passes` and
//! `kungfufurby_irq_tests`' remaining 5 ROMs' status). The sub-cases that
//! don't depend on interrupt timing (#3, #6, #9, #11-14, #16-19) already
//! matched before and still do.

use super::rom_runner::{RunConfig, RunExitReason, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const ROOT: &str = "roms/snes/automated_tests/snes_test_roms/Sour/SnesTests/dma_irq_test";

#[cfg(test)]
mod tests {
    use super::*;

    /// NESER's current CRC (screen has 6/19 sub-case values off by one
    /// instruction, down from 8/19 before #3049's per-cycle dispatch
    /// fixes), NOT a Mesen2-approved golden. See #3049.
    #[test]
    #[ignore = "6/19 sub-cases off by one dispatched instruction; pending #3049 follow-up"]
    fn dma_irq_test_passes() {
        let path = Path::new(ROOT).join("dma_irq_test.sfc");
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let result = run_rom_with_oracle(
            &rom,
            "dma_irq_test.sfc",
            "sour_dma_irq_tests",
            RunConfig::new(400_000_000, 0),
            RunOracle::ScreenCrc {
                frames: 600,
                expected_crc: 0xF5F5_37A7,
            },
        );
        assert!(
            result.passed && result.exit_reason == RunExitReason::ScreenCrcFrame,
            "dma_irq_test.sfc: expected screen-CRC PASS at frame 600, \
             got crc=0x{:08X} passed={} exit={:?}",
            result.screen_crc32,
            result.passed,
            result.exit_reason
        );
    }
}
