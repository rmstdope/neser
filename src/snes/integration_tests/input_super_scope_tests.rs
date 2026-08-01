//! Issue #2890: ROM-level verification of the SNES Super Scope.
//!
//! Written spec-first against fullsnes / the SNESdev wiki and cross-checked
//! against Mesen2's `SuperScope`/`SnesPpu` implementation. After a strobe
//! latch, the scope shifts a 16-bit packet out on the port's data1 line, one
//! bit per `$4016`/`$4017` read, first bit first:
//!
//! - bit 0: fire (trigger)      bit 4: 0
//! - bit 1: cursor              bit 5: 0
//! - bit 2: turbo (toggle)      bit 6: offscreen / Null
//! - bit 3: pause               bit 7: 0
//! - bits 8-15: signature ones (further reads keep returning 1)
//!
//! [`FixtureRom::serial_read_bits`] packs those bits MSB-first into WRAM, so the
//! first byte read holds `fire cursor turbo pause 0 0 offscreen 0` (fire in bit
//! 7) and the second byte is the `0xFF` signature. The fire and pause buttons
//! are single-shot (one packet per press); cursor is a level; turbo toggles a
//! latched state and, while on, holds fire high (auto-fire). The fire button is
//! reported regardless of aim -- only the offscreen bit reflects the aim.
//!
//! Aiming: while fire or cursor is held on-screen, the light sensor latches the
//! beam position into OPHCT (`$213C`) / OPVCT (`$213D`) with the STAT78
//! (`$213F`) bit-6 flag; neser mirrors Mesen2's centering offset, latching
//! `OPHCT = aimX + 10`, `OPVCT = max(0, aimY - 3)`. The beam-gated *timing* of
//! that latch is covered by the PPU unit tests
//! (`super_scope_aim_latch_latches_requested_coords_when_beam_passes`); here we
//! verify the end-to-end latched values a ROM reads back.

#[cfg(test)]
mod tests {
    use crate::snes::input::SnesControllerType;
    use crate::snes::integration_tests::fixture_rom::FixtureRom;
    use crate::snes::integration_tests::rom_runner::{
        InputAction, InputEvent, RunConfig, RunExitReason, run_rom,
    };

    /// Serial packet scratch: `$0010` = bits 1-8 (fire..offscreen), `$0011` =
    /// the `0xFF` signature tail.
    const PKT: u16 = 0x0010;
    /// Physical controller port 2 (the Super Scope's hardware port), addressed
    /// as `port = 1` by the `Snes` input-injection API.
    const SCOPE: u8 = 1;

