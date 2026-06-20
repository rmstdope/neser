use super::Sdsp;

#[test]
fn given_phase_31_when_step_phase_then_wraps_to_0() {
    let mut dsp = Sdsp::new();
    for _ in 0..31 {
        dsp.step_phase();
    }
    assert_eq!(dsp.phase(), 31);

    dsp.step_phase();
    assert_eq!(dsp.phase(), 0);
}

#[test]
fn given_all_register_addresses_when_written_then_reads_back_same_value() {
    let mut dsp = Sdsp::new();
    for addr in 0u8..=0x7F {
        let value = addr.wrapping_mul(3).wrapping_add(1);
        dsp.write_reg(addr, value);
    }

    for addr in 0u8..=0x7F {
        let value = addr.wrapping_mul(3).wrapping_add(1);
        assert_eq!(dsp.read_reg(addr), value, "addr=0x{addr:02X}");
    }
}

#[test]
fn given_mirrored_register_addresses_when_written_then_base_registers_match() {
    let mut dsp = Sdsp::new();

    dsp.write_reg(0x95, 0xAB);
    assert_eq!(dsp.read_reg(0x15), 0xAB);
    assert_eq!(dsp.read_reg(0x95), 0xAB);
}

#[test]
fn given_brr_header_with_loop_and_end_bits_when_decoded_then_flags_are_exposed() {
    let header = 0b0000_0011;
    let decoded = Sdsp::decode_brr_block(header, [0; 8], 0, 0);
    assert!(decoded.loop_flag);
    assert!(decoded.end_flag);
}

#[test]
fn given_filter0_shift0_nibbles_when_decoded_then_positive_and_negative_samples_survive() {
    let header = 0b0000_0000;
    let mut data = [0u8; 8];
    data[0] = 0x78;
    data[1] = 0x0F;

    let decoded = Sdsp::decode_brr_block(header, data, 0, 0);

    assert_eq!(decoded.samples[0], 7);
    assert_eq!(decoded.samples[1], -8);
    assert_eq!(decoded.samples[2], 0);
    assert_eq!(decoded.samples[3], -1);
}

#[test]
fn given_voice_pitch_when_step_voice_pitch_then_sample_position_advances() {
    let mut dsp = Sdsp::new();
    dsp.set_voice_pitch(2, 0x1234);

    dsp.step_voice_pitch(2);
    assert_eq!(dsp.voice_sample_pos(2), 0x1234);
}

#[test]
fn given_constant_samples_when_gaussian_interpolating_then_output_preserves_level() {
    let dsp = Sdsp::new();
    let out = dsp.gaussian_interpolate(100, 100, 100, 100, 64);
    assert!((out - 100).abs() <= 2);
    assert_eq!(out & 1, 0);
}

#[test]
fn given_voice_and_master_volume_when_mixing_then_both_are_applied() {
    let mut dsp = Sdsp::new();
    dsp.set_voice_volume(0, 64, 32);
    dsp.set_master_volume(127, 127);

    let (left, right) = dsp.mix_voice_sample(0, 1000);
    assert_eq!(left, 496);
    assert_eq!(right, 248);
}

#[test]
fn normalize_after_restore_rebuilds_cached_pitch_and_volume_fields() {
    let mut dsp = Sdsp::new();
    dsp.write_reg(0x00, 64);
    dsp.write_reg(0x01, 32);
    dsp.write_reg(0x0C, 127);
    dsp.write_reg(0x1C, 127);
    dsp.write_reg(0x02, 0x34);
    dsp.write_reg(0x03, 0x12);

    dsp.voices[0].vol_l = 0;
    dsp.voices[0].vol_r = 0;
    dsp.master_vol_l = 0;
    dsp.master_vol_r = 0;
    dsp.voices[0].pitch = 0;

    dsp.normalize_after_restore()
        .expect("normalize should rebuild cached fields");
    let (left, right) = dsp.mix_voice_sample(0, 1000);
    assert_eq!(left, 496);
    assert_eq!(right, 248);

    dsp.step_voice_pitch(0);
    assert_eq!(dsp.voice_sample_pos(0), 0x1234);
}

#[test]
fn normalize_after_restore_masks_phase_to_5_bits() {
    let mut dsp = Sdsp::new();
    dsp.phase = 0xFF;
    dsp.normalize_after_restore()
        .expect("normalize should accept default register file");
    assert_eq!(dsp.phase(), 0x1F);
}

#[test]
fn given_kon_for_adsr_voice_when_latency_passes_then_envx_becomes_non_zero() {
    let mut dsp = Sdsp::new();
    dsp.write_reg(0x05, 0x8F); // ADSR1: ADSR enabled, fastest attack
    dsp.write_reg(0x06, 0xE0); // ADSR2: high sustain level, slow sustain
    dsp.write_reg(0x4C, 0x01); // KON voice 0

    for _ in 0..4 {
        dsp.step_phase();
    }
    assert_eq!(
        dsp.read_reg(0x08),
        0x00,
        "ENVX must still be zero before KON latency elapses"
    );

    for _ in 0..8 {
        dsp.step_phase();
    }
    assert!(
        dsp.read_reg(0x08) > 0,
        "ENVX should rise after KON latency and attack progression"
    );
}

