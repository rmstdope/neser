#[cfg(test)]
mod tests {
    use crate::apu::Apu;
    use crate::bus::BusDevice;
    use crate::bus::apu_device::ApuDevice;
    use std::cell::RefCell;
    use std::rc::Rc;

    const CPU_CLOCK_NTSC: f32 = 1_789_773.0;

    #[derive(Clone, Copy)]
    enum PulseChannel {
        Pulse1,
        Pulse2,
    }

    /// Create a baseline APU for audio analysis.
    ///
    /// We set the sample rate to the CPU clock so every `clock()` yields
    /// one mixed sample, and disable non-pulse channels for isolation.
    fn create_apu() -> (Rc<RefCell<Apu>>, ApuDevice) {
        create_apu_with_sample_rate(CPU_CLOCK_NTSC)
    }

    fn create_apu_with_sample_rate(sample_rate: f32) -> (Rc<RefCell<Apu>>, ApuDevice) {
        let apu = Rc::new(RefCell::new(Apu::new()));
        {
            let mut apu_mut = apu.borrow_mut();
            apu_mut.set_sample_rate(sample_rate);
            apu_mut.set_triangle_enabled(false);
            apu_mut.set_noise_enabled(false);
            apu_mut.set_dmc_enabled(false);
        }
        let device = ApuDevice::new(apu.clone());
        (apu, device)
    }

    fn set_pulse_mix_enable(apu: &Rc<RefCell<Apu>>, channel: PulseChannel) {
        let mut apu_mut = apu.borrow_mut();
        apu_mut.set_pulse1_enabled(matches!(channel, PulseChannel::Pulse1));
        apu_mut.set_pulse2_enabled(matches!(channel, PulseChannel::Pulse2));
    }

    fn pulse_enable_mask(channel: PulseChannel) -> u8 {
        match channel {
            PulseChannel::Pulse1 => 0x01,
            PulseChannel::Pulse2 => 0x02,
        }
    }

    fn write_pulse_control(device: &mut ApuDevice, channel: PulseChannel, value: u8) {
        let addr = match channel {
            PulseChannel::Pulse1 => 0x4000,
            PulseChannel::Pulse2 => 0x4004,
        };
        assert!(device.write(addr, value, false));
    }

    fn write_pulse_timer_low(device: &mut ApuDevice, channel: PulseChannel, value: u8) {
        let addr = match channel {
            PulseChannel::Pulse1 => 0x4002,
            PulseChannel::Pulse2 => 0x4006,
        };
        assert!(device.write(addr, value, false));
    }

    fn write_pulse_timer_high_and_length(device: &mut ApuDevice, channel: PulseChannel, value: u8) {
        let addr = match channel {
            PulseChannel::Pulse1 => 0x4003,
            PulseChannel::Pulse2 => 0x4007,
        };
        assert!(device.write(addr, value, false));
    }

    fn write_pulse_sweep(device: &mut ApuDevice, channel: PulseChannel, value: u8) {
        let addr = match channel {
            PulseChannel::Pulse1 => 0x4001,
            PulseChannel::Pulse2 => 0x4005,
        };
        assert!(device.write(addr, value, false));
    }

    /// Configure the pulse channel for a simple, constant-volume tone.
    ///
    /// This writes:
    /// - $4000/$4004 control: duty mode + constant volume
    /// - $4002/$4003 or $4006/$4007 timer (period)
    /// - length reload + enable, so output is not muted by length counter
    fn configure_pulse_constant_volume(
        apu: &Rc<RefCell<Apu>>,
        device: &mut ApuDevice,
        channel: PulseChannel,
        duty: u8,
        volume: u8,
        timer: u16,
        length_index: u8,
    ) {
        let duty_bits = (duty & 0x03) << 6;
        let control = duty_bits | 0b0001_0000 | (volume & 0x0F);
        write_pulse_control(device, channel, control);

        assert!(device.write(0x4015, pulse_enable_mask(channel), false));
        set_pulse_mix_enable(apu, channel);

        write_pulse_timer_low(device, channel, (timer & 0x00FF) as u8);
        let timer_high = ((timer >> 8) as u8) & 0x07;
        let length_and_high = ((length_index & 0x1F) << 3) | timer_high;
        write_pulse_timer_high_and_length(device, channel, length_and_high);

        // Apply pending length reload on the next clock.
        let mut apu_mut = apu.borrow_mut();
        apu_mut.clock();
        let _ = apu_mut.get_sample();
    }

    /// Configure the pulse channel for envelope-driven output (disable constant volume).
    ///
    /// This writes:
    /// - $4000/$4004 control: duty mode + envelope period (constant volume disabled)
    /// - $4002/$4003 or $4006/$4007 timer (period)
    /// - length reload + enable, so output is not muted by length counter
    fn configure_pulse_envelope(
        apu: &Rc<RefCell<Apu>>,
        device: &mut ApuDevice,
        channel: PulseChannel,
        duty: u8,
        envelope_period: u8,
        timer: u16,
        length_index: u8,
    ) {
        let duty_bits = (duty & 0x03) << 6;
        let control = duty_bits | (envelope_period & 0x0F);
        write_pulse_control(device, channel, control);

        assert!(device.write(0x4015, pulse_enable_mask(channel), false));
        set_pulse_mix_enable(apu, channel);

        write_pulse_timer_low(device, channel, (timer & 0x00FF) as u8);
        let timer_high = ((timer >> 8) as u8) & 0x07;
        let length_and_high = ((length_index & 0x1F) << 3) | timer_high;
        write_pulse_timer_high_and_length(device, channel, length_and_high);

        let mut apu_mut = apu.borrow_mut();
        apu_mut.clock();
        let _ = apu_mut.get_sample();
    }

