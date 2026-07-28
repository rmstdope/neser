//! Issue #2889: ROM-level verification of the SNES Mouse.
//!
//! Written spec-first against the fullsnes / SNESdev mouse documentation.
//! After a strobe latch, the mouse shifts out a 32-bit packet on the port's
//! data1 line, MSB-first per byte:
//!
//! - bits 1-8: always 0
//! - bit 9: right button, bit 10: left button
//! - bits 11-12: sensitivity (speed) setting
//! - bits 13-16: hardware ID `0001`
//! - bit 17: vertical direction (1 = up), bits 18-24: vertical magnitude
//! - bit 25: horizontal direction (1 = left), bits 26-32: horizontal
//!   magnitude (7-bit, clamped to 127)
//!
//! Clocking the port (reading `$4016`) while the strobe is held high cycles
//! the sensitivity setting 0 -> 1 -> 2 -> 0; reads past bit 32 return 1.
//!
//! Fixtures are in-code assembled LoROM programs (see `fixture_rom`) that
//! strobe and serially read whole packets, reporting through the
//! `rom_runner` WRAM pass/fail marker. Host motion deltas use positive dx =
//! right and positive dy = down, so negative deltas set the direction bits.

#[cfg(test)]
mod tests {
    use crate::snes::input::SnesControllerType;
    use crate::snes::integration_tests::fixture_rom::FixtureRom;
    use crate::snes::integration_tests::rom_runner::{
        InputAction, InputEvent, MouseButton, RunConfig, RunExitReason, run_rom,
    };

    /// Packet scratch bytes: $0010 = bits 1-8, $0011 = buttons/speed/ID,
    /// $0012 = vertical, $0013 = horizontal, $0014 = post-packet tail.
    const PKT: u16 = 0x0010;

