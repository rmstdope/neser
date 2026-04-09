//! NES APU DMC (Delta Modulation Channel)
//!
//! The DMC plays 1-bit delta-encoded samples from CPU memory.
//! Components:
//! - Timer with 16 NTSC rate periods
//! - Memory reader (reads from CPU memory $C000+)
//! - Sample buffer (8-bit)
//! - Output unit (shift register + 7-bit output level 0-127)
//! - IRQ flag
//! - Loop flag for sample restart
use crate::nes::console::TimingMode;
use crate::trace_apu;
use serde::{Deserialize, Serialize};

/// APU DMC channel state for save-state support.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DmcState {
    pub timer: u16,
    pub timer_period: u16,
    pub output_level: u8,
    pub sample_address: u16,
    pub sample_length: u16,
    pub current_address: u16,
    pub bytes_remaining: u16,
    pub sample_buffer: Option<u8>,
    pub shift_register: u8,
    pub bits_remaining: u8,
    pub silence_flag: bool,
    pub irq_enabled: bool,
    pub irq_flag: bool,
    pub loop_flag: bool,
    pub dma_pending: bool,
    #[serde(default)]
    pub dma_pending_deferred: bool,
    pub transfer_start_delay: u8,
    #[serde(default)]
    pub disable_delay: u8,
}

// NTSC rate periods (in CPU cycles)
const DMC_RATE_TABLE_NTSC: [u16; 16] = [
    428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54,
];

// PAL rate periods (in CPU cycles)
const DMC_RATE_TABLE_PAL: [u16; 16] = [
    398, 354, 316, 298, 276, 236, 210, 198, 176, 148, 132, 118, 98, 78, 66, 50,
];

pub struct Dmc {
    tv_system: TimingMode,
    // Timer
    timer: u16,
    timer_period: u16,

    // Flags (from $4010)
    irq_enabled: bool,
    loop_flag: bool,

    // Output level (0-127, 7-bit)
    output_level: u8,

    // Sample buffer
    sample_buffer: Option<u8>,

    // Shift register and bits remaining
    shift_register: u8,
    bits_remaining: u8,
    silence_flag: bool,

    // Memory reader
    sample_address: u16,
    sample_length: u16,
    current_address: u16,
    bytes_remaining: u16,

    dma_pending: bool,
    dma_pending_deferred: bool,

    // IRQ
    interrupt_flag: bool,

    #[cfg(test)]
    irq_trigger_count: u32,

    // Transfer start delay (1-2 cycles after enabling DMC via $4015)
    transfer_start_delay: u8,

    // Disable delay (2-3 cycles after disabling DMC via $4015)
    disable_delay: u8,
}

impl Default for Dmc {
    fn default() -> Self {
        Self::new()
    }
}

impl Dmc {
    pub fn new() -> Self {
        Self::new_with_tv_system(TimingMode::Ntsc)
    }

    pub fn new_with_tv_system(tv_system: TimingMode) -> Self {
        let timer_period = match tv_system {
            TimingMode::Ntsc | TimingMode::Dendy => DMC_RATE_TABLE_NTSC[0],
            TimingMode::Pal => DMC_RATE_TABLE_PAL[0],
            TimingMode::MultiRegion | TimingMode::Unknown(_) => DMC_RATE_TABLE_NTSC[0],
        };

        Dmc {
            tv_system,
            timer: timer_period.saturating_sub(1),
            timer_period,
            irq_enabled: false,
            loop_flag: false,
            output_level: 0,
            sample_buffer: None,
            shift_register: 0,
            bits_remaining: 8,
            silence_flag: true,
            sample_address: 0,
            sample_length: 0,
            current_address: 0,
            bytes_remaining: 0,
            dma_pending: false,
            dma_pending_deferred: false,
            interrupt_flag: false,
            #[cfg(test)]
            irq_trigger_count: 0,
            transfer_start_delay: 0,
            disable_delay: 0,
        }
    }

    /// Reset DMC channel to initial state
    pub fn reset(&mut self) {
        trace_apu!(2; "dmc reset");
        let tv_system = self.tv_system;
        *self = Self::new_with_tv_system(tv_system);
    }

    /// Reinitialize the timer counter to the full period value.
    ///
    /// This is called after the CPU reset sequence completes. The CPU reset
    /// runs 7 internal cycles that clock the APU, which would decrement the
    /// timer away from its initial value. On real hardware (and in catch-up
    /// emulators like Mesen), the timer effectively starts counting from the
    /// full period when user code begins, so we restore it here.
    pub fn reinit_timer_after_reset(&mut self) {
        self.timer = self.timer_period.saturating_sub(1);
    }

    /// Returns true if the DMC has a pending DMA request for the next sample byte.
    #[cfg(test)]
    pub fn dma_pending(&self) -> bool {
        self.dma_pending
    }

    /// Returns true if the CPU should service the pending DMA request this read cycle.
    pub fn cpu_dma_pending(&self) -> bool {
        self.dma_pending && !self.dma_pending_deferred
    }