    /// Default port layout: standard pad on port 1, Super Scope on port 2
    /// (serial reads come out on `$4017`, sharing the `$4016` strobe).
    fn scope_ports() -> RunConfig<'static> {
        RunConfig::new(400_000_000, 120)
            .with_controller_ports(SnesControllerType::Standard, SnesControllerType::SuperScope)
    }

    const fn scope_pos(frame: u32, x: i16, y: i16) -> InputEvent {
        InputEvent {
            frame,
            action: InputAction::SuperScopePosition { port: SCOPE, x, y },
        }
    }
    const fn scope_trigger(frame: u32, pressed: bool) -> InputEvent {
        InputEvent {
            frame,
            action: InputAction::SuperScopeTrigger {
                port: SCOPE,
                pressed,
            },
        }
    }
    const fn scope_cursor(frame: u32, pressed: bool) -> InputEvent {
        InputEvent {
            frame,
            action: InputAction::SuperScopeCursor {
                port: SCOPE,
                pressed,
            },
        }
    }
    const fn scope_turbo(frame: u32, pressed: bool) -> InputEvent {
        InputEvent {
            frame,
            action: InputAction::SuperScopeTurbo {
                port: SCOPE,
                pressed,
            },
        }
    }
    const fn scope_pause(frame: u32, pressed: bool) -> InputEvent {
        InputEvent {
            frame,
            action: InputAction::SuperScopePause {
                port: SCOPE,
                pressed,
            },
        }
    }

    /// Emit a poll block that strobes, serially reads one 16-bit packet, and
    /// loops until the status byte equals `status` *and* the signature tail is
    /// `0xFF`. Chaining these asserts an ordered sequence of observed packets;
    /// a state that never appears surfaces as a frame-limit failure. Because it
    /// re-strobes on every mismatch, a value that is stuck wrong (e.g. a
    /// single-shot button that never clears) also fails via the frame limit.
    fn expect_status(fx: &mut FixtureRom, joy: u16, status: u8) {
        let poll = fx.pos();
        fx.strobe_pulse();
        fx.serial_read_bits(joy, 16, PKT);
        fx.lda_abs(PKT);
        fx.cmp_imm(status);
        fx.bne_to(poll);
        fx.lda_abs(PKT + 1);
        fx.cmp_imm(0xFF);
        fx.bne_to(poll);
    }

    fn assert_passed(result: &crate::snes::integration_tests::rom_runner::RunResult, what: &str) {
        assert!(
            result.passed,
            "{what} (exit={:?} frames={})",
            result.exit_reason, result.frames
        );
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
    }

    /// An idle scope aimed on-screen reports no buttons (status `0x00`) with the
    /// `0xFF` signature tail -- the documented bit layout with everything clear.
    #[test]
    fn idle_packet_reports_signature_and_no_buttons() {
        let mut fx = FixtureRom::new(b"NESER SCOPE IDLE");
        expect_status(&mut fx, 0x4017, 0x00);
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let result = run_rom(&rom, "super-scope-idle.sfc", scope_ports());
        assert_passed(&result, "idle scope should report only the signature tail");
    }

    /// The fire button is single-shot: one packet reports it set (`0x80`), and
    /// while it stays held the next packet already reads clear (`0x00`).
    #[test]
    fn trigger_button_is_single_shot() {
        let mut fx = FixtureRom::new(b"NESER SCOPE FIRE");
        expect_status(&mut fx, 0x4017, 0x80); // fire registers once
        expect_status(&mut fx, 0x4017, 0x00); // ...then clears while held
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let script = [scope_trigger(10, true)];
        let result = run_rom(
            &rom,
            "super-scope-fire.sfc",
            scope_ports().with_input_script(&script),
        );
        assert_passed(&result, "fire should be reported for exactly one packet");
    }

    /// The cursor button is a level: it stays set across consecutive packets
    /// while held (a single-shot cursor would clear and hang this test).
    #[test]
    fn cursor_button_is_reported_as_a_level() {
        let mut fx = FixtureRom::new(b"NESER SCOPE CURSOR");
        expect_status(&mut fx, 0x4017, 0x40);
        expect_status(&mut fx, 0x4017, 0x40); // still set on the next packet
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let script = [scope_cursor(10, true)];
        let result = run_rom(
            &rom,
            "super-scope-cursor.sfc",
            scope_ports().with_input_script(&script),
        );
        assert_passed(&result, "cursor should stay set while held");
    }

    /// The pause button is single-shot, like fire: set for one packet (`0x10`)
    /// then clear while held.
    #[test]
    fn pause_button_is_single_shot() {
        let mut fx = FixtureRom::new(b"NESER SCOPE PAUSE");
        expect_status(&mut fx, 0x4017, 0x10);
        expect_status(&mut fx, 0x4017, 0x00);
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let script = [scope_pause(10, true)];
        let result = run_rom(
            &rom,
            "super-scope-pause.sfc",
            scope_ports().with_input_script(&script),
        );
        assert_passed(&result, "pause should be reported for exactly one packet");
    }

    /// Pressing turbo toggles the latched turbo bit on (`0x20`); with turbo on
    /// and fire held, fire auto-fires -- it stays set (`0xA0`) across packets
    /// instead of single-shotting.
    #[test]
    fn turbo_toggles_on_and_auto_fires_the_trigger() {
        let mut fx = FixtureRom::new(b"NESER SCOPE TURBO");
        expect_status(&mut fx, 0x4017, 0x20); // turbo toggled on
        expect_status(&mut fx, 0x4017, 0xA0); // fire + turbo
        expect_status(&mut fx, 0x4017, 0xA0); // ...still set: auto-fire, not single-shot
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let script = [scope_turbo(10, true), scope_trigger(20, true)];
        let result = run_rom(
            &rom,
            "super-scope-turbo.sfc",
            scope_ports().with_input_script(&script),
        );
        assert_passed(&result, "turbo should hold fire high (auto-fire)");
    }

    /// The offscreen/Null bit (bit 6) tracks the aim: clear on-screen (`0x00`),
    /// set once the aim moves off the visible raster (`0x02`).
    #[test]
    fn null_bit_tracks_on_and_off_screen_aim() {
        let mut fx = FixtureRom::new(b"NESER SCOPE NULL");
        expect_status(&mut fx, 0x4017, 0x00); // default aim is on-screen
        expect_status(&mut fx, 0x4017, 0x02); // ...then moved off-screen
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let script = [scope_pos(10, -1, 120)];
        let result = run_rom(
            &rom,
            "super-scope-null.sfc",
            scope_ports().with_input_script(&script),
        );
        assert_passed(&result, "offscreen bit should follow the aim");
    }

    /// The fire button is reported even when aimed off-screen (the "shoot
    /// off-screen to reload" mechanic): the packet carries fire + offscreen
    /// (`0x82`), then the offscreen bit persists while fire single-shots to
    /// `0x02`.
    #[test]
    fn trigger_is_reported_off_screen() {
        let mut fx = FixtureRom::new(b"NESER SCOPE OFFFIRE");
        expect_status(&mut fx, 0x4017, 0x82); // fire + offscreen
        expect_status(&mut fx, 0x4017, 0x02); // fire cleared, still offscreen
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let script = [scope_pos(5, -30, 120), scope_trigger(10, true)];
        let result = run_rom(
            &rom,
            "super-scope-offscreen-fire.sfc",
            scope_ports().with_input_script(&script),
        );
        assert_passed(&result, "fire must be reported regardless of aim");
    }

    /// The issue's example sequence, observed packet-by-packet in order: idle
    /// center, fire (then release), cursor, turbo-on (cursor released), pause
    /// (with turbo latched on), and an off-screen aim edge (turbo still on).
    #[test]
    fn example_input_sequence_is_observed_in_order() {
        // Observed status bytes, in order.
        const STEPS: [u8; 7] = [
            0x00, // idle, aimed center
            0x80, // fire
            0x00, // fire released / single-shot cleared
            0x40, // cursor
            0x20, // cursor released, turbo toggled on
            0x30, // pause pressed (turbo still on)
            0x22, // aim edge off-screen (turbo still on)
        ];

        let mut fx = FixtureRom::new(b"NESER SCOPE SEQ");
        for status in STEPS {
            expect_status(&mut fx, 0x4017, status);
        }
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let script = [
            scope_trigger(10, true),
            scope_trigger(20, false),
            scope_cursor(25, true),
            scope_cursor(35, false),
            scope_turbo(35, true),
            scope_pause(45, true),
            scope_pos(55, -20, 120),
        ];
        let result = run_rom(
            &rom,
            "super-scope-example-sequence.sfc",
            RunConfig::new(2_000_000_000, 240)
                .with_controller_ports(SnesControllerType::Standard, SnesControllerType::SuperScope)
                .with_input_script(&script),
        );
        assert_passed(&result, "the example sequence should be observed in order");
    }

    /// Port routing: a Super Scope on port 1 shifts its packet out on `$4016`.
    #[test]
    fn port1_super_scope_reads_via_4016() {
        let mut fx = FixtureRom::new(b"NESER SCOPE PORT1");
        expect_status(&mut fx, 0x4016, 0x00);
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let result = run_rom(
            &rom,
            "super-scope-port1.sfc",
            RunConfig::new(400_000_000, 120).with_controller_ports(
                SnesControllerType::SuperScope,
                SnesControllerType::Standard,
            ),
        );
        assert_passed(&result, "a port-1 scope should read out on $4016");
    }

    /// Emit the aim-latch verification tail: wait for the STAT78 latch flag,
    /// then read OPHCT (low, high) and OPVCT (low, high) and require them to
    /// equal `(ophct, opvct)`. On any mismatch it loops back to the flag poll;
    /// `$213F` reads reset the OPHCT/OPVCT read flipflops, so each retry starts
    /// from the low byte. A persistently wrong value fails via the frame limit.
    fn assert_latched_coords(fx: &mut FixtureRom, ophct: u16, opvct: u16) {
        let poll = fx.pos();
        fx.lda_abs(0x213F); // STAT78: clears the latch flag, resets read toggles
        fx.and_imm(0x40); // bit 6 = counter-latched flag
        fx.beq_to(poll); // not latched yet
        fx.lda_abs(0x213C); // OPHCT low byte
        fx.cmp_imm((ophct & 0xFF) as u8);
        fx.bne_to(poll);
        fx.lda_abs(0x213C); // OPHCT high bit (bits 1-7 are open bus)
        fx.and_imm(0x01);
        fx.cmp_imm(((ophct >> 8) & 0x01) as u8);
        fx.bne_to(poll);
        fx.lda_abs(0x213D); // OPVCT low byte
        fx.cmp_imm((opvct & 0xFF) as u8);
        fx.bne_to(poll);
        fx.lda_abs(0x213D); // OPVCT high bit
        fx.and_imm(0x01);
        fx.cmp_imm(((opvct >> 8) & 0x01) as u8);
        fx.bne_to(poll);
    }

    /// Holding fire on-screen latches the aim into OPHCT/OPVCT with the Mesen2
    /// centering offset: aim (100, 80) -> OPHCT 110, OPVCT 77.
    #[test]
    fn aim_latch_reports_offset_ophct_opvct_on_fire() {
        let mut fx = FixtureRom::new(b"NESER SCOPE LATCH");
        assert_latched_coords(&mut fx, 110, 77);
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let script = [scope_pos(5, 100, 80), scope_trigger(5, true)];
        let result = run_rom(
            &rom,
            "super-scope-latch-fire.sfc",
            scope_ports().with_input_script(&script),
        );
        assert_passed(&result, "fire should latch OPHCT/OPVCT at the aimed point");
    }

    /// The cursor button also latches the aim (Mesen2 latches on fire OR
    /// cursor): aim (60, 40) -> OPHCT 70, OPVCT 37.
    #[test]
    fn cursor_press_also_latches_the_aim() {
        let mut fx = FixtureRom::new(b"NESER SCOPE LATCHC");
        assert_latched_coords(&mut fx, 70, 37);
        fx.pass_marker_and_idle();
        let rom = fx.build();

        let script = [scope_pos(5, 60, 40), scope_cursor(5, true)];
        let result = run_rom(
            &rom,
            "super-scope-latch-cursor.sfc",
            scope_ports().with_input_script(&script),
        );
        assert_passed(
            &result,
            "cursor should latch OPHCT/OPVCT at the aimed point",
        );
    }
}
