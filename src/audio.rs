/// Audio output module for the NES APU
///
/// This module handles SDL2 audio initialization and manages the audio callback
/// that retrieves samples from the APU.
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use std::sync::{
    Arc,
    atomic::AtomicU64,
    atomic::{AtomicU32, Ordering},
    mpsc::{Receiver, SyncSender, sync_channel},
};

/// Audio output handler that receives samples from the NES APU
pub struct NesAudio {
    device: AudioDevice<AudioCallbackImpl>,
    sample_sender: SyncSender<f32>,
    volume: Arc<AtomicU32>,
    stats: Arc<AudioStats>,
    actual_sample_rate: i32,
}

#[derive(Default)]
struct AudioStats {
    received_samples: AtomicU64,
    dropped_samples: AtomicU64,
    underrun_samples: AtomicU64,
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

        let stats = Arc::new(AudioStats::default());
        let stats_clone = Arc::clone(&stats);

        let device =
            audio_subsystem.open_playback(None, &desired_spec, |_spec| AudioCallbackImpl {
                sample_receiver: receiver,
                volume: volume_clone,
                stats: stats_clone,
            })?;

        let actual_rate = device.spec().freq;
        if actual_rate != sample_rate {
            eprintln!(
                "Audio: requested {} Hz, got {} Hz from SDL device",
                sample_rate, actual_rate
            );
        }

        Ok(Self {
            device,
            sample_sender: sender,
            volume,
            stats,
            actual_sample_rate: actual_rate,
        })
    }

    /// Returns the actual sample rate of the opened SDL audio device.
    pub fn actual_sample_rate(&self) -> i32 {
        self.actual_sample_rate
    }

    /// Send an audio sample to the audio output
    ///
    /// Sends a sample to the audio callback for playback.
    /// If the buffer is full, this will block until the audio callback consumes samples.
    ///
    /// # Arguments
    /// * `sample` - Audio sample in range 0.0 to 1.0
    pub fn queue_sample(&mut self, sample: f32) {
        queue_sample_to_sender(&self.sample_sender, sample, &self.stats);
    }

    /// Returns and resets audio stats counters.
    ///
    /// Useful for debugging pops/clicks: underruns correspond to the audio callback
    /// outputting silence because it had no queued samples available.
    pub fn take_and_reset_stats(&self) -> (u64, u64, u64) {
        let received = self.stats.received_samples.swap(0, Ordering::Relaxed);
        let dropped = self.stats.dropped_samples.swap(0, Ordering::Relaxed);
        let underrun = self.stats.underrun_samples.swap(0, Ordering::Relaxed);
        (received, dropped, underrun)
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

fn queue_sample_to_sender(sender: &SyncSender<f32>, sample: f32, stats: &AudioStats) {
    // Blocking send provides backpressure instead of dropping samples.
    // Dropped samples create discontinuities that can manifest as audible clicks.
    if sender.send(sample).is_err() {
        // Receiver was dropped; not expected during normal execution.
        stats.dropped_samples.fetch_add(1, Ordering::Relaxed);
    }
}

/// SDL2 audio callback implementation
struct AudioCallbackImpl {
    sample_receiver: Receiver<f32>,
    volume: Arc<AtomicU32>,
    stats: Arc<AudioStats>,
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
                Err(_) => {
                    self.stats.underrun_samples.fetch_add(1, Ordering::Relaxed);
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
    use std::env;
    use std::sync::{Arc, Barrier};
    use std::time::Duration;

    #[test]
    #[serial]
    fn test_audio_functionality() {
        // CI often runs without an audio device; force SDL to use its dummy backend.
        // Restore the previous env value after the test to avoid cross-test pollution.
        struct EnvRestore {
            key: &'static str,
            prev: Option<String>,
        }

        impl Drop for EnvRestore {
            fn drop(&mut self) {
                match &self.prev {
                    Some(value) => unsafe {
                        // SAFETY: This test is marked `#[serial]`, and this env var is only used
                        // to configure SDL audio backend selection for this test.
                        env::set_var(self.key, value)
                    },
                    None => unsafe {
                        // SAFETY: See above.
                        env::remove_var(self.key)
                    },
                }
            }
        }

        let restore = EnvRestore {
            key: "SDL_AUDIODRIVER",
            prev: env::var("SDL_AUDIODRIVER").ok(),
        };
        unsafe {
            // SAFETY: This test is marked `#[serial]`, and SDL reads this env var during init.
            env::set_var("SDL_AUDIODRIVER", "dummy");
        }

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

        drop(restore);
    }

    #[test]
    fn test_queue_sample_does_not_drop_when_buffer_full() {
        // Desired behavior: when the bounded audio buffer is full, do NOT drop samples.
        // Dropping samples introduces discontinuities that can manifest as clicks.
        //
        // Current implementation drops, so this test is expected to FAIL until fixed.

        let stats = Arc::new(AudioStats::default());
        let (sender, receiver) = sync_channel::<f32>(1);

        // Fill the buffer.
        queue_sample_to_sender(&sender, 0.1, &stats);

        let barrier = Arc::new(Barrier::new(2));
        let barrier_consumer = Arc::clone(&barrier);
        let (result_tx, result_rx) = std::sync::mpsc::channel::<(f32, f32)>();
        let (producer_ready_tx, producer_ready_rx) = std::sync::mpsc::channel::<()>();

        let consumer = std::thread::spawn(move || {
            // Ensure producer attempts to enqueue while the queue is full.
            barrier_consumer.wait();

            let first = receiver
                .recv_timeout(Duration::from_millis(200))
                .expect("expected first sample");
            let second = receiver
                .recv_timeout(Duration::from_millis(200))
                .expect("expected second sample (must not be dropped)");

            result_tx
                .send((first, second))
                .expect("failed to send samples to main thread");
        });

        let stats_producer = Arc::clone(&stats);
        let producer = std::thread::spawn(move || {
            // Signal that we're about to attempt enqueue while the queue is full.
            producer_ready_tx
                .send(())
                .expect("failed to signal producer readiness");
            queue_sample_to_sender(&sender, 0.2, &stats_producer);
        });

        // Ensure the producer has started the enqueue attempt before letting the consumer drain.
        producer_ready_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("producer did not become ready");

        barrier.wait();

        producer.join().expect("producer thread panicked");
        consumer.join().expect("consumer thread panicked");

        let (first, second) = result_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("expected samples from consumer");
        assert_eq!(first, 0.1);
        assert_eq!(second, 0.2);

        let dropped = stats.dropped_samples.load(Ordering::Relaxed);
        assert_eq!(dropped, 0, "no samples should be dropped");
    }
}
