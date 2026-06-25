use std::cell::RefCell;
use std::rc::Rc;

use super::dmc::Dmc;
use super::frame_counter::FrameCounter;
use super::noise::Noise;
use super::pulse::Pulse;
use super::triangle::Triangle;
use crate::nes::apu::dmc::DmcState;
use crate::nes::apu::noise::NoiseState;
use crate::nes::apu::pulse::PulseState;
use crate::nes::apu::triangle::TriangleState;
use crate::nes::console::TimingMode;
use crate::platform::save_state::Stateful;
use crate::trace_apu;
use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Observer, RingBuffer};
use serde::{Deserialize, Serialize};

/// APU frame counter state for save-state support.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FrameCounterState {
    pub cycle_counter: u32,
    pub mode: bool,
    pub irq_inhibit: bool,
    pub irq_flag: bool,
    pub irq_assert_cycles_remaining: u8,
    pub block_frame_counter: bool,
    pub five_step_extra_cycle: bool,
    pub pending_write: Option<u8>,
    pub write_delay: u8,
    pub pending_write_on_odd_cpu_cycle: bool,
    pub pending_immediate_quarter: bool,
    pub pending_immediate_half: bool,
}

/// APU complete state for save-state support.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApuState {
    pub frame_counter: FrameCounterState,
    pub pulse1: PulseState,
    pub pulse2: PulseState,
    pub triangle: TriangleState,
    pub noise: NoiseState,
    pub dmc: DmcState,
    pub sample_accumulator: f32,
    pub cycles_per_sample: f32,
    pub pending_samples: Vec<f32>,
    pub pulse1_enabled: bool,
    pub pulse2_enabled: bool,
    pub triangle_enabled: bool,
    pub noise_enabled: bool,
    pub dmc_enabled: bool,
    pub apu_cycle: u32,
    pub cpu_cycle: u64,
    pub last_4017_write: u8,
}

// Upper bound for queued audio samples awaiting retrieval.
//
// Rationale:
// - The emulator can run with `--no-audio` (no consumer), and some workloads (e.g. DMA stalls)
//   may temporarily delay polling.
// - Keeping this bounded prevents unbounded memory growth while still providing ample headroom
//   for short-lived stalls.
const MAX_PENDING_SAMPLES: usize = 16_384;

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
#[allow(clippy::excessive_precision)]
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
#[allow(clippy::excessive_precision)]
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

pub type SharedApu = Rc<RefCell<Apu>>;

/// Main APU module integrating frame counter and sound channels
pub struct Apu {
    tv_system: TimingMode,
    frame_counter: FrameCounter,
    pulse1: Pulse,
    pulse2: Pulse,
    triangle: Triangle,
    noise: Noise,
    dmc: Dmc,
    // Sample generation
    sample_accumulator: f32,
    cycles_per_sample: f32,
    pending_samples: HeapRb<f32>,
    // Channel enable/disable flags for debugging
    pulse1_enabled: bool,
    pulse2_enabled: bool,
    triangle_enabled: bool,
    noise_enabled: bool,
    dmc_enabled: bool,
    // APU cycle counter for timer clocking
    apu_cycle: u32,
    // CPU cycle counter (tracked by APU for DMC timing)
    cpu_cycle: u64,
    // Power-on/reset state
    last_4017_write: u8,
}

impl Apu {
    /// Create a new APU
    pub fn new() -> Self {
        Self::new_with_tv_system(TimingMode::Ntsc)
    }

    pub fn new_with_tv_system(tv_system: TimingMode) -> Self {
        const DEFAULT_SAMPLE_RATE: f32 = 44100.0;

        let pending_samples = HeapRb::<f32>::new(MAX_PENDING_SAMPLES);
        let cpu_clock = tv_system.cpu_clock_hz();

        let mut apu = Self {
            tv_system,
            frame_counter: FrameCounter::new_with_tv_system(tv_system),
            pulse1: Pulse::new(true),  // Pulse 1 uses ones' complement
            pulse2: Pulse::new(false), // Pulse 2 uses two's complement
            triangle: Triangle::new(),
            noise: Noise::new_with_tv_system(tv_system),
            dmc: Dmc::new_with_tv_system(tv_system),
            sample_accumulator: 0.0,
            cycles_per_sample: cpu_clock / DEFAULT_SAMPLE_RATE,
            pending_samples,
            pulse1_enabled: true,
            pulse2_enabled: true,
            triangle_enabled: true,
            noise_enabled: true,
            dmc_enabled: true,
            apu_cycle: 0,
            cpu_cycle: 0,
            last_4017_write: 0x00,
        };

        // At power-on: $00 written to $4017, then 9-12 cycle delay before CPU execution
        // Delay after NES being powered off for a minute is usually 9
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

        let pending_samples = HeapRb::<f32>::new(MAX_PENDING_SAMPLES);
        let cpu_clock = TimingMode::Ntsc.cpu_clock_hz();

        let mut apu = Self {
            tv_system: TimingMode::Ntsc,
            frame_counter: FrameCounter::new(),
            pulse1: Pulse::new(true),
            pulse2: Pulse::new(false),
            triangle: Triangle::new(),
            noise: Noise::new(),
            dmc: Dmc::new(),
            sample_accumulator: 0.0,
            cycles_per_sample: cpu_clock / DEFAULT_SAMPLE_RATE,
            pending_samples,
            // For testing: start with all channels enabled for convenience
            pulse1_enabled: true,
            pulse2_enabled: true,
            triangle_enabled: true,
            noise_enabled: true,
            dmc_enabled: true,
            apu_cycle: 0,
            cpu_cycle: 0,
            last_4017_write: 0x00,
        };

        // Initialize frame counter to 0 without power-on delay
        apu.frame_counter.write_register(0x00);

        apu
    }

    /// Get APU cycle counter
    pub fn apu_cycle(&self) -> u32 {
        self.apu_cycle
    }

