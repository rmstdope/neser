// ── PCM FIFO ─────────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};

/// GBA FIFO capacity: 32 bytes per GBATek spec.
pub(super) const FIFO_CAPACITY: usize = 32;

/// PCM FIFO channel (A or B).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FifoChannel {
    samples: std::collections::VecDeque<i8>,
    /// The sample currently being output.
    pub current: i8,
}

impl FifoChannel {
    /// Push one 8-bit signed sample into the FIFO (ignored when full).
    pub fn push(&mut self, sample: i8) {
        if self.samples.len() < FIFO_CAPACITY {
            self.samples.push_back(sample);
        }
    }

    /// Push four bytes (one word) into the FIFO, low byte first.
    pub fn push_word(&mut self, word: u32) {
        for i in 0..4 {
            self.push(((word >> (i * 8)) as u8) as i8);
        }
    }

    /// Consume the next sample from the FIFO, updating `current`.
    /// When the FIFO is empty the current sample holds.
    pub fn advance(&mut self) {
        if let Some(s) = self.samples.pop_front() {
            self.current = s;
        }
    }

    /// Clear FIFO and reset current sample to 0.
    pub fn clear(&mut self) {
        self.samples.clear();
        self.current = 0;
    }

    /// Number of samples waiting in the FIFO.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// True when the FIFO is empty.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Normalised output in `[-1.0, 1.0]`.
    pub fn output(&self) -> f32 {
        self.current as f32 / 128.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fifo_push_and_advance() {
        let mut fifo = FifoChannel::default();
        fifo.push(42);
        fifo.push(-10);
        fifo.advance();
        assert_eq!(fifo.current, 42);
        fifo.advance();
        assert_eq!(fifo.current, -10);
    }

    #[test]
    fn test_fifo_capacity_limit() {
        let mut fifo = FifoChannel::default();
        for i in 0..40i8 {
            fifo.push(i);
        }
        assert_eq!(
            fifo.len(),
            FIFO_CAPACITY,
            "FIFO should cap at {FIFO_CAPACITY}"
        );
    }

    #[test]
    fn test_fifo_clear() {
        let mut fifo = FifoChannel::default();
        fifo.push(10);
        fifo.clear();
        assert!(fifo.is_empty());
        assert_eq!(fifo.current, 0);
    }

    #[test]
    fn test_fifo_output_normalised() {
        let mut fifo = FifoChannel::default();
        fifo.push(64);
        fifo.advance();
        let out = fifo.output();
        assert!(
            (out - 0.5).abs() < 1e-6,
            "64 / 128 should be 0.5, got {out}"
        );
    }
}
