/// Frame Counter for the NES APU
/// Sequences envelope, sweep, and length counter clocks
/// Operates in two modes: 4-step and 5-step
use crate::nes::console::TimingMode;
use crate::trace_apu;
pub struct FrameCounter {
    tv_system: TimingMode,
    mode: Mode,
    irq_inhibit: bool,
    cycle_counter: u32,
    irq_flag: bool,
    irq_assert_cycles_remaining: u8,
    /// Block frame counter ticks for the next N CPU cycles.
    ///
    /// This prevents a delayed $4017 write (which can trigger an immediate
    /// quarter/half-frame clock when bit 7 is set) from producing a back-to-back
    /// tick with the regular sequencer on the very next cycle. Without this, we
    /// can double-clock length counters (e.g., 29829 tick + immediate tick at
    /// cycle 0) and clear pulse-1 early, causing `test_apu_2/test_1.nes`
    /// to fail at the $4015 check.
    block_frame_counter: bool,
    /// Alternate +1 CPU-cycle offset in 5-step mode.
    ///
    /// The 5-step sequence is 37281 CPU cycles (odd length), so the phase between
    /// the frame sequencer and CPU/APU clocks flips every other sequence. We model
    /// that by shifting the step boundaries by +1 cycle on alternating runs. This
    /// is required for timing-sensitive tests like
    /// `apu_test/rom_singles/5-len_timing.nes` and
    /// `blargg_apu_2005.07.30/06.len_timing_mode1.nes`.
    five_step_extra_cycle: bool,
    pending_write: Option<u8>, // Pending write to $4017 register
    write_delay: u8,           // Cycles remaining before pending write takes effect
    pending_write_on_odd_cpu_cycle: bool,
    pending_immediate_clock: (bool, bool), // Extra quarter/half clocks from delayed $4017 side-effects
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    FourStep,
    FiveStep,
}

impl Default for FrameCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameCounter {
    /// Create a new frame counter
    pub fn new() -> Self {
        Self::new_with_tv_system(TimingMode::Ntsc)
    }

    pub fn new_with_tv_system(tv_system: TimingMode) -> Self {
        Self {
            tv_system,
            mode: Mode::FourStep,
            irq_inhibit: false,
            cycle_counter: 0,
            irq_flag: false,
            irq_assert_cycles_remaining: 0,
            block_frame_counter: false,
            five_step_extra_cycle: false,
            pending_write: None,
            write_delay: 0,
            pending_write_on_odd_cpu_cycle: false,
            pending_immediate_clock: (false, false),
        }
    }

    /// Reset frame counter to initial state
    pub fn reset(&mut self) {
        let tv_system = self.tv_system;
        *self = Self::new_with_tv_system(tv_system);
    }

    /// Write to frame counter register ($4017) immediately (for internal/test use only)
    ///
    /// **Note**: For CPU writes to $4017, use `Apu::write_frame_counter()` instead.
    /// This method applies the write immediately without the 3-4 cycle delay
    /// that real hardware exhibits for CPU writes.
    ///
    /// Bit 7: Mode (0 = 4-step, 1 = 5-step)
    /// Bit 6: IRQ inhibit (1 = disable IRQ)
    pub(crate) fn write_register(&mut self, value: u8) {
        let new_mode = if (value & 0x80) != 0 {
            Mode::FiveStep
        } else {
            Mode::FourStep
        };
        trace_apu!(
            2; "frame_counter write_register value=0x{:02X} mode={} irq_inhibit={}",
            value,
            if new_mode == Mode::FiveStep { "5-step" } else { "4-step" },
            (value & 0x40) != 0
        );
        self.mode = new_mode;
        self.irq_inhibit = (value & 0x40) != 0;
        self.cycle_counter = 0;
        // Note: Phase not tracked here as we don't know the APU cycle
        self.five_step_extra_cycle = false;
        self.irq_assert_cycles_remaining = 0;
        self.block_frame_counter = false;

        // Writing 1 to IRQ inhibit clears the IRQ flag
        if (value & 0x40) != 0 {
            self.irq_flag = false;
        }

        // If mode is set (5-step), generate immediate quarter+half clocks.
        // This matches NESDev: "Writing to $4017 with bit 7 set will immediately
        // generate a clock for both the quarter frame and the half frame units."
        if new_mode == Mode::FiveStep {
            self.pending_immediate_clock = (true, true);
        }
    }

    /// Get the current mode
    pub fn get_mode(&self) -> bool {
        self.mode == Mode::FiveStep
    }

    /// Check if IRQ is inhibited
    #[cfg(test)]
    pub fn is_irq_inhibited(&self) -> bool {
        self.irq_inhibit
    }

    /// Get the current cycle counter
    pub fn get_cycle_counter(&self) -> u32 {
        self.cycle_counter
    }

