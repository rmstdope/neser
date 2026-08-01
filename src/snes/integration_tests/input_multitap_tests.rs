//! Issue #2891: ROM-level verification of the SNES Super Multitap.
//!
//! Written spec-first and cross-checked against Mesen2's `Multitap::ReadRam`.
//! The multitap plugs into port 2 and presents four controllers as two pairs,
//! chosen by WRIO (`$4201`) bit 7: bit 7 high selects slots 1 & 2, bit 7 low
//! selects slots 3 & 4. A `$4017` read returns two controllers at once -- the
//! pair's first controller on data1 (bit 0), the second on data2 (bit 1) --
//! each a normal 16-bit standard-controller packet.
//!
//! Detection: while the strobe is held high the multitap drives data1 = 0 and
//! data2 = 1, so a `$4017` read returns `0x02` regardless of buttons; that is
//! how games distinguish a multitap from a lone controller (which returns its
//! live B on data1). Auto-joypad latches the currently-selected pair into
//! JOY2/JOY4, so all four slots are read by toggling bit 7 across frames.
//!
//! Slot numbering follows the issue: "slot N" is `InputAction` port N, i.e.
//! neser `players[N-1]`. Fixtures assemble LoROM programs (see `fixture_rom`)
//! that toggle `$4201`, serially read `$4017`, and report via the WRAM marker.

#[cfg(test)]
mod tests {
    use crate::snes::input::{SnesButton, SnesControllerType};
    use crate::snes::integration_tests::fixture_rom::FixtureRom;
    use crate::snes::integration_tests::rom_runner::{
        InputAction, InputEvent, RunConfig, RunExitReason, RunResult, run_rom,
    };

    /// Scratch words for a paired serial read: `A_BUF` = data1 controller,
    /// `B_BUF` = data2 controller, each two bytes (high byte first).
    const A_BUF: u16 = 0x0010;
    const B_BUF: u16 = 0x0012;

    /// WRIO ($4201) values selecting each controller pair.
    const PAIR_HIGH: u8 = 0x80; // bit 7 set -> slots 1 & 2
    const PAIR_LOW: u8 = 0x00; // bit 7 clear -> slots 3 & 4

