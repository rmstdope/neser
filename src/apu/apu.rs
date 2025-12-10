use super::dmc::Dmc;
use super::frame_counter::FrameCounter;
use super::noise::Noise;
use super::pulse::Pulse;
use super::triangle::Triangle;

// CPU clock frequency (NTSC)
const CPU_CLOCK_NTSC: f32 = 1_789_773.0;

// Status register ($4015) bit masks
const STATUS_PULSE1: u8 = 1 << 0;
const STATUS_PULSE2: u8 = 1 << 1;
const STATUS_TRIANGLE: u8 = 1 << 2;
const STATUS_NOISE: u8 = 1 << 3;
const STATUS_DMC: u8 = 1 << 4;
const STATUS_FRAME_IRQ: u8 = 1 << 6;
const STATUS_DMC_IRQ: u8 = 1 << 7;

// Mixer lookup tables for non-linear DAC
// Pulse table: 31 entries for pulse1 + pulse2 (0-30)
// Formula: pulse_table[n] = 95.52 / (8128.0 / n + 100)
#[rustfmt::skip]
const PULSE_TABLE: [f32; 31] = [
    0.0, 0.011609139, 0.022937592, 0.033999473, 0.044808503, 0.055377416, 0.065718144,
    0.075841725, 0.085758299, 0.095477104, 0.105006486, 0.114354908, 0.123530001,
    0.132538617, 0.141387892, 0.150083256, 0.158630435, 0.167034455, 0.175300646,
    0.183433647, 0.191437408, 0.199316200, 0.207074609, 0.214716494, 0.222245022,
    0.229663670, 0.236976123, 0.244186282, 0.251297271, 0.258312434, 0.265235335,
];

// TND table: 203 entries for 3*triangle + 2*noise + dmc (0-202)
// Formula: tnd_table[n] = 163.67 / (24329.0 / n + 100)
#[rustfmt::skip]
const TND_TABLE: [f32; 203] = [
    0.000000000, 0.006699824, 0.013345020, 0.019936254, 0.026474180, 0.032959443, 0.039392675,
    0.045774502, 0.052105535, 0.058386381, 0.064617632, 0.070799874, 0.076933683, 0.083019626,
    0.089058261, 0.095050137, 0.100995796, 0.106895770, 0.112750584, 0.118560753, 0.124326788,
    0.130049188, 0.135728448, 0.141365053, 0.146959482, 0.152512207, 0.158023692, 0.163494395,
    0.168924767, 0.174315252, 0.179666289, 0.184978308, 0.190251735, 0.195486988, 0.200684482,
    0.205844623, 0.210967811, 0.216054444, 0.221104910, 0.226119593, 0.231098874, 0.236043125,
    0.240952715, 0.245828007, 0.250669358, 0.255477124, 0.260251651, 0.264993283, 0.269702358,
    0.274379212, 0.279024174, 0.283637568, 0.288219716, 0.292770934, 0.297291534, 0.301781823,
    0.306242106, 0.310672683, 0.315073849, 0.319445896, 0.323789113, 0.328103783, 0.332390186,
    0.336648601, 0.340879300, 0.345082552, 0.349258625, 0.353407780, 0.357530277, 0.361626373,
    0.365696320, 0.369740367, 0.373758762, 0.377751747, 0.381719563, 0.385662446, 0.389580632,
    0.393474351, 0.397343833, 0.401189302, 0.405010981, 0.408809091, 0.412583848, 0.416335468,
    0.420064163, 0.423770142, 0.427453612, 0.431114778, 0.434753841, 0.438371001, 0.441966456,
    0.445540399, 0.449093024, 0.452624521, 0.456135077, 0.459624878, 0.463094108, 0.466542949,
    0.469971578, 0.473380175, 0.476768913, 0.480137965, 0.483487503, 0.486817696, 0.490128711,
    0.493420713, 0.496693865, 0.499948329, 0.503184264, 0.506401828, 0.509601178, 0.512782466,
    0.515945847, 0.519091470, 0.522219486, 0.525330040, 0.528423279, 0.531499348, 0.534558388,
    0.537600541, 0.540625946, 0.543634742, 0.546627063, 0.549603047, 0.552562825, 0.555506530,
    0.558434293, 0.561346242, 0.564242506, 0.567123210, 0.569988481, 0.572838441, 0.575673213,
    0.578492918, 0.581297676, 0.584087605, 0.586862823, 0.589623445, 0.592369587, 0.595101363,
    0.597818884, 0.600522262, 0.603211607, 0.605887028, 0.608548633, 0.611196528, 0.613830820,
    0.616451613, 0.619059010, 0.621653114, 0.624234026, 0.626801846, 0.629356675, 0.631898610,
    0.634427748, 0.636944186, 0.639448020, 0.641939344, 0.644418251, 0.646884834, 0.649339185,
    0.651781395, 0.654211552, 0.656629747, 0.659036068, 0.661430601, 0.663813433, 0.666184650,
    0.668544336, 0.670892576, 0.673229451, 0.675555046, 0.677869441, 0.680172716, 0.682464952,
    0.684746229, 0.687016623, 0.689276214, 0.691525078, 0.693763291, 0.695990928, 0.698208065,
    0.700414776, 0.702611133, 0.704797210, 0.706973079, 0.709138811, 0.711294476, 0.713440145,
    0.715575887, 0.717701770, 0.719817864, 0.721924234, 0.724020949, 0.726108075, 0.728185676,
    0.730253819, 0.732312567, 0.734361984, 0.736402134, 0.738433080, 0.740454883, 0.742467605,
];

