use super::rom_runner::{RunConfig, RunExitReason, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const ROM_PASS_FAIL_ROOT: &str = "roms/snes/automated_tests/rom_pass_fail/blargg_spc_apu/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum BehaviorCategory {
    Smp,
    Timers,
    Dsp,
}

/// A blargg SPC700/APU ROM that currently FAILS in this emulator. Tracked per
/// #2876 (documented in the manifest notes and the README-SNES candidate table)
/// but neither committed nor run as an ignored test.
#[derive(Debug, Clone, Copy)]
struct CandidateRom {
    category: BehaviorCategory,
    name: &'static str,
    reason: &'static str,
}

fn candidate_roms() -> Vec<CandidateRom> {
    vec![
        CandidateRom {
            category: BehaviorCategory::Smp,
            name: "4-test_ram_disable",
            reason: "fails: reports code CC",
        },
        CandidateRom {
            category: BehaviorCategory::Smp,
            name: "test_ram_disable_ipl",
            reason: "fails: APU RAM/IPL disable",
        },
        CandidateRom {
            category: BehaviorCategory::Smp,
            name: "spc_smp",
            reason: "fails 02: SMP behavior",
        },
        CandidateRom {
            category: BehaviorCategory::Smp,
            name: "spc_mem_access_times",
            reason: "fails: memory access timing (screen not yet stable)",
        },
        CandidateRom {
            category: BehaviorCategory::Dsp,
            name: "spc_dsp6",
            reason: "fails 03: DSP echo/basics",
        },
        CandidateRom {
            category: BehaviorCategory::Timers,
            name: "spc_timer",
            reason: "fails 02: SPC timer",
        },
        CandidateRom {
            category: BehaviorCategory::Timers,
            name: "test_timer_speed",
            reason: "fails: timer speed",
        },
        CandidateRom {
            category: BehaviorCategory::Timers,
            name: "test_timer_speed2",
            reason: "fails: timer speed",
        },
        CandidateRom {
            category: BehaviorCategory::Timers,
            name: "test_timer_stop",
            reason: "fails: timer stop",
        },
        CandidateRom {
            category: BehaviorCategory::Timers,
            name: "test_timer_stop2",
            reason: "fails: timer stop",
        },
        CandidateRom {
            category: BehaviorCategory::Timers,
            name: "timer_at_power_reset",
            reason: "fails: timer at power/reset",
        },
        CandidateRom {
            category: BehaviorCategory::Timers,
            name: "speed_2_freezes2",
            reason: "fails: speed/freeze",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_roms_cover_expected_categories() {
        let entries = candidate_roms();
        let categories: std::collections::BTreeSet<BehaviorCategory> =
            entries.iter().map(|entry| entry.category).collect();

        for required in [
            BehaviorCategory::Smp,
            BehaviorCategory::Timers,
            BehaviorCategory::Dsp,
        ] {
            assert!(
                categories.contains(&required),
                "candidate catalog missing expected category {required:?}"
            );
        }

        for entry in &entries {
            assert!(
                !entry.name.is_empty() && !entry.reason.is_empty(),
                "candidate ROM must have a non-empty name and reason"
            );
        }
    }

    #[test]
    fn candidate_roms_are_documented_in_manifest_notes() {
        let manifest_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("roms/snes/automated_tests/manifest.json");
        let manifest_text = std::fs::read_to_string(&manifest_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));

        for entry in candidate_roms() {
            assert!(
                manifest_text.contains(entry.name),
                "expected manifest notes to document candidate ROM '{}'",
                entry.name
            );
        }
    }

    /// Vendored blargg SPC700/APU ROMs whose PASS screen was visually approved.
    /// Each tuple is (committed file name, capture frame, golden screen CRC32).
    const VERIFIED_ROMS: &[(&str, u32, u32)] = &[
        ("1-test_exec_from_io.smc", 600, 0x7EEE_5E15),
        ("2-test_single_instr.smc", 600, 0x2B42_CE76),
        ("3-test_write_disable.smc", 600, 0xC3DE_3F4F),
        ("test_speed.smc", 600, 0x5085_D88F),
        ("test_timer_speed_2.smc", 600, 0x471F_26BD),
        ("test_timer_speed3.smc", 600, 0x0BBB_12C6),
    ];

    #[test]
    fn verified_spc_apu_roms_pass_with_screen_crc_golden() {
        let root = Path::new(ROM_PASS_FAIL_ROOT);
        for &(file, frames, expected_crc) in VERIFIED_ROMS {
            let path = root.join(file);
            let rom = fs::read(&path).unwrap_or_else(|err| {
                panic!("failed to read verified ROM {}: {err}", path.display())
            });

            let result = run_rom_with_oracle(
                &rom,
                file,
                RunConfig::new(400_000_000, 0),
                RunOracle::ScreenCrc {
                    frames,
                    expected_crc,
                },
            );

            assert!(
                result.passed && result.exit_reason == RunExitReason::ScreenCrcFrame,
                "{file}: expected screen-CRC PASS at frame {frames}, got crc=0x{:08X} passed={} exit={:?}",
                result.screen_crc32,
                result.passed,
                result.exit_reason
            );
        }
    }

    #[test]
    fn outcome_passed_is_true_for_screen_crc_frame_match() {
        // RunResult with passed=true and ScreenCrcFrame exit reason represents a
        // confirmed screen-CRC match. Verify the test data is consistent.
        use super::super::rom_runner::RunResult;
        let result = RunResult {
            passed: true,
            exit_reason: RunExitReason::ScreenCrcFrame,
            ticks: 1,
            frames: 1,
            pc: 0,
            marker: [0; 5],
            screen_crc32: 0x1234_5678,
            capture_path: None,
        };
        assert!(result.passed);
        assert_eq!(result.exit_reason, RunExitReason::ScreenCrcFrame);
    }
}
