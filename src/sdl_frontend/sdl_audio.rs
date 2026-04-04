use crate::audio::types::queue_sample_to_producer;
use crate::audio::{AudioProducer, AudioResampler, AudioStats, NesAudio};
use crate::debugging::log_info;
/// Audio output module for the NES APU
///
/// This module handles SDL2 audio initialization and manages the audio callback
/// that retrieves samples from the APU.
use crate::sdl_frontend::sdl_audio_callback::SdlAudioCallbackImpl;
use ringbuf::HeapRb;
use ringbuf::traits::Split;
use sdl2::audio::{AudioDevice, AudioSpecDesired};
use std::sync::{
    Arc,
    atomic::{AtomicU32, AtomicUsize, Ordering},
};

/// Audio output handler that receives samples from the NES APU
pub struct SdlNesAudio {
    device: AudioDevice<SdlAudioCallbackImpl>,
    sample_producer: AudioProducer,
    volume: Arc<AtomicU32>,
    stats: Arc<AudioStats>,
    fill_level: Arc<AtomicUsize>,
    actual_sample_rate: i32,
}

impl SdlNesAudio {
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

        // Create bounded ring buffer for sending samples to audio callback.
        let ring_buffer = HeapRb::<f32>::new(Self::BUFFER_SIZE);
        let (producer, consumer) = ring_buffer.split();
        let fill_level = Arc::new(AtomicUsize::new(0));

        // Create shared volume control (default 75% to match tests and avoid distortion)
        let volume = Arc::new(AtomicU32::new(f32::to_bits(0.75)));
        let volume_clone = Arc::clone(&volume);

        let stats = Arc::new(AudioStats::default());
        let stats_clone = Arc::clone(&stats);
        let fill_level_clone = Arc::clone(&fill_level);

        let device =
            audio_subsystem.open_playback(None, &desired_spec, |_spec| SdlAudioCallbackImpl {
                sample_consumer: consumer,
                volume: volume_clone,
                stats: stats_clone,
                fill_level: fill_level_clone,
                resampler: AudioResampler::new(Self::BUFFER_SIZE / 2),
            })?;

        let actual_rate = device.spec().freq;
        if actual_rate != sample_rate {
            log_info(format!(
                "Audio: requested {} Hz, got {} Hz from SDL device",
                sample_rate, actual_rate
            ));
        }

        Ok(Self {
            device,
            sample_producer: producer,
            volume,
            stats,
            fill_level,
            actual_sample_rate: actual_rate,
        })
    }

    /// Returns the current buffered sample count in the ring buffer.
    #[cfg(test)]
    pub fn buffered_samples(&self) -> usize {
        self.fill_level.load(Ordering::Relaxed)
    }
}

impl NesAudio for SdlNesAudio {
    fn queue_sample(&mut self, sample: f32) {
        queue_sample_to_producer(
            &mut self.sample_producer,
            sample,
            &self.stats,
            &self.fill_level,
        );
    }

    fn resume(&self) {
        self.device.resume();
    }

    fn pause(&self) {
        self.device.pause();
    }

    fn set_volume(&self, volume: f32) {
        let clamped = volume.clamp(0.0, 1.0);
        self.volume.store(f32::to_bits(clamped), Ordering::Relaxed);
    }

    fn get_volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    fn prime_startup(&mut self, samples: usize) {
        for _ in 0..samples {
            queue_sample_to_producer(
                &mut self.sample_producer,
                0.0,
                &self.stats,
                &self.fill_level,
            );
        }
    }

    fn take_and_reset_stats(&self) -> (u64, u64, u64) {
        let received = self.stats.received_samples.swap(0, Ordering::Relaxed);
        let dropped = self.stats.dropped_samples.swap(0, Ordering::Relaxed);
        let underrun = self.stats.underrun_samples.swap(0, Ordering::Relaxed);
        (received, dropped, underrun)
    }

    fn actual_sample_rate(&self) -> i32 {
        self.actual_sample_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::types::queue_sample_to_producer;
    use ringbuf::traits::{Consumer, Split};
    use serial_test::serial;
    use std::env;
    use std::sync::{Arc, Barrier};
    use std::time::{Duration, Instant};

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

        let audio = SdlNesAudio::new(&sdl_context, 44100);
        assert!(audio.is_ok(), "Audio initialization should succeed");

        let mut audio = audio.unwrap();

        // Test volume control
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
    #[serial]
    fn test_prime_startup_buffers_silence() {
        struct EnvRestore {
            key: &'static str,
            prev: Option<String>,
        }

        impl Drop for EnvRestore {
            fn drop(&mut self) {
                match &self.prev {
                    Some(value) => unsafe { env::set_var(self.key, value) },
                    None => unsafe { env::remove_var(self.key) },
                }
            }
        }

        let restore = EnvRestore {
            key: "SDL_AUDIODRIVER",
            prev: env::var("SDL_AUDIODRIVER").ok(),
        };
        unsafe {
            env::set_var("SDL_AUDIODRIVER", "dummy");
        }

        let sdl_context = sdl2::init().expect("Failed to initialize SDL2");
        let mut audio = SdlNesAudio::new(&sdl_context, 44100).expect("Audio init should succeed");

        assert_eq!(audio.buffered_samples(), 0);

        audio.prime_startup(2048);

        assert!(audio.buffered_samples() >= 2048);

        drop(restore);
    }

    #[test]
    fn test_queue_sample_does_not_drop_when_buffer_full() {
        let stats = Arc::new(AudioStats::default());
        let ring_buffer = HeapRb::<f32>::new(1);
        let (mut producer, mut consumer) = ring_buffer.split();
        let fill_level = Arc::new(AtomicUsize::new(0));

        queue_sample_to_producer(&mut producer, 0.1, &stats, &fill_level);

        let barrier = Arc::new(Barrier::new(2));
        let barrier_consumer = Arc::clone(&barrier);
        let (result_tx, result_rx) = std::sync::mpsc::channel::<(f32, f32)>();
        let (producer_ready_tx, producer_ready_rx) = std::sync::mpsc::channel::<()>();

        let fill_level_consumer = Arc::clone(&fill_level);
        let consumer = std::thread::spawn(move || {
            barrier_consumer.wait();

            let first = {
                let start = Instant::now();
                loop {
                    if let Some(value) = consumer.try_pop() {
                        fill_level_consumer.fetch_sub(1, Ordering::Relaxed);
                        break value;
                    }
                    if start.elapsed() > Duration::from_millis(200) {
                        panic!("expected first sample");
                    }
                    std::thread::yield_now();
                }
            };

            let second = {
                let start = Instant::now();
                loop {
                    if let Some(value) = consumer.try_pop() {
                        fill_level_consumer.fetch_sub(1, Ordering::Relaxed);
                        break value;
                    }
                    if start.elapsed() > Duration::from_millis(200) {
                        panic!("expected second sample (must not be dropped)");
                    }
                    std::thread::yield_now();
                }
            };

            result_tx
                .send((first, second))
                .expect("failed to send samples to main thread");
        });

        let stats_producer = Arc::clone(&stats);
        let fill_level_producer = Arc::clone(&fill_level);
        let producer = std::thread::spawn(move || {
            producer_ready_tx
                .send(())
                .expect("failed to signal producer readiness");
            queue_sample_to_producer(&mut producer, 0.2, &stats_producer, &fill_level_producer);
        });

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