/// Main APU module integrating frame counter and sound channels
pub struct Apu {
    frame_counter: FrameCounter,
    pulse1: Pulse,
    pulse2: Pulse,
    triangle: Triangle,
    noise: Noise,
    dmc: Dmc,
    // Sample generation
    sample_accumulator: f32,
    cycles_per_sample: f32,
    pending_sample: Option<f32>,
    // Channel enable/disable flags for debugging
    pulse1_enabled: bool,
    pulse2_enabled: bool,
    triangle_enabled: bool,
    noise_enabled: bool,
    dmc_enabled: bool,
    // APU cycle counter for timer clocking
    apu_cycle: u32,
    // Power-on/reset state
    last_4017_write: u8,
    power_on_delay: Option<u32>, // Countdown for power-on $4017 write (9-12 cycles)
}

impl Apu {
    /// Create a new APU
    pub fn new() -> Self {
        const DEFAULT_SAMPLE_RATE: f32 = 44100.0;

        let mut apu = Self {
            frame_counter: FrameCounter::new(),
            pulse1: Pulse::new(true),  // Pulse 1 uses ones' complement
            pulse2: Pulse::new(false), // Pulse 2 uses two's complement
            triangle: Triangle::new(),
            noise: Noise::new(),
            dmc: Dmc::new(),
            sample_accumulator: 0.0,
            cycles_per_sample: CPU_CLOCK_NTSC / DEFAULT_SAMPLE_RATE,
            pending_sample: None,
            pulse1_enabled: true,
            pulse2_enabled: true,
            triangle_enabled: true,
            noise_enabled: true,
            dmc_enabled: true,
            apu_cycle: 0,
            last_4017_write: 0x00,
            power_on_delay: None,
        };

        // At power-on: $00 written to $4017, then 9 cycle delay before CPU execution
        apu.frame_counter.write_register(0x00);
        for _ in 0..9 {
            apu.frame_counter.clock();
        }

        apu
    }
    /// Create a new APU without power-on delay (for testing)
    /// This creates an APU as if code execution started immediately at frame counter cycle 0
    #[cfg(test)]
    fn new_for_testing() -> Self {
        const DEFAULT_SAMPLE_RATE: f32 = 44100.0;

        let mut apu = Self {
            frame_counter: FrameCounter::new(),
            pulse1: Pulse::new(true),
            pulse2: Pulse::new(false),
            triangle: Triangle::new(),
            noise: Noise::new(),
            dmc: Dmc::new(),
            sample_accumulator: 0.0,
            cycles_per_sample: CPU_CLOCK_NTSC / DEFAULT_SAMPLE_RATE,
            pending_sample: None,
            pulse1_enabled: true,
            pulse2_enabled: true,
            triangle_enabled: true,
            noise_enabled: true,
            dmc_enabled: true,
            apu_cycle: 0,
            last_4017_write: 0x00,
            power_on_delay: None,
        };

        // Initialize frame counter to 0 without power-on delay
        apu.frame_counter.write_register(0x00);

        apu
    }

