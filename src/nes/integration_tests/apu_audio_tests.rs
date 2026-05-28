/// All of the test ROMs below are verified manually by listening to them and, where possible,
/// comparing to known-good audio captures from real hardware.
/// The audio analysis functions below are more of a fun exploration than strict test verifications,
/// but they could help catch gross APU emulation errors in the future.
#[cfg(test)]
mod tests {
    use crate::nes::cartridge::Cartridge;
    use crate::nes::console::{Config, Nes, TimingMode};
    use crate::nes::integration_tests::rom_test_runner::tests::init_tracing_from_env;
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
        Cartridge::load_from_file(rom_data, rom_path, None).expect("ROM should parse")
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

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
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

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
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
            "roms/nes/automated_tests/apu_phase_reset/apu_phase_reset.nes",
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

    /// Compute AC-coupled RMS values over sliding windows.
    fn ac_rms_windows(samples: &[f32], window_size: usize, hop_size: usize) -> Vec<f32> {
        if window_size == 0 || hop_size == 0 || samples.len() < window_size {
            return Vec::new();
        }

        let mut rms = Vec::new();
        let mut start = 0usize;
        while start + window_size <= samples.len() {
            rms.push(ac_coupled_rms(&samples[start..start + window_size]));
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

    /// Locate the noise marker in an RMS envelope.
    ///
    /// Returns `(start, end)` window indices of the contiguous noise marker region.
    /// Panics if no marker is found, if the marker is not contiguous, or if it
    /// spans more than `max_span` windows.
    fn find_noise_marker_range(noise_rms: &[f32], max_span: usize) -> (usize, usize) {
        let noise_max = noise_rms.iter().copied().fold(0.0f32, f32::max);
        assert!(noise_max > 0.0, "no noise audio captured");
        let noise_threshold = noise_max * 0.05;

        let noise_indices: Vec<usize> = noise_rms
            .iter()
            .enumerate()
            .filter(|(_, value)| **value > noise_threshold)
            .map(|(index, _)| index)
            .collect();
        assert!(!noise_indices.is_empty(), "expected a noise marker");
        assert!(
            noise_indices
                .windows(2)
                .all(|window| window[1] == window[0] + 1),
            "expected a contiguous noise marker, got windows {:?}",
            noise_indices
        );

        let noise_start = *noise_indices.first().unwrap();
        let noise_end = *noise_indices.last().unwrap() + 1;
        assert!(
            noise_end - noise_start <= max_span,
            "noise marker spans too many windows: {} (max {})",
            noise_end - noise_start,
            max_span
        );

        (noise_start, noise_end)
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
    setup_rom_test!(
        test_apu_mixer_dmc,
        "roms/nes/automated_tests/apu_mixer/dmc.nes"
    );
    setup_rom_test!(
        test_apu_mixer_noise,
        "roms/nes/automated_tests/apu_mixer/noise.nes"
    );
    setup_rom_test!(
        test_apu_mixer_square,
        "roms/nes/automated_tests/apu_mixer/square.nes"
    );
    setup_rom_test!(
        test_apu_mixer_triangle,
        "roms/nes/automated_tests/apu_mixer/triangle.nes"
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
        "roms/nes/automated_tests/dmc_tests/buffer_retained.nes",
        0xE149,
        check_one_dmc_byte_processed
    );
    setup_rom_address_test!(
        test_dmc_tests_latency,
        "roms/nes/automated_tests/dmc_tests/latency.nes",
        0xE162,
        check_four_by_two_dmc_bytes_processed
    );
    setup_rom_address_test!(
        test_dmc_tests_status_irq,
        "roms/nes/automated_tests/dmc_tests/status_irq.nes",
        0xE154,
        check_one_irq_fired
    );
    setup_rom_address_test!(
        test_dmc_tests_status,
        "roms/nes/automated_tests/dmc_tests/status.nes",
        0xE14E,
        check_zero_irq_fired
    );

    // fadeout_and_triangle_tests
    #[test]
    fn test_fadeout_and_triangle() {
        init_tracing_from_env();
        let rom_path =
            "roms/nes/automated_tests/fadeout_and_triangle_tests/fadeout_and_triangle_test.nes";

        // Run for ~4 seconds to capture at least one full fade cycle (~3.25s).
        let total_cycles = NTSC_CPU_CYCLES_PER_FRAME * 240;

        // --- Collect mixed output (all channels) for fadeout envelope analysis ---
        let rom_data = fs::read(rom_path).expect("ROM should load");
        let cartridge = load_test_cartridge(&rom_data, rom_path);
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cartridge);
        nes.reset(false);
        {
            let mut apu = nes.apu().borrow_mut();
            apu.set_sample_rate(SAMPLE_RATE_HZ);
        }
        let mut mixed_samples = Vec::new();
        let mut cycles_run = 0u32;
        while cycles_run < total_cycles {
            let consumed = nes.run_cpu_tick() as u32;
            cycles_run = cycles_run.saturating_add(consumed.max(1));
            while nes.sample_ready() {
                if let Some(sample) = nes.get_sample() {
                    mixed_samples.push(sample);
                }
            }
        }
        let mixed_samples = trim_warmup(&mixed_samples, WARMUP_SAMPLES);
        assert!(
            mixed_samples.len() > 10_000,
            "expected sufficient mixed samples, got {}",
            mixed_samples.len()
        );

        // --- Collect triangle-only output for frequency analysis ---
        let tri_samples = collect_forced_channel_samples(
            rom_path,
            total_cycles,
            false, // pulse1
            false, // pulse2
            true,  // triangle
            false, // noise
            false, // dmc
        );
        let tri_samples = trim_warmup(&tri_samples, WARMUP_SAMPLES);
        assert!(
            tri_samples.len() > 10_000,
            "expected sufficient triangle samples, got {}",
            tri_samples.len()
        );

        // --- Assertion 1: Fadeout envelope in mixed output ---
        // The ROM plays multiple channels initially, then they fade out leaving
        // only the triangle. The RMS envelope should decrease significantly.
        let window_size = (SAMPLE_RATE_HZ as usize / 50).max(1); // 20ms windows
        let hop_size = (window_size / 2).max(1);
        let mixed_rms = rms_windows(mixed_samples, window_size, hop_size);
        let max_rms = mixed_rms.iter().copied().fold(0.0f32, f32::max);
        let min_rms = mixed_rms.iter().copied().fold(f32::INFINITY, f32::min);

        // The peak RMS should be substantially higher than the trough (fadeout occurred).
        let fadeout_ratio = max_rms / min_rms.max(f32::EPSILON);
        assert!(
            fadeout_ratio > 1.5,
            "expected significant fadeout (peak/trough ratio > 1.5), got {:.3} \
             (max_rms={:.4}, min_rms={:.4})",
            fadeout_ratio,
            max_rms,
            min_rms
        );

        // Verify the envelope generally decreases: the first quarter of RMS windows
        // should have a higher average than the middle portion (post-fade steady state).
        let q1_end = mixed_rms.len() / 4;
        let q2_start = mixed_rms.len() / 4;
        let q2_end = mixed_rms.len() / 2;
        let q1_avg: f32 = mixed_rms[..q1_end].iter().sum::<f32>() / q1_end.max(1) as f32;
        let q2_avg: f32 =
            mixed_rms[q2_start..q2_end].iter().sum::<f32>() / (q2_end - q2_start).max(1) as f32;
        assert!(
            q1_avg > q2_avg,
            "expected first quarter RMS ({:.4}) > second quarter RMS ({:.4}) — fadeout \
             envelope should decrease over time",
            q1_avg,
            q2_avg
        );

        // --- Assertion 2: Triangle output is active (not silence) ---
        // The triangle channel should produce non-silent output throughout.
        let tri_rms = rms_windows(tri_samples, window_size, hop_size);
        let tri_avg_rms: f32 = tri_rms.iter().sum::<f32>() / tri_rms.len().max(1) as f32;
        assert!(
            tri_avg_rms > 0.05,
            "expected non-silent triangle output (avg RMS > 0.05), got {:.4} — \
             triangle channel may be broken",
            tri_avg_rms
        );

        // The steady-state mixed RMS (post-fade) should also be non-zero,
        // confirming the triangle continues playing after other channels fade.
        let steady_region = &mixed_rms[mixed_rms.len() / 2..mixed_rms.len() * 3 / 4];
        let steady_avg: f32 = steady_region.iter().sum::<f32>() / steady_region.len().max(1) as f32;
        assert!(
            steady_avg > 0.05,
            "expected non-silent steady region (avg RMS > 0.05), got {:.4} — triangle \
             may have been silenced prematurely",
            steady_avg
        );

        // --- Assertion 3: Triangle frequency stability ---
        // The triangle channel should produce a stable ~196 Hz tone (period ~225 samples
        // at 44100 Hz sample rate).
        let periods = period_series(tri_samples);
        assert!(
            periods.len() > 100,
            "expected >100 triangle periods for reliable measurement, got {}",
            periods.len()
        );

        // Use median-filtered periods to ignore outliers from warmup edges.
        let mut sorted_periods = periods.clone();
        sorted_periods.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median_period = sorted_periods[sorted_periods.len() / 2];
        let filtered: Vec<f32> = periods
            .iter()
            .copied()
            .filter(|&p| (p - median_period).abs() / median_period < 0.1)
            .collect();
        assert!(
            !filtered.is_empty(),
            "expected at least one triangle period within ±10% of median ({:.1}), got 0 out of {} periods",
            median_period,
            periods.len()
        );
        let avg_period: f32 = filtered.iter().sum::<f32>() / filtered.len() as f32;
        let freq = SAMPLE_RATE_HZ / avg_period;

        // Triangle frequency should be within ±10% of expected ~196 Hz.
        assert!(
            (176.0..=216.0).contains(&freq),
            "expected triangle frequency ~196 Hz (±10%), got {:.1} Hz (period={:.1} samples)",
            freq,
            avg_period
        );

        // Most periods should be within ±10% of median (frequency stability).
        let stable_ratio = filtered.len() as f32 / periods.len() as f32;
        assert!(
            stable_ratio > 0.90,
            "expected >90% of periods within ±10% of median ({:.1}), got {:.1}% ({}/{})",
            median_period,
            stable_ratio * 100.0,
            filtered.len(),
            periods.len()
        );
    }

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
            "roms/nes/automated_tests/square_timer_div2/square_timer_div2.nes",
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
            load_wav_samples_at_rate("roms/nes/automated_tests/square_timer_div2/correct.wav");

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
            load_wav_samples_at_rate("roms/nes/automated_tests/test_apu_env/test_apu_env.wav");

        // Capture slightly longer than the WAV to allow warmup and alignment slack.
        let total_cycles = capture_cycles_for_samples(wav_samples.len(), WARMUP_SAMPLES, 10_000);

        // Collect pulse1 output only to isolate the envelope behavior.
        let samples = collect_pulse_samples(
            "roms/nes/automated_tests/test_apu_env/test_apu_env.nes",
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
            "roms/nes/automated_tests/test_apu_sweep/sweep_cutoff.nes",
            total_cycles,
            false,
            false,
            false,
            true,
            false,
        );
        let pulse_samples = collect_pulse_samples(
            "roms/nes/automated_tests/test_apu_sweep/sweep_cutoff.nes",
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

        let pulse_max = pulse_rms.iter().copied().fold(0.0f32, f32::max);
        assert!(pulse_max > 0.0, "no pulse audio captured");
        let pulse_threshold = pulse_max * 0.10;

        // Locate the noise marker using the shared helper.
        let (noise_start, _noise_end) = find_noise_marker_range(&noise_rms, 2);

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
            "roms/nes/automated_tests/test_apu_sweep/sweep_sub.nes",
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
    //         load_wav_samples_at_rate("roms/nes/automated_tests/test_apu_timers/dmc_pitch.wav");
    //     // Capture a little extra beyond the WAV length to allow for warmup and alignment.
    //     let total_cycles = capture_cycles_for_samples(wav_samples.len(), WARMUP_SAMPLES, 20_000);

    //     // Run the ROM and capture DMC-only output so other channels don't contaminate analysis.
    //     let samples = collect_dmc_samples(
    //         "roms/nes/automated_tests/test_apu_timers/dmc_pitch.nes",
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
    //         "roms/nes/automated_tests/test_apu_timers/noise_pitch.nes",
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

    // test_tri_lin_ctr
    #[test]
    fn test_tri_lin_ctr() {
        // The ROM tests the triangle linear counter in three phases:
        //   1. No waveform – linear-counter manipulations halt the triangle sequencer
        //   2. Noise marker – a brief noise burst separating the phases
        //   3. Continuous triangle – linear counter reloaded and sustained
        //
        // Strategy: two captures (triangle-only + noise-only) to locate the noise
        // marker via RMS, then verify no AC waveform before it and sustained activity after.

        let rom_path = "roms/nes/automated_tests/test_tri_lin_ctr/lin_ctr.nes";

        // ~8 seconds of capture to cover the full ROM execution with margin.
        let capture_samples = (SAMPLE_RATE_HZ as usize) * 8;
        let total_cycles = capture_cycles_for_samples(capture_samples, WARMUP_SAMPLES, 20_000);

        // Capture 1: triangle-only
        let tri_samples = collect_forced_channel_samples(
            rom_path,
            total_cycles,
            false, // pulse1
            false, // pulse2
            true,  // triangle
            false, // noise
            false, // dmc
        );

        // Capture 2: noise-only
        let noise_samples = collect_forced_channel_samples(
            rom_path,
            total_cycles,
            false, // pulse1
            false, // pulse2
            false, // triangle
            true,  // noise
            false, // dmc
        );

        let tri_samples = trim_warmup(&tri_samples, WARMUP_SAMPLES);
        let noise_samples = trim_warmup(&noise_samples, WARMUP_SAMPLES);

        // 200ms non-overlapping RMS windows (matches ROM delay granularity).
        let window_samples = (SAMPLE_RATE_HZ * 0.20) as usize;
        let hop_samples = window_samples;

        let tri_rms = ac_rms_windows(tri_samples, window_samples, hop_samples);
        let noise_rms = rms_windows(noise_samples, window_samples, hop_samples);
        assert!(
            !tri_rms.is_empty(),
            "triangle RMS windowing produced no samples"
        );
        assert!(
            !noise_rms.is_empty(),
            "noise RMS windowing produced no samples"
        );

        // Locate the noise marker using the shared helper.
        let (noise_start, noise_end) = find_noise_marker_range(&noise_rms, 3);

        // Triangle threshold: 5% of peak triangle RMS.
        let tri_max = tri_rms.iter().copied().fold(0.0f32, f32::max);
        assert!(tri_max > 0.0, "no triangle audio captured");
        let silence_threshold = tri_max * 0.05;
        let click_threshold = tri_max * 0.50;

        // Phase 1 – no sustained triangle waveform. The ROM documentation allows
        // a few slight pops/clicks before the noise marker, so permit a short
        // transient but reject full-strength triangle output.
        for (index, &rms) in tri_rms.iter().enumerate().take(noise_start) {
            assert!(
                rms <= click_threshold,
                "expected at most triangle click/transient before noise marker at window {} (ac_rms={}, threshold={})",
                index,
                rms,
                click_threshold
            );
        }

        // Phase 2 – During the noise marker, triangle should also have no AC waveform.
        let noise_window_end = noise_end.min(tri_rms.len());
        for (offset, &rms) in tri_rms[noise_start..noise_window_end].iter().enumerate() {
            let window = noise_start + offset;
            assert!(
                rms <= silence_threshold,
                "expected no triangle waveform during noise marker window {} (ac_rms={})",
                window,
                rms
            );
        }

        // Phase 3 – Continuous triangle: windows after the noise marker should be active.
        // Allow a brief transition window right after the noise marker.
        // The ROM ends by disabling all channels (lda #0; sta $4015; jmp forever),
        // so trailing silence is expected. We verify no silent gaps within the active region.
        let continuous_start = (noise_end + 1).min(tri_rms.len());

        // Find the contiguous active region: from continuous_start to the first silent window.
        let active_count = tri_rms[continuous_start..]
            .iter()
            .take_while(|&&rms| rms > silence_threshold)
            .count();

        assert!(
            active_count >= 5,
            "expected at least 5 continuous active triangle windows after noise marker, got {}",
            active_count
        );

        // Verify no silent gaps appear *after* the active region restarts (i.e., the
        // active region is truly contiguous — once it stops, it stays stopped).
        let after_active = continuous_start + active_count;
        for (offset, &rms) in tri_rms[after_active..].iter().enumerate() {
            let window = after_active + offset;
            assert!(
                rms <= silence_threshold,
                "unexpected triangle activity at window {} after continuous region ended at {} \
                 (rms={}); indicates a silent gap within what should be continuous playback",
                window,
                after_active,
                rms
            );
        }
    }

    // ── volume_tests helpers ────────────────────────────────────────────

    /// Compute AC-coupled RMS (DC offset removed) for a slice of audio samples.
    ///
    /// Subtracts the mean before computing RMS so that only the alternating
    /// signal energy is measured.  This matches real NES hardware behavior where
    /// a high-pass filter removes the DC component from the unsigned DACs.
    fn ac_coupled_rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let mean = samples.iter().sum::<f32>() / samples.len() as f32;
        let variance =
            samples.iter().map(|&s| (s - mean).powi(2)).sum::<f32>() / samples.len() as f32;
        variance.sqrt()
    }

    /// Run a ROM for a total number of CPU cycles with all APU channels enabled,
    /// pressing the A button on controller 1 at `press_frame` and releasing it
    /// one frame later.
    ///
    /// Returns the collected audio samples (mixed, all channels).
    fn collect_samples_with_a_press(
        rom_path: &str,
        total_cycles: u32,
        press_frame: u64,
    ) -> Vec<f32> {
        let rom_data = fs::read(rom_path).expect("ROM should load");
        let cartridge = load_test_cartridge(&rom_data, rom_path);

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        {
            let mut apu = nes.apu().borrow_mut();
            apu.set_sample_rate(SAMPLE_RATE_HZ);
        }

        let mut samples = Vec::new();
        let mut cycles_run = 0u32;
        let mut a_pressed = false;
        let mut a_released = false;

        while cycles_run < total_cycles {
            let frame = nes.ppu().borrow().frame_count();

            if !a_pressed && frame >= press_frame {
                nes.set_button(1, crate::nes::input::Button::A, true);
                a_pressed = true;
            } else if a_pressed && !a_released && frame > press_frame {
                nes.set_button(1, crate::nes::input::Button::A, false);
                a_released = true;
            }

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

    /// Frame offsets (relative to A-press) for each tone within a single DMC pass.
    ///
    /// Derived from the ROM's `sound.s` assembly: each tone occupies 72 frames
    /// (8-frame setup silence + 64-frame volume sweep 15→0 at 4 frames/step).
    /// The peak-volume audio starts at offset + 8 frames.
    const TONE_PASS_OFFSETS: [u64; 12] = [
        0,   // Tone 1:  Sq1 1/8 duty
        72,  // Tone 2:  Sq1 1/4 duty
        144, // Tone 3:  Sq1 1/2 duty
        216, // Tone 4:  Sq1 3/4 duty
        288, // Tone 5:  Sq1+Sq2 1/8 duty
        360, // Tone 6:  Sq1+Sq2 1/4 duty
        432, // Tone 7:  Sq1+Sq2 1/2 duty
        504, // Tone 8:  Sq1+Sq2 3/4 duty
        576, // Tone 9:  Triangle
        648, // Tone 10: Noise long LFSR
        720, // Tone 11: Noise short LFSR
        792, // Tone 12: DMC amplitude 30
    ];

    /// Frames of silence before volume starts within each tone.
    const TONE_SETUP_FRAMES: u64 = 8;

    /// Frames of peak-volume audio to measure RMS over.
    const PEAK_MEASURE_FRAMES: u64 = 8;

    /// Total frames per DMC pass (4 initial + 12 tones).
    /// Tones 1-8: 576 frames, tone 9: 72, tones 10-11: 144, tone 12: ~88 = ~880.
    const FRAMES_PER_PASS: u64 = 880;

    /// Initial silence frames at the start of volume_test (before pass 0).
    const INITIAL_SILENCE_FRAMES: u64 = 4;

    /// Approximate samples per NTSC frame at 44100 Hz.
    const SAMPLES_PER_FRAME: f32 = SAMPLE_RATE_HZ / 60.0988;

    /// Extract RMS values for all 12 tones in a given DMC pass from raw samples.
    ///
    /// `press_frame` is the PPU frame at which A was pressed.
    /// `pass_index` is 0, 1, or 2 (DMC baseline 0, 48, 96).
    /// `samples` is the full audio buffer.
    fn extract_tone_rms_values(samples: &[f32], press_frame: u64, pass_index: u64) -> [f32; 12] {
        let pass_start_frame = press_frame + INITIAL_SILENCE_FRAMES + pass_index * FRAMES_PER_PASS;

        let mut rms_values = [0.0f32; 12];
        for (i, &tone_offset) in TONE_PASS_OFFSETS.iter().enumerate() {
            let peak_start_frame = pass_start_frame + tone_offset + TONE_SETUP_FRAMES;
            let peak_start_sample = (peak_start_frame as f32 * SAMPLES_PER_FRAME) as usize;
            let peak_end_sample =
                ((peak_start_frame + PEAK_MEASURE_FRAMES) as f32 * SAMPLES_PER_FRAME) as usize;

            if peak_end_sample <= samples.len() {
                rms_values[i] = ac_coupled_rms(&samples[peak_start_sample..peak_end_sample]);
            }
        }
        rms_values
    }

    /// Assert relative volume relationships for one DMC pass.
    fn assert_volume_relationships(rms: &[f32; 12], pass_label: &str) {
        // Tones 1-4: duty cycle ordering.
        // Tone 3 (1/2 duty) should be the loudest single-channel tone.
        // Tone 4 (3/4 duty) should be approximately equal to tone 3 (by waveform symmetry).
        // Tone 1 (1/8 duty) should be the quietest.
        assert!(
            rms[2] > rms[0],
            "{}: tone 3 (1/2 duty, rms={:.6}) should be louder than tone 1 (1/8 duty, rms={:.6})",
            pass_label,
            rms[2],
            rms[0]
        );
        assert!(
            rms[1] > rms[0],
            "{}: tone 2 (1/4 duty, rms={:.6}) should be louder than tone 1 (1/8 duty, rms={:.6})",
            pass_label,
            rms[1],
            rms[0]
        );

        // Tones 5-8 (combined sq1+sq2) should each be louder than tones 1-4 (single sq1).
        for i in 0..4 {
            assert!(
                rms[4 + i] > rms[i],
                "{}: combined tone {} (rms={:.6}) should be louder than single tone {} (rms={:.6})",
                pass_label,
                5 + i,
                rms[4 + i],
                1 + i,
                rms[i]
            );
        }

        // Tone 9 (triangle): audible
        assert!(
            rms[8] > 1e-5,
            "{}: tone 9 (triangle, rms={:.6}) should be audible",
            pass_label,
            rms[8]
        );

        // Tone 10 (noise long LFSR): audible
        assert!(
            rms[9] > 1e-5,
            "{}: tone 10 (noise long, rms={:.6}) should be audible",
            pass_label,
            rms[9]
        );

        // Tone 11 (noise short LFSR): audible
        assert!(
            rms[10] > 1e-5,
            "{}: tone 11 (noise short, rms={:.6}) should be audible",
            pass_label,
            rms[10]
        );

        // Tone 12 (DMC amplitude 30): audible
        assert!(
            rms[11] > 1e-5,
            "{}: tone 12 (DMC, rms={:.6}) should be audible",
            pass_label,
            rms[11]
        );
    }

    #[test]
    fn test_volume_tests() {
        init_tracing_from_env();

        let rom_path = "roms/nes/automated_tests/volume_tests/volumes.nes";

        // Press A after 5 frames to let the ROM initialize.
        let press_frame: u64 = 5;

        // Total frames needed: 5 (warmup) + 4 (initial silence) + 3 * 880 (passes) ≈ 2649.
        // Add margin for ramps between passes and measurement slack.
        let total_frames: u64 = press_frame + INITIAL_SILENCE_FRAMES + 3 * FRAMES_PER_PASS + 50;
        let total_cycles = (total_frames as f32 * NTSC_CPU_CYCLES_PER_FRAME as f32) as u32;

        let samples = collect_samples_with_a_press(rom_path, total_cycles, press_frame);

        for pass in 0..3u64 {
            let rms = extract_tone_rms_values(&samples, press_frame, pass);
            let pass_label = format!("pass {} (DMC={})", pass, pass * 48);
            assert_volume_relationships(&rms, &pass_label);
        }
    }
}