    #[cfg(test)]
    pub fn debug_timer(&self) -> u16 {
        self.timer
    }

    #[cfg(test)]
    pub fn debug_set_dma_pending(&mut self, value: bool) {
        self.dma_pending = value;
        self.dma_pending_deferred = false;
    }

    #[cfg(test)]
    pub fn debug_set_transfer_start_delay(&mut self, value: u8) {
        self.transfer_start_delay = value;
    }

    #[cfg(test)]
    pub fn debug_transfer_start_delay(&self) -> u8 {
        self.transfer_start_delay
    }

    /// If a DMA request is pending, returns the address the DMA should read.
    pub fn dma_address(&self) -> Option<u16> {
        (self.dma_pending && self.bytes_remaining > 0).then_some(self.current_address)
    }

    /// Complete a pending DMA read by supplying the fetched byte.
    pub fn complete_dma_read(&mut self, value: u8) {
        if !self.dma_pending {
            return;
        }

        self.dma_pending = false;
        self.dma_pending_deferred = false;
        self.sample_buffer = Some(value);
        trace_apu!(4; "dmc sample_buffer set value=0x{:02X}", value);
        trace_apu!(3; "dmc complete_dma_read value=0x{:02X}", value);
        self.advance_reader_after_fetch();
    }

    /// Get current output level (0-127)
    pub fn output(&self) -> u8 {
        self.output_level
    }

    /// Write to flags and rate register ($4010)
    /// Format: IL--.RRRR
    /// I = IRQ enable, L = loop flag, R = rate index (0-15)
    pub fn write_flags_and_rate(&mut self, value: u8) {
        self.irq_enabled = (value >> 7) & 1 == 1;
        self.loop_flag = (value >> 6) & 1 == 1;
        let rate_index = (value & 0x0F) as usize;
        self.timer_period = match self.tv_system {
            TimingMode::Ntsc | TimingMode::Dendy => DMC_RATE_TABLE_NTSC[rate_index],
            TimingMode::Pal => DMC_RATE_TABLE_PAL[rate_index],
            TimingMode::MultiRegion | TimingMode::Unknown(_) => DMC_RATE_TABLE_NTSC[rate_index],
        };

        trace_apu!(2; "dmc write_flags_and_rate value=0x{:02X} irq_enabled={} loop={} rate_index={} period={}", value, self.irq_enabled, self.loop_flag, rate_index, self.timer_period);

        // If IRQ is disabled, clear the interrupt flag
        if !self.irq_enabled {
            self.interrupt_flag = false;
        }
    }

    /// Write to direct load register ($4011)
    /// Format: -DDD.DDDD (7-bit output level)
    pub fn write_direct_load(&mut self, value: u8) {
        self.output_level = value & 0x7F;
        trace_apu!(2; "dmc write_direct_load value=0x{:02X} output_level={}", value, self.output_level);
    }

    /// Write to sample address register ($4012)
    /// Format: AAAA.AAAA
    /// Sample address = $C000 + (A * 64)
    pub fn write_sample_address(&mut self, value: u8) {
        self.sample_address = 0xC000 + (value as u16 * 64);
        trace_apu!(2; "dmc write_sample_address value=0x{:02X} address=0x{:04X}", value, self.sample_address);
    }

    /// Write to sample length register ($4013)
    /// Format: LLLL.LLLL
    /// Sample length = (L * 16) + 1 bytes
    pub fn write_sample_length(&mut self, value: u8) {
        self.sample_length = (value as u16 * 16) + 1;
        trace_apu!(2; "dmc write_sample_length value=0x{:02X} length={}", value, self.sample_length);
    }

    /// Clock the timer. When it reaches zero, clock the output unit.
    pub fn clock_timer(&mut self) {
        // Memory reader runs independently and keeps the one-byte sample buffer filled.
        // If the buffer is empty and there are bytes remaining, the DMC will fetch a byte.
        // Blargg's `apu_test/7-dmc_basics` depends on this happening immediately when empty.
        // However, we skip this if transfer_start_delay is active (just enabled via $4015)
        if self.transfer_start_delay == 0 {
            self.fill_sample_buffer_if_needed(false);
        }

        // The DMC rate table values are expressed in CPU cycles per output-unit clock.
        // To achieve an exact period of `timer_period`, we reload to `timer_period - 1`
        // because the timer is decremented once per CPU cycle and only triggers when it
        // is observed at zero.
        if self.timer == 0 {
            self.timer = self.timer_period.saturating_sub(1);
            self.clock_output_unit();
            if self.transfer_start_delay == 0 {
                self.fill_sample_buffer_if_needed(true);
            }
            trace_apu!(4; "dmc clock_timer reload period={} output_level={}", self.timer_period, self.output_level);
        } else {
            self.timer -= 1;
        }
    }

    fn fill_sample_buffer_if_needed(&mut self, defer_dma_visibility: bool) {
        if self.sample_buffer.is_some() {
            return;
        }
        if self.bytes_remaining == 0 {
            return;
        }

        // Request a CPU-side DMA read. The CPU will stall and provide the byte.
        if !self.dma_pending {
            self.dma_pending = true;
            self.dma_pending_deferred = defer_dma_visibility;
            trace_apu!(4; "dmc dma_pending address=0x{:04X} bytes_remaining={}", self.current_address, self.bytes_remaining);
        }
    }

