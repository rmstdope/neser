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
        InputEvent, RunConfig, RunExitReason, run_rom,
    };

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
}
