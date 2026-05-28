#[cfg(test)]
mod tests {
    use crate::nes::apu::Apu;
    use crate::nes::bus::BusDevice;
    use crate::nes::bus::apu_device::ApuDevice;
    use crate::nes::console::TimingMode;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn create_apu_with_sample_rate(sample_rate: f32) -> (Rc<RefCell<Apu>>, ApuDevice) {
        let apu = Rc::new(RefCell::new(Apu::new()));
        {
            let mut apu_mut = apu.borrow_mut();
            apu_mut.set_sample_rate(sample_rate);
            apu_mut.set_pulse1_enabled(false);
            apu_mut.set_pulse2_enabled(false);
            apu_mut.set_noise_enabled(false);
            apu_mut.set_dmc_enabled(false);
            apu_mut.set_triangle_enabled(true);
        }
        let device = ApuDevice::new(apu.clone());
        (apu, device)
    }

    fn create_triangle_only_apu() -> (Rc<RefCell<Apu>>, ApuDevice) {
        create_apu_with_sample_rate(TimingMode::Ntsc.cpu_clock_hz())
    }

    fn write_register(device: &mut ApuDevice, addr: u16, value: u8) {
        assert!(device.write(addr, value, false));
    }

    fn enable_triangle_channel(apu: &Rc<RefCell<Apu>>, device: &mut ApuDevice) {
        write_register(device, 0x4015, 0x04);
        apu.borrow_mut().set_triangle_enabled(true);
    }

    fn write_triangle_timer_and_length(device: &mut ApuDevice, timer: u16, length_index: u8) {
        write_register(device, 0x400A, (timer & 0x00FF) as u8);
        let timer_high = ((timer >> 8) as u8) & 0x07;
        let length_and_high = ((length_index & 0x1F) << 3) | timer_high;
        write_register(device, 0x400B, length_and_high);
    }

    fn configure_triangle(
        apu: &Rc<RefCell<Apu>>,
        device: &mut ApuDevice,
        timer: u16,
        length_index: u8,
        linear_reload: u8,
        control_flag: bool,
    ) {
        enable_triangle_channel(apu, device);
        let control_bit = if control_flag { 0x80 } else { 0x00 };
        write_register(device, 0x4008, control_bit | (linear_reload & 0x7F));
        write_triangle_timer_and_length(device, timer, length_index);

        let mut apu_mut = apu.borrow_mut();
        apu_mut.clock();
        let _ = apu_mut.get_sample();
        drop(apu_mut);
        clock_immediate_quarter_and_half(apu, device);
    }

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

    fn clock_cycles(apu: &Rc<RefCell<Apu>>, cycles: usize) {
        for _ in 0..cycles {
            let mut apu_mut = apu.borrow_mut();
            apu_mut.clock();
            let _ = apu_mut.get_sample();
        }
    }

    fn clock_immediate_quarter_and_half(apu: &Rc<RefCell<Apu>>, device: &mut ApuDevice) {
        write_register(device, 0x4017, 0x80);
        clock_cycles(apu, 4);
    }

    fn expected_triangle_sequence_without_double_zero() -> Vec<usize> {
        (0..=15).rev().chain(1..=15).collect()
    }

    fn is_rotation(expected: &[usize], actual: &[usize]) -> bool {
        if expected.len() != actual.len() {
            return false;
        }

        let mut doubled = Vec::with_capacity(expected.len() * 2);
        doubled.extend_from_slice(expected);
        doubled.extend_from_slice(expected);

        doubled.windows(actual.len()).any(|window| window == actual)
    }

    fn quantize_samples(samples: &[f32]) -> Vec<usize> {
        let mut unique: Vec<f32> = samples.to_vec();
        unique.sort_by(|a, b| a.partial_cmp(b).unwrap());
        unique.dedup_by(|a, b| a.to_bits() == b.to_bits());

        samples
            .iter()
            .map(|&value| {
                unique
                    .iter()
                    .position(|&entry| entry.to_bits() == value.to_bits())
                    .unwrap_or(0)
            })
            .collect()
    }