    /// Configure the pulse channel with explicit control bits.
    ///
    /// This is useful for tests that require the length counter halt (bit 5)
    /// or envelope loop behavior while still using APU-level sampling.
    fn configure_pulse_with_control(
        apu: &Rc<RefCell<Apu>>,
        device: &mut ApuDevice,
        channel: PulseChannel,
        control: u8,
        timer: u16,
        length_index: u8,
    ) {
        write_pulse_control(device, channel, control);

        assert!(device.write(0x4015, pulse_enable_mask(channel), false));
        set_pulse_mix_enable(apu, channel);

        write_pulse_timer_low(device, channel, (timer & 0x00FF) as u8);
        let timer_high = ((timer >> 8) as u8) & 0x07;
        let length_and_high = ((length_index & 0x1F) << 3) | timer_high;
        write_pulse_timer_high_and_length(device, channel, length_and_high);

        let mut apu_mut = apu.borrow_mut();
        apu_mut.clock();
        let _ = apu_mut.get_sample();
    }

    /// Clock the APU and collect mixed output samples.
    ///
    /// We set the sample rate so each `clock()` yields one sample; if a sample
    /// is delayed, we keep clocking until it is available.
    fn collect_samples(apu: &Rc<RefCell<Apu>>, samples: usize) -> Vec<f32> {
        let mut outputs = Vec::with_capacity(samples);
        while outputs.len() < samples {
            let mut apu_mut = apu.borrow_mut();
            apu_mut.clock();
            if let Some(sample) = apu_mut.get_sample() {
                outputs.push(sample);
            }
        }
        outputs
    }

    /// Convert a stream of output samples into periods measured by rising edges.
    ///
    /// We treat any transition from 0 to >0 as a rising edge, then compute
    /// the sample distance between successive edges.
    fn rising_edge_periods(samples: &[f32]) -> Vec<usize> {
        let mut edges = Vec::new();
        for index in 1..samples.len() {
            if samples[index - 1] <= 0.0 && samples[index] > 0.0 {
                edges.push(index);
            }
        }

        let mut periods = Vec::new();
        for window in edges.windows(2) {
            periods.push(window[1] - window[0]);
        }
        periods
    }

    fn average_period(samples: &[f32]) -> f32 {
        let periods = rising_edge_periods(samples);
        assert!(!periods.is_empty(), "expected rising edges");
        periods.iter().sum::<usize>() as f32 / periods.len() as f32
    }

    /// Measure the maximum output observed over one full waveform period.
    fn max_output_over_period(apu: &Rc<RefCell<Apu>>, period_samples: usize) -> f32 {
        collect_samples(apu, period_samples)
            .into_iter()
            .fold(0.0, f32::max)
    }

    /// Advance the frame counter to trigger an immediate quarter+half clock.
    ///
    /// Writing $4017 with bit 7 set generates an immediate quarter+half clock
    /// once the delayed write takes effect (3-4 CPU cycles).
    fn clock_immediate_quarter_and_half(apu: &Rc<RefCell<Apu>>, device: &mut ApuDevice) {
        assert!(device.write(0x4017, 0x80, false));
        for _ in 0..4 {
            let mut apu_mut = apu.borrow_mut();
            apu_mut.clock();
            let _ = apu_mut.get_sample();
        }
    }

    #[test]
    fn test_pulse_constant_volume_duty_cycle() {
        // Verify that constant-volume pulses match the NES duty ratios.
        //
        // Per NESdev APU_Pulse duty table:
        // duty 0 = 12.5%, duty 1 = 25%, duty 2 = 50%, duty 3 = 75% (negated 25%).
        let timer = 0x0010;
        let period_samples = 16 * (timer as usize + 1);

        let cases = [(0, 0.125f32), (1, 0.25f32), (2, 0.5f32), (3, 0.75f32)];

        for (duty_mode, expected_ratio) in cases {
            let (apu, mut device) = create_apu();
            configure_pulse_constant_volume(
                &apu,
                &mut device,
                PulseChannel::Pulse1,
                duty_mode,
                15,
                timer,
                0,
            );
            let outputs = collect_samples(&apu, period_samples * 4);

            // Count non-zero samples to approximate duty ratio.
            let high = outputs.iter().filter(|&&value| value > 0.0).count();
            let duty = high as f32 / outputs.len() as f32;

            assert_eq!(
                expected_ratio, duty,
                "duty {} ratio mismatch: expected {}, got {}",
                duty_mode, expected_ratio, duty
            );
        }
    }

