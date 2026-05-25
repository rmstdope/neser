//! MBC3 mapper implementation with Real Time Clock (RTC) support.
//!
//! Supports cartridge types $0F-$13:
//! - $0F: MBC3+TIMER+BATTERY
//! - $10: MBC3+TIMER+RAM+BATTERY
//! - $11: MBC3
//! - $12: MBC3+RAM
//! - $13: MBC3+RAM+BATTERY

use super::cartridge::GbCartridge;

const RTC_SECONDS_MASK: u8 = 0x3F;
const RTC_MINUTES_MASK: u8 = 0x3F;
const RTC_HOURS_MASK: u8 = 0x1F;
const RTC_DAY_HIGH_MASK: u8 = 0xC1;

/// RTC register values (live counters)
#[derive(Debug, Clone, Default)]
struct RtcRegisters {
    seconds: u8,  // Bits 0-5
    minutes: u8,  // Bits 0-5
    hours: u8,    // Bits 0-4
    day_low: u8,  // Lower 8 bits of day counter
    day_high: u8, // Bit 0: day counter bit 8, Bit 6: halt, Bit 7: carry
}

impl RtcRegisters {
    /// Returns the full 9-bit day counter value
    fn day_counter(&self) -> u16 {
        ((self.day_high as u16 & 0x01) << 8) | self.day_low as u16
    }

    /// Sets the full 9-bit day counter value
    fn set_day_counter(&mut self, value: u16) {
        self.day_low = (value & 0xFF) as u8;
        self.day_high = (self.day_high & 0xFE) | ((value >> 8) & 0x01) as u8;
    }

    fn normalize(&mut self) {
        self.seconds &= RTC_SECONDS_MASK;
        self.minutes &= RTC_MINUTES_MASK;
        self.hours &= RTC_HOURS_MASK;
        self.day_high &= RTC_DAY_HIGH_MASK;
    }

    /// Returns true if the halt flag is set
    fn is_halted(&self) -> bool {
        self.day_high & 0x40 != 0
    }

    /// Returns true if the carry flag is set
    #[allow(dead_code)]
    fn has_carry(&self) -> bool {
        self.day_high & 0x80 != 0
    }

    /// Sets the carry flag
    fn set_carry(&mut self, carry: bool) {
        if carry {
            self.day_high |= 0x80;
        } else {
            self.day_high &= !0x80;
        }
    }
}

/// MBC3 mapper with optional RTC support
pub struct Mbc3 {
    rom: Vec<u8>,
    ram: Vec<u8>,
    rom_bank: u8, // 7-bit ROM bank number (1-127)
    ram_bank: u8, // RAM bank or RTC register select
    ram_enabled: bool,
    has_rtc: bool,
    has_ram: bool,
    has_battery: bool,

    // RTC state
    rtc: RtcRegisters,
    latched_rtc: RtcRegisters,
    cycle_counter: u32, // M-cycle counter for RTC ticking
}

impl Mbc3 {
    /// Creates a new MBC3 mapper
    ///
    /// # Arguments
    /// * `rom` - The ROM data
    /// * `ram_size` - Size of external RAM in bytes (0, 8KB, or 32KB)
    /// * `has_rtc` - Whether this cartridge has an RTC
    /// * `has_battery` - Whether this cartridge has battery backup
    pub fn new(rom: Vec<u8>, ram_size: usize, has_rtc: bool, has_battery: bool) -> Self {
        let has_ram = ram_size > 0;
        Self {
            rom,
            ram: vec![0; ram_size],
            rom_bank: 1,
            ram_bank: 0,
            ram_enabled: false,
            has_rtc,
            has_ram,
            has_battery,
            rtc: RtcRegisters::default(),
            latched_rtc: RtcRegisters::default(),
            cycle_counter: 0,
        }
    }

    /// Returns the effective ROM bank, promoting 0 to 1
    fn effective_rom_bank(&self) -> u8 {
        if self.rom_bank == 0 { 1 } else { self.rom_bank }
    }

    /// Returns the number of ROM banks
    fn rom_bank_count(&self) -> usize {
        self.rom.len() / 0x4000
    }

    /// Returns the number of RAM banks
    fn ram_bank_count(&self) -> usize {
        if self.ram.is_empty() {
            0
        } else {
            self.ram.len().div_ceil(0x2000)
        }
    }

    /// Returns true if the current RAM/RTC bank selects an RTC register
    #[cfg(test)]
    fn is_rtc_selected(&self) -> bool {
        self.selected_rtc_register().is_some()
    }

    fn rtc_mapped(&self) -> bool {
        self.has_rtc && self.ram_bank & 0x08 != 0
    }

    fn selected_rtc_register(&self) -> Option<u8> {
        if !self.rtc_mapped() {
            return None;
        }

        let register = self.ram_bank & 0x07;
        (register <= 0x04).then_some(0x08 + register)
    }