    fn run_lengths(outputs: &[usize]) -> Vec<(usize, usize)> {
        if outputs.is_empty() {
            return Vec::new();
        }

        let mut runs = Vec::new();
        let mut current = outputs[0];
        let mut count = 1usize;

        for &value in outputs.iter().skip(1) {
            if value == current {
                count += 1;
            } else {
                runs.push((current, count));
                current = value;
                count = 1;
            }
        }

        runs.push((current, count));
        runs
    }

    fn mode_run_length(runs: &[(usize, usize)]) -> usize {
        use std::collections::HashMap;

        let mut counts: HashMap<usize, usize> = HashMap::new();
        for &(_, length) in runs.iter().skip(1) {
            *counts.entry(length).or_insert(0) += 1;
        }

        counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(length, _)| length)
            .expect("expected run lengths")
    }

    fn has_nonzero_output(samples: &[f32]) -> bool {
        samples.iter().any(|&value| value > 0.0)
    }

    fn all_samples_equal_to(samples: &[f32], expected: f32) -> bool {
        samples
            .iter()
            .all(|&value| value.to_bits() == expected.to_bits())
    }

    fn read_status(device: &mut ApuDevice) -> u8 {
        device.read(0x4015, 0, false).unwrap_or(0)
    }

    #[test]
    fn test_triangle_waveform_sequence_is_32_step_15_to_0_to_15() {
        let (apu, mut device) = create_triangle_only_apu();
        let timer = 0x0002;
        configure_triangle(&apu, &mut device, timer, 1, 0x7F, true);

        let samples = collect_samples(&apu, 32 * (timer as usize + 1) * 2);
        let quantized = quantize_samples(&samples);
        let runs = run_lengths(&quantized);
        let values: Vec<usize> = runs.iter().map(|(value, _)| *value).collect();
        let expected = expected_triangle_sequence_without_double_zero();

        let has_expected_sequence = values
            .windows(expected.len())
            .any(|window| is_rotation(&expected, window));

        assert!(
            has_expected_sequence,
            "triangle sequence mismatch: expected rotation of {:?}, got {:?}",
            expected, values
        );

        let mode = mode_run_length(&runs);
        let has_double_zero_run = runs
            .iter()
            .any(|(value, length)| *value == 0 && *length >= 2 * mode);
        assert!(
            has_double_zero_run,
            "expected double-length run for 0 in 32-step sequence"
        );
    }

    #[test]
    fn test_triangle_timer_period_matches_fcpu_over_32_times_tval_plus_one() {
        let (apu, mut device) = create_triangle_only_apu();
        let timer = 0x0005;
        configure_triangle(&apu, &mut device, timer, 1, 0x7F, false);

        let expected_step = (timer as usize) + 1;

        let samples = collect_samples(&apu, 512);
        let quantized = quantize_samples(&samples);
        let runs = run_lengths(&quantized);
        let observed_step = mode_run_length(&runs);

        assert_eq!(
            observed_step, expected_step,
            "triangle timer period mismatch: expected {}, got {}",
            expected_step, observed_step
        );
    }

    #[test]
    fn test_triangle_timer_ticks_at_cpu_rate_not_cpu_div2() {
        let (apu, mut device) = create_triangle_only_apu();
        let timer = 0x0003;
        configure_triangle(&apu, &mut device, timer, 1, 0x7F, false);

        let samples = collect_samples(&apu, 200);
        let quantized = quantize_samples(&samples);
        let runs = run_lengths(&quantized);
        let min_step = mode_run_length(&runs);

        let expected = (timer + 1) as usize;
        let cpu_div2 = ((timer + 1) * 2) as usize;

        assert_eq!(
            min_step, expected,
            "expected CPU-rate timer period {}, got {}",
            expected, min_step
        );
        assert_ne!(
            min_step, cpu_div2,
            "triangle timer appears to tick at CPU/2"
        );
    }

    #[test]
    fn test_triangle_length_counter_gates_output_and_halting_stops_sequencer_advancement() {
        let (apu, mut device) = create_triangle_only_apu();
        let timer = 0x0008;
        configure_triangle(&apu, &mut device, timer, 3, 0x7F, false);

        let initial_outputs = collect_samples(&apu, 32);
        assert!(
            has_nonzero_output(&initial_outputs),
            "expected audible triangle output before length expires"
        );
        let held_output = *initial_outputs
            .last()
            .expect("expected active triangle samples before halt");

        for _ in 0..3 {
            clock_immediate_quarter_and_half(&apu, &mut device);
        }

        let halted_outputs = collect_samples(&apu, 64);
        assert!(
            all_samples_equal_to(&halted_outputs, held_output),
            "expected length counter to halt triangle on its last DAC output"
        );
    }

    #[test]
    fn test_triangle_linear_counter_reload_and_decrement_rules() {
        let (apu, mut device) = create_triangle_only_apu();
        enable_triangle_channel(&apu, &mut device);

        write_register(&mut device, 0x4008, 0x03); // reload=3, control off
        write_triangle_timer_and_length(&mut device, 0x0004, 0x1F);

        clock_immediate_quarter_and_half(&apu, &mut device);
        let outputs = collect_samples(&apu, 32);
        assert!(has_nonzero_output(&outputs), "expected output after reload");

        clock_immediate_quarter_and_half(&apu, &mut device);
        let outputs = collect_samples(&apu, 32);
        assert!(has_nonzero_output(&outputs), "expected output at counter=2");

        clock_immediate_quarter_and_half(&apu, &mut device);
        let outputs = collect_samples(&apu, 32);
        assert!(has_nonzero_output(&outputs), "expected output at counter=1");
        let held_output = *outputs
            .last()
            .expect("expected active triangle samples before linear halt");

        clock_immediate_quarter_and_half(&apu, &mut device);
        let outputs = collect_samples(&apu, 32);
        assert!(
            all_samples_equal_to(&outputs, held_output),
            "expected output held when linear counter reaches 0"
        );
    }

    #[test]
    fn test_triangle_control_flag_halts_length_counter_and_preserves_reload_flag() {
        let (apu, mut device) = create_triangle_only_apu();
        enable_triangle_channel(&apu, &mut device);

        write_register(&mut device, 0x4008, 0x80 | 0x02); // control=1, reload=2
        write_triangle_timer_and_length(&mut device, 0x0004, 3);

        clock_immediate_quarter_and_half(&apu, &mut device);
        let status_before = read_status(&mut device);

        for _ in 0..6 {
            clock_immediate_quarter_and_half(&apu, &mut device);
        }

        let status_after = read_status(&mut device);
        assert_eq!(
            status_before & 0x04,
            status_after & 0x04,
            "control flag should halt length counter"
        );
        let outputs = collect_samples(&apu, 64);
        assert!(
            has_nonzero_output(&outputs),
            "expected output with control flag set"
        );
    }

    #[test]
    fn test_triangle_write_400b_sets_linear_reload_flag_and_loads_length() {
        let (apu, mut device) = create_triangle_only_apu();
        enable_triangle_channel(&apu, &mut device);

        write_register(&mut device, 0x4008, 0x05);
        assert_eq!(read_status(&mut device) & 0x04, 0);

        write_triangle_timer_and_length(&mut device, 0x0002, 1);

        clock_immediate_quarter_and_half(&apu, &mut device);
        let outputs = collect_samples(&apu, 32);
        assert!(
            read_status(&mut device) & 0x04 != 0,
            "expected length counter to load on $400B write"
        );
        assert!(has_nonzero_output(&outputs), "expected output after reload");
    }

    #[test]
    fn test_triangle_write_4008_with_control_and_reload_flag_updates_next_reload_value() {
        let (apu, mut device) = create_triangle_only_apu();
        enable_triangle_channel(&apu, &mut device);

        write_register(&mut device, 0x4008, 0x04); // control off, reload 4
        write_triangle_timer_and_length(&mut device, 0x0002, 0x1F);
        clock_immediate_quarter_and_half(&apu, &mut device);

        let outputs = collect_samples(&apu, 32);
        assert!(
            has_nonzero_output(&outputs),
            "expected output after reload=4"
        );

        write_register(&mut device, 0x4008, 0x80); // control on, reload 0
        write_triangle_timer_and_length(&mut device, 0x0002, 0x1F);
        clock_immediate_quarter_and_half(&apu, &mut device);
        let outputs = collect_samples(&apu, 32);
        let held_output = *outputs.first().expect("expected held samples");
        assert!(
            all_samples_equal_to(&outputs, held_output),
            "expected output held with reload=0"
        );

        write_register(&mut device, 0x4008, 0x80 | 0x07); // control on, reload 7
        write_triangle_timer_and_length(&mut device, 0x0002, 0x1F);
        clock_immediate_quarter_and_half(&apu, &mut device);
        let outputs = collect_samples(&apu, 32);
        assert!(
            has_nonzero_output(&outputs),
            "expected output after reload updated to 7"
        );
    }

    #[test]
    fn test_triangle_output_resumes_from_same_phase_after_halt() {
        let (apu, mut device) = create_triangle_only_apu();
        let timer = 0x07FF;
        configure_triangle(&apu, &mut device, timer, 1, 0x7F, true);

        let sample_before = collect_samples(&apu, 1)[0];

        write_register(&mut device, 0x4015, 0x00);
        let halted_outputs = collect_samples(&apu, 16);
        assert!(all_samples_equal_to(&halted_outputs, sample_before));

        write_register(&mut device, 0x4015, 0x04);
        write_triangle_timer_and_length(&mut device, timer, 1);
        clock_cycles(&apu, 1);

        let sample_after = collect_samples(&apu, 1)[0];
        assert_eq!(
            sample_after, sample_before,
            "expected triangle sequencer to resume from same phase after halt"
        );
    }

    #[test]
    fn test_triangle_gates_on_both_length_and_linear_counters_nonzero() {
        let (apu, mut device) = create_triangle_only_apu();
        enable_triangle_channel(&apu, &mut device);

        write_register(&mut device, 0x4008, 0x00);
        write_triangle_timer_and_length(&mut device, 0x0002, 1);
        clock_immediate_quarter_and_half(&apu, &mut device);

        let outputs = collect_samples(&apu, 16);
        assert!(
            outputs.iter().all(|&value| value == 0.0),
            "expected initial DAC output held at zero when linear counter is zero"
        );

        write_register(&mut device, 0x4008, 0x04);
        write_triangle_timer_and_length(&mut device, 0x0002, 1);
        clock_immediate_quarter_and_half(&apu, &mut device);

        let outputs = collect_samples(&apu, 16);
        assert!(
            has_nonzero_output(&outputs),
            "expected output when both counters are non-zero"
        );
        let held_output = *outputs
            .last()
            .expect("expected active triangle samples before halt");

        write_register(&mut device, 0x4015, 0x00);
        let outputs = collect_samples(&apu, 16);
        assert!(
            all_samples_equal_to(&outputs, held_output),
            "expected triangle DAC to hold its last output when length counter is zero"
        );
    }

    #[test]
    fn test_triangle_disable_via_4015_clears_length_and_requires_400b_reload_to_resume() {
        let (apu, mut device) = create_triangle_only_apu();
        configure_triangle(&apu, &mut device, 0x0004, 1, 0x7F, false);

        let initial = collect_samples(&apu, 16);
        assert!(has_nonzero_output(&initial));

        write_register(&mut device, 0x4015, 0x00);
        assert_eq!(read_status(&mut device) & 0x04, 0);
        let held_output = *initial
            .last()
            .expect("expected active triangle samples before disable");
        let halted = collect_samples(&apu, 16);
        assert!(all_samples_equal_to(&halted, held_output));

        write_register(&mut device, 0x4015, 0x04);
        let still_halted = collect_samples(&apu, 16);
        assert!(
            all_samples_equal_to(&still_halted, held_output),
            "expected triangle to remain halted until $400B reload"
        );

        write_triangle_timer_and_length(&mut device, 0x0004, 1);
        clock_cycles(&apu, 1);

        let resumed = collect_samples(&apu, 16);
        assert!(has_nonzero_output(&resumed));
    }

    #[test]
    fn test_triangle_timer_values_0_or_1_produce_ultrasonic_mid_level_output() {
        for timer in [0u16, 1u16] {
            let (apu, mut device) = create_triangle_only_apu();
            configure_triangle(&apu, &mut device, timer, 1, 0x7F, true);

            let outputs = collect_samples(&apu, 64);
            let min = outputs.iter().copied().fold(f32::INFINITY, f32::min);
            let max = outputs.iter().copied().fold(0.0, f32::max);
            let average = outputs.iter().sum::<f32>() / outputs.len() as f32;

            assert!(
                min < max,
                "expected ultrasonic output to vary for timer {}",
                timer
            );
            assert!(
                (0.1..=0.2).contains(&average),
                "expected mid-level average output for timer {}, got {}",
                timer,
                average
            );
        }
    }

    #[test]
    fn test_triangle_linear_counter_max_127_limits_duration_in_quarter_frames() {
        let (apu, mut device) = create_triangle_only_apu();
        enable_triangle_channel(&apu, &mut device);

        write_register(&mut device, 0x4008, 0x7F); // reload 127, control off
        write_triangle_timer_and_length(&mut device, 0x0004, 0x01);

        clock_immediate_quarter_and_half(&apu, &mut device);
        let outputs = collect_samples(&apu, 16);
        assert!(has_nonzero_output(&outputs));

        for _ in (1..=126).rev() {
            clock_immediate_quarter_and_half(&apu, &mut device);
            let outputs = collect_samples(&apu, 16);
            assert!(has_nonzero_output(&outputs));
        }
        let held_output = *collect_samples(&apu, 1)
            .last()
            .expect("expected active triangle sample before duration expiry");

        clock_immediate_quarter_and_half(&apu, &mut device);
        let outputs = collect_samples(&apu, 16);
        assert!(all_samples_equal_to(&outputs, held_output));
    }

    #[test]
    fn test_triangle_mixer_level_varies_with_dmc_level() {
        let (apu, mut device) = create_apu_with_sample_rate(TimingMode::Ntsc.cpu_clock_hz());

        {
            let mut apu_mut = apu.borrow_mut();
            apu_mut.set_dmc_enabled(true);
            apu_mut.set_triangle_enabled(true);
            apu_mut.set_pulse1_enabled(false);
            apu_mut.set_pulse2_enabled(false);
            apu_mut.set_noise_enabled(false);
        }

        write_register(&mut device, 0x4015, 0x14); // triangle + dmc
        write_register(&mut device, 0x4008, 0x80 | 0x7F);
        write_triangle_timer_and_length(&mut device, 0x0000, 1);
        clock_cycles(&apu, 1);
        clock_immediate_quarter_and_half(&apu, &mut device);

        write_register(&mut device, 0x4011, 0x00);
        let low = collect_samples(&apu, 1)[0];

        write_register(&mut device, 0x4011, 0x40);
        let high = collect_samples(&apu, 1)[0];

        assert!(
            high > low,
            "expected mixer output to increase with DMC level"
        );
    }
}
