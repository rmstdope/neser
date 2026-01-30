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

    /// Run a ROM for a fixed number of CPU cycles and collect pulse-only audio samples.
    ///
    /// This configures the APU to output a single pulse channel and disables other channels.
    fn collect_pulse_samples(
        rom_path: &str,
        channel: ApuPulseChannel,
        total_cycles: u32,
    ) -> Vec<f32> {
        let rom_data = fs::read(rom_path).expect("ROM should load");
        let cartridge = Cartridge::new(&rom_data).expect("ROM should parse");

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

    /// Collect samples for the apu_phase_reset ROM over a fixed window.
    fn collect_apu_phase_reset_samples(channel: ApuPulseChannel) -> Vec<f32> {
        let total_cycles = NTSC_CPU_CYCLES_PER_FRAME * 5;
        collect_pulse_samples(
            "roms/automated_tests/apu_phase_reset/apu_phase_reset.nes",
            channel,
            total_cycles,
        )
    }

    /// Compute a midpoint threshold between min/max sample values.
    ///
    /// Returns `None` when the samples are empty or flat.
    fn compute_threshold(samples: &[f32]) -> Option<f32> {
        if samples.is_empty() {
            return None;
        }

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

        if max > min {
            Some((min + max) * 0.5)
        } else {
            None
        }
    }

    /// Collect indices where the waveform crosses the threshold from low to high.
    fn collect_rising_edges(samples: &[f32], threshold: f32) -> Vec<usize> {
        let mut rising_edges = Vec::new();
        for index in 1..samples.len() {
            if samples[index - 1] < threshold && samples[index] >= threshold {
                rising_edges.push(index);
            }
        }

        rising_edges
    }

    /// Compute a threshold and rising edges for a non-empty waveform.
    fn rising_edges_with_threshold(samples: &[f32]) -> (f32, Vec<usize>) {
        assert!(!samples.is_empty(), "no samples captured");

        let threshold = compute_threshold(samples).expect("samples appear constant");
        let rising_edges = collect_rising_edges(samples, threshold);

        assert!(
            rising_edges.len() >= 3,
            "expected at least 3 rising edges, got {}",
            rising_edges.len()
        );

        (threshold, rising_edges)
    }

    /// Measure successive rising-edge periods in samples.
    fn rising_edge_periods(samples: &[f32]) -> Vec<f32> {
        let (_threshold, rising_edges) = rising_edges_with_threshold(samples);

        let mut periods = Vec::new();
        for window in rising_edges.windows(2) {
            periods.push((window[1] - window[0]) as f32);
        }

        periods
    }

    /// Return the first rising-edge index, if a crossing is found.
    fn first_rising_edge_index(samples: &[f32]) -> Option<usize> {
        let threshold = compute_threshold(samples)?;
        collect_rising_edges(samples, threshold).into_iter().next()
    }

    /// Compute a normalized correlation coefficient with DC offset removed.
    fn normalized_correlation(a: &[f32], b: &[f32]) -> f32 {
        if a.is_empty() || b.is_empty() {
            return 0.0;
        }

        let mean_a = a.iter().copied().sum::<f32>() / a.len() as f32;
        let mean_b = b.iter().copied().sum::<f32>() / b.len() as f32;

        let mut dot = 0.0;
        let mut norm_a = 0.0;
        let mut norm_b = 0.0;
        for (&x, &y) in a.iter().zip(b.iter()) {
            let xa = x - mean_a;
            let yb = y - mean_b;
            dot += xa * yb;
            norm_a += xa * xa;
            norm_b += yb * yb;
        }

        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }

        dot / (norm_a.sqrt() * norm_b.sqrt())
    }

    /// Compute the maximum absolute correlation between two signals within a lag window.
    fn max_abs_correlation_with_lag(a: &[f32], b: &[f32], max_lag: usize) -> f32 {
        if a.is_empty() || b.is_empty() {
            return 0.0;
        }

        let mut best = 0.0f32;
        let max_lag = max_lag
            .min(a.len().saturating_sub(1))
            .min(b.len().saturating_sub(1));

        for lag in 0..=max_lag {
            let len = a.len().saturating_sub(lag).min(b.len());
            if len > 0 {
                let corr = normalized_correlation(&a[lag..lag + len], &b[..len]).abs();
                if corr > best {
                    best = corr;
                }
            }

            if lag > 0 {
                let len = b.len().saturating_sub(lag).min(a.len());
                if len > 0 {
                    let corr = normalized_correlation(&a[..len], &b[lag..lag + len]).abs();
                    if corr > best {
                        best = corr;
                    }
                }
            }
        }

        best
    }

    /// Load a WAV file and return mono samples plus sample rate.
    fn read_wav_mono_samples(path: &str) -> (Vec<f32>, u32) {
        let mut reader = hound::WavReader::open(path)
            .unwrap_or_else(|err| panic!("failed to open wav {}: {}", path, err));
        let spec = reader.spec();
        let channels = spec.channels as usize;
        assert!(channels >= 1, "wav has no channels");

        let mut samples = Vec::new();
        let mut frame_sum = 0.0f32;
        let mut frame_count = 0usize;

        match spec.sample_format {
            hound::SampleFormat::Float => {
                for sample in reader.samples::<f32>() {
                    let value = sample.expect("failed to read wav sample");
                    frame_sum += value;
                    frame_count += 1;
                    if frame_count == channels {
                        samples.push(frame_sum / channels as f32);
                        frame_sum = 0.0;
                        frame_count = 0;
                    }
                }
            }
            hound::SampleFormat::Int => {
                if spec.bits_per_sample == 8 {
                    for sample in reader.samples::<i8>() {
                        let raw = sample.expect("failed to read wav sample") as u8;
                        let centered = (raw as f32 - 128.0) / 128.0;
                        frame_sum += centered;
                        frame_count += 1;
                        if frame_count == channels {
                            samples.push(frame_sum / channels as f32);
                            frame_sum = 0.0;
                            frame_count = 0;
                        }
                    }
                } else {
                    let scale = (1u64 << (spec.bits_per_sample - 1)) as f32;
                    for sample in reader.samples::<i32>() {
                        let value = sample.expect("failed to read wav sample") as f32 / scale;
                        frame_sum += value;
                        frame_count += 1;
                        if frame_count == channels {
                            samples.push(frame_sum / channels as f32);
                            frame_sum = 0.0;
                            frame_count = 0;
                        }
                    }
                }
            }
        }

        (samples, spec.sample_rate)
    }

    /// Upsample a signal by an integer factor using sample repetition.
    fn upsample_repeat(samples: &[f32], factor: usize) -> Vec<f32> {
        if factor <= 1 {
            return samples.to_vec();
        }

        let mut out = Vec::with_capacity(samples.len() * factor);
        for &sample in samples {
            out.extend(std::iter::repeat_n(sample, factor));
        }
        out
    }

    /// Find the first window index where the RMS stays above a threshold for a run.
    fn steady_start_index(rms: &[f32], threshold_ratio: f32, min_run: usize) -> Option<usize> {
        if rms.is_empty() || min_run == 0 {
            return None;
        }

        let max_rms = rms.iter().copied().fold(0.0f32, f32::max);
        if max_rms == 0.0 {
            return None;
        }

        let threshold = max_rms * threshold_ratio;
        let mut run = 0usize;
        for (index, &value) in rms.iter().enumerate() {
            if value >= threshold {
                run += 1;
                if run >= min_run {
                    return Some(index + 1 - min_run);
                }
            } else {
                run = 0;
            }
        }

        None
    }

    /// Compute RMS values over sliding windows.
    fn rms_windows(samples: &[f32], window_size: usize, hop_size: usize) -> Vec<f32> {
        if window_size == 0 || hop_size == 0 || samples.len() < window_size {
            return Vec::new();
        }

        let mut rms = Vec::new();
        let mut start = 0usize;
        while start + window_size <= samples.len() {
            let mut sum = 0.0f32;
            for &sample in &samples[start..start + window_size] {
                sum += sample * sample;
            }
            rms.push((sum / window_size as f32).sqrt());
            start += hop_size;
        }

        rms
    }

    /// Skip an initial warmup window to avoid power-on transients.
    fn trim_warmup(samples: &[f32], warmup_samples: usize) -> &[f32] {
        if samples.len() > warmup_samples {
            &samples[warmup_samples..]
        } else {
            samples
        }
    }

    /// Analyze a pulse waveform for period, duty cycle, and peak amplitude.
    fn analyze_pulse_samples(samples: &[f32]) -> PulseAnalysis {
        assert!(!samples.is_empty(), "no samples captured");

        const WARMUP_SAMPLES: usize = 2_000;
        let samples = trim_warmup(samples, WARMUP_SAMPLES);

        let (threshold, rising_edges) = rising_edges_with_threshold(samples);

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
            peak: samples.iter().copied().fold(f32::NEG_INFINITY, f32::max),
        }
    }

    /// Convert a pulse timer value into an expected sample period (NTSC timing).
    fn expected_pulse_period_samples(timer: u16) -> f32 {
        let cycles_per_sample = CPU_CLOCK_NTSC / SAMPLE_RATE_HZ;
        let period_cycles = 16.0 * (timer as f32 + 1.0);
        period_cycles / cycles_per_sample
    }

    /// Convert a CPU-cycle offset into samples (NTSC timing).
    fn expected_phase_offset_samples(cpu_cycles: u32) -> f32 {
        let cycles_per_sample = CPU_CLOCK_NTSC / SAMPLE_RATE_HZ;
        cpu_cycles as f32 / cycles_per_sample
    }

    /// Verify that exactly one DMC byte (0x55) was processed by alternating steps.
    ///
    /// The final pulse tone never plays because we stop at the infinite loop.
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

    /// Count alternating small-amplitude steps while ignoring flat regions and large jumps.
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

    /// Verify that two DMC bytes (0x55) are processed four times.
    ///
    /// The DMC continues processing buffered bits even after the output is forced to 0x32.
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

    /// Check that exactly one IRQ has been fired from the DMC.
    fn check_one_irq_fired(nes: &mut Nes) -> bool {
        let irq_count = nes.apu.borrow().dmc().debug_irq_trigger_count();
        assert_eq!(irq_count, 1, "expected 1 IRQ fired, got {}", irq_count);
        true
    }

    /// Check that exactly zero IRQs have been fired from the DMC.
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

    // square_timer_div2
    #[test]
    fn test_square_timer_div2() {
        // Run the ROM long enough to cover the pre-loop delay, the loop body, and
        // the post-loop tones for verification against the reference WAV.
        let cycles_per_ms = CPU_CLOCK_NTSC / 1000.0;
        let pre_loop_cycles = (cycles_per_ms * 350.0) as u32; // 250ms + 100ms delay
        let loop_cycles = 1792u32 * 256;
        let post_cycles = (cycles_per_ms * 600.0) as u32; // 250ms + 250ms + buffer
        let total_cycles = pre_loop_cycles + loop_cycles + post_cycles;

        let samples = collect_pulse_samples(
            "roms/automated_tests/square_timer_div2/square_timer_div2.nes",
            ApuPulseChannel::Pulse1,
            total_cycles,
        );

        const WARMUP_SAMPLES: usize = 2_000;
        let samples = trim_warmup(&samples, WARMUP_SAMPLES);

        let cycles_per_sample = CPU_CLOCK_NTSC / SAMPLE_RATE_HZ;
        let pre_loop_cycles = CPU_CLOCK_NTSC * 0.35;
        let loop_cycles = 1792.0 * 256.0;

        // Focus the analysis window on the middle half of the loop to avoid edges.
        let loop_start_sample = (pre_loop_cycles / cycles_per_sample) as usize;
        let loop_window_start =
            loop_start_sample + ((loop_cycles * 0.25) / cycles_per_sample) as usize;
        let loop_window_end =
            loop_start_sample + ((loop_cycles * 0.75) / cycles_per_sample) as usize;

        let window_start = loop_window_start.min(samples.len());
        let window_end = loop_window_end.min(samples.len());
        assert!(
            window_end > window_start + 100,
            "not enough samples captured for loop analysis"
        );

        let window = &samples[window_start..window_end];
        let periods = rising_edge_periods(window);

        let expected_223 = expected_pulse_period_samples(223);
        let expected_255 = expected_pulse_period_samples(255);
        let tolerance = 1.0;

        let mut count_223 = 0usize;
        let mut count_255 = 0usize;
        let mut avg_period = 0.0f32;
        for period in &periods {
            avg_period += *period;
            if (period - expected_223).abs() <= tolerance {
                count_223 += 1;
            }
            if (period - expected_255).abs() <= tolerance {
                count_255 += 1;
            }
        }
        avg_period /= periods.len() as f32;

        assert!(count_223 >= 3, "expected 223-like periods during loop");
        assert!(
            (avg_period - expected_223).abs() < (avg_period - expected_255).abs(),
            "loop period closer to 255 than 223: avg={} (223={}, 255={})",
            avg_period,
            expected_223,
            expected_255
        );
        assert!(
            count_223 > count_255,
            "expected more 223-like periods than 255-like periods"
        );

        // WAV correlation: compare a aligned window to the golden reference.
        let (wav_samples, wav_rate) =
            read_wav_mono_samples("roms/automated_tests/square_timer_div2/correct.wav");
        assert_eq!(wav_rate, SAMPLE_RATE_HZ as u32, "wav sample rate mismatch");

        let wav_edge =
            first_rising_edge_index(&wav_samples).expect("failed to find rising edge in wav");
        let emu_edge =
            first_rising_edge_index(samples).expect("failed to find rising edge in emu output");

        let max_len = (SAMPLE_RATE_HZ as usize)
            .min(wav_samples.len().saturating_sub(wav_edge))
            .min(samples.len().saturating_sub(emu_edge));
        assert!(max_len > 1000, "not enough samples for correlation");

        let wav_slice = &wav_samples[wav_edge..wav_edge + max_len];
        let emu_slice = &samples[emu_edge..emu_edge + max_len];
        let correlation = max_abs_correlation_with_lag(wav_slice, emu_slice, 200);
        assert!(
            correlation > 0.8,
            "expected strong wav correlation magnitude, got {}",
            correlation
        );
    }

    // test_apu_env
    #[test]
    fn test_apu_env() {
        // Load the reference WAV and match sample rate to the emulator output.
        let (wav_samples, wav_rate) =
            read_wav_mono_samples("roms/automated_tests/test_apu_env/test_apu_env.wav");
        let wav_samples = if wav_rate != SAMPLE_RATE_HZ as u32 {
            let factor = (SAMPLE_RATE_HZ as u32 / wav_rate) as usize;
            assert_eq!(
                wav_rate * factor as u32,
                SAMPLE_RATE_HZ as u32,
                "wav sample rate mismatch"
            );
            upsample_repeat(&wav_samples, factor)
        } else {
            wav_samples
        };

        let cycles_per_sample = CPU_CLOCK_NTSC / SAMPLE_RATE_HZ;
        const WARMUP_SAMPLES: usize = 2_000;
        // Capture slightly longer than the WAV to allow warmup and alignment slack.
        let capture_samples = wav_samples.len() + WARMUP_SAMPLES + 2_000;
        let total_cycles = (capture_samples as f32 * cycles_per_sample) as u32;

        let samples = collect_pulse_samples(
            "roms/automated_tests/test_apu_env/test_apu_env.nes",
            ApuPulseChannel::Pulse1,
            total_cycles,
        );
        let samples = trim_warmup(&samples, WARMUP_SAMPLES);

        // Align on the first rising edge to compare equivalent waveform segments.
        let wav_edge =
            first_rising_edge_index(&wav_samples).expect("failed to find rising edge in wav");
        let emu_edge =
            first_rising_edge_index(samples).expect("failed to find rising edge in emu output");

        let max_len = (SAMPLE_RATE_HZ as usize)
            .min(wav_samples.len().saturating_sub(wav_edge))
            .min(samples.len().saturating_sub(emu_edge));
        assert!(max_len > 1000, "not enough samples for correlation");

        let wav_slice = &wav_samples[wav_edge..wav_edge + max_len];
        let emu_slice = &samples[emu_edge..emu_edge + max_len];

        // Compare envelope shapes using RMS windows.
        let window_size = (SAMPLE_RATE_HZ as usize / 100).max(1); // 10ms
        let hop_size = (window_size / 2).max(1);
        let wav_rms = rms_windows(wav_slice, window_size, hop_size);
        let emu_rms = rms_windows(emu_slice, window_size, hop_size);

        assert!(!wav_rms.is_empty(), "wav rms windowing produced no samples");
        assert!(!emu_rms.is_empty(), "emu rms windowing produced no samples");

        let env_correlation = max_abs_correlation_with_lag(&wav_rms, &emu_rms, 20);
        assert!(
            env_correlation > 0.9,
            "expected strong envelope correlation, got {}",
            env_correlation
        );

        // Locate steady-state sections and compare waveform correlation there.
        let wav_steady = steady_start_index(&wav_rms, 0.9, 10)
            .expect("failed to find steady region in wav envelope");
        let emu_steady = steady_start_index(&emu_rms, 0.9, 10)
            .expect("failed to find steady region in emu envelope");

        let wav_start = wav_steady * hop_size;
        let emu_start = emu_steady * hop_size;
        let steady_len = (SAMPLE_RATE_HZ as usize / 2)
            .min(wav_slice.len().saturating_sub(wav_start))
            .min(emu_slice.len().saturating_sub(emu_start));
        assert!(
            steady_len > 1000,
            "not enough steady samples for correlation"
        );

        let _wav_steady_slice = &wav_slice[wav_start..wav_start + steady_len];
        let _emu_steady_slice = &emu_slice[emu_start..emu_start + steady_len];

        // Ensure the steady-state envelope stays within a tight band.
        let steady_start = emu_rms.len() * 3 / 4;
        let steady_slice = &emu_rms[steady_start..];
        let mean = steady_slice.iter().sum::<f32>() / steady_slice.len() as f32;
        let max_dev = steady_slice
            .iter()
            .map(|value| (value - mean).abs())
            .fold(0.0f32, f32::max);
        assert!(
            max_dev <= mean * 0.15,
            "steady envelope deviates too much (max_dev={}, mean={})",
            max_dev,
            mean
        );
    }

    // TODO test_apu_sweep

    // TODO test_apu_timers

    // TODO test_tri_lin_ctr

    // TODO volume_tests
}