    #[test]
    fn test_pulse_period_matches_timer() {
        // Verify that the observed period matches the configured timer.
        //
        // We detect rising edges in the output and compare the average period
        // to 16 * (timer + 1), which is the pulse timer formula in CPU cycles.
        let (apu, mut device) = create_apu();
        let timer = 0x000F;
        configure_pulse_constant_volume(&apu, &mut device, PulseChannel::Pulse1, 2, 15, timer, 0);

        // Collect several periods to average out edge jitter.
        let period_samples = 16 * (timer as usize + 1);
        let outputs = collect_samples(&apu, period_samples * 5);

        // Compare average period to expected timer-derived period.
        let avg_period = average_period(&outputs);
        let expected = period_samples as f32;
        assert_eq!(
            avg_period,
            expected.abs(),
            "period mismatch: expected {} cycles, got {}",
            expected,
            avg_period
        );
    }

    #[test]
    fn test_pulse_length_counter_silences_output() {
        // Verify that the length counter eventually silences the pulse.
        //
        // We enable the length counter, then advance a few half-frame clocks.
        // Once the counter expires, output should be zero.
        let (apu, mut device) = create_apu();
        let timer = 0x0008;
        configure_pulse_constant_volume(
            &apu,
            &mut device,
            PulseChannel::Pulse1,
            2,
            15,
            timer,
            0x03,
        );

        // Each period provides a window to detect non-zero output.
        let period_samples = 16 * (timer as usize + 1);

        let early_outputs = collect_samples(&apu, period_samples);
        let has_sound_initially = early_outputs.iter().any(|&value| value > 0.0);
        assert!(
            has_sound_initially,
            "expected initial output before silence"
        );

        // Advance enough half-frame clocks to expire a short length (index 3 -> length 2).
        for _ in 0..3 {
            clock_immediate_quarter_and_half(&apu, &mut device);
        }

        let late_outputs = collect_samples(&apu, period_samples * 2);
        assert!(
            late_outputs.iter().all(|&value| value == 0.0),
            "expected length counter to silence output"
        );
    }

    #[test]
    fn test_pulse_envelope_decay_reduces_output() {
        // Verify that the envelope decay reduces output over successive
        // quarter-frame clocks when constant volume is disabled.
        //
        // With envelope period n=0, the envelope counter should decrement
        // on every envelope clock. We sample one full period after each
        // clock and expect the maximum output to step down.
        let (apu, mut device) = create_apu();
        let timer = 0x0010;
        let period_samples = 16 * (timer as usize + 1);

        configure_pulse_envelope(&apu, &mut device, PulseChannel::Pulse1, 2, 0, timer, 0x1F);

        // Collect a few envelope steps and ensure they decay by 1 each clock.
        let mut levels = Vec::with_capacity(4);
        for _ in 0..4 {
            clock_immediate_quarter_and_half(&apu, &mut device);
            let observed = max_output_over_period(&apu, period_samples);
            levels.push(observed);
        }

        assert!(levels[0] > 0.0, "expected envelope output");
        let epsilon = 1e-6;
        assert!(
            levels[0] > levels[1] + epsilon,
            "expected envelope to decay"
        );
        assert!(
            levels[1] > levels[2] + epsilon,
            "expected envelope to decay"
        );
        assert!(
            levels[2] > levels[3] + epsilon,
            "expected envelope to decay"
        );
    }

    #[test]
    fn test_pulse_length_counter_halt_prevents_silence() {
        // Verify that setting the length counter halt flag prevents the
        // length counter from silencing the channel.
        let (apu, mut device) = create_apu();
        let timer = 0x0008;
        let duty_bits = 2u8 << 6;
        let control = duty_bits | 0b0011_0000 | 0x0F; // halt=1, constant volume=15
        configure_pulse_with_control(
            &apu,
            &mut device,
            PulseChannel::Pulse1,
            control,
            timer,
            0x03,
        );

        for _ in 0..10 {
            clock_immediate_quarter_and_half(&apu, &mut device);
        }

        let period_samples = 16 * (timer as usize + 1);
        let outputs = collect_samples(&apu, period_samples);
        assert!(
            outputs.iter().any(|&value| value > 0.0),
            "expected output with halt set"
        );
    }

    #[test]
    fn test_pulse_envelope_loop_restarts_after_zero() {
        // Verify that the envelope loops back to 15 when loop flag is set.
        //
        // With envelope period n=0, the envelope counter should decrement
        // on every envelope clock, then wrap to 15 after reaching 0.
        let (apu, mut device) = create_apu();
        let timer = 0x0010;
        let duty_bits = 2u8 << 6;
        let control = duty_bits | 0b0010_0000; // loop=1, constant volume disabled, n=0
        configure_pulse_with_control(
            &apu,
            &mut device,
            PulseChannel::Pulse1,
            control,
            timer,
            0x1F,
        );

        let period_samples = 16 * (timer as usize + 1);
        let mut levels = Vec::with_capacity(20);
        for _ in 0..20 {
            clock_immediate_quarter_and_half(&apu, &mut device);
            levels.push(max_output_over_period(&apu, period_samples));
        }

        let min_before_wrap = levels[..16].iter().copied().fold(f32::INFINITY, f32::min);
        let max_after_wrap = levels[16..].iter().copied().fold(0.0, f32::max);

        let epsilon = 1e-6;
        assert!(
            max_after_wrap > min_before_wrap + epsilon,
            "expected envelope wrap"
        );
    }