    pub(crate) fn pending_write(&self) -> Option<u8> {
        self.pending_write
    }

    pub(crate) fn write_delay(&self) -> u8 {
        self.write_delay
    }

    pub(crate) fn pending_write_on_odd_cpu_cycle(&self) -> bool {
        self.pending_write_on_odd_cpu_cycle
    }

    pub(crate) fn pending_immediate_clock(&self) -> (bool, bool) {
        self.pending_immediate_clock
    }

    pub(crate) fn irq_assert_cycles_remaining(&self) -> u8 {
        self.irq_assert_cycles_remaining
    }

    pub(crate) fn block_frame_counter(&self) -> bool {
        self.block_frame_counter
    }

    pub(crate) fn five_step_extra_cycle(&self) -> bool {
        self.five_step_extra_cycle
    }

    #[cfg(test)]
    pub fn debug_pending_write(&self) -> Option<u8> {
        self.pending_write
    }

    #[cfg(test)]
    pub fn debug_write_delay(&self) -> u8 {
        self.write_delay
    }

    #[cfg(test)]
    pub fn debug_pending_write_on_odd_cpu_cycle(&self) -> bool {
        self.pending_write_on_odd_cpu_cycle
    }

    #[cfg(test)]
    pub fn debug_pending_immediate_clock(&self) -> (bool, bool) {
        self.pending_immediate_clock
    }

    #[cfg(test)]
    pub fn debug_irq_assert_cycles_remaining(&self) -> u8 {
        self.irq_assert_cycles_remaining
    }

    #[cfg(test)]
    pub fn debug_five_step_extra_cycle(&self) -> bool {
        self.five_step_extra_cycle
    }

    #[cfg(test)]
    pub fn debug_set_irq_assert_cycles_remaining(&mut self, value: u8) {
        self.irq_assert_cycles_remaining = value;
    }

    #[cfg(test)]
    pub fn debug_set_five_step_extra_cycle(&mut self, value: bool) {
        self.five_step_extra_cycle = value;
    }

    #[cfg(test)]
    pub fn debug_set_pending_immediate_clock(&mut self, quarter: bool, half: bool) {
        self.pending_immediate_clock = (quarter, half);
    }

    /// Get the IRQ flag state
    pub fn get_irq_flag(&self) -> bool {
        self.irq_flag
    }

    /// Get the IRQ inhibit flag state
    pub fn get_irq_inhibit(&self) -> bool {
        self.irq_inhibit
    }

    /// Restore frame counter state from a save-state.
    #[allow(clippy::too_many_arguments)]
    pub fn restore_state(
        &mut self,
        cycle_counter: u32,
        mode: bool,
        irq_inhibit: bool,
        irq_flag: bool,
        irq_assert_cycles_remaining: u8,
        block_frame_counter: bool,
        five_step_extra_cycle: bool,
        pending_write: Option<u8>,
        write_delay: u8,
        pending_write_on_odd_cpu_cycle: bool,
        pending_immediate_clock: (bool, bool),
    ) {
        self.cycle_counter = cycle_counter;
        self.mode = if mode { Mode::FiveStep } else { Mode::FourStep };
        self.irq_inhibit = irq_inhibit;
        self.irq_flag = irq_flag;
        self.irq_assert_cycles_remaining = irq_assert_cycles_remaining;
        self.block_frame_counter = block_frame_counter;
        self.five_step_extra_cycle = five_step_extra_cycle;
        self.pending_write = pending_write;
        self.write_delay = write_delay;
        self.pending_write_on_odd_cpu_cycle = pending_write_on_odd_cpu_cycle;
        self.pending_immediate_clock = pending_immediate_clock;
    }

    /// Clear the IRQ flag
    pub fn clear_irq_flag(&mut self) {
        trace_apu!(2; "frame_counter clear_irq_flag");
        self.irq_flag = false;
    }

    /// Queue a delayed write to $4017 register
    /// This is used for power-on/reset timing where the write takes effect after a delay
    /// delay: number of CPU cycles before the write takes effect (typically 3-4)
    pub fn queue_delayed_write(&mut self, value: u8, delay: u8) {
        trace_apu!(2; "frame_counter queue_delayed_write value=0x{:02X} delay={}", value, delay);
        self.pending_write = Some(value);
        self.write_delay = delay;
        self.pending_write_on_odd_cpu_cycle = false;
    }

