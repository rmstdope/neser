use super::frame_counter::FrameCounter;
use super::pulse::Pulse;
use super::triangle::Triangle;

/// Main APU module integrating frame counter and sound channels
pub struct Apu {
    frame_counter: FrameCounter,
    pulse1: Pulse,
    pulse2: Pulse,
    triangle: Triangle,
}

impl Apu {
    /// Create a new APU
    pub fn new() -> Self {
        Self {
            frame_counter: FrameCounter::new(),
            pulse1: Pulse::new(true),  // Pulse 1 uses ones' complement
            pulse2: Pulse::new(false), // Pulse 2 uses two's complement
            triangle: Triangle::new(),
        }
    }

    /// Get reference to pulse channel 1
    pub fn pulse1(&self) -> &Pulse {
        &self.pulse1
    }

    /// Get mutable reference to pulse channel 1
    pub fn pulse1_mut(&mut self) -> &mut Pulse {
        &mut self.pulse1
    }

    /// Get reference to pulse channel 2
    pub fn pulse2(&self) -> &Pulse {
        &self.pulse2
    }

    /// Get mutable reference to pulse channel 2
    pub fn pulse2_mut(&mut self) -> &mut Pulse {
        &mut self.pulse2
    }

    /// Get reference to frame counter
    pub fn frame_counter(&self) -> &FrameCounter {
        &self.frame_counter
    }

    /// Get mutable reference to frame counter
    pub fn frame_counter_mut(&mut self) -> &mut FrameCounter {
        &mut self.frame_counter
    }

    /// Get reference to triangle channel
    pub fn triangle(&self) -> &Triangle {
        &self.triangle
    }

    /// Get mutable reference to triangle channel
    pub fn triangle_mut(&mut self) -> &mut Triangle {
        &mut self.triangle
    }

