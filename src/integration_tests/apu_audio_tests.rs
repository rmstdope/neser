#[cfg(test)]
mod tests {
    use crate::cartridge::Cartridge;
    use crate::console::{Nes, TvSystem};
    use crate::{setup_rom_address_test, setup_rom_test};
    use std::fs;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ApuPulseChannel {
        Pulse1,
        Pulse2,
    }

    #[derive(Debug, Clone, Copy)]
    struct PulseAnalysis {
        first_rising_edge: usize,
        period_samples: f32,
        duty_cycle: f32,
        peak: f32,
    }

    const NTSC_CPU_CYCLES_PER_FRAME: u32 = 29_780;
    const CPU_CLOCK_NTSC: f32 = 1_789_773.0;
    const SAMPLE_RATE_HZ: f32 = 44_100.0;

    fn collect_apu_phase_reset_samples(channel: ApuPulseChannel) -> Vec<f32> {
        let rom_path = "roms/automated_tests/apu_phase_reset/apu_phase_reset.nes";
        let rom_data = fs::read(rom_path).expect("apu_phase_reset ROM should load");
        let cartridge = Cartridge::new(&rom_data).expect("apu_phase_reset ROM should parse");

        let mut nes = Nes::new(TvSystem::Ntsc);
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        {
            let mut apu = nes.apu.borrow_mut();
            apu.set_sample_rate(SAMPLE_RATE_HZ);
            apu.set_triangle_enabled(false);
            apu.set_noise_enabled(false);
            apu.set_dmc_enabled(false);
            match channel {
                ApuPulseChannel::Pulse1 => {
                    apu.set_pulse1_enabled(true);
                    apu.set_pulse2_enabled(false);
                }
                ApuPulseChannel::Pulse2 => {
                    apu.set_pulse1_enabled(false);
                    apu.set_pulse2_enabled(true);
                }
            }
        }

        let mut samples = Vec::new();
        let total_cycles = NTSC_CPU_CYCLES_PER_FRAME * 5;
        for _cycle in 0..total_cycles {
            nes.run_cpu_tick();
            while nes.sample_ready() {
                if let Some(sample) = nes.get_sample() {
                    samples.push(sample);
                }
            }
        }

        samples
    }

    fn analyze_pulse_samples(samples: &[f32]) -> PulseAnalysis {
        assert!(!samples.is_empty(), "no samples captured");

        const WARMUP_SAMPLES: usize = 2_000;
        let samples = if samples.len() > WARMUP_SAMPLES {
            &samples[WARMUP_SAMPLES..]
        } else {
            samples
        };

        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for &sample in samples {
            if sample < min {
                min = sample;
            }
            if sample > max {
                max = sample;
            }
        }
        assert!(max > min, "samples appear constant");

        let threshold = (min + max) * 0.5;
        let mut rising_edges = Vec::new();
        for index in 1..samples.len() {
            if samples[index - 1] < threshold && samples[index] >= threshold {
                rising_edges.push(index);
            }
        }

        assert!(
            rising_edges.len() >= 3,
            "expected at least 3 rising edges, got {}",
            rising_edges.len()
        );

        let mut periods = Vec::new();
        for window in rising_edges.windows(2).take(6) {
            periods.push((window[1] - window[0]) as f32);
        }
        let period_samples = periods.iter().sum::<f32>() / periods.len() as f32;

        let mut duty_cycles = Vec::new();
        for window in rising_edges.windows(2).take(6) {
            let start = window[0];
            let end = window[1];
            let mut high = 0usize;
            for &sample in &samples[start..end] {
                if sample >= threshold {
                    high += 1;
                }
            }
            let period = (end - start) as f32;
            duty_cycles.push(high as f32 / period);
        }
        let duty_cycle = duty_cycles.iter().sum::<f32>() / duty_cycles.len() as f32;

        PulseAnalysis {
            first_rising_edge: rising_edges[0],
            period_samples,
            duty_cycle,
            peak: max,
        }
    }

