/// All of the test ROMs below are verified manually by listening to them and, where possible,
/// comparing to known-good audio captures from real hardware.
/// The audio analysis functions below are more of a fun exploration than strict test verifications,
/// but they could help catch gross APU emulation errors in the future.
#[cfg(test)]
mod tests {
    use crate::cartridge::Cartridge;
    use crate::console::{Config, Nes, TimingMode};
    use crate::integration_tests::rom_test_runner::tests::init_tracing_from_env;
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
    fn cpu_clock_ntsc() -> f32 {
        TimingMode::Ntsc.cpu_clock_hz()
    }
    const SAMPLE_RATE_HZ: f32 = 44_100.0;
    const WARMUP_SAMPLES: usize = 2_000;

    fn load_test_cartridge(rom_data: &[u8], rom_path: &str) -> Cartridge {
        Cartridge::load_from_file(rom_data, rom_path, crate::app_context::AppContext::new())
            .expect("ROM should parse")
    }

    /// Run a ROM for a fixed number of CPU cycles and collect pulse-only audio samples.
    ///
    /// This configures the APU to output a single pulse channel and disables other channels.
    fn collect_pulse_samples(
        rom_path: &str,
        channel: ApuPulseChannel,
        total_cycles: u32,
        enable_noise: bool,
    ) -> Vec<f32> {
        let rom_data = fs::read(rom_path).expect("ROM should load");
        let cartridge = load_test_cartridge(&rom_data, rom_path);

        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        {
            let mut apu = nes.apu().borrow_mut();
            apu.set_sample_rate(SAMPLE_RATE_HZ);
            apu.set_triangle_enabled(false);
            apu.set_noise_enabled(enable_noise);
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
        let mut cycles_run = 0u32;
        while cycles_run < total_cycles {
            let consumed = nes.run_cpu_tick() as u32;
            cycles_run = cycles_run.saturating_add(consumed.max(1));
            while nes.sample_ready() {
                if let Some(sample) = nes.get_sample() {
                    samples.push(sample);
                }
            }
        }

        samples
    }

    /// Collect samples while forcing channel enable flags each CPU tick.
    ///
    /// This overrides any $4015 writes in the ROM so we can isolate noise-only
    /// or pulse-only sequences from the same test program.
    fn collect_forced_channel_samples(
        rom_path: &str,
        total_cycles: u32,
        pulse1_enabled: bool,
        pulse2_enabled: bool,
        triangle_enabled: bool,
        noise_enabled: bool,
        dmc_enabled: bool,
    ) -> Vec<f32> {
        let rom_data = fs::read(rom_path).expect("ROM should load");
        let cartridge = load_test_cartridge(&rom_data, rom_path);

        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        {
            let mut apu = nes.apu().borrow_mut();
            apu.set_sample_rate(SAMPLE_RATE_HZ);
            apu.set_triangle_enabled(triangle_enabled);
            apu.set_dmc_enabled(dmc_enabled);
            apu.set_pulse1_enabled(pulse1_enabled);
            apu.set_pulse2_enabled(pulse2_enabled);
            apu.set_noise_enabled(noise_enabled);
        }

        let mut samples = Vec::new();
        let mut cycles_run = 0u32;
        while cycles_run < total_cycles {
            let consumed = nes.run_cpu_tick() as u32;
            cycles_run = cycles_run.saturating_add(consumed.max(1));

            {
                let mut apu = nes.apu().borrow_mut();
                apu.set_pulse1_enabled(pulse1_enabled);
                apu.set_pulse2_enabled(pulse2_enabled);
                apu.set_noise_enabled(noise_enabled);
                apu.set_triangle_enabled(triangle_enabled);
                apu.set_dmc_enabled(dmc_enabled);
            }

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
            false,
        )
    }
    /// Convert a capture length into total CPU cycles at the emulator sample rate.
    fn capture_cycles_for_samples(sample_len: usize, warmup: usize, extra: usize) -> u32 {
        let cycles_per_sample = cpu_clock_ntsc() / SAMPLE_RATE_HZ;
        let capture_samples = sample_len + warmup + extra;
        (capture_samples as f32 * cycles_per_sample) as u32
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
                        let value = sample.expect("failed to read wav sample");
                        let raw = value as u8;
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

    /// Load a WAV file and return mono samples at the emulator sample rate.
    fn load_wav_samples_at_rate(path: &str) -> Vec<f32> {
        let (wav_samples, wav_rate) = read_wav_mono_samples(path);
        if wav_rate != SAMPLE_RATE_HZ as u32 {
            assert!(
                wav_rate <= SAMPLE_RATE_HZ as u32,
                "wav sample rate must not exceed emulator rate"
            );
            let factor = (SAMPLE_RATE_HZ as u32 / wav_rate) as usize;
            assert_eq!(
                wav_rate * factor as u32,
                SAMPLE_RATE_HZ as u32,
                "wav sample rate mismatch"
            );
            upsample_repeat(&wav_samples, factor)
        } else {
            wav_samples
        }
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

    /// Count how many distinct period plateaus appear, based on a tolerance.
    fn count_period_segments(periods: &[f32], tolerance: f32) -> usize {
        if periods.is_empty() {
            return 0;
        }

        let mut segments = 1usize;
        let mut current = periods[0];
        for &period in periods.iter().skip(1) {
            if (period - current).abs() > tolerance {
                segments += 1;
                current = period;
            }
        }

        segments
    }

    fn median_value(values: &mut [f32]) -> f32 {
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = values.len() / 2;
        if values.len().is_multiple_of(2) {
            (values[mid - 1] + values[mid]) * 0.5
        } else {
            values[mid]
        }
    }

    /// Compute median period per fixed segment count, using equal-sized sample windows.
    fn median_periods_by_segments(
        samples: &[f32],
        segment_samples: usize,
        segment_count: usize,
    ) -> Vec<f32> {
        if segment_samples == 0 || segment_count == 0 {
            return Vec::new();
        }

        let mut medians = Vec::with_capacity(segment_count);
        for index in 0..segment_count {
            let start = index * segment_samples;
            let end = start + segment_samples;
            if end > samples.len() {
                break;
            }
            let window = &samples[start..end];
            let mut periods = period_series(window);
            if periods.is_empty() {
                break;
            }
            let median = median_value(&mut periods);
            medians.push(median);
        }

        medians
    }

    /// Build a period series from rising edges.
    fn period_series(samples: &[f32]) -> Vec<f32> {
        let (_threshold, rising_edges) = rising_edges_with_threshold(samples);
        let mut periods = Vec::new();
        for window in rising_edges.windows(2) {
            periods.push((window[1] - window[0]) as f32);
        }
        periods
    }

    fn find_period_run_start(
        samples: &[f32],
        expected_period: f32,
        tolerance: f32,
        min_run: usize,
    ) -> Option<usize> {
        let (_threshold, rising_edges) = rising_edges_with_threshold(samples);
        let mut run = 0usize;
        for window in rising_edges.windows(2) {
            let period = (window[1] - window[0]) as f32;
            if (period - expected_period).abs() <= tolerance {
                run += 1;
                if run >= min_run {
                    return Some(window[0]);
                }
            } else {
                run = 0;
            }
        }
        None
    }

    /// Skip an initial warmup window to avoid power-on transients.
    fn trim_warmup(samples: &[f32], warmup_samples: usize) -> &[f32] {
        if samples.len() > warmup_samples {
            &samples[warmup_samples..]
        } else {
            samples
        }
    }

    // Removes all leading zeros from the vector
    fn trim_leading_zeros(samples: &[f32]) -> &[f32] {
        let first_nonzero = samples
            .iter()
            .position(|&x| x != 0.0)
            .unwrap_or(samples.len());
        &samples[first_nonzero..]
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
        let cycles_per_sample = cpu_clock_ntsc() / SAMPLE_RATE_HZ;
        let period_cycles = 16.0 * (timer as f32 + 1.0);
        period_cycles / cycles_per_sample
    }

    /// Convert a CPU-cycle offset into samples (NTSC timing).
    fn expected_phase_offset_samples(cpu_cycles: u32) -> f32 {
        let cycles_per_sample = cpu_clock_ntsc() / SAMPLE_RATE_HZ;
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
    ///
    /// Expected alternations: 70. With the hardware-accurate DMC timer
    /// initialization (timer starts at full period, not zero), the output unit
    /// does not fire during the 7 CPU reset cycles. This shifts the output-unit
    /// phase relative to the ROM's instruction sequence, resulting in 70
    /// alternations instead of 64 (which was calibrated against the incorrect
    /// timer=0 behavior where the output unit fired immediately during reset).
    fn check_four_by_two_dmc_bytes_processed(nes: &mut Nes) -> bool {
        let mut samples = Vec::new();
        while nes.sample_ready() {
            let sample = nes.get_sample().unwrap();
            samples.push(sample);
        }
        let alternations = max_alternating_small_steps(&samples);
        assert_eq!(alternations, 70);

        true
    }

    /// Check that exactly one IRQ has been fired from the DMC.
    fn check_one_irq_fired(nes: &mut Nes) -> bool {
        let irq_count = nes.apu().borrow().dmc().debug_irq_trigger_count();
        assert_eq!(irq_count, 1, "expected 1 IRQ fired, got {}", irq_count);
        true
    }

    /// Check that exactly zero IRQs have been fired from the DMC.
    fn check_zero_irq_fired(nes: &mut Nes) -> bool {
        let irq_count = nes.apu().borrow().dmc().debug_irq_trigger_count();
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
        let expected_phase_offset = expected_phase_offset_samples(1280);
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
        let cycles_per_ms = cpu_clock_ntsc() / 1000.0;
        let pre_loop_cycles = (cycles_per_ms * 350.0) as u32; // 250ms + 100ms delay
        let loop_cycles = 1792u32 * 256;
        let post_cycles = (cycles_per_ms * 600.0) as u32; // 250ms + 250ms + buffer
        let total_cycles = pre_loop_cycles + loop_cycles + post_cycles;

        // Collect pulse1 output only to isolate the square channel.
        let samples = collect_pulse_samples(
            "roms/automated_tests/square_timer_div2/square_timer_div2.nes",
            ApuPulseChannel::Pulse1,
            total_cycles,
            false,
        );

        // Drop power-on transients before analysis.
        let samples = trim_warmup(&samples, WARMUP_SAMPLES);

        let cycles_per_sample = cpu_clock_ntsc() / SAMPLE_RATE_HZ;
        let pre_loop_cycles = cpu_clock_ntsc() * 0.35;
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

        // Measure rising-edge periods inside the stable loop window.
        let window = &samples[window_start..window_end];
        let periods = period_series(window);

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

        // WAV correlation: compare an aligned window to the golden reference.
        let wav_samples =
            load_wav_samples_at_rate("roms/automated_tests/square_timer_div2/correct.wav");

        // Align both waveforms on the first rising edge before correlating.
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
        // Correlate to ensure the waveform matches the reference tone pattern.
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
        let wav_samples =
            load_wav_samples_at_rate("roms/automated_tests/test_apu_env/test_apu_env.wav");

        // Capture slightly longer than the WAV to allow warmup and alignment slack.
        let total_cycles = capture_cycles_for_samples(wav_samples.len(), WARMUP_SAMPLES, 10_000);

        // Collect pulse1 output only to isolate the envelope behavior.
        let samples = collect_pulse_samples(
            "roms/automated_tests/test_apu_env/test_apu_env.nes",
            ApuPulseChannel::Pulse1,
            total_cycles,
            false,
        );
        // Drop power-on transients before analysis.
        let samples = trim_warmup(&samples, WARMUP_SAMPLES);

        // Align on the first non-silent RMS window to match the envelope region.
        let window_size = (SAMPLE_RATE_HZ as usize / 50).max(1); // 20ms
        let hop_size = (window_size / 2).max(1);
        let wav_rms_full = rms_windows(&wav_samples, window_size, hop_size);
        let emu_rms_full = rms_windows(samples, window_size, hop_size);
        let wav_max_full = wav_rms_full.iter().copied().fold(0.0f32, f32::max);
        let emu_max_full = emu_rms_full.iter().copied().fold(0.0f32, f32::max);
        let wav_threshold = wav_max_full * 0.05;
        let emu_threshold = emu_max_full * 0.05;
        let wav_start_window = wav_rms_full
            .iter()
            .position(|&value| value > wav_threshold)
            .unwrap_or(0);
        let emu_start_window = emu_rms_full
            .iter()
            .position(|&value| value > emu_threshold)
            .unwrap_or(0);
        let wav_start = wav_start_window * hop_size;
        let emu_start = emu_start_window * hop_size;

        let max_len = wav_samples
            .len()
            .saturating_sub(wav_start)
            .min(samples.len().saturating_sub(emu_start));
        assert!(max_len > 1000, "not enough samples for correlation");

        let wav_slice = &wav_samples[wav_start..wav_start + max_len];
        let emu_slice = &samples[emu_start..emu_start + max_len];

        // Compare envelope shapes using RMS windows.
        // The RMS windowing smooths the waveform into an amplitude envelope.
        let wav_rms = rms_windows(wav_slice, window_size, hop_size);
        let emu_rms = rms_windows(emu_slice, window_size, hop_size);

        assert!(!wav_rms.is_empty(), "wav rms windowing produced no samples");
        assert!(!emu_rms.is_empty(), "emu rms windowing produced no samples");

        // Correlate envelopes to verify the attack/decay contour matches the reference.
        let env_correlation = max_abs_correlation_with_lag(&wav_rms, &emu_rms, 20);
        assert!(
            env_correlation > 0.65,
            "expected strong envelope correlation, got {}",
            env_correlation
        );

        // Locate steady-state sections and compare waveform correlation there.
        let steady_ratio = 0.7;
        let wav_steady = steady_start_index(&wav_rms, steady_ratio, 10);
        let emu_steady = steady_start_index(&emu_rms, steady_ratio, 10);

        let wav_steady = wav_steady.expect("failed to find steady region in wav envelope");
        let emu_steady = emu_steady.expect("failed to find steady region in emu envelope");

        let wav_start = wav_steady * hop_size;
        let emu_start = emu_steady * hop_size;
        let steady_len = (SAMPLE_RATE_HZ as usize / 2)
            .min(wav_slice.len().saturating_sub(wav_start))
            .min(emu_slice.len().saturating_sub(emu_start));
        assert!(
            steady_len > 1000,
            "not enough steady samples for correlation"
        );

        let expected_period = expected_pulse_period_samples(0xC0);
        let period_tolerance = 2.0;
        let min_run = 6;
        let wav_period_start =
            find_period_run_start(wav_slice, expected_period, period_tolerance, min_run)
                .expect("failed to find wav period run for steady pitch");
        let emu_period_start =
            find_period_run_start(emu_slice, expected_period, period_tolerance, min_run)
                .expect("failed to find emu period run for steady pitch");

        let wav_period_slice = &wav_slice[wav_period_start..];
        let emu_period_slice = &emu_slice[emu_period_start..];
        let steady_len = (SAMPLE_RATE_HZ as usize / 2)
            .min(wav_period_slice.len())
            .min(emu_period_slice.len());
        let wav_period_slice = &wav_period_slice[..steady_len];
        let emu_period_slice = &emu_period_slice[..steady_len];

        // Correlate steady waveform segments to confirm pitch/duty stability.
        let correlation = max_abs_correlation_with_lag(wav_period_slice, emu_period_slice, 200);
        assert!(
            correlation > 0.75,
            "expected strong wav correlation magnitude, got {}",
            correlation
        );

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

    // test_apu_sweep
    #[test]
    fn test_apu_sweep_cutoff() {
        // Capture a fixed window long enough to cover the full sweep_cutoff sequence (~4.25s).
        let capture_samples = (SAMPLE_RATE_HZ as usize) * 5;
        let total_cycles = capture_cycles_for_samples(capture_samples, WARMUP_SAMPLES, 20_000);

        // Collect noise-only and pulse-only captures so we can verify ordering.
        let noise_samples = collect_forced_channel_samples(
            "roms/automated_tests/test_apu_sweep/sweep_cutoff.nes",
            total_cycles,
            false,
            false,
            false,
            true,
            false,
        );
        let pulse_samples = collect_pulse_samples(
            "roms/automated_tests/test_apu_sweep/sweep_cutoff.nes",
            ApuPulseChannel::Pulse1,
            total_cycles,
            false,
        );

        // Drop power-on transients so the 200ms windowing aligns to steady audio.
        let noise_samples = trim_warmup(&noise_samples, WARMUP_SAMPLES);
        let pulse_samples = trim_warmup(&pulse_samples, WARMUP_SAMPLES);

        // The ROM uses 200ms delays for each marker/tone, so use fixed 200ms windows.
        let window_samples = (SAMPLE_RATE_HZ * 0.20) as usize; // 200ms per tone/marker
        let hop_samples = window_samples;

        // RMS windows provide a simple energy envelope for noise/pulse presence.
        let noise_rms = rms_windows(noise_samples, window_samples, hop_samples);
        let pulse_rms = rms_windows(pulse_samples, window_samples, hop_samples);
        assert!(
            !noise_rms.is_empty(),
            "noise rms windowing produced no samples"
        );
        assert!(
            !pulse_rms.is_empty(),
            "pulse rms windowing produced no samples"
        );

        let noise_max = noise_rms.iter().copied().fold(0.0f32, f32::max);
        let pulse_max = pulse_rms.iter().copied().fold(0.0f32, f32::max);
        assert!(noise_max > 0.0, "no noise audio captured");
        assert!(pulse_max > 0.0, "no pulse audio captured");

        // Thresholds distinguish silence vs. noise/pulse energy within each 200ms window.
        let noise_threshold = noise_max * 0.05;
        let pulse_threshold = pulse_max * 0.10;

        // Identify the noise marker window(s) in the noise-only capture.
        let noise_indices: Vec<usize> = noise_rms
            .iter()
            .enumerate()
            .filter(|(_, value)| **value > noise_threshold)
            .map(|(index, _)| index)
            .collect();
        assert!(!noise_indices.is_empty(), "expected a noise marker run");

        let noise_start = *noise_indices.first().unwrap();
        let noise_end = *noise_indices.last().unwrap() + 1;
        assert!(
            noise_end - noise_start <= 2,
            "expected noise marker to span at most 2 windows"
        );

        // Align pulse capture so window 0 corresponds to the noise marker window.
        let aligned_start = noise_start * window_samples;
        assert!(
            aligned_start < pulse_samples.len(),
            "noise marker alignment beyond pulse samples"
        );
        let aligned_pulse = &pulse_samples[aligned_start..];
        let aligned_pulse_rms = rms_windows(aligned_pulse, window_samples, hop_samples);
        assert!(
            !aligned_pulse_rms.is_empty(),
            "no aligned pulse RMS windows"
        );

        // The pulse channel should be silent during the noise marker.
        assert!(
            aligned_pulse_rms[0] <= pulse_threshold,
            "expected silence during noise marker window"
        );

        // After the noise marker, the ROM plays nine 200ms pulse tones (8 + 1).
        let tone_windows = 9usize;
        let end_index = 1 + tone_windows;
        assert!(
            aligned_pulse_rms.len() >= end_index,
            "expected at least {} tone windows after noise marker",
            tone_windows
        );
        // Each of the next nine windows should contain a pulse tone.
        for (index, _item) in aligned_pulse_rms
            .iter()
            .enumerate()
            .take(tone_windows + 1)
            .skip(1)
        {
            assert!(
                aligned_pulse_rms[index] > pulse_threshold,
                "expected pulse tone in window {}",
                index
            );
        }

        // The ROM silences the pulse after the final tone.
        if aligned_pulse_rms.len() > end_index {
            assert!(
                aligned_pulse_rms[end_index] <= pulse_threshold,
                "expected silence after final tone"
            );
        }

        // Analyze tone periods per 200ms window and count plateaus.
        let tone_slice = &aligned_pulse[window_samples..window_samples * (1 + tone_windows)];
        let tone_medians = median_periods_by_segments(tone_slice, window_samples, tone_windows);
        assert!(
            tone_medians.len() == tone_windows,
            "expected {} median periods in tone run, got {}",
            tone_windows,
            tone_medians.len()
        );

        let segments = count_period_segments(&tone_medians, 2.0);
        assert!(
            segments == 9,
            "expected exactly 9 tone segments, got {}",
            segments
        );
    }

    // test_apu_sweep
    #[test]
    fn test_apu_sweep_sub() {
        init_tracing_from_env();
        // The test ROM plays two runs of ~6 audible 200ms windows each (~12 total).
        let leading = (SAMPLE_RATE_HZ * 0.30) as usize;
        let trailing = (SAMPLE_RATE_HZ * 0.50) as usize;
        // The CPU cycles calculation correspond to 8813 samples, nearly 200ms at 44.1kHz.
        let segment_samples = 8813usize;
        let total_segments = 14usize;
        let capture_samples = segment_samples * (total_segments + 2);
        let total_cycles = capture_cycles_for_samples(capture_samples, leading, trailing);

        let samples = collect_pulse_samples(
            "roms/automated_tests/test_apu_sweep/sweep_sub.nes",
            ApuPulseChannel::Pulse1,
            total_cycles,
            false,
        );
        let samples = trim_leading_zeros(&samples);
        //let samples = trim_trailing_zeros(&samples);

        // Divide into 200ms RMS windows to locate audible tone regions.
        let rms = rms_windows(samples, segment_samples, segment_samples);
        assert!(!rms.is_empty(), "no RMS windows captured");

        // Should be exactlty 14 audible 200ms windows and then silence.
        assert!(
            rms.len() > total_segments,
            "expected more than {} RMS windows",
            total_segments
        );
        assert_eq!(
            rms[total_segments], 0.0,
            "expected silence after tone windows"
        );

        // All segments should have nearly the same period
        for index in 0..13 {
            assert!(
                (rms[index] - rms[index + 1]).abs() < 0.0002,
                "expected stable RMS in first half"
            );
        }
    }

    // test_apu_timers
    // #[test]
    // fn test_apu_timers_dmc_pitch() {
    //     init_apu_tracing_from_env();
    //     // Golden reference for the expected DMC output (recorded from the test ROM).
    //     let wav_samples =
    //         load_wav_samples_at_rate("roms/automated_tests/test_apu_timers/dmc_pitch.wav");
    //     // Capture a little extra beyond the WAV length to allow for warmup and alignment.
    //     let total_cycles = capture_cycles_for_samples(wav_samples.len(), WARMUP_SAMPLES, 20_000);

    //     // Run the ROM and capture DMC-only output so other channels don't contaminate analysis.
    //     let samples = collect_dmc_samples(
    //         "roms/automated_tests/test_apu_timers/dmc_pitch.nes",
    //         total_cycles,
    //     );
    //     // Remove initial power-on transients for stable audio analysis.
    //     let samples = trim_warmup(&samples, WARMUP_SAMPLES);

    //     // Align on the first steady RMS window to compare timbre against the WAV.
    //     let window_size = (SAMPLE_RATE_HZ as usize / 50).max(1); // 20ms
    //     let hop_size = (window_size / 2).max(1);
    //     let wav_rms = rms_windows(&wav_samples, window_size, hop_size);
    //     let emu_rms = rms_windows(samples, window_size, hop_size);

    //     let wav_start_window =
    //         steady_start_index(&wav_rms, 0.05, 3).expect("failed to find wav steady start");
    //     let emu_start_window =
    //         steady_start_index(&emu_rms, 0.05, 3).expect("failed to find emu steady start");
    //     let wav_start = wav_start_window * hop_size;
    //     let emu_start = emu_start_window * hop_size;

    //     let max_len = wav_samples
    //         .len()
    //         .saturating_sub(wav_start)
    //         .min(samples.len().saturating_sub(emu_start));
    //     assert!(max_len > 10_000, "not enough samples for DMC correlation");

    //     let wav_slice = &wav_samples[wav_start..wav_start + max_len];
    //     let emu_slice = &samples[emu_start..emu_start + max_len];

    //     // Timbre check: compare distributions of per-sample deltas (DMC step patterns).
    //     let wav_hist = delta_histogram(wav_slice, 64, 0.05);
    //     let emu_hist = delta_histogram(emu_slice, 64, 0.05);
    //     let timbre_corr = normalized_correlation(&wav_hist, &emu_hist).abs();
    //     assert!(
    //         timbre_corr > 0.7,
    //         "expected DMC timbre histogram correlation > 0.7, got {}",
    //         timbre_corr
    //     );

    //     // Period analysis:
    //     // - DMC pitch steps should form 16 distinct tone segments.
    //     // - Each step should lower the pitch, which increases the measured period.
    //     let period_window = (SAMPLE_RATE_HZ * 0.10) as usize; // 100ms windows
    //     let period_hop = (period_window / 2).max(1); // 50ms hop
    //     // Expected period range in samples (derived from DMC rate table and sample shape).
    //     let min_lag = 2usize;
    //     let max_lag = 80usize;
    //     let wav_period_windows =
    //         periods_by_autocorr(wav_slice, period_window, period_hop, min_lag, max_lag);
    //     let emu_period_windows =
    //         periods_by_autocorr(emu_slice, period_window, period_hop, min_lag, max_lag);
    //     assert!(
    //         !wav_period_windows.is_empty() && !emu_period_windows.is_empty(),
    //         "no periods detected for DMC pitch test"
    //     );

    //     // Build median periods across small chunks to reduce jitter.
    //     let wav_medians = median_periods_by_chunks(&wav_period_windows, 8);
    //     let emu_medians = median_periods_by_chunks(&emu_period_windows, 8);
    //     assert!(
    //         wav_medians.len() >= 16 && emu_medians.len() >= 16,
    //         "expected enough median windows for 16 tones"
    //     );

    //     let tolerance = 25.0;
    //     let wav_segment_count = count_period_segments(&wav_medians, tolerance);
    //     let emu_segment_count = count_period_segments(&emu_medians, tolerance);
    //     assert!(
    //         wav_segment_count >= 16 && wav_segment_count <= 20,
    //         "expected ~16 wav tone segments, got {}",
    //         wav_segment_count
    //     );
    //     assert!(
    //         emu_segment_count >= 16 && emu_segment_count <= 20,
    //         "expected ~16 DMC tone segments, got {}",
    //         emu_segment_count
    //     );

    //     // Compare period series shapes to ensure pitch steps align with the WAV.
    //     let series_len = wav_medians.len().min(emu_medians.len());
    //     let series_corr =
    //         normalized_correlation(&wav_medians[..series_len], &emu_medians[..series_len]).abs();
    //     assert!(
    //         series_corr > 0.6,
    //         "expected DMC pitch series correlation > 0.6, got {}",
    //         series_corr
    //     );

    //     // The reference WAV should show meaningful pitch variation.
    //     let wav_min = wav_medians.iter().copied().fold(f32::INFINITY, f32::min);
    //     let wav_max = wav_medians
    //         .iter()
    //         .copied()
    //         .fold(f32::NEG_INFINITY, f32::max);
    //     assert!(
    //         wav_max - wav_min > 100.0,
    //         "wav pitch range too small (min={}, max={})",
    //         wav_min,
    //         wav_max
    //     );

    //     // Emulator should also show meaningful pitch variation.
    //     let emu_min = emu_medians.iter().copied().fold(f32::INFINITY, f32::min);
    //     let emu_max = emu_medians
    //         .iter()
    //         .copied()
    //         .fold(f32::NEG_INFINITY, f32::max);
    //     assert!(
    //         emu_max - emu_min > 100.0,
    //         "emu pitch range too small (min={}, max={})",
    //         emu_min,
    //         emu_max
    //     );
    // }

    // #[test]
    // fn test_apu_timers_noise_pitch() {
    //     let samples = collect_forced_channel_samples(
    //         "roms/automated_tests/test_apu_timers/noise_pitch.nes",
    //         10_000_000,
    //         false,
    //         false,
    //         false,
    //         true,
    //         true,
    //     );

    //     // Save raw samples for external analysis if the environment variable is set.
    //     {
    //         let mut file = fs::File::create("samples.raw").expect("Failed to create samples.raw");
    //         for &sample in &samples {
    //             // Clamp and convert f32 [0.0, 1.0] to u8 [0, 255]
    //             let byte = (sample.clamp(0.0, 1.0) * 255.0).round() as u8;
    //             std::io::Write::write_all(&mut file, &[byte]).expect("Failed to write sample");
    //         }
    //     }
    // }

    // TODO test_tri_lin_ctr

    // TODO volume_tests
}