    fn mouse_ports() -> RunConfig<'static> {
        RunConfig::new(400_000_000, 120)
            .with_controller_ports(SnesControllerType::Mouse, SnesControllerType::Standard)
    }

    /// Emits a poll block that strobes `joy_addr`'s shared strobe, serially
    /// reads a full 32-bit packet, and loops until byte 2 (buttons/speed/ID),
    /// byte 3 (vertical) and byte 4 (horizontal) match. Also requires the
    /// always-zero first byte.
    fn wait_for_packet(
        fx: &mut FixtureRom,
        joy_addr: u16,
        status: u8,
        vertical: u8,
        horizontal: u8,
    ) {
        let poll = fx.pos();
        fx.strobe_pulse();
        fx.serial_read_bits(joy_addr, 32, PKT);
        fx.lda_abs(PKT);
        fx.cmp_imm(0x00);
        fx.bne_to(poll);
        fx.lda_abs(PKT + 1);
        fx.cmp_imm(status);
        fx.bne_to(poll);
        fx.lda_abs(PKT + 2);
        fx.cmp_imm(vertical);
        fx.bne_to(poll);
        fx.lda_abs(PKT + 3);
        fx.cmp_imm(horizontal);
        fx.bne_to(poll);
    }

    /// Emits a hang-on-mismatch compare: re-reads the (static) WRAM byte
    /// forever if it does not equal `expected`, so a mismatch surfaces as a
    /// frame-limit failure instead of a false pass.
    fn require_wram(fx: &mut FixtureRom, addr: u16, expected: u8) {
        let probe = fx.pos();
        fx.lda_abs(addr);
        fx.cmp_imm(expected);
        fx.bne_to(probe);
    }

    const fn mouse_delta(frame: u32, port: u8, dx: i16, dy: i16) -> InputEvent {
        InputEvent {
            frame,
            action: InputAction::MouseDelta { port, dx, dy },
        }
    }

    const fn mouse_button(frame: u32, button: MouseButton, pressed: bool) -> InputEvent {
        InputEvent {
            frame,
            action: InputAction::MouseButton {
                port: 0,
                button,
                pressed,
            },
        }
    }

    /// Given an idle mouse on port 1, when the ROM strobes and serially
    /// reads 40 bits, then bits 1-8 are zero, the status byte carries only
    /// the hardware ID `0001` (no buttons, speed 0), both motion bytes are
    /// zero, and the eight reads past bit 32 all return 1.
    #[test]
    fn identification_reports_id_nibble_zero_lead_byte_and_tail_ones() {
        let mut fx = FixtureRom::new(b"NESER MOUSE ID");
        let poll = fx.pos();
        fx.strobe_pulse();
        fx.serial_read_bits(0x4016, 40, PKT);
        fx.lda_abs(PKT);
        fx.cmp_imm(0x00); // bits 1-8: always zero
        fx.bne_to(poll);
        fx.lda_abs(PKT + 1);
        fx.cmp_imm(0x01); // no buttons, speed 0, ID 0001
        fx.bne_to(poll);
        fx.lda_abs(PKT + 2);
        fx.cmp_imm(0x00); // no vertical motion
        fx.bne_to(poll);
        fx.lda_abs(PKT + 3);
        fx.cmp_imm(0x00); // no horizontal motion
        fx.bne_to(poll);
        fx.lda_abs(PKT + 4);
        fx.cmp_imm(0xFF); // tail bits past bit 32 read as ones
        fx.bne_to(poll);
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let result = run_rom(&rom, "mouse-identify.sfc", mouse_ports());

        assert!(
            result.passed,
            "idle mouse packet should carry only the ID nibble \
             (exit={:?} frames={})",
            result.exit_reason, result.frames
        );
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
    }

    /// The issue's example sequence: identify, move right, left, down, up
    /// (sign + direction bit checked per fullsnes: 1 = up / left), press
    /// the left button, press the right button (both held), release both.
    #[test]
    fn example_sequence_motion_buttons_and_release() {
        // (status, vertical, horizontal) per step, in observation order.
        const STEPS: [(u8, u8, u8); 8] = [
            (0x01, 0x00, 0x00), // identify: idle packet
            (0x01, 0x00, 0x07), // right: dx=+7, direction bit clear
            (0x01, 0x00, 0x87), // left: dx=-7, direction bit set
            (0x01, 0x07, 0x00), // down: dy=+7, direction bit clear
            (0x01, 0x87, 0x00), // up: dy=-7, direction bit set
            (0x41, 0x00, 0x00), // left button held (bit 10)
            (0xC1, 0x00, 0x00), // both buttons held (bits 9+10)
            (0x01, 0x00, 0x00), // both released
        ];

        let mut fx = FixtureRom::new(b"NESER MOUSE SEQ");
        for (status, vertical, horizontal) in STEPS {
            wait_for_packet(&mut fx, 0x4016, status, vertical, horizontal);
        }
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let script = [
            mouse_delta(10, 0, 7, 0),
            mouse_delta(20, 0, -7, 0),
            mouse_delta(30, 0, 0, 7),
            mouse_delta(40, 0, 0, -7),
            mouse_button(50, MouseButton::Left, true),
            mouse_button(60, MouseButton::Right, true),
            mouse_button(70, MouseButton::Left, false),
            mouse_button(70, MouseButton::Right, false),
        ];
        let result = run_rom(
            &rom,
            "mouse-example-sequence.sfc",
            RunConfig::new(2_000_000_000, 240)
                .with_controller_ports(SnesControllerType::Mouse, SnesControllerType::Standard)
                .with_input_script(&script),
        );

        assert!(
            result.passed,
            "ROM should observe every packet of the example sequence in \
             order (exit={:?} frames={})",
            result.exit_reason, result.frames
        );
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
    }

    /// Clocking `$4016` while the strobe is held high cycles the
    /// sensitivity setting: packets read after 1, 2 and 3 clocks report
    /// speed 1, speed 2, and back to speed 0 in bits 11-12.
    #[test]
    fn speed_bits_cycle_on_clocks_while_strobe_high() {
        let mut fx = FixtureRom::new(b"NESER MOUSE SPEED");
        // Baseline packet: speed 0.
        fx.strobe_pulse();
        fx.serial_read_bits(0x4016, 32, PKT);
        require_wram(&mut fx, PKT + 1, 0x01);
        // Three rounds of: strobe high, one clock, strobe low, read packet.
        for expected_status in [0x11, 0x21, 0x01] {
            fx.write_long(0x00_4016, 0x01);
            fx.lda_abs(0x4016); // one clock while the strobe is high
            fx.write_long(0x00_4016, 0x00);
            fx.serial_read_bits(0x4016, 32, PKT);
            require_wram(&mut fx, PKT + 1, expected_status);
        }
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let result = run_rom(&rom, "mouse-speed-cycle.sfc", mouse_ports());

        assert!(
            result.passed,
            "speed bits should cycle 0 -> 1 -> 2 -> 0 on strobe-high clocks \
             (exit={:?} frames={})",
            result.exit_reason, result.frames
        );
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
    }

    /// A scripted delta far beyond the 7-bit range reports magnitude 127
    /// with the correct direction bits (dx=+200 -> right 0x7F, dy=-200 ->
    /// up 0x80|0x7F).
    #[test]
    fn magnitude_clamps_to_127_with_direction_bits() {
        let mut fx = FixtureRom::new(b"NESER MOUSE CLAMP");
        wait_for_packet(&mut fx, 0x4016, 0x01, 0xFF, 0x7F);
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let script = [mouse_delta(5, 0, 200, -200)];
        let result = run_rom(
            &rom,
            "mouse-clamp.sfc",
            mouse_ports().with_input_script(&script),
        );

        assert!(
            result.passed,
            "an oversized delta should clamp to a 7-bit magnitude of 127 \
             (exit={:?} frames={})",
            result.exit_reason, result.frames
        );
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
    }

    /// With a standard pad on port 1 and the mouse on port 2, the mouse
    /// packet (ID and a scripted +3 dx) is shifted out on `$4017` using the
    /// shared `$4016` strobe.
    #[test]
    fn mouse_on_port2_reports_packet_via_4017() {
        let mut fx = FixtureRom::new(b"NESER MOUSE PORT2");
        wait_for_packet(&mut fx, 0x4017, 0x01, 0x00, 0x03);
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let script = [mouse_delta(5, 1, 3, 0)];
        let result = run_rom(
            &rom,
            "mouse-port2.sfc",
            RunConfig::new(400_000_000, 120)
                .with_controller_ports(SnesControllerType::Standard, SnesControllerType::Mouse)
                .with_input_script(&script),
        );

        assert!(
            result.passed,
            "a port-2 mouse should shift its packet out on $4017 \
             (exit={:?} frames={})",
            result.exit_reason, result.frames
        );
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
    }
}