    fn advance_reader_after_fetch(&mut self) {
        // Guard against underflow - this can happen if DMA completes after
        // bytes_remaining was cleared (e.g., by disabling the channel)
        if self.bytes_remaining == 0 {
            return;
        }

        // Advance to next byte
        self.current_address = self.current_address.wrapping_add(1);
        // Wrap address at $FFFF to $8000
        if self.current_address == 0x0000 {
            self.current_address = 0x8000;
        }

        // Decrement bytes remaining and handle completion
        self.bytes_remaining -= 1;
        trace_apu!(4; "dmc advance_reader address=0x{:04X} bytes_remaining={}", self.current_address, self.bytes_remaining);

        if self.bytes_remaining == 0 {
            // Sample finished (at end of memory fetch of the last byte)
            if self.loop_flag {
                // Loop: restart the sample
                self.restart_sample();
            } else if self.irq_enabled {
                // No loop: set IRQ flag if enabled
                self.interrupt_flag = true;
                #[cfg(test)]
                {
                    self.irq_trigger_count += 1;
                }
                trace_apu!(3; "dmc irq_flag set (sample complete)");
            }
        }
    }

    /// Clock the output unit (processes one bit from shift register)
    fn clock_output_unit(&mut self) {
        // Step 1: If silence flag is clear, update output level based on bit 0
        if !self.silence_flag {
            let bit0 = self.shift_register & 1;
            trace_apu!(4; "dmc clock_output_unit bit0={}", bit0);
            if bit0 == 1 {
                // Add 2, but only if output level <= 125
                if self.output_level <= 125 {
                    self.output_level += 2;
                }
            } else {
                // Subtract 2, but only if output level >= 2
                if self.output_level >= 2 {
                    self.output_level -= 2;
                }
            }
        }

        // Step 2: Shift the register right
        self.shift_register >>= 1;

        // Step 3: Decrement bits remaining counter
        if self.bits_remaining > 0 {
            self.bits_remaining -= 1;
        }

        trace_apu!(4; "dmc clock_output_unit shift_register=0x{:02X}, bits_remaining={}", self.shift_register, self.bits_remaining);
        // When bits remaining reaches 0, start a new output cycle
        if self.bits_remaining == 0 {
            self.start_output_cycle();
        }
    }

    /// Start a new output cycle
    fn start_output_cycle(&mut self) {
        self.bits_remaining = 8;

        // If sample buffer is empty, set silence flag
        // Otherwise, load sample buffer into shift register
        if let Some(sample) = self.sample_buffer {
            self.silence_flag = false;
            self.shift_register = sample;
            self.sample_buffer = None;
            trace_apu!(4; "dmc start_output_cycle shift=0x{:02X}", self.shift_register);
            trace_apu!(4; "dmc sample_buffer cleared (loaded into shift)");
        } else {
            self.silence_flag = true;
            trace_apu!(4; "dmc start_output_cycle silence");
        }
    }

    /// Restart the sample from the beginning
    fn restart_sample(&mut self) {
        self.current_address = self.sample_address;
        self.bytes_remaining = self.sample_length;
        trace_apu!(3; "dmc restart_sample address=0x{:04X} length={}", self.current_address, self.bytes_remaining);
    }

    fn delay_for_cpu_cycle(cpu_cycle: u64, even_cycle_delay: u8, odd_cycle_delay: u8) -> u8 {
        if cpu_cycle.is_multiple_of(2) {
            even_cycle_delay
        } else {
            odd_cycle_delay
        }
    }

    fn process_disable_delay(&mut self) {
        if self.disable_delay == 0 {
            return;
        }

        self.disable_delay -= 1;
        if self.disable_delay == 0 {
            self.bytes_remaining = 0;
            self.dma_pending = false;
            self.dma_pending_deferred = false;
            trace_apu!(4; "dmc disable_delay expired");
        }
    }

    fn process_transfer_start_delay(&mut self) {
        if self.transfer_start_delay == 0 {
            return;
        }

        self.transfer_start_delay -= 1;
        if self.transfer_start_delay == 0 {
            // Delay expired, now trigger DMA if buffer is still empty
            self.fill_sample_buffer_if_needed(false);
            trace_apu!(4; "dmc transfer_start_delay expired");
        }
    }

