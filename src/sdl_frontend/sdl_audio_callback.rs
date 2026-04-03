use crate::audio::types::{AudioStats, process_sample};
use crate::audio::{AudioConsumer, AudioResampler};
use ringbuf::traits::Consumer;
use sdl2::audio::AudioCallback;
use std::sync::{
    Arc,
    atomic::{AtomicU32, AtomicUsize, Ordering},
};

/// SDL2 audio callback implementation
pub(crate) struct SdlAudioCallbackImpl {
    pub(crate) sample_consumer: AudioConsumer,
    pub(crate) volume: Arc<AtomicU32>,
    pub(crate) stats: Arc<AudioStats>,
    pub(crate) fill_level: Arc<AtomicUsize>,
    pub(crate) resampler: AudioResampler,
}

impl AudioCallback for SdlAudioCallbackImpl {
    type Channel = f32;

    fn callback(&mut self, out: &mut [f32]) {
        // Load current volume
        let volume = f32::from_bits(self.volume.load(Ordering::Relaxed));
        let fill_level = self.fill_level.load(Ordering::Relaxed);
        self.resampler.update_rate(fill_level);

        for sample in out.iter_mut() {
            let raw_sample = self.resampler.render_next(&mut || {
                let sample = self.sample_consumer.try_pop();
                if sample.is_some() {
                    self.fill_level.fetch_sub(1, Ordering::Relaxed);
                }
                sample
            });

            match raw_sample {
                Some(raw_sample) => {
                    self.stats.received_samples.fetch_add(1, Ordering::Relaxed);
                    *sample = process_sample(raw_sample, volume);
                }
                None => {
                    self.stats.underrun_samples.fetch_add(1, Ordering::Relaxed);
                    // Buffer underrun - output silence
                    *sample = 0.0;
                }
            }
        }
    }
}