    /// Queue a delayed write to $4017 register, preserving whether the write occurred on an odd
    /// CPU cycle so we can apply frame-counter jitter when the write takes effect.
    pub fn queue_delayed_write_with_jitter(
        &mut self,
        value: u8,
        delay: u8,
        write_on_odd_cpu_cycle: bool,
    ) {
        trace_apu!(
            2; "frame_counter queue_delayed_write_with_jitter value=0x{:02X} delay={} odd_cpu_cycle={}",
            value,
            delay,
            write_on_odd_cpu_cycle
        );
        self.pending_write = Some(value);
        self.write_delay = delay;
        self.pending_write_on_odd_cpu_cycle = write_on_odd_cpu_cycle;
    }

    /// Process pending delayed write (called at start of each clock cycle)
    ///
    /// Returns true if a pending write took effect on this cycle.
    fn process_delayed_write(&mut self) -> bool {
        if self.pending_write.is_none() {
            return false;
        }

        // NESDev: Effects of a $4017 write occur 3 or 4 CPU cycles later.
        // Interpret write_delay as "cycles remaining until the write takes effect".
        // So we count down each CPU clock and apply on the clock that reaches 0.
        if self.write_delay > 0 {
            self.write_delay -= 1;
            if self.write_delay > 0 {
                return false;
            }
        }

        let value = self.pending_write.expect("checked above");

        let new_mode = if (value & 0x80) != 0 {
            Mode::FiveStep
        } else {
            Mode::FourStep
        };

        trace_apu!(
            2; "frame_counter delayed_write_effect value=0x{:02X} mode={} irq_inhibit={}",
            value,
            if new_mode == Mode::FiveStep { "5-step" } else { "4-step" },
            (value & 0x40) != 0
        );

        // Apply the delayed write
        self.mode = new_mode;
        self.irq_inhibit = (value & 0x40) != 0;

        // Reset the 5-step alternating offset each time the sequencer is reset.
        self.five_step_extra_cycle = false;
        self.irq_assert_cycles_remaining = 0;

        // Reset cycle_counter to 0.
        // Note: The jitter effect (odd vs even cycle writes) is handled by the 3 vs 4 cycle
        // delay already. We just reset to 0 here.
        self.cycle_counter = 0;

        // Writing 1 to IRQ inhibit clears the IRQ flag
        if (value & 0x40) != 0 {
            self.irq_flag = false;
        }

        // If mode is set (5-step), generate immediate quarter+half clocks at effect time.
        if new_mode == Mode::FiveStep {
            self.pending_immediate_clock = (true, true);
        }

        // Clear the pending write
        self.pending_write = None;
        self.pending_write_on_odd_cpu_cycle = false;

        true
    }

    /// Clock the frame counter by one CPU cycle
    /// Returns (quarter_frame, half_frame) signals
    pub fn clock(&mut self) -> (bool, bool) {
        // Process any pending delayed write before advancing
        let write_took_effect = self.process_delayed_write();

        // Frame counter increments every CPU cycle.
        // Important timing detail: when a delayed $4017 write takes effect on this CPU cycle,
        // the sequencer is reset but does not also immediately advance on the same cycle.
        // This matches blargg's APU timing tests (e.g. apu_test 5-len_timing).
        if !write_took_effect {
            self.cycle_counter = self.cycle_counter.wrapping_add(1);
        }

        let (quarter_frame, half_frame) = match self.mode {
            Mode::FourStep => self.clock_four_step(),
            Mode::FiveStep => self.clock_five_step(),
        };

        let (immediate_quarter, immediate_half) = self.pending_immediate_clock;
        self.pending_immediate_clock = (false, false);

        let block_active = self.block_frame_counter;
        let quarter = if block_active {
            false
        } else {
            quarter_frame || immediate_quarter
        };
        let half = if block_active {
            false
        } else {
            half_frame || immediate_half
        };

        if self.block_frame_counter {
            self.block_frame_counter = false;
        }

        if quarter || half {
            self.block_frame_counter = true;
        }

        if quarter || half {
            trace_apu!(
                3; "frame_counter clock quarter={} half={} cycle={}",
                quarter,
                half,
                self.cycle_counter
            );
        }

        (quarter, half)
    }