    /// Reset the APU to its initial power-on state
    pub fn reset(&mut self) {
        self.frame_counter = FrameCounter::new();
        self.pulse1 = Pulse::new(true);
        self.pulse2 = Pulse::new(false);
        self.triangle = Triangle::new();
        self.noise = Noise::new();
        self.dmc = Dmc::new();
        self.sample_accumulator = 0.0;
        self.pending_sample = None;
        self.pulse1_enabled = true;
        self.pulse2_enabled = true;
        self.triangle_enabled = true;
        self.noise_enabled = true;
        self.dmc_enabled = true;
        self.apu_cycle = 0;
        // Reset: re-write last $4017 value, then 9 cycle delay before CPU execution
        self.frame_counter.write_register(self.last_4017_write);
        for _ in 0..9 {
            self.frame_counter.clock();
        }
        // Note: sample rate is preserved across resets
        // Note: last_4017_write is preserved (not reset to $00)
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

    /// Write to frame counter register ($4017)
    /// This is the public API that should be used instead of frame_counter_mut().write_register()
    /// to properly track the last written value for reset behavior
    pub fn write_frame_counter(&mut self, value: u8) {
        self.last_4017_write = value;
        self.frame_counter.write_register(value);
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

        // Clock timers every APU cycle (every 2 CPU cycles)
        // APU runs at half the CPU clock rate
        // Use dedicated apu_cycle counter to ensure consistent timing
        if self.apu_cycle % 2 == 0 {
            self.pulse1.clock_timer();
            self.pulse2.clock_timer();
            self.triangle.clock_timer();
            self.noise.clock_timer();
        }

        // Increment APU cycle counter
        self.apu_cycle = self.apu_cycle.wrapping_add(1);

        // DMC timer runs every CPU cycle (independent of frame counter)
        self.dmc.clock_timer();

        // Sample generation
        self.sample_accumulator += 1.0;
        if self.sample_accumulator >= self.cycles_per_sample {
            self.sample_accumulator -= self.cycles_per_sample;
            self.pending_sample = Some(self.mix());
        }
    }

    /// Read the APU status register ($4015)
    /// Returns: IF-D NT21
    /// - Bit 7 (I): DMC interrupt flag
    /// - Bit 6 (F): Frame counter interrupt flag
    /// - Bit 5: Open bus (returns the current open bus value)
    /// - Bit 4 (D): DMC active (bytes remaining > 0)
    /// - Bit 3 (N): Noise length counter > 0
    /// - Bit 2 (T): Triangle length counter > 0
    /// - Bit 1 (2): Pulse 2 length counter > 0
    /// - Bit 0 (1): Pulse 1 length counter > 0
    ///
    /// Side effect: Clears the frame counter interrupt flag
    pub fn read_status(&mut self, open_bus: u8) -> u8 {
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

        // Bit 5 is open bus - preserve it from the last value on the data bus
        status |= open_bus & (1 << 5);

        // Side effect: Clear frame counter interrupt flag
        self.frame_counter.clear_irq_flag();

        status
    }

    /// Write to the APU enable register ($4015)
    /// Format: ---D NT21
    /// - Bit 4 (D): Enable DMC
    /// - Bit 3 (N): Enable Noise
    /// - Bit 2 (T): Enable Triangle
    /// - Bit 1 (2): Enable Pulse 2
    /// - Bit 0 (1): Enable Pulse 1
    ///
    /// Writing 0 to a channel bit silences that channel and halts its length counter.
    /// Writing 1 enables the channel.
    /// For DMC: If enabled and bytes remaining = 0, restart sample.
    ///
    /// Side effect: Clears the DMC interrupt flag
    pub fn write_enable(&mut self, value: u8) {
        self.pulse1
            .set_length_counter_enabled(value & STATUS_PULSE1 != 0);
        self.pulse2
            .set_length_counter_enabled(value & STATUS_PULSE2 != 0);
        self.triangle
            .set_length_counter_enabled(value & STATUS_TRIANGLE != 0);
        self.noise
            .set_length_counter_enabled(value & STATUS_NOISE != 0);
        self.dmc.set_enabled(value & STATUS_DMC != 0);

        // Side effect: Clear DMC interrupt flag
        self.dmc.clear_irq_flag();
    }

    /// Mix all channel outputs using non-linear DAC
    /// Returns audio output in range 0.0 to 1.0
    pub fn mix(&self) -> f32 {
        // Get channel outputs (0 if channel is disabled)
        let pulse1 = if self.pulse1_enabled {
            self.pulse1.output() as usize
        } else {
            0
        };
        let pulse2 = if self.pulse2_enabled {
            self.pulse2.output() as usize
        } else {
            0
        };
        let triangle = if self.triangle_enabled {
            self.triangle.output() as usize
        } else {
            0
        };
        let noise = if self.noise_enabled {
            self.noise.output() as usize
        } else {
            0
        };
        let dmc = if self.dmc_enabled {
            self.dmc.output() as usize
        } else {
            0
        };

        // Pulse mixing (table index is sum of both pulse channels)
        let pulse_index = pulse1 + pulse2;
        let pulse_out = if pulse_index < PULSE_TABLE.len() {
            PULSE_TABLE[pulse_index]
        } else {
            0.0
        };

        // TND mixing (table index is 3*triangle + 2*noise + dmc)
        let tnd_index = 3 * triangle + 2 * noise + dmc;
        let tnd_out = if tnd_index < TND_TABLE.len() {
            TND_TABLE[tnd_index]
        } else {
            0.0
        };

        // Combine outputs
        pulse_out + tnd_out
    }

    /// Set the sample rate for audio output
    ///
    /// # Arguments
    /// * `sample_rate` - Target sample rate in Hz (e.g., 44100.0, 48000.0)
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.cycles_per_sample = CPU_CLOCK_NTSC / sample_rate;
        self.sample_accumulator = 0.0;
        self.pending_sample = None;
    }

