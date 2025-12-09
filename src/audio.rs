/// Audio output module for the NES APU
///
/// This module handles SDL2 audio initialization and manages the audio callback
/// that retrieves samples from the APU.
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use std::sync::mpsc::{Receiver, Sender, channel};

/// Audio output handler that receives samples from the NES APU
pub struct NesAudio {
    device: AudioDevice<AudioCallbackImpl>,
    sample_sender: Sender<f32>,
}

impl NesAudio {
    /// Create a new audio output handler
    ///
    /// Initializes SDL2 audio subsystem with the specified sample rate.
    /// Creates a channel for sending audio samples from the emulator to the audio callback.
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
            channels: Some(1), // Mono audio
            samples: None,     // Use SDL2 default buffer size
        };

        // Create channel for sending samples to audio callback
        let (sender, receiver) = channel();

        let device =
            audio_subsystem.open_playback(None, &desired_spec, |_spec| AudioCallbackImpl {
                sample_receiver: receiver,
            })?;

        Ok(Self {
            device,
            sample_sender: sender,
        })
    }

    /// Send an audio sample to the audio output
    ///
    /// Sends a sample to the audio callback for playback.
    /// If the buffer is full, old samples may be dropped.
    ///
    /// # Arguments
    /// * `sample` - Audio sample in range 0.0 to 1.0
    pub fn queue_sample(&mut self, sample: f32) {
        // Send sample to audio callback
        // If the channel is disconnected or full, ignore the error
        let _ = self.sample_sender.send(sample);
    }

    /// Start audio playback
    pub fn resume(&self) {
        self.device.resume();
    }

    /// Pause audio playback
    pub fn pause(&self) {
        self.device.pause();
    }
}

/// SDL2 audio callback implementation
struct AudioCallbackImpl {
    sample_receiver: Receiver<f32>,
}

impl AudioCallback for AudioCallbackImpl {
    type Channel = f32;

    fn callback(&mut self, out: &mut [f32]) {
        for sample in out.iter_mut() {
            // Try to receive a sample from the channel
            // If no sample is available, output silence
            *sample = self.sample_receiver.try_recv().unwrap_or(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // SDL2 can only be initialized once per process, so we use a mutex to ensure tests run serially
    // This is the same mutex used in eventloop tests to prevent conflicts
    static TEST_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_audio_functionality() {
        let _lock = TEST_MUTEX.lock().unwrap();

        // Test audio creation, control, and sample queueing
        // Combine into one test to avoid SDL2 thread issues
        let sdl_context = sdl2::init().expect("Failed to initialize SDL2");

        let audio = NesAudio::new(&sdl_context, 44100);
        assert!(audio.is_ok(), "Audio initialization should succeed");

        let mut audio = audio.unwrap();

        // Test control methods - should not panic
        audio.resume();
        audio.pause();

        // Test queueing samples - should not panic
        audio.queue_sample(0.5);
        audio.queue_sample(0.3);
        audio.queue_sample(0.8);
    }
}
