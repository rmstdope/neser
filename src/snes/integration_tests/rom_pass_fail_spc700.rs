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
        ("4-test_ram_disable.smc", 600, 0x85F1_D154),
        ("test_ram_disable_ipl.smc", 600, 0xD001_765E),
        ("spc_smp.sfc", 2200, 0xEFD1_3576),
        ("spc_mem_access_times.sfc", 600, 0x3AC3_E30F),
        ("spc_timer.sfc", 600, 0x2497_38B2),
        ("test_speed.smc", 600, 0x8EAD_6D95),
        ("test_timer_speed_2.smc", 600, 0xC003_A7D0),
        ("test_timer_speed3.smc", 600, 0xD0FA_7627),
        ("test_timer_stop.smc", 600, 0x7CC2_B76B),
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
    // Additional ROM tests — a mix of:
    //   • Passing tests with real golden CRCs (no #[ignore]).
    //   • Failing tests with a placeholder CRC of 0x0000_0000 and #[ignore].
    //
    // For failing tests: when the emulator is fixed and the ROM prints
    // "Passed", run with NESER_CAPTURE_SCREEN=1 to capture the golden screen,
    // then replace the placeholder with the real CRC and remove the #[ignore].
    // -------------------------------------------------------------------------

    fn run_rom_with_expected_crc(file: &str, expected_crc: u32) {
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
                expected_crc,
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

    fn run_failing_rom(file: &str) {
        run_rom_with_expected_crc(file, 0x0000_0000);
    }

    #[test]
    fn blargg_spc_mem_access_times_passes() {
        run_rom_with_expected_crc("spc_mem_access_times.sfc", 0x3AC3_E30F);
    }

    #[test]
    #[ignore = "fails: DSP echo/basics (Failed 03) — fix emulator then update CRC"]
    fn blargg_spc_dsp6_passes() {
        run_failing_rom("spc_dsp6.sfc");
    }

    #[test]
    fn blargg_spc_timer_passes() {
        run_rom_with_expected_crc("spc_timer.sfc", 0x2497_38B2);
    }

    #[test]
    fn blargg_test_timer_speed_passes() {
        run_rom_with_expected_crc("test_timer_speed.smc", 0x65B1_1CE0);
    }

    #[test]
    fn blargg_test_timer_speed2_passes() {
        run_rom_with_expected_crc("test_timer_speed2.smc", 0x65B1_1CE0);
    }

    #[test]
    #[ignore = "fails: timer stop — fix emulator then update CRC"]
    fn blargg_test_timer_stop2_passes() {
        run_failing_rom("test_timer_stop2.smc");
    }

    #[test]
    #[ignore = "fails: timer at power/reset — see #2930 (H/V-IRQ dispatch delay fixed \
                in #2909, but the ROM's Passed/Failed decision path hasn't been fully \
                traced yet) — fix emulator then update CRC"]
    fn blargg_timer_at_power_reset_passes() {
        run_failing_rom("timer_at_power_reset.smc");
    }

    #[test]
    fn blargg_speed_2_freezes2_passes() {
        run_rom_with_expected_crc("speed_2_freezes2.smc", 0x6E1B_F905);
    }

    // -------------------------------------------------------------------------
    // Debug helper for #2911 (RAM-disable) and other open sub-issues of #2908.
    // Runs a failing ROM for a short period, then samples SPC PC at a stride
    // and prints the most-frequent regions plus a small ARAM dump around them.
    // Marked #[ignore] — only run manually with `--include-ignored`.
    // -------------------------------------------------------------------------

    fn investigate_failing_rom_spc_pc(file: &str, sample_stride_ticks: u64, samples: usize) {
        use crate::platform::app_context::AppContext;
        use crate::platform::debugging::{Tracing, init_tracing};
        use crate::platform::emulator::Emulator;
        use crate::snes::console::Snes;
        use std::collections::HashMap;

        // Enable APU port-write tracing for the first ~2M ticks of execution,
        // then disable so the output isn't drowned by the post-hang quiet.
        init_tracing(Tracing {
            enabled: true,
            apu: 3,
            ..Default::default()
        });

        let root = Path::new(ROM_PASS_FAIL_ROOT);
        let path = root.join(file);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));

        let mut snes = Snes::new(AppContext::default());
        snes.load_rom(&rom, file)
            .unwrap_or_else(|err| panic!("load failed: {err}"));

        // Sample SPC PC + host CPU PC + ports at fixed intervals from t=0.
        let mut spc_hist: HashMap<u16, u32> = HashMap::new();
        let mut cpu_hist: HashMap<u16, u32> = HashMap::new();
        let mut last_ports_combined: ([u8; 4], [u8; 4]) = ([0; 4], [0; 4]);
        let mut port_changes: Vec<(u64, [u8; 4], [u8; 4])> = Vec::new();
        let mut ticks: u64 = 0;
        let mut trace_disabled = false;

        for sample_idx in 0..samples {
            let target = ticks.saturating_add(sample_stride_ticks);
            while ticks < target {
                ticks = ticks.saturating_add(u64::from(snes.run_tick()));
            }
            if !trace_disabled && ticks > 2_000_000 {
                init_tracing(Tracing::default());
                trace_disabled = true;
                eprintln!("--- APU trace disabled at ticks={ticks} ---");
            }
            if let Some(pc) = snes.apu_spc_pc_for_debug() {
                *spc_hist.entry(pc).or_insert(0) += 1;
            }
            if let Some(pc) = snes.cpu_pc_for_tests() {
                *cpu_hist.entry(pc).or_insert(0) += 1;
            }
            // Read both directions of the 4 APU ports.
            let spc_to_cpu = snes.apu_spc_to_main_ports_for_debug().unwrap_or([0; 4]);
            let cpu_to_spc = snes.apu_main_to_spc_ports_for_debug().unwrap_or([0; 4]);
            let combined = (spc_to_cpu, cpu_to_spc);
            if combined != last_ports_combined {
                port_changes.push((ticks, spc_to_cpu, cpu_to_spc));
                last_ports_combined = combined;
            }
            // Print a progress marker every 10% of samples.
            if sample_idx > 0 && samples >= 10 && sample_idx % (samples / 10) == 0 {
                eprintln!(
                    "  [progress sample {sample_idx}/{samples}] ticks={ticks} \
                     spc_pc={:04X} cpu_pc={:04X} CPU->SPC={:02X?} SPC->CPU={:02X?}",
                    snes.apu_spc_pc_for_debug().unwrap_or(0),
                    snes.cpu_pc_for_tests().unwrap_or(0),
                    cpu_to_spc,
                    spc_to_cpu,
                );
            }
        }

        let mut top_spc: Vec<(u16, u32)> = spc_hist.into_iter().collect();
        top_spc.sort_by(|a, b| b.1.cmp(&a.1));
        let mut top_cpu: Vec<(u16, u32)> = cpu_hist.into_iter().collect();
        top_cpu.sort_by(|a, b| b.1.cmp(&a.1));

        eprintln!("\n=== SPC PC hotspots for {file} ({samples} samples) ===");
        for (pc, count) in top_spc.iter().take(10) {
            eprint!("  PC=${pc:04X}  hits={count:>4}  bytes:");
            for off in 0..8u16 {
                let b = snes
                    .apu_peek_spc_memory_for_debug(pc.wrapping_add(off))
                    .unwrap_or(0);
                eprint!(" {b:02X}");
            }
            eprintln!();
        }
        eprintln!("=== CPU PC hotspots ===");
        for (pc, count) in top_cpu.iter().take(10) {
            eprint!("  CPU PC=${pc:04X}  hits={count}  bytes:");
            let pc_full = *pc as u32;
            for off in 0..16u32 {
                let b = snes
                    .read_bus_for_debugger_for_tests(pc_full.wrapping_add(off))
                    .unwrap_or(0);
                eprint!(" {b:02X}");
            }
            eprintln!();
        }
        eprintln!("=== Port change timeline (last 30) ===");
        let start = port_changes.len().saturating_sub(30);
        for (t, spc_out, cpu_out) in &port_changes[start..] {
            eprintln!("  t={t:>12}  SPC->CPU={spc_out:02X?}  CPU->SPC={cpu_out:02X?}");
        }
        eprintln!("=== CPU code dump $8800-$8830 ===");
        for base in (0x8800u32..0x8830).step_by(8) {
            eprint!("  ${base:04X}:");
            for off in 0..8u32 {
                let b = snes
                    .read_bus_for_debugger_for_tests(base + off)
                    .unwrap_or(0);
                eprint!(" {b:02X}");
            }
            eprintln!();
        }
        eprintln!("=== CPU code dump $8200-$8330 ===");
        for base in (0x8200u32..0x8330).step_by(8) {
            eprint!("  ${base:04X}:");
            for off in 0..8u32 {
                let b = snes
                    .read_bus_for_debugger_for_tests(base + off)
                    .unwrap_or(0);
                eprint!(" {b:02X}");
            }
            eprintln!();
        }
        eprintln!("Total port changes: {}", port_changes.len());
        eprintln!("=== end ===\n");
    }

    #[test]
    #[ignore = "debug helper: run with --include-ignored to print SPC PC hotspots"]
    fn debug_spc_pc_hotspots_4_test_ram_disable() {
        investigate_failing_rom_spc_pc("4-test_ram_disable.smc", 50_000, 1000);
    }

    #[test]
    #[ignore = "debug helper: run with --include-ignored to print SPC PC hotspots"]
    fn debug_spc_pc_hotspots_test_ram_disable_ipl() {
        investigate_failing_rom_spc_pc("test_ram_disable_ipl.smc", 50_000, 1000);
    }
}