    fn expected_pulse_period_samples(timer: u16) -> f32 {
        let cycles_per_sample = CPU_CLOCK_NTSC / SAMPLE_RATE_HZ;
        let period_cycles = 16.0 * (timer as f32 + 1.0);
        period_cycles / cycles_per_sample
    }

    fn expected_phase_offset_samples(cpu_cycles: u32) -> f32 {
        let cycles_per_sample = CPU_CLOCK_NTSC / SAMPLE_RATE_HZ;
        cpu_cycles as f32 / cycles_per_sample
    }

    // Test that the dmc is processing exactly one byte (0x55) -> alternating eight times
    // The final pulse tone will never play as we are stopping at first sight of the infinite loop
    fn check_one_dmc_byte_processed(nes: &mut Nes) -> bool {
        let mut samples = Vec::new();
        while nes.sample_ready() {
            samples.push(nes.get_sample().unwrap());
        }
        let mut expect_up = true;
        // First sample is garbage (0)
        let mut prev = samples[1];
        let mut alternations = 0;
        for &next in samples.iter().skip(2) {
            if next == prev {
                continue;
            }
            if expect_up {
                assert!(next > prev, "expected up step: {} -> {}", prev, next);
            } else {
                assert!(next < prev, "expected down step: {} -> {}", prev, next);
            }
            expect_up = !expect_up;
            prev = next;
            alternations += 1;
        }
        assert_eq!(
            alternations, 8,
            "expected 8 alternations, got {}",
            alternations
        );

        true
    }

    fn max_alternating_small_steps(samples: &[f32]) -> usize {
        const MIN_STEP: f32 = 0.000_05;
        const BIG_JUMP: f32 = 0.02;

        let mut count = 0usize;
        let mut last_dir: i32 = 0;
        let mut prev = match samples.first() {
            Some(value) => *value,
            None => return 0,
        };

        for &next in samples.iter().skip(1) {
            let delta = next - prev;
            let abs_delta = delta.abs();

            if abs_delta < MIN_STEP {
                prev = next;
                continue;
            }

            if abs_delta >= BIG_JUMP {
                prev = next;
                last_dir = 0;
                // println!("Big jump to {}", next);
                continue;
            }

            // println!("Processing {} count {}", next, count + 1);
            let dir = if delta > 0.0 { 1 } else { -1 };
            assert!(
                last_dir == 0 || dir != last_dir,
                "last_dir={}, dir={}, prev={}, next={}",
                last_dir,
                dir,
                prev,
                next
            );
            count += 1;
            last_dir = dir;

            prev = next;
        }

        count
    }

    // Test that the dmc is processing two bytes (0x55) four times
    // Sometime during the first byte, the output will be set to 0x32 but the DMC will keep
    // processing the remaining bits in that byte as well as the next byte already loaded into
    // sample buffer.
    // The final pulse tone will never play as we are stopping at first sight of the infinite loop
    fn check_four_by_two_dmc_bytes_processed(nes: &mut Nes) -> bool {
        let mut samples = Vec::new();
        while nes.sample_ready() {
            let sample = nes.get_sample().unwrap();
            samples.push(sample);
        }
        let alternations = max_alternating_small_steps(&samples);
        assert_eq!(alternations, 16 * 4);

        true
    }

    // Check that exaclty one IRQ has been fired from the DMC
    fn check_one_irq_fired(nes: &mut Nes) -> bool {
        let irq_count = nes.apu.borrow().dmc().debug_irq_trigger_count();
        assert_eq!(irq_count, 1, "expected 1 IRQ fired, got {}", irq_count);
        true
    }

    // Check that exaclty zero IRQ has been fired from the DMC
    fn check_zero_irq_fired(nes: &mut Nes) -> bool {
        let irq_count = nes.apu.borrow().dmc().debug_irq_trigger_count();
        assert_eq!(irq_count, 0, "expected 0 IRQ fired, got {}", irq_count);
        true
    }

