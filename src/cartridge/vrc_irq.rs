#[derive(Debug, Clone)]
pub struct VrcIrq {
    latch: u8,
    counter: u8,
    enabled: bool,
    mode_cycle: bool,
    enable_after_ack: bool,
    asserted: bool,
    prescaler: i32,
    prescaler_init: i32,
    prescaler_step: i32,
}

impl Default for VrcIrq {
    fn default() -> Self {
        Self {
            latch: 0,
            counter: 0,
            enabled: false,
            mode_cycle: false,
            enable_after_ack: false,
            asserted: false,
            prescaler: 0,
            prescaler_init: 341,
            prescaler_step: 3,
        }
    }
}

impl VrcIrq {
    pub fn new(prescaler_init: i32, prescaler_step: i32) -> Self {
        Self {
            prescaler_init,
            prescaler_step,
            ..Self::default()
        }
    }

    pub fn write_latch(&mut self, value: u8) {
        self.latch = value;
    }

    pub fn write_latch_low_nibble(&mut self, value: u8) {
        self.latch = (self.latch & 0xF0) | (value & 0x0F);
    }

    pub fn write_latch_high_nibble(&mut self, value: u8) {
        self.latch = (self.latch & 0x0F) | ((value & 0x0F) << 4);
    }

    pub fn write_control(&mut self, value: u8) {
        self.asserted = false;
        self.prescaler = self.prescaler_init;

        self.mode_cycle = (value & 0b0000_0100) != 0;
        let enable = (value & 0b0000_0010) != 0;
        self.enable_after_ack = (value & 0b0000_0001) != 0;

        if enable {
            self.enabled = true;
            self.counter = self.latch;
        } else {
            self.enabled = false;
        }
    }

    pub fn write_acknowledge(&mut self) {
        self.asserted = false;
        self.enabled = self.enable_after_ack;
    }

    pub fn tick(&mut self) {
        if !self.enabled {
            return;
        }

        if self.mode_cycle {
            self.clock_counter();
            return;
        }

        self.prescaler -= self.prescaler_step;
        if self.prescaler <= 0 {
            self.prescaler += self.prescaler_init;
            self.clock_counter();
        }
    }

    pub fn pending(&self) -> bool {
        self.asserted
    }

    pub fn latch(&self) -> u8 {
        self.latch
    }

    pub fn counter(&self) -> u8 {
        self.counter
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn mode_cycle(&self) -> bool {
        self.mode_cycle
    }

    pub fn enable_after_ack(&self) -> bool {
        self.enable_after_ack
    }

    pub fn prescaler(&self) -> i32 {
        self.prescaler
    }

    pub fn set_counter(&mut self, counter: u8) {
        self.counter = counter;
    }

    pub fn set_asserted(&mut self, asserted: bool) {
        self.asserted = asserted;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_mode_cycle(&mut self, mode_cycle: bool) {
        self.mode_cycle = mode_cycle;
    }

    pub fn set_enable_after_ack(&mut self, enable_after_ack: bool) {
        self.enable_after_ack = enable_after_ack;
    }

    pub fn set_prescaler(&mut self, prescaler: i32) {
        self.prescaler = prescaler;
    }

    fn clock_counter(&mut self) {
        if self.counter == 0xFF {
            self.counter = self.latch;
            self.asserted = true;
        } else {
            self.counter = self.counter.wrapping_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_control_in_cycle_mode_reloads_counter_and_asserts_after_overflow() {
        let mut irq = VrcIrq::new(341, 3);

        irq.write_latch(0xFE);
        irq.write_control(0b0000_0110); // M=1, E=1, A=0

        irq.tick();
        assert!(!irq.pending());
        irq.tick();
        assert!(irq.pending());
    }

    #[test]
    fn write_control_in_scanline_mode_asserts_after_114_ticks_from_ff() {
        let mut irq = VrcIrq::new(341, 3);

        irq.write_latch(0xFF);
        irq.write_control(0b0000_0010); // M=0, E=1, A=0

        for _ in 0..113 {
            irq.tick();
        }
        assert!(!irq.pending());

        irq.tick();
        assert!(irq.pending());
    }

    #[test]
    fn acknowledge_clears_pending_and_copies_enable_after_ack() {
        let mut irq = VrcIrq::new(341, 3);

        irq.write_latch(0xFE);
        irq.write_control(0b0000_0111); // M=1, E=1, A=1
        irq.tick();
        irq.tick();
        assert!(irq.pending());

        irq.write_acknowledge();

        assert!(!irq.pending());
        assert!(irq.enabled());
    }

    #[test]
    fn split_latch_writes_build_full_byte() {
        let mut irq = VrcIrq::new(341, 3);

        irq.write_latch_low_nibble(0x0E);
        irq.write_latch_high_nibble(0x0F);

        assert_eq!(irq.latch(), 0xFE);
    }
}
