//! Issue #2886: ROM-level verification of the standard SNES controller.
//!
//! Written spec-first against the fullsnes / SNESdev controller documentation:
//! `$4016` bit 0 is the shared strobe (latch while high); `$4016`/`$4017`
//! bit 0 are the port 1/port 2 data1 serial lines; a standard pad shifts out
//! B, Y, Select, Start, Up, Down, Left, Right, A, X, L, R, then four ID zeros,
//! and a connected pad returns all ones for every clock past bit 16.
//!
//! Fixtures are in-code assembled LoROM programs (see `fixture_rom`) that
//! poll the serial lines and report through the `rom_runner` WRAM marker.

#[cfg(test)]
mod tests {
    use crate::snes::input::SnesButton;
    use crate::snes::integration_tests::fixture_rom::FixtureRom;
    use crate::snes::integration_tests::rom_runner::{
        InputAction, InputEvent, RunConfig, RunExitReason, run_rom,
    };

    /// JOY1L/JOY2L ($4218/$421A): A X L R + four ID zeros, bits 7..0.
    /// JOY1H/JOY2H ($4219/$421B): B Y Select Start Up Down Left Right.
    const JOY1L: u16 = 0x4218;
    const JOY1H: u16 = 0x4219;

    /// Emits a poll block that waits until the auto-joypad registers show
    /// exactly `(joy_h, joy_l)` for the given high/low register pair.
    fn wait_for_joy(fx: &mut FixtureRom, joy_h_addr: u16, joy_h: u8, joy_l_addr: u16, joy_l: u8) {
        let poll = fx.pos();
        fx.lda_abs(joy_h_addr);
        fx.cmp_imm(joy_h);
        fx.bne_to(poll);
        fx.lda_abs(joy_l_addr);
        fx.cmp_imm(joy_l);
        fx.bne_to(poll);
    }

    /// A port-2 button edge (the [`InputEvent::button`] ctor covers port 1).
    const fn button_port2(frame: u32, button: SnesButton, pressed: bool) -> InputEvent {
        InputEvent {
            frame,
            action: InputAction::Button {
                port: 1,
                button,
                pressed,
            },
        }
    }

    /// Given Start is held on the port 1 standard pad, when the ROM strobes
    /// and serially reads 24 bits from `$4016`, then it observes the
    /// documented order: `B Y Select Start Up Down Left Right` (only the
    /// Start bit set), `A X L R` plus four ID zeros (all clear), and eight
    /// all-ones padding bits past bit 16.
    #[test]
    fn serial_order_reports_held_start_id_zeros_and_padding_ones() {
        let mut fx = FixtureRom::new(b"NESER PAD SERIAL");
        let poll_start = fx.pos();
        fx.strobe_pulse();
        fx.serial_read_bits(0x4016, 24, 0x0010);
        // Bits 1-8 (B Y Select Start Up Down Left Right), MSB-first: Start
        // held -> 0b0001_0000.
        fx.lda_abs(0x0010);
        fx.cmp_imm(0x10);
        fx.bne_to(poll_start);
        // Bits 9-16 (A X L R + ID 0000): all clear.
        fx.lda_abs(0x0011);
        fx.cmp_imm(0x00);
        fx.bne_to(poll_start);
        // Bits 17-24: connected-pad padding reads as ones.
        fx.lda_abs(0x0012);
        fx.cmp_imm(0xFF);
        fx.bne_to(poll_start);
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let script = [InputEvent::button(2, SnesButton::Start, true)];
        let result = run_rom(
            &rom,
            "pad-serial-order.sfc",
            RunConfig::new(400_000_000, 120).with_input_script(&script),
        );

        assert!(
            result.passed,
            "ROM should observe Start in the documented serial position \
             (exit={:?} frames={} marker={:?})",
            result.exit_reason, result.frames, result.marker
        );
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
    }

    /// Given Start is held on the port 2 standard pad, when the ROM strobes
    /// (shared `$4016` strobe) and serially reads 24 bits from `$4017`, then
    /// port 2 reports the same documented serial order as port 1.
    #[test]
    fn port2_serial_order_reports_held_start() {
        let mut fx = FixtureRom::new(b"NESER PAD2 SERIAL");
        let poll_start = fx.pos();
        fx.strobe_pulse();
        fx.serial_read_bits(0x4017, 24, 0x0010);
        fx.lda_abs(0x0010);
        fx.cmp_imm(0x10); // Start
        fx.bne_to(poll_start);
        fx.lda_abs(0x0011);
        fx.cmp_imm(0x00); // A X L R + ID zeros
        fx.bne_to(poll_start);
        fx.lda_abs(0x0012);
        fx.cmp_imm(0xFF); // connected-pad padding
        fx.bne_to(poll_start);
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let script = [button_port2(2, SnesButton::Start, true)];
        let result = run_rom(
            &rom,
            "pad2-serial-order.sfc",
            RunConfig::new(400_000_000, 120).with_input_script(&script),
        );

        assert!(
            result.passed,
            "port 2 should shift out the documented serial order via $4017 \
             (exit={:?} frames={})",
            result.exit_reason, result.frames
        );
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
    }