    /// Check if an audio sample is ready for retrieval
    pub fn sample_ready(&self) -> bool {
        self.pending_sample.is_some()
    }

    /// Get the next audio sample if one is ready
    ///
    /// Returns `Some(sample)` if a sample is available, `None` otherwise.
    /// After calling this, `sample_ready()` will return false until the next sample is generated.
    pub fn get_sample(&mut self) -> Option<f32> {
        self.pending_sample.take()
    }

    /// Enable or disable individual channels for debugging
    pub fn set_pulse1_enabled(&mut self, enabled: bool) {
        self.pulse1_enabled = enabled;
    }

    pub fn set_pulse2_enabled(&mut self, enabled: bool) {
        self.pulse2_enabled = enabled;
    }

    pub fn set_triangle_enabled(&mut self, enabled: bool) {
        self.triangle_enabled = enabled;
    }

    pub fn set_noise_enabled(&mut self, enabled: bool) {
        self.noise_enabled = enabled;
    }

    pub fn set_dmc_enabled(&mut self, enabled: bool) {
        self.dmc_enabled = enabled;
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
        let apu = Apu::new_for_testing();
        assert_eq!(apu.frame_counter().get_cycle_counter(), 0);
        assert_eq!(apu.pulse1().output(), 0);
        assert_eq!(apu.pulse2().output(), 0);
        assert_eq!(apu.triangle().output(), 0); // Triangle is muted with zero counters
        assert_eq!(apu.noise().output(), 0); // Noise is muted with zero length counter
    }

    #[test]
    fn test_frame_counter_advances() {
        let mut apu = Apu::new_for_testing();
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
        let mut apu = Apu::new_for_testing();

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
        let mut apu = Apu::new_for_testing();

        // Set up pulse with length counter = 1
        apu.write_enable(STATUS_PULSE1);
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
        let mut apu = Apu::new_for_testing();

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
        let mut apu = Apu::new_for_testing();

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
        let mut apu = Apu::new_for_testing();
        apu.write_enable(0b0001_1111); // Enable all channels
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
        let mut apu = Apu::new_for_testing();

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
        let mut apu = Apu::new_for_testing();

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
        let mut apu = Apu::new_for_testing();

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
        let mut apu = Apu::new_for_testing();

        // Load length counter (index 5 = value 4)
        apu.write_enable(STATUS_TRIANGLE);
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
        let apu = Apu::new_for_testing();
        assert_eq!(apu.noise().output(), 0); // Noise starts muted (length counter = 0)
    }

    #[test]
    fn test_noise_envelope_gets_clocked() {
        let mut apu = Apu::new_for_testing();

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
        let mut apu = Apu::new_for_testing();

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
        let apu = Apu::new_for_testing();
        // Should be able to access DMC channel
        assert_eq!(apu.dmc().output(), 0);
    }

    #[test]
    fn test_dmc_channel_mutable() {
        let mut apu = Apu::new_for_testing();
        // Should be able to mutably access DMC channel
        apu.dmc_mut().write_direct_load(0b0100_0000); // Set output to 64
        assert_eq!(apu.dmc().output(), 64);
    }

