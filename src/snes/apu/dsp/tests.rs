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
    let is_read_only_status = |addr: u8| -> bool {
        let voice = addr >> 4;
        addr == 0x7C || (voice < 8 && matches!(addr & 0x0F, 0x08 | 0x09))
    };
    for addr in 0u8..=0x7F {
        let value = addr.wrapping_mul(3).wrapping_add(1);
        dsp.write_reg(addr, value);
    }

    for addr in 0u8..=0x7F {
        if is_read_only_status(addr) {
            continue;
        }
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
fn given_filter0_shift0_nibbles_when_decoded_then_samples_are_halved_to_15_bit_scale() {
    let header = 0b0000_0000;
    let mut data = [0u8; 8];
    data[0] = 0x78;
    data[1] = 0x0F;

    let decoded = Sdsp::decode_brr_block(header, data, 0, 0);

    assert_eq!(decoded.samples[0], 3);
    assert_eq!(decoded.samples[1], -4);
    assert_eq!(decoded.samples[2], 0);
    assert_eq!(decoded.samples[3], -1);
}

#[test]
fn given_filter0_shift12_nibbles_when_decoded_then_samples_are_15_bit_scaled() {
    let header = 0b1100_0000;
    let mut data = [0u8; 8];
    data[0] = 0x78;

    let decoded = Sdsp::decode_brr_block(header, data, 0, 0);

    assert_eq!(decoded.samples[0], 0x3800);
    assert_eq!(decoded.samples[1], -0x4000);
}

#[test]
fn given_reserved_shift_nibbles_when_decoded_then_negative_samples_use_shift12_sign_fill() {
    let header = 0b1111_0000;
    let mut data = [0u8; 8];
    data[0] = 0x78;

    let decoded = Sdsp::decode_brr_block(header, data, 0, 0);

    assert_eq!(decoded.samples[0], 0);
    assert_eq!(decoded.samples[1], -2048);
}

#[test]
fn given_filter2_negative_history_when_decoded_then_fullsnes_signed_rounding_is_used() {
    let header = 0b0000_1000;
    let data = [0u8; 8];

    let decoded = Sdsp::decode_brr_block(header, data, -64, -63);

    assert_eq!(decoded.samples[0], -63);
}

#[test]
fn given_filter3_negative_history_when_decoded_then_fullsnes_signed_rounding_is_used() {
    let header = 0b0000_1100;
    let data = [0u8; 8];

    let decoded = Sdsp::decode_brr_block(header, data, -64, -63);

    assert_eq!(decoded.samples[0], -64);
}

#[test]
fn given_voice_pitch_when_step_voice_pitch_then_sample_position_advances() {
    let mut dsp = Sdsp::new();
    dsp.set_voice_pitch(2, 0x1234);

    dsp.step_voice_pitch(2);
    assert_eq!(dsp.voice_sample_pos(2), 0x1234);
}

#[test]
fn given_fractional_brr_position_when_sampling_voice_then_gaussian_interpolation_is_used() {
    let mut dsp = Sdsp::new();
    dsp.voices[0].brr_initialized = true;
    dsp.voices[0].envx = 0x7F;
    dsp.voices[0].sample_pos = 0x3800;
    dsp.voices[0].brr_samples[0] = 0;
    dsp.voices[0].brr_samples[1] = 0;
    dsp.voices[0].brr_samples[2] = 0;
    dsp.voices[0].brr_samples[3] = 0x3000;

    let expected = dsp.gaussian_interpolate(0, 0, 0, 0x3000, 0x80);
    let sample = dsp.voice_sample(0, 0, Some(&[]));

    assert_eq!(
        sample, expected,
        "fractional BRR positions should use S-DSP gaussian interpolation"
    );
    assert_ne!(
        sample, dsp.voices[0].brr_samples[3],
        "playback must not point-sample the selected BRR entry"
    );
}

#[test]
fn given_brr_position_at_block_start_when_sampling_voice_then_previous_block_history_is_used() {
    let mut dsp = Sdsp::new();
    dsp.voices[0].brr_initialized = true;
    dsp.voices[0].envx = 0x7F;
    dsp.voices[0].sample_pos = 0x0800;
    dsp.voices[0].brr_history = [0x1000, 0x2000, 0x3000];
    dsp.voices[0].brr_samples[0] = 0x4000;

    let expected = dsp.gaussian_interpolate(0x1000, 0x2000, 0x3000, 0x4000, 0x80);
    let sample = dsp.voice_sample(0, 0, Some(&[]));

    assert_eq!(
        sample, expected,
        "gaussian interpolation at a block boundary should include the previous block tail"
    );
}

#[test]
fn given_constant_samples_when_gaussian_interpolating_then_output_preserves_level() {
    let dsp = Sdsp::new();
    let out = dsp.gaussian_interpolate(100, 100, 100, 100, 64);
    assert!((out - 100).abs() <= 2);
    assert_eq!(out & 1, 0);
}

#[test]
fn given_fraction_endpoints_when_gaussian_interpolating_then_indices_are_safe() {
    let dsp = Sdsp::new();
    let out0 = dsp.gaussian_interpolate(-123, 456, -789, 321, 0);
    let out255 = dsp.gaussian_interpolate(-123, 456, -789, 321, 255);
    assert_eq!(out0 & 1, 0);
    assert_eq!(out255 & 1, 0);
}

#[test]
fn given_extreme_samples_when_gaussian_interpolating_then_output_is_clamped_and_even() {
    let dsp = Sdsp::new();
    let out = dsp.gaussian_interpolate(i16::MIN, i16::MAX, i16::MIN, i16::MAX, 127);
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
    dsp.write_reg(0x6C, 0x1F); // fastest noise clock in this implementation

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
fn given_full_scale_voice_when_rendering_then_mixer_uses_full_resolution_sample_not_outx_register()
{
    let mut dsp = Sdsp::new();
    dsp.write_reg(0x0C, 0x7F);
    dsp.write_reg(0x1C, 0x7F);
    dsp.write_reg(0x00, 0x7F);
    dsp.write_reg(0x01, 0x7F);
    dsp.write_reg(0x07, 0x7F); // direct gain => ENVX=0x7F
    dsp.write_reg(0x6C, 0x00); // unmute

    dsp.step_phase();
    let (left, right) = dsp.render_stereo_sample();

    assert!(
        (0.45..0.55).contains(&left),
        "left channel should use full-resolution voice sample before OUTX quantization"
    );
    assert!(
        (0.45..0.55).contains(&right),
        "right channel should use full-resolution voice sample before OUTX quantization"
    );
}

#[test]
fn given_pmon_enabled_for_voice1_when_voice0_outx_nonzero_then_voice1_pitch_step_is_modulated() {
    let mut dsp = Sdsp::new();
    dsp.write_reg(0x0C, 127);
    dsp.write_reg(0x1C, 127);
    dsp.write_reg(0x00, 127);
    dsp.write_reg(0x01, 127);
    dsp.write_reg(0x07, 0x7F);
    dsp.write_reg(0x12, 0x00); // voice1 pitch low
    dsp.write_reg(0x13, 0x10); // voice1 pitch high => 0x1000

    dsp.step_phase();
    let base_step = dsp.voice_sample_pos(1);

    let mut modulated = Sdsp::new();
    modulated.write_reg(0x0C, 127);
    modulated.write_reg(0x1C, 127);
    modulated.write_reg(0x00, 127);
    modulated.write_reg(0x01, 127);
    modulated.write_reg(0x07, 0x7F);
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
fn given_envx_outx_when_written_then_voice_status_is_not_directly_overridden() {
    let mut dsp = Sdsp::new();
    dsp.write_reg(0x05, 0x8F);
    dsp.write_reg(0x06, 0xE0);
    dsp.write_reg(0x4C, 0x01);
    for _ in 0..8 {
        dsp.step_phase();
    }
    let env_before = dsp.read_reg(0x08);
    let out_before = dsp.read_reg(0x09);

    dsp.write_reg(0x08, env_before.wrapping_add(1));
    dsp.write_reg(0x09, out_before.wrapping_add(1));

    assert_eq!(dsp.read_reg(0x08), env_before);
    assert_eq!(dsp.read_reg(0x09), out_before);
}

#[test]
fn given_pmon_enabled_when_master_volume_zero_then_pitch_modulation_still_applies() {
    let mut base = Sdsp::new();
    base.write_reg(0x00, 127);
    base.write_reg(0x01, 127);
    base.write_reg(0x07, 0x7F);
    base.write_reg(0x12, 0x00);
    base.write_reg(0x13, 0x10);
    base.step_phase();
    let base_step = base.voice_sample_pos(1);

    let mut modulated = Sdsp::new();
    modulated.write_reg(0x00, 127);
    modulated.write_reg(0x01, 127);
    modulated.write_reg(0x07, 0x7F);
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
fn given_pmon_enabled_with_voice0_output_when_voice1_steps_then_pitch_uses_modulation_formula() {
    let mut dsp = Sdsp::new();
    dsp.write_reg(0x00, 127);
    dsp.write_reg(0x01, 127);
    dsp.write_reg(0x07, 0x7F);
    dsp.write_reg(0x12, 0x00);
    dsp.write_reg(0x13, 0x10);
    dsp.write_reg(0x2D, 0x02);

    dsp.step_phase();

    let voice0_outx = i32::from(dsp.read_reg(0x09) as i8);
    let expected_step = (0x1000 * ((voice0_outx >> 4) + 0x400)) >> 10;
    assert_eq!(
        dsp.voice_sample_pos(1),
        expected_step as u32,
        "voice1 pitch should use the previous voice OUTX-derived modulation factor"
    );
}

#[test]
fn given_adsr_attack_rate_15_when_key_on_latency_expires_then_envelope_jumps_by_1024() {
    let mut dsp = Sdsp::new();
    dsp.write_reg(0x05, 0x8F);
    dsp.write_reg(0x06, 0xE0);
    dsp.write_reg(0x4C, 0x01);

    for _ in 0..5 {
        dsp.step_phase();
    }

    assert_eq!(
        dsp.read_reg(0x08),
        0x40,
        "attack rate 15 should step the envelope by 1024 at the first active tick"
    );
}

#[test]
fn given_write_to_endx_when_acknowledged_then_all_voice_end_bits_clear() {
    let mut dsp = Sdsp::new();

    dsp.write_reg(0x7C, 0xFF);

    assert_eq!(
        dsp.read_reg(0x7C),
        0x00,
        "writing ENDX should acknowledge and clear every bit"
    );
}

#[test]
fn given_key_on_with_zero_brr_block_when_phase_steps_then_voice_uses_decoded_sample_data() {
    let mut dsp = Sdsp::new();
    let mut aram = [0u8; 0x1_0000];
    aram[0x0000] = 0x00;
    aram[0x0001] = 0x01;
    aram[0x0002] = 0x00;
    aram[0x0003] = 0x01;

    dsp.write_reg(0x04, 0x00);
    dsp.write_reg(0x05, 0x8F);
    dsp.write_reg(0x06, 0xE0);
    dsp.write_reg(0x00, 0x7F);
    dsp.write_reg(0x01, 0x7F);
    dsp.write_reg(0x07, 0x7F);
    dsp.write_reg(0x02, 0x00);
    dsp.write_reg(0x03, 0x10);
    dsp.write_reg(0x4C, 0x01);

    for _ in 0..5 {
        dsp.step_phase_with_memory(&aram);
    }

    assert_eq!(
        dsp.read_reg(0x09),
        0x00,
        "a decoded zero BRR block should produce a silent OUTX sample"
    );
}

#[test]
fn given_end_flagged_brr_block_when_keyed_on_then_endx_is_set() {
    let mut dsp = Sdsp::new();
    let mut aram = [0u8; 0x1_0000];
    aram[0x0000] = 0x00;
    aram[0x0001] = 0x01;
    aram[0x0002] = 0x00;
    aram[0x0003] = 0x01;
    aram[0x0100] = 0x01;

    dsp.write_reg(0x04, 0x00);
    dsp.write_reg(0x05, 0x8F);
    dsp.write_reg(0x06, 0xE0);
    dsp.write_reg(0x00, 0x7F);
    dsp.write_reg(0x01, 0x7F);
    dsp.write_reg(0x07, 0x7F);
    dsp.write_reg(0x02, 0x00);
    dsp.write_reg(0x03, 0x10);
    dsp.write_reg(0x4C, 0x01);

    for _ in 0..5 {
        dsp.step_phase_with_memory(&aram);
    }

    assert_ne!(
        dsp.read_reg(0x7C),
        0x00,
        "an END BRR block should raise the ENDX voice status bit"
    );
}

#[test]
fn given_echo_enabled_when_rendering_with_memory_then_echo_ring_buffer_is_written_at_esa_base() {
    let mut dsp = Sdsp::new();
    let mut aram = [0u8; 0x1_0000];
    dsp.write_reg(0x00, 0x7F);
    dsp.write_reg(0x01, 0x7F);
    dsp.write_reg(0x0C, 0x7F);
    dsp.write_reg(0x1C, 0x7F);
    dsp.write_reg(0x4D, 0x01); // EON voice 0
    dsp.write_reg(0x6D, 0x40); // ESA base 0x4000
    dsp.write_reg(0x7D, 0x01); // EDL non-zero
    dsp.write_reg(0x6C, 0x00); // FLG: unmute + echo write enable
    dsp.voices[0].outx = 64;
    dsp.voices[0].current_output = i16::from(dsp.voices[0].outx) << 8;

    let _ = dsp.render_stereo_sample_with_memory(&mut aram);

    let base = 0x4000usize;
    let left = i16::from_le_bytes([aram[base], aram[base + 1]]);
    let right = i16::from_le_bytes([aram[base + 2], aram[base + 3]]);
    assert_ne!(left, 0, "left echo sample should be written to ARAM");
    assert_ne!(right, 0, "right echo sample should be written to ARAM");
}

#[test]
fn given_echo_write_disabled_when_rendering_with_memory_then_echo_buffer_is_not_overwritten() {
    let mut dsp = Sdsp::new();
    let mut aram = [0x55u8; 0x1_0000];
    dsp.write_reg(0x00, 0x7F);
    dsp.write_reg(0x01, 0x7F);
    dsp.write_reg(0x0C, 0x7F);
    dsp.write_reg(0x1C, 0x7F);
    dsp.write_reg(0x4D, 0x01); // EON voice 0
    dsp.write_reg(0x6D, 0x20); // ESA base 0x2000
    dsp.write_reg(0x7D, 0x01); // EDL non-zero
    dsp.write_reg(0x6C, 0x20); // FLG.5 = echo write disable
    dsp.voices[0].outx = 64;
    dsp.voices[0].current_output = i16::from(dsp.voices[0].outx) << 8;

    let _ = dsp.render_stereo_sample_with_memory(&mut aram);

    let base = 0x2000usize;
    assert_eq!(aram[base], 0x55);
    assert_eq!(aram[base + 1], 0x55);
    assert_eq!(aram[base + 2], 0x55);
    assert_eq!(aram[base + 3], 0x55);
}

#[test]
fn given_echo_ram_and_fir_coefficients_when_rendering_with_memory_then_echo_is_mixed_to_output() {
    let mut dsp = Sdsp::new();
    let mut aram = [0u8; 0x1_0000];
    dsp.write_reg(0x2C, 0x7F); // EVOLL
    dsp.write_reg(0x3C, 0x7F); // EVOLR
    dsp.write_reg(0x7F, 0x7F); // FIR7 (newest sample tap)
    dsp.write_reg(0x6D, 0x10); // ESA base 0x1000
    dsp.write_reg(0x7D, 0x01); // EDL non-zero
    dsp.write_reg(0x6C, 0x00); // FLG: unmute + echo write enable
    let base = 0x1000usize;
    aram[base] = 0xFE;
    aram[base + 1] = 0x7F; // near +0x7FFF
    aram[base + 2] = 0xFE;
    aram[base + 3] = 0x7F;

    let (left, right) = dsp.render_stereo_sample_with_memory(&mut aram);

    assert!(
        left.abs() > 0.01,
        "left output should include echo contribution"
    );
    assert!(
        right.abs() > 0.01,
        "right output should include echo contribution"
    );
}

#[test]
fn given_edl_zero_when_rendering_then_echo_ring_wraps_each_sample() {
    let mut dsp = Sdsp::new();
    let mut aram = [0u8; 0x1_0000];
    dsp.write_reg(0x00, 0x7F);
    dsp.write_reg(0x01, 0x7F);
    dsp.write_reg(0x0C, 0x7F);
    dsp.write_reg(0x1C, 0x7F);
    dsp.write_reg(0x4D, 0x01); // EON voice 0
    dsp.write_reg(0x6D, 0x30); // ESA base 0x3000
    dsp.write_reg(0x7D, 0x00); // EDL=0 => 4-byte ring
    dsp.write_reg(0x6C, 0x00); // FLG: unmute + echo write enable
    dsp.voices[0].outx = 10;
    dsp.voices[0].current_output = i16::from(dsp.voices[0].outx) << 8;
    let _ = dsp.render_stereo_sample_with_memory(&mut aram);
    let first = [aram[0x3000], aram[0x3001], aram[0x3002], aram[0x3003]];

    dsp.voices[0].outx = 64;
    dsp.voices[0].current_output = i16::from(dsp.voices[0].outx) << 8;
    let _ = dsp.render_stereo_sample_with_memory(&mut aram);
    let second = [aram[0x3000], aram[0x3001], aram[0x3002], aram[0x3003]];

    assert_ne!(
        first, second,
        "EDL=0 should keep overwriting the same 4-byte entry"
    );
    assert_eq!(aram[0x3004], 0);
    assert_eq!(aram[0x3005], 0);
    assert_eq!(aram[0x3006], 0);
    assert_eq!(aram[0x3007], 0);
}

#[test]
fn given_esa_change_when_rendering_then_write_base_switches_after_one_sample_delay() {
    let mut dsp = Sdsp::new();
    let mut aram = [0u8; 0x1_0000];
    dsp.write_reg(0x00, 0x7F);
    dsp.write_reg(0x01, 0x7F);
    dsp.write_reg(0x0C, 0x7F);
    dsp.write_reg(0x1C, 0x7F);
    dsp.write_reg(0x4D, 0x01); // EON voice 0
    dsp.write_reg(0x7D, 0x00); // EDL=0 keeps ring index at 0
    dsp.write_reg(0x6D, 0x10);
    dsp.write_reg(0x6C, 0x00); // FLG: unmute + echo write enable
    dsp.voices[0].outx = 24;
    dsp.voices[0].current_output = i16::from(dsp.voices[0].outx) << 8;
    let _ = dsp.render_stereo_sample_with_memory(&mut aram);
    let first_base = [aram[0x1000], aram[0x1001], aram[0x1002], aram[0x1003]];

    dsp.write_reg(0x6D, 0x20);
    dsp.voices[0].outx = 40;
    dsp.voices[0].current_output = i16::from(dsp.voices[0].outx) << 8;
    let _ = dsp.render_stereo_sample_with_memory(&mut aram);
    let delayed_base = [aram[0x1000], aram[0x1001], aram[0x1002], aram[0x1003]];
    let new_base_after_second = [aram[0x2000], aram[0x2001], aram[0x2002], aram[0x2003]];

    dsp.voices[0].outx = 56;
    dsp.voices[0].current_output = i16::from(dsp.voices[0].outx) << 8;
    let _ = dsp.render_stereo_sample_with_memory(&mut aram);
    let new_base_after_third = [aram[0x2000], aram[0x2001], aram[0x2002], aram[0x2003]];

    assert_ne!(
        first_base, delayed_base,
        "second sample should still target old ESA base"
    );
    assert_eq!(new_base_after_second, [0, 0, 0, 0]);
    assert_ne!(
        new_base_after_third,
        [0, 0, 0, 0],
        "new ESA base should take effect on the following sample"
    );
}

#[test]
fn legacy_deserialization_without_echo_state_uses_default_echo_state() {
    let legacy = serde_json::from_str::<Sdsp>(r#"{"phase":255,"regs":[]}"#)
        .expect("legacy payload without echo_state should deserialize with defaults");
    assert_eq!(legacy.phase(), 255);
    let serialized = serde_json::to_value(legacy).expect("serialize loaded legacy state");
    assert_eq!(serialized["echo_state"]["ring_size"], 1);
    assert_eq!(serialized["echo_state"]["ring_index"], 0);
}