    /// Reset the APU.
    ///
    /// - `cpu_cycle`: The total CPU cycles executed before this reset (for coordinated timing)
    /// - `soft_reset`: true for a reset-button style reset, false for power-on
    pub fn reset(&mut self, cpu_cycle: u64, soft_reset: bool) {
        trace_apu!(1; "reset cpu_cycle={} soft_reset={} last_4017_write=0x{:02X}", cpu_cycle, soft_reset, self.last_4017_write);
        self.frame_counter.reset();
        self.pulse1.reset();
        self.pulse2.reset();
        // At reset, triangle is preserved, but length counter is disabled
        self.triangle.reset();
        self.noise.reset();
        self.dmc.reset();
        self.sample_accumulator = 0.0;
        self.clear_pending_samples();
        self.apu_cycle = 0;

        // Power-on: behave as if `$4017 = $00`.
        if !soft_reset {
            self.last_4017_write = 0x00;
            self.frame_counter.write_register(0x00);
            // Blargg describes power-on as if `$4017=$00`, then a 9-12 CPU-cycle delay, then
            // code begins executing from the reset vector.
            //
            // Our `Cpu::reset(false)` consumes 7 CPU cycles (5 internal + 2 reset-vector reads)
            // before the first instruction executes, and these cycles tick the APU.
            // Empirically for blargg's `4017_written`, we also need to account for opcode-fetch
            // alignment, so we only advance 1 cycle here.
            for _ in 0..1 {
                self.frame_counter.clock();
            }
            return;
        }

        // Coordinated reset timing:
        // "APU acts as if $4017 were written with $00 from 9 to 12 clocks before first instruction"
        //
        // - Queue the delayed write FIRST with appropriate delay
        // - Then clock the frame counter which will process the write after the delay
        // - Additional clocking to reach the 9-12 cycle range

        // Delay based on CPU cycle parity: even = 4, odd = 3
        let write_delay = if cpu_cycle.is_multiple_of(2) { 4 } else { 3 };

        // Queue the delayed write (rewrite last value written to $4017)
        self.frame_counter
            .queue_delayed_write(self.last_4017_write, write_delay);

        // Clock the frame counter so that we end up 1 cycle after the *effective* $4017 write
        // at the end of `Apu::reset()`.
        //
        // Since `Nes::reset(true)` resets the APU before calling `Cpu::reset(true)`, the CPU reset
        // will tick the APU for 7 additional CPU cycles. That means we want:
        //   (apu_reset_position) + 7 == 8
        // so the CPU begins executing 8 cycles after the effective $4017 rewrite.
        //
        // Our delayed-write implementation applies the write during `clock()` (before increment),
        // and then increments within that same `clock()` call. So on the clock where the delayed
        // write takes effect, the counter ends at 1.
        let total_clocks = u32::from(write_delay) + 1;
        for _ in 0..total_clocks {
            self.frame_counter.clock();
        }

        // Note: sample rate is preserved across resets
        // Note: last_4017_write is preserved (not reset to $00)
        // Note: triangle channel is preserved (unaffected by reset)
    }

    pub fn debug_frame_counter_cycle(&self) -> u32 {
        self.frame_counter.get_cycle_counter()
    }

    /// Get reference to pulse channel 1
    #[cfg(test)]
    pub fn pulse1(&self) -> &Pulse {
        &self.pulse1
    }

    /// Get mutable reference to pulse channel 1
    pub fn pulse1_mut(&mut self) -> &mut Pulse {
        &mut self.pulse1
    }

    /// Get reference to pulse channel 2
    #[cfg(test)]
    pub fn pulse2(&self) -> &Pulse {
        &self.pulse2
    }

    /// Get mutable reference to pulse channel 2
    pub fn pulse2_mut(&mut self) -> &mut Pulse {
        &mut self.pulse2
    }

    /// Get reference to frame counter
    #[cfg(test)]
    pub fn frame_counter(&self) -> &FrameCounter {
        &self.frame_counter
    }

    /// Get mutable reference to frame counter
    #[cfg(test)]
    pub fn frame_counter_mut(&mut self) -> &mut FrameCounter {
        &mut self.frame_counter
    }

    /// Write to frame counter register ($4017)
    /// This is the public API that should be used instead of frame_counter_mut().write_register()
    /// to properly track the last written value for reset behavior
    pub fn write_frame_counter(&mut self, value: u8) {
        self.last_4017_write = value;

        // NESDev: $4017 write side-effects occur after 3 CPU cycles if written "during" an APU
        // cycle, or 4 CPU cycles if written "between" APU cycles.
        // In our timing model, timers clock on even apu_cycle values, so we treat:
        // - even apu_cycle: during APU cycle => 3-cycle delay
        // - odd apu_cycle: between APU cycles => 4-cycle delay
        let write_delay = if self.apu_cycle.is_multiple_of(2) {
            3
        } else {
            4
        };

        // Jitter: writing $4017 on an odd CPU cycle delays the reset by 1 CPU cycle.
        let write_on_odd_cpu_cycle = !self.apu_cycle.is_multiple_of(2);
        trace_apu!(1; "write $4017 value=0x{:02X} apu_cycle={} delay={} odd_cpu_cycle={}", value, self.apu_cycle, write_delay, write_on_odd_cpu_cycle);
        self.frame_counter.queue_delayed_write_with_jitter(
            value,
            write_delay,
            write_on_odd_cpu_cycle,
        );
    }

    /// Get reference to triangle channel
    #[cfg(test)]
    pub fn triangle(&self) -> &Triangle {
        &self.triangle
    }

    /// Get mutable reference to triangle channel
    pub fn triangle_mut(&mut self) -> &mut Triangle {
        &mut self.triangle
    }

    /// Get reference to noise channel
    #[cfg(test)]
    pub fn noise(&self) -> &Noise {
        &self.noise
    }

    /// Get mutable reference to noise channel
    pub fn noise_mut(&mut self) -> &mut Noise {
        &mut self.noise
    }

    /// Get reference to DMC channel
    #[cfg(test)]
    pub fn dmc(&self) -> &Dmc {
        &self.dmc
    }

    /// Get mutable reference to DMC channel
    pub fn dmc_mut(&mut self) -> &mut Dmc {
        &mut self.dmc
    }

    /// Clock the APU by one CPU cycle
    /// This advances the frame counter and triggers channel clocking when needed
    #[cfg(test)]
    pub fn clock(&mut self) {
        self.clock_with_expansion(0.0);
    }

