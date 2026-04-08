use crate::bus::bus::{BusDevice, ControllerModes};
use crate::cartridge::VsHardwareType;
use crate::input::ArkanoidController;
use crate::input::Controller;
use crate::input::Zapper;
use crate::input::{Button, PowerPad};
use std::cell::{Cell, RefCell};
use std::ops::RangeInclusive;
use std::rc::Rc;

/// Packed bit flags for VS System arcade input, shared between Bus and
/// ControllerDevice via `Rc<Cell<u8>>`.
pub(crate) const VS_INPUT_COIN_SLOT1: u8 = 0x01;
pub(crate) const VS_INPUT_COIN_SLOT2: u8 = 0x02;
pub(crate) const VS_INPUT_SERVICE: u8 = 0x04;

pub(crate) struct ControllerDevice {
    controllers: [Rc<RefCell<Box<dyn Controller>>>; 2],
    four_score_extra_button_states: Rc<RefCell<[u8; 2]>>,
    four_score_enabled: bool,
    four_score_strobe: bool,
    four_score_index: [u8; 2],
    famicom_four_players_enabled: bool,
    famicom_four_players_strobe: bool,
    famicom_four_players_index: [u8; 2],
    famicom_mode: bool,
    arkanoid_expansion: Option<Rc<RefCell<ArkanoidController>>>,
    arkanoid_famicom_enabled: bool,
    zapper_expansion: Option<Rc<RefCell<Zapper>>>,
    zapper_famicom_enabled: bool,
    power_pad_expansion: Option<Rc<RefCell<PowerPad>>>,
    power_pad_famicom_enabled: bool,
    vs_system_enabled: bool,
    vs_arcade_input: Rc<Cell<u8>>,
    vs_dip_switches: u8,
    vs_hardware_type: Option<VsHardwareType>,
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
            famicom_mode: false,
            arkanoid_expansion: None,
            arkanoid_famicom_enabled: false,
            zapper_expansion: None,
            zapper_famicom_enabled: false,
            power_pad_expansion: None,
            power_pad_famicom_enabled: false,
            vs_system_enabled: false,
            vs_arcade_input: Rc::new(Cell::new(0)),
            vs_dip_switches: 0,
            vs_hardware_type: None,
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

    pub(crate) fn set_arkanoid_famicom_expansion(
        &mut self,
        expansion: Option<Rc<RefCell<ArkanoidController>>>,
    ) {
        self.arkanoid_expansion = expansion;
    }

    pub(crate) fn set_arkanoid_famicom_enabled(&mut self, enabled: bool) {
        self.arkanoid_famicom_enabled = enabled;
    }

    pub(crate) fn set_zapper_famicom_expansion(&mut self, expansion: Option<Rc<RefCell<Zapper>>>) {
        self.zapper_expansion = expansion;
    }

    pub(crate) fn set_zapper_famicom_enabled(&mut self, enabled: bool) {
        self.zapper_famicom_enabled = enabled;
    }

    pub(crate) fn set_power_pad_famicom_expansion(
        &mut self,
        expansion: Option<Rc<RefCell<PowerPad>>>,
    ) {
        self.power_pad_expansion = expansion;
    }

    pub(crate) fn set_power_pad_famicom_enabled(&mut self, enabled: bool) {
        self.power_pad_famicom_enabled = enabled;
    }

    pub(crate) fn set_vs_system_enabled(&mut self, enabled: bool) {
        self.vs_system_enabled = enabled;
    }

    pub(crate) fn set_vs_arcade_input(&mut self, input: Rc<Cell<u8>>) {
        self.vs_arcade_input = input;
    }

    pub(crate) fn set_vs_dip_switches(&mut self, value: u8) {
        self.vs_dip_switches = value;
    }