    /// Enable or disable the channel (called from $4015 status register)
    /// cpu_cycle is the current CPU cycle count for accurate delay timing
    pub fn set_enabled(&mut self, enabled: bool, cpu_cycle: u64) {
        trace_apu!(2; "dmc set_enabled {} cpu_cycle={}", enabled, cpu_cycle);
        if enabled {
            self.disable_delay = 0;

            // If bytes_remaining is 0, restart the sample
            if self.bytes_remaining == 0 {
                self.restart_sample();
                // Delay DMA request by 1-2 cycles based on odd/even CPU cycle
                self.transfer_start_delay = Self::delay_for_cpu_cycle(cpu_cycle, 1, 2);
                trace_apu!(4; "dmc transfer_start_delay {}", self.transfer_start_delay);
            }
        } else {
            // Disabling the DMC takes effect after a short CPU-cycle delay.
            if self.disable_delay == 0 {
                self.disable_delay = Self::delay_for_cpu_cycle(cpu_cycle, 2, 3);
            }

            if self.sample_buffer.is_some() {
                trace_apu!(4; "dmc sample_buffer retained (disable)");
            }
        }
    }

    /// Process one CPU clock cycle - handles transfer start delays
    /// Must be called once per CPU cycle to properly time DMC DMA requests
    pub fn process_clock(&mut self) {
        self.dma_pending_deferred = false;
        self.process_disable_delay();
        self.process_transfer_start_delay();
    }

    /// Simulate finishing a byte read (decrements bytes_remaining and handles loop/IRQ)
    /// In a real implementation, this would be called after reading from memory
    #[cfg(test)]
    fn finish_byte(&mut self) {
        if self.bytes_remaining > 0 {
            self.bytes_remaining -= 1;

            if self.bytes_remaining == 0 {
                // Sample finished
                if self.loop_flag {
                    // Loop: restart the sample
                    self.restart_sample();
                } else if self.irq_enabled {
                    // No loop: set IRQ flag if enabled
                    self.interrupt_flag = true;
                    #[cfg(test)]
                    {
                        self.irq_trigger_count += 1;
                    }
                }
            }
        }
    }