    #[test]
    fn test_pulse_sweep_shift_zero_keeps_period() {
        // Verify that sweep shift=0 does not update the period.
        let (apu, mut device) = create_apu();
        let timer = 0x0030;
        configure_pulse_constant_volume(
            &apu,
            &mut device,
            PulseChannel::Pulse1,
            2,
            15,
            timer,
            0x1F,
        );

        let base_period_samples = 16 * (timer as usize + 1);
        let base_outputs = collect_samples(&apu, base_period_samples * 4);
        let base_avg = average_period(&base_outputs);

        // Enable sweep with shift=0 (no change expected).
        write_pulse_sweep(&mut device, PulseChannel::Pulse1, 0x80);
        clock_immediate_quarter_and_half(&apu, &mut device);
        clock_immediate_quarter_and_half(&apu, &mut device);

        let updated_outputs = collect_samples(&apu, base_period_samples * 4);
        let updated_avg = average_period(&updated_outputs);

        assert!(
            (updated_avg - base_avg).abs() <= 1e-6,
            "expected sweep shift=0 to keep period"
        );
    }

    #[test]
    fn test_pulse_disabled_via_4015_silences_output() {
        // Verify that disabling the pulse via $4015 silences output.
        let (apu, mut device) = create_apu();
        let timer = 0x0010;
        configure_pulse_constant_volume(
            &apu,
            &mut device,
            PulseChannel::Pulse1,
            2,
            15,
            timer,
            0x1F,
        );

        let period_samples = 16 * (timer as usize + 1);
        let outputs = collect_samples(&apu, period_samples);
        assert!(
            outputs.iter().any(|&value| value > 0.0),
            "expected output before disable"
        );

        assert!(device.write(0x4015, 0x00, false));
        let outputs = collect_samples(&apu, period_samples * 2);
        assert!(
            outputs.iter().all(|&value| value == 0.0),
            "expected silence after disable"
        );
    }

    #[test]
    fn test_pulse_sweep_increases_period() {
        // Verify that sweep updates increase the observed period when
        // sweep is enabled with positive shift.
        //
        // We configure a sweep with shift=1 and period=0 so that every
        // applicable sweep clock updates the timer.
        let (apu, mut device) = create_apu();
        let timer = 0x0020;
        configure_pulse_constant_volume(
            &apu,
            &mut device,
            PulseChannel::Pulse1,
            2,
            15,
            timer,
            0x1F,
        );

        let base_period_samples = 16 * (timer as usize + 1);
        let base_outputs = collect_samples(&apu, base_period_samples * 4);
        let base_avg = average_period(&base_outputs);

        // Enable sweep: period=0, negate=0, shift=1.
        write_pulse_sweep(&mut device, PulseChannel::Pulse1, 0x81);

        // With sweep period=0, each half-frame clock updates immediately.
        // Two clocks apply two successive updates.
        clock_immediate_quarter_and_half(&apu, &mut device);
        clock_immediate_quarter_and_half(&apu, &mut device);

        let updated_timer = timer + (timer >> 1);
        let updated_timer = updated_timer + (updated_timer >> 1);
        let updated_period_samples = 16 * (updated_timer as usize + 1);
        let updated_outputs = collect_samples(&apu, updated_period_samples * 4);
        let updated_avg = average_period(&updated_outputs);

        assert!(updated_avg > base_avg, "expected sweep to increase period");
        assert_eq!(
            updated_avg, updated_period_samples as f32,
            "updated period mismatch: expected {}, got {}",
            updated_period_samples, updated_avg
        );
    }

    #[test]
    fn test_pulse_sweep_negate_differs_between_channels() {
        // Verify that negate mode differs between Pulse 1 and Pulse 2.
        //
        // Pulse 1 uses ones' complement: target = period - (period >> shift) - 1.
        // Pulse 2 uses two's complement: target = period - (period >> shift).
        let timer = 0x0040;
        let shift = 1;
        let change = timer >> shift;

        let expected_pulse1 = timer - change - 1;
        let expected_pulse2 = timer - change;

        let (apu1, mut device1) = create_apu();
        let (apu2, mut device2) = create_apu();

        configure_pulse_constant_volume(
            &apu1,
            &mut device1,
            PulseChannel::Pulse1,
            2,
            15,
            timer,
            0x1F,
        );
        configure_pulse_constant_volume(
            &apu2,
            &mut device2,
            PulseChannel::Pulse2,
            2,
            15,
            timer,
            0x1F,
        );

        // Enable sweep: period=0, negate=1, shift=1.
        write_pulse_sweep(&mut device1, PulseChannel::Pulse1, 0x89);
        write_pulse_sweep(&mut device2, PulseChannel::Pulse2, 0x89);

        // With sweep period=0, the update happens on the first half-frame clock.
        clock_immediate_quarter_and_half(&apu1, &mut device1);
        clock_immediate_quarter_and_half(&apu2, &mut device2);

        let expected_pulse1_samples = 16 * (expected_pulse1 as usize + 1);
        let expected_pulse2_samples = 16 * (expected_pulse2 as usize + 1);

        let outputs1 = collect_samples(&apu1, expected_pulse1_samples * 4);
        let outputs2 = collect_samples(&apu2, expected_pulse2_samples * 4);

        let avg1 = average_period(&outputs1);
        let avg2 = average_period(&outputs2);

        assert!(avg1 < avg2, "expected pulse1 period < pulse2 period");
        assert_eq!(
            avg1, expected_pulse1_samples as f32,
            "pulse1 period mismatch: expected {}, got {}",
            expected_pulse1_samples, avg1
        );
        assert_eq!(
            avg2, expected_pulse2_samples as f32,
            "pulse2 period mismatch: expected {}, got {}",
            expected_pulse2_samples, avg2
        );
    }