    /// The issue's example sequence, observed through auto-joypad reads:
    /// no buttons, then A, B, Start, direct Up->Down->Left->Right
    /// transitions (release+press applied atomically on the same frame),
    /// then all released. Each step requires the exact `(JOY1H, JOY1L)`
    /// pair, so the four ID bits must read zero throughout.
    #[test]
    fn example_sequence_transitions_are_observed_via_auto_joypad() {
        // (JOY1H, JOY1L) per step, in the order the ROM must observe them.
        const STEPS: [(u8, u8); 8] = [
            (0x00, 0x80), // A
            (0x80, 0x00), // B
            (0x10, 0x00), // Start
            (0x08, 0x00), // Up
            (0x04, 0x00), // Down
            (0x02, 0x00), // Left
            (0x01, 0x00), // Right
            (0x00, 0x00), // all released
        ];

        let mut fx = FixtureRom::new(b"NESER PAD SEQUENCE");
        fx.write_long(0x00_4200, 0x01); // enable auto-joypad reads
        for (joy_h, joy_l) in STEPS {
            wait_for_joy(&mut fx, JOY1H, joy_h, JOY1L, joy_l);
        }
        fx.pass_marker_and_idle();
        let rom = fx.build();

        // Direct transitions: the previous release and the next press share
        // a frame stamp and are applied atomically before any tick runs.
        let script = [
            InputEvent::button(10, SnesButton::A, true),
            InputEvent::button(20, SnesButton::A, false),
            InputEvent::button(20, SnesButton::B, true),
            InputEvent::button(30, SnesButton::B, false),
            InputEvent::button(30, SnesButton::Start, true),
            InputEvent::button(40, SnesButton::Start, false),
            InputEvent::button(40, SnesButton::Up, true),
            InputEvent::button(50, SnesButton::Up, false),
            InputEvent::button(50, SnesButton::Down, true),
            InputEvent::button(60, SnesButton::Down, false),
            InputEvent::button(60, SnesButton::Left, true),
            InputEvent::button(70, SnesButton::Left, false),
            InputEvent::button(70, SnesButton::Right, true),
            InputEvent::button(80, SnesButton::Right, false),
        ];
        let result = run_rom(
            &rom,
            "pad-example-sequence.sfc",
            RunConfig::new(2_000_000_000, 240).with_input_script(&script),
        );

        assert!(
            result.passed,
            "ROM should observe every step of the example sequence in order \
             (exit={:?} frames={})",
            result.exit_reason, result.frames
        );
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
    }

    /// While the strobe is held high, every `$4016` read returns the live B
    /// state without advancing the shift register; driving the strobe low
    /// latches, and the serial packet then starts over from B.
    #[test]
    fn strobe_high_reads_live_b_and_falling_edge_latches() {
        let mut fx = FixtureRom::new(b"NESER PAD STROBE");
        // Hold the strobe high and read 8 bits: once B is held they must
        // all be ones (an advancing shift register would deliver the
        // not-pressed Y/Select/... bits instead).
        fx.write_long(0x00_4016, 0x01);
        let poll_live = fx.pos();
        fx.serial_read_bits(0x4016, 8, 0x0010);
        fx.lda_abs(0x0010);
        fx.cmp_imm(0xFF);
        fx.bne_to(poll_live);
        // Falling edge: latch and read the full packet from the start.
        fx.write_long(0x00_4016, 0x00);
        fx.serial_read_bits(0x4016, 24, 0x0011);
        let fail_probe = fx.pos();
        fx.lda_abs(0x0011);
        fx.cmp_imm(0x80); // B held, everything else clear
        fx.bne_to(fail_probe); // hang -> frame-limit failure diagnostics
        fx.lda_abs(0x0012);
        fx.cmp_imm(0x00); // A X L R + ID zeros
        fx.bne_to(fail_probe);
        fx.lda_abs(0x0013);
        fx.cmp_imm(0xFF); // padding ones
        fx.bne_to(fail_probe);
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let script = [InputEvent::button(2, SnesButton::B, true)];
        let result = run_rom(
            &rom,
            "pad-strobe-semantics.sfc",
            RunConfig::new(400_000_000, 120).with_input_script(&script),
        );

        assert!(
            result.passed,
            "strobe-high reads should return the live B state and the \
             falling edge should latch a fresh packet (exit={:?} frames={})",
            result.exit_reason, result.frames
        );
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
    }

    /// The auto-joypad JOY1 layout matches the manual serial order: with
    /// Start held, the ROM first waits for `JOY1H == $10`, then manually
    /// strobes and re-reads the packet through `$4016`, expecting the same
    /// Start bit in the same position.
    #[test]
    fn auto_joypad_layout_matches_manual_serial_order() {
        let mut fx = FixtureRom::new(b"NESER PAD AUTO EQ");
        fx.write_long(0x00_4200, 0x01);
        wait_for_joy(&mut fx, JOY1H, 0x10, JOY1L, 0x00);
        let poll_manual = fx.pos();
        fx.strobe_pulse();
        fx.serial_read_bits(0x4016, 16, 0x0010);
        fx.lda_abs(0x0010);
        fx.cmp_imm(0x10); // Start in the same serial position
        fx.bne_to(poll_manual);
        fx.lda_abs(0x0011);
        fx.cmp_imm(0x00);
        fx.bne_to(poll_manual);
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let script = [InputEvent::button(2, SnesButton::Start, true)];
        let result = run_rom(
            &rom,
            "pad-auto-matches-serial.sfc",
            RunConfig::new(400_000_000, 120).with_input_script(&script),
        );

        assert!(
            result.passed,
            "manual serial reads should agree with the auto-joypad layout \
             (exit={:?} frames={})",
            result.exit_reason, result.frames
        );
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
    }
}