    /// Clock the APU by one CPU cycle
    /// This advances the frame counter and triggers channel clocking when needed
    pub fn clock(&mut self) {
        let (quarter_frame, half_frame) = self.frame_counter.clock();

        // Quarter frame: clock envelopes and linear counter
        if quarter_frame {
            self.pulse1.clock_envelope();
            self.pulse2.clock_envelope();
            self.triangle.clock_linear_counter_with_reload();
        }

        // Half frame: clock length counters and sweep units
        if half_frame {
            self.pulse1.clock_length_counter();
            self.pulse1.clock_sweep();
            self.pulse2.clock_length_counter();
            self.pulse2.clock_sweep();
            self.triangle.clock_length_counter();
        }
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apu_new() {
        let apu = Apu::new();
        assert_eq!(apu.frame_counter().get_cycle_counter(), 0);
        assert_eq!(apu.pulse1().output(), 0);
        assert_eq!(apu.pulse2().output(), 0);
        assert_eq!(apu.triangle().output(), 0); // Triangle is muted with zero counters
    }

    #[test]
    fn test_frame_counter_advances() {
        let mut apu = Apu::new();
        assert_eq!(apu.frame_counter().get_cycle_counter(), 0);

        apu.clock();
        assert_eq!(apu.frame_counter().get_cycle_counter(), 1);

        for _ in 0..100 {
            apu.clock();
        }
        assert_eq!(apu.frame_counter().get_cycle_counter(), 101);
    }

    #[test]
    fn test_envelope_gets_clocked() {
        let mut apu = Apu::new();

        // Set up pulse with envelope that will be clocked
        apu.pulse1_mut().write_control(0b0000_0000); // Envelope period 0
        apu.pulse1_mut().write_length_counter_timer_high(0xFF); // Set start flag

        // Envelope start flag should be set
        assert!(apu.pulse1().get_envelope_start_flag());

        // Clock to first quarter frame
        for _ in 0..7457 {
            apu.clock();
        }

        // Envelope should have been clocked (start flag consumed)
        assert!(!apu.pulse1().get_envelope_start_flag());
    }

    #[test]
    fn test_length_counter_gets_clocked() {
        let mut apu = Apu::new();

        // Set up pulse with length counter = 1
        apu.pulse1_mut().write_control(0b0000_0000); // halt=0
        apu.pulse1_mut()
            .write_length_counter_timer_high(0b00010_000); // Index 2 = length 20

        let initial_length = apu.pulse1().get_length_counter();
        assert_eq!(initial_length, 20);

        // Clock to first half frame (14913 cycles)
        for _ in 0..14913 {
            apu.clock();
        }

        // Length counter should have decremented
        assert_eq!(apu.pulse1().get_length_counter(), 19);
    }

    #[test]
    fn test_sweep_gets_clocked() {
        let mut apu = Apu::new();

        // Set up pulse with sweep reload flag
        apu.pulse1_mut().write_sweep(0b1000_0001); // Sets sweep_reload = true

        assert!(apu.pulse1().get_sweep_reload());

        // Clock to first half frame
        for _ in 0..14913 {
            apu.clock();
        }

        // Sweep should have been clocked (reload flag consumed)
        assert!(!apu.pulse1().get_sweep_reload());
    }

    #[test]
    fn test_frame_counter_mode_change() {
        let mut apu = Apu::new();

        // Start in 4-step mode (default)
        assert!(!apu.frame_counter().get_mode());

        // Switch to 5-step mode
        apu.frame_counter_mut().write_register(0b1000_0000);
        assert!(apu.frame_counter().get_mode());

        // Switch back to 4-step mode
        apu.frame_counter_mut().write_register(0b0000_0000);
        assert!(!apu.frame_counter().get_mode());
    }

    #[test]
    fn test_both_pulse_channels_get_clocked() {
        let mut apu = Apu::new();

        // Set up both pulses
        apu.pulse1_mut().write_length_counter_timer_high(0xFF);
        apu.pulse2_mut().write_length_counter_timer_high(0xFF);

        assert!(apu.pulse1().get_envelope_start_flag());
        assert!(apu.pulse2().get_envelope_start_flag());

        // Clock to first quarter frame
        for _ in 0..7457 {
            apu.clock();
        }

        // Both envelopes should have been clocked
        assert!(!apu.pulse1().get_envelope_start_flag());
        assert!(!apu.pulse2().get_envelope_start_flag());
    }

    #[test]
    fn test_pulse1_uses_ones_complement_for_sweep() {
        let mut apu = Apu::new();

        // Set up pulse 1 with period = 20, shift = 1, negate enabled
        apu.pulse1_mut().write_timer_low(20);
        apu.pulse1_mut().write_timer_high(0);
        apu.pulse1_mut().write_sweep(0b1000_1001); // Enable=1, period=0, negate=1, shift=1

        // Target period calculation for Pulse 1 (ones' complement):
        // change = 20 >> 1 = 10
        // ones' complement: -10 - 1 = -11
        // target = 20 + (-11) = 9
        assert_eq!(apu.pulse1().get_sweep_target_period(), 9);
    }

    #[test]
    fn test_pulse2_uses_twos_complement_for_sweep() {
        let mut apu = Apu::new();

        // Set up pulse 2 with period = 20, shift = 1, negate enabled
        apu.pulse2_mut().write_timer_low(20);
        apu.pulse2_mut().write_timer_high(0);
        apu.pulse2_mut().write_sweep(0b1000_1001); // Enable=1, period=0, negate=1, shift=1

        // Target period calculation for Pulse 2 (two's complement):
        // change = 20 >> 1 = 10
        // two's complement: -10
        // target = 20 + (-10) = 10
        assert_eq!(apu.pulse2().get_sweep_target_period(), 10);
    }

    #[test]
    fn test_triangle_linear_counter_gets_clocked() {
        let mut apu = Apu::new();

        // Set up triangle with a linear counter reload value
        apu.triangle_mut().write_linear_counter(0x7F); // Max reload value (127), control flag off
        apu.triangle_mut().write_length_counter_timer_high(0x08); // Sets reload flag

        // Check initial state after setting reload flag
        assert!(apu.triangle_mut().is_linear_counter_reload_flag_set());

        // Clock to first quarter frame (7457 cycles in 4-step mode)
        for _ in 0..7457 {
            apu.clock();
        }

        // Linear counter should have been reloaded to 127
        assert_eq!(apu.triangle().get_linear_counter(), 127);
        // Reload flag should be cleared (control flag is off)
        assert!(!apu.triangle().is_linear_counter_reload_flag_set());

        // Clock to next quarter frame
        for _ in 0..7456 {
            apu.clock();
        }

        // Linear counter should have decremented
        assert_eq!(apu.triangle().get_linear_counter(), 126);
    }

    #[test]
    fn test_triangle_length_counter_gets_clocked() {
        let mut apu = Apu::new();

        // Load length counter (index 5 = value 4)
        apu.triangle_mut().load_length_counter(5);
        assert_eq!(apu.triangle().get_length_counter(), 4);

        // Clock to first half frame (14913 cycles in 4-step mode)
        for _ in 0..14913 {
            apu.clock();
        }

        // Length counter should have decremented
        assert_eq!(apu.triangle().get_length_counter(), 3);
    }
}