    #[test]
    fn test_pulse_sweep_mutes_when_period_too_small() {
        // Verify that the pulse output is muted when timer period < 8.
        //
        // This mute condition is independent of sweep enable and should
        // silence the channel even if the duty sequence is high.
        let (apu, mut device) = create_apu();
        let timer = 0x0007;
        configure_pulse_constant_volume(
            &apu,
            &mut device,
            PulseChannel::Pulse1,
            2,
            15,
            timer,
            0x1F,
        );

        let period_samples = 16 * (timer as usize + 1);
        let outputs = collect_samples(&apu, period_samples * 2);
        assert!(outputs.iter().all(|&value| value == 0.0), "expected mute");
    }

    #[test]
    fn test_pulse_sweep_mutes_when_target_overflows() {
        // Verify that the pulse output is muted when sweep target exceeds $7FF.
        //
        // The muting check uses the current sweep target regardless of sweep
        // enable, so configuring a positive shift that overflows should silence
        // the output immediately.
        let (apu, mut device) = create_apu();
        let timer = 0x07FE;
        configure_pulse_constant_volume(
            &apu,
            &mut device,
            PulseChannel::Pulse1,
            2,
            15,
            timer,
            0x1F,
        );

        // shift=1 produces target = timer + (timer >> 1) > $7FF.
        write_pulse_sweep(&mut device, PulseChannel::Pulse1, 0x81);

        let period_samples = 16 * (timer as usize + 1);
        let outputs = collect_samples(&apu, period_samples * 2);
        assert!(outputs.iter().all(|&value| value == 0.0), "expected mute");
    }

    #[test]
    fn test_pulse_envelope_restart_on_length_write() {
        // Verify that writing $4003/$4007 restarts the envelope on the next
        // envelope clock, increasing the observed output.
        let (apu, mut device) = create_apu();
        let timer = 0x0010;
        let period_samples = 16 * (timer as usize + 1);

        configure_pulse_envelope(&apu, &mut device, PulseChannel::Pulse1, 2, 0, timer, 0x1F);

        for _ in 0..3 {
            clock_immediate_quarter_and_half(&apu, &mut device);
        }
        let before = max_output_over_period(&apu, period_samples);

        let timer_high = ((timer >> 8) as u8) & 0x07;
        let length_and_high = (0x1F << 3) | timer_high;
        write_pulse_timer_high_and_length(&mut device, PulseChannel::Pulse1, length_and_high);

        clock_immediate_quarter_and_half(&apu, &mut device);
        let after = max_output_over_period(&apu, period_samples);

        let epsilon = 1e-6;
        assert!(
            after > before + epsilon,
            "expected envelope restart to raise output"
        );
    }

    #[test]
    fn test_pulse_sweep_disabled_does_not_change_period() {
        // Verify that sweep does not affect the period when the enable bit is 0.
        let (apu, mut device) = create_apu();
        let timer = 0x0030;
        configure_pulse_constant_volume(
            &apu,
            &mut device,
            PulseChannel::Pulse1,
            2,
            15,
            timer,
            0x1F,
        );

        let expected_period = 16 * (timer as usize + 1);
        let outputs = collect_samples(&apu, expected_period * 4);
        let base_avg = average_period(&outputs);
        assert_eq!(base_avg, expected_period as f32, "unexpected base period");

        // Enable=0, shift=1.
        write_pulse_sweep(&mut device, PulseChannel::Pulse1, 0x01);
        clock_immediate_quarter_and_half(&apu, &mut device);
        clock_immediate_quarter_and_half(&apu, &mut device);

        let outputs = collect_samples(&apu, expected_period * 4);
        let updated_avg = average_period(&outputs);
        assert_eq!(
            updated_avg, expected_period as f32,
            "expected period to remain unchanged"
        );
    }

    #[test]
    fn test_pulse_disable_then_reenable_keeps_silence_until_reload() {
        // Verify that disabling a pulse clears the length counter, and
        // re-enabling without a length reload keeps it silent.
        let (apu, mut device) = create_apu();
        let timer = 0x0010;
        configure_pulse_constant_volume(
            &apu,
            &mut device,
            PulseChannel::Pulse1,
            2,
            15,
            timer,
            0x1F,
        );

        let period_samples = 16 * (timer as usize + 1);
        let outputs = collect_samples(&apu, period_samples);
        assert!(
            outputs.iter().any(|&value| value > 0.0),
            "expected output before disable"
        );

        assert!(device.write(0x4015, 0x00, false));
        assert!(device.write(0x4015, 0x01, false));
        set_pulse_mix_enable(&apu, PulseChannel::Pulse1);

        let outputs = collect_samples(&apu, period_samples * 2);
        assert!(
            outputs.iter().all(|&value| value == 0.0),
            "expected silence without reload"
        );
    }

