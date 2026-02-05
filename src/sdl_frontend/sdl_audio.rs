use crate::debugging::log_info;
/// Audio output module for the NES APU
///
/// This module handles SDL2 audio initialization and manages the audio callback
/// that retrieves samples from the APU.
use crate::sdl_frontend::sdl_audio_callback::SdlAudioCallbackImpl;
use crate::sdl_frontend::sdl_audio_resampler::SdlAudioResampler;
use ringbuf::HeapRb;
use ringbuf::traits::{Producer, Split};
use sdl2::audio::{AudioDevice, AudioSpecDesired};
use std::sync::{
    Arc,
    atomic::AtomicU64,
    atomic::{AtomicU32, AtomicUsize, Ordering},
};

pub(crate) type AudioProducer = <HeapRb<f32> as Split>::Prod;
pub(crate) type AudioConsumer = <HeapRb<f32> as Split>::Cons;

/// Audio output handler that receives samples from the NES APU
pub struct SdlNesAudio {
    device: AudioDevice<SdlAudioCallbackImpl>,
    sample_producer: AudioProducer,
    volume: Arc<AtomicU32>,
    stats: Arc<AudioStats>,
    fill_level: Arc<AtomicUsize>,
    actual_sample_rate: i32,
}

#[derive(Default)]
pub(crate) struct AudioStats {
    pub(crate) received_samples: AtomicU64,
    pub(crate) dropped_samples: AtomicU64,
    pub(crate) underrun_samples: AtomicU64,
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
                resampler: SdlAudioResampler::new(Self::BUFFER_SIZE / 2),
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
        queue_sample_to_producer(
            &mut self.sample_producer,
            sample,
            &self.stats,
            &self.fill_level,
        );
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

    /// Returns the current buffered sample count in the ring buffer.
    #[cfg(test)]
    pub fn buffered_samples(&self) -> usize {
        self.fill_level.load(Ordering::Relaxed)
    }

    /// Start audio playback
    pub fn resume(&self) {
        self.device.resume();
    }

    /// Pre-fills the audio buffer with silence to avoid startup underruns.
    pub fn prime_startup(&mut self, samples: usize) {
        for _ in 0..samples {
            queue_sample_to_producer(
                &mut self.sample_producer,
                0.0,
                &self.stats,
                &self.fill_level,
            );
        }
    }

    /// Pause audio playback
    #[cfg(test)]
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

fn queue_sample_to_producer(
    producer: &mut AudioProducer,
    sample: f32,
    _stats: &AudioStats,
    fill_level: &AtomicUsize,
) {
    // Blocking push provides backpressure instead of dropping samples.
    // Dropped samples create discontinuities that can manifest as audible clicks.
    let mut pending = sample;
    loop {
        match producer.try_push(pending) {
            Ok(()) => {
                fill_level.fetch_add(1, Ordering::Relaxed);
                return;
            }
            Err(sample) => {
                pending = sample;
                std::thread::yield_now();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdl_frontend::sdl_audio_resampler::SdlAudioResampler;
    use ringbuf::traits::{Consumer, Split};
    use serial_test::serial;
    use std::collections::VecDeque;
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
        // Desired behavior: when the bounded audio buffer is full, do NOT drop samples.
        // Dropping samples introduces discontinuities that can manifest as clicks.

        let stats = Arc::new(AudioStats::default());
        let ring_buffer = HeapRb::<f32>::new(1);
        let (mut producer, mut consumer) = ring_buffer.split();
        let fill_level = Arc::new(AtomicUsize::new(0));

        // Fill the buffer.
        queue_sample_to_producer(&mut producer, 0.1, &stats, &fill_level);

        let barrier = Arc::new(Barrier::new(2));
        let barrier_consumer = Arc::clone(&barrier);
        let (result_tx, result_rx) = std::sync::mpsc::channel::<(f32, f32)>();
        let (producer_ready_tx, producer_ready_rx) = std::sync::mpsc::channel::<()>();

        let fill_level_consumer = Arc::clone(&fill_level);
        let consumer = std::thread::spawn(move || {
            // Ensure producer attempts to enqueue while the queue is full.
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
            // Signal that we're about to attempt enqueue while the queue is full.
            producer_ready_tx
                .send(())
                .expect("failed to signal producer readiness");
            queue_sample_to_producer(&mut producer, 0.2, &stats_producer, &fill_level_producer);
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

    #[test]
    fn test_resampler_rate_clamps_to_limits() {
        let mut resampler = SdlAudioResampler::new(100);

        resampler.update_rate(100);
        assert!((resampler.rate() - 1.0).abs() < 0.00001);

        resampler.update_rate(0);
        assert!((resampler.rate() - (1.0 - SdlAudioResampler::MAX_RATE_ADJUST)).abs() < 0.00001);

        resampler.update_rate(200);
        assert!((resampler.rate() - (1.0 + SdlAudioResampler::MAX_RATE_ADJUST)).abs() < 0.00001);
    }

    #[test]
    fn test_resampler_outputs_source_sequence_at_unity_rate() {
        let mut resampler = SdlAudioResampler::new(4);
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
