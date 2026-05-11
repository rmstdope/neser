use super::helpers::{MooneyeResult, run_and_detect_cgb, run_and_detect_dmg};
use crate::gb::model::{CgbModel, DmgModel};

const BASE: &str = "roms/gb/automated_tests/SameSuite/apu";
const SAME_SUITE_CYCLE_LIMIT: u64 = 15_000_000;

#[derive(Clone, Copy)]
enum SameSuiteHardware {
    DmgB,
    Cgb(CgbModel),
}

fn run_samesuite_apu_rom(path: &str, hardware: SameSuiteHardware) -> MooneyeResult {
    match hardware {
        SameSuiteHardware::DmgB => run_and_detect_dmg(path, DmgModel::DmgB, SAME_SUITE_CYCLE_LIMIT),
        SameSuiteHardware::Cgb(model) => run_and_detect_cgb(path, model, SAME_SUITE_CYCLE_LIMIT),
    }
}

macro_rules! assert_samesuite_pass {
    ($path:expr, $hardware:expr) => {
        let path = $path;
        let result = run_samesuite_apu_rom(path, $hardware);
        assert_eq!(
            result,
            MooneyeResult::Pass,
            "SameSuite APU test failed: {:?} — ROM: {}",
            result,
            path
        );
    };
}

/// Macro for SameSuite APU tests that are now passing (no ignore).
macro_rules! samesuite_apu_test_enabled {
    ($name:ident, $path:expr, $hardware:expr) => {
        #[test]
        fn $name() {
            assert_samesuite_pass!($path, $hardware);
        }
    };
}

samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_align,
    &format!("{BASE}/channel_1/channel_1_align.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_align_cpu,
    &format!("{BASE}/channel_1/channel_1_align_cpu.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_delay,
    &format!("{BASE}/channel_1/channel_1_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_duty,
    &format!("{BASE}/channel_1/channel_1_duty.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_duty_delay,
    &format!("{BASE}/channel_1/channel_1_duty_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_extra_length_clocking_cgb0b,
    &format!("{BASE}/channel_1/channel_1_extra_length_clocking-cgb0B.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbB)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_freq_change,
    &format!("{BASE}/channel_1/channel_1_freq_change.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_freq_change_timing_cgb0bc,
    &format!("{BASE}/channel_1/channel_1_freq_change_timing-cgb0BC.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbB)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_freq_change_timing_cgbde,
    &format!("{BASE}/channel_1/channel_1_freq_change_timing-cgbDE.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_freq_change_timing_cgbd,
    &format!("{BASE}/channel_1/channel_1_freq_change_timing-cgbDE.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbD)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_nrx2_glitch,
    &format!("{BASE}/channel_1/channel_1_nrx2_glitch.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_nrx2_speed_change,
    &format!("{BASE}/channel_1/channel_1_nrx2_speed_change.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_restart,
    &format!("{BASE}/channel_1/channel_1_restart.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_restart_nrx2_glitch,
    &format!("{BASE}/channel_1/channel_1_restart_nrx2_glitch.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_stop_div,
    &format!("{BASE}/channel_1/channel_1_stop_div.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_stop_restart,
    &format!("{BASE}/channel_1/channel_1_stop_restart.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_sweep,
    &format!("{BASE}/channel_1/channel_1_sweep.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_sweep_restart,
    &format!("{BASE}/channel_1/channel_1_sweep_restart.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_sweep_restart_2,
    &format!("{BASE}/channel_1/channel_1_sweep_restart_2.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_volume,
    &format!("{BASE}/channel_1/channel_1_volume.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_1_volume_div,
    &format!("{BASE}/channel_1/channel_1_volume_div.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);

samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_2_align,
    &format!("{BASE}/channel_2/channel_2_align.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_2_align_cpu,
    &format!("{BASE}/channel_2/channel_2_align_cpu.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_2_delay,
    &format!("{BASE}/channel_2/channel_2_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_2_duty,
    &format!("{BASE}/channel_2/channel_2_duty.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_2_duty_delay,
    &format!("{BASE}/channel_2/channel_2_duty_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_2_extra_length_clocking_cgb0b,
    &format!("{BASE}/channel_2/channel_2_extra_length_clocking-cgb0B.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbB)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_2_freq_change,
    &format!("{BASE}/channel_2/channel_2_freq_change.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_2_nrx2_glitch,
    &format!("{BASE}/channel_2/channel_2_nrx2_glitch.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_2_nrx2_speed_change,
    &format!("{BASE}/channel_2/channel_2_nrx2_speed_change.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_2_restart,
    &format!("{BASE}/channel_2/channel_2_restart.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_2_restart_nrx2_glitch,
    &format!("{BASE}/channel_2/channel_2_restart_nrx2_glitch.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_2_stop_div,
    &format!("{BASE}/channel_2/channel_2_stop_div.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_2_stop_restart,
    &format!("{BASE}/channel_2/channel_2_stop_restart.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_2_volume,
    &format!("{BASE}/channel_2/channel_2_volume.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_2_volume_div,
    &format!("{BASE}/channel_2/channel_2_volume_div.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);

samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_3_and_glitch,
    &format!("{BASE}/channel_3/channel_3_and_glitch.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_3_delay,
    &format!("{BASE}/channel_3/channel_3_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_3_extra_length_clocking_cgb0,
    &format!("{BASE}/channel_3/channel_3_extra_length_clocking-cgb0.gb"),
    SameSuiteHardware::Cgb(CgbModel::Cgb0)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_3_extra_length_clocking_cgbb,
    &format!("{BASE}/channel_3/channel_3_extra_length_clocking-cgbB.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbB)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_3_first_sample,
    &format!("{BASE}/channel_3/channel_3_first_sample.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_3_freq_change_delay,
    &format!("{BASE}/channel_3/channel_3_freq_change_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_3_restart_delay,
    &format!("{BASE}/channel_3/channel_3_restart_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_3_restart_during_delay,
    &format!("{BASE}/channel_3/channel_3_restart_during_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_3_restart_stop_delay,
    &format!("{BASE}/channel_3/channel_3_restart_stop_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_3_shift_delay,
    &format!("{BASE}/channel_3/channel_3_shift_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_3_shift_skip_delay,
    &format!("{BASE}/channel_3/channel_3_shift_skip_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_3_stop_delay,
    &format!("{BASE}/channel_3/channel_3_stop_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_3_stop_div,
    &format!("{BASE}/channel_3/channel_3_stop_div.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_3_wave_ram_dac_on_rw,
    &format!("{BASE}/channel_3/channel_3_wave_ram_dac_on_rw.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_3_wave_ram_locked_write,
    &format!("{BASE}/channel_3/channel_3_wave_ram_locked_write.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_3_wave_ram_sync,
    &format!("{BASE}/channel_3/channel_3_wave_ram_sync.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);

samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_4_align,
    &format!("{BASE}/channel_4/channel_4_align.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_4_delay,
    &format!("{BASE}/channel_4/channel_4_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_4_equivalent_frequencies,
    &format!("{BASE}/channel_4/channel_4_equivalent_frequencies.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_4_extra_length_clocking_cgb0b,
    &format!("{BASE}/channel_4/channel_4_extra_length_clocking-cgb0B.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbB)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_4_freq_change,
    &format!("{BASE}/channel_4/channel_4_freq_change.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_4_frequency_alignment,
    &format!("{BASE}/channel_4/channel_4_frequency_alignment.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_4_lfsr,
    &format!("{BASE}/channel_4/channel_4_lfsr.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_4_lfsr15,
    &format!("{BASE}/channel_4/channel_4_lfsr15.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_4_lfsr_15_7,
    &format!("{BASE}/channel_4/channel_4_lfsr_15_7.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_4_lfsr_7_15,
    &format!("{BASE}/channel_4/channel_4_lfsr_7_15.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_4_lfsr_restart,
    &format!("{BASE}/channel_4/channel_4_lfsr_restart.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_4_lfsr_restart_fast,
    &format!("{BASE}/channel_4/channel_4_lfsr_restart_fast.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_channel_4_volume_div,
    &format!("{BASE}/channel_4/channel_4_volume_div.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);

samesuite_apu_test_enabled!(
    test_samesuite_apu_div_trigger_volume_10,
    &format!("{BASE}/div_trigger_volume_10.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_div_write_trigger,
    &format!("{BASE}/div_write_trigger.gb"),
    SameSuiteHardware::DmgB
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_div_write_trigger_10,
    &format!("{BASE}/div_write_trigger_10.gb"),
    SameSuiteHardware::DmgB
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_div_write_trigger_volume,
    &format!("{BASE}/div_write_trigger_volume.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);
samesuite_apu_test_enabled!(
    test_samesuite_apu_div_write_trigger_volume_10,
    &format!("{BASE}/div_write_trigger_volume_10.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE)
);

// ── Phase 7 / #2293 debug helper ─────────────────────────────────────────
//
// Runs a SameSuite CH1 sweep ROM, dumps the actual PCM12 sample bytes the
// ROM stored at $C000, and prints which 8-byte rows / individual sub-tests
// disagree with the expected `CorrectResults` table baked into each ROM.
// Helps narrow down WHICH sub-test (no-sweep round 1, sweep-round-2/3, the
// trigger-overflow guard rows, etc.) is failing so we can pinpoint the
// missing CGB-E quirk.

#[cfg(test)]
fn dump_samesuite_sweep_failures(rom_path: &str, expected: &[u8]) {
    use crate::gb::bus::GbBus;
    use crate::gb::integration_tests::helpers::{LD_B_B, load_cgb_rom_with_model};

    let mut gb = load_cgb_rom_with_model(rom_path, CgbModel::CgbE);
    let start = gb.cycles();
    // Run until the LD B,B breakpoint or cycle limit.
    loop {
        let opcode = gb.cpu.bus.read(gb.cpu.regs.pc);
        if opcode == LD_B_B {
            break;
        }
        if gb.cycles().saturating_sub(start) >= SAME_SUITE_CYCLE_LIMIT {
            panic!("ROM hit cycle limit without reaching breakpoint");
        }
        gb.step();
    }
    // Dump WRAM[$C000..$C000+expected.len()] and compare.
    let mut mismatches = Vec::new();
    for (i, &want) in expected.iter().enumerate() {
        let got = gb.cpu.bus.read(0xC000 + i as u16);
        if got != want {
            mismatches.push((i, want, got));
        }
    }
    if mismatches.is_empty() {
        println!("ALL {} SUBTESTS MATCH ✓", expected.len());
        return;
    }
    println!(
        "MISMATCHES: {}/{} sub-tests",
        mismatches.len(),
        expected.len()
    );
    for (i, want, got) in &mismatches {
        let row = i / 8;
        let col = i % 8;
        println!(
            "  idx={:3} (row {:2}, col {}): expected 0x{:02X}, got 0x{:02X}",
            i, row, col, want, got
        );
    }
}

#[test]
#[ignore = "debug-only: dumps PCM12 sub-test results for #2293 investigation"]
fn debug_dump_channel_1_sweep_failures() {
    // CorrectResults from channel_1_sweep.asm — 18 rows × 8 bytes = 144.
    #[rustfmt::skip]
    let expected: &[u8] = &[
        0x00, 0x00, 0x08, 0x08, 0x08, 0x08, 0x00, 0x00,
        0x00, 0x00, 0x08, 0x08, 0x08, 0x08, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x08, 0x08, 0x08, 0x08,
        0x00, 0x00, 0x00, 0x00, 0x08, 0x08, 0x08, 0x08,
        0x00, 0x00, 0x00, 0x00, 0x08, 0x08, 0x08, 0x08,
        0x00, 0x00, 0x08, 0x08, 0x08, 0x08, 0x00, 0x00,

        0x00, 0x00, 0x08, 0x08, 0x08, 0x08, 0x00, 0x00,
        0x00, 0x00, 0x08, 0x08, 0x08, 0x08, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x08, 0x08, 0x08, 0x08,
        0x00, 0x00, 0x00, 0x00, 0x08, 0x08, 0x08, 0x08,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08,

        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x08, 0x08,
        0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08,
        0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    dump_samesuite_sweep_failures(&format!("{BASE}/channel_1/channel_1_sweep.gb"), expected);
}

#[test]
#[ignore = "debug only"]
fn test_debug_shift_delay_dump() {
    use super::helpers::run_cgb_and_dump;
    let path = format!("{BASE}/channel_3/channel_3_shift_delay.gb");
    let (result, buf) = run_cgb_and_dump::<8>(&path, CgbModel::CgbE, 15_000_000, 0xC000);
    println!("Result: {:?}", result);
    println!("Actual:   {:02X?}", buf);
    println!("Expected: [01, 01, 01, 01, 01, 03, 03, 03]");
    // \1 values: $0,$1,$7E,$7F,$80,$81,$82,$83
    let subtests = [0u16, 1, 0x7E, 0x7F, 0x80, 0x81, 0x82, 0x83];
    let expected = [0x01u8, 0x01, 0x01, 0x01, 0x01, 0x03, 0x03, 0x03];
    for i in 0..8 {
        let ok = if buf[i] == expected[i] { "✓" } else { "✗" };
        println!(
            "  [{i}] \\1={:#04X} expected={:#04X} actual={:#04X} {ok}",
            subtests[i], expected[i], buf[i]
        );
    }
}

#[test]
#[ignore = "debug only"]
fn test_debug_freq_change_delay_dump() {
    use super::helpers::run_cgb_and_dump;
    let path = format!("{BASE}/channel_3/channel_3_freq_change_delay.gb");
    let (result, buf) = run_cgb_and_dump::<16>(&path, CgbModel::CgbE, 15_000_000, 0xC000);
    println!("Result: {:?}", result);
    println!("Actual:   {:02X?}", buf);
    println!("Expected: [00, 00, 00, 00, 00, 00, 00, 0F,  00, 00, 0F, 0F, 0F, 0F, 0F, 0F]");
}

#[test]
#[ignore = "debug only"]
fn test_debug_first_sample_dump() {
    use super::helpers::run_cgb_and_dump;
    let path = format!("{BASE}/channel_3/channel_3_first_sample.gb");
    let (result, buf) = run_cgb_and_dump::<8>(&path, CgbModel::CgbE, 15_000_000, 0xC000);
    println!("Result: {:?}", result);
    println!("Actual:   {:02X?}", buf);
    println!("Expected: [00, 00, 00, 00, 00, 0E, 0E, 0E]");
}