    fn tap_ports() -> RunConfig<'static> {
        RunConfig::new(400_000_000, 120)
            .with_controller_ports(SnesControllerType::Standard, SnesControllerType::Multitap)
    }

    /// A button edge on multitap slot `slot` (1..=4).
    const fn tap_btn(frame: u32, slot: u8, button: SnesButton, pressed: bool) -> InputEvent {
        InputEvent {
            frame,
            action: InputAction::Button {
                port: slot,
                button,
                pressed,
            },
        }
    }

    /// Selects the `wrio` pair, strobes, reads one 16-bit packet from each
    /// controller of the pair ($4017 data1/data2), and loops until the data1
    /// controller reads `a_word` and the data2 controller reads `b_word`.
    /// Re-strobing on mismatch means a scripted state is awaited and a stuck
    /// wrong value fails via the frame limit.
    fn expect_pair(fx: &mut FixtureRom, wrio: u8, a_word: u16, b_word: u16) {
        let poll = fx.pos();
        fx.store_imm_abs(0x4201, wrio);
        fx.strobe_pulse();
        fx.serial_read_pair(0x4017, 16, A_BUF, B_BUF);
        fx.lda_abs(A_BUF);
        fx.cmp_imm((a_word >> 8) as u8);
        fx.bne_to(poll);
        fx.lda_abs(A_BUF + 1);
        fx.cmp_imm((a_word & 0xFF) as u8);
        fx.bne_to(poll);
        fx.lda_abs(B_BUF);
        fx.cmp_imm((b_word >> 8) as u8);
        fx.bne_to(poll);
        fx.lda_abs(B_BUF + 1);
        fx.cmp_imm((b_word & 0xFF) as u8);
        fx.bne_to(poll);
    }

    fn assert_passed(result: &RunResult, what: &str) {
        assert!(
            result.passed,
            "{what} (exit={:?} frames={})",
            result.exit_reason, result.frames
        );
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
    }

    /// While the strobe is held high, a `$4017` read returns the multitap
    /// detection signature `0x02` (data2 set, data1 clear) regardless of
    /// buttons; dropping the strobe then yields an uncorrupted (idle) read.
    #[test]
    fn detection_signature_reads_02_while_strobe_high() {
        let mut fx = FixtureRom::new(b"NESER TAP DETECT");
        fx.store_imm_abs(0x4016, 0x01); // strobe high
        fx.lda_abs(0x4017);
        fx.and_imm(0x03);
        fx.branch_fail_if_ne(0x02);
        fx.store_imm_abs(0x4016, 0x00); // strobe low
        // A normal read of the idle pair is unaffected by the detection read.
        expect_pair(&mut fx, PAIR_HIGH, 0x0000, 0x0000);
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let result = run_rom(&rom, "multitap-detect.sfc", tap_ports());
        assert_passed(&result, "strobe-high read should report the 0x02 signature");
    }

    /// With no buttons held, all four slots read `0x0000` across both pairs.
    #[test]
    fn no_buttons_reads_zero_across_all_four_slots() {
        let mut fx = FixtureRom::new(b"NESER TAP IDLE");
        expect_pair(&mut fx, PAIR_HIGH, 0x0000, 0x0000);
        expect_pair(&mut fx, PAIR_LOW, 0x0000, 0x0000);
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let result = run_rom(&rom, "multitap-idle.sfc", tap_ports());
        assert_passed(&result, "idle multitap should read zero on all four slots");
    }

    /// A button held on one slot appears only in that slot's packet, on the
    /// correct data line and pair -- Start on slot 3 shows up as data1 of the
    /// low pair (`0x1000`) and nowhere else.
    #[test]
    fn a_single_slot_button_does_not_leak_to_other_slots() {
        let mut fx = FixtureRom::new(b"NESER TAP ISOLATE");
        expect_pair(&mut fx, PAIR_HIGH, 0x0000, 0x0000); // slots 1 & 2 clear
        expect_pair(&mut fx, PAIR_LOW, 0x1000, 0x0000); // slot 3 = Start, slot 4 clear
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let script = [tap_btn(5, 3, SnesButton::Start, true)];
        let result = run_rom(
            &rom,
            "multitap-isolate.sfc",
            tap_ports().with_input_script(&script),
        );
        assert_passed(&result, "a slot-3 button must not leak into other slots");
    }

    /// The issue's example sequence, observed per slot in order: no buttons,
    /// then slot 1 A / slot 2 B / slot 3 Start / slot 4 Right, then all
    /// released. Verifies per-slot mapping, data-line and pair ordering, and
    /// held/released transitions across all four slots.
    #[test]
    fn example_sequence_maps_each_slot_to_its_packet() {
        let mut fx = FixtureRom::new(b"NESER TAP SEQ");
        // No buttons.
        expect_pair(&mut fx, PAIR_HIGH, 0x0000, 0x0000);
        expect_pair(&mut fx, PAIR_LOW, 0x0000, 0x0000);
        // Slot 1 = A (0x0080), slot 2 = B (0x8000); slot 3 = Start (0x1000),
        // slot 4 = Right (0x0100).
        expect_pair(&mut fx, PAIR_HIGH, 0x0080, 0x8000);
        expect_pair(&mut fx, PAIR_LOW, 0x1000, 0x0100);
        // All released.
        expect_pair(&mut fx, PAIR_HIGH, 0x0000, 0x0000);
        expect_pair(&mut fx, PAIR_LOW, 0x0000, 0x0000);
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let script = [
            tap_btn(10, 1, SnesButton::A, true),
            tap_btn(10, 2, SnesButton::B, true),
            tap_btn(10, 3, SnesButton::Start, true),
            tap_btn(10, 4, SnesButton::Right, true),
            tap_btn(30, 1, SnesButton::A, false),
            tap_btn(30, 2, SnesButton::B, false),
            tap_btn(30, 3, SnesButton::Start, false),
            tap_btn(30, 4, SnesButton::Right, false),
        ];
        let result = run_rom(
            &rom,
            "multitap-example-sequence.sfc",
            RunConfig::new(2_000_000_000, 240)
                .with_controller_ports(SnesControllerType::Standard, SnesControllerType::Multitap)
                .with_input_script(&script),
        );
        assert_passed(&result, "each slot's packet should be observed in order");
    }

    /// Auto-joypad latches the currently-selected pair: with bit 7 high, JOY2
    /// carries slot 1 and JOY4 carries slot 2.
    #[test]
    fn auto_joypad_latches_the_selected_pair() {
        let mut fx = FixtureRom::new(b"NESER TAP AUTO");
        fx.store_imm_abs(0x4200, 0x01); // enable auto-joypad
        fx.store_imm_abs(0x4201, PAIR_HIGH); // select slots 1 & 2
        // Poll JOY2 ($421A/B) and JOY4 ($421E/F) until the scripted buttons
        // latch: slot 1 = B (0x8000) -> JOY2, slot 2 = Start (0x1000) -> JOY4.
        let poll = fx.pos();
        fx.lda_abs(0x421B);
        fx.cmp_imm(0x80);
        fx.bne_to(poll);
        fx.lda_abs(0x421A);
        fx.cmp_imm(0x00);
        fx.bne_to(poll);
        fx.lda_abs(0x421F);
        fx.cmp_imm(0x10);
        fx.bne_to(poll);
        fx.lda_abs(0x421E);
        fx.cmp_imm(0x00);
        fx.bne_to(poll);
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let script = [
            tap_btn(10, 1, SnesButton::B, true),
            tap_btn(10, 2, SnesButton::Start, true),
        ];
        let result = run_rom(
            &rom,
            "multitap-auto-joypad.sfc",
            RunConfig::new(2_000_000_000, 240)
                .with_controller_ports(SnesControllerType::Standard, SnesControllerType::Multitap)
                .with_input_script(&script),
        );
        assert_passed(
            &result,
            "auto-joypad should latch the selected pair into JOY2/JOY4",
        );
    }
}