    #[test]
    fn test_pulse_timer_high_low_write_ordering() {
        // Verify that writing timer high then timer low produces the expected
        // period once both bytes are set.
        let (apu, mut device) = create_apu();
        let timer = 0x01AB;
        let period_samples = 16 * (timer as usize + 1);

        let duty_bits = 2u8 << 6;
        let control = duty_bits | 0b0001_0000 | 0x0F;
        write_pulse_control(&mut device, PulseChannel::Pulse1, control);
        assert!(device.write(0x4015, 0x01, false));
        set_pulse_mix_enable(&apu, PulseChannel::Pulse1);

        let timer_high = ((timer >> 8) as u8) & 0x07;
        let length_and_high = (0x1F << 3) | timer_high;
        write_pulse_timer_high_and_length(&mut device, PulseChannel::Pulse1, length_and_high);
        write_pulse_timer_low(&mut device, PulseChannel::Pulse1, (timer & 0x00FF) as u8);

        {
            let mut apu_mut = apu.borrow_mut();
            apu_mut.clock();
            let _ = apu_mut.get_sample();
        }

        let outputs = collect_samples(&apu, period_samples * 4);
        let avg_period = average_period(&outputs);
        assert_eq!(
            avg_period, period_samples as f32,
            "expected period from high/low write ordering"
        );
    }

    #[test]
    fn test_pulse_sweep_divider_reload_timing() {
        // Verify that sweep reload delays the update by the divider period.
        //
        // With sweep period = 1, the update should occur on the third half-frame
        // clock after the write (reload, decrement, then update).
        let (apu, mut device) = create_apu();
        let timer = 0x0020;
        configure_pulse_constant_volume(
            &apu,
            &mut device,
            PulseChannel::Pulse1,
            2,
            15,
            timer,
            0x1F,
        );

        let base_period_samples = 16 * (timer as usize + 1);
        let base_outputs = collect_samples(&apu, base_period_samples * 4);
        let base_avg = average_period(&base_outputs);

        // Enable sweep: period=1, negate=0, shift=1.
        write_pulse_sweep(&mut device, PulseChannel::Pulse1, 0x91);

        // First two half-frame clocks should not update the period.
        clock_immediate_quarter_and_half(&apu, &mut device);
        clock_immediate_quarter_and_half(&apu, &mut device);

        let outputs = collect_samples(&apu, base_period_samples * 4);
        let mid_avg = average_period(&outputs);
        assert!(
            (mid_avg - base_avg).abs() <= 1e-6,
            "expected no update before divider expires"
        );

        // Third half-frame clock should update the period.
        clock_immediate_quarter_and_half(&apu, &mut device);

        let updated_timer = timer + (timer >> 1);
        let updated_period_samples = 16 * (updated_timer as usize + 1);
        let outputs = collect_samples(&apu, updated_period_samples * 4);
        let updated_avg = average_period(&outputs);
        assert_eq!(
            updated_avg, updated_period_samples as f32,
            "expected update after divider expires"
        );
    }

    #[test]
    fn test_pulse_constant_volume_ignores_envelope_clock() {
        // Verify that constant volume output remains stable across envelope clocks.
        let (apu, mut device) = create_apu();
        let timer = 0x0010;
        let duty_bits = 2u8 << 6;
        let control = duty_bits | 0b0001_0000 | 0x0F; // constant volume 15
        configure_pulse_with_control(
            &apu,
            &mut device,
            PulseChannel::Pulse1,
            control,
            timer,
            0x1F,
        );

        let period_samples = 16 * (timer as usize + 1);
        let baseline = max_output_over_period(&apu, period_samples);

        for _ in 0..4 {
            clock_immediate_quarter_and_half(&apu, &mut device);
            let observed = max_output_over_period(&apu, period_samples);
            assert_eq!(
                observed, baseline,
                "expected constant volume across envelope clocks"
            );
        }
    }

    #[test]
    fn test_pulse_constant_volume_change_updates_output() {
        // Verify that changing constant volume updates the output amplitude.
        let (apu, mut device) = create_apu();
        let timer = 0x0010;
        let duty_bits = 2u8 << 6;
        let control_loud = duty_bits | 0b0001_0000 | 0x0F;
        let control_soft = duty_bits | 0b0001_0000 | 0x08;
        configure_pulse_with_control(
            &apu,
            &mut device,
            PulseChannel::Pulse1,
            control_loud,
            timer,
            0x1F,
        );

        let period_samples = 16 * (timer as usize + 1);
        let loud = max_output_over_period(&apu, period_samples);

        write_pulse_control(&mut device, PulseChannel::Pulse1, control_soft);
        let soft = max_output_over_period(&apu, period_samples);

        let epsilon = 1e-6;
        assert!(
            loud > soft + epsilon,
            "expected lower output after volume change"
        );
    }