    /// Get the IRQ flag status
    pub fn get_irq_flag(&self) -> bool {
        self.interrupt_flag
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub fn debug_irq_trigger_count(&self) -> u32 {
        self.irq_trigger_count
    }

    /// Clear the IRQ flag (side effect of writing to $4015)
    pub fn clear_irq_flag(&mut self) {
        self.interrupt_flag = false;
        trace_apu!(3; "dmc clear_irq_flag");
    }

    /// Check if the channel has bytes remaining (for status register $4015 bit 4)
    pub fn has_bytes_remaining(&self) -> bool {
        self.bytes_remaining > 0
    }

    /// Get the number of bytes_remaining (for testing purposes)
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn get_bytes_remaining(&self) -> u16 {
        self.bytes_remaining
    }

    /// Capture the current DMC channel state for save-state.
    pub fn capture_state(&self) -> DmcState {
        DmcState {
            timer: self.timer,
            timer_period: self.timer_period,
            output_level: self.output_level,
            sample_address: self.sample_address,
            sample_length: self.sample_length,
            current_address: self.current_address,
            bytes_remaining: self.bytes_remaining,
            sample_buffer: self.sample_buffer,
            shift_register: self.shift_register,
            bits_remaining: self.bits_remaining,
            silence_flag: self.silence_flag,
            irq_enabled: self.irq_enabled,
            irq_flag: self.interrupt_flag,
            loop_flag: self.loop_flag,
            dma_pending: self.dma_pending,
            dma_pending_deferred: self.dma_pending_deferred,
            transfer_start_delay: self.transfer_start_delay,
            disable_delay: self.disable_delay,
        }
    }

    /// Restore DMC channel state from a save-state.
    pub fn restore_state(&mut self, state: &DmcState) {
        self.timer = state.timer;
        self.timer_period = state.timer_period;
        self.output_level = state.output_level;
        self.sample_address = state.sample_address;
        self.sample_length = state.sample_length;
        self.current_address = state.current_address;
        self.bytes_remaining = state.bytes_remaining;
        self.sample_buffer = state.sample_buffer;
        self.shift_register = state.shift_register;
        self.bits_remaining = state.bits_remaining;
        self.silence_flag = state.silence_flag;
        self.irq_enabled = state.irq_enabled;
        self.interrupt_flag = state.irq_flag;
        self.loop_flag = state.loop_flag;
        self.dma_pending = state.dma_pending;
        self.dma_pending_deferred = state.dma_pending_deferred;
        self.transfer_start_delay = state.transfer_start_delay;
        self.disable_delay = state.disable_delay;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::console::TimingMode;

    #[test]
    fn test_dmc_new() {
        let dmc = Dmc::new();
        assert_eq!(dmc.output(), 0);
        assert_eq!(dmc.timer_period, 428); // Rate 0
        assert_eq!(dmc.bits_remaining, 8);
        assert!(dmc.silence_flag);
    }

    #[test]
    fn test_dmc_pal_rate_table() {
        let mut dmc = Dmc::new_with_tv_system(TimingMode::Pal);

        // PAL rate 0
        assert_eq!(dmc.timer_period, 398);

        // PAL rate $F
        dmc.write_flags_and_rate(0x0F);
        assert_eq!(dmc.timer_period, 50);
    }

    #[test]
    fn test_write_flags_and_rate() {
        let mut dmc = Dmc::new();

        // $4010: IL--.RRRR
        // I = IRQ enable, L = loop flag, R = rate index
        dmc.write_flags_and_rate(0b1100_0101); // IRQ=1, loop=1, rate=5

        assert!(dmc.irq_enabled);
        assert!(dmc.loop_flag);
        assert_eq!(dmc.timer_period, 254); // Rate 5 from table
    }

    #[test]
    fn test_write_direct_load() {
        let mut dmc = Dmc::new();

        // $4011: -DDD.DDDD (7-bit output level)
        dmc.write_direct_load(0b0111_1111); // Max value 127

        assert_eq!(dmc.output(), 127);

        dmc.write_direct_load(0b0100_0000); // Value 64
        assert_eq!(dmc.output(), 64);
    }

    #[test]
    fn test_write_sample_address() {
        let mut dmc = Dmc::new();

        // $4012: AAAA.AAAA
        // Sample address = $C000 + (A * 64)
        dmc.write_sample_address(0x00); // $C000
        assert_eq!(dmc.sample_address, 0xC000);

        dmc.write_sample_address(0x01); // $C040
        assert_eq!(dmc.sample_address, 0xC040);

        dmc.write_sample_address(0xFF); // $FFC0
        assert_eq!(dmc.sample_address, 0xFFC0);
    }

    #[test]
    fn test_write_sample_length() {
        let mut dmc = Dmc::new();

        // $4013: LLLL.LLLL
        // Sample length = (L * 16) + 1 bytes
        dmc.write_sample_length(0x00); // 1 byte
        assert_eq!(dmc.sample_length, 1);

        dmc.write_sample_length(0x01); // 17 bytes
        assert_eq!(dmc.sample_length, 17);

        dmc.write_sample_length(0xFF); // 4081 bytes
        assert_eq!(dmc.sample_length, 4081);
    }

    #[test]
    fn test_timer_clocking() {
        let mut dmc = Dmc::new();
        dmc.write_flags_and_rate(0b0000_1111); // Rate $F = period 54

        // Timer starts at full period (rate 0 = 428-1 = 427), not at 0.
        // write_flags_and_rate only changes the period, not the running counter.
        assert_eq!(dmc.timer, 427);
        assert_eq!(dmc.timer_period, 54);

        // First clock counts down from the initial timer value
        dmc.clock_timer();
        assert_eq!(dmc.timer, 426);

        // Subsequent clocks continue counting down
        dmc.clock_timer();
        assert_eq!(dmc.timer, 425);
    }

    #[test]
    fn test_output_level_increment() {
        let mut dmc = Dmc::new();
        dmc.output_level = 50;
        dmc.shift_register = 0b0000_0001; // Bit 0 = 1
        dmc.silence_flag = false;
        dmc.bits_remaining = 1;

        // Clock output unit should increment by 2
        dmc.clock_output_unit();
        assert_eq!(dmc.output_level, 52);
    }

    #[test]
    fn test_output_level_decrement() {
        let mut dmc = Dmc::new();
        dmc.output_level = 50;
        dmc.shift_register = 0b0000_0000; // Bit 0 = 0
        dmc.silence_flag = false;
        dmc.bits_remaining = 1;

        // Clock output unit should decrement by 2
        dmc.clock_output_unit();
        assert_eq!(dmc.output_level, 48);
    }

    #[test]
    fn test_output_level_clamping() {
        let mut dmc = Dmc::new();
        dmc.silence_flag = false;
        dmc.bits_remaining = 5; // Keep it > 1 to avoid triggering new cycle

        // Test that we don't add when at 126 (> 125)
        dmc.output_level = 126;
        dmc.shift_register = 0b0000_0001; // Would add 2, but shouldn't
        dmc.clock_output_unit();
        assert_eq!(dmc.output_level, 126); // Stays at 126

        // Test exact upper limit (125 + 2 = 127)
        dmc.output_level = 125;
        dmc.shift_register = 0b0000_0001;
        dmc.bits_remaining = 5;
        dmc.clock_output_unit();
        assert_eq!(dmc.output_level, 127); // Should add 2

        // Test that we don't subtract when at 1 (< 2)
        dmc.output_level = 1;
        dmc.shift_register = 0b0000_0000; // Would subtract 2, but shouldn't
        dmc.bits_remaining = 5;
        dmc.clock_output_unit();
        assert_eq!(dmc.output_level, 1); // Stays at 1

        // Test exact lower limit (2 - 2 = 0)
        dmc.output_level = 2;
        dmc.shift_register = 0b0000_0000;
        dmc.bits_remaining = 5;
        dmc.clock_output_unit();
        assert_eq!(dmc.output_level, 0); // Should subtract 2
    }

    #[test]
    fn test_output_cycle_with_sample_buffer() {
        let mut dmc = Dmc::new();
        dmc.sample_buffer = Some(0b1010_1010);
        dmc.silence_flag = true;
        dmc.bits_remaining = 0;

        // Starting a new cycle should load the sample
        dmc.start_output_cycle();
        assert_eq!(dmc.shift_register, 0b1010_1010);
        assert!(!dmc.silence_flag);
        assert_eq!(dmc.bits_remaining, 8);
        assert!(dmc.sample_buffer.is_none());
    }

    #[test]
    fn test_output_cycle_without_sample_buffer() {
        let mut dmc = Dmc::new();
        dmc.sample_buffer = None;
        dmc.silence_flag = false;
        dmc.bits_remaining = 0;

        // Starting a new cycle with empty buffer sets silence
        dmc.start_output_cycle();
        assert!(dmc.silence_flag);
        assert_eq!(dmc.bits_remaining, 8);
    }

    #[test]
    fn test_clock_timer_raises_dma_pending_on_same_cycle_sample_buffer_is_consumed() {
        let mut dmc = Dmc::new();
        dmc.write_sample_address(0x00);
        dmc.current_address = 0xC000;
        dmc.bytes_remaining = 2;
        dmc.dma_pending = true;
        dmc.complete_dma_read(0xAA);
        dmc.bits_remaining = 1;
        dmc.timer = 0;
        dmc.transfer_start_delay = 0;
        dmc.dma_pending = false;

        assert_eq!(dmc.sample_buffer, Some(0xAA));
        assert!(!dmc.dma_pending());

        dmc.clock_timer();

        assert!(dmc.sample_buffer.is_none());
        assert!(
            dmc.dma_pending(),
            "expected dma_pending to be raised on the same clock_timer call that consumed the sample buffer"
        );
        assert!(!dmc.cpu_dma_pending());
    }

    #[test]
    fn test_deferred_rollover_dma_becomes_cpu_visible_next_cycle() {
        let mut dmc = Dmc::new();
        dmc.write_sample_address(0x00);
        dmc.current_address = 0xC000;
        dmc.bytes_remaining = 2;
        dmc.dma_pending = true;
        dmc.complete_dma_read(0xAA);
        dmc.bits_remaining = 1;
        dmc.timer = 0;
        dmc.transfer_start_delay = 0;
        dmc.dma_pending = false;

        dmc.clock_timer();

        assert!(dmc.dma_pending());
        assert!(!dmc.cpu_dma_pending());

        dmc.process_clock();

        assert!(dmc.cpu_dma_pending());
    }

    #[test]
    fn test_sample_buffer_retained_when_disabled() {
        let mut dmc = Dmc::new();
        dmc.sample_buffer = Some(0x55);

        dmc.set_enabled(false, 0);

        assert_eq!(dmc.sample_buffer, Some(0x55));
    }

    #[test]
    fn test_disable_channel_does_not_force_silence_flag() {
        let mut dmc = Dmc::new();
        dmc.silence_flag = false;
        dmc.shift_register = 0xAA;
        dmc.bits_remaining = 4;

        dmc.set_enabled(false, 0);

        assert!(!dmc.silence_flag);
        assert_eq!(dmc.shift_register, 0xAA);
        assert_eq!(dmc.bits_remaining, 4);
    }

    #[test]
    fn test_cold_start_timer_starts_at_full_period() {
        // On real hardware the timer counter starts at the full period value
        // so the first output-unit tick occurs after exactly `period` CPU cycles,
        // not immediately on the first clock_timer() call.
        let dmc = Dmc::new();
        let expected_timer = DMC_RATE_TABLE_NTSC[0] - 1; // 428 - 1 = 427
        assert_eq!(
            dmc.debug_timer(),
            expected_timer,
            "timer should start at full period ({expected_timer}), not 0"
        );
    }

    #[test]
    fn test_reset_timer_starts_at_full_period() {
        // After reset the timer phase should restart from full period,
        // same as cold start.
        let mut dmc = Dmc::new();
        // Drain the timer partially
        for _ in 0..10 {
            dmc.clock_timer();
        }
        assert_ne!(dmc.debug_timer(), DMC_RATE_TABLE_NTSC[0] - 1);

        dmc.reset();

        let expected_timer = DMC_RATE_TABLE_NTSC[0] - 1;
        assert_eq!(
            dmc.debug_timer(),
            expected_timer,
            "after reset, timer should restart at full period ({expected_timer})"
        );
    }

    #[test]
    fn test_first_dmc_tick_occurs_after_full_period_cycles() {
        // After cold start, the first output-unit tick must occur after
        // exactly `period` clock_timer() calls, not on the first call.
        let mut dmc = Dmc::new();
        // Use the default rate 0 period (428 CPU cycles)
        let period = DMC_RATE_TABLE_NTSC[0]; // 428

        // Set up a non-silent shift register so we can observe output changes.
        dmc.silence_flag = false;
        dmc.shift_register = 0xFF; // all 1-bits -> will increment output_level
        dmc.bits_remaining = 8;
        dmc.output_level = 0;

        // Clock (period - 1) times — timer should count down but NOT fire
        for i in 0..(period - 1) {
            dmc.clock_timer();
            assert_eq!(
                dmc.output_level, 0,
                "output level should not change before full period (cycle {i})"
            );
        }

        // On the period-th clock, timer reaches 0 and the output unit fires
        dmc.clock_timer();
        assert_eq!(
            dmc.output_level, 2,
            "output level should change on the {period}th cycle (full period)"
        );
    }

    #[test]
    fn test_dendy_dmc_power_on_uses_ntsc_rate_table() {
        // Dendy APU works with NTSC timings.
        // Spec: Mesen2 NesApu.cpp GetApuRegion() — Dendy routes to ConsoleRegion::Ntsc
        // NTSC rate[0]=428, PAL rate[0]=398 — they differ, use index 0.
        let dmc = Dmc::new_with_tv_system(TimingMode::Dendy);
        assert_eq!(
            dmc.timer_period, DMC_RATE_TABLE_NTSC[0],
            "Dendy DMC must use NTSC rate table at power-on (got {}, expected NTSC[0]={})",
            dmc.timer_period, DMC_RATE_TABLE_NTSC[0]
        );
    }

    #[test]
    fn test_dendy_dmc_write_flags_uses_ntsc_rate_table() {
        // At rate index 1: NTSC=380, PAL=354 — use write_flags_and_rate to distinguish.
        let mut dmc = Dmc::new_with_tv_system(TimingMode::Dendy);
        dmc.write_flags_and_rate(0b0000_0001); // rate_index=1
        assert_ne!(
            dmc.timer_period, DMC_RATE_TABLE_PAL[1],
            "Dendy DMC must NOT use PAL rate table"
        );
        assert_eq!(
            dmc.timer_period, DMC_RATE_TABLE_NTSC[1],
            "Dendy DMC must use NTSC rate table at index 1"
        );
    }
}

#[cfg(test)]
mod sample_tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::nes::cartridge::Cartridge;
    use crate::nes::console::{Config, Nes};

    fn make_ines_nrom_32k(prg_rom: &[u8]) -> Vec<u8> {
        assert_eq!(prg_rom.len(), 2 * 16 * 1024);
        let mut rom = Vec::with_capacity(16 + prg_rom.len());
        rom.extend_from_slice(b"NES\x1A");
        rom.push(2); // 2 * 16KB PRG
        rom.push(0); // 0 * 8KB CHR
        rom.push(0); // flags 6
        rom.push(0); // flags 7
        rom.push(0); // PRG-RAM size (unused)
        rom.push(0);
        rom.push(0);
        rom.extend_from_slice(&[0u8; 5]);
        rom.extend_from_slice(prg_rom);
        rom
    }

    #[test]
    fn test_restart_sample() {
        let mut dmc = Dmc::new();
        dmc.write_sample_address(0x10); // $C400
        dmc.write_sample_length(0x0F); // 241 bytes

        dmc.restart_sample();

        assert_eq!(dmc.current_address, 0xC400);
        assert_eq!(dmc.bytes_remaining, 241);
    }

    #[test]
    fn test_enable_channel_restarts_sample() {
        let mut dmc = Dmc::new();
        dmc.write_sample_address(0x20); // $C800
        dmc.write_sample_length(0x01); // 17 bytes
        dmc.bytes_remaining = 0; // Sample finished

        // Enable channel (called from $4015) - use even cycle for 2-cycle delay
        dmc.set_enabled(true, 0);

        assert_eq!(dmc.current_address, 0xC800);
        assert_eq!(dmc.bytes_remaining, 17);
    }

    #[test]
    fn test_disable_channel_waits_two_cycles_before_clearing_on_even_cpu_cycle() {
        let mut dmc = Dmc::new();
        dmc.bytes_remaining = 100;

        dmc.set_enabled(false, 0);

        assert_eq!(dmc.bytes_remaining, 100);

        dmc.process_clock();
        assert_eq!(dmc.bytes_remaining, 100);

        dmc.process_clock();
        assert_eq!(dmc.bytes_remaining, 0);
    }

    #[test]
    fn test_disable_channel_waits_three_cycles_before_clearing_on_odd_cpu_cycle() {
        let mut dmc = Dmc::new();
        dmc.bytes_remaining = 100;

        dmc.set_enabled(false, 1);

        assert_eq!(dmc.bytes_remaining, 100);

        dmc.process_clock();
        assert_eq!(dmc.bytes_remaining, 100);

        dmc.process_clock();
        assert_eq!(dmc.bytes_remaining, 100);

        dmc.process_clock();
        assert_eq!(dmc.bytes_remaining, 0);
    }

    #[test]
    fn test_irq_flag_set_when_sample_ends() {
        let mut dmc = Dmc::new();
        dmc.write_flags_and_rate(0b1000_0000); // Enable IRQ
        dmc.bytes_remaining = 1;
        dmc.loop_flag = false;

        // Simulate finishing the last byte
        dmc.finish_byte();

        assert!(dmc.interrupt_flag);
        assert_eq!(dmc.bytes_remaining, 0);
    }

    #[test]
    fn test_loop_restarts_sample() {
        let mut dmc = Dmc::new();
        dmc.write_flags_and_rate(0b0100_0000); // Enable loop
        dmc.write_sample_address(0x30); // $CC00
        dmc.write_sample_length(0x02); // 33 bytes
        dmc.current_address = 0xCC20;
        dmc.bytes_remaining = 1;

        // Simulate finishing the last byte with loop enabled
        dmc.finish_byte();

        // Should restart from beginning
        assert_eq!(dmc.current_address, 0xCC00);
        assert_eq!(dmc.bytes_remaining, 33);
        assert!(!dmc.interrupt_flag); // No IRQ when looping
    }

    #[test]
    fn test_irq_flag_cleared_when_disabled() {
        let mut dmc = Dmc::new();
        dmc.interrupt_flag = true;

        // Disable IRQ
        dmc.write_flags_and_rate(0b0000_0000);

        assert!(!dmc.interrupt_flag);
    }

    #[test]
    fn test_get_irq_flag() {
        let mut dmc = Dmc::new();
        assert!(!dmc.get_irq_flag());

        dmc.interrupt_flag = true;
        assert!(dmc.get_irq_flag());
    }

    #[test]
    fn test_get_bytes_remaining_status() {
        let mut dmc = Dmc::new();

        // When bytes_remaining > 0, channel is active
        dmc.bytes_remaining = 10;
        assert!(dmc.has_bytes_remaining());

        // When bytes_remaining = 0, channel is inactive
        dmc.bytes_remaining = 0;
        assert!(!dmc.has_bytes_remaining());
    }

    #[test]
    fn test_dmc_reads_sample_byte_from_cpu_memory() {
        // Arrange: build a tiny NROM-256 ROM with a known byte at CPU $C000.
        // For 32KB PRG ROM, CPU $8000 maps to PRG[0x0000] and CPU $C000 maps to PRG[0x4000].
        let mut prg = vec![0xEAu8; 2 * 16 * 1024]; // Fill with NOPs for safe CPU execution
        // Simple infinite loop at $8000: NOP; NOP; NOP; JMP $8000
        prg[0x0000] = 0xEA;
        prg[0x0001] = 0xEA;
        prg[0x0002] = 0xEA;
        prg[0x0003] = 0x4C;
        prg[0x0004] = 0x00;
        prg[0x0005] = 0x80;

        // DMC sample byte at CPU $C000
        prg[0x4000] = 0xFF;

        // Set vectors (NMI/RESET/IRQ) to $8000
        prg[0x7FFA] = 0x00;
        prg[0x7FFB] = 0x80;
        prg[0x7FFC] = 0x00;
        prg[0x7FFD] = 0x80;
        prg[0x7FFE] = 0x00;
        prg[0x7FFF] = 0x80;

        let rom = make_ines_nrom_32k(&prg);
        let mut nes = Nes::new(Rc::new(RefCell::new(
            crate::app_context::AppContext::new_with_config(Config::default()),
        )));
        let cartridge = Cartridge::load_from_file(&rom, "apu-dmc-test-rom.nes", Some(nes.rom_db()))
            .expect("test ROM should parse");

        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Configure DMC to play 1 byte starting at $C000, at the fastest rate.
        {
            let mut apu = nes.apu().borrow_mut();
            apu.dmc_mut().write_flags_and_rate(0x0F);
            apu.dmc_mut().write_sample_address(0x00);
            apu.dmc_mut().write_sample_length(0x00);
            apu.write_enable(0x10);
        }

        // Act: run enough CPU cycles for DMC to fetch and output bits.
        for _ in 0..5_000 {
            nes.run_cpu_tick();
        }

        // Assert: once the DMC reads 0xFF from $C000, output should have increased above 0.
        // This currently FAILS because the DMC memory reader is stubbed (uses 0x00).
        let output = nes.apu().borrow().dmc().output();
        assert!(
            output > 0,
            "expected DMC output to be > 0 after reading sample; got {output}"
        );
    }

    #[test]
    fn reset_restores_dmc_to_initial_state() {
        let mut dmc = Dmc::new();
        // Modify all fields
        dmc.write_flags_and_rate(0b1100_1111); // IRQ=1, loop=1, rate=15
        dmc.write_direct_load(0x40); // output_level=64
        dmc.write_sample_address(0x40); // address=$C000 + 0x40*64
        dmc.write_sample_length(0x10); // length=0x10*16+1
        dmc.interrupt_flag = true;
        dmc.sample_buffer = Some(0xAB);
        dmc.shift_register = 0xFF;
        dmc.bits_remaining = 3;
        dmc.silence_flag = false;
        dmc.dma_pending = true;
        dmc.bytes_remaining = 100;
        dmc.current_address = 0xD000;

        // Verify state changed
        assert!(dmc.get_irq_flag());
        assert_eq!(dmc.output(), 64);
        assert!(dmc.has_bytes_remaining());
        assert!(dmc.dma_pending());

        // Reset
        dmc.reset();

        // Verify all fields back to default
        assert!(!dmc.get_irq_flag());
        assert_eq!(dmc.output(), 0);
        assert!(!dmc.has_bytes_remaining());
        assert!(!dmc.dma_pending());
        assert_eq!(dmc.bits_remaining, 8);
    }
}