    /// Tick the APU by one CPU cycle, optionally adding mapper-provided expansion audio.
    ///
    /// `expansion_audio` is expected to be a small linear contribution (e.g. 0.0..~0.5)
    /// that will be added to the base APU mix when a sample is generated.
    pub fn clock_with_expansion(&mut self, expansion_audio: f32) {
        // Trace APU tick with cycle and frame counter state (verbose)
        trace_apu!(
            5; "tick apu_cycle={} frame_counter_cycle={}",
            self.apu_cycle,
            self.frame_counter.get_cycle_counter()
        );

        let (quarter_frame, half_frame) = self.frame_counter.clock();

        if quarter_frame || half_frame {
            trace_apu!(
                3; "frame_counter clock quarter={} half={} cycle={} cpu_cycle={} apu_cycle={}",
                quarter_frame,
                half_frame,
                self.frame_counter.get_cycle_counter(),
                self.cpu_cycle,
                self.apu_cycle
            );
        }

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

        // Apply pending length counter reloads after any length clocks for this CPU cycle.
        self.pulse1.apply_pending_length_reload();
        self.pulse2.apply_pending_length_reload();
        self.triangle.apply_pending_length_reload();
        self.noise.apply_pending_length_reload();

        // Apply pending halt changes after any length clocks for this CPU cycle.
        self.pulse1.apply_pending_length_halt();
        self.pulse2.apply_pending_length_halt();
        self.triangle.apply_pending_length_halt();
        self.noise.apply_pending_length_halt();

        // Clock timers on the APU cycle (every 2 CPU cycles).
        // Note: The triangle timer runs at the CPU clock rate (NESdev), so it
        // must be clocked every CPU cycle, not every other CPU cycle.
        if self.apu_cycle.is_multiple_of(2) {
            self.pulse1.clock_timer();
            self.pulse2.clock_timer();
            self.noise.clock_timer();
        }
        self.triangle.clock_timer();

        // Increment APU cycle counter
        self.apu_cycle = self.apu_cycle.wrapping_add(1);

        // Increment CPU cycle counter (used for DMC timing)
        self.cpu_cycle = self.cpu_cycle.wrapping_add(1);

        // Process DMC delays and timing - must be called every CPU cycle
        self.dmc.process_clock();

        // DMC timer runs every CPU cycle (independent of frame counter)
        self.dmc.clock_timer();

        // Sample generation
        self.sample_accumulator += 1.0;
        if self.sample_accumulator >= self.cycles_per_sample {
            self.sample_accumulator -= self.cycles_per_sample;
            self.push_pending_sample(self.mix() + expansion_audio.max(0.0));
        }
    }

    /// Check if an audio sample is ready for retrieval
    ///
    /// Returns true when the APU has generated at least one new audio sample.
    pub fn sample_ready(&self) -> bool {
        !self.pending_samples.is_empty()
    }

    /// Get the next audio sample if one is ready
    ///
    /// Returns `Some(sample)` if a sample is available, `None` otherwise.
    /// The sample is in the range 0.0 to 1.0.
    pub fn get_sample(&mut self) -> Option<f32> {
        self.pending_samples.try_pop()
    }