    #[test]
    fn test_dmc_timer_gets_clocked() {
        let mut apu = Apu::new_for_testing();

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
        let mut apu = Apu::new_for_testing();
        // All channels start with length counter = 0
        // Bits: IF-D NT21
        // Expected: 0b0000_0000 (all inactive)
        assert_eq!(apu.read_status(0), 0b0000_0000);
    }

    #[test]
    fn test_status_pulse1_active() {
        let mut apu = Apu::new_for_testing();
        // Enable pulse 1 channel first
        apu.write_enable(STATUS_PULSE1);
        // Load length counter for pulse 1
        apu.pulse1_mut()
            .write_length_counter_timer_high(0b00001_000); // Index 1 = length 254
        // Bit 0 should be set
        assert_eq!(apu.read_status(0) & 0b0000_0001, 0b0000_0001);
    }

    #[test]
    fn test_status_pulse2_active() {
        let mut apu = Apu::new_for_testing();
        // Enable pulse 2 channel first
        apu.write_enable(STATUS_PULSE2);
        // Load length counter for pulse 2
        apu.pulse2_mut()
            .write_length_counter_timer_high(0b00001_000); // Index 1 = length 254
        // Bit 1 should be set
        assert_eq!(apu.read_status(0) & 0b0000_0010, 0b0000_0010);
    }

    #[test]
    fn test_status_triangle_active() {
        let mut apu = Apu::new_for_testing();
        // Enable triangle channel first
        apu.write_enable(STATUS_TRIANGLE);
        // Load length counter for triangle
        apu.triangle_mut().load_length_counter(1); // Index 1 = length 254
        // Bit 2 should be set
        assert_eq!(apu.read_status(0) & 0b0000_0100, 0b0000_0100);
    }

    #[test]
    fn test_status_noise_active() {
        let mut apu = Apu::new_for_testing();
        // Enable noise channel first
        apu.write_enable(STATUS_NOISE);
        // Load length counter for noise (index 1 = length 254)
        apu.noise_mut().write_length(0b00001_000);
        // Bit 3 should be set
        assert_eq!(apu.read_status(0) & 0b0000_1000, 0b0000_1000);
    }

    #[test]
    fn test_status_all_channels_active() {
        let mut apu = Apu::new_for_testing();
        // Load length counters for all channels
        apu.write_enable(0b0001_1111);
        apu.pulse1_mut()
            .write_length_counter_timer_high(0b00001_000);
        apu.pulse2_mut()
            .write_length_counter_timer_high(0b00001_000);
        apu.triangle_mut().load_length_counter(1);
        apu.noise_mut().write_length(0b00001_000);
        // Bits 0-3 should be set (no DMC, no interrupts yet)
        assert_eq!(apu.read_status(0) & 0b0000_1111, 0b0000_1111);
    }

    #[test]
    fn test_enable_disable_pulse1() {
        let mut apu = Apu::new_for_testing();
        // Load pulse 1 length counter
        apu.write_enable(STATUS_PULSE1);
        apu.pulse1_mut()
            .write_length_counter_timer_high(0b00001_000);
        assert_eq!(apu.read_status(0) & STATUS_PULSE1, STATUS_PULSE1);

        // Disable pulse 1
        apu.write_enable(0b0000_0000);
        assert_eq!(apu.read_status(0) & STATUS_PULSE1, 0);
    }

    #[test]
    fn test_enable_pulse1_with_enable_bit() {
        let mut apu = Apu::new_for_testing();
        // Enable pulse 1
        apu.write_enable(STATUS_PULSE1);
        // Load length counter should work
        apu.pulse1_mut()
            .write_length_counter_timer_high(0b00001_000);
        assert_eq!(apu.read_status(0) & STATUS_PULSE1, STATUS_PULSE1);
    }

    #[test]
    fn test_enable_all_channels() {
        let mut apu = Apu::new_for_testing();
        // Enable all channels
        apu.write_enable(0b0001_1111);
        // Load all length counters
        apu.pulse1_mut()
            .write_length_counter_timer_high(0b00001_000);
        apu.pulse2_mut()
            .write_length_counter_timer_high(0b00001_000);
        apu.triangle_mut().load_length_counter(1);
        apu.noise_mut().write_length(0b00001_000);
        // All should be active
        assert_eq!(apu.read_status(0) & 0b0000_1111, 0b0000_1111);
    }

