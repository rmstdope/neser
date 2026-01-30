pub(crate) struct SdlAudioResampler {
    phase: f32,
    rate: f32,
    target_fill: usize,
    current: f32,
    next: f32,
    has_current: bool,
    has_next: bool,
}

impl SdlAudioResampler {
    pub(crate) const MAX_RATE_ADJUST: f32 = 0.005;

    pub(crate) fn new(target_fill: usize) -> Self {
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

    pub(crate) fn update_rate(&mut self, fill_level: usize) {
        let target = self.target_fill.max(1) as f32;
        let delta = fill_level as f32 - target;
        let normalized = (delta / target).clamp(-1.0, 1.0);
        self.rate = 1.0 + normalized * Self::MAX_RATE_ADJUST;
    }

    #[cfg(test)]
    pub(crate) fn rate(&self) -> f32 {
        self.rate
    }

    #[cfg(test)]
    pub(crate) fn set_rate_for_test(&mut self, rate: f32) {
        self.rate = rate;
    }

    pub(crate) fn render_next<F>(&mut self, pop_sample: &mut F) -> Option<f32>
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
