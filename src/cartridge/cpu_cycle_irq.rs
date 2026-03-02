//! Shared CPU-cycle IRQ counter used by multiple mappers.
//!
//! Handles the common pattern of a u16 counter ticked on each CPU cycle,
//! with configurable counting direction, threshold, and post-fire behavior.

/// Defines how the IRQ counter ticks and fires.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CpuCycleIrqMode {
    /// Count up with mask, fire when counter >= threshold (level-sensitive).
    /// Sets `pending = (counter >= threshold)` every tick.
    /// Used by: Mapper 42
    UpLevel { threshold: u16, mask: u16 },

    /// Count up, fire when counter >= threshold, reset counter to 0 on fire.
    /// Used by: Mapper 43
    UpReset { threshold: u16 },

    /// Count up, fire when counter == threshold, auto-disable on fire.
    /// Used by: Mapper 50
    UpAutoDisable { threshold: u16 },

    /// Count up, fire at `fire_count`, self-acknowledge at `ack_count`.
    /// Used by: NTDEC 2722 (Mapper 40)
    UpSelfAck { fire_count: u16, ack_count: u16 },

    /// Count down, fire when counter reaches 0. Counter stops at 0.
    /// Used by: Mapper 65, Bandai FCG
    DownToZero,

    /// Count down, check for 0 before decrement. Auto-disable on fire.
    /// Used by: Mapper 67
    DownAutoDisable,

    /// Count down with wrapping. Fire on underflow (0 → 0xFFFF).
    /// `enabled` only gates IRQ assertion, not counting.
    /// Used by: Sunsoft FME-7
    DownUnderflow,
}

/// A configurable CPU-cycle IRQ counter shared by many mappers.
pub struct CpuCycleIrq {
    counter: u16,
    enabled: bool,
    pending: bool,
    reload: u16,
    mode: CpuCycleIrqMode,
}

impl CpuCycleIrq {
    pub fn new(mode: CpuCycleIrqMode) -> Self {
        Self {
            counter: 0,
            enabled: false,
            pending: false,
            reload: 0,
            mode,
        }
    }

    /// Tick the IRQ counter for one CPU cycle.
    pub fn tick(&mut self) {
        match self.mode {
            CpuCycleIrqMode::UpLevel { threshold, mask } => {
                if !self.enabled {
                    return;
                }
                self.counter = (self.counter + 1) & mask;
                self.pending = self.counter >= threshold;
            }
            CpuCycleIrqMode::UpReset { threshold } => {
                if !self.enabled {
                    return;
                }
                self.counter = self.counter.wrapping_add(1);
                if self.counter >= threshold {
                    self.counter = 0;
                    self.pending = true;
                }
            }
            CpuCycleIrqMode::UpAutoDisable { threshold } => {
                if !self.enabled {
                    return;
                }
                self.counter = self.counter.wrapping_add(1);
                if self.counter == threshold {
                    self.pending = true;
                    self.enabled = false;
                }
            }
            CpuCycleIrqMode::UpSelfAck {
                fire_count,
                ack_count,
            } => {
                if !self.enabled {
                    return;
                }
                self.counter += 1;
                if self.counter == fire_count {
                    self.pending = true;
                } else if self.counter == ack_count {
                    self.pending = false;
                }
            }
            CpuCycleIrqMode::DownToZero => {
                if !self.enabled || self.counter == 0 {
                    return;
                }
                self.counter -= 1;
                if self.counter == 0 {
                    self.pending = true;
                }
            }
            CpuCycleIrqMode::DownAutoDisable => {
                if !self.enabled {
                    return;
                }
                if self.counter == 0 {
                    self.pending = true;
                    self.enabled = false;
                    return;
                }
                self.counter -= 1;
            }
            CpuCycleIrqMode::DownUnderflow => {
                // enabled only gates IRQ assertion, not counting
                self.counter = self.counter.wrapping_sub(1);
                if self.counter == 0xFFFF && self.enabled {
                    self.pending = true;
                }
            }
        }
    }

