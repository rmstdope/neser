#[cfg(test)]
mod tests {
    use crate::apu::Apu;
    use crate::bus::BusDevice;
    use crate::bus::apu_device::ApuDevice;
    use crate::console::TimingMode;
    use std::cell::RefCell;
    use std::rc::Rc;

    const NOISE_PERIOD_TABLE: [u16; 16] = [
        4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
    ];

    fn create_noise_device() -> (Rc<RefCell<Apu>>, ApuDevice) {
        let apu = Rc::new(RefCell::new(Apu::new()));
        {
            let mut apu_mut = apu.borrow_mut();
            apu_mut.set_sample_rate(TimingMode::Ntsc.cpu_clock_hz());
            apu_mut.set_pulse1_enabled(false);
            apu_mut.set_pulse2_enabled(false);
            apu_mut.set_triangle_enabled(false);
            apu_mut.set_noise_enabled(true);
            apu_mut.set_dmc_enabled(false);
        }
        let device = ApuDevice::new(apu.clone());
        (apu, device)
    }

    fn write_register(device: &mut ApuDevice, addr: u16, value: u8) {
        assert!(device.write(addr, value, false));
    }

    fn read_status(device: &mut ApuDevice) -> u8 {
        device.read(0x4015, 0, false).unwrap_or(0)
    }