    // apu_mixer
    setup_rom_test!(test_apu_mixer_dmc, "roms/automated_tests/apu_mixer/dmc.nes");
    setup_rom_test!(
        test_apu_mixer_noise,
        "roms/automated_tests/apu_mixer/noise.nes"
    );
    setup_rom_test!(
        test_apu_mixer_square,
        "roms/automated_tests/apu_mixer/square.nes"
    );
    setup_rom_test!(
        test_apu_mixer_triangle,
        "roms/automated_tests/apu_mixer/triangle.nes"
    );

    // apu_phase_reset
    #[test]
    fn test_apu_phase_reset() {
        let pulse1_samples = collect_apu_phase_reset_samples(ApuPulseChannel::Pulse1);
        let pulse2_samples = collect_apu_phase_reset_samples(ApuPulseChannel::Pulse2);

        let pulse1 = analyze_pulse_samples(&pulse1_samples);
        let pulse2 = analyze_pulse_samples(&pulse2_samples);

        let expected_period = expected_pulse_period_samples(0x81);
        let period_tolerance = 0.1;
        assert!(
            (pulse1.period_samples - expected_period).abs() <= period_tolerance,
            "pulse1 period {} not within {} samples of expected {}",
            pulse1.period_samples,
            period_tolerance,
            expected_period
        );
        assert!(
            (pulse2.period_samples - expected_period).abs() <= period_tolerance,
            "pulse2 period {} not within {} samples of expected {}",
            pulse2.period_samples,
            period_tolerance,
            expected_period
        );

        let phase_offset_samples =
            pulse2.first_rising_edge.abs_diff(pulse1.first_rising_edge) as f32;
        let expected_phase_offset = expected_phase_offset_samples(256);
        let phase_tolerance = 1.0;
        assert!(
            (phase_offset_samples - expected_phase_offset).abs() <= phase_tolerance,
            "phase offset {} not within {} samples of expected {}",
            phase_offset_samples,
            phase_tolerance,
            expected_phase_offset
        );

        let duty_tolerance = 0.01;
        assert!(
            (pulse1.duty_cycle - 0.5).abs() <= duty_tolerance,
            "pulse1 duty {} not within {} of expected 0.5",
            pulse1.duty_cycle,
            duty_tolerance
        );
        assert!(
            (pulse2.duty_cycle - 0.5).abs() <= duty_tolerance,
            "pulse2 duty {} not within {} of expected 0.5",
            pulse2.duty_cycle,
            duty_tolerance
        );

        let peak_tolerance = 1e-4;
        assert!(
            (pulse1.peak - pulse2.peak).abs() <= peak_tolerance,
            "pulse peaks differ more than tolerance: {} vs {}",
            pulse1.peak,
            pulse2.peak
        );
    }

    // dmc_tests
    setup_rom_address_test!(
        test_dmc_tests_buffer_retained,
        "roms/automated_tests/dmc_tests/buffer_retained.nes",
        0xE149,
        check_one_dmc_byte_processed
    );
    setup_rom_address_test!(
        test_dmc_tests_latency,
        "roms/automated_tests/dmc_tests/latency.nes",
        0xE162,
        check_four_by_two_dmc_bytes_processed
    );
    setup_rom_address_test!(
        test_dmc_tests_status_irq,
        "roms/automated_tests/dmc_tests/status_irq.nes",
        0xE154,
        check_one_irq_fired
    );
    setup_rom_address_test!(
        test_dmc_tests_status,
        "roms/automated_tests/dmc_tests/status.nes",
        0xE14E,
        check_zero_irq_fired
    );

    // TODO fadeout_and_triangle_tests

    // TODO square_timer_div2

    // TODO test_apu_env

    // TODO test_apu_sweep

    // TODO test_apu_timers

    // TODO test_tri_lin_ctr

    // TODO volume_tests
}