#[test]
fn given_active_adsr_voice_when_koff_then_envx_decreases() {
    let mut dsp = Sdsp::new();
    dsp.write_reg(0x05, 0x8F);
    dsp.write_reg(0x06, 0xE0);
    dsp.write_reg(0x4C, 0x01);
    for _ in 0..20 {
        dsp.step_phase();
    }
    let before = dsp.read_reg(0x08);
    assert!(
        before > 0,
        "precondition: ENVX should be non-zero after KON attack"
    );

    dsp.write_reg(0x5C, 0x01); // KOFF voice 0
    for _ in 0..8 {
        dsp.step_phase();
    }
    let after = dsp.read_reg(0x08);
    assert!(
        after < before,
        "ENVX should fall after KOFF enters release state"
    );
}

#[test]
fn given_non_voice_when_noise_clock_ticks_then_outx_changes_from_silence() {
    let mut dsp = Sdsp::new();
    dsp.write_reg(0x0C, 127);
    dsp.write_reg(0x1C, 127);
    dsp.write_reg(0x00, 127);
    dsp.write_reg(0x01, 127);
    dsp.write_reg(0x07, 0x7F); // direct gain for non-zero envelope
    dsp.write_reg(0x3D, 0x01); // NON voice 0
    dsp.write_reg(0x6C, 0x00); // fastest noise clock in this implementation

    let before = dsp.read_reg(0x09);
    for _ in 0..8 {
        dsp.step_phase();
    }
    let after = dsp.read_reg(0x09);
    assert_ne!(
        after, before,
        "noise-routed OUTX should evolve as LFSR advances"
    );
}

#[test]
fn given_pmon_enabled_for_voice1_when_voice0_outx_nonzero_then_voice1_pitch_step_is_modulated() {
    let mut dsp = Sdsp::new();
    dsp.write_reg(0x0C, 127);
    dsp.write_reg(0x1C, 127);
    dsp.write_reg(0x00, 127);
    dsp.write_reg(0x01, 127);
    dsp.write_reg(0x08, 0x7F);
    dsp.write_reg(0x12, 0x00); // voice1 pitch low
    dsp.write_reg(0x13, 0x10); // voice1 pitch high => 0x1000

    dsp.step_phase();
    let base_step = dsp.voice_sample_pos(1);

    let mut modulated = Sdsp::new();
    modulated.write_reg(0x0C, 127);
    modulated.write_reg(0x1C, 127);
    modulated.write_reg(0x00, 127);
    modulated.write_reg(0x01, 127);
    modulated.write_reg(0x08, 0x7F);
    modulated.write_reg(0x12, 0x00);
    modulated.write_reg(0x13, 0x10);
    modulated.write_reg(0x2D, 0x02); // PMON voice 1
    modulated.step_phase();
    let mod_step = modulated.voice_sample_pos(1);

    assert_ne!(
        mod_step, base_step,
        "PMON should alter voice1 pitch accumulation when voice0 OUTX is non-zero"
    );
}

#[test]
fn given_koff_during_kon_delay_when_latency_would_expire_then_voice_stays_released() {
    let mut dsp = Sdsp::new();
    dsp.write_reg(0x05, 0x8F);
    dsp.write_reg(0x06, 0xE0);
    dsp.write_reg(0x4C, 0x01);
    dsp.write_reg(0x5C, 0x01);

    for _ in 0..16 {
        dsp.step_phase();
    }
    assert_eq!(
        dsp.read_reg(0x08),
        0,
        "KOFF should cancel pending KON attack start"
    );
}

#[test]
fn given_pmon_enabled_when_master_volume_zero_then_pitch_modulation_still_applies() {
    let mut base = Sdsp::new();
    base.write_reg(0x00, 127);
    base.write_reg(0x01, 127);
    base.write_reg(0x08, 0x7F);
    base.write_reg(0x12, 0x00);
    base.write_reg(0x13, 0x10);
    base.step_phase();
    let base_step = base.voice_sample_pos(1);

    let mut modulated = Sdsp::new();
    modulated.write_reg(0x00, 127);
    modulated.write_reg(0x01, 127);
    modulated.write_reg(0x08, 0x7F);
    modulated.write_reg(0x12, 0x00);
    modulated.write_reg(0x13, 0x10);
    modulated.write_reg(0x0C, 0x00);
    modulated.write_reg(0x1C, 0x00);
    modulated.write_reg(0x2D, 0x02);
    modulated.step_phase();
    let mod_step = modulated.voice_sample_pos(1);

    assert_ne!(
        mod_step, base_step,
        "PMON source must be independent from post-mix master volume"
    );
}

#[test]
fn legacy_deserialization_without_new_voice_fields_uses_defaults() {
    let mut dsp: Sdsp =
        serde_json::from_str(r#"{"phase":255,"regs":[]}"#).expect("legacy-compatible decode");
    dsp.normalize_after_restore()
        .expect("normalization should accept defaulted new fields");
    assert_eq!(dsp.phase(), 0x1F);
}