    /// Read VS System controller register.
    ///
    /// In VS System mode all 8 bits of $4016/$4017 are hardware-defined
    /// (no open-bus bits).
    ///
    /// $4016 read: `PCCD DS0B`
    ///   bit 0: right stick serial data
    ///   bit 1: always 0
    ///   bit 2: service button
    ///   bits 3-4: DIP switches 1-2
    ///   bits 5-6: coin inserted (slot 1, slot 2)
    ///   bit 7: 0 = primary CPU (UniSystem)
    ///
    /// $4017 read: `DDDD DD0B`
    ///   bit 0: left stick serial data
    ///   bit 1: always 0
    ///   bits 2-7: DIP switches 3-8
    fn read_vs_system_bit(&mut self, port_index: usize, is_dummy_read: bool) -> u8 {
        let controller_bit = self.controllers[port_index]
            .borrow_mut()
            .read(is_dummy_read)
            & 0x01;

        if port_index == 0 {
            let arcade = self.vs_arcade_input.get();
            let mut value = controller_bit;
            if arcade & VS_INPUT_SERVICE != 0 {
                value |= 0x04;
            }
            value |= (self.vs_dip_switches & 0x03) << 3;
            if arcade & VS_INPUT_COIN_SLOT1 != 0 {
                value |= 0x20;
            }
            if arcade & VS_INPUT_COIN_SLOT2 != 0 {
                value |= 0x40;
            }
            value
        } else {
            let mut value = controller_bit;
            value |= self.vs_dip_switches & 0xFC;
            value
        }
    }