    #[test]
    fn test_pulse_sequence_resets_on_length_write() {
        // Verify that writing $4003/$4007 resets the duty sequencer.
        //
        // For duty 12.5%, after a falling edge we should normally see
        // 7 low steps. A length write should reset the sequence and
        // produce a high within the next step.
        let (apu, mut device) = create_apu();
        let timer = 0x0008;
        configure_pulse_constant_volume(
            &apu,
            &mut device,
            PulseChannel::Pulse1,
            0,
            15,
            timer,
            0x1F,
        );

        let period_samples = 16 * (timer as usize + 1);
        let mut prev = 0.0;
        let mut seen_high = false;
        let mut found_fall = false;
        for _ in 0..(period_samples * 2) {
            let mut apu_mut = apu.borrow_mut();
            apu_mut.clock();
            if let Some(sample) = apu_mut.get_sample() {
                if prev > 0.0 && sample == 0.0 {
                    found_fall = true;
                    break;
                }
                if sample > 0.0 {
                    seen_high = true;
                }
                prev = sample;
            }
        }
        assert!(seen_high && found_fall, "expected a falling edge");

        let timer_high = ((timer >> 8) as u8) & 0x07;
        let length_and_high = (0x1F << 3) | timer_high;
        write_pulse_timer_high_and_length(&mut device, PulseChannel::Pulse1, length_and_high);

        let step_samples = 2 * (timer as usize + 1);
        let outputs = collect_samples(&apu, step_samples);
        assert!(
            outputs.iter().any(|&value| value > 0.0),
            "expected high within one step after reset"
        );
    }

    #[test]
    fn test_pulse_sweep_overflow_mutes_even_when_disabled() {
        // Verify that sweep target overflow mutes output even when sweep is disabled.
        let (apu, mut device) = create_apu();
        let timer = 0x07FE;
        configure_pulse_constant_volume(
            &apu,
            &mut device,
            PulseChannel::Pulse1,
            2,
            15,
            timer,
            0x1F,
        );

        // Enable bit = 0, shift=1 -> target overflow.
        write_pulse_sweep(&mut device, PulseChannel::Pulse1, 0x01);

        let period_samples = 16 * (timer as usize + 1);
        let outputs = collect_samples(&apu, period_samples * 2);
        assert!(outputs.iter().all(|&value| value == 0.0), "expected mute");
    }

    #[test]
    fn test_pulse_sweep_negate_underflow_differs_between_channels() {
        // Verify that negate underflow can mute Pulse 1 while Pulse 2 remains audible.
        //
        // With timer=9 and shift=3, change=1. Pulse 1 target=7 (<8) mutes,
        // Pulse 2 target=8 (not muted).
        let timer = 0x0009;
        let period_samples = 16 * (timer as usize + 1);

        let (apu1, mut device1) = create_apu();
        let (apu2, mut device2) = create_apu();

        configure_pulse_constant_volume(
            &apu1,
            &mut device1,
            PulseChannel::Pulse1,
            2,
            15,
            timer,
            0x1F,
        );
        configure_pulse_constant_volume(
            &apu2,
            &mut device2,
            PulseChannel::Pulse2,
            2,
            15,
            timer,
            0x1F,
        );

        // Enable sweep negate with shift=3.
        write_pulse_sweep(&mut device1, PulseChannel::Pulse1, 0x8B);
        write_pulse_sweep(&mut device2, PulseChannel::Pulse2, 0x8B);

        let outputs1 = collect_samples(&apu1, period_samples * 2);
        let outputs2 = collect_samples(&apu2, period_samples * 2);

        assert!(
            outputs1.iter().all(|&value| value == 0.0),
            "expected pulse1 mute"
        );
        assert!(
            outputs2.iter().any(|&value| value > 0.0),
            "expected pulse2 output"
        );
    }

    #[test]
    fn test_pulse_envelope_disable_still_clocks_internally() {
        // Verify that the envelope counter continues to clock even when
        // constant-volume mode is enabled.
        //
        // We run for several envelope clocks in constant-volume mode, then
        // clear the disable flag and expect the output to reflect a decayed level.
        let (apu, mut device) = create_apu();
        let timer = 0x0010;
        let period_samples = 16 * (timer as usize + 1);

        let duty_bits = 2u8 << 6;
        let control_const = duty_bits | 0b0001_0000; // disable=1, n=0
        configure_pulse_with_control(
            &apu,
            &mut device,
            PulseChannel::Pulse1,
            control_const,
            timer,
            0x1F,
        );

        for _ in 0..4 {
            clock_immediate_quarter_and_half(&apu, &mut device);
        }

        let control_env = duty_bits; // disable=0, n=0
        write_pulse_control(&mut device, PulseChannel::Pulse1, control_env);
        clock_immediate_quarter_and_half(&apu, &mut device);

        let observed = max_output_over_period(&apu, period_samples);
        let epsilon = 1e-6;
        assert!(
            observed < 1.0 - epsilon,
            "expected decayed output after re-enabling envelope"
        );
    }

