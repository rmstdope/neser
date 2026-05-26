//! GBA timer subsystem.
//!
//! The GBA has four 16-bit timers (TM0–TM3) at I/O addresses
//! `0x0400_0100`..`0x0400_010F`. Each timer has two registers:
//!
//! * `CNT_L` — read returns the live counter, write sets the *reload* value
//!   (the counter is loaded from the reload value when the timer is
//!   re-enabled or on overflow).
//! * `CNT_H` — control:
//!   * bits 0–1: prescaler — `00`=1, `01`=64, `10`=256, `11`=1024 cycles.
//!   * bit 2: cascade — when set, the timer ticks each time the previous
//!     timer overflows instead of using its own prescaler. Has no effect on
//!     timer 0.
//!   * bit 6: IRQ on overflow.
//!   * bit 7: enable (rising-edge reloads counter from reload value).
//!
//! Modeled per GBATek "Timer Registers".
//!
//! <https://problemkaputt.de/gbatek.htm#gbatimers>

use super::interrupt::{InterruptController, bits};
use serde::{Deserialize, Serialize};

/// The four GBA timer IRQ bits, indexed 0–3.
const TIMER_IRQ_BITS: [u16; 4] = [bits::TIMER0, bits::TIMER1, bits::TIMER2, bits::TIMER3];

/// Prescaler period in CPU cycles for `CNT_H[1:0]`.
const PRESCALERS: [u32; 4] = [1, 64, 256, 1024];

/// Single GBA timer channel.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct Timer {
    /// Live counter value (the value reported by `CNT_L` reads).
    pub counter: u16,
    /// Reload value latched from `CNT_L` writes; loaded into `counter` on
    /// rising-edge enable and on overflow.
    pub reload: u16,
    /// Raw `CNT_H` register value (control bits).
    pub control: u16,
    /// Internal prescaler tick accumulator, in CPU cycles.
    prescaler_acc: u32,
}

impl Timer {
    /// Whether this timer is enabled (`CNT_H` bit 7).
    pub fn enabled(&self) -> bool {
        self.control & 0x80 != 0
    }

    /// Whether this timer should raise an IRQ on overflow (`CNT_H` bit 6).
    pub fn irq_on_overflow(&self) -> bool {
        self.control & 0x40 != 0
    }

    /// Whether this timer is in cascade mode (`CNT_H` bit 2).
    pub fn cascade(&self) -> bool {
        self.control & 0x04 != 0
    }

    /// Prescaler period in CPU cycles.
    pub fn prescaler(&self) -> u32 {
        PRESCALERS[(self.control & 0x3) as usize]
    }
}

/// Bank of four GBA timers TM0–TM3.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Timers {
    pub channels: [Timer; 4],
}

impl Timers {
    /// Create a new bank with all timers disabled and zeroed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Read `CNT_L` of timer `i` — returns the live counter.
    pub fn read_cnt_l(&self, i: usize) -> u16 {
        self.channels[i].counter
    }

    /// Read `CNT_H` of timer `i`.
    pub fn read_cnt_h(&self, i: usize) -> u16 {
        // Only documented bits are observable.
        self.channels[i].control & 0x00C7
    }

    /// Write `CNT_L` of timer `i` — updates the reload latch only.
    pub fn write_cnt_l(&mut self, i: usize, value: u16) {
        self.channels[i].reload = value;
    }

    /// Write `CNT_H` of timer `i` — handles enable rising-edge reload.
    pub fn write_cnt_h(&mut self, i: usize, value: u16) {
        let value = value & 0x00C7;
        let was_enabled = self.channels[i].enabled();
        self.channels[i].control = value;
        let now_enabled = self.channels[i].enabled();
        if !was_enabled && now_enabled {
            // Rising edge: reload counter and reset prescaler accumulator.
            self.channels[i].counter = self.channels[i].reload;
            self.channels[i].prescaler_acc = 0;
        }
    }

    pub fn align_prescaler_phase(&mut self, i: usize, global_phase: u32) {
        let period = self.channels[i].prescaler();
        self.channels[i].prescaler_acc = global_phase % period;
    }

