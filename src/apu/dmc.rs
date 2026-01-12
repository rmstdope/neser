/// NES APU DMC (Delta Modulation Channel)
///
/// The DMC plays 1-bit delta-encoded samples from CPU memory.
/// Components:
/// - Timer with 16 NTSC rate periods
/// - Memory reader (reads from CPU memory $C000+)
/// - Sample buffer (8-bit)
/// - Output unit (shift register + 7-bit output level 0-127)
/// - IRQ flag
/// - Loop flag for sample restart

// NTSC rate periods (in CPU cycles)
const DMC_RATE_TABLE: [u16; 16] = [
    428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54,
];

pub struct Dmc {
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

    // IRQ
    interrupt_flag: bool,

    // Transfer start delay (2-3 cycles after enabling DMC via $4015)
    // This matches Mesen2 behavior for accurate timing
    transfer_start_delay: u8,
}

impl Dmc {
    pub fn new() -> Self {
        Dmc {
            timer: 0,
            timer_period: DMC_RATE_TABLE[0],
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
            interrupt_flag: false,
            transfer_start_delay: 0,
        }
    }

    /// Returns true if the DMC has a pending DMA request for the next sample byte.
    pub fn dma_pending(&self) -> bool {
        self.dma_pending
    }

    /// If a DMA request is pending, returns the address the DMA should read.
    pub fn dma_address(&self) -> Option<u16> {
        self.dma_pending.then_some(self.current_address)
    }

    /// Complete a pending DMA read by supplying the fetched byte.
    pub fn complete_dma_read(&mut self, value: u8) {
        if !self.dma_pending {
            return;
        }

        self.dma_pending = false;
        self.sample_buffer = Some(value);
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
        self.timer_period = DMC_RATE_TABLE[rate_index];

        // If IRQ is disabled, clear the interrupt flag
        if !self.irq_enabled {
            self.interrupt_flag = false;
        }
    }

    /// Write to direct load register ($4011)
    /// Format: -DDD.DDDD (7-bit output level)
    pub fn write_direct_load(&mut self, value: u8) {
        self.output_level = value & 0x7F;
    }

    /// Write to sample address register ($4012)
    /// Format: AAAA.AAAA
    /// Sample address = $C000 + (A * 64)
    pub fn write_sample_address(&mut self, value: u8) {
        self.sample_address = 0xC000 + (value as u16 * 64);
    }

    /// Write to sample length register ($4013)
    /// Format: LLLL.LLLL
    /// Sample length = (L * 16) + 1 bytes
    pub fn write_sample_length(&mut self, value: u8) {
        self.sample_length = (value as u16 * 16) + 1;
    }

    /// Clock the timer. When it reaches zero, clock the output unit.
    pub fn clock_timer(&mut self) {
        // Memory reader runs independently and keeps the one-byte sample buffer filled.
        // If the buffer is empty and there are bytes remaining, the DMC will fetch a byte.
        // Blargg's `apu_test/7-dmc_basics` depends on this happening immediately when empty.
        // However, we skip this if transfer_start_delay is active (just enabled via $4015)
        if self.transfer_start_delay == 0 {
            self.fill_sample_buffer_if_needed();
        }

        // The DMC rate table values are expressed in CPU cycles per output-unit clock.
        // To achieve an exact period of `timer_period`, we reload to `timer_period - 1`
        // because the timer is decremented once per CPU cycle and only triggers when it
        // is observed at zero.
        if self.timer == 0 {
            self.timer = self.timer_period.saturating_sub(1);
            self.clock_output_unit();
        } else {
            self.timer -= 1;
        }
    }

    fn fill_sample_buffer_if_needed(&mut self) {
        if self.sample_buffer.is_some() {
            return;
        }
        if self.bytes_remaining == 0 {
            return;
        }

        // Request a CPU-side DMA read. The CPU will stall and provide the byte.
        if !self.dma_pending {
            self.dma_pending = true;
        }
    }