    #[test]
    fn test_pulse_sweep_sub_sequence_pitch_drop() {
        // Recreate the sweep_sub sequence from roms/automated_tests/test_apu_sweep/sweep_sub.asm
        // and verify a slight pitch drop after the sweep clock + $4003 write.
        let sample_rate = 44_100.0;
        let (apu, mut device) = create_apu_with_sample_rate(sample_rate);
        set_pulse_mix_enable(&apu, PulseChannel::Pulse1);

        // Sync APU and configure pulse 1.
        assert!(device.write(0x4017, 0xC0, false));
        for _ in 0..4 {
            let mut apu_mut = apu.borrow_mut();
            apu_mut.clock();
            let _ = apu_mut.get_sample();
        }
        assert!(device.write(0x4015, 0x01, false));

        // $4000 = $BF (duty=2, halt=1, constant volume=15)
        write_pulse_control(&mut device, PulseChannel::Pulse1, 0xBF);

        // Use the first table entry (y=7): table_h=2, table_l=0x04.
        let timer_low = 0x04u8;
        let timer_high = 0x02u8;
        write_pulse_timer_low(&mut device, PulseChannel::Pulse1, timer_low);
        write_pulse_timer_high_and_length(&mut device, PulseChannel::Pulse1, timer_high);

        // Sweep: enable, negate, period=0, shift=7 (0x88 | 0x07).
        write_pulse_sweep(&mut device, PulseChannel::Pulse1, 0x8F);

        // Clock sweep via $4017 (immediate quarter+half), then write $4003 with +1.
        assert!(device.write(0x4017, 0xC0, false));
        for _ in 0..4 {
            let mut apu_mut = apu.borrow_mut();
            apu_mut.clock();
            let _ = apu_mut.get_sample();
        }
        write_pulse_timer_high_and_length(&mut device, PulseChannel::Pulse1, 0x01);
        write_pulse_sweep(&mut device, PulseChannel::Pulse1, 0x00);

        // Capture 200ms of audio and compare early vs late period.
        // With immediate sweep updates, the pitch should remain stable after the sequence.
        let capture_samples = (sample_rate * 0.2) as usize;
        let outputs = collect_samples(&apu, capture_samples);
        let mid = outputs.len() / 2;
        let early_avg = average_period(&outputs[..mid]);
        let late_avg = average_period(&outputs[mid..]);

        let epsilon = 1e-6;
        assert!(
            late_avg <= early_avg + epsilon,
            "expected no late pitch drop after sweep sequence"
        );
    }

    #[test]
    fn test_pulse_length_load_ignored_when_disabled() {
        // Verify that writing $4003/$4007 while disabled does not load
        // the length counter, and output remains silent until a new reload.
        let (apu, mut device) = create_apu();
        let timer = 0x0010;
        let period_samples = 16 * (timer as usize + 1);

        let duty_bits = 2u8 << 6;
        let control = duty_bits | 0b0001_0000 | 0x0F; // constant volume 15
        write_pulse_control(&mut device, PulseChannel::Pulse1, control);
        write_pulse_timer_low(&mut device, PulseChannel::Pulse1, (timer & 0x00FF) as u8);

        assert!(device.write(0x4015, 0x00, false));
        set_pulse_mix_enable(&apu, PulseChannel::Pulse1);

        let timer_high = ((timer >> 8) as u8) & 0x07;
        let length_and_high = (0x1F << 3) | timer_high;
        write_pulse_timer_high_and_length(&mut device, PulseChannel::Pulse1, length_and_high);

        let outputs = collect_samples(&apu, period_samples);
        assert!(
            outputs.iter().all(|&value| value == 0.0),
            "expected silence while disabled"
        );

        assert!(device.write(0x4015, 0x01, false));
        set_pulse_mix_enable(&apu, PulseChannel::Pulse1);
        let outputs = collect_samples(&apu, period_samples);
        assert!(
            outputs.iter().all(|&value| value == 0.0),
            "expected silence without reload after enable"
        );

        write_pulse_timer_high_and_length(&mut device, PulseChannel::Pulse1, length_and_high);
        let outputs = collect_samples(&apu, period_samples);
        assert!(
            outputs.iter().any(|&value| value > 0.0),
            "expected output after reload"
        );
    }

    #[test]
    fn test_pulse_duty_change_applies_without_timer_reset() {
        // Verify that changing duty cycle takes effect without needing
        // a timer reload, and the observed duty ratio matches the new duty.
        let (apu, mut device) = create_apu();
        let timer = 0x0010;
        let period_samples = 16 * (timer as usize + 1);

        configure_pulse_constant_volume(
            &apu,
            &mut device,
            PulseChannel::Pulse1,
            0,
            15,
            timer,
            0x1F,
        );
        let outputs = collect_samples(&apu, period_samples * 2);
        let high = outputs.iter().filter(|&&value| value > 0.0).count();
        let duty_before = high as f32 / outputs.len() as f32;

        let duty_bits = 3u8 << 6;
        let control = duty_bits | 0b0001_0000 | 0x0F;
        write_pulse_control(&mut device, PulseChannel::Pulse1, control);

        let outputs = collect_samples(&apu, period_samples * 2);
        let high = outputs.iter().filter(|&&value| value > 0.0).count();
        let duty_after = high as f32 / outputs.len() as f32;

        let epsilon = 1e-6;
        assert!(
            (duty_before - 0.125).abs() <= epsilon,
            "expected duty 12.5%"
        );
        assert!((duty_after - 0.75).abs() <= epsilon, "expected duty 75%");
    }
}
