/// Adaptive audio resampler using linear interpolation.
///
/// Adjusts playback rate by ±0.5% based on ring buffer fill level
/// to maintain sync between the emulator's sample production rate
/// and the audio device's consumption rate.
pub struct AudioResampler {
    phase: f32,
    rate: f32,
    target_fill: usize,
    current: f32,
    next: f32,
    has_current: bool,
    has_next: bool,
}

impl AudioResampler {
    pub const MAX_RATE_ADJUST: f32 = 0.005;

    pub fn new(target_fill: usize) -> Self {
        Self {
            phase: 0.0,
            rate: 1.0,
            target_fill,
            current: 0.0,
            next: 0.0,
            has_current: false,
            has_next: false,
        }
    }

    /// Updates the resampling rate based on current buffer fill level.
    ///
    /// When the buffer is emptier than target, rate decreases (slower playback, fills buffer).
    /// When fuller, rate increases (faster playback, drains buffer).
    pub fn update_rate(&mut self, fill_level: usize) {
        let target = self.target_fill.max(1) as f32;
        let delta = fill_level as f32 - target;
        let normalized = (delta / target).clamp(-1.0, 1.0);
        self.rate = 1.0 + normalized * Self::MAX_RATE_ADJUST;
    }

    #[cfg(test)]
    pub fn rate(&self) -> f32 {
        self.rate
    }

    #[cfg(test)]
    pub fn set_rate_for_test(&mut self, rate: f32) {
        self.rate = rate;
    }

    /// Renders the next interpolated output sample.
    ///
    /// Uses linear interpolation between consecutive input samples,
    /// advancing through the input stream at the current resampling rate.
    pub fn render_next<F>(&mut self, pop_sample: &mut F) -> Option<f32>
    where
        F: FnMut() -> Option<f32>,
    {
        if !self.has_current {
            self.current = pop_sample()?;
            self.has_current = true;
        }
        if !self.has_next {
            if let Some(next) = pop_sample() {
                self.next = next;
            } else {
                self.next = self.current;
            }
            self.has_next = true;
        }

        let sample = self.current + (self.next - self.current) * self.phase;
        self.phase += self.rate;

        while self.phase >= 1.0 {
            self.phase -= 1.0;
            self.current = self.next;
            if let Some(next) = pop_sample() {
                self.next = next;
            } else {
                self.next = self.current;
            }
            self.has_next = true;
        }

        Some(sample)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[test]
    fn test_resampler_rate_clamps_to_limits() {
        let mut resampler = AudioResampler::new(100);

        resampler.update_rate(100);
        assert!((resampler.rate() - 1.0).abs() < 0.00001);

        resampler.update_rate(0);
        assert!((resampler.rate() - (1.0 - AudioResampler::MAX_RATE_ADJUST)).abs() < 0.00001);

        resampler.update_rate(200);
        assert!((resampler.rate() - (1.0 + AudioResampler::MAX_RATE_ADJUST)).abs() < 0.00001);
    }

    #[test]
    fn test_resampler_outputs_source_sequence_at_unity_rate() {
        let mut resampler = AudioResampler::new(4);
        resampler.set_rate_for_test(1.0);

        let mut samples = VecDeque::from([0.0, 1.0, 0.0, 1.0]);
        let mut pop_sample = || samples.pop_front();

        let first = resampler
            .render_next(&mut pop_sample)
            .expect("first sample");
        let second = resampler
            .render_next(&mut pop_sample)
            .expect("second sample");
        let third = resampler
            .render_next(&mut pop_sample)
            .expect("third sample");

        assert!((first - 0.0).abs() < 0.00001);
        assert!((second - 1.0).abs() < 0.00001);
        assert!((third - 0.0).abs() < 0.00001);
    }
}