    fn read_arkanoid_famicom_bit(&mut self, port_index: usize, is_dummy_read: bool) -> u8 {
        let mut controller_state = self.controllers[port_index]
            .borrow_mut()
            .read(is_dummy_read);

        if let Some(ref arkanoid) = self.arkanoid_expansion {
            let expansion_bit = if port_index == 0 {
                // $4016: fire button on bit 1
                arkanoid.borrow().read_expansion_trigger()
            } else {
                // $4017: serial knob data on bit 1
                arkanoid.borrow_mut().read_expansion_knob(is_dummy_read)
            };
            controller_state = (controller_state & !0x02) | expansion_bit;
        }

        controller_state
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

    fn read_zapper_famicom_bit(&mut self, port_index: usize, is_dummy_read: bool) -> u8 {
        let mut controller_state = self.controllers[port_index]
            .borrow_mut()
            .read(is_dummy_read);

        // Famicom expansion connector only routes bits 1-4 on $4017;
        // $4016 only exposes bit 1 from expansion, so Zapper bits 3-4
        // are not connected on $4016 reads.
        if let (1, Some(zapper)) = (port_index, &self.zapper_expansion) {
            // Zapper expansion port: trigger on bit 4, light sense on bit 3
            let zapper_bits = zapper.borrow_mut().read(is_dummy_read);
            controller_state = (controller_state & !0x18) | (zapper_bits & 0x18);
        }

        controller_state
    }

    fn read_power_pad_famicom_bit(&mut self, port_index: usize, is_dummy_read: bool) -> u8 {
        if port_index != 1 {
            return self.controllers[port_index]
                .borrow_mut()
                .read(is_dummy_read);
        }

        let mut controller_state = self.controllers[port_index]
            .borrow_mut()
            .read(is_dummy_read);

        if let Some(power_pad) = &self.power_pad_expansion {
            let power_pad_bits = power_pad.borrow_mut().read(is_dummy_read);
            controller_state = (controller_state & !0x18) | (power_pad_bits & 0x18);
        }

        controller_state
    }
}

impl BusDevice for ControllerDevice {
    fn read(&mut self, addr: u16, open_bus: u8, is_dummy_read: bool) -> Option<u8> {
        let index = (addr - 0x4016) as usize;

        // VS System: all 8 bits are hardware-defined, no open bus
        if self.vs_system_enabled {
            return Some(self.read_vs_system_bit(index, is_dummy_read));
        }

        let mut controller_state = if self.four_score_enabled {
            self.read_four_score_bit(index, is_dummy_read)
        } else if self.famicom_four_players_enabled {
            self.read_famicom_four_players_bit(index, is_dummy_read)
        } else if self.arkanoid_famicom_enabled {
            self.read_arkanoid_famicom_bit(index, is_dummy_read)
        } else if self.zapper_famicom_enabled {
            self.read_zapper_famicom_bit(index, is_dummy_read)
        } else if self.power_pad_famicom_enabled {
            self.read_power_pad_famicom_bit(index, is_dummy_read)
        } else {
            self.controllers[index].borrow_mut().read(is_dummy_read)
        };
        // In Famicom mode, $4016 bit 2 is the controller 2 microphone line.
        // Until mic input is implemented, we explicitly stub it as always 0
        // to match hardware docs/tests and avoid leaking controller state.
        if self.famicom_mode && addr == 0x4016 {
            controller_state &= !0x04;
        }
        // Open bus behavior differs by hardware model:
        // NES-001: bits 5-7 are open bus; bits 0-4 driven by controller I/O.
        // Famicom (HVC-001): bits 3-7 are open bus; bits 0-2 driven
        //   (bit 0 = serial data, bit 1 = expansion port, bit 2 = mic/$4016 only).
        // When Zapper expansion is active on $4017, bits 3-4 are driven by
        //   the Zapper via the expansion port pins, so only bits 5-7 are open bus.
        //   $4016 keeps the standard Famicom mask since bits 3-4 are not routed
        //   to the expansion connector on that port.
        let open_bus_mask = if self.famicom_mode
            && !(self.zapper_famicom_enabled && addr == 0x4017)
            && !(self.power_pad_famicom_enabled && addr == 0x4017)
        {
            0xF8
        } else {
            0xE0
        };
        Some((open_bus & open_bus_mask) | (controller_state & !open_bus_mask))
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

                // Ice Climber Japan and Raid on Bungeling Bay force Start on
                // both controllers as an anti-piracy measure.
                if matches!(
                    self.vs_hardware_type,
                    Some(VsHardwareType::IceClimberJapan | VsHardwareType::RaidOnBungelingBay)
                ) {
                    self.controllers[0]
                        .borrow_mut()
                        .set_button(Button::Start, true);
                    self.controllers[1]
                        .borrow_mut()
                        .set_button(Button::Start, true);
                }

                if let Some(ref arkanoid) = self.arkanoid_expansion {
                    arkanoid.borrow_mut().write_strobe(value);
                }
                if let Some(ref power_pad) = self.power_pad_expansion {
                    power_pad.borrow_mut().write_strobe(value);
                }

                true
            }
            0x4017 => false,
            _ => false,
        }
    }

    fn address_range(&self) -> RangeInclusive<u16> {
        0x4016..=0x4017
    }

    fn sync_controller_modes(&mut self, modes: &ControllerModes) {
        self.set_four_score_enabled(modes.four_score_enabled);
        self.set_famicom_four_players_enabled(modes.famicom_four_players_enabled);
        self.famicom_mode = modes.famicom_mode;
        self.set_arkanoid_famicom_enabled(modes.arkanoid_famicom_enabled);
        self.set_zapper_famicom_enabled(modes.zapper_famicom_enabled);
        self.set_power_pad_famicom_enabled(modes.power_pad_famicom_enabled);
        self.set_vs_system_enabled(modes.vs_system_enabled);
        self.set_vs_dip_switches(modes.vs_dip_switches);
        self.vs_hardware_type = modes.vs_hardware_type;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{Button, ControllerInput, PowerPad};

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

    /// On Famicom (HVC-001), bits 3-7 of $4016/$4017 are open bus.
    /// Bits 0-2 are driven (serial data, expansion, microphone).
    /// With open_bus = $40 and controller returning 0,
    /// the result should be $40 (bits 3-7 from open bus).
    #[test]
    fn test_famicom_open_bus_bits_3_to_7() {
        let mut device = create_test_controller_device();
        device.famicom_mode = true;

        let result = device.read(0x4016, 0x40, false).unwrap();
        assert_eq!(
            result, 0x40,
            "Expected $40 (bits 3-7 from open bus $40), got ${:02X}",
            result
        );
    }

    /// On Famicom, bits 3-4 should come from open bus, unlike NES-001.
    /// With open_bus = $18 (bits 3-4 set), controller returning 0,
    /// the result should be $18 on Famicom but $00 on NES-001.
    #[test]
    fn test_famicom_open_bus_bits_3_4_differ_from_nes() {
        let mut device = create_test_controller_device();

        // NES-001: bits 3-4 are driven (grounded), not open bus
        let nes_result = device.read(0x4016, 0x18, false).unwrap();
        assert_eq!(
            nes_result, 0x00,
            "NES-001 should ground bits 3-4, got ${:02X}",
            nes_result
        );

        // Famicom: bits 3-4 are open bus
        device.famicom_mode = true;
        let famicom_result = device.read(0x4016, 0x18, false).unwrap();
        assert_eq!(
            famicom_result, 0x18,
            "Famicom should pass bits 3-4 from open bus, got ${:02X}",
            famicom_result
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

    fn create_arkanoid_expansion_device() -> (ControllerDevice, Rc<RefCell<ArkanoidController>>) {
        let reads = Rc::new(RefCell::new(0));
        let dummy_reads = Rc::new(RefCell::new(0));
        let controller1: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(
            TestController::new(reads.clone(), dummy_reads.clone()),
        )));
        let controller2: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(
            TestController::new(reads, dummy_reads),
        )));
        let arkanoid = Rc::new(RefCell::new(ArkanoidController::new()));
        let mut device = ControllerDevice::new(controller1, controller2);
        device.set_arkanoid_famicom_expansion(Some(arkanoid.clone()));
        device.set_arkanoid_famicom_enabled(true);
        device.famicom_mode = true;
        (device, arkanoid)
    }

    #[test]
    fn test_arkanoid_famicom_expansion_fire_on_4016_bit1() {
        let (mut device, arkanoid) = create_arkanoid_expansion_device();

        // Fire not pressed
        let value = device.read(0x4016, 0x00, false).unwrap();
        assert_eq!(
            value & 0x02,
            0x00,
            "Fire should be 0 when not pressed, got ${:02X}",
            value
        );

        // Fire pressed
        arkanoid.borrow_mut().set_trigger(true);
        let value = device.read(0x4016, 0x00, false).unwrap();
        assert_eq!(
            value & 0x02,
            0x02,
            "Fire should be 1 on bit 1 when pressed, got ${:02X}",
            value
        );
    }

    #[test]
    fn test_arkanoid_famicom_expansion_knob_on_4017_bit1() {
        let (mut device, arkanoid) = create_arkanoid_expansion_device();
        arkanoid.borrow_mut().set_position(0x92); // inverted: 0b0110_1101

        // Strobe to latch position
        assert!(device.write(0x4016, 1, false));
        assert!(device.write(0x4016, 0, false));

        // Read MSB first (inverted) on bit 1 of $4017
        let expected_bits = [0, 1, 1, 0, 1, 1, 0, 1];
        for (i, expected) in expected_bits.iter().enumerate() {
            let value = device.read(0x4017, 0x00, false).unwrap();
            assert_eq!(
                (value >> 1) & 0x01,
                *expected,
                "Bit {} expected {}, got value 0x{:02X}",
                i,
                expected,
                value
            );
        }
    }

    #[test]
    fn test_arkanoid_famicom_expansion_strobe_forwarded() {
        let (mut device, arkanoid) = create_arkanoid_expansion_device();
        arkanoid.borrow_mut().set_position(0x92);

        // Strobe and read first bit
        assert!(device.write(0x4016, 1, false));
        assert!(device.write(0x4016, 0, false));

        let first = device.read(0x4017, 0x00, false).unwrap();
        let _ = device.read(0x4017, 0x00, false).unwrap(); // advance

        // Strobe again - should reset shift register
        assert!(device.write(0x4016, 1, false));
        assert!(device.write(0x4016, 0, false));

        let after_strobe = device.read(0x4017, 0x00, false).unwrap();
        assert_eq!(
            first & 0x02,
            after_strobe & 0x02,
            "Strobe should reset shift register to first bit"
        );
    }

    #[test]
    fn test_arkanoid_famicom_expansion_does_not_affect_port_controller_bit0() {
        let (mut device, _arkanoid) = create_arkanoid_expansion_device();

        // Port controller returns 0 for all reads (TestController always returns 0)
        // Arkanoid expansion should only set bit 1, leaving bit 0 from port controller
        let value = device.read(0x4016, 0x00, false).unwrap();
        assert_eq!(
            value & 0x01,
            0x00,
            "Bit 0 should come from port controller, got ${:02X}",
            value
        );
    }

    fn create_zapper_expansion_device() -> (ControllerDevice, Rc<RefCell<crate::input::Zapper>>) {
        let reads = Rc::new(RefCell::new(0));
        let dummy_reads = Rc::new(RefCell::new(0));
        let controller1: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(
            TestController::new(reads.clone(), dummy_reads.clone()),
        )));
        let controller2: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(
            TestController::new(reads, dummy_reads),
        )));

        let ppu = Rc::new(RefCell::new(crate::ppu::Ppu::new_for_testing(
            crate::console::TimingMode::Ntsc,
        )));
        let app_context = Rc::new(RefCell::new(
            crate::app_context::AppContext::new_with_config(crate::console::Config::default()),
        ));
        let zapper = Rc::new(RefCell::new(crate::input::Zapper::new(ppu, app_context)));

        let mut device = ControllerDevice::new(controller1, controller2);
        device.set_zapper_famicom_expansion(Some(zapper.clone()));
        device.set_zapper_famicom_enabled(true);
        device.famicom_mode = true;
        (device, zapper)
    }

    fn create_power_pad_expansion_device() -> (ControllerDevice, Rc<RefCell<PowerPad>>) {
        let reads = Rc::new(RefCell::new(0));
        let dummy_reads = Rc::new(RefCell::new(0));
        let controller1: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(
            TestController::new(reads.clone(), dummy_reads.clone()),
        )));
        let controller2: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(
            TestController::new(reads, dummy_reads),
        )));

        let power_pad = Rc::new(RefCell::new(PowerPad::new()));
        let mut device = ControllerDevice::new(controller1, controller2);
        device.set_power_pad_famicom_expansion(Some(power_pad.clone()));
        device.set_power_pad_famicom_enabled(true);
        device.famicom_mode = true;
        (device, power_pad)
    }

    #[test]
    fn test_power_pad_famicom_expansion_serial_on_4017_bits_3_4() {
        let (mut device, power_pad) = create_power_pad_expansion_device();
        power_pad
            .borrow_mut()
            .set_button(crate::input::PowerPadButton::One, true);
        power_pad
            .borrow_mut()
            .set_button(crate::input::PowerPadButton::Four, true);

        assert!(device.write(0x4016, 1, false));
        assert!(device.write(0x4016, 0, false));

        let first = device.read(0x4017, 0x00, false).unwrap();
        let second = device.read(0x4017, 0x00, false).unwrap();
        assert_eq!(first & 0x18, 0x10);
        assert_eq!(second & 0x18, 0x08);
    }

    /// Famicom Zapper expansion port: trigger should appear on bit 4 of $4017 reads.
    #[test]
    fn test_zapper_famicom_expansion_trigger_on_4017_bit4() {
        let (mut device, zapper) = create_zapper_expansion_device();

        // Trigger not pressed
        let value = device.read(0x4017, 0x00, false).unwrap();
        assert_eq!(
            value & 0x10,
            0x00,
            "Trigger bit 4 should be 0 when not pressed, got ${:02X}",
            value
        );

        // Trigger pressed
        zapper.borrow_mut().set_mouse_left_button(true);
        let value = device.read(0x4017, 0x00, false).unwrap();
        assert_eq!(
            value & 0x10,
            0x10,
            "Trigger bit 4 should be 1 when pressed, got ${:02X}",
            value
        );
    }

    /// Famicom expansion connector only routes bits 1-4 on $4017; $4016 does
    /// not carry Zapper bits 3-4, so trigger must NOT appear on $4016.
    #[test]
    fn test_zapper_famicom_expansion_trigger_not_on_4016() {
        let (mut device, zapper) = create_zapper_expansion_device();

        zapper.borrow_mut().set_mouse_left_button(true);
        let value = device.read(0x4016, 0x00, false).unwrap();
        assert_eq!(
            value & 0x10,
            0x00,
            "Trigger bit 4 should NOT appear on $4016 (expansion bits only on $4017), got ${:02X}",
            value
        );
    }

    /// Famicom Zapper expansion port: light sense (no light) should set bit 3.
    #[test]
    fn test_zapper_famicom_expansion_light_sense_no_light_on_bit3() {
        let (mut device, _zapper) = create_zapper_expansion_device();

        // With no light detected (default state), bit 3 should be 1
        let value = device.read(0x4017, 0x00, false).unwrap();
        assert_eq!(
            value & 0x08,
            0x08,
            "Light sense bit 3 should be 1 when no light detected, got ${:02X}",
            value
        );
    }

    /// Famicom Zapper expansion port: standard controller bit 0 is preserved.
    #[test]
    fn test_zapper_famicom_expansion_does_not_affect_bit0() {
        let (mut device, _zapper) = create_zapper_expansion_device();

        // TestController always returns 0, so bit 0 should be 0
        let value = device.read(0x4017, 0x00, false).unwrap();
        assert_eq!(
            value & 0x01,
            0x00,
            "Bit 0 should come from port controller, got ${:02X}",
            value
        );
    }

    /// When Zapper expansion is active in Famicom mode, bits 3-4 on $4017
    /// should NOT be masked as open bus (the Zapper drives these expansion port pins).
    /// With open_bus = 0x00, the only source for bits 3-4 is the Zapper.
    #[test]
    fn test_zapper_famicom_expansion_open_bus_preserves_bits_3_4_on_4017() {
        let (mut device, zapper) = create_zapper_expansion_device();

        // Trigger pressed (bit 4), no light (bit 3) → both should come from Zapper, not open bus
        zapper.borrow_mut().set_mouse_left_button(true);
        let value = device.read(0x4017, 0x00, false).unwrap();
        assert_eq!(
            value & 0x18,
            0x18,
            "Bits 3-4 should be driven by Zapper expansion (not open bus 0x00). Got ${:02X}",
            value
        );
    }

    /// On $4016, bits 3-4 should remain open bus even with Zapper expansion active,
    /// since the Famicom expansion connector doesn't route those bits to $4016.
    #[test]
    fn test_zapper_famicom_expansion_open_bus_4016_keeps_famicom_mask() {
        let (mut device, zapper) = create_zapper_expansion_device();

        // Trigger pressed, but $4016 should use Famicom open bus mask (0xF8)
        zapper.borrow_mut().set_mouse_left_button(true);
        let value = device.read(0x4016, 0xFF, false).unwrap();
        assert_eq!(
            value & 0xF8,
            0xF8,
            "$4016 bits 3-7 should be open bus (mask 0xF8) even with Zapper expansion. Got ${:02X}",
            value
        );
    }

    // ── VS System tests ──────────────────────────────────────────────────────

    /// Create a VS System-enabled controller device with a shared arcade input cell.
    fn create_vs_controller_device() -> (ControllerDevice, Rc<Cell<u8>>) {
        let mut device = create_test_controller_device();
        let arcade_input = Rc::new(Cell::new(0u8));
        device.set_vs_arcade_input(arcade_input.clone());
        device.set_vs_system_enabled(true);
        (device, arcade_input)
    }

    #[test]
    fn test_vs_system_4016_read_has_zero_on_bit_1() {
        let (mut device, _) = create_vs_controller_device();
        let result = device.read(0x4016, 0xFF, false).unwrap();
        assert_eq!(result & 0x02, 0x00, "VS $4016 bit 1 should always be 0");
    }

    #[test]
    fn test_vs_system_4016_read_service_button_on_bit_2() {
        let (mut device, arcade_input) = create_vs_controller_device();

        // Service button not pressed
        let result = device.read(0x4016, 0xFF, false).unwrap();
        assert_eq!(
            result & 0x04,
            0x00,
            "Service button should be 0 when not pressed"
        );

        // Service button pressed
        arcade_input.set(VS_INPUT_SERVICE);
        let result = device.read(0x4016, 0xFF, false).unwrap();
        assert_eq!(
            result & 0x04,
            0x04,
            "Service button should set bit 2 of $4016"
        );
    }

    #[test]
    fn test_vs_system_4016_read_dip_switches_on_bits_3_4() {
        let (mut device, _) = create_vs_controller_device();

        // DIP 1 on (bit 0 of dip_switches) → $4016 bit 3
        device.set_vs_dip_switches(0x01);
        let result = device.read(0x4016, 0xFF, false).unwrap();
        assert_eq!(
            result & 0x18,
            0x08,
            "DIP switch 1 should map to $4016 bit 3"
        );

        // DIP 2 on (bit 1 of dip_switches) → $4016 bit 4
        device.set_vs_dip_switches(0x02);
        let result = device.read(0x4016, 0xFF, false).unwrap();
        assert_eq!(
            result & 0x18,
            0x10,
            "DIP switch 2 should map to $4016 bit 4"
        );

        // Both DIP 1 and 2 on
        device.set_vs_dip_switches(0x03);
        let result = device.read(0x4016, 0xFF, false).unwrap();
        assert_eq!(result & 0x18, 0x18, "DIP switches 1+2 should set bits 3-4");
    }

    #[test]
    fn test_vs_system_4016_read_coin_on_bits_5_6() {
        let (mut device, arcade_input) = create_vs_controller_device();

        // No coin
        let result = device.read(0x4016, 0xFF, false).unwrap();
        assert_eq!(result & 0x60, 0x00, "No coin bits when nothing inserted");

        // Coin slot 1
        arcade_input.set(VS_INPUT_COIN_SLOT1);
        let result = device.read(0x4016, 0xFF, false).unwrap();
        assert_eq!(result & 0x60, 0x20, "Coin slot 1 should set $4016 bit 5");

        // Coin slot 2
        arcade_input.set(VS_INPUT_COIN_SLOT2);
        let result = device.read(0x4016, 0xFF, false).unwrap();
        assert_eq!(result & 0x60, 0x40, "Coin slot 2 should set $4016 bit 6");

        // Both coin slots
        arcade_input.set(VS_INPUT_COIN_SLOT1 | VS_INPUT_COIN_SLOT2);
        let result = device.read(0x4016, 0xFF, false).unwrap();
        assert_eq!(result & 0x60, 0x60, "Both coin slots should set bits 5-6");
    }

    #[test]
    fn test_vs_system_4016_read_primary_cpu_bit_7_is_zero() {
        let (mut device, _) = create_vs_controller_device();
        let result = device.read(0x4016, 0xFF, false).unwrap();
        assert_eq!(
            result & 0x80,
            0x00,
            "VS $4016 bit 7 should be 0 (primary CPU)"
        );
    }

    #[test]
    fn test_vs_system_4017_read_has_zero_on_bit_1() {
        let (mut device, _) = create_vs_controller_device();
        let result = device.read(0x4017, 0xFF, false).unwrap();
        assert_eq!(result & 0x02, 0x00, "VS $4017 bit 1 should always be 0");
    }

    #[test]
    fn test_vs_system_4017_read_dip_switches_on_bits_2_7() {
        let (mut device, _) = create_vs_controller_device();

        // DIP 3 (bit 2 of dip_switches) → $4017 bit 2
        device.set_vs_dip_switches(0x04);
        let result = device.read(0x4017, 0xFF, false).unwrap();
        assert_eq!(
            result & 0xFC,
            0x04,
            "DIP switch 3 should map to $4017 bit 2"
        );

        // DIP 8 (bit 7 of dip_switches) → $4017 bit 7
        device.set_vs_dip_switches(0x80);
        let result = device.read(0x4017, 0xFF, false).unwrap();
        assert_eq!(
            result & 0xFC,
            0x80,
            "DIP switch 8 should map to $4017 bit 7"
        );

        // All DIP 3-8 on
        device.set_vs_dip_switches(0xFC);
        let result = device.read(0x4017, 0xFF, false).unwrap();
        assert_eq!(
            result & 0xFC,
            0xFC,
            "DIP switches 3-8 should set $4017 bits 2-7"
        );
    }

    #[test]
    fn test_vs_system_all_bits_defined_no_open_bus() {
        let (mut device, _) = create_vs_controller_device();
        // Open bus should be ignored — all 8 bits are hardware-defined in VS mode
        let result = device.read(0x4016, 0xFF, false).unwrap();
        assert_eq!(
            result, 0x00,
            "VS $4016 with no inputs should be 0x00 regardless of open bus"
        );

        let result = device.read(0x4017, 0xFF, false).unwrap();
        assert_eq!(
            result, 0x00,
            "VS $4017 with no inputs should be 0x00 regardless of open bus"
        );
    }

    #[test]
    fn test_vs_system_does_not_affect_non_vs_reads() {
        let mut device = create_test_controller_device();
        // VS mode NOT enabled — normal NES open bus behavior
        let result = device.read(0x4016, 0xFF, false).unwrap();
        assert_eq!(
            result & 0xE0,
            0xE0,
            "Non-VS mode should apply NES open bus (bits 5-7)"
        );
    }

    #[test]
    fn test_vs_system_4016_combined_bits() {
        let (mut device, arcade_input) = create_vs_controller_device();

        // Set DIP 1+2, coin slot 1, and service button
        device.set_vs_dip_switches(0x03);
        arcade_input.set(VS_INPUT_COIN_SLOT1 | VS_INPUT_SERVICE);

        let result = device.read(0x4016, 0xFF, false).unwrap();
        // Expected: bit 0 = 0 (controller), bit 1 = 0, bit 2 = 1 (service),
        //           bits 3-4 = 11 (DIP 1+2), bit 5 = 1 (coin), bits 6-7 = 0
        assert_eq!(
            result, 0x3C,
            "VS $4016 combined: service + DIP 1,2 + coin1 = $3C, got ${:02X}",
            result
        );
    }

    // ── VS System protection (forced Start button) tests ─────────────────

    type CtrlRef = Rc<RefCell<Box<dyn Controller>>>;

    fn create_vs_joypad_device(hw_type: VsHardwareType) -> (ControllerDevice, CtrlRef, CtrlRef) {
        let joypad1 = crate::input::NesJoypad::new();
        let joypad2 = crate::input::NesJoypad::new();
        let ctrl1: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(joypad1)));
        let ctrl2: Rc<RefCell<Box<dyn Controller>>> = Rc::new(RefCell::new(Box::new(joypad2)));
        let mut device = ControllerDevice::new(Rc::clone(&ctrl1), Rc::clone(&ctrl2));
        device.set_vs_system_enabled(true);
        device.vs_hardware_type = Some(hw_type);
        (device, ctrl1, ctrl2)
    }

    /// Read a specific serial bit from the controller by doing N+1 reads
    /// after a strobe. Returns bit 0 of the Nth read (0-indexed).
    fn read_nth_serial_bit(device: &mut ControllerDevice, addr: u16, n: usize) -> u8 {
        // Strobe: write 1 then 0 to $4016
        device.write(0x4016, 1, false);
        device.write(0x4016, 0, false);
        for _ in 0..n {
            device.read(addr, 0x00, false);
        }
        device.read(addr, 0x00, false).unwrap() & 0x01
    }

    #[test]
    fn ice_climber_protection_forces_start_bit_on_port1() {
        let (mut device, _ctrl1, _ctrl2) = create_vs_joypad_device(VsHardwareType::IceClimberJapan);
        // No buttons pressed — Start (bit 3, read index 3) should still be 1
        let start_bit = read_nth_serial_bit(&mut device, 0x4016, 3);
        assert_eq!(
            start_bit, 1,
            "Ice Climber: Start bit on $4016 (right stick) should be forced to 1"
        );
    }

    #[test]
    fn ice_climber_protection_forces_start_bit_on_port2() {
        let (mut device, _ctrl1, _ctrl2) = create_vs_joypad_device(VsHardwareType::IceClimberJapan);
        let start_bit = read_nth_serial_bit(&mut device, 0x4017, 3);
        assert_eq!(
            start_bit, 1,
            "Ice Climber: Start bit on $4017 (left stick) should be forced to 1"
        );
    }

    #[test]
    fn raid_on_bungeling_bay_forces_start_bit() {
        let (mut device, _ctrl1, _ctrl2) =
            create_vs_joypad_device(VsHardwareType::RaidOnBungelingBay);
        let start_bit = read_nth_serial_bit(&mut device, 0x4016, 3);
        assert_eq!(
            start_bit, 1,
            "Raid on Bungeling Bay: Start bit should be forced to 1"
        );
    }

    #[test]
    fn unisystem_does_not_force_start_bit() {
        let (mut device, _ctrl1, _ctrl2) = create_vs_joypad_device(VsHardwareType::Unisystem);
        // No buttons pressed — Start should be 0
        let start_bit = read_nth_serial_bit(&mut device, 0x4016, 3);
        assert_eq!(
            start_bit, 0,
            "Unisystem (no protection): Start bit should NOT be forced"
        );
    }
}
