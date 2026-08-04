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
//! diverging value exactly one less than Mesen2's, i.e. one fewer instruction
//! ran before dispatch). #3049's per-CPU-cycle dispatch fixes closed 2 (#5,
//! #15) and #3065 closed the remaining 6 (#1, #2, #4, #7 `IRQ-*`, #8, #10
//! `NMI-*`). The mechanism has since been simplified twice without moving any
//! value: #3074 replaced #3065's two-cycle suppression window with Mesen2's
//! per-cycle DMA interrupt lock (`Cpu::dma_locked_this_cycle`, set from
//! `gpdma_cycle_hook` for exactly the cycle a transfer runs in), and #3081
//! deleted the `$420B`/`$4200` instruction-granular `irq_lock_step` overlay,
//! which this oracle demonstrably never depended on. All 19 sub-cases match
//! Mesen2 (WRAM values and a 0-pixel-diff frame-600 screen).

use super::rom_runner::{RunConfig, RunExitReason, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const ROOT: &str = "roms/snes/automated_tests/snes_test_roms/Sour/SnesTests/dma_irq_test";

#[cfg(test)]
mod tests {
    use super::*;

    /// Mesen2-approved golden (#3065): NESER's frame-600 screen is a
    /// 0-pixel-diff match for a Mesen2 headless capture of the same ROM, all 19
    /// sub-case values now correct.
    #[test]
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
                expected_crc: 0xFC3B_465C,
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

    /// Precise per-sub-case oracle for the #3049 dispatch follow-up (#3065):
    /// reads all 19 dma_irq_test result words from WRAM (`testResults` at
    /// `$7E:0011 + N*2`, N=1..19) and asserts them against the hardware/Mesen2
    /// expected table (`$00FF` sentinels for the two no-interrupt SEI+INC cases
    /// #6/#16, per the README `$FFFF` transcription fix).
    ///
    /// Fixed by #3065: all 19 sub-cases now match. (Previously 6 -- #1/#2/#4/#7
    /// IRQ and #8/#10 NMI, all 8-bit `$420B` writes -- dispatched one
    /// instruction early.)
    #[test]
    fn dma_irq_test_wram_results_match_hardware() {
        use crate::platform::app_context::AppContext;
        use crate::platform::emulator::Emulator;
        use crate::snes::console::Snes;
        let path = Path::new(ROOT).join("dma_irq_test.sfc");
        let rom = fs::read(&path).unwrap();
        let mut snes = Snes::new(AppContext::new_with_config(
            crate::platform::config::Config::default(),
        ));
        snes.load_rom(&rom, "dma_irq_test.sfc").unwrap();
        let mut frames = 0u32;
        while frames < 350 {
            snes.run_tick();
            if snes.is_ready_to_render() {
                frames += 1;
                snes.clear_ready_to_render();
            }
        }
        let rd = |a: u32| snes.read_bus_for_debugger_for_tests(a).unwrap_or(0) as u16;
        let word = |a: u32| rd(a) | (rd(a + 1) << 8);
        let expected = [
            0x0002u16, 0x0002, 0x0001, 0x0002, 0x0001, 0x00FF, 0x0001, 0x0002, 0x0001, 0x0001,
            0x0001, 0x0001, 0x0001, 0x0001, 0x0001, 0x00FF, 0x0001, 0x0001, 0x0000,
        ];
        let mismatches: Vec<String> = expected
            .iter()
            .enumerate()
            .filter_map(|(i, exp)| {
                let n = i + 1;
                let got = word(0x7E_0011 + (n as u32) * 2);
                (got != *exp).then(|| format!("#{n}: expected {exp:04X} got {got:04X}"))
            })
            .collect();
        assert!(
            mismatches.is_empty(),
            "dma_irq_test sub-case divergences (#3065): {}",
            mismatches.join(", ")
        );
    }
}
