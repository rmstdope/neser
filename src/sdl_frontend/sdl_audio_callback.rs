use crate::sdl_frontend::sdl_audio::{AudioConsumer, AudioStats};
use crate::sdl_frontend::sdl_audio_resampler::SdlAudioResampler;
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
    pub(crate) resampler: SdlAudioResampler,
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
                    // NES APU mix() outputs 0.0-1.177, where 0.0 represents silence
                    // SDL2 f32 format expects -1.0 to +1.0 where 0.0 is silence
                    // The NES output needs to be scaled to use the full SDL2 range
                    // and shifted so NES silence (0.0) maps to SDL2 silence (0.0)
                    //
                    // Strategy: Map NES 0.0-1.177 to SDL2 0.0-1.0
                    const NES_APU_MAX: f32 = 1.177;
                    let normalized = raw_sample / NES_APU_MAX;
                    let final_sample = normalized * volume;

                    // Safety clamp to prevent any unexpected clipping
                    *sample = final_sample.clamp(-1.0, 1.0);
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
