use crate::bus::bus::BusDevice;
use crate::input::Controller;
use std::cell::RefCell;
use std::ops::RangeInclusive;
use std::rc::Rc;

pub(crate) struct ControllerDevice {
    controllers: [Rc<RefCell<Box<dyn Controller>>>; 2],
    four_score_extra_button_states: Rc<RefCell<[u8; 2]>>,
    four_score_enabled: bool,
    four_score_strobe: bool,
    four_score_index: [u8; 2],
    famicom_four_players_enabled: bool,
    famicom_four_players_strobe: bool,
    famicom_four_players_index: [u8; 2],
}

impl ControllerDevice {
    #[cfg(test)]
    pub(crate) fn new(
        port1_controller: Rc<RefCell<Box<dyn Controller>>>,
        port2_controller: Rc<RefCell<Box<dyn Controller>>>,
    ) -> Self {
        Self::new_with_four_score_state(
            port1_controller,
            port2_controller,
            false,
            false,
            Rc::new(RefCell::new([0, 0])),
        )
    }

    pub(crate) fn new_with_four_score_state(
        port1_controller: Rc<RefCell<Box<dyn Controller>>>,
        port2_controller: Rc<RefCell<Box<dyn Controller>>>,
        four_score_enabled: bool,
        famicom_four_players_enabled: bool,
        four_score_extra_button_states: Rc<RefCell<[u8; 2]>>,
    ) -> Self {
        Self {
            controllers: [port1_controller, port2_controller],
            four_score_extra_button_states,
            four_score_enabled,
            four_score_strobe: false,
            four_score_index: [0, 0],
            famicom_four_players_enabled,
            famicom_four_players_strobe: false,
            famicom_four_players_index: [0, 0],
        }
    }

    pub(crate) fn set_four_score_enabled(&mut self, enabled: bool) {
        self.four_score_enabled = enabled;
        self.four_score_index = [0, 0];
        self.four_score_strobe = false;
    }

    pub(crate) fn set_famicom_four_players_enabled(&mut self, enabled: bool) {
        self.famicom_four_players_enabled = enabled;
        self.famicom_four_players_index = [0, 0];
        self.famicom_four_players_strobe = false;
    }

    fn read_four_score_bit(&mut self, port_index: usize, is_dummy_read: bool) -> u8 {
        let idx = self.four_score_index[port_index];

        // Always read the full controller state once so we can preserve any
        // non-joypad bits (e.g., Zapper, Arkanoid) while still applying
        // Four Score's serial protocol on bit 0.
        let mut controller_state = self.controllers[port_index]
            .borrow_mut()
            .read(is_dummy_read);

        let serial_bit = if idx < 8 {
            // First 8 bits are the underlying controller's serial data (bit 0).
            controller_state & 0x01
        } else if idx < 16 {
            // Next 8 bits are extra buttons (players 3/4).
            let extra_state = self.four_score_extra_button_states.borrow();
            let player_state = extra_state[port_index];
            (player_state >> (idx - 8)) & 0x01
        } else if idx < 24 {
            // Next 8 bits are the Four Score signature.
            let signature = if port_index == 0 { 0x10 } else { 0x20 };
            (signature >> (idx - 16)) & 0x01
        } else {
            // Remaining reads return 1.
            1
        };

        // Preserve all higher bits from the underlying controller and only
        // override bit 0 with the Four Score serial bit.
        controller_state = (controller_state & !0x01) | serial_bit;

        if !is_dummy_read && !self.four_score_strobe {
            self.four_score_index[port_index] = self.four_score_index[port_index].saturating_add(1);
        }

        controller_state
    }

    fn read_famicom_four_players_bit(&mut self, port_index: usize, is_dummy_read: bool) -> u8 {
        let mut controller_state = self.controllers[port_index]
            .borrow_mut()
            .read(is_dummy_read);

        let idx = self.famicom_four_players_index[port_index];
        let extra_state = self.four_score_extra_button_states.borrow()[port_index];
        let serial_bit = if idx < 8 {
            (extra_state >> idx) & 0x01
        } else {
            1
        };

        controller_state = (controller_state & !0x02) | (serial_bit << 1);

        if !is_dummy_read && !self.famicom_four_players_strobe {
            self.famicom_four_players_index[port_index] =
                self.famicom_four_players_index[port_index].saturating_add(1);
        }

        controller_state
    }
}

impl BusDevice for ControllerDevice {
    fn read(&mut self, addr: u16, open_bus: u8, is_dummy_read: bool) -> Option<u8> {
        let index = (addr - 0x4016) as usize;

        let controller_state = if self.four_score_enabled {
            self.read_four_score_bit(index, is_dummy_read)
        } else if self.famicom_four_players_enabled {
            self.read_famicom_four_players_bit(index, is_dummy_read)
        } else {
            self.controllers[index].borrow_mut().read(is_dummy_read)
        };
        // NES-001 open bus: only bits 5-7 are unconnected (open bus).
        // Bits 0-4 are driven by the controller I/O register:
        //   bit 0 = serial data, bits 1-2 = grounded, bits 3-4 = controller port.
        Some((open_bus & 0xE0) | controller_state)
    }