    /// Poll the APU IRQ flag (frame counter or DMC IRQ)
    /// Returns true if an IRQ should be triggered
    /// This method does NOT clear the IRQ flags - they are cleared by reading $4015
    pub fn poll_irq(&self) -> bool {
        self.frame_counter.get_irq_flag() || self.dmc.get_irq_flag()
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

        // Only report length counter > 0 if channel is also enabled
        if self.pulse1.is_length_counter_enabled() && self.pulse1.get_length_counter() > 0 {
            status |= STATUS_PULSE1;
        }
        if self.pulse2.is_length_counter_enabled() && self.pulse2.get_length_counter() > 0 {
            status |= STATUS_PULSE2;
        }
        if self.triangle.is_length_counter_enabled() && self.triangle.get_length_counter() > 0 {
            status |= STATUS_TRIANGLE;
        }
        if self.noise.is_length_counter_enabled() && self.noise.get_length_counter() > 0 {
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

        trace_apu!(
            3; "read $4015 status=0b{:08b} open_bus=0x{:02X}",
            status,
            open_bus
        );

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
    /// Writing 0 to a channel bit clears that channel's length counter to 0.
    /// Writing 1 enables the channel.
    /// For DMC: If enabled and bytes remaining = 0, restart sample.
    ///
    /// Side effect: Clears the DMC interrupt flag
    pub fn write_enable(&mut self, value: u8) {
        trace_apu!(1; "write $4015 value=0x{:02X}", value);
        // Pulse 1
        let pulse1_enabled = value & STATUS_PULSE1 != 0;
        if !pulse1_enabled {
            self.pulse1.clear_length_counter();
        }
        self.pulse1.set_length_counter_enabled(pulse1_enabled);

        // Pulse 2
        let pulse2_enabled = value & STATUS_PULSE2 != 0;
        if !pulse2_enabled {
            self.pulse2.clear_length_counter();
        }
        self.pulse2.set_length_counter_enabled(pulse2_enabled);

        // Triangle
        let triangle_enabled = value & STATUS_TRIANGLE != 0;
        if !triangle_enabled {
            self.triangle.clear_length_counter();
        }
        self.triangle.set_length_counter_enabled(triangle_enabled);

        // Noise
        let noise_enabled = value & STATUS_NOISE != 0;
        if !noise_enabled {
            self.noise.clear_length_counter();
        }
        self.noise.set_length_counter_enabled(noise_enabled);

        // DMC - pass current CPU cycle for accurate delay timing
        self.dmc
            .set_enabled(value & STATUS_DMC != 0, self.cpu_cycle);

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

        trace_apu!(5; "Mixing pulse1,pulse2,triangle,noise,dmc=({}, {}, {}, {}, {}) into {}", pulse1, pulse2, triangle, noise, dmc, pulse_out + tnd_out);

        // Combine outputs
        pulse_out + tnd_out
    }

    /// Set the sample rate for audio output
    ///
    /// # Arguments
    /// * `sample_rate` - Target sample rate in Hz (e.g., 44100.0, 48000.0)
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.cycles_per_sample = self.tv_system.cpu_clock_hz() / sample_rate;
        self.sample_accumulator = 0.0;
        self.clear_pending_samples();
    }

    fn push_pending_sample(&mut self, sample: f32) {
        self.pending_samples.push_overwrite(sample);
    }

    fn clear_pending_samples(&mut self) {
        self.pending_samples.clear();
    }

    fn pending_samples_snapshot(&self) -> Vec<f32> {
        let start = self.pending_samples.read_index();
        let end = self.pending_samples.write_index();
        let (first, second) = unsafe { self.pending_samples.unsafe_slices(start, end) };
        let mut samples = Vec::with_capacity(self.pending_samples.occupied_len());
        samples.extend(
            first
                .iter()
                .map(|value| unsafe { *value.assume_init_ref() }),
        );
        samples.extend(
            second
                .iter()
                .map(|value| unsafe { *value.assume_init_ref() }),
        );
        samples
    }

    fn restore_pending_samples(&mut self, samples: &[f32]) {
        self.clear_pending_samples();
        for &sample in samples {
            self.push_pending_sample(sample);
        }
    }

    #[cfg(test)]
    pub(crate) fn push_sample_for_test(&mut self, sample: f32) {
        self.push_pending_sample(sample);
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

    /// Capture the current APU state for save-state.
    fn capture_state_inner(&self) -> ApuState {
        ApuState {
            frame_counter: FrameCounterState {
                cycle_counter: self.frame_counter.get_cycle_counter(),
                mode: self.frame_counter.get_mode(),
                irq_inhibit: self.frame_counter.get_irq_inhibit(),
                irq_flag: self.frame_counter.get_irq_flag(),
                irq_assert_cycles_remaining: self.frame_counter.irq_assert_cycles_remaining(),
                block_frame_counter: self.frame_counter.block_frame_counter(),
                five_step_extra_cycle: self.frame_counter.five_step_extra_cycle(),
                pending_write: self.frame_counter.pending_write(),
                write_delay: self.frame_counter.write_delay(),
                pending_write_on_odd_cpu_cycle: self.frame_counter.pending_write_on_odd_cpu_cycle(),
                pending_immediate_quarter: self.frame_counter.pending_immediate_clock().0,
                pending_immediate_half: self.frame_counter.pending_immediate_clock().1,
            },
            pulse1: self.pulse1.capture_state(),
            pulse2: self.pulse2.capture_state(),
            triangle: self.triangle.capture_state(),
            noise: self.noise.capture_state(),
            dmc: self.dmc.capture_state(),
            sample_accumulator: self.sample_accumulator,
            cycles_per_sample: self.cycles_per_sample,
            pending_samples: self.pending_samples_snapshot(),
            pulse1_enabled: self.pulse1_enabled,
            pulse2_enabled: self.pulse2_enabled,
            triangle_enabled: self.triangle_enabled,
            noise_enabled: self.noise_enabled,
            dmc_enabled: self.dmc_enabled,
            apu_cycle: self.apu_cycle,
            cpu_cycle: self.cpu_cycle,
            last_4017_write: self.last_4017_write,
        }
    }

    /// Restore APU state from a save-state.
    fn restore_state_inner(&mut self, state: &ApuState) {
        // Restore frame counter
        self.frame_counter.restore_state(
            state.frame_counter.cycle_counter,
            state.frame_counter.mode,
            state.frame_counter.irq_inhibit,
            state.frame_counter.irq_flag,
            state.frame_counter.irq_assert_cycles_remaining,
            state.frame_counter.block_frame_counter,
            state.frame_counter.five_step_extra_cycle,
            state.frame_counter.pending_write,
            state.frame_counter.write_delay,
            state.frame_counter.pending_write_on_odd_cpu_cycle,
            (
                state.frame_counter.pending_immediate_quarter,
                state.frame_counter.pending_immediate_half,
            ),
        );

        // Restore channels
        self.pulse1.restore_state(&state.pulse1);
        self.pulse2.restore_state(&state.pulse2);
        self.triangle.restore_state(&state.triangle);
        self.noise.restore_state(&state.noise);
        self.dmc.restore_state(&state.dmc);

        // Restore timing state
        self.apu_cycle = state.apu_cycle;
        self.cpu_cycle = state.cpu_cycle;
        self.last_4017_write = state.last_4017_write;

        self.sample_accumulator = state.sample_accumulator;
        self.cycles_per_sample = state.cycles_per_sample;
        self.restore_pending_samples(&state.pending_samples);

        self.pulse1_enabled = state.pulse1_enabled;
        self.pulse2_enabled = state.pulse2_enabled;
        self.triangle_enabled = state.triangle_enabled;
        self.noise_enabled = state.noise_enabled;
        self.dmc_enabled = state.dmc_enabled;
    }
}

impl Stateful for Apu {
    type State = ApuState;

    fn capture_state(&self) -> ApuState {
        self.capture_state_inner()
    }

    fn restore_state(&mut self, state: &ApuState) {
        self.restore_state_inner(state);
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unusual_byte_groupings)]
mod tests {
    use super::*;

    fn write_pulse1_length(apu: &mut Apu, value: u8) {
        apu.pulse1_mut().write_length_counter_timer_high(value);
        apu.pulse1_mut().apply_pending_length_reload();
    }

    fn write_pulse2_length(apu: &mut Apu, value: u8) {
        apu.pulse2_mut().write_length_counter_timer_high(value);
        apu.pulse2_mut().apply_pending_length_reload();
    }

    fn write_triangle_length(apu: &mut Apu, value: u8) {
        apu.triangle_mut().write_length_counter_timer_high(value);
        apu.triangle_mut().apply_pending_length_reload();
    }

    fn load_triangle_length(apu: &mut Apu, index: u8) {
        apu.triangle_mut().load_length_counter(index);
        apu.triangle_mut().apply_pending_length_reload();
    }

    fn write_noise_length(apu: &mut Apu, value: u8) {
        apu.noise_mut().write_length(value);
        apu.noise_mut().apply_pending_length_reload();
    }

    #[test]
    fn test_clock_with_expansion_adds_to_mix() {
        let mut apu = Apu::new_for_testing();

        // Advance until a sample is generated, feeding a constant expansion contribution.
        // With all internal channels silent, the produced sample should be ~expansion.
        for _ in 0..128 {
            apu.clock_with_expansion(0.1);
            if apu.sample_ready() {
                break;
            }
        }

        let sample = apu.get_sample().expect("expected a sample");
        assert!((sample - 0.1).abs() < 0.0001, "sample was {sample}");
    }

    #[test]
    fn test_apu_save_state_roundtrip_includes_internal_state() {
        let mut apu = Apu::new_for_testing();

        apu.sample_accumulator = 7.25;
        apu.cycles_per_sample = 123.0;
        apu.pulse1_enabled = false;
        apu.pulse2_enabled = true;
        apu.triangle_enabled = false;
        apu.noise_enabled = true;
        apu.dmc_enabled = false;

        apu.apu_cycle = 1234;
        apu.cpu_cycle = 5678;
        apu.last_4017_write = 0xC0;

        apu.push_sample_for_test(0.1);
        apu.push_sample_for_test(0.2);

        apu.frame_counter
            .queue_delayed_write_with_jitter(0x80, 3, true);
        apu.frame_counter
            .debug_set_pending_immediate_clock(true, false);
        apu.frame_counter.debug_set_irq_assert_cycles_remaining(2);
        apu.frame_counter.debug_set_five_step_extra_cycle(true);

        apu.dmc.debug_set_dma_pending(true);
        apu.dmc.debug_set_transfer_start_delay(2);

        apu.pulse1_mut().write_control(0x30); // queue halt=true
        apu.pulse1_mut().apply_pending_length_halt(); // apply
        apu.pulse1_mut().write_control(0x10); // queue halt=false

        let state = apu.capture_state();

        let mut restored = Apu::new_for_testing();
        restored.restore_state(&state);

        assert!((restored.sample_accumulator - 7.25).abs() < 1e-6);
        assert!((restored.cycles_per_sample - 123.0).abs() < 1e-6);
        assert!(!restored.pulse1_enabled);
        assert!(restored.pulse2_enabled);
        assert!(!restored.triangle_enabled);
        assert!(restored.noise_enabled);
        assert!(!restored.dmc_enabled);

        assert_eq!(restored.apu_cycle, 1234);
        assert_eq!(restored.cpu_cycle, 5678);
        assert_eq!(restored.last_4017_write, 0xC0);

        assert_eq!(restored.frame_counter.debug_pending_write(), Some(0x80));
        assert_eq!(restored.frame_counter.debug_write_delay(), 3);
        assert!(
            restored
                .frame_counter
                .debug_pending_write_on_odd_cpu_cycle()
        );
        assert_eq!(
            restored.frame_counter.debug_pending_immediate_clock(),
            (true, false)
        );
        assert_eq!(
            restored.frame_counter.debug_irq_assert_cycles_remaining(),
            2
        );
        assert!(restored.frame_counter.debug_five_step_extra_cycle());

        assert!(restored.dmc.dma_pending());
        assert_eq!(restored.dmc.debug_transfer_start_delay(), 2);

        assert!(restored.pulse1().debug_length_counter_halt());
        assert_eq!(
            restored.pulse1().debug_length_counter_pending_halt(),
            Some(false)
        );

        let sample1 = restored.get_sample();
        let sample2 = restored.get_sample();
        let sample3 = restored.get_sample();

        assert!(matches!(sample1, Some(value) if (value - 0.1).abs() < 1e-6));
        assert!(matches!(sample2, Some(value) if (value - 0.2).abs() < 1e-6));
        assert!(sample3.is_none());
    }

    #[test]
    fn test_4017_write_takes_effect_after_3_cycles_when_during_apu_cycle() {
        let mut apu = Apu::new_for_testing();

        // Make sure we're well away from any frame counter wrap.
        for _ in 0..100 {
            apu.clock();
        }

        let before = apu.frame_counter().get_cycle_counter();
        assert!(before >= 100);

        // $4017 write should NOT take effect immediately; it takes effect after 3 CPU cycles
        // when written during an APU cycle.
        apu.write_frame_counter(0x80);

        // Still not reset after 0,1,2 cycles.
        for _ in 0..2 {
            apu.clock();
            assert!(
                apu.frame_counter().get_cycle_counter() >= before,
                "Frame counter should not have been reset yet"
            );
        }

        // Takes effect on the 3rd cycle after the write.
        apu.clock();
        assert!(
            apu.frame_counter().get_cycle_counter() < 10,
            "Frame counter should have been reset by the delayed $4017 write"
        );
    }

    #[test]
    fn test_4017_write_takes_effect_after_4_cycles_when_between_apu_cycles() {
        let mut apu = Apu::new_for_testing();

        // Move to the in-between (odd) CPU cycle relative to the APU cycle.
        apu.clock();

        // Make sure we're well away from any frame counter wrap.
        for _ in 0..100 {
            apu.clock();
        }

        let before = apu.frame_counter().get_cycle_counter();
        assert!(before >= 100);

        // $4017 write should take effect after 4 CPU cycles when written between APU cycles.
        apu.write_frame_counter(0x80);

        // Still not reset after 0,1,2,3 cycles.
        for _ in 0..3 {
            apu.clock();
            assert!(
                apu.frame_counter().get_cycle_counter() >= before,
                "Frame counter should not have been reset yet"
            );
        }

        // Takes effect on the 4th cycle after the write.
        apu.clock();
        assert!(
            apu.frame_counter().get_cycle_counter() < 10,
            "Frame counter should have been reset by the delayed $4017 write"
        );
    }

    #[test]
    fn test_4017_odd_jitter_delays_effective_reset_by_one_cycle() {
        let mut apu = Apu::new_for_testing();

        // Move to the in-between (odd) CPU cycle relative to the APU cycle.
        // This should produce the "odd jitter" behavior that shifts the frame counter by 1.
        apu.clock();

        apu.write_frame_counter(0x00);

        // When written on an odd CPU cycle, the $4017 write takes effect after 4 cycles.
        for _ in 0..4 {
            apu.clock();
        }

        // Correct jitter behavior: on odd-cycle writes the reset is effectively delayed by 1
        // cycle, meaning the frame counter starts at 0 on the effect cycle (not 1).
        assert_eq!(apu.frame_counter().get_cycle_counter(), 0);
    }

    #[test]
    fn test_length_halt_change_applies_after_half_frame_clock() {
        let mut apu = Apu::new_for_testing();

        apu.write_enable(STATUS_PULSE1);
        apu.pulse1_mut().write_control(0x10); // unhalt
        write_pulse1_length(&mut apu, 0x18); // length index 3 -> 2

        while apu.frame_counter().get_cycle_counter() < 14912 {
            apu.clock();
        }

        // Writing halt just before the half-frame clock should not prevent this clock.
        apu.pulse1_mut().write_control(0x30); // halt
        apu.clock(); // half-frame at 14913

        assert_eq!(
            apu.pulse1().get_length_counter(),
            1,
            "halt should apply after the half-frame length clock"
        );
    }

    #[test]
    fn test_length_halt_applies_before_immediate_half_frame_on_4017_write() {
        let mut apu = Apu::new_for_testing();

        apu.write_enable(STATUS_PULSE1);
        apu.pulse1_mut().write_control(0x10); // unhalt
        write_pulse1_length(&mut apu, 0x18); // length index 3 -> 2
        assert_eq!(apu.pulse1().get_length_counter(), 2);

        apu.pulse1_mut().write_control(0x30); // halt (pending)
        apu.frame_counter_mut().write_register(0x80); // immediate quarter+half clock next tick
        apu.clock();

        assert_eq!(
            apu.pulse1().get_length_counter(),
            1,
            "halt should apply after the immediate half-frame length clock"
        );
    }

    #[test]
    fn test_length_reload_during_half_frame_with_nonzero_counter_is_ignored() {
        let mut apu = Apu::new_for_testing();

        apu.write_enable(STATUS_PULSE1);
        apu.pulse1_mut().write_control(0x10); // unhalt
        write_pulse1_length(&mut apu, 0x38); // length index 7 -> 6

        while apu.frame_counter().get_cycle_counter() < 14912 {
            apu.clock();
        }

        // Write during the half-frame length clock (same CPU cycle).
        apu.pulse1_mut().write_length_counter_timer_high(0x18); // length index 3 -> 2
        apu.clock(); // half-frame at 14913 (length clocks + pending reload apply)

        assert_eq!(
            apu.pulse1().get_length_counter(),
            5,
            "reload during length clock with nonzero counter should be ignored"
        );
    }

    #[test]
    fn test_length_reload_during_half_frame_with_zero_counter_is_allowed() {
        let mut apu = Apu::new_for_testing();

        apu.write_enable(STATUS_PULSE1);
        apu.pulse1_mut().write_control(0x10); // unhalt
        write_pulse1_length(&mut apu, 0x38); // length index 7 -> 6

        apu.write_enable(0x00); // disable, clearing length counter
        apu.write_enable(STATUS_PULSE1); // re-enable with length=0

        while apu.frame_counter().get_cycle_counter() < 14912 {
            apu.clock();
        }

        // Write during the half-frame length clock (same CPU cycle).
        apu.pulse1_mut().write_length_counter_timer_high(0x18); // length index 3 -> 2
        apu.clock(); // half-frame at 14913 (length clocks + pending reload apply)

        assert_eq!(
            apu.pulse1().get_length_counter(),
            2,
            "reload during length clock with zero counter should be allowed"
        );
    }

    #[test]
    fn test_soft_reset_positions_frame_counter_1_cycle_after_effective_4017_write_even_cpu_cycle() {
        let mut apu = Apu::new_for_testing();

        // Empirically (from tracing blargg's `4017_written`), we need the soft reset path to be
        // aligned one cycle earlier than the previous hypothesis.
        apu.reset(0, true);

        assert_eq!(apu.frame_counter().get_cycle_counter(), 1);
    }

    #[test]
    fn test_soft_reset_positions_frame_counter_1_cycle_after_effective_4017_write_odd_cpu_cycle() {
        let mut apu = Apu::new_for_testing();

        apu.reset(1, true);

        assert_eq!(apu.frame_counter().get_cycle_counter(), 1);
    }

    #[test]
    fn test_power_on_reset_advances_frame_counter_to_1_cycles_after_effective_4017_write() {
        let mut apu = Apu::new_for_testing();

        // Blargg's `4017_timing` describes power-on as:
        // - effective `$4017 = $00` write
        // - then a 9-12 CPU-cycle delay
        // - then reset-vector fetch / execution begins
        //
        // In our emulator, `Cpu::reset(false)` consumes 7 CPU cycles before the first
        // instruction fetch. Empirically for blargg's `4017_written`, we also need to account
        // for the opcode fetch cycle alignment, so we advance only 1 cycle here.
        apu.reset(0, false);

        assert_eq!(apu.frame_counter().get_cycle_counter(), 1);
    }

    #[test]
    fn test_apu_reset_distinguishes_power_on_from_soft_reset() {
        let mut apu = Apu::new_for_testing();

        // Set a non-default $4017 value.
        apu.write_frame_counter(0x80);

        // Soft reset should rewrite the last $4017 value.
        apu.reset(0, true);
        assert!(
            apu.frame_counter().get_mode(),
            "Soft reset should keep the last-written $4017 mode"
        );

        // Power-on reset should behave as if $4017=$00.
        apu.reset(0, false);
        assert!(
            !apu.frame_counter().get_mode(),
            "Power-on reset should behave as if $4017 was written with $00"
        );
    }

    #[test]
    fn test_apu_new() {
        let apu = Apu::new_for_testing();
        assert_eq!(apu.frame_counter().get_cycle_counter(), 0);
        assert_eq!(apu.pulse1().output(), 0);
        assert_eq!(apu.pulse2().output(), 0);
        assert_eq!(apu.triangle().output(), 0); // Triangle DAC starts at 0
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
        write_pulse1_length(&mut apu, 0b00010_000); // Index 2 = length 20

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
    fn test_sweep_immediate_half_frame_updates_period() {
        let mut apu = Apu::new_for_testing();

        apu.pulse1_mut().write_sweep(0b1000_1001); // enable, period=0, negate=1, shift=1
        apu.pulse1_mut().write_timer_low(16);
        apu.pulse1_mut().write_timer_high(0);

        apu.write_frame_counter(0b1100_0000); // 5-step + immediate quarter/half

        for _ in 0..4 {
            apu.clock();
        }

        assert_eq!(apu.pulse1().get_timer_period(), 16);
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
        write_pulse1_length(&mut apu, 0xFF);
        write_pulse2_length(&mut apu, 0xFF);

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
        write_triangle_length(&mut apu, 0x08); // Sets reload flag

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
    fn test_triangle_linear_counter_clocks_only_on_quarter_frames_in_5_step_mode() {
        let mut apu = Apu::new_for_testing();

        // Switch to 5-step mode via the public $4017 write path.
        // This models the delayed write + immediate clock side-effects correctly.
        apu.write_frame_counter(0b1000_0000);

        // Advance until the delayed $4017 write takes effect (frame counter reset).
        // We intentionally configure triangle AFTER this, so the immediate quarter+half clocks
        // from the mode switch don't affect the assertions below.
        let mut prev = apu.debug_frame_counter_cycle();
        for _ in 0..10 {
            apu.clock();
            let now = apu.debug_frame_counter_cycle();
            if now == 0 && prev != 0 {
                break;
            }
            prev = now;
        }
        assert_eq!(apu.debug_frame_counter_cycle(), 0);

        // Enable triangle and configure linear counter reload value = 3.
        apu.write_enable(STATUS_TRIANGLE);
        apu.triangle_mut().write_linear_counter(0b0000_0011); // reload=3, control off
        write_triangle_length(&mut apu, 0x00); // sets reload flag
        assert!(apu.triangle().is_linear_counter_reload_flag_set());

        // Before the first quarter frame, the linear counter must not change.
        for _ in 0..7456 {
            apu.clock();
        }
        assert_eq!(apu.triangle().get_linear_counter(), 0);
        assert!(apu.triangle().is_linear_counter_reload_flag_set());

        // At cycle 7457, quarter-frame clocks and reloads.
        apu.clock();
        assert_eq!(apu.triangle().get_linear_counter(), 3);
        assert!(!apu.triangle().is_linear_counter_reload_flag_set());

        // Next quarter frame is at cycle 14913. It should decrement by exactly 1 (not per CPU cycle).
        for _ in 0..7456 {
            apu.clock();
        }
        assert_eq!(apu.triangle().get_linear_counter(), 2);
    }

    #[test]
    fn test_triangle_length_counter_gets_clocked() {
        let mut apu = Apu::new_for_testing();

        // Load length counter (index 5 = value 4)
        apu.write_enable(STATUS_TRIANGLE);
        load_triangle_length(&mut apu, 5);
        assert_eq!(apu.triangle().get_length_counter(), 4);

        // Clock to first half frame (14913 cycles in 4-step mode)
        for _ in 0..14913 {
            apu.clock();
        }

        // Length counter should have decremented
        assert_eq!(apu.triangle().get_length_counter(), 3);
    }

    #[test]
    fn test_triangle_length_counter_clocks_only_on_half_frames_in_5_step_mode() {
        let mut apu = Apu::new_for_testing();

        // Switch to 5-step mode via the public $4017 write path.
        // This models the delayed write + immediate clock side-effects correctly.
        apu.write_frame_counter(0b1000_0000);

        // Advance until the delayed $4017 write takes effect (frame counter reset).
        // We intentionally configure triangle AFTER this, so the immediate quarter+half clocks
        // from the mode switch don't affect the assertions below.
        let mut prev = apu.debug_frame_counter_cycle();
        for _ in 0..10 {
            apu.clock();
            let now = apu.debug_frame_counter_cycle();
            if now == 0 && prev != 0 {
                break;
            }
            prev = now;
        }
        assert_eq!(apu.debug_frame_counter_cycle(), 0);

        apu.write_enable(STATUS_TRIANGLE);
        load_triangle_length(&mut apu, 3); // index 3 => length 2
        assert_eq!(apu.triangle().get_length_counter(), 2);

        // At cycle 7457 (quarter frame only), length must not decrement.
        for _ in 0..7457 {
            apu.clock();
        }
        assert_eq!(apu.triangle().get_length_counter(), 2);

        // At cycle 14913 (half frame), length decrements.
        for _ in 0..7456 {
            apu.clock();
        }
        assert_eq!(apu.triangle().get_length_counter(), 1);

        // At cycle 22371 (quarter only), length must not decrement.
        for _ in 0..7458 {
            apu.clock();
        }
        assert_eq!(apu.triangle().get_length_counter(), 1);

        // At cycle 29829 (quarter only), length must not decrement.
        for _ in 0..7458 {
            apu.clock();
        }
        assert_eq!(apu.triangle().get_length_counter(), 1);

        // At cycle 37281 (half only), length decrements again.
        for _ in 0..7452 {
            apu.clock();
        }
        assert_eq!(apu.triangle().get_length_counter(), 0);
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
        write_noise_length(&mut apu, 0xFF); // Set length and envelope start flag

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
        write_noise_length(&mut apu, 0b00010_000); // Index 2

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
        write_pulse1_length(&mut apu, 0b00001_000); // Index 1 = length 254
        // Bit 0 should be set
        assert_eq!(apu.read_status(0) & 0b0000_0001, 0b0000_0001);
    }

    #[test]
    fn test_status_pulse2_active() {
        let mut apu = Apu::new_for_testing();
        // Enable pulse 2 channel first
        apu.write_enable(STATUS_PULSE2);
        // Load length counter for pulse 2
        write_pulse2_length(&mut apu, 0b00001_000); // Index 1 = length 254
        // Bit 1 should be set
        assert_eq!(apu.read_status(0) & 0b0000_0010, 0b0000_0010);
    }

    #[test]
    fn test_status_triangle_active() {
        let mut apu = Apu::new_for_testing();
        // Enable triangle channel first
        apu.write_enable(STATUS_TRIANGLE);
        // Load length counter for triangle
        load_triangle_length(&mut apu, 1); // Index 1 = length 254
        // Bit 2 should be set
        assert_eq!(apu.read_status(0) & 0b0000_0100, 0b0000_0100);
    }

    #[test]
    fn test_status_noise_active() {
        let mut apu = Apu::new_for_testing();
        // Enable noise channel first
        apu.write_enable(STATUS_NOISE);
        // Load length counter for noise (index 1 = length 254)
        write_noise_length(&mut apu, 0b00001_000);
        // Bit 3 should be set
        assert_eq!(apu.read_status(0) & 0b0000_1000, 0b0000_1000);
    }

    #[test]
    fn test_status_all_channels_active() {
        let mut apu = Apu::new_for_testing();
        // Load length counters for all channels
        apu.write_enable(0b0001_1111);
        write_pulse1_length(&mut apu, 0b00001_000);
        write_pulse2_length(&mut apu, 0b00001_000);
        load_triangle_length(&mut apu, 1);
        write_noise_length(&mut apu, 0b00001_000);
        // Bits 0-3 should be set (no DMC, no interrupts yet)
        assert_eq!(apu.read_status(0) & 0b0000_1111, 0b0000_1111);
    }

    #[test]
    fn test_enable_disable_pulse1() {
        let mut apu = Apu::new_for_testing();
        // Load pulse 1 length counter
        apu.write_enable(STATUS_PULSE1);
        write_pulse1_length(&mut apu, 0b00001_000);
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
        write_pulse1_length(&mut apu, 0b00001_000);
        assert_eq!(apu.read_status(0) & STATUS_PULSE1, STATUS_PULSE1);
    }

    #[test]
    fn test_enable_all_channels() {
        let mut apu = Apu::new_for_testing();
        // Enable all channels
        apu.write_enable(0b0001_1111);
        // Load all length counters
        write_pulse1_length(&mut apu, 0b00001_000);
        write_pulse2_length(&mut apu, 0b00001_000);
        load_triangle_length(&mut apu, 1);
        write_noise_length(&mut apu, 0b00001_000);
        // All should be active
        assert_eq!(apu.read_status(0) & 0b0000_1111, 0b0000_1111);
    }

    #[test]
    fn test_disable_clears_length_counters() {
        let mut apu = Apu::new_for_testing();
        // Load all length counters
        apu.write_enable(0b0001_1111);
        write_pulse1_length(&mut apu, 0b00001_000);
        write_pulse2_length(&mut apu, 0b00001_000);
        load_triangle_length(&mut apu, 1);
        write_noise_length(&mut apu, 0b00001_000);
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
    fn test_disable_dmc_clears_bytes_remaining_after_disable_delay() {
        let mut apu = Apu::new_for_testing();
        // Set up and enable DMC
        apu.dmc_mut().write_sample_address(0x00);
        apu.dmc_mut().write_sample_length(0x01);
        apu.write_enable(STATUS_DMC);
        assert!(apu.dmc().has_bytes_remaining());
        assert_eq!(apu.read_status(0) & STATUS_DMC, STATUS_DMC);

        // Disable DMC
        apu.write_enable(0b0000_0000);
        assert!(apu.dmc().has_bytes_remaining());
        assert_eq!(apu.read_status(0) & STATUS_DMC, STATUS_DMC);

        apu.clock();
        assert!(apu.dmc().has_bytes_remaining());
        assert_eq!(apu.read_status(0) & STATUS_DMC, STATUS_DMC);

        apu.clock();
        assert!(!apu.dmc().has_bytes_remaining());
        assert_eq!(apu.read_status(0) & STATUS_DMC, 0);
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
        write_pulse1_length(&mut apu, 0b00001_000); // Load length counter
        // Pulse generates square wave, output should be non-zero when high
        let output = apu.mix();
        assert!(output > 0.0);
        assert!(output <= 1.0);
    }

    #[test]
    fn test_mixer_isolation_triangle_only_no_bleed_from_muted_channels() {
        // Arrange: mute all channels except triangle (debug mixer mutes),
        // and make triangle output non-zero.
        let mut triangle_only = Apu::new_for_testing();
        triangle_only.set_pulse1_enabled(false);
        triangle_only.set_pulse2_enabled(false);
        triangle_only.set_noise_enabled(false);
        triangle_only.set_dmc_enabled(false);
        triangle_only.set_triangle_enabled(true);

        triangle_only.write_enable(STATUS_TRIANGLE);
        triangle_only.triangle_mut().write_linear_counter(0x7F);
        triangle_only.triangle_mut().trigger_linear_counter_reload();
        write_triangle_length(&mut triangle_only, 0b00001_000);
        triangle_only.triangle_mut().clock_timer();

        let baseline = triangle_only.mix();
        assert!(baseline > 0.0, "Expected non-zero triangle-only mix output");

        // Arrange: same triangle setup, but configure other channels to be "loud"
        // while keeping them muted by the debug mixer flags.
        let mut loud_but_muted = Apu::new_for_testing();
        loud_but_muted.set_pulse1_enabled(false);
        loud_but_muted.set_pulse2_enabled(false);
        loud_but_muted.set_noise_enabled(false);
        loud_but_muted.set_dmc_enabled(false);
        loud_but_muted.set_triangle_enabled(true);

        loud_but_muted.write_enable(STATUS_TRIANGLE);
        loud_but_muted.triangle_mut().write_linear_counter(0x7F);
        loud_but_muted
            .triangle_mut()
            .trigger_linear_counter_reload();
        write_triangle_length(&mut loud_but_muted, 0b00001_000);
        loud_but_muted.triangle_mut().clock_timer();

        // Pulse channels (would be non-zero if enabled)
        loud_but_muted.pulse1_mut().write_control(0b1111_1111);
        loud_but_muted.pulse1_mut().write_timer_low(0x08);
        write_pulse1_length(&mut loud_but_muted, 0b00001_000);
        loud_but_muted.pulse2_mut().write_control(0b1111_1111);
        loud_but_muted.pulse2_mut().write_timer_low(0x08);
        write_pulse2_length(&mut loud_but_muted, 0b00001_000);

        // Noise channel (would be non-zero if enabled)
        loud_but_muted.noise_mut().write_envelope(0b0011_1111);
        write_noise_length(&mut loud_but_muted, 0xFF);

        // DMC channel (would be non-zero if not muted)
        loud_but_muted.dmc_mut().write_direct_load(0b0111_1111);

        let with_muted_channels_configured = loud_but_muted.mix();

        // Assert: muted channels contribute nothing to the mix.
        assert!(
            (with_muted_channels_configured - baseline).abs() < 1e-9,
            "Muted channels must not affect mixer output"
        );
    }

    #[test]
    fn test_mixer_output_range() {
        let mut apu = Apu::new_for_testing();
        // Set all channels to max with duty 3 (starts high) for pulse channels
        apu.pulse1_mut().write_control(0b1111_1111); // Duty 3, constant volume 15
        apu.pulse1_mut().write_timer_low(0x08); // Timer period >= 8
        write_pulse1_length(&mut apu, 0b00001_000);
        apu.pulse2_mut().write_control(0b1111_1111); // Duty 3, constant volume 15
        apu.pulse2_mut().write_timer_low(0x08); // Timer period >= 8
        write_pulse2_length(&mut apu, 0b00001_000);
        apu.triangle_mut().write_linear_counter(0xFF);
        write_triangle_length(&mut apu, 0xFF);
        apu.noise_mut().write_envelope(0b0011_1111);
        write_noise_length(&mut apu, 0xFF);
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
        write_pulse1_length(&mut apu, 0b00001_000);
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
        write_pulse1_length(&mut apu, 0b00001_000);

        // Clock enough to generate multiple samples - at least one should be non-zero
        // With duty 2 (50%), half the samples should be non-zero
        let mut non_zero_found = false;
        for _ in 0..200 {
            apu.clock();
            if let Some(sample) = apu.get_sample()
                && sample > 0.0
            {
                non_zero_found = true;
                assert!(sample <= 1.0);
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
        assert!((43..=45).contains(&sample_count));
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
        assert!((47..=49).contains(&sample_count));
    }

    #[test]
    fn test_sample_generation_does_not_drop_samples_when_not_polled_each_cycle() {
        let mut apu = Apu::new_for_testing();

        // Clock for 1789 cycles (~44 samples at 44100 Hz) without polling samples.
        for _ in 0..1789 {
            apu.clock();
        }

        // Now drain everything that was generated.
        let mut sample_count = 0;
        while apu.sample_ready() {
            apu.get_sample();
            sample_count += 1;
        }

        // Should still have generated approximately 44 samples.
        assert!((43..=45).contains(&sample_count));
    }

    #[test]
    fn test_sample_generation_pending_queue_is_bounded() {
        let mut apu = Apu::new_for_testing();

        // Generate more than the max queued samples without polling.
        // ~41 CPU cycles per sample at 44100 Hz.
        let samples_to_generate = MAX_PENDING_SAMPLES + 100;
        for _ in 0..(samples_to_generate * 41) {
            apu.clock();
        }

        // Drain what's available; it must be capped.
        let mut drained = 0usize;
        while apu.sample_ready() {
            apu.get_sample();
            drained += 1;
        }

        assert_eq!(drained, MAX_PENDING_SAMPLES);
    }

    #[test]
    fn test_sample_generation_pending_queue_drops_oldest() {
        let mut apu = Apu::new_for_testing();

        for i in 0..(MAX_PENDING_SAMPLES + 2) {
            apu.push_sample_for_test(i as f32);
        }

        let first = apu.get_sample().expect("sample should be available");
        assert_eq!(first, 2.0);
    }
}
