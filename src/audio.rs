/// Audio output module for the NES APU
///
/// This module handles SDL2 audio initialization and manages the audio callback
/// that retrieves samples from the APU.
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
    mpsc::{Receiver, SyncSender, sync_channel},
};

/// Audio output handler that receives samples from the NES APU
pub struct NesAudio {
    device: AudioDevice<AudioCallbackImpl>,
    sample_sender: SyncSender<f32>,
    volume: Arc<AtomicU32>,
}

impl NesAudio {
    /// Audio buffer size in samples
    /// At 44.1kHz, this provides ~0.5 seconds of buffering (22050 samples / 44100 Hz)
    const BUFFER_SIZE: usize = 22050;

    /// Create a new audio output handler
    ///
    /// Initializes SDL2 audio subsystem with the specified sample rate.
    /// Creates a bounded channel for sending audio samples from the emulator to the audio callback.
    ///
    /// # Arguments
    /// * `sdl_context` - The SDL2 context for audio initialization
    /// * `sample_rate` - Target sample rate in Hz (e.g., 44100, 48000)
    ///
    /// # Errors
    /// Returns an error if SDL2 audio initialization fails
    pub fn new(sdl_context: &sdl2::Sdl, sample_rate: i32) -> Result<Self, String> {
        let audio_subsystem = sdl_context.audio()?;

        let desired_spec = AudioSpecDesired {
            freq: Some(sample_rate),
            channels: Some(1),   // Mono audio
            samples: Some(1024), // Larger buffer for debug mode (less CPU pressure)
        };

        // Create bounded channel for sending samples to audio callback
        // This prevents unbounded memory growth if audio callback falls behind
        let (sender, receiver) = sync_channel(Self::BUFFER_SIZE);

        // Create shared volume control (default 25% to avoid distortion)
        let volume = Arc::new(AtomicU32::new(f32::to_bits(0.25)));
        let volume_clone = Arc::clone(&volume);

        let device =
            audio_subsystem.open_playback(None, &desired_spec, |_spec| AudioCallbackImpl {
                sample_receiver: receiver,
                volume: volume_clone,
                prev_sample: 0.0,
            })?;

        Ok(Self {
            device,
            sample_sender: sender,
            volume,
        })
    }

    /// Send an audio sample to the audio output
    ///
    /// Sends a sample to the audio callback for playback.
    /// If the buffer is full, the sample will be dropped to prevent blocking.
    ///
    /// # Arguments
    /// * `sample` - Audio sample in range 0.0 to 1.0
    pub fn queue_sample(&mut self, sample: f32) {
        // Send sample to audio callback using try_send to avoid blocking
        // If the buffer is full, drop the sample to prevent emulation slowdown
        let _ = self.sample_sender.try_send(sample);
    }

    /// Start audio playback
    pub fn resume(&self) {
        self.device.resume();
    }

    /// Pause audio playback
    pub fn pause(&self) {
        self.device.pause();
    }

    /// Set audio volume
    ///
    /// # Arguments
    /// * `volume` - Volume level from 0.0 (mute) to 1.0 (full volume)
    pub fn set_volume(&self, volume: f32) {
        let clamped = volume.clamp(0.0, 1.0);
        self.volume.store(f32::to_bits(clamped), Ordering::Relaxed);
    }

    /// Get current audio volume
    ///
    /// # Returns
    /// Current volume level from 0.0 to 1.0
    pub fn get_volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }
}

/// SDL2 audio callback implementation
struct AudioCallbackImpl {
    sample_receiver: Receiver<f32>,
    volume: Arc<AtomicU32>,
    // Simple low-pass filter state (previous sample for smoothing)
    prev_sample: f32,
}

impl AudioCallback for AudioCallbackImpl {
    type Channel = f32;

    fn callback(&mut self, out: &mut [f32]) {
        // Load current volume
        let volume = f32::from_bits(self.volume.load(Ordering::Relaxed));

        for sample in out.iter_mut() {
            // Try to receive a sample from the channel
            // If no sample is available, output silence (0.0 for signed audio)
            match self.sample_receiver.try_recv() {
                Ok(raw_sample) => {
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
                Err(_) => {
                    // Buffer underrun - output silence
                    *sample = 0.0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_audio_functionality() {
        // Test audio creation, control, and sample queueing
        // Combine into one test to avoid SDL2 thread issues
        let sdl_context = sdl2::init().expect("Failed to initialize SDL2");

        let audio = NesAudio::new(&sdl_context, 44100);
        assert!(audio.is_ok(), "Audio initialization should succeed");

        let mut audio = audio.unwrap();

        // Test volume control
        assert_eq!(audio.get_volume(), 0.25, "Default volume should be 0.25");
        audio.set_volume(0.5);
        assert_eq!(audio.get_volume(), 0.5, "Volume should be 0.5");
        audio.set_volume(2.0); // Test clamping
        assert_eq!(audio.get_volume(), 1.0, "Volume should clamp to 1.0");
        audio.set_volume(-0.5); // Test clamping
        assert_eq!(audio.get_volume(), 0.0, "Volume should clamp to 0.0");

        // Test control methods - should not panic
        audio.resume();
        audio.pause();

        // Test queueing samples - should not panic
        audio.queue_sample(0.5);
        audio.queue_sample(0.3);
        audio.queue_sample(0.8);
    }
}
