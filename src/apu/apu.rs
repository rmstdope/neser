use super::dmc::Dmc;
use super::frame_counter::FrameCounter;
use super::noise::Noise;
use super::pulse::Pulse;
use super::triangle::Triangle;

// Status register ($4015) bit masks
const STATUS_PULSE1: u8 = 1 << 0;
const STATUS_PULSE2: u8 = 1 << 1;
const STATUS_TRIANGLE: u8 = 1 << 2;
const STATUS_NOISE: u8 = 1 << 3;
const STATUS_DMC: u8 = 1 << 4;
const STATUS_FRAME_IRQ: u8 = 1 << 6;
const STATUS_DMC_IRQ: u8 = 1 << 7;

/// Main APU module integrating frame counter and sound channels
pub struct Apu {
    frame_counter: FrameCounter,
    pulse1: Pulse,
    pulse2: Pulse,
    triangle: Triangle,
    noise: Noise,
    dmc: Dmc,
}

impl Apu {
    /// Create a new APU
    pub fn new() -> Self {
        Self {
            frame_counter: FrameCounter::new(),
            pulse1: Pulse::new(true),  // Pulse 1 uses ones' complement
            pulse2: Pulse::new(false), // Pulse 2 uses two's complement
            triangle: Triangle::new(),
            noise: Noise::new(),
            dmc: Dmc::new(),
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

    /// Get reference to noise channel
    pub fn noise(&self) -> &Noise {
        &self.noise
    }

    /// Get mutable reference to noise channel
    pub fn noise_mut(&mut self) -> &mut Noise {
        &mut self.noise
    }

    /// Get reference to DMC channel
    pub fn dmc(&self) -> &Dmc {
        &self.dmc
    }

    /// Get mutable reference to DMC channel
    pub fn dmc_mut(&mut self) -> &mut Dmc {
        &mut self.dmc
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
            self.noise.clock_envelope();
        }

        // Half frame: clock length counters and sweep units
        if half_frame {
            self.pulse1.clock_length_counter();
            self.pulse1.clock_sweep();
            self.pulse2.clock_length_counter();
            self.pulse2.clock_sweep();
            self.triangle.clock_length_counter();
            self.noise.clock_length_counter();
        }

        // DMC timer runs every CPU cycle (independent of frame counter)
        self.dmc.clock_timer();
    }

    /// Read the APU status register ($4015)
    /// Returns: IF-D NT21
    /// - Bit 7 (I): DMC interrupt flag
    /// - Bit 6 (F): Frame counter interrupt flag
    /// - Bit 5: Open bus (not implemented, returns 0)
    /// - Bit 4 (D): DMC active (bytes remaining > 0)
    /// - Bit 3 (N): Noise length counter > 0
    /// - Bit 2 (T): Triangle length counter > 0
    /// - Bit 1 (2): Pulse 2 length counter > 0
    /// - Bit 0 (1): Pulse 1 length counter > 0
    ///
    /// Side effect: Clears the frame counter interrupt flag
    pub fn read_status(&mut self) -> u8 {
        let mut status = 0;

        if self.pulse1.get_length_counter() > 0 {
            status |= STATUS_PULSE1;
        }
        if self.pulse2.get_length_counter() > 0 {
            status |= STATUS_PULSE2;
        }
        if self.triangle.get_length_counter() > 0 {
            status |= STATUS_TRIANGLE;
        }
        if self.noise.get_length_counter() > 0 {
            status |= STATUS_NOISE;
        }
        if self.dmc.has_bytes_remaining() {
            status |= STATUS_DMC;
        }
        if self.frame_counter.get_irq_flag() {
            status |= STATUS_FRAME_IRQ;
        }
        if self.dmc.get_irq_flag() {
            status |= STATUS_DMC_IRQ;
        }

        // Side effect: Clear frame counter interrupt flag
        self.frame_counter.clear_irq_flag();

        status
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
        assert_eq!(apu.noise().output(), 0); // Noise is muted with zero length counter
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

    #[test]
    fn test_noise_channel_integrated() {
        let apu = Apu::new();
        assert_eq!(apu.noise().output(), 0); // Noise starts muted (length counter = 0)
    }

    #[test]
    fn test_noise_envelope_gets_clocked() {
        let mut apu = Apu::new();

        // Set up noise with envelope that will be clocked
        apu.noise_mut().write_envelope(0b0000_0101); // Volume 5, constant volume
        apu.noise_mut().write_length(0xFF); // Set length and envelope start flag

        // Clock to first quarter frame
        for _ in 0..7457 {
            apu.clock();
        }

        // Envelope should have been clocked (integration works - verified by no panic)
        // Note: output() may still be 0 if shift_register bit 0 is set (which mutes output)
    }

    #[test]
    fn test_noise_length_counter_gets_clocked() {
        let mut apu = Apu::new();

        // Set up noise with length counter (index 2 = length 20)
        apu.noise_mut().write_envelope(0b0000_0000); // halt=0
        apu.noise_mut().write_length(0b00010_000); // Index 2

        // Length counter should be loaded
        // Output will be 0 because shift register bit 0 might be set

        // Clock to first half frame (14913 cycles)
        for _ in 0..14913 {
            apu.clock();
        }

        // Length counter should have decremented (can't directly check but it affects output)
        // This test verifies the integration works without panicking
    }

    #[test]
    fn test_dmc_channel_accessible() {
        let apu = Apu::new();
        // Should be able to access DMC channel
        assert_eq!(apu.dmc().output(), 0);
    }

    #[test]
    fn test_dmc_channel_mutable() {
        let mut apu = Apu::new();
        // Should be able to mutably access DMC channel
        apu.dmc_mut().write_direct_load(0b0100_0000); // Set output to 64
        assert_eq!(apu.dmc().output(), 64);
    }

    #[test]
    fn test_dmc_timer_gets_clocked() {
        let mut apu = Apu::new();

        // Set up DMC with fastest rate (rate index 0 = period 428)
        apu.dmc_mut().write_flags_and_rate(0b0000_0000); // Rate 0
        apu.dmc_mut().write_direct_load(0b0000_0000); // Output = 0

        // Clock less than one period
        for _ in 0..427 {
            apu.clock();
        }

        // Timer should not have triggered yet
        // (We can't directly check timer state, but we verify no crash)

        // Clock one more to complete the period
        apu.clock();

        // Timer should have clocked (verified by no panic)
        // Note: Without sample data, DMC won't change output
    }

    #[test]
    fn test_status_all_channels_inactive() {
        let mut apu = Apu::new();
        // All channels start with length counter = 0
        // Bits: IF-D NT21
        // Expected: 0b0000_0000 (all inactive)
        assert_eq!(apu.read_status(), 0b0000_0000);
    }

    #[test]
    fn test_status_pulse1_active() {
        let mut apu = Apu::new();
        // Load length counter for pulse 1
        apu.pulse1_mut()
            .write_length_counter_timer_high(0b00001_000); // Index 1 = length 254
        // Bit 0 should be set
        assert_eq!(apu.read_status() & 0b0000_0001, 0b0000_0001);
    }

    #[test]
    fn test_status_pulse2_active() {
        let mut apu = Apu::new();
        // Load length counter for pulse 2
        apu.pulse2_mut()
            .write_length_counter_timer_high(0b00001_000); // Index 1 = length 254
        // Bit 1 should be set
        assert_eq!(apu.read_status() & 0b0000_0010, 0b0000_0010);
    }

    #[test]
    fn test_status_triangle_active() {
        let mut apu = Apu::new();
        // Load length counter for triangle
        apu.triangle_mut().load_length_counter(1); // Index 1 = length 254
        // Bit 2 should be set
        assert_eq!(apu.read_status() & 0b0000_0100, 0b0000_0100);
    }

    #[test]
    fn test_status_noise_active() {
        let mut apu = Apu::new();
        // Load length counter for noise (index 1 = length 254)
        apu.noise_mut().write_length(0b00001_000);
        // Bit 3 should be set
        assert_eq!(apu.read_status() & 0b0000_1000, 0b0000_1000);
    }

    #[test]
    fn test_status_all_channels_active() {
        let mut apu = Apu::new();
        // Load length counters for all channels
        apu.pulse1_mut()
            .write_length_counter_timer_high(0b00001_000);
        apu.pulse2_mut()
            .write_length_counter_timer_high(0b00001_000);
        apu.triangle_mut().load_length_counter(1);
        apu.noise_mut().write_length(0b00001_000);
        // Bits 0-3 should be set (no DMC, no interrupts yet)
        assert_eq!(apu.read_status() & 0b0000_1111, 0b0000_1111);
    }
}
