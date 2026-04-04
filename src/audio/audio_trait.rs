/// Audio output trait for the NES APU.
///
/// Implemented by both `SdlNesAudio` (SDL2 backend) and `NativeAudio` (cpal backend)
/// to provide a common interface for audio playback.
#[allow(dead_code)]
pub trait NesAudio {
    /// Send an audio sample to the audio output.
    ///
    /// Sends a sample to the audio callback for playback.
    /// If the buffer is full, this will block until the audio callback consumes samples.
    ///
    /// # Arguments
    /// * `sample` - Audio sample in range 0.0 to 1.0
    fn queue_sample(&mut self, sample: f32);

    /// Start audio playback.
    fn resume(&self);

    /// Pause audio playback.
    fn pause(&self);

    /// Set audio volume.
    ///
    /// # Arguments
    /// * `volume` - Volume level from 0.0 (mute) to 1.0 (full volume)
    fn set_volume(&self, volume: f32);

    /// Get current audio volume.
    ///
    /// # Returns
    /// Current volume level from 0.0 to 1.0
    fn get_volume(&self) -> f32;

    /// Pre-fills the audio buffer with silence to avoid startup underruns.
    fn prime_startup(&mut self, samples: usize);

    /// Returns and resets audio stats counters.
    ///
    /// Returns (received_samples, dropped_samples, underrun_samples).
    fn take_and_reset_stats(&self) -> (u64, u64, u64);

    /// Returns the actual sample rate of the opened audio device.
    fn actual_sample_rate(&self) -> i32;
}