    /// Advance all timers by `cycles` CPU cycles. Any overflows are routed to
    /// the supplied [`InterruptController`].
    ///
    /// Returns the number of overflows per timer `[TM0, TM1, TM2, TM3]`.
    /// Callers that only need to know *whether* a timer overflowed can check
    /// `count > 0`; callers that need to advance state N times per overflow
    /// (e.g. FIFO channels) use the exact count.
    pub fn step(&mut self, cycles: u32, ic: &mut InterruptController) -> [u32; 4] {
        // Track per-timer overflow counts so cascade can chain through TM1..TM3.
        let mut overflows = [0u32; 4];
        for (i, t) in self.channels.iter_mut().enumerate() {
            if !t.enabled() {
                continue;
            }
            // Per GBATek the cascade bit has no effect on TM0 (there is no
            // previous timer to feed it). For TM1..TM3, cascade-mode timers
            // don't tick from CPU cycles — their ticks come from the
            // previous timer's overflow count, applied below.
            if i > 0 && t.cascade() {
                continue;
            }
            let cycles_to_first_overflow = cycles_to_first_overflow(t, cycles);
            t.prescaler_acc += cycles;
            let period = t.prescaler();
            let ticks = t.prescaler_acc / period;
            t.prescaler_acc %= period;
            overflows[i] = advance_ticks(t, ticks);
            if overflows[i] > 0
                && t.irq_on_overflow()
                && let Some(cycles_to_first_overflow) = cycles_to_first_overflow
            {
                let cycles_late = cycles.saturating_sub(cycles_to_first_overflow);
                ic.raise_late(TIMER_IRQ_BITS[i], cycles_late);
            }
        }
        // Cascade: feed previous-timer overflow count into next cascade timer.
        for i in 1..4 {
            let cascade_ticks = overflows[i - 1];
            if cascade_ticks == 0 {
                continue;
            }
            let t = &mut self.channels[i];
            if !t.enabled() || !t.cascade() {
                continue;
            }
            overflows[i] = advance_ticks(t, cascade_ticks);
        }
        // Raise IRQs for timers whose overflow IRQ bit is set.
        for i in 0..4 {
            if overflows[i] > 0 && self.channels[i].irq_on_overflow() {
                ic.raise(TIMER_IRQ_BITS[i]);
            }
        }
        overflows
    }
}

fn cycles_to_first_overflow(timer: &Timer, cycles: u32) -> Option<u32> {
    if cycles == 0 {
        return None;
    }
    let period = timer.prescaler();
    let cycles_to_next_tick = if timer.prescaler_acc == 0 {
        period
    } else {
        period - timer.prescaler_acc
    };
    let room = 0x1_0000u32 - u32::from(timer.counter);
    let cycles_to_overflow = cycles_to_next_tick + (room - 1) * period;
    (cycles_to_overflow <= cycles).then_some(cycles_to_overflow)
}

