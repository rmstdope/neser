use super::rom_runner::{RunConfig, RunExitReason, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const ROM_PASS_FAIL_ROOT: &str = "roms/snes/automated_tests/rom_pass_fail/blargg_spc_apu/v1";

#[cfg(test)]
mod tests {
    use super::*;

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

    // -------------------------------------------------------------------------
    // ROMs that currently FAIL — committed and tracked, ignored until fixed.
    //
    // Each test uses a placeholder expected_crc of 0x0000_0000. When the
    // emulator is fixed and the ROM prints "Passed", run with
    // NESER_CAPTURE_SCREEN=1 to capture the golden screen, then replace the
    // placeholder with the real CRC and remove the #[ignore].
    // -------------------------------------------------------------------------

    fn run_failing_rom(file: &str) {
        let root = Path::new(ROM_PASS_FAIL_ROOT);
        let path = root.join(file);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let result = run_rom_with_oracle(
            &rom,
            file,
            RunConfig::new(400_000_000, 0),
            RunOracle::ScreenCrc {
                frames: 600,
                expected_crc: 0x0000_0000, // placeholder — update once Passed
            },
        );
        assert!(
            result.passed && result.exit_reason == RunExitReason::ScreenCrcFrame,
            "{file}: expected screen-CRC PASS at frame 600, \
             got crc=0x{:08X} passed={} exit={:?}",
            result.screen_crc32,
            result.passed,
            result.exit_reason
        );
    }

    #[test]
    #[ignore = "fails: APU RAM disable reports code CC — fix emulator then update CRC"]
    fn blargg_4_test_ram_disable_passes() {
        run_failing_rom("4-test_ram_disable.smc");
    }

    #[test]
    #[ignore = "fails: APU RAM/IPL disable — fix emulator then update CRC"]
    fn blargg_test_ram_disable_ipl_passes() {
        run_failing_rom("test_ram_disable_ipl.smc");
    }

    #[test]
    #[ignore = "fails: SMP behavior (Failed 02) — fix emulator then update CRC"]
    fn blargg_spc_smp_passes() {
        run_failing_rom("spc_smp.sfc");
    }

    #[test]
    #[ignore = "fails: SPC memory access timing — fix emulator then update CRC"]
    fn blargg_spc_mem_access_times_passes() {
        run_failing_rom("spc_mem_access_times.sfc");
    }

    #[test]
    #[ignore = "fails: DSP echo/basics (Failed 03) — fix emulator then update CRC"]
    fn blargg_spc_dsp6_passes() {
        run_failing_rom("spc_dsp6.sfc");
    }

    #[test]
    #[ignore = "fails: SPC timer (Failed 02) — fix emulator then update CRC"]
    fn blargg_spc_timer_passes() {
        run_failing_rom("spc_timer.sfc");
    }

    #[test]
    #[ignore = "fails: timer speed — fix emulator then update CRC"]
    fn blargg_test_timer_speed_passes() {
        run_failing_rom("test_timer_speed.smc");
    }

    #[test]
    #[ignore = "fails: timer speed — fix emulator then update CRC"]
    fn blargg_test_timer_speed2_passes() {
        run_failing_rom("test_timer_speed2.smc");
    }

    #[test]
    #[ignore = "fails: timer stop — fix emulator then update CRC"]
    fn blargg_test_timer_stop_passes() {
        run_failing_rom("test_timer_stop.smc");
    }

    #[test]
    #[ignore = "fails: timer stop — fix emulator then update CRC"]
    fn blargg_test_timer_stop2_passes() {
        run_failing_rom("test_timer_stop2.smc");
    }

    #[test]
    #[ignore = "fails: timer at power/reset — fix emulator then update CRC"]
    fn blargg_timer_at_power_reset_passes() {
        run_failing_rom("timer_at_power_reset.smc");
    }

    #[test]
    #[ignore = "fails: SPC speed/freeze — fix emulator then update CRC"]
    fn blargg_speed_2_freezes2_passes() {
        run_failing_rom("speed_2_freezes2.smc");
    }
}