    /// Returns whether an IRQ is pending.
    pub fn is_pending(&self) -> bool {
        self.pending
    }

    /// Acknowledge (clear) a pending IRQ.
    pub fn acknowledge(&mut self) {
        self.pending = false;
    }

    pub fn counter(&self) -> u16 {
        self.counter
    }

    pub fn set_counter(&mut self, value: u16) {
        self.counter = value;
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, value: bool) {
        self.enabled = value;
    }

    pub fn set_pending(&mut self, value: bool) {
        self.pending = value;
    }

    pub fn reload(&self) -> u16 {
        self.reload
    }

    pub fn set_reload(&mut self, value: u16) {
        self.reload = value;
    }

    /// Load counter from reload register.
    pub fn reload_counter(&mut self) {
        self.counter = self.reload;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_up_level_fires_at_threshold_and_clears_below() {
        // Mapper 42 style: level-sensitive, 15-bit counter, threshold 0x6000
        let mut irq = CpuCycleIrq::new(CpuCycleIrqMode::UpLevel {
            threshold: 0x6000,
            mask: 0x7FFF,
        });
        irq.set_enabled(true);

        // Tick up to threshold - 1: should not be pending
        for _ in 0..0x5FFF {
            irq.tick();
        }
        assert!(!irq.is_pending(), "should not be pending below threshold");

        // One more tick reaches threshold: should be pending
        irq.tick();
        assert!(irq.is_pending(), "should be pending at threshold");

        // Counter wraps around via mask back below threshold → no longer pending
        // 0x7FFF - 0x6000 + 1 = 0x2000 more ticks to wrap around
        for _ in 0..0x2000 {
            irq.tick();
        }
        assert!(
            !irq.is_pending(),
            "should not be pending after wrapping below threshold"
        );
    }

    #[test]
    fn test_count_up_reset_fires_and_resets_counter() {
        // Mapper 43 style: fire at 0x1000, reset counter
        let mut irq = CpuCycleIrq::new(CpuCycleIrqMode::UpReset { threshold: 0x1000 });
        irq.set_enabled(true);

        // Tick to threshold
        for _ in 0..0x1000 {
            irq.tick();
        }
        assert!(irq.is_pending(), "should fire at threshold");
        assert_eq!(irq.counter(), 0, "counter should reset to 0 on fire");

        // Acknowledge and tick again to next threshold
        irq.acknowledge();
        for _ in 0..0x1000 {
            irq.tick();
        }
        assert!(
            irq.is_pending(),
            "should fire again after reset and re-counting"
        );
        assert_eq!(irq.counter(), 0, "counter should reset again");
    }

    #[test]
    fn test_count_up_auto_disable_fires_once() {
        // Mapper 50 style: fire at 0x1000, auto-disable
        let mut irq = CpuCycleIrq::new(CpuCycleIrqMode::UpAutoDisable { threshold: 0x1000 });
        irq.set_enabled(true);

        for _ in 0..0x1000 {
            irq.tick();
        }
        assert!(irq.is_pending(), "should fire at threshold");
        assert!(!irq.enabled(), "should auto-disable on fire");

        // Further ticks should not change state (disabled)
        irq.acknowledge();
        for _ in 0..0x1000 {
            irq.tick();
        }
        assert!(
            !irq.is_pending(),
            "should not fire again while auto-disabled"
        );
    }

    #[test]
    fn test_count_up_self_ack_fires_and_self_acks() {
        // NTDEC 2722 style: fire at 4096, self-ack at 8192
        let mut irq = CpuCycleIrq::new(CpuCycleIrqMode::UpSelfAck {
            fire_count: 4096,
            ack_count: 8192,
        });
        irq.set_enabled(true);

        // Tick to fire count
        for _ in 0..4096 {
            irq.tick();
        }
        assert!(irq.is_pending(), "should fire at fire_count");

        // Continue ticking to self-ack count
        for _ in 0..4096 {
            irq.tick();
        }
        assert!(!irq.is_pending(), "should self-acknowledge at ack_count");
    }

    #[test]
    fn test_count_down_to_zero_fires_at_zero() {
        // Mapper 65 / Bandai FCG style: count down, fire at 0, stop at 0
        let mut irq = CpuCycleIrq::new(CpuCycleIrqMode::DownToZero);
        irq.set_enabled(true);
        irq.set_counter(100);

        // Tick 100 times to reach 0
        for _ in 0..100 {
            irq.tick();
        }
        assert!(irq.is_pending(), "should fire when counter reaches 0");
        assert_eq!(irq.counter(), 0, "counter should be 0");

        // Further ticks should not change counter (stopped at 0)
        irq.acknowledge();
        irq.tick();
        assert!(!irq.is_pending(), "should not fire again when stopped at 0");
        assert_eq!(irq.counter(), 0, "counter should remain 0");
    }

    #[test]
    fn test_count_down_auto_disable_fires_and_disables() {
        // Mapper 67 style: check 0 before decrement, auto-disable
        let mut irq = CpuCycleIrq::new(CpuCycleIrqMode::DownAutoDisable);
        irq.set_enabled(true);
        irq.set_counter(5);

        // Tick 5 times to decrement to 0
        for _ in 0..5 {
            irq.tick();
        }
        assert_eq!(
            irq.counter(),
            0,
            "counter should reach 0 after 5 decrements"
        );

        // Next tick sees counter==0 and fires + auto-disables
        irq.tick();
        assert!(irq.is_pending(), "should fire when counter is 0");
        assert!(!irq.enabled(), "should auto-disable on fire");
    }

    #[test]
    fn test_count_down_underflow_fires_on_wrap() {
        // Sunsoft FME-7 style: wrapping decrement, fire on 0→0xFFFF
        let mut irq = CpuCycleIrq::new(CpuCycleIrqMode::DownUnderflow);
        irq.set_enabled(true);
        irq.set_counter(2);

        // Tick 1: counter goes 2→1
        irq.tick();
        assert!(!irq.is_pending());
        assert_eq!(irq.counter(), 1);

        // Tick 2: counter goes 1→0
        irq.tick();
        assert!(!irq.is_pending());
        assert_eq!(irq.counter(), 0);

        // Tick 3: counter goes 0→0xFFFF (underflow) → fire
        irq.tick();
        assert!(irq.is_pending(), "should fire on underflow");
        assert_eq!(irq.counter(), 0xFFFF);
    }

    #[test]
    fn test_count_down_underflow_does_not_fire_when_disabled() {
        // FME-7: enabled gates IRQ assertion but NOT counting
        let mut irq = CpuCycleIrq::new(CpuCycleIrqMode::DownUnderflow);
        irq.set_enabled(false);
        irq.set_counter(1);

        // Tick 1: counter 1→0 (counting happens even when disabled)
        irq.tick();
        assert_eq!(
            irq.counter(),
            0,
            "counter should still decrement when disabled"
        );

        // Tick 2: counter 0→0xFFFF (underflow, but disabled so no IRQ)
        irq.tick();
        assert_eq!(irq.counter(), 0xFFFF);
        assert!(
            !irq.is_pending(),
            "should NOT fire on underflow when disabled"
        );
    }

    #[test]
    fn test_acknowledge_clears_pending() {
        let mut irq = CpuCycleIrq::new(CpuCycleIrqMode::DownToZero);
        irq.set_enabled(true);
        irq.set_counter(1);

        irq.tick();
        assert!(irq.is_pending());

        irq.acknowledge();
        assert!(!irq.is_pending(), "acknowledge should clear pending");
    }

    #[test]
    fn test_enabled_gate_prevents_counting() {
        // For modes where enabled gates counting (not DownUnderflow)
        let mut irq = CpuCycleIrq::new(CpuCycleIrqMode::DownToZero);
        irq.set_enabled(false);
        irq.set_counter(5);

        for _ in 0..10 {
            irq.tick();
        }
        assert_eq!(irq.counter(), 5, "counter should not change when disabled");
        assert!(!irq.is_pending(), "should not fire when disabled");
    }
}