    /// Clock the 4-step sequencer
    ///
    /// # IRQ Semantics (blargg compatibility mode)
    ///
    /// The frame IRQ in 4-step mode is implemented as a 3-cycle "asserting signal" that begins
    /// at cycle 29828 (APU cycle 14914). This behavior differs from a strict reading of NESDev's
    /// `apu_ref.txt`, which describes a simple flag that's set once by the sequencer.
    ///
    /// The multi-cycle assertion model was chosen to pass blargg's `apu_test/6-irq_flag_timing`,
    /// which tests that reading $4015 (which clears the IRQ flag) during the asserting window
    /// will cause the flag to be re-set on subsequent cycles.
    ///
    /// If you're debugging IRQ timing issues and need spec-first behavior, the alternative is:
    /// - Set `irq_flag = true` once at IRQ_CYCLE
    /// - Remove the `irq_assert_cycles_remaining` mechanism
    /// - This will likely break blargg test 6
    fn clock_four_step(&mut self) -> (bool, bool) {
        const IRQ_ASSERT_CYCLES: u8 = 3; // How long the internal IRQ signal keeps asserting

        let (step_1, step_2, step_3, step_4, irq_cycle, frame_cycles) = match self.tv_system {
            TimingMode::Ntsc | TimingMode::Dendy => (7457, 14913, 22371, 29829, 29828, 29830),
            TimingMode::Pal => (8313, 16627, 24939, 33253, 33252, 33254),
            TimingMode::MultiRegion | TimingMode::Unknown(_) => {
                (7457, 14913, 22371, 29829, 29828, 29830)
            }
        };

        let quarter_frame = self.cycle_counter == step_1
            || self.cycle_counter == step_2
            || self.cycle_counter == step_3
            || self.cycle_counter == step_4;
        let half_frame = self.cycle_counter == step_2 || self.cycle_counter == step_4;

        // Start the IRQ asserting window at the designated cycle.
        // See doc comment above for why we use a multi-cycle window instead of a one-shot.
        if self.cycle_counter == irq_cycle && !self.irq_inhibit {
            trace_apu!(2; "frame_counter irq_assert start cycle={}", self.cycle_counter);
            self.irq_assert_cycles_remaining = IRQ_ASSERT_CYCLES;
        }

        // While the IRQ signal is asserting, keep (re-)setting the flag each cycle.
        // This allows the flag to be re-set if cleared during the window.
        if self.irq_assert_cycles_remaining > 0 {
            if !self.irq_inhibit {
                self.irq_flag = true;
            }
            self.irq_assert_cycles_remaining -= 1;
        }

        // 4-step sequence length is 29830 CPU cycles.
        if self.cycle_counter >= frame_cycles {
            self.cycle_counter = 0;
        }

        (quarter_frame, half_frame)
    }

    /// Clock the 5-step sequencer
    /// NESDev: 5-step mode clocks at cycles 7457, 14913, 22371, 29829, 37281
    /// Frame types:
    /// - 7457:  QuarterFrame (envelope only)
    /// - 14913: HalfFrame (envelope + length)
    /// - 22371: QuarterFrame (envelope only)
    /// - 29829: None (no clocks)
    /// - 37281: HalfFrame (envelope + length)
    fn clock_five_step(&mut self) -> (bool, bool) {
        let (step_1_base, step_2_base, step_3_base, step_5_base) = match self.tv_system {
            TimingMode::Ntsc | TimingMode::Dendy => (7457, 14913, 22371, 37281),
            TimingMode::Pal => (8313, 16627, 24939, 41565),
            TimingMode::MultiRegion | TimingMode::Unknown(_) => (7457, 14913, 22371, 37281),
        };

        // The 5-step sequence length is odd for both PAL and NTSC , which causes the relative phase to
        // alternate; we model that as a +1 cycle offset every other sequence.
        let offset: u32 = self.five_step_extra_cycle as u32;

        let step_1 = step_1_base + offset;
        let step_2 = step_2_base + offset;
        let step_3 = step_3_base + offset;
        let step_5 = step_5_base + offset;

        // Quarter frame (envelope) clocks at steps 1, 2, 3, and 5 (NOT step 4)
        let quarter_frame = self.cycle_counter == step_1
            || self.cycle_counter == step_2
            || self.cycle_counter == step_3
            || self.cycle_counter == step_5;
        // Half frame (length counter) clocks at steps 2 and 5
        let half_frame = self.cycle_counter == step_2 || self.cycle_counter == step_5;

        // Wrap around after step 5
        if self.cycle_counter >= step_5 {
            self.cycle_counter = 0;
            // Toggle alternating phase
            self.five_step_extra_cycle = !self.five_step_extra_cycle;
        }

        (quarter_frame, half_frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::console::TimingMode;

    #[test]
    fn test_frame_counter_new() {
        let fc = FrameCounter::new();
        assert!(!fc.get_mode()); // Default to 4-step (false)
        assert!(!fc.is_irq_inhibited());
        assert_eq!(fc.get_cycle_counter(), 0);
    }

    #[test]
    fn test_dendy_frame_counter_4step_uses_ntsc_timing() {
        // Dendy APU works with NTSC timings.
        // Spec: Mesen2 NesApu.cpp GetApuRegion() — Dendy routes to ConsoleRegion::Ntsc
        // NTSC 4-step quarter-frame at cycle 7457; PAL at 8313.
        let mut fc = FrameCounter::new_with_tv_system(TimingMode::Dendy);
        fc.write_register(0b0000_0000); // 4-step, no IRQ inhibit

        // Run to cycle 7457: should clock a quarter frame (NTSC step_1)
        for i in 0..7457u32 {
            fc.cycle_counter = i;
            let (quarter, _) = fc.clock_four_step();
            assert!(!quarter, "unexpected quarter frame at cycle {i}");
        }
        fc.cycle_counter = 7457;
        let (quarter_at_7457, _) = fc.clock_four_step();
        assert!(
            quarter_at_7457,
            "Dendy should quarter-frame at NTSC step_1 cycle 7457"
        );
    }

    #[test]
    fn test_dendy_frame_counter_4step_not_pal_timing() {
        // PAL 4-step fires first quarter frame at cycle 8313. Dendy must NOT do this.
        let mut fc = FrameCounter::new_with_tv_system(TimingMode::Dendy);
        fc.write_register(0b0000_0000);
        fc.cycle_counter = 8313;

        let (quarter_at_8313, _) = fc.clock_four_step();
        assert!(
            !quarter_at_8313,
            "Dendy must not quarter-frame at PAL step_1 cycle 8313"
        );

        // Dendy follows NTSC frame timing, so it must not trigger IRQ at PAL's irq cycle.
        let mut fc2 = FrameCounter::new_with_tv_system(TimingMode::Dendy);
        fc2.write_register(0b0000_0000);
        fc2.cycle_counter = 33252; // PAL irq_cycle — must not fire for Dendy
        let (_, _) = fc2.clock_four_step();
        assert!(
            !fc2.get_irq_flag(),
            "Dendy must not fire IRQ at PAL irq_cycle 33252"
        );
    }

    #[test]
    fn test_write_register_4_step_mode() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // Mode = 0 (4-step), IRQ inhibit = 0

        assert!(!fc.get_mode()); // 4-step mode
        assert!(!fc.is_irq_inhibited());
    }

    #[test]
    fn test_write_register_5_step_mode() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1000_0000); // Mode = 1 (5-step), IRQ inhibit = 0