    fn selected_ram_bank(&self) -> Option<usize> {
        let bank = (self.ram_bank & 0x07) as usize;
        if self.has_rtc && bank > 3 {
            return None;
        }

        let bank_count = self.ram_bank_count().max(1);
        Some(bank % bank_count)
    }

    /// Read from ROM space ($0000-$7FFF)
    fn read_rom(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => {
                // Bank 0 is always at $0000-$3FFF
                *self.rom.get(addr as usize).unwrap_or(&0xFF)
            }
            0x4000..=0x7FFF => {
                // Switchable bank at $4000-$7FFF
                let bank = self.effective_rom_bank() as usize % self.rom_bank_count().max(1);
                let offset = (addr as usize - 0x4000) + bank * 0x4000;
                *self.rom.get(offset).unwrap_or(&0xFF)
            }
            _ => 0xFF,
        }
    }

    /// Read from RAM/RTC space ($A000-$BFFF)
    fn read_ram(&self, addr: u16) -> u8 {
        if !self.ram_enabled {
            return 0xFF;
        }

        if self.rtc_mapped() {
            self.selected_rtc_register()
                .map_or(0xFF, |register| self.read_rtc_register(register))
        } else if self.has_ram && !self.ram.is_empty() {
            // Read from RAM
            if let Some(bank) = self.selected_ram_bank() {
                let offset = (addr as usize - 0xA000) + bank * 0x2000;
                *self.ram.get(offset).unwrap_or(&0xFF)
            } else {
                0xFF
            }
        } else {
            0xFF
        }
    }

    fn read_rtc_register(&self, register: u8) -> u8 {
        match register {
            0x08 => self.latched_rtc.seconds,
            0x09 => self.latched_rtc.minutes,
            0x0A => self.latched_rtc.hours,
            0x0B => self.latched_rtc.day_low,
            0x0C => self.latched_rtc.day_high,
            _ => 0xFF,
        }
    }

    /// Write to register/RAM/RTC space
    fn write_impl(&mut self, addr: u16, value: u8) {
        match addr {
            // $0000-$1FFF: RAM/RTC Enable
            0x0000..=0x1FFF => {
                self.ram_enabled = (value & 0x0F) == 0x0A;
            }
            // $2000-$3FFF: ROM Bank Number (7-bit)
            0x2000..=0x3FFF => {
                self.rom_bank = value & 0x7F;
            }
            // $4000-$5FFF: RAM Bank / RTC Register Select
            0x4000..=0x5FFF => {
                self.ram_bank = value;
            }
            // $6000-$7FFF: Latch Clock Data
            0x6000..=0x7FFF => {
                // CasualPokePlayer's latch-rtc ROM, and SameBoy's MBC3 model,
                // observe latch-on-write behavior rather than a strict $00->$01
                // edge sequence.
                if self.has_rtc {
                    self.latched_rtc = self.rtc.clone();
                }
            }
            // $A000-$BFFF: RAM / RTC Write
            0xA000..=0xBFFF => {
                if !self.ram_enabled {
                    return;
                }

                if self.rtc_mapped() {
                    if let Some(register) = self.selected_rtc_register() {
                        self.write_rtc_register(register, value);
                    }
                } else if self.has_ram && !self.ram.is_empty() {
                    // Write to RAM
                    if let Some(bank) = self.selected_ram_bank() {
                        let offset = (addr as usize - 0xA000) + bank * 0x2000;
                        if let Some(byte) = self.ram.get_mut(offset) {
                            *byte = value;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn write_rtc_register(&mut self, register: u8, value: u8) {
        match register {
            0x08 => {
                self.rtc.seconds = value & RTC_SECONDS_MASK;
                self.cycle_counter = 0;
            }
            0x09 => self.rtc.minutes = value & RTC_MINUTES_MASK,
            0x0A => self.rtc.hours = value & RTC_HOURS_MASK,
            0x0B => self.rtc.day_low = value,
            0x0C => self.rtc.day_high = value & RTC_DAY_HIGH_MASK,
            _ => {}
        }
    }

    /// Increment the RTC by one second, handling overflow
    fn increment_rtc_second(&mut self) {
        let seconds = self.rtc.seconds & RTC_SECONDS_MASK;
        if seconds == 59 {
            self.rtc.seconds = 0;
            self.increment_rtc_minute();
        } else {
            self.rtc.seconds = seconds.wrapping_add(1) & RTC_SECONDS_MASK;
        }
    }

    fn increment_rtc_minute(&mut self) {
        let minutes = self.rtc.minutes & RTC_MINUTES_MASK;
        if minutes == 59 {
            self.rtc.minutes = 0;
            self.increment_rtc_hour();
        } else {
            self.rtc.minutes = minutes.wrapping_add(1) & RTC_MINUTES_MASK;
        }
    }

    fn increment_rtc_hour(&mut self) {
        let hours = self.rtc.hours & RTC_HOURS_MASK;
        if hours == 23 {
            self.rtc.hours = 0;
            self.increment_rtc_day();
        } else {
            self.rtc.hours = hours.wrapping_add(1) & RTC_HOURS_MASK;
        }
    }

    fn increment_rtc_day(&mut self) {
        let day = self.rtc.day_counter().wrapping_add(1);
        if day > 511 {
            // Day counter overflow - set carry flag and wrap to 0
            self.rtc.set_day_counter(0);
            self.rtc.set_carry(true);
        } else {
            self.rtc.set_day_counter(day);
        }
    }

    /// Add elapsed seconds to the RTC (used when restoring from save)
    fn add_elapsed_seconds(&mut self, seconds: u32) {
        if self.rtc.is_halted() {
            return;
        }

        let elapsed_seconds = seconds as u64;
        let minute_increments =
            elapsed_register_carries(self.rtc.seconds, elapsed_seconds, RTC_SECONDS_MASK, 59);
        self.rtc.seconds =
            wrapping_register_add(self.rtc.seconds, elapsed_seconds, RTC_SECONDS_MASK);

        let hour_increments =
            elapsed_register_carries(self.rtc.minutes, minute_increments, RTC_MINUTES_MASK, 59);
        self.rtc.minutes =
            wrapping_register_add(self.rtc.minutes, minute_increments, RTC_MINUTES_MASK);

        let day_increments =
            elapsed_register_carries(self.rtc.hours, hour_increments, RTC_HOURS_MASK, 23);
        self.rtc.hours = wrapping_register_add(self.rtc.hours, hour_increments, RTC_HOURS_MASK);

        let days = self.rtc.day_counter() as u64 + day_increments;
        if days > 511 {
            self.rtc.set_day_counter((days % 512) as u16);
            self.rtc.set_carry(true);
        } else {
            self.rtc.set_day_counter(days as u16);
        }
    }
}

fn wrapping_register_add(value: u8, increment: u64, mask: u8) -> u8 {
    let modulus = mask as u64 + 1;
    ((value as u64 + increment) % modulus) as u8
}

fn elapsed_register_carries(value: u8, increments: u64, mask: u8, carry_value: u8) -> u64 {
    if increments == 0 {
        return 0;
    }

    let modulus = mask as u64 + 1;
    let first_carry_offset = (carry_value as u64 + modulus - (value & mask) as u64) % modulus;
    if increments <= first_carry_offset {
        0
    } else {
        1 + (increments - first_carry_offset - 1) / modulus
    }
}

impl GbCartridge for Mbc3 {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.read_rom(addr),
            0xA000..=0xBFFF => self.read_ram(addr),
            _ => 0xFF,
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.write_impl(addr, value);
    }

    fn has_battery(&self) -> bool {
        self.has_battery
    }

    fn ram_snapshot(&self) -> Vec<u8> {
        let mut data = self.ram.clone();

        if self.has_rtc {
            // Append RTC state (18 bytes total)
            // Live registers (5 bytes)
            data.push(self.rtc.seconds);
            data.push(self.rtc.minutes);
            data.push(self.rtc.hours);
            data.push(self.rtc.day_low);
            data.push(self.rtc.day_high);
            // Latched registers (5 bytes)
            data.push(self.latched_rtc.seconds);
            data.push(self.latched_rtc.minutes);
            data.push(self.latched_rtc.hours);
            data.push(self.latched_rtc.day_low);
            data.push(self.latched_rtc.day_high);
            // Unix timestamp (8 bytes, little-endian)
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            data.extend_from_slice(&timestamp.to_le_bytes());
        }

        data
    }

    fn restore_ram(&mut self, data: &[u8]) {
        let ram_size = self.ram.len();
        let rtc_size = if self.has_rtc { 18 } else { 0 };

        if self.has_rtc && data.len() >= ram_size + rtc_size {
            // Restore RAM
            self.ram.copy_from_slice(&data[..ram_size]);

            // Restore RTC state
            let rtc_data = &data[ram_size..];
            self.rtc.seconds = rtc_data[0];
            self.rtc.minutes = rtc_data[1];
            self.rtc.hours = rtc_data[2];
            self.rtc.day_low = rtc_data[3];
            self.rtc.day_high = rtc_data[4];
            self.rtc.normalize();
            self.latched_rtc.seconds = rtc_data[5];
            self.latched_rtc.minutes = rtc_data[6];
            self.latched_rtc.hours = rtc_data[7];
            self.latched_rtc.day_low = rtc_data[8];
            self.latched_rtc.day_high = rtc_data[9];
            self.latched_rtc.normalize();

            // Calculate elapsed time and add to RTC
            let saved_timestamp = u64::from_le_bytes(rtc_data[10..18].try_into().unwrap_or([0; 8]));
            let current_timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            if current_timestamp > saved_timestamp {
                let elapsed_seconds = (current_timestamp - saved_timestamp) as u32;
                self.add_elapsed_seconds(elapsed_seconds);
            }
        } else if ram_size > 0 && data.len() >= ram_size {
            // No RTC or legacy save format - just restore RAM
            self.ram.copy_from_slice(&data[..ram_size]);
        } else if !data.is_empty() {
            // Partial restore - copy what we can
            let copy_len = data.len().min(ram_size);
            self.ram[..copy_len].copy_from_slice(&data[..copy_len]);
        }
    }

    fn mbc_state_snapshot(&self) -> Vec<u8> {
        let mut state = vec![
            self.rom_bank,
            self.ram_bank,
            self.ram_enabled as u8,
            0, // Legacy latch_armed snapshot byte; ignored by current latch semantics.
        ];
        // Include cycle_counter for smooth RTC across save-states
        state.extend_from_slice(&self.cycle_counter.to_le_bytes());
        state
    }

    fn restore_mbc_state(&mut self, data: &[u8]) {
        if data.len() >= 4 {
            self.rom_bank = data[0];
            self.ram_bank = data[1];
            self.ram_enabled = data[2] != 0;
        }
        if data.len() >= 8 {
            self.cycle_counter = u32::from_le_bytes(data[4..8].try_into().unwrap_or([0; 4]));
        }
    }

    fn tick(&mut self, cycles: u32) {
        if !self.has_rtc || self.rtc.is_halted() {
            return;
        }

        self.cycle_counter += cycles;
        // M-cycles per second (CPU runs at 1,048,576 M-cycles/sec)
        const M_CYCLES_PER_SECOND: u32 = 1_048_576;

        while self.cycle_counter >= M_CYCLES_PER_SECOND {
            self.cycle_counter -= M_CYCLES_PER_SECOND;
            self.increment_rtc_second();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates a test ROM with the specified number of 16KB banks
    fn make_rom(bank_count: usize) -> Vec<u8> {
        let mut rom = vec![0u8; bank_count * 0x4000];
        // Write bank number to first byte of each bank for verification
        for bank in 0..bank_count {
            rom[bank * 0x4000] = bank as u8;
        }
        rom
    }

    // ========================================================================
    // ROM Banking Tests
    // ========================================================================

    #[test]
    fn test_rom_bank0_always_accessible() {
        let rom = make_rom(4);
        let mbc3 = Mbc3::new(rom, 0, false, false);

        // Bank 0 should always be at $0000-$3FFF
        assert_eq!(mbc3.read(0x0000), 0, "Bank 0 should be readable at $0000");
    }

    #[test]
    fn test_rom_bank_switching() {
        let rom = make_rom(8);
        let mut mbc3 = Mbc3::new(rom, 0, false, false);

        // Select bank 3
        mbc3.write(0x2000, 3);
        assert_eq!(mbc3.read(0x4000), 3, "Bank 3 should be at $4000");

        // Select bank 7
        mbc3.write(0x2000, 7);
        assert_eq!(mbc3.read(0x4000), 7, "Bank 7 should be at $4000");
    }

    #[test]
    fn test_rom_bank_0_promotes_to_1() {
        let rom = make_rom(4);
        let mut mbc3 = Mbc3::new(rom, 0, false, false);

        // Writing 0 should select bank 1
        mbc3.write(0x2000, 0);
        assert_eq!(mbc3.read(0x4000), 1, "Writing 0 should select bank 1");
    }

    #[test]
    fn test_rom_bank_wraps_to_rom_size() {
        let rom = make_rom(4); // 4 banks, so mask is 0x03
        let mut mbc3 = Mbc3::new(rom, 0, false, false);

        // Bank 5 should wrap to bank 1 (5 & 0x03 = 1)
        mbc3.write(0x2000, 5);
        assert_eq!(mbc3.read(0x4000), 1, "Bank 5 should wrap to bank 1");
    }

    #[test]
    fn test_rom_bank_register_is_7bit() {
        let rom = make_rom(128); // Max banks for MBC3
        let mut mbc3 = Mbc3::new(rom, 0, false, false);

        // Writing $FF should select bank 127 (7-bit mask: $7F)
        mbc3.write(0x2000, 0xFF);
        assert_eq!(mbc3.read(0x4000), 127, "ROM bank should be 7-bit (0-127)");
    }

    // ========================================================================
    // RAM Banking Tests
    // ========================================================================

    #[test]
    fn test_ram_disabled_by_default() {
        let rom = make_rom(2);
        let mbc3 = Mbc3::new(rom, 0x8000, false, false);

        // RAM should return $FF when disabled
        assert_eq!(mbc3.read(0xA000), 0xFF, "Disabled RAM should return $FF");
    }

    #[test]
    fn test_ram_enable_with_0a() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x8000, false, false);

        // Enable RAM
        mbc3.write(0x0000, 0x0A);
        mbc3.write(0xA000, 0x42);
        assert_eq!(
            mbc3.read(0xA000),
            0x42,
            "RAM should be writable when enabled"
        );
    }

    #[test]
    fn test_ram_disable() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x8000, false, false);

        // Enable, write, disable, read
        mbc3.write(0x0000, 0x0A);
        mbc3.write(0xA000, 0x42);
        mbc3.write(0x0000, 0x00);
        assert_eq!(mbc3.read(0xA000), 0xFF, "Disabled RAM should return $FF");
    }

    #[test]
    fn test_ram_bank_switching() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x8000, false, false); // 32KB = 4 banks

        mbc3.write(0x0000, 0x0A); // Enable RAM

        // Write to bank 0
        mbc3.write(0x4000, 0);
        mbc3.write(0xA000, 0x11);

        // Write to bank 1
        mbc3.write(0x4000, 1);
        mbc3.write(0xA000, 0x22);

        // Verify bank 0
        mbc3.write(0x4000, 0);
        assert_eq!(mbc3.read(0xA000), 0x11, "Bank 0 should have 0x11");

        // Verify bank 1
        mbc3.write(0x4000, 1);
        assert_eq!(mbc3.read(0xA000), 0x22, "Bank 1 should have 0x22");
    }

    #[test]
    fn test_ram_bank_wraps_to_ram_size() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, false, false); // 8KB = 1 bank

        mbc3.write(0x0000, 0x0A); // Enable RAM

        // Write to bank 0
        mbc3.write(0x4000, 0);
        mbc3.write(0xA000, 0x42);

        // Select bank 3 - should wrap to bank 0
        mbc3.write(0x4000, 3);
        assert_eq!(mbc3.read(0xA000), 0x42, "Bank 3 should wrap to bank 0");
    }

    #[test]
    fn test_mbc3_rtc_cart_ram_banks_4_to_7_are_unmapped() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x8000, true, true);

        mbc3.write(0x0000, 0x0A);
        mbc3.write(0x4000, 0x00);
        mbc3.write(0xA000, 0x42);

        for bank in 0x04..=0x07 {
            mbc3.write(0x4000, bank);
            assert_eq!(
                mbc3.read(0xA000),
                0xFF,
                "RAM bank {bank:#04X} should be unmapped"
            );
            mbc3.write(0xA000, bank);
        }

        mbc3.write(0x4000, 0x00);
        assert_eq!(
            mbc3.read(0xA000),
            0x42,
            "unmapped bank writes should be ignored"
        );
    }

    #[test]
    fn test_mbc3_rtc_cart_bank_select_ignores_upper_nibble() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x8000, true, true);

        mbc3.write(0x0000, 0x0A);
        mbc3.write(0x4000, 0x00);
        mbc3.write(0xA000, 0x34);
        write_rtc_register(&mut mbc3, 0x08, 0x12);
        mbc3.write(0x6000, 0x01);

        mbc3.write(0x4000, 0x10);
        assert_eq!(mbc3.read(0xA000), 0x34, "0x10 should mirror RAM bank 0");

        mbc3.write(0x4000, 0x18);
        assert_eq!(mbc3.read(0xA000), 0x12, "0x18 should mirror RTC seconds");

        mbc3.write(0x4000, 0x1D);
        assert_eq!(
            mbc3.read(0xA000),
            0xFF,
            "0x1D should mirror unmapped RTC index 5"
        );
    }

    // ========================================================================
    // RTC Register Tests
    // ========================================================================

    #[test]
    fn test_rtc_register_select() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, true, true);

        mbc3.write(0x0000, 0x0A); // Enable

        // Select RTC seconds register
        mbc3.write(0x4000, 0x08);
        assert!(mbc3.is_rtc_selected(), "Should select RTC register");

        // Select RTC day high register
        mbc3.write(0x4000, 0x0C);
        assert!(mbc3.is_rtc_selected(), "Should select RTC register");

        // Select RAM bank
        mbc3.write(0x4000, 0x00);
        assert!(!mbc3.is_rtc_selected(), "Should select RAM, not RTC");
    }

    #[test]
    fn test_rtc_register_read_write() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, true, true);

        mbc3.write(0x0000, 0x0A); // Enable

        // Write to seconds register
        mbc3.write(0x4000, 0x08);
        mbc3.write(0xA000, 30);

        // Latch and verify
        mbc3.write(0x6000, 0x00);
        mbc3.write(0x6000, 0x01);
        assert_eq!(mbc3.read(0xA000), 30, "Seconds should be 30");
    }

    fn write_rtc_register(mbc3: &mut Mbc3, register: u8, value: u8) {
        mbc3.write(0x4000, register);
        mbc3.write(0xA000, value);
    }

    fn latch_rtc(mbc3: &mut Mbc3) {
        mbc3.write(0x6000, 0x00);
        mbc3.write(0x6000, 0x01);
    }

    fn read_latched_rtc_register(mbc3: &mut Mbc3, register: u8) -> u8 {
        mbc3.write(0x4000, register);
        mbc3.read(0xA000)
    }

    #[test]
    fn test_rtc_register_writes_keep_only_valid_bits() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, true, true);

        mbc3.write(0x0000, 0x0A); // Enable

        write_rtc_register(&mut mbc3, 0x08, 0xFF);
        write_rtc_register(&mut mbc3, 0x09, 0xFF);
        write_rtc_register(&mut mbc3, 0x0A, 0xFF);
        write_rtc_register(&mut mbc3, 0x0B, 0xFF);
        write_rtc_register(&mut mbc3, 0x0C, 0xFF);
        latch_rtc(&mut mbc3);

        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x08), 0x3F);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x09), 0x3F);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x0A), 0x1F);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x0B), 0xFF);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x0C), 0xC1);
    }

    #[test]
    fn test_rtc_invalid_time_values_increment_without_decimal_carry() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, true, true);

        mbc3.write(0x0000, 0x0A);

        write_rtc_register(&mut mbc3, 0x08, 60);
        write_rtc_register(&mut mbc3, 0x09, 63);
        write_rtc_register(&mut mbc3, 0x0A, 28);
        write_rtc_register(&mut mbc3, 0x0B, 0x34);
        write_rtc_register(&mut mbc3, 0x0C, 0x01);

        mbc3.tick(M_CYCLES_PER_SECOND);
        latch_rtc(&mut mbc3);

        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x08), 61);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x09), 63);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x0A), 28);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x0B), 0x34);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x0C) & 0x01, 0x01);
    }

    #[test]
    fn test_rtc_invalid_seconds_rollover_does_not_increment_minutes() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, true, true);

        mbc3.write(0x0000, 0x0A);

        write_rtc_register(&mut mbc3, 0x08, 63);
        write_rtc_register(&mut mbc3, 0x09, 12);

        mbc3.tick(M_CYCLES_PER_SECOND);
        latch_rtc(&mut mbc3);

        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x08), 0);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x09), 12);
    }

    #[test]
    fn test_rtc_invalid_minutes_rollover_does_not_increment_hours() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, true, true);

        mbc3.write(0x0000, 0x0A);

        write_rtc_register(&mut mbc3, 0x08, 59);
        write_rtc_register(&mut mbc3, 0x09, 63);
        write_rtc_register(&mut mbc3, 0x0A, 12);

        mbc3.tick(M_CYCLES_PER_SECOND);
        latch_rtc(&mut mbc3);

        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x08), 0);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x09), 0);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x0A), 12);
    }

    #[test]
    fn test_rtc_invalid_hours_rollover_does_not_increment_day() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, true, true);

        mbc3.write(0x0000, 0x0A);

        write_rtc_register(&mut mbc3, 0x08, 59);
        write_rtc_register(&mut mbc3, 0x09, 59);
        write_rtc_register(&mut mbc3, 0x0A, 31);
        write_rtc_register(&mut mbc3, 0x0B, 0x22);

        mbc3.tick(M_CYCLES_PER_SECOND);
        latch_rtc(&mut mbc3);

        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x08), 0);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x09), 0);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x0A), 0);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x0B), 0x22);
    }

    #[test]
    fn test_rtc_high_minutes_and_hours_increment_on_valid_lower_rollovers() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, true, true);

        mbc3.write(0x0000, 0x0A);

        write_rtc_register(&mut mbc3, 0x08, 59);
        write_rtc_register(&mut mbc3, 0x09, 62);
        write_rtc_register(&mut mbc3, 0x0A, 30);

        mbc3.tick(M_CYCLES_PER_SECOND);
        latch_rtc(&mut mbc3);

        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x08), 0);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x09), 63);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x0A), 30);

        write_rtc_register(&mut mbc3, 0x08, 59);
        write_rtc_register(&mut mbc3, 0x09, 59);
        write_rtc_register(&mut mbc3, 0x0A, 30);

        mbc3.tick(M_CYCLES_PER_SECOND);
        latch_rtc(&mut mbc3);

        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x08), 0);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x09), 0);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x0A), 31);
    }

    #[test]
    fn test_rtc_elapsed_restore_preserves_invalid_register_semantics() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, true, true);

        mbc3.write(0x0000, 0x0A);

        write_rtc_register(&mut mbc3, 0x08, 60);
        write_rtc_register(&mut mbc3, 0x09, 63);
        write_rtc_register(&mut mbc3, 0x0A, 28);

        mbc3.add_elapsed_seconds(64);
        latch_rtc(&mut mbc3);

        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x08), 60);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x09), 0);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x0A), 28);
    }

    // ========================================================================
    // RTC Latch Tests
    // ========================================================================

    #[test]
    fn test_rtc_latch_matches_casual_poke_player_single_write_semantics() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, true, true);

        mbc3.write(0x0000, 0x0A); // Enable

        // Set seconds to 30
        mbc3.write(0x4000, 0x08);
        mbc3.write(0xA000, 30);

        mbc3.write(0x6000, 0x01);

        assert_eq!(
            mbc3.read(0xA000),
            30,
            "MBC3 should latch RTC on a single latch register write"
        );
    }

    #[test]
    fn test_rtc_latch_matches_casual_poke_player_any_write_semantics() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, true, true);

        mbc3.write(0x0000, 0x0A);
        write_rtc_register(&mut mbc3, 0x08, 0x21);

        mbc3.write(0x6000, 0xA5);

        assert_eq!(
            read_latched_rtc_register(&mut mbc3, 0x08),
            0x21,
            "MBC3 should latch RTC on any write to the latch register"
        );
    }

    #[test]
    fn test_rtc_latch_correct_sequence() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, true, true);

        mbc3.write(0x0000, 0x0A); // Enable

        // Set seconds to 45
        mbc3.write(0x4000, 0x08);
        mbc3.write(0xA000, 45);

        // Correct latch sequence
        mbc3.write(0x6000, 0x00);
        mbc3.write(0x6000, 0x01);

        assert_eq!(mbc3.read(0xA000), 45, "Latched seconds should be 45");
    }

    #[test]
    fn test_rtc_latch_freezes_values() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, true, true);

        mbc3.write(0x0000, 0x0A); // Enable

        // Set and latch seconds to 30
        mbc3.write(0x4000, 0x08);
        mbc3.write(0xA000, 30);
        mbc3.write(0x6000, 0x00);
        mbc3.write(0x6000, 0x01);

        // Change live value to 45
        mbc3.write(0xA000, 45);

        // Latched value should still be 30
        assert_eq!(mbc3.read(0xA000), 30, "Latched value should not change");
    }

    // ========================================================================
    // RTC Tick Tests
    // ========================================================================

    const M_CYCLES_PER_SECOND: u32 = 1_048_576;

    #[test]
    fn test_rtc_tick_increments_seconds() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, true, true);

        mbc3.write(0x0000, 0x0A);
        mbc3.write(0x4000, 0x08); // Select seconds

        // Tick one second
        mbc3.tick(M_CYCLES_PER_SECOND);

        // Latch and read
        mbc3.write(0x6000, 0x00);
        mbc3.write(0x6000, 0x01);

        assert_eq!(mbc3.read(0xA000), 1, "Seconds should increment to 1");
    }

    #[test]
    fn test_rtc_seconds_write_resets_sub_second_counter() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, true, true);

        mbc3.write(0x0000, 0x0A);
        write_rtc_register(&mut mbc3, 0x08, 10);
        mbc3.tick(M_CYCLES_PER_SECOND - 1);

        write_rtc_register(&mut mbc3, 0x08, 20);
        mbc3.tick(1);
        latch_rtc(&mut mbc3);

        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x08), 20);

        mbc3.tick(M_CYCLES_PER_SECOND - 1);
        latch_rtc(&mut mbc3);

        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x08), 21);
    }

    #[test]
    fn test_rtc_non_seconds_writes_preserve_sub_second_counter() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, true, true);

        mbc3.write(0x0000, 0x0A);
        write_rtc_register(&mut mbc3, 0x08, 10);
        mbc3.tick(M_CYCLES_PER_SECOND - 1);

        write_rtc_register(&mut mbc3, 0x09, 1);
        write_rtc_register(&mut mbc3, 0x0A, 2);
        write_rtc_register(&mut mbc3, 0x0B, 3);
        write_rtc_register(&mut mbc3, 0x0C, 0);
        mbc3.tick(1);
        latch_rtc(&mut mbc3);

        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x08), 11);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x09), 1);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x0A), 2);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x0B), 3);
        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x0C), 0);
    }

    #[test]
    fn test_rtc_halt_resume_preserves_sub_second_counter() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, true, true);

        mbc3.write(0x0000, 0x0A);
        write_rtc_register(&mut mbc3, 0x08, 10);
        mbc3.tick(M_CYCLES_PER_SECOND - 1);

        write_rtc_register(&mut mbc3, 0x0C, 0x40);
        mbc3.tick(10);
        write_rtc_register(&mut mbc3, 0x0C, 0x00);
        mbc3.tick(1);
        latch_rtc(&mut mbc3);

        assert_eq!(read_latched_rtc_register(&mut mbc3, 0x08), 11);
    }

    #[test]
    fn test_rtc_tick_seconds_overflow_to_minutes() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, true, true);

        mbc3.write(0x0000, 0x0A);

        // Set seconds to 59
        mbc3.write(0x4000, 0x08);
        mbc3.write(0xA000, 59);

        // Tick one second
        mbc3.tick(M_CYCLES_PER_SECOND);

        // Latch
        mbc3.write(0x6000, 0x00);
        mbc3.write(0x6000, 0x01);

        // Check seconds reset to 0
        mbc3.write(0x4000, 0x08);
        assert_eq!(mbc3.read(0xA000), 0, "Seconds should reset to 0");

        // Check minutes incremented to 1
        mbc3.write(0x4000, 0x09);
        assert_eq!(mbc3.read(0xA000), 1, "Minutes should increment to 1");
    }

    #[test]
    fn test_rtc_tick_respects_halt_flag() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, true, true);

        mbc3.write(0x0000, 0x0A);

        // Set halt flag (bit 6 of day high register)
        mbc3.write(0x4000, 0x0C);
        mbc3.write(0xA000, 0x40);

        // Tick multiple seconds
        mbc3.tick(M_CYCLES_PER_SECOND * 5);

        // Latch and verify seconds stayed at 0
        mbc3.write(0x6000, 0x00);
        mbc3.write(0x6000, 0x01);
        mbc3.write(0x4000, 0x08);
        assert_eq!(mbc3.read(0xA000), 0, "RTC should not tick when halted");
    }

    #[test]
    fn test_rtc_day_counter_overflow_sets_carry() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom, 0x2000, true, true);

        mbc3.write(0x0000, 0x0A);

        // Set to day 511 (max 9-bit value), 23:59:59
        mbc3.write(0x4000, 0x08);
        mbc3.write(0xA000, 59); // seconds
        mbc3.write(0x4000, 0x09);
        mbc3.write(0xA000, 59); // minutes
        mbc3.write(0x4000, 0x0A);
        mbc3.write(0xA000, 23); // hours
        mbc3.write(0x4000, 0x0B);
        mbc3.write(0xA000, 0xFF); // day low = 255
        mbc3.write(0x4000, 0x0C);
        mbc3.write(0xA000, 0x01); // day high bit 0 set = day 511

        // Tick one second to overflow
        mbc3.tick(M_CYCLES_PER_SECOND);

        // Latch
        mbc3.write(0x6000, 0x00);
        mbc3.write(0x6000, 0x01);

        // Check carry flag is set
        mbc3.write(0x4000, 0x0C);
        let day_high = mbc3.read(0xA000);
        assert!(
            day_high & 0x80 != 0,
            "Carry flag should be set on day overflow"
        );
    }

    // ========================================================================
    // Save/Restore Tests
    // ========================================================================

    #[test]
    fn test_ram_snapshot_and_restore() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom.clone(), 0x2000, false, true);

        mbc3.write(0x0000, 0x0A);
        mbc3.write(0xA000, 0x42);
        mbc3.write(0xA001, 0x43);

        let snapshot = mbc3.ram_snapshot();

        let mut mbc3_new = Mbc3::new(rom, 0x2000, false, true);
        mbc3_new.restore_ram(&snapshot);
        mbc3_new.write(0x0000, 0x0A);

        assert_eq!(mbc3_new.read(0xA000), 0x42);
        assert_eq!(mbc3_new.read(0xA001), 0x43);
    }

    #[test]
    fn test_rtc_snapshot_and_restore() {
        let rom = make_rom(2);
        let mut mbc3 = Mbc3::new(rom.clone(), 0x2000, true, true);

        mbc3.write(0x0000, 0x0A);

        // Set RTC values
        mbc3.write(0x4000, 0x08);
        mbc3.write(0xA000, 30); // seconds
        mbc3.write(0x4000, 0x09);
        mbc3.write(0xA000, 15); // minutes

        let snapshot = mbc3.ram_snapshot();

        let mut mbc3_new = Mbc3::new(rom, 0x2000, true, true);
        mbc3_new.restore_ram(&snapshot);
        mbc3_new.write(0x0000, 0x0A);

        // Latch and verify
        mbc3_new.write(0x6000, 0x00);
        mbc3_new.write(0x6000, 0x01);

        mbc3_new.write(0x4000, 0x08);
        assert_eq!(mbc3_new.read(0xA000), 30, "Seconds should be restored");

        mbc3_new.write(0x4000, 0x09);
        assert_eq!(mbc3_new.read(0xA000), 15, "Minutes should be restored");
    }

    #[test]
    fn test_has_battery() {
        let rom = make_rom(2);

        let mbc3_with_battery = Mbc3::new(rom.clone(), 0x2000, false, true);
        assert!(mbc3_with_battery.has_battery());

        let mbc3_no_battery = Mbc3::new(rom, 0x2000, false, false);
        assert!(!mbc3_no_battery.has_battery());
    }

    // ========================================================================
    // MBC State Snapshot Tests
    // ========================================================================

    #[test]
    fn test_mbc_state_snapshot_and_restore() {
        let rom = make_rom(8);
        let mut mbc3 = Mbc3::new(rom.clone(), 0x8000, true, true);

        // Set up various state
        mbc3.write(0x0000, 0x0A); // Enable RAM
        mbc3.write(0x2000, 5); // ROM bank 5
        mbc3.write(0x4000, 2); // RAM bank 2

        let state = mbc3.mbc_state_snapshot();
        assert!(!state.is_empty());

        let mut mbc3_new = Mbc3::new(rom, 0x8000, true, true);
        mbc3_new.restore_mbc_state(&state);

        // Verify ROM bank
        assert_eq!(mbc3_new.read(0x4000), 5, "ROM bank should be restored");
    }
}
