use crate::apu;
use crate::bus::bus::BusDevice;
use std::cell::RefCell;
use std::ops::RangeInclusive;
use std::rc::Rc;

pub(crate) struct ApuDevice {
    apu: Rc<RefCell<apu::Apu>>,
}

impl ApuDevice {
    pub(crate) fn new(apu: Rc<RefCell<apu::Apu>>) -> Self {
        Self { apu }
    }
}

impl BusDevice for ApuDevice {
    fn read(&mut self, addr: u16, open_bus: u8, _clock_joypads: bool) -> Option<u8> {
        if !self.address_range().contains(&addr) {
            return None;
        }

        match addr {
            0x4000..=0x4013 => Some(open_bus),
            0x4015 => Some(self.apu.borrow_mut().read_status(open_bus)),
            0x4016 | 0x4017 => None,
            0x4018..=0x40FF => Some(open_bus),
            _ => None,
        }
    }

    fn write(&mut self, addr: u16, value: u8, _is_dummy_write: bool) -> bool {
        if !self.address_range().contains(&addr) {
            return false;
        }

        match addr {
            0x4000 => self.apu.borrow_mut().pulse1_mut().write_control(value),
            0x4001 => self.apu.borrow_mut().pulse1_mut().write_sweep(value),
            0x4002 => self.apu.borrow_mut().pulse1_mut().write_timer_low(value),
            0x4003 => self
                .apu
                .borrow_mut()
                .pulse1_mut()
                .write_length_counter_timer_high(value),

            0x4004 => self.apu.borrow_mut().pulse2_mut().write_control(value),
            0x4005 => self.apu.borrow_mut().pulse2_mut().write_sweep(value),
            0x4006 => self.apu.borrow_mut().pulse2_mut().write_timer_low(value),
            0x4007 => self
                .apu
                .borrow_mut()
                .pulse2_mut()
                .write_length_counter_timer_high(value),

            0x4008 => self
                .apu
                .borrow_mut()
                .triangle_mut()
                .write_linear_counter(value),
            0x400A => self.apu.borrow_mut().triangle_mut().write_timer_low(value),
            0x400B => self
                .apu
                .borrow_mut()
                .triangle_mut()
                .write_length_counter_timer_high(value),

            0x400C => self.apu.borrow_mut().noise_mut().write_envelope(value),
            0x400E => self.apu.borrow_mut().noise_mut().write_period(value),
            0x400F => self.apu.borrow_mut().noise_mut().write_length(value),

            0x4010 => self.apu.borrow_mut().dmc_mut().write_flags_and_rate(value),
            0x4011 => self.apu.borrow_mut().dmc_mut().write_direct_load(value),
            0x4012 => self.apu.borrow_mut().dmc_mut().write_sample_address(value),
            0x4013 => self.apu.borrow_mut().dmc_mut().write_sample_length(value),

            0x4014 => return false,
            0x4016 => return false,

            0x4015 => self.apu.borrow_mut().write_enable(value),
            0x4017 => self.apu.borrow_mut().write_frame_counter(value),

            0x4018..=0x40FF => {}

            0x4009 | 0x400D => {}
            _ => {
                eprintln!(
                    "Warning: Write to unimplemented APU/IO register {:04X} ignored",
                    addr
                );
            }
        }

        true
    }

    fn address_range(&self) -> RangeInclusive<u16> {
        0x4000..=0x401F
    }
}