        assert!(fc.get_mode()); // 5-step mode
        assert!(!fc.is_irq_inhibited());
    }

    #[test]
    fn test_write_register_irq_inhibit() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0100_0000); // Mode = 0, IRQ inhibit = 1

        assert!(!fc.get_mode()); // 4-step mode
        assert!(fc.is_irq_inhibited());
    }

    #[test]
    fn test_write_register_both_flags() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1100_0000); // Mode = 1, IRQ inhibit = 1

        assert!(fc.get_mode()); // 5-step mode
        assert!(fc.is_irq_inhibited());
    }

    #[test]
    fn test_write_register_resets_cycle_counter() {
        let mut fc = FrameCounter::new();
        fc.cycle_counter = 12345; // Manually set counter

        fc.write_register(0b0000_0000);

        assert_eq!(fc.get_cycle_counter(), 0);
    }

    #[test]
    fn test_write_register_ignores_lower_bits() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0011_1111); // All lower bits set, mode = 0, IRQ inhibit = 0

        assert!(!fc.get_mode());
        assert!(!fc.is_irq_inhibited());
    }

    #[test]
    fn test_mode_change_from_4_to_5_step() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step
        assert!(!fc.get_mode());

        fc.write_register(0b1000_0000); // 5-step
        assert!(fc.get_mode());
    }

    #[test]
    fn test_delayed_4017_immediate_clock_blocked_after_half_frame_tick() {
        let mut fc = FrameCounter::new();

        // Arrange: place the counter just before a half-frame tick in 4-step mode,
        // and schedule a delayed write to 5-step so it takes effect on the next cycle.
        fc.restore_state(
            29828,
            false,
            false,
            false,
            0,
            false,
            false,
            Some(0x80),
            2,
            false,
            (false, false),
        );

        // First clock: should hit the 4-step half-frame tick at cycle 29829.
        let (quarter_1, half_1) = fc.clock();
        assert!(quarter_1);
        assert!(half_1);

        // Second clock: delayed $4017 write takes effect (mode 5-step).
        // Immediate quarter/half clocks from the write should be blocked for this cycle.
        let (quarter_2, half_2) = fc.clock();
        assert!(!quarter_2);
        assert!(!half_2);
    }

    #[test]
    fn test_mode_change_from_5_to_4_step() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1000_0000); // 5-step
        assert!(fc.get_mode());

        fc.write_register(0b0000_0000); // 4-step
        assert!(!fc.get_mode());
    }

    #[test]
    fn test_write_register_5_step_generates_immediate_clock() {
        let mut fc = FrameCounter::new();

        // Writing to $4017 with bit 7 set (5-step mode) should generate
        // immediate quarter+half frame clocks on the next clock() call.
        fc.write_register(0b1000_0000); // 5-step mode

        // The first clock() after the write should include the immediate clocks
        let (quarter, half) = fc.clock();
        assert!(
            quarter,
            "5-step mode write should generate immediate quarter frame clock"
        );
        assert!(
            half,
            "5-step mode write should generate immediate half frame clock"
        );
    }

    #[test]
    fn test_write_register_4_step_no_immediate_clock() {
        let mut fc = FrameCounter::new();

        // Writing to $4017 with bit 7 clear (4-step mode) should NOT generate
        // immediate quarter+half frame clocks.
        fc.write_register(0b0000_0000); // 4-step mode

        // The first clock() after the write should NOT have immediate clocks
        let (quarter, half) = fc.clock();
        assert!(
            !quarter,
            "4-step mode write should not generate immediate quarter frame clock"
        );
        assert!(
            !half,
            "4-step mode write should not generate immediate half frame clock"
        );
    }

    #[test]
    fn test_irq_inhibit_can_be_toggled() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0100_0000); // IRQ inhibit = 1
        assert!(fc.is_irq_inhibited());

        fc.write_register(0b0000_0000); // IRQ inhibit = 0
        assert!(!fc.is_irq_inhibited());

        fc.write_register(0b0100_0000); // IRQ inhibit = 1
        assert!(fc.is_irq_inhibited());
    }

    // 4-Step Sequencer Tests
    #[test]
    fn test_four_step_cycle_counter_increments() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        assert_eq!(fc.get_cycle_counter(), 0);
        fc.clock();
        assert_eq!(fc.get_cycle_counter(), 1);
        fc.clock();
        assert_eq!(fc.get_cycle_counter(), 2);
    }

    #[test]
    fn test_four_step_step_1_signals() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        // Clock up to step 1 (7457 cycles)
        for _ in 0..7456 {
            let (quarter, half) = fc.clock();
            assert!(!quarter);
            assert!(!half);
        }

        // At cycle 7457, quarter frame signal
        let (quarter, half) = fc.clock();
        assert!(quarter);
        assert!(!half);
    }

    #[test]
    fn test_pal_four_step_step_1_signals() {
        let mut fc = FrameCounter::new_with_tv_system(TimingMode::Pal);
        fc.write_register(0b0000_0000); // 4-step mode

        // Clock up to PAL step 1 (8313 cycles)
        for _ in 0..8312 {
            let (quarter, half) = fc.clock();
            assert!(!quarter);
            assert!(!half);
        }

        let (quarter, half) = fc.clock();
        assert!(quarter);
        assert!(!half);
    }

    #[test]
    fn test_four_step_step_2_signals() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        // Clock up to step 2 (14913 cycles)
        for _ in 0..14912 {
            fc.clock();
        }

        // At cycle 14913, quarter and half frame signals
        let (quarter, half) = fc.clock();
        assert!(quarter);
        assert!(half);
    }

    #[test]
    fn test_four_step_step_3_signals() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        // Clock up to step 3 (22371 cycles)
        for _ in 0..22370 {
            fc.clock();
        }

        // At cycle 22371, quarter frame signal
        let (quarter, half) = fc.clock();
        assert!(quarter);
        assert!(!half);
    }

    #[test]
    fn test_four_step_step_4_signals() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        // Clock up to step 4 (29829 cycles)
        for _ in 0..29828 {
            fc.clock();
        }

        // At cycle 29829, quarter and half frame signals
        let (quarter, half) = fc.clock();
        assert!(quarter);
        assert!(half);
    }

    #[test]
    fn test_four_step_wraparound() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        // Clock through full sequence (29830 cycles)
        for _ in 0..29830 {
            fc.clock();
        }

        // Counter should have wrapped to 0
        assert_eq!(fc.get_cycle_counter(), 0);

        // Next clock should be at cycle 1
        fc.clock();
        assert_eq!(fc.get_cycle_counter(), 1);
    }

    #[test]
    fn test_pal_four_step_wraparound() {
        let mut fc = FrameCounter::new_with_tv_system(TimingMode::Pal);
        fc.write_register(0b0000_0000); // 4-step mode

        // Clock through full PAL sequence (33254 cycles)
        for _ in 0..33254 {
            fc.clock();
        }

        assert_eq!(fc.get_cycle_counter(), 0);
        fc.clock();
        assert_eq!(fc.get_cycle_counter(), 1);
    }

    #[test]
    fn test_four_step_wraparound_at_29830_and_irq_sticks_until_cleared() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode, IRQ enabled

        // On NTSC hardware, the 4-step sequencer frame is 29830 CPU cycles long.
        // The frame IRQ flag is set at the end of the sequence and remains set until cleared.
        for _ in 0..29830 {
            fc.clock();
        }

        assert_eq!(fc.get_cycle_counter(), 0, "Should wrap at 29830 cycles");
        assert!(fc.get_irq_flag(), "Frame IRQ flag should be set by wrap");

        fc.clear_irq_flag();
        assert!(
            !fc.get_irq_flag(),
            "IRQ flag should clear via explicit clear"
        );
    }

    #[test]
    fn test_four_step_complete_sequence() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        let mut quarter_count = 0;
        let mut half_count = 0;

        // Run through one complete sequence (29830 cycles)
        for _ in 0..29830 {
            let (quarter, half) = fc.clock();
            if quarter {
                quarter_count += 1;
            }
            if half {
                half_count += 1;
            }
        }

        assert_eq!(quarter_count, 4); // 4 quarter frame clocks
        assert_eq!(half_count, 2); // 2 half frame clocks
        assert_eq!(fc.get_cycle_counter(), 0); // Wrapped around after end-of-frame
    }

    #[test]
    fn test_four_step_no_signals_between_steps() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        // Clock past step 1 (7457)
        for _ in 0..7457 {
            fc.clock();
        }

        // Check no signals between step 1 and step 2
        for _ in 0..100 {
            let (quarter, half) = fc.clock();
            assert!(!quarter);
            assert!(!half);
        }
    }

    #[test]
    fn test_four_step_multiple_sequences() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        // Run two complete sequences (29830 cycles each)
        for sequence in 0..2 {
            let mut quarter_count = 0;
            let mut half_count = 0;

            for _ in 0..29830 {
                let (quarter, half) = fc.clock();
                if quarter {
                    quarter_count += 1;
                }
                if half {
                    half_count += 1;
                }
            }

            assert_eq!(quarter_count, 4, "Sequence {}", sequence);
            assert_eq!(half_count, 2, "Sequence {}", sequence);
            assert_eq!(fc.get_cycle_counter(), 0, "Sequence {}", sequence);
        }
    }

    // 5-Step Sequencer Tests
    #[test]
    fn test_five_step_step_1_signals() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1000_0000); // 5-step mode

        // The first clock after setting 5-step mode generates immediate quarter+half
        // (per NESDev: "Writing to $4017 with bit 7 set will immediately generate a clock")
        let (quarter, half) = fc.clock();
        assert!(
            quarter,
            "5-step mode should generate immediate quarter frame"
        );
        assert!(half, "5-step mode should generate immediate half frame");

        // After the immediate clock, no more signals until step 1 (7457 cycles)
        // We're at cycle 1 now, need 7456 more clocks to reach 7457
        for _ in 1..7456 {
            let (quarter, half) = fc.clock();
            assert!(!quarter);
            assert!(!half);
        }

        // At cycle 7457, quarter frame signal
        let (quarter, half) = fc.clock();
        assert!(quarter);
        assert!(!half);
    }

    #[test]
    fn test_five_step_step_2_signals() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1000_0000); // 5-step mode

        // Clock up to step 2 (14913 cycles)
        for _ in 0..14912 {
            fc.clock();
        }

        // At cycle 14913, quarter and half frame signals
        let (quarter, half) = fc.clock();
        assert!(quarter);
        assert!(half);
    }

    #[test]
    fn test_five_step_step_3_signals() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1000_0000); // 5-step mode

        // Clock up to step 3 (22371 cycles)
        for _ in 0..22370 {
            fc.clock();
        }

        // At cycle 22371, quarter frame signal
        let (quarter, half) = fc.clock();
        assert!(quarter);
        assert!(!half);
    }

    #[test]
    fn test_five_step_step_4_signals() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1000_0000); // 5-step mode

        // Clock up to step 4 (29829 cycles)
        for _ in 0..29828 {
            fc.clock();
        }

        // At cycle 29829, NO clocks at all
        let (quarter, half) = fc.clock();
        assert!(!quarter);
        assert!(!half);
    }

    #[test]
    fn test_five_step_step_5_signals() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1000_0000); // 5-step mode

        // Clock up to step 5 (37281 cycles)
        for _ in 0..37280 {
            fc.clock();
        }

        // At cycle 37281, BOTH quarter and half frame signals
        let (quarter, half) = fc.clock();
        assert!(quarter);
        assert!(half);
    }

    #[test]
    fn test_five_step_wraparound() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1000_0000); // 5-step mode

        // Clock to step 5 (37281 cycles)
        for _ in 0..37281 {
            fc.clock();
        }

        // Counter should have wrapped to 0
        assert_eq!(fc.get_cycle_counter(), 0);

        // Next clock should be at cycle 1
        fc.clock();
        assert_eq!(fc.get_cycle_counter(), 1);
    }

    #[test]
    fn test_five_step_complete_sequence() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1000_0000); // 5-step mode

        let mut quarter_count = 0;
        let mut half_count = 0;

        // Run through one complete sequence
        for _ in 0..37281 {
            let (quarter, half) = fc.clock();
            if quarter {
                quarter_count += 1;
            }
            if half {
                half_count += 1;
            }
        }

        // Per NESDev: Writing to $4017 with bit 7 set generates immediate quarter+half clocks.
        // Then the regular sequence has 4 quarter frames and 2 half frames.
        // Total: 5 quarter frames (1 immediate + 4 regular), 3 half frames (1 immediate + 2 regular)
        assert_eq!(quarter_count, 5);
        assert_eq!(half_count, 3);
        assert_eq!(fc.get_cycle_counter(), 0); // Wrapped around
    }

    // IRQ Generation Tests
    #[test]
    fn test_irq_flag_set_at_step_4_in_4_step_mode() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode, IRQ not inhibited

        // Clock to IRQ assertion cycle (29828)
        for _ in 0..29828 {
            fc.clock();
        }

        // IRQ flag should be set
        assert!(fc.get_irq_flag());
    }

    #[test]
    fn test_irq_flag_not_set_when_inhibited() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0100_0000); // 4-step mode, IRQ inhibited

        // Clock to IRQ cycle
        for _ in 0..29828 {
            fc.clock();
        }

        // IRQ flag should NOT be set (inhibited)
        assert!(!fc.get_irq_flag());
    }

    #[test]
    fn test_irq_flag_not_set_in_5_step_mode() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b1000_0000); // 5-step mode, IRQ not inhibited

        // Clock through entire 5-step sequence
        for _ in 0..37281 {
            fc.clock();
        }

        // IRQ flag should NOT be set (5-step mode never generates IRQ)
        assert!(!fc.get_irq_flag());
    }

    #[test]
    fn test_irq_flag_cleared_by_clear_method() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        // Set IRQ flag
        for _ in 0..29828 {
            fc.clock();
        }
        assert!(fc.get_irq_flag());

        // Clear it
        fc.clear_irq_flag();
        assert!(!fc.get_irq_flag());
    }

    #[test]
    fn test_irq_flag_cleared_when_setting_inhibit_bit() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode, IRQ not inhibited

        // Set IRQ flag
        for _ in 0..29828 {
            fc.clock();
        }
        assert!(fc.get_irq_flag());

        // Write with inhibit bit set - should clear IRQ
        fc.write_register(0b0100_0000);
        assert!(!fc.get_irq_flag());
    }

    #[test]
    fn test_irq_flag_not_cleared_when_inhibit_already_set() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0100_0000); // Already inhibited

        // Manually set IRQ flag for testing
        fc.irq_flag = true;
        assert!(fc.get_irq_flag());

        // Write with inhibit bit still set - IRQ should be cleared
        fc.write_register(0b0100_0000);
        assert!(!fc.get_irq_flag());
    }

    #[test]
    fn test_irq_flag_persists_across_multiple_cycles() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        // Set IRQ flag
        for _ in 0..29828 {
            fc.clock();
        }
        assert!(fc.get_irq_flag());

        // Clock a few more times (wraps around)
        for _ in 0..100 {
            fc.clock();
        }

        // IRQ flag should still be set
        assert!(fc.get_irq_flag());
    }

    #[test]
    fn test_irq_flag_set_again_on_next_sequence() {
        let mut fc = FrameCounter::new();
        fc.write_register(0b0000_0000); // 4-step mode

        // First sequence - clock to IRQ cycle
        for _ in 0..29828 {
            fc.clock();
        }
        assert!(fc.get_irq_flag());

        // Clear IRQ
        fc.clear_irq_flag();
        assert!(!fc.get_irq_flag());

        // Second sequence - clock through end-of-frame (29830) and back to IRQ cycle (29828)
        // Remaining to frame end: 29830 - 29828 = 2
        // Then another 29828 cycles to reach IRQ again
        for _ in 0..(2 + 29828) {
            fc.clock();
        }
        assert!(fc.get_irq_flag());
    }

    #[test]
    fn reset_restores_frame_counter_to_initial_state() {
        let mut fc = FrameCounter::new();
        // Modify state: set 5-step mode with IRQ inhibit
        fc.write_register(0b1100_0000);
        // Clock to build up state
        for _ in 0..1000 {
            fc.clock();
        }
        // Queue a delayed write
        fc.queue_delayed_write(0x80, 3);

        // Verify state changed
        assert!(fc.get_mode()); // 5-step mode
        assert!(fc.is_irq_inhibited());
        assert!(fc.get_cycle_counter() > 0);

        // Reset
        fc.reset();

        // Verify all fields back to default
        assert!(!fc.get_mode()); // 4-step mode
        assert!(!fc.is_irq_inhibited());
        assert_eq!(fc.get_cycle_counter(), 0);
        assert!(!fc.get_irq_flag());
    }
}
