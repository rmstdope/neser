use ringbuf::HeapRb;
use ringbuf::traits::{Producer, Split};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Producer half of the audio ring buffer.
pub type AudioProducer = <HeapRb<f32> as Split>::Prod;

/// Consumer half of the audio ring buffer; used by the audio callback.
#[allow(dead_code)]
pub type AudioConsumer = <HeapRb<f32> as Split>::Cons;

/// Atomic counters for tracking audio pipeline health.
#[derive(Default)]
pub struct AudioStats {
    pub received_samples: AtomicU64,
    pub dropped_samples: AtomicU64,
    pub underrun_samples: AtomicU64,
}

/// Pushes a sample into the ring buffer, blocking if full.
///
/// Provides backpressure instead of dropping samples.
/// Dropped samples create discontinuities that can manifest as audible clicks.
pub fn queue_sample_to_producer(
    producer: &mut AudioProducer,
    sample: f32,
    _stats: &AudioStats,
    fill_level: &AtomicUsize,
) {
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

/// Maximum combined output level of the NES APU mixer (all five channels at peak
/// output plus mixer headroom for expansion audio from mappers such as VRC6,
/// Sunsoft 5B, etc.).
const NES_APU_MAX: f32 = 1.177;

/// Applies volume scaling and NES APU normalisation to a raw audio sample.
///
/// Divides by [`NES_APU_MAX`] so the APU's `[0.0, ~1.177]` range maps
/// to `[0.0, 1.0]`, then applies volume and clamps to `[-1.0, 1.0]`.
pub fn process_sample(raw_sample: f32, volume: f32) -> f32 {
    ((raw_sample / NES_APU_MAX) * volume).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::HeapRb;
    use ringbuf::traits::{Consumer, Split};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    #[test]
    fn test_queue_sample_does_not_drop_when_buffer_full() {
        let stats = Arc::new(AudioStats::default());
        let ring_buffer = HeapRb::<f32>::new(1);
        let (mut producer, mut consumer) = ring_buffer.split();
        let fill_level = Arc::new(AtomicUsize::new(0));

        queue_sample_to_producer(&mut producer, 0.1, &stats, &fill_level);

        let barrier = Arc::new(std::sync::Barrier::new(2));
        let barrier_consumer = Arc::clone(&barrier);
        let (result_tx, result_rx) = std::sync::mpsc::channel::<(f32, f32)>();
        let (producer_ready_tx, producer_ready_rx) = std::sync::mpsc::channel::<()>();

        let fill_level_consumer = Arc::clone(&fill_level);
        let consumer_thread = std::thread::spawn(move || {
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

            result_tx.send((first, second)).expect("failed to send");
        });

        let stats_producer = Arc::clone(&stats);
        let fill_level_producer = Arc::clone(&fill_level);
        let producer_thread = std::thread::spawn(move || {
            producer_ready_tx.send(()).expect("failed to signal");
            queue_sample_to_producer(&mut producer, 0.2, &stats_producer, &fill_level_producer);
        });

        producer_ready_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("producer did not become ready");

        barrier.wait();

        producer_thread.join().expect("producer thread panicked");
        consumer_thread.join().expect("consumer thread panicked");

        let (first, second) = result_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("expected samples");
        assert_eq!(first, 0.1);
        assert_eq!(second, 0.2);

        let dropped = stats.dropped_samples.load(Ordering::Relaxed);
        assert_eq!(dropped, 0, "no samples should be dropped");
    }

    #[test]
    fn test_process_sample_scales_and_clamps() {
        let result = process_sample(0.0, 0.75);
        assert!((result - 0.0).abs() < 0.0001);

        let result = process_sample(NES_APU_MAX, 1.0);
        assert!((result - 1.0).abs() < 0.0001);

        let result = process_sample(NES_APU_MAX, 0.5);
        assert!((result - 0.5).abs() < 0.01);

        let result = process_sample(NES_APU_MAX * 2.0, 1.0);
        assert_eq!(result, 1.0, "should clamp to 1.0");
    }
}
