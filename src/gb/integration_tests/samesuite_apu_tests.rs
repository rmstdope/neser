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

macro_rules! samesuite_apu_test {
    ($name:ident, $path:expr, $hardware:expr, $reason:expr) => {
        #[test]
        #[ignore = $reason]
        fn $name() {
            assert_samesuite_pass!($path, $hardware);
        }
    };
}

samesuite_apu_test!(
    test_samesuite_apu_channel_1_align,
    &format!("{BASE}/channel_1/channel_1_align.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_1_align_cpu,
    &format!("{BASE}/channel_1/channel_1_align_cpu.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_1_delay,
    &format!("{BASE}/channel_1/channel_1_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_1_duty,
    &format!("{BASE}/channel_1/channel_1_duty.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_1_duty_delay,
    &format!("{BASE}/channel_1/channel_1_duty_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_1_extra_length_clocking_cgb0b,
    &format!("{BASE}/channel_1/channel_1_extra_length_clocking-cgb0B.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbB),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_1_freq_change,
    &format!("{BASE}/channel_1/channel_1_freq_change.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_1_freq_change_timing_cgb0bc,
    &format!("{BASE}/channel_1/channel_1_freq_change_timing-cgb0BC.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbC),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_1_freq_change_timing_cgbde,
    &format!("{BASE}/channel_1/channel_1_freq_change_timing-cgbDE.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbD),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_1_nrx2_glitch,
    &format!("{BASE}/channel_1/channel_1_nrx2_glitch.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_1_nrx2_speed_change,
    &format!("{BASE}/channel_1/channel_1_nrx2_speed_change.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_1_restart,
    &format!("{BASE}/channel_1/channel_1_restart.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_1_restart_nrx2_glitch,
    &format!("{BASE}/channel_1/channel_1_restart_nrx2_glitch.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_1_stop_div,
    &format!("{BASE}/channel_1/channel_1_stop_div.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_1_stop_restart,
    &format!("{BASE}/channel_1/channel_1_stop_restart.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_1_sweep,
    &format!("{BASE}/channel_1/channel_1_sweep.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_1_sweep_restart,
    &format!("{BASE}/channel_1/channel_1_sweep_restart.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_1_sweep_restart_2,
    &format!("{BASE}/channel_1/channel_1_sweep_restart_2.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_1_volume,
    &format!("{BASE}/channel_1/channel_1_volume.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_1_volume_div,
    &format!("{BASE}/channel_1/channel_1_volume_div.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);

samesuite_apu_test!(
    test_samesuite_apu_channel_2_align,
    &format!("{BASE}/channel_2/channel_2_align.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_2_align_cpu,
    &format!("{BASE}/channel_2/channel_2_align_cpu.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_2_delay,
    &format!("{BASE}/channel_2/channel_2_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_2_duty,
    &format!("{BASE}/channel_2/channel_2_duty.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_2_duty_delay,
    &format!("{BASE}/channel_2/channel_2_duty_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_2_extra_length_clocking_cgb0b,
    &format!("{BASE}/channel_2/channel_2_extra_length_clocking-cgb0B.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbB),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_2_freq_change,
    &format!("{BASE}/channel_2/channel_2_freq_change.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_2_nrx2_glitch,
    &format!("{BASE}/channel_2/channel_2_nrx2_glitch.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_2_nrx2_speed_change,
    &format!("{BASE}/channel_2/channel_2_nrx2_speed_change.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_2_restart,
    &format!("{BASE}/channel_2/channel_2_restart.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_2_restart_nrx2_glitch,
    &format!("{BASE}/channel_2/channel_2_restart_nrx2_glitch.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_2_stop_div,
    &format!("{BASE}/channel_2/channel_2_stop_div.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_2_stop_restart,
    &format!("{BASE}/channel_2/channel_2_stop_restart.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_2_volume,
    &format!("{BASE}/channel_2/channel_2_volume.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_2_volume_div,
    &format!("{BASE}/channel_2/channel_2_volume_div.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);

samesuite_apu_test!(
    test_samesuite_apu_channel_3_and_glitch,
    &format!("{BASE}/channel_3/channel_3_and_glitch.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_3_delay,
    &format!("{BASE}/channel_3/channel_3_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_3_extra_length_clocking_cgb0,
    &format!("{BASE}/channel_3/channel_3_extra_length_clocking-cgb0.gb"),
    SameSuiteHardware::Cgb(CgbModel::Cgb0),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_3_extra_length_clocking_cgbb,
    &format!("{BASE}/channel_3/channel_3_extra_length_clocking-cgbB.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbB),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_3_first_sample,
    &format!("{BASE}/channel_3/channel_3_first_sample.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_3_freq_change_delay,
    &format!("{BASE}/channel_3/channel_3_freq_change_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_3_restart_delay,
    &format!("{BASE}/channel_3/channel_3_restart_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_3_restart_during_delay,
    &format!("{BASE}/channel_3/channel_3_restart_during_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_3_restart_stop_delay,
    &format!("{BASE}/channel_3/channel_3_restart_stop_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_3_shift_delay,
    &format!("{BASE}/channel_3/channel_3_shift_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_3_shift_skip_delay,
    &format!("{BASE}/channel_3/channel_3_shift_skip_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_3_stop_delay,
    &format!("{BASE}/channel_3/channel_3_stop_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_3_stop_div,
    &format!("{BASE}/channel_3/channel_3_stop_div.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_3_wave_ram_dac_on_rw,
    &format!("{BASE}/channel_3/channel_3_wave_ram_dac_on_rw.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_3_wave_ram_locked_write,
    &format!("{BASE}/channel_3/channel_3_wave_ram_locked_write.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_3_wave_ram_sync,
    &format!("{BASE}/channel_3/channel_3_wave_ram_sync.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);

samesuite_apu_test!(
    test_samesuite_apu_channel_4_align,
    &format!("{BASE}/channel_4/channel_4_align.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_4_delay,
    &format!("{BASE}/channel_4/channel_4_delay.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_4_equivalent_frequencies,
    &format!("{BASE}/channel_4/channel_4_equivalent_frequencies.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_4_extra_length_clocking_cgb0b,
    &format!("{BASE}/channel_4/channel_4_extra_length_clocking-cgb0B.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbB),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_4_freq_change,
    &format!("{BASE}/channel_4/channel_4_freq_change.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_4_frequency_alignment,
    &format!("{BASE}/channel_4/channel_4_frequency_alignment.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_4_lfsr,
    &format!("{BASE}/channel_4/channel_4_lfsr.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_4_lfsr15,
    &format!("{BASE}/channel_4/channel_4_lfsr15.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_4_lfsr_15_7,
    &format!("{BASE}/channel_4/channel_4_lfsr_15_7.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_4_lfsr_7_15,
    &format!("{BASE}/channel_4/channel_4_lfsr_7_15.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_4_lfsr_restart,
    &format!("{BASE}/channel_4/channel_4_lfsr_restart.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_4_lfsr_restart_fast,
    &format!("{BASE}/channel_4/channel_4_lfsr_restart_fast.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_channel_4_volume_div,
    &format!("{BASE}/channel_4/channel_4_volume_div.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);

samesuite_apu_test!(
    test_samesuite_apu_div_trigger_volume_10,
    &format!("{BASE}/div_trigger_volume_10.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_div_write_trigger,
    &format!("{BASE}/div_write_trigger.gb"),
    SameSuiteHardware::DmgB,
    "Known SameSuite APU failure on neser DMG-B; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_div_write_trigger_10,
    &format!("{BASE}/div_write_trigger_10.gb"),
    SameSuiteHardware::DmgB,
    "Known SameSuite APU failure on neser DMG-B; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_div_write_trigger_volume,
    &format!("{BASE}/div_write_trigger_volume.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
samesuite_apu_test!(
    test_samesuite_apu_div_write_trigger_volume_10,
    &format!("{BASE}/div_write_trigger_volume_10.gb"),
    SameSuiteHardware::Cgb(CgbModel::CgbE),
    "Known SameSuite APU CGB gap on neser; tracked under issue #2038"
);