    fn advance_reader_after_fetch(&mut self) {
        // Advance to next byte
        self.current_address = self.current_address.wrapping_add(1);
        // Wrap address at $FFFF to $8000
        if self.current_address == 0x0000 {
            self.current_address = 0x8000;
        }

        // Decrement bytes remaining and handle completion
        self.bytes_remaining -= 1;

        if self.bytes_remaining == 0 {
            // Sample finished (at end of memory fetch of the last byte)
            if self.loop_flag {
                // Loop: restart the sample
                self.restart_sample();
            } else if self.irq_enabled {
                // No loop: set IRQ flag if enabled
                self.interrupt_flag = true;
            }
        }
    }

    /// Clock the output unit (processes one bit from shift register)
    fn clock_output_unit(&mut self) {
        // Step 1: If silence flag is clear, update output level based on bit 0
        if !self.silence_flag {
            let bit0 = self.shift_register & 1;
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
        } else {
            self.silence_flag = true;
        }
    }

    /// Restart the sample from the beginning
    fn restart_sample(&mut self) {
        self.current_address = self.sample_address;
        self.bytes_remaining = self.sample_length;
    }

    /// Enable or disable the channel (called from $4015 status register)
    /// cpu_cycle is the current CPU cycle count for accurate delay timing
    pub fn set_enabled(&mut self, enabled: bool, cpu_cycle: u64) {
        if enabled {
            // If bytes_remaining is 0, restart the sample
            if self.bytes_remaining == 0 {
                self.restart_sample();
                // Delay DMA request by 2-3 cycles based on odd/even CPU cycle
                // This matches Mesen2's behavior for accurate DMC/OAM DMA collision timing
                if cpu_cycle % 2 == 0 {
                    self.transfer_start_delay = 2;
                } else {
                    self.transfer_start_delay = 3;
                }
            }
        } else {
            // Disable: clear bytes remaining
            self.bytes_remaining = 0;
            self.sample_buffer = None;
            self.silence_flag = true;
        }
    }

    /// Process one CPU clock cycle - handles transfer start delays
    /// Must be called once per CPU cycle to properly time DMC DMA requests
    pub fn process_clock(&mut self) {
        if self.transfer_start_delay > 0 {
            self.transfer_start_delay -= 1;
            if self.transfer_start_delay == 0 {
                // Delay expired, now trigger DMA if buffer is still empty
                self.fill_sample_buffer_if_needed();
            }
        }
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
                }
            }
        }
    }

    /// Get the IRQ flag status
    pub fn get_irq_flag(&self) -> bool {
        self.interrupt_flag
    }

    /// Clear the IRQ flag (side effect of writing to $4015)
    pub fn clear_irq_flag(&mut self) {
        self.interrupt_flag = false;
    }

    /// Check if the channel has bytes remaining (for status register $4015 bit 4)
    pub fn has_bytes_remaining(&self) -> bool {
        self.bytes_remaining > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dmc_new() {
        let dmc = Dmc::new();
        assert_eq!(dmc.output(), 0);
        assert_eq!(dmc.timer_period, 428); // Rate 0
        assert_eq!(dmc.bits_remaining, 8);
        assert!(dmc.silence_flag);
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

        assert_eq!(dmc.timer, 0);
        assert_eq!(dmc.timer_period, 54);

        // First clock loads the timer
        dmc.clock_timer();
        assert_eq!(dmc.timer, 53);

        // Subsequent clocks count down
        dmc.clock_timer();
        assert_eq!(dmc.timer, 52);
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
}

#[cfg(test)]
mod sample_tests {
    use super::*;
    use crate::cartridge::Cartridge;
    use crate::nes::{Nes, TvSystem};

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
    fn test_disable_channel_clears_bytes_remaining() {
        let mut dmc = Dmc::new();
        dmc.bytes_remaining = 100;

        dmc.set_enabled(false, 0);

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
        let cartridge = Cartridge::new(&rom).expect("test ROM should parse");

        let mut nes = Nes::new(TvSystem::Ntsc);
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Configure DMC to play 1 byte starting at $C000, at the fastest rate.
        {
            let mut apu = nes.apu.borrow_mut();
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
        let output = nes.apu.borrow().dmc().output();
        assert!(
            output > 0,
            "expected DMC output to be > 0 after reading sample; got {output}"
        );
    }
}