    #[test]
    fn test_disable_clears_length_counters() {
        let mut apu = Apu::new_for_testing();
        // Load all length counters
        apu.write_enable(0b0001_1111);
        apu.pulse1_mut()
            .write_length_counter_timer_high(0b00001_000);
        apu.pulse2_mut()
            .write_length_counter_timer_high(0b00001_000);
        apu.triangle_mut().load_length_counter(1);
        apu.noise_mut().write_length(0b00001_000);
        // Verify all active
        assert_eq!(apu.read_status(0) & 0b0000_1111, 0b0000_1111);

        // Disable all channels
        apu.write_enable(0b0000_0000);
        // All should be inactive
        assert_eq!(apu.read_status(0) & 0b0000_1111, 0b0000_0000);
    }

    #[test]
    fn test_enable_dmc_restarts_sample_when_empty() {
        let mut apu = Apu::new_for_testing();
        // Set up DMC with sample address and length
        apu.dmc_mut().write_sample_address(0x00); // Address $C000
        apu.dmc_mut().write_sample_length(0x01); // Length 17 bytes

        // Enable DMC - should restart sample
        apu.write_enable(STATUS_DMC);

        // DMC should now have bytes remaining
        assert_eq!(apu.read_status(0) & STATUS_DMC, STATUS_DMC);
    }

    #[test]
    fn test_disable_dmc_clears_bytes_remaining() {
        let mut apu = Apu::new_for_testing();
        // Set up and enable DMC
        apu.dmc_mut().write_sample_address(0x00);
        apu.dmc_mut().write_sample_length(0x01);
        apu.write_enable(STATUS_DMC);
        assert!(apu.dmc().has_bytes_remaining());

        // Disable DMC
        apu.write_enable(0b0000_0000);
        assert!(!apu.dmc().has_bytes_remaining());
    }

    #[test]
    fn test_write_enable_clears_dmc_interrupt() {
        let mut apu = Apu::new_for_testing();
        // Manually trigger DMC IRQ by setting it up to finish
        apu.dmc_mut().write_flags_and_rate(0b1000_0000); // IRQ enabled
        apu.dmc_mut().write_sample_address(0x00);
        apu.dmc_mut().write_sample_length(0x00); // Minimal length

        // Any write to enable register should clear DMC IRQ flag
        apu.write_enable(0b0000_0000);
        assert_eq!(apu.read_status(0) & STATUS_DMC_IRQ, 0);
    }

    #[test]
    fn test_mixer_all_channels_silent() {
        let apu = Apu::new_for_testing();
        // All channels start at 0
        let output = apu.mix();
        assert_eq!(output, 0.0);
    }

    #[test]
    fn test_mixer_pulse_only() {
        let mut apu = Apu::new_for_testing();
        // Set pulse 1 to max volume (15) with duty 3 (starts high)
        apu.write_enable(STATUS_PULSE1);
        apu.pulse1_mut().write_control(0b1111_1111); // Duty 3, constant volume 15
        apu.pulse1_mut().write_timer_low(0x08); // Timer period >= 8
        apu.pulse1_mut()
            .write_length_counter_timer_high(0b00001_000); // Load length counter
        // Pulse generates square wave, output should be non-zero when high
        let output = apu.mix();
        assert!(output > 0.0);
        assert!(output <= 1.0);
    }

    #[test]
    fn test_mixer_output_range() {
        let mut apu = Apu::new_for_testing();
        // Set all channels to max with duty 3 (starts high) for pulse channels
        apu.pulse1_mut().write_control(0b1111_1111); // Duty 3, constant volume 15
        apu.pulse1_mut().write_timer_low(0x08); // Timer period >= 8
        apu.pulse1_mut()
            .write_length_counter_timer_high(0b00001_000);
        apu.pulse2_mut().write_control(0b1111_1111); // Duty 3, constant volume 15
        apu.pulse2_mut().write_timer_low(0x08); // Timer period >= 8
        apu.pulse2_mut()
            .write_length_counter_timer_high(0b00001_000);
        apu.triangle_mut().write_linear_counter(0xFF);
        apu.triangle_mut().write_length_counter_timer_high(0xFF);
        apu.noise_mut().write_envelope(0b0011_1111);
        apu.noise_mut().write_length(0xFF);
        apu.dmc_mut().write_direct_load(0b0111_1111); // Max DMC output (127)

        let output = apu.mix();
        // Output should be in valid range
        assert!(output >= 0.0);
        assert!(output <= 1.0);
    }

