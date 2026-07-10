use super::rom_runner::{RunConfig, RunExitReason, RunOracle, run_rom_with_oracle};
use std::fs;
use std::path::Path;

const ROM_PASS_FAIL_ROOT: &str = "roms/snes/automated_tests/blargg_apu";

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs a vendored blargg SPC700/APU ROM and asserts that the screen
    /// rendered at `frames` matches the visually-approved PASS golden CRC32.
    ///
    /// To approve a new golden, run with NESER_CAPTURE_SCREEN=1, visually
    /// confirm the capture under target/snes_test_captures/ shows a PASS
    /// screen, then record the (frame, CRC) here.
    fn run_rom_screen_crc(file: &str, frames: u32, expected_crc: u32, config: RunConfig) {
        let path = Path::new(ROM_PASS_FAIL_ROOT).join(file);
        let rom = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
        let result = run_rom_with_oracle(
            &rom,
            file,
            "blargg_apu_tests",
            config,
            RunOracle::ScreenCrc {
                frames,
                expected_crc,
            },
        );
        assert!(
            result.passed && result.exit_reason == RunExitReason::ScreenCrcFrame,
            "{file}: expected screen-CRC PASS at frame {frames}, \
             got crc=0x{:08X} passed={} exit={:?}",
            result.screen_crc32,
            result.passed,
            result.exit_reason
        );
    }

    /// Declares one `#[test]` per ROM. The four-argument form uses the
    /// default run budget; pass an explicit `RunConfig` as a fifth argument
    /// for ROMs that need a larger tick budget or a mid-test reset trap.
    macro_rules! blargg_rom_test {
        ($name:ident, $file:expr, $frames:expr, $crc:expr) => {
            blargg_rom_test!($name, $file, $frames, $crc, RunConfig::new(400_000_000, 0));
        };
        ($name:ident, $file:expr, $frames:expr, $crc:expr, $config:expr) => {
            #[test]
            fn $name() {
                run_rom_screen_crc($file, $frames, $crc, $config);
            }
        };
    }

    blargg_rom_test!(
        blargg_1_test_exec_from_io_passes,
        "1-test_exec_from_io.smc",
        600,
        0x31D3_DB73
    );

    blargg_rom_test!(
        blargg_2_test_single_instr_passes,
        "2-test_single_instr.smc",
        600,
        0x880D_ED35
    );

    blargg_rom_test!(
        blargg_3_test_write_disable_passes,
        "3-test_write_disable.smc",
        600,
        0xF638_BAD0
    );

    blargg_rom_test!(
        blargg_4_test_ram_disable_passes,
        "4-test_ram_disable.smc",
        600,
        0x9BC6_1B79
    );

    blargg_rom_test!(
        blargg_test_ram_disable_ipl_passes,
        "test_ram_disable_ipl.smc",
        600,
        0x5623_82B1
    );

    blargg_rom_test!(blargg_spc_smp_passes, "spc_smp.sfc", 2200, 0x5D6F_C4EA);

    blargg_rom_test!(
        blargg_spc_mem_access_times_passes,
        "spc_mem_access_times.sfc",
        600,
        0x1B4A_C6BA
    );

    // Full suite (KON, Misc, Order, Random and Timing batteries) ends with
    // "PASSED TESTS" on a blue background just before frame 9000; the
    // default 400M-tick budget stops short of that.
    blargg_rom_test!(
        blargg_spc_dsp6_passes,
        "spc_dsp6.sfc",
        9100,
        0xD52F_0829,
        RunConfig::new(600_000_000, 0)
    );

    blargg_rom_test!(blargg_spc_timer_passes, "spc_timer.sfc", 600, 0xAA7E_CF03);

    // Golden re-approved for #2938: with trampoline micro-ops cycle-scripted
    // and the stepper charging TEST wait states, every row now matches Mesen
    // exactly (fast 252/251, waited 126 and 26).
    blargg_rom_test!(blargg_test_speed_passes, "test_speed.smc", 600, 0x7BA3_BF97);

    // Golden re-approved for #2914 (cycle-stepped port polling, the hardware
    // 32040 Hz SPC rate + CPU->SPC write latch, then the exact Mesen
    // master-clock denominator 21,477,270); screen reads "Passed" and every
    // measured row now matches Mesen exactly.
    blargg_rom_test!(
        blargg_test_timer_speed_passes,
        "test_timer_speed.smc",
        600,
        0x2B6A_56F0
    );

    // Same re-approval as test_timer_speed (identical output screen).
    blargg_rom_test!(
        blargg_test_timer_speed2_passes,
        "test_timer_speed2.smc",
        600,
        0x2B6A_56F0
    );

    // Golden re-approved for #2914 (see test_timer_speed note); measured
    // values match Mesen's within ±1 (still "Passed").
    blargg_rom_test!(
        blargg_test_timer_speed_2_passes,
        "test_timer_speed_2.smc",
        600,
        0x6E5A_EFDF
    );

    // Golden re-approved for #2914 (see test_timer_speed_2 note); "Done"
    // measurement screen, rows match Mesen within ±1.
    blargg_rom_test!(
        blargg_test_timer_speed3_passes,
        "test_timer_speed3.smc",
        600,
        0x2A5A_79CF
    );

    blargg_rom_test!(
        blargg_test_timer_stop_passes,
        "test_timer_stop.smc",
        600,
        0x7DFB_5542
    );

    blargg_rom_test!(
        blargg_test_timer_stop2_passes,
        "test_timer_stop2.smc",
        600,
        0xA33B_ED78
    );

    // The ROM jumps to $0000 mid-test to request a soft reset; model it with
    // a reset-on-PC trap.
    blargg_rom_test!(
        blargg_timer_at_power_reset_passes,
        "timer_at_power_reset.smc",
        600,
        0x84EA_9009,
        RunConfig::new(400_000_000, 0).with_reset_on_pc_trap(0x0000)
    );

    blargg_rom_test!(
        blargg_speed_2_freezes2_passes,
        "speed_2_freezes2.smc",
        600,
        0x6F17_26D2
    );
}