    fn clock_and_collect_samples(apu: &Rc<RefCell<Apu>>, samples: usize) -> Vec<f32> {
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

    fn clock_cycles(apu: &Rc<RefCell<Apu>>, cycles: usize) {
        for _ in 0..cycles {
            let mut apu_mut = apu.borrow_mut();
            apu_mut.clock();
            let _ = apu_mut.get_sample();
        }
    }

    fn configure_noise_constant_volume(
        device: &mut ApuDevice,
        apu: &Rc<RefCell<Apu>>,
        volume: u8,
        period_index: u8,
        length_index: u8,
        mode_short: bool,
        halt_length: bool,
    ) {
        let halt_bit = if halt_length { 0x20 } else { 0x00 };
        let control = halt_bit | 0x10 | (volume & 0x0F);
        write_register(device, 0x400C, control);

        let mode_bit = if mode_short { 0x80 } else { 0x00 };
        write_register(device, 0x400E, mode_bit | (period_index & 0x0F));

        write_register(device, 0x4015, 0x08);
        write_register(device, 0x400F, (length_index & 0x1F) << 3);

        clock_cycles(apu, 1);
    }

    fn configure_noise_envelope(
        device: &mut ApuDevice,
        apu: &Rc<RefCell<Apu>>,
        envelope_period: u8,
        period_index: u8,
        length_index: u8,
        mode_short: bool,
        loop_envelope: bool,
    ) {
        let loop_bit = if loop_envelope { 0x20 } else { 0x00 };
        let control = loop_bit | (envelope_period & 0x0F);
        write_register(device, 0x400C, control);

        let mode_bit = if mode_short { 0x80 } else { 0x00 };
        write_register(device, 0x400E, mode_bit | (period_index & 0x0F));

        write_register(device, 0x4015, 0x08);
        write_register(device, 0x400F, (length_index & 0x1F) << 3);

        clock_cycles(apu, 1);
    }

    fn max_sample(samples: &[f32]) -> f32 {
        samples.iter().copied().fold(0.0, f32::max)
    }

    fn bool_runs(samples: &[f32]) -> Vec<(bool, usize)> {
        let mut runs = Vec::new();
        let mut iter = samples.iter();
        let Some(first) = iter.next() else {
            return runs;
        };
        let mut current = *first > 0.0;
        let mut count = 1usize;
        for &value in iter {
            let state = value > 0.0;
            if state == current {
                count += 1;
            } else {
                runs.push((current, count));
                current = state;
                count = 1;
            }
        }
        runs.push((current, count));
        runs
    }

    fn min_run_length(samples: &[f32]) -> usize {
        bool_runs(samples)
            .iter()
            .map(|(_, length)| *length)
            .min()
            .unwrap_or(0)
    }

    fn count_transitions(samples: &[f32]) -> usize {
        samples
            .windows(2)
            .filter(|window| (window[0] > 0.0) != (window[1] > 0.0))
            .count()
    }

    fn clock_immediate_quarter_and_half(device: &mut ApuDevice, apu: &Rc<RefCell<Apu>>) {
        write_register(device, 0x4017, 0x80);
        clock_cycles(apu, 4);
    }

    fn collect_shift_bits(
        samples: &[f32],
        shift_cycles: usize,
        shifts: usize,
        offset: usize,
    ) -> Option<Vec<u8>> {
        let end = offset + shift_cycles * shifts;
        if end >= samples.len() {
            return None;
        }

        let mut bits = Vec::with_capacity(shifts);
        for index in 0..shifts {
            let sample = samples[offset + index * shift_cycles];
            bits.push(if sample == 0.0 { 1 } else { 0 });
        }
        Some(bits)
    }

    fn expected_lfsr_bits(mode_short: bool, shifts: usize) -> Vec<u8> {
        let mut reg: u16 = 1;
        let mut bits = Vec::with_capacity(shifts);
        for _ in 0..shifts {
            let bit0 = reg & 1;
            let tap_bit = if mode_short {
                (reg >> 6) & 1
            } else {
                (reg >> 1) & 1
            };
            let feedback = bit0 ^ tap_bit;
            reg = (reg >> 1) | (feedback << 14);
            bits.push((reg & 1) as u8);
        }
        bits
    }

    fn sequence_is_subslice(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    fn find_bits_matching_expected(
        samples: &[f32],
        shift_cycles: usize,
        shifts: usize,
        expected: &[u8],
    ) -> Option<Vec<u8>> {
        for offset in 0..shift_cycles {
            if let Some(bits) = collect_shift_bits(samples, shift_cycles, shifts, offset)
                && sequence_is_subslice(expected, &bits)
            {
                return Some(bits);
            }
        }
        None
    }

    fn any_offset_repeats(samples: &[f32], shift_cycles: usize, period: usize) -> bool {
        let shifts = period * 2;
        for offset in 0..shift_cycles {
            if let Some(bits) = collect_shift_bits(samples, shift_cycles, shifts, offset)
                && bits[..period] == bits[period..]
            {
                return true;
            }
        }
        false
    }

    #[test]
    fn test_noise_period_matches_rate_table() {
        let cases = [0usize, 3, 7, 15];

        for &index in &cases {
            let (apu, mut device) = create_noise_device();
            configure_noise_constant_volume(&mut device, &apu, 15, index as u8, 0x1F, false, true);

            let period = NOISE_PERIOD_TABLE[index] as usize;
            let expected = 2 * (period + 1);
            let sample_cycles = expected * 40;
            let samples = clock_and_collect_samples(&apu, sample_cycles);
            let min_run = min_run_length(&samples);

            assert!(
                (min_run as isize - expected as isize).abs() <= 1,
                "rate index {} period mismatch: expected ~{}, got {}",
                index,
                expected,
                min_run
            );
        }
    }

    #[test]
    fn test_noise_timer_ticks_at_cpu_div2_rate() {
        let (apu, mut device) = create_noise_device();
        configure_noise_constant_volume(&mut device, &apu, 15, 0, 0x1F, false, true);

        let period = NOISE_PERIOD_TABLE[0] as usize;
        let expected = 2 * (period + 1);
        let samples = clock_and_collect_samples(&apu, expected * 20);
        let min_run = min_run_length(&samples);

        assert!(
            (min_run as isize - expected as isize).abs() <= 1,
            "expected noise timer to tick at CPU/2 (~{} cycles), got {}",
            expected,
            min_run
        );
        assert!(
            min_run != period + 1,
            "noise timer appears to tick at CPU rate"
        );
    }

    #[test]
    fn test_noise_mode_flag_selects_short_or_long_lfsr() {
        let shifts = 93;

        let (apu_short, mut device_short) = create_noise_device();
        configure_noise_constant_volume(&mut device_short, &apu_short, 15, 0, 0x1F, true, true);
        let shift_cycles = 2 * (NOISE_PERIOD_TABLE[0] as usize + 1);
        let short_samples = clock_and_collect_samples(&apu_short, shift_cycles * shifts * 4);

        let (apu_long, mut device_long) = create_noise_device();
        configure_noise_constant_volume(&mut device_long, &apu_long, 15, 0, 0x1F, false, true);
        let long_samples = clock_and_collect_samples(&apu_long, shift_cycles * shifts * 4);

        assert!(
            any_offset_repeats(&short_samples, shift_cycles, shifts),
            "expected short mode LFSR to repeat every 93 steps"
        );
        assert!(
            !any_offset_repeats(&long_samples, shift_cycles, shifts),
            "expected long mode LFSR to exceed 93-step period"
        );
    }

    #[test]
    fn test_noise_lfsr_feedback_tap_behavior_for_mode0() {
        let (apu, mut device) = create_noise_device();
        configure_noise_constant_volume(&mut device, &apu, 15, 0, 0x1F, false, true);

        let shift_cycles = 2 * (NOISE_PERIOD_TABLE[0] as usize + 1);
        let samples = clock_and_collect_samples(&apu, shift_cycles * 300);
        let expected = expected_lfsr_bits(false, 200);
        let observed = find_bits_matching_expected(&samples, shift_cycles, 20, &expected)
            .expect("expected to align with LFSR sequence");

        assert!(
            sequence_is_subslice(&expected, &observed),
            "mode 0 LFSR feedback tap mismatch"
        );
    }

    #[test]
    fn test_noise_lfsr_feedback_tap_behavior_for_mode1() {
        let (apu, mut device) = create_noise_device();
        configure_noise_constant_volume(&mut device, &apu, 15, 0, 0x1F, true, true);

        let shift_cycles = 2 * (NOISE_PERIOD_TABLE[0] as usize + 1);
        let samples = clock_and_collect_samples(&apu, shift_cycles * 300);
        let expected = expected_lfsr_bits(true, 200);
        let observed = find_bits_matching_expected(&samples, shift_cycles, 20, &expected)
            .expect("expected to align with LFSR sequence");

        assert!(
            sequence_is_subslice(&expected, &observed),
            "mode 1 LFSR feedback tap mismatch"
        );
    }

    #[test]
    fn test_noise_output_is_zero_when_lfsr_bit0_set() {
        let (apu, mut device) = create_noise_device();
        configure_noise_constant_volume(&mut device, &apu, 15, 0, 0x1F, false, true);

        let samples = clock_and_collect_samples(&apu, 400);
        assert!(
            samples.contains(&0.0),
            "expected samples muted when LFSR bit0 is set"
        );
        assert!(
            samples.iter().any(|&value| value > 0.0),
            "expected samples audible when LFSR bit0 is clear"
        );
    }

    #[test]
    fn test_noise_length_counter_gates_output() {
        let (apu, mut device) = create_noise_device();
        configure_noise_constant_volume(&mut device, &apu, 15, 0, 0x03, false, false);

        let initial_samples = clock_and_collect_samples(&apu, 200);
        assert!(
            initial_samples.iter().any(|&value| value > 0.0),
            "expected initial noise output"
        );

        for _ in 0..3 {
            clock_immediate_quarter_and_half(&mut device, &apu);
        }

        assert_eq!(
            read_status(&mut device) & 0x08,
            0,
            "expected length counter to expire"
        );

        let muted_samples = clock_and_collect_samples(&apu, 200);
        assert!(
            muted_samples.iter().all(|&value| value == 0.0),
            "expected noise output gated by length counter"
        );
    }

    #[test]
    fn test_noise_length_counter_halt_prevents_silence() {
        let (apu, mut device) = create_noise_device();
        configure_noise_constant_volume(&mut device, &apu, 15, 0, 0x03, false, true);

        let status_before = read_status(&mut device);
        for _ in 0..10 {
            clock_immediate_quarter_and_half(&mut device, &apu);
        }

        let status_after = read_status(&mut device);
        assert_eq!(
            status_before & 0x08,
            status_after & 0x08,
            "expected halt flag to prevent length decrement"
        );

        let outputs = clock_and_collect_samples(&apu, 400);
        assert!(
            outputs.iter().any(|&value| value > 0.0),
            "expected noise output with halt flag set"
        );
    }

    #[test]
    fn test_noise_envelope_decay_reduces_output() {
        let (apu, mut device) = create_noise_device();
        configure_noise_envelope(&mut device, &apu, 0, 0, 0x1F, false, false);

        let mut levels = Vec::with_capacity(4);
        for _ in 0..4 {
            clock_immediate_quarter_and_half(&mut device, &apu);
            let samples = clock_and_collect_samples(&apu, 200);
            levels.push(max_sample(&samples));
        }

        assert!(levels[0] > 0.0, "expected envelope output");
        assert!(levels[0] > levels[1], "expected envelope decay");
        assert!(levels[1] > levels[2], "expected envelope decay");
        assert!(levels[2] > levels[3], "expected envelope decay");
    }

    #[test]
    fn test_noise_envelope_loop_restarts_after_zero() {
        let (apu, mut device) = create_noise_device();
        configure_noise_envelope(&mut device, &apu, 0, 0, 0x1F, false, true);

        let mut levels = Vec::with_capacity(20);
        for _ in 0..20 {
            clock_immediate_quarter_and_half(&mut device, &apu);
            let samples = clock_and_collect_samples(&apu, 200);
            levels.push(max_sample(&samples));
        }

        let min_before = levels[..16].iter().copied().fold(f32::INFINITY, f32::min);
        let max_after = levels[16..].iter().copied().fold(0.0, f32::max);

        assert!(max_after > min_before, "expected envelope loop to wrap");
    }

    #[test]
    fn test_noise_write_400f_loads_length_and_starts_envelope() {
        let (apu, mut device) = create_noise_device();
        write_register(&mut device, 0x400C, 0x00); // envelope period 0, constant volume off
        write_register(&mut device, 0x400E, 0x00); // mode 0, rate 0
        write_register(&mut device, 0x4015, 0x08);

        assert_eq!(read_status(&mut device) & 0x08, 0);

        write_register(&mut device, 0x400F, 0xF8);
        clock_cycles(&apu, 1);

        assert_ne!(
            read_status(&mut device) & 0x08,
            0,
            "expected $400F write to load length counter"
        );

        clock_immediate_quarter_and_half(&mut device, &apu);
        let samples = clock_and_collect_samples(&apu, 200);
        let peak_initial = max_sample(&samples);

        for _ in 0..4 {
            clock_immediate_quarter_and_half(&mut device, &apu);
        }

        let later_samples = clock_and_collect_samples(&apu, 200);
        let peak_later = max_sample(&later_samples);

        assert!(
            peak_initial > peak_later,
            "expected envelope to start at max after $400F write"
        );
    }

    #[test]
    fn test_noise_write_400c_sets_constant_volume() {
        let (apu_low, mut device_low) = create_noise_device();
        configure_noise_constant_volume(&mut device_low, &apu_low, 7, 0, 0x1F, false, true);
        let low_samples = clock_and_collect_samples(&apu_low, 200);
        let low_peak = max_sample(&low_samples);

        let (apu_high, mut device_high) = create_noise_device();
        configure_noise_constant_volume(&mut device_high, &apu_high, 15, 0, 0x1F, false, true);
        let high_samples = clock_and_collect_samples(&apu_high, 200);
        let high_peak = max_sample(&high_samples);

        assert!(
            high_peak > low_peak,
            "expected higher constant volume to increase output"
        );
    }

    #[test]
    fn test_noise_disable_via_4015_clears_length_and_requires_400f_reload() {
        let (apu, mut device) = create_noise_device();
        configure_noise_constant_volume(&mut device, &apu, 15, 0, 0x1F, false, true);

        let outputs = clock_and_collect_samples(&apu, 200);
        assert!(outputs.iter().any(|&value| value > 0.0));

        write_register(&mut device, 0x4015, 0x00);
        let outputs = clock_and_collect_samples(&apu, 200);
        assert!(outputs.iter().all(|&value| value == 0.0));
        assert_eq!(read_status(&mut device) & 0x08, 0);

        write_register(&mut device, 0x4015, 0x08);
        let outputs = clock_and_collect_samples(&apu, 200);
        assert!(
            outputs.iter().all(|&value| value == 0.0),
            "expected silence until $400F reload"
        );
        assert_eq!(read_status(&mut device) & 0x08, 0);

        write_register(&mut device, 0x400F, 0xF8);
        clock_cycles(&apu, 1);

        let outputs = clock_and_collect_samples(&apu, 200);
        assert!(
            outputs.iter().any(|&value| value > 0.0),
            "expected output after $400F reload"
        );
        assert_ne!(read_status(&mut device) & 0x08, 0);
    }

    #[test]
    fn test_noise_mixer_level_varies_with_dmc_level() {
        let (apu, mut device) = create_noise_device();
        {
            let mut apu_mut = apu.borrow_mut();
            apu_mut.set_dmc_enabled(true);
        }
        configure_noise_constant_volume(&mut device, &apu, 15, 0, 0x1F, false, true);

        write_register(&mut device, 0x4015, 0x18); // enable noise + dmc
        write_register(&mut device, 0x4011, 0x00);
        let low_samples = clock_and_collect_samples(&apu, 512);
        let avg_low = low_samples.iter().sum::<f32>() / low_samples.len() as f32;

        write_register(&mut device, 0x4011, 0x40);
        let high_samples = clock_and_collect_samples(&apu, 512);
        let avg_high = high_samples.iter().sum::<f32>() / high_samples.len() as f32;

        assert!(
            avg_high > avg_low,
            "expected mixed output to increase with DMC level"
        );
    }

    #[test]
    fn test_noise_ultrasonic_rates_are_audibly_filtered_behavior() {
        let (apu_fast, mut device_fast) = create_noise_device();
        configure_noise_constant_volume(&mut device_fast, &apu_fast, 15, 0, 0x1F, false, true);
        let fast_samples = clock_and_collect_samples(&apu_fast, 100_000);
        let fast_transitions = count_transitions(&fast_samples);

        let (apu_slow, mut device_slow) = create_noise_device();
        configure_noise_constant_volume(&mut device_slow, &apu_slow, 15, 15, 0x1F, false, true);
        let slow_samples = clock_and_collect_samples(&apu_slow, 100_000);
        let slow_transitions = count_transitions(&slow_samples);

        assert!(
            fast_transitions > slow_transitions * 10,
            "expected ultrasonic rates to change output much more rapidly"
        );
    }
}