/// Advance `timer` by `ticks` counter increments, reloading from `timer.reload`
/// on each overflow. Returns the number of overflows that occurred.
fn advance_ticks(timer: &mut Timer, mut ticks: u32) -> u32 {
    let mut overflows = 0u32;
    while ticks > 0 {
        let room = 0x1_0000u32 - timer.counter as u32;
        if ticks >= room {
            ticks -= room;
            timer.counter = timer.reload;
            overflows += 1;
        } else {
            timer.counter = timer.counter.wrapping_add(ticks as u16);
            ticks = 0;
        }
    }
    overflows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enable(prescaler: u16, irq: bool, cascade: bool) -> u16 {
        let mut c = 0x80 | (prescaler & 0x3);
        if irq {
            c |= 0x40;
        }
        if cascade {
            c |= 0x04;
        }
        c
    }

    #[test]
    fn prescaler_zero_ticks_every_cycle() {
        let mut t = Timers::new();
        let mut ic = InterruptController::new();
        t.write_cnt_l(0, 0);
        t.write_cnt_h(0, enable(0, false, false));
        t.step(1024, &mut ic);
        assert_eq!(t.read_cnt_l(0), 1024);
    }

    #[test]
    fn prescaler_64() {
        let mut t = Timers::new();
        let mut ic = InterruptController::new();
        t.write_cnt_h(0, enable(1, false, false));
        t.step(64 * 5, &mut ic);
        assert_eq!(t.read_cnt_l(0), 5);
        // Sub-period accumulation persists across calls.
        t.step(63, &mut ic);
        assert_eq!(t.read_cnt_l(0), 5);
        t.step(1, &mut ic);
        assert_eq!(t.read_cnt_l(0), 6);
    }

    #[test]
    fn prescaler_256_and_1024() {
        let mut t = Timers::new();
        let mut ic = InterruptController::new();
        t.write_cnt_h(1, enable(2, false, false));
        t.write_cnt_h(2, enable(3, false, false));
        t.step(256 * 3, &mut ic);
        assert_eq!(t.read_cnt_l(1), 3);
        t.step(1024 * 2 - 256 * 3, &mut ic);
        // Timer 1 keeps counting up
        assert!(t.read_cnt_l(1) > 3);
        let t2 = t.read_cnt_l(2);
        assert_eq!(t2, 2);
    }

    #[test]
    fn enable_rising_edge_loads_reload() {
        let mut t = Timers::new();
        t.write_cnt_l(0, 0x1234);
        // Before enable, reload writes don't update the counter.
        assert_eq!(t.read_cnt_l(0), 0);
        t.write_cnt_h(0, enable(0, false, false));
        // Rising edge of enable loads the counter from reload.
        assert_eq!(t.read_cnt_l(0), 0x1234);
    }

    #[test]
    fn overflow_reloads_from_reload_value() {
        let mut t = Timers::new();
        let mut ic = InterruptController::new();
        t.write_cnt_l(0, 0xFFF0);
        t.write_cnt_h(0, enable(0, false, false));
        // 0x10 ticks brings counter from 0xFFF0 to 0x0000 → overflow.
        t.step(0x10, &mut ic);
        assert_eq!(t.read_cnt_l(0), 0xFFF0);
    }

    #[test]
    fn overflow_raises_irq_when_enabled() {
        let mut t = Timers::new();
        let mut ic = InterruptController::new();
        ic.write_ie(bits::TIMER0);
        ic.write_ime(1);
        t.write_cnt_l(0, 0xFFFF);
        t.write_cnt_h(0, enable(0, true, false));
        t.step(1, &mut ic);
        assert_eq!(t.read_cnt_l(0), 0xFFFF); // reload
        assert!(ic.irq_line());
    }

    #[test]
    fn no_irq_when_irq_bit_clear() {
        let mut t = Timers::new();
        let mut ic = InterruptController::new();
        ic.write_ie(bits::TIMER0);
        ic.write_ime(1);
        t.write_cnt_l(0, 0xFFFF);
        t.write_cnt_h(0, enable(0, false, false));
        t.step(1, &mut ic);
        assert!(!ic.irq_line());
    }

    #[test]
    fn cascade_increments_on_previous_overflow() {
        let mut t = Timers::new();
        let mut ic = InterruptController::new();
        t.write_cnt_l(0, 0xFFFF);
        t.write_cnt_h(0, enable(0, false, false));
        t.write_cnt_l(1, 0);
        t.write_cnt_h(1, enable(0, false, true)); // cascade on
        t.step(1, &mut ic);
        // TM0 overflowed once; TM1 should be 1.
        assert_eq!(t.read_cnt_l(1), 1);
    }

    #[test]
    fn cascade_does_not_apply_to_timer0() {
        let mut t = Timers::new();
        let mut ic = InterruptController::new();
        // Per GBATek the cascade bit has no effect on TM0, so with
        // prescaler=0 the counter should advance once per CPU cycle even
        // when the cascade flag is set.
        t.write_cnt_h(0, enable(0, false, true));
        t.step(100, &mut ic);
        assert_eq!(
            t.read_cnt_l(0),
            100,
            "TM0 should tick normally even when the cascade flag is set"
        );
    }

    #[test]
    fn full_overflow_cycle() {
        // Acceptance: prescaler=0, tick 0x10000 cycles → wrap to 0 + IRQ.
        let mut t = Timers::new();
        let mut ic = InterruptController::new();
        ic.write_ie(bits::TIMER0);
        ic.write_ime(1);
        t.write_cnt_l(0, 0); // reload 0
        t.write_cnt_h(0, enable(0, true, false));
        t.step(0x1_0000, &mut ic);
        assert_eq!(t.read_cnt_l(0), 0);
        assert!(ic.irq_line());
    }
}