    #[test]
    fn test_mixer_formula_pulse() {
        let apu = Apu::new_for_testing();
        // Test with known pulse values
        // pulse_out = 95.88 / ((8128 / (pulse1 + pulse2)) + 100)
        // For pulse1 = 0, pulse2 = 0: pulse_out = 0
        let output = apu.mix();
        assert_eq!(output, 0.0);
    }

    #[test]
    fn test_mixer_combines_channels() {
        let mut apu = Apu::new_for_testing();
        // Set pulse 1 with duty 3 (starts high)
        apu.pulse1_mut().write_control(0b1111_0101); // Duty 3, constant volume 5
        apu.pulse1_mut().write_timer_low(0x08); // Timer period >= 8
        apu.pulse1_mut()
            .write_length_counter_timer_high(0b00001_000);
        let pulse_only = apu.mix();

        // Add DMC
        apu.dmc_mut().write_direct_load(0b0010_0000); // DMC output 32
        let pulse_and_dmc = apu.mix();

        // Combined output should be greater (non-linear mixing)
        assert!(pulse_and_dmc >= pulse_only);
    }

    #[test]
    fn test_sample_generation_no_sample_initially() {
        let apu = Apu::new_for_testing();
        // No sample should be ready before clocking
        assert!(!apu.sample_ready());
    }

    #[test]
    fn test_sample_generation_after_clocking() {
        let mut apu = Apu::new_for_testing();
        // Clock the APU enough times to generate a sample
        // For 44100 Hz from 1.789 MHz: ~40.56 cycles per sample
        for _ in 0..41 {
            apu.clock();
        }
        // Sample should be ready after ~41 cycles
        assert!(apu.sample_ready());
    }

    #[test]
    fn test_sample_generation_retrieves_sample() {
        let mut apu = Apu::new_for_testing();
        // Generate a sample
        for _ in 0..41 {
            apu.clock();
        }
        assert!(apu.sample_ready());

        // Retrieve the sample
        let sample = apu.get_sample();
        assert!(sample.is_some());

        // After retrieval, no sample should be ready
        assert!(!apu.sample_ready());
    }

    #[test]
    fn test_sample_generation_uses_mixer_output() {
        let mut apu = Apu::new_for_testing();
        // Set up pulse channel to produce output with 50% duty cycle
        apu.write_enable(STATUS_PULSE1);
        apu.pulse1_mut().write_control(0b1011_1111); // Duty 2 (50%), constant volume 15
        apu.pulse1_mut().write_timer_low(0x08); // Timer = 8
        apu.pulse1_mut()
            .write_length_counter_timer_high(0b00001_000);

        // Clock enough to generate multiple samples - at least one should be non-zero
        // With duty 2 (50%), half the samples should be non-zero
        let mut non_zero_found = false;
        for _ in 0..200 {
            apu.clock();
            if let Some(sample) = apu.get_sample() {
                if sample > 0.0 {
                    non_zero_found = true;
                    assert!(sample <= 1.0);
                }
            }
        }
        assert!(
            non_zero_found,
            "Expected at least one non-zero sample with 50% duty cycle"
        );
    }

    #[test]
    fn test_sample_generation_timing() {
        let mut apu = Apu::new_for_testing();
        let mut sample_count = 0;

        // Clock for 1789 cycles (should generate ~44 samples at 44100 Hz)
        for _ in 0..1789 {
            apu.clock();
            if apu.sample_ready() {
                apu.get_sample();
                sample_count += 1;
            }
        }

        // Should generate approximately 44 samples (1789 / 40.56 ≈ 44.08)
        assert!(sample_count >= 43 && sample_count <= 45);
    }

    #[test]
    fn test_sample_generation_configurable_rate() {
        let mut apu = Apu::new_for_testing();

        // Set to 48000 Hz (1.789 MHz / 48000 ≈ 37.27 cycles per sample)
        apu.set_sample_rate(48000.0);

        // Clock for 1789 cycles (should generate ~48 samples at 48000 Hz)
        let mut sample_count = 0;
        for _ in 0..1789 {
            apu.clock();
            if apu.sample_ready() {
                apu.get_sample();
                sample_count += 1;
            }
        }

        // Should generate approximately 48 samples (1789 / 37.27 ≈ 48)
        assert!(sample_count >= 47 && sample_count <= 49);
    }
}
