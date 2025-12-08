pub mod pulse;
pub mod frame_counter;

use pulse::Pulse;
use frame_counter::FrameCounter;

/// Main APU module integrating frame counter and sound channels
pub struct Apu {
    frame_counter: FrameCounter,
    pulse1: Pulse,
    pulse2: Pulse,
}

impl Apu {
    /// Create a new APU
    pub fn new() -> Self {
        Self {
            frame_counter: FrameCounter::new(),
            pulse1: Pulse::new(),
            pulse2: Pulse::new(),
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

    /// Clock the APU by one CPU cycle
    /// This advances the frame counter and triggers channel clocking when needed
    pub fn clock(&mut self) {
        let (quarter_frame, half_frame) = self.frame_counter.clock();

        // Quarter frame: clock envelopes
        if quarter_frame {
            self.pulse1.clock_envelope();
            self.pulse2.clock_envelope();
        }

        // Half frame: clock length counters and sweep units
        if half_frame {
            self.pulse1.clock_length_counter();
            self.pulse1.clock_sweep();
            self.pulse2.clock_length_counter();
            self.pulse2.clock_sweep();
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
        apu.pulse1_mut().write_length_counter_timer_high(0b00010_000); // Index 2 = length 20
        
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
}