    fn write(&mut self, addr: u16, value: u8, _is_dummy_write: bool) -> bool {
        match addr {
            0x4016 => {
                let new_strobe = value & 0x01 != 0;
                if self.four_score_strobe && !new_strobe {
                    self.four_score_index = [0, 0];
                }
                self.four_score_strobe = new_strobe;

                if self.famicom_four_players_strobe && !new_strobe {
                    self.famicom_four_players_index = [0, 0];
                }
                self.famicom_four_players_strobe = new_strobe;

                self.controllers[0].borrow_mut().write_strobe(value);
                self.controllers[1].borrow_mut().write_strobe(value);
                true
            }
            0x4017 => false,
            _ => false,
        }
    }

    fn address_range(&self) -> RangeInclusive<u16> {
        0x4016..=0x4017
    }

    fn sync_controller_modes(
        &mut self,
        four_score_enabled: bool,
        famicom_four_players_enabled: bool,
    ) {
        self.set_four_score_enabled(four_score_enabled);
        self.set_famicom_four_players_enabled(famicom_four_players_enabled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{Button, ControllerInput};

    fn read_24_bits(device: &mut ControllerDevice, addr: u16) -> u32 {
        let mut value = 0u32;
        for bit in 0..24 {
            let sample = device.read(addr, 0x00, false).unwrap() & 0x01;
            value |= (sample as u32) << bit;
        }
        value
    }

    struct TestController {
        reads: Rc<RefCell<u32>>,
        dummy_reads: Rc<RefCell<u32>>,
    }

    impl TestController {
        fn new(reads: Rc<RefCell<u32>>, dummy_reads: Rc<RefCell<u32>>) -> Self {
            Self { reads, dummy_reads }
        }
    }

    impl Controller for TestController {
        fn write_strobe(&mut self, _value: u8) {}

        fn read(&mut self, is_dummy_read: bool) -> u8 {
            if is_dummy_read {
                *self.dummy_reads.borrow_mut() += 1;
            } else {
                *self.reads.borrow_mut() += 1;
            }
            0
        }

        fn capture_state(&self) -> crate::input::ControllerState {
            crate::input::ControllerState::Joypad(crate::input::JoypadState {
                strobe: false,
                button_index: 0,
                button_states: 0,
            })
        }

        fn restore_state(&mut self, _state: &crate::input::ControllerState) {}

        fn set_button(&mut self, _button: Button, _pressed: bool) -> bool {
            true
        }

        fn set_mouse_x_position(&mut self, _position: u8) -> bool {
            false
        }

        fn set_mouse_y_position(&mut self, _position: u8) -> bool {
            false
        }

        fn set_mouse_left_button(&mut self, _pressed: bool) -> bool {
            false
        }

        fn input_type(&self) -> ControllerInput {
            ControllerInput::Gamepad
        }
    }

    fn create_test_controller_device() -> ControllerDevice {
        let reads = Rc::new(RefCell::new(0));
        let dummy_reads = Rc::new(RefCell::new(0));
        let controller1: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(
            TestController::new(reads.clone(), dummy_reads.clone()),
        )));
        let controller2: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(
            TestController::new(reads, dummy_reads),
        )));
        ControllerDevice::new(controller1, controller2)
    }

    #[test]
    fn test_dummy_read_uses_no_clock() {
        let reads = Rc::new(RefCell::new(0));
        let dummy_reads = Rc::new(RefCell::new(0));
        let reads_check = reads.clone();
        let dummy_reads_check = dummy_reads.clone();
        let controller1 = Rc::new(RefCell::new(Box::new(TestController::new(
            reads.clone(),
            dummy_reads.clone(),
        )) as Box<dyn Controller>));
        let controller2 = Rc::new(RefCell::new(
            Box::new(TestController::new(reads, dummy_reads)) as Box<dyn Controller>,
        ));

        let mut device = ControllerDevice::new(controller1, controller2);

        device.read(0x4016, 0xFF, true);

        assert_eq!(*reads_check.borrow(), 0);
        assert_eq!(*dummy_reads_check.borrow(), 1);
    }

    /// On NES-001, only bits 5-7 of $4016/$4017 are open bus.
    /// Bits 0-4 are driven by the controller I/O register.
    /// With open_bus = $BF and controller returning 0,
    /// the result should be $A0 (bits 5,7 from open bus).
    #[test]
    fn test_gamepad_open_bus_only_on_bits_5_to_7() {
        let mut device = create_test_controller_device();

        let result = device.read(0x4016, 0xBF, false).unwrap();
        assert_eq!(
            result, 0xA0,
            "Expected $A0 (only bits 5-7 from open bus), got ${:02X}",
            result
        );
    }

    /// With open_bus = $40, only bits 5-7 should reflect open bus.
    /// Controller returns 0, so result should be $40.
    #[test]
    fn test_gamepad_open_bus_with_40() {
        let mut device = create_test_controller_device();

        let result = device.read(0x4016, 0x40, false).unwrap();
        assert_eq!(
            result, 0x40,
            "Expected $40 (bits 5-7 from open bus $40), got ${:02X}",
            result
        );
    }

    #[test]
    fn test_four_score_port1_sequence() {
        let reads = Rc::new(RefCell::new(0));
        let dummy_reads = Rc::new(RefCell::new(0));
        let controller1: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(
            TestController::new(reads.clone(), dummy_reads.clone()),
        )));
        let controller2: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(
            TestController::new(reads, dummy_reads),
        )));
        let mut device = ControllerDevice::new(controller1, controller2);
        device.set_four_score_enabled(true);

        // Strobe high->low latches and resets shift state.
        assert!(device.write(0x4016, 1, false));
        assert!(device.write(0x4016, 0, false));

        // Expected Four Score sequence on $4016:
        // P1 byte (all 0 in this fixture), P3 byte (all 0 in this fixture), signature $10.
        let bits = read_24_bits(&mut device, 0x4016);
        assert_eq!(bits, 0x0010_0000);
    }

    #[test]
    fn test_four_score_port2_sequence() {
        let reads = Rc::new(RefCell::new(0));
        let dummy_reads = Rc::new(RefCell::new(0));
        let controller1: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(
            TestController::new(reads.clone(), dummy_reads.clone()),
        )));
        let controller2: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(
            TestController::new(reads, dummy_reads),
        )));
        let mut device = ControllerDevice::new(controller1, controller2);
        device.set_four_score_enabled(true);

        // Strobe high->low latches and resets shift state.
        assert!(device.write(0x4016, 1, false));
        assert!(device.write(0x4016, 0, false));

        // Expected Four Score sequence on $4017:
        // P2 byte (all 0 in this fixture), P4 byte (all 0 in this fixture), signature $20.
        let bits = read_24_bits(&mut device, 0x4017);
        assert_eq!(bits, 0x0020_0000);
    }

    #[test]
    fn test_famicom_four_players_sets_player3_serial_on_4016_bit1() {
        let reads = Rc::new(RefCell::new(0));
        let dummy_reads = Rc::new(RefCell::new(0));
        let controller1: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(
            TestController::new(reads.clone(), dummy_reads.clone()),
        )));
        let controller2: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(
            TestController::new(reads, dummy_reads),
        )));
        let extra_states = Rc::new(RefCell::new([0x01, 0x00]));
        let mut device = ControllerDevice::new_with_four_score_state(
            controller1,
            controller2,
            false,
            true,
            extra_states,
        );

        assert!(device.write(0x4016, 1, false));
        assert!(device.write(0x4016, 0, false));

        let first = device.read(0x4016, 0x00, false).unwrap();
        assert_eq!(first & 0x02, 0x02);
    }

    #[test]
    fn test_famicom_four_players_sets_player4_serial_on_4017_bit1() {
        let reads = Rc::new(RefCell::new(0));
        let dummy_reads = Rc::new(RefCell::new(0));
        let controller1: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(
            TestController::new(reads.clone(), dummy_reads.clone()),
        )));
        let controller2: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(
            TestController::new(reads, dummy_reads),
        )));
        let extra_states = Rc::new(RefCell::new([0x00, 0x01]));
        let mut device = ControllerDevice::new_with_four_score_state(
            controller1,
            controller2,
            false,
            true,
            extra_states,
        );

        assert!(device.write(0x4016, 1, false));
        assert!(device.write(0x4016, 0, false));

        let first = device.read(0x4017, 0x00, false).unwrap();
        assert_eq!(first & 0x02, 0x02);
    }

    /// Famicom controller 2 has a microphone whose state is read on $4016 bit 2.
    /// This is a silent stub: bit 2 always reads 0 (no microphone input).
    #[test]
    fn test_famicom_microphone_bit2_of_4016_is_always_zero() {
        let reads = Rc::new(RefCell::new(0));
        let dummy_reads = Rc::new(RefCell::new(0));
        let controller1: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(
            TestController::new(reads.clone(), dummy_reads.clone()),
        )));
        let controller2: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(
            TestController::new(reads, dummy_reads),
        )));
        let mut device = ControllerDevice::new_with_four_score_state(
            controller1,
            controller2,
            false,
            true,                                // famicom_four_players_enabled (Famicom mode)
            Rc::new(RefCell::new([0xFF, 0xFF])), // all buttons pressed
        );

        // Strobe and read multiple times
        assert!(device.write(0x4016, 1, false));
        assert!(device.write(0x4016, 0, false));

        for _ in 0..16 {
            let value = device.read(0x4016, 0x00, false).unwrap();
            assert_eq!(
                value & 0x04,
                0,
                "Microphone bit (bit 2) should always be 0, got ${:02X}",
                value
            );
        }
    }
}
