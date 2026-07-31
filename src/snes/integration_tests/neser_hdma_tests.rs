//! Custom, in-code HDMA fixture ROMs for issue #2884.
//!
//! Like `neser_dma_tests`, these are NESER-authored synthetic ROMs (built with
//! [`FixtureRom`]) that report PASS/FAIL through the `rom_runner` WRAM marker
//! protocol, authored against the fullsnes/anomie spec without reading the DMA
//! implementation. They complement the white-box HDMA unit tests in
//! `src/snes/bus/dma.rs`/`system_bus.rs` with black-box, end-to-end checks
//! driven by the real per-scanline PPU timing.
//!
//! Readback technique: an HDMA channel targets the WMDATA port (`$2180`,
//! B-bus `$80`) with WMADD preset, so every per-scanline HDMA byte is deposited
//! into consecutive low WRAM. The channel is enabled during VBlank so HDMA init
//! runs cleanly at the top of the following frame; after that frame the CPU
//! reads the deposited buffer back and asserts the sequence, which pins exactly
//! which lines transferred and what values (line-counter, repeat, indirect and
//! terminator behaviour).

use super::fixture_rom::FixtureRom;
use super::rom_runner::{RunConfig, RunExitReason, run_rom};

/// B-bus address of the WMDATA port (`$2180`).
const BBAD_WMDATA: u8 = 0x80;
/// Sentinel pre-filled into the WRAM capture buffer so untransferred cells are
/// distinguishable from a real transfer of the same value.
const SENTINEL: u8 = 0xEE;
/// Low-WRAM address where HDMA deposits land (via WMDATA).
const CAPTURE: u16 = 0x0300;

/// Presets WMADD (`$2181/2/3`) so WMDATA writes land at `addr` in bank `$7E`.
fn set_wmadd(fx: &mut FixtureRom, addr: u16) {
    fx.store_imm_abs(0x2181, (addr & 0xFF) as u8);
    fx.store_imm_abs(0x2182, (addr >> 8) as u8);
    fx.store_imm_abs(0x2183, 0x00);
}

/// Emits CPU code that reads WRAM `addr` and fails the fixture unless it holds
/// `expected`.
fn assert_wram(fx: &mut FixtureRom, addr: u16, expected: u8) {
    fx.lda_abs(addr);
    fx.branch_fail_if_ne(expected);
}

/// Spins until the PPU reports VBlank (`HVBJOY $4212` bit 7 set).
fn wait_until_vblank(fx: &mut FixtureRom) {
    let loop_top = fx.pos();
    fx.lda_abs(0x4212);
    fx.and_imm(0x80);
    fx.beq_to(loop_top);
}

/// Spins until active display resumes (`HVBJOY $4212` bit 7 clear).
fn wait_until_active_display(fx: &mut FixtureRom) {
    let loop_top = fx.pos();
    fx.lda_abs(0x4212);
    fx.and_imm(0x80);
    fx.bne_to(loop_top);
}

/// Arms `channel` with `table` and runs exactly one clean frame of HDMA into
/// the WRAM capture buffer, leaving the deposited bytes at [`CAPTURE`]`..`.
/// `dmap` selects direction/mode/indirect; `indirect_bank` is DASB.
fn run_one_hdma_frame(fx: &mut FixtureRom, dmap: u8, table: u16, indirect_bank: u8) {
    fx.force_blank_on();
    fx.setup_hdma(0, dmap, BBAD_WMDATA, table, indirect_bank);
    // Enable during VBlank so init fires cleanly at the next frame's line 0.
    wait_until_vblank(fx);
    set_wmadd(fx, CAPTURE);
    for i in 0..8 {
        fx.store_imm_abs(CAPTURE + i, SENTINEL);
    }
    fx.enable_hdma(0x01);
    wait_until_active_display(fx); // line 0: HDMA init
    wait_until_vblank(fx); // visible lines done: all transfers complete
    fx.disable_hdma();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_fixture(rom: Vec<u8>, name: &str) {
        let result = run_rom(&rom, name, RunConfig::new(400_000_000, 60));
        assert_eq!(
            result.exit_reason,
            RunExitReason::PassMarker,
            "{name}: expected PASS marker, got exit={:?} pc=0x{:04X} marker={:?}",
            result.exit_reason,
            result.pc,
            result.marker,
        );
        assert!(result.passed, "{name}: fixture did not pass");
    }

    /// Direct mode 0, three single-line non-repeat entries: one table data byte
    /// is transferred per active scanline, in order, then the `$00` terminator
    /// stops the channel.
    #[test]
    fn hdma_direct_mode_deposits_one_byte_per_line() {
        let mut fx = FixtureRom::new(b"NESER HDMA DIR");
        let table = fx.place_data(&[0x01, 0xAA, 0x01, 0xBB, 0x01, 0xCC, 0x00]);
        run_one_hdma_frame(&mut fx, 0x00, table, 0x00);
        assert_wram(&mut fx, CAPTURE, 0xAA);
        assert_wram(&mut fx, CAPTURE + 1, 0xBB);
        assert_wram(&mut fx, CAPTURE + 2, 0xCC);
        assert_wram(&mut fx, CAPTURE + 3, SENTINEL); // terminator: nothing more
        fx.pass_marker_and_idle();
        run_fixture(fx.build(), "hdma-direct.sfc");
    }

    /// A non-repeat entry with a line count > 1 transfers its data once (first
    /// line) and then idles for the remaining lines of the entry.
    #[test]
    fn hdma_nonrepeat_entry_transfers_once_then_idles() {
        let mut fx = FixtureRom::new(b"NESER HDMA NREP");
        // $03 = non-repeat, 3 lines; one data byte $99; then terminator.
        let table = fx.place_data(&[0x03, 0x99, 0x00]);
        run_one_hdma_frame(&mut fx, 0x00, table, 0x00);
        assert_wram(&mut fx, CAPTURE, 0x99);
        assert_wram(&mut fx, CAPTURE + 1, SENTINEL); // idled, no transfer
        assert_wram(&mut fx, CAPTURE + 2, SENTINEL);
        fx.pass_marker_and_idle();
        run_fixture(fx.build(), "hdma-nonrepeat.sfc");
    }

    /// A repeat entry (`$80` bit set) transfers a fresh table byte on every one
    /// of its lines.
    #[test]
    fn hdma_repeat_entry_transfers_every_line() {
        let mut fx = FixtureRom::new(b"NESER HDMA REP");
        // $83 = repeat, 3 lines; three data bytes; then terminator.
        let table = fx.place_data(&[0x83, 0x11, 0x22, 0x33, 0x00]);
        run_one_hdma_frame(&mut fx, 0x00, table, 0x00);
        assert_wram(&mut fx, CAPTURE, 0x11);
        assert_wram(&mut fx, CAPTURE + 1, 0x22);
        assert_wram(&mut fx, CAPTURE + 2, 0x33);
        assert_wram(&mut fx, CAPTURE + 3, SENTINEL);
        fx.pass_marker_and_idle();
        run_fixture(fx.build(), "hdma-repeat.sfc");
    }

    /// Indirect mode dereferences the table's 2-byte pointer (in bank DASB) and
    /// transfers from there, advancing the pointer per byte.
    #[test]
    fn hdma_indirect_mode_dereferences_pointer() {
        let mut fx = FixtureRom::new(b"NESER HDMA IND");
        let data = fx.place_data(&[0xDE, 0xAD]);
        // $82 = repeat, 2 lines; 2-byte pointer to `data`; then terminator.
        let table = fx.place_data(&[0x82, (data & 0xFF) as u8, (data >> 8) as u8, 0x00]);
        // DMAP $40: A->B, indirect, mode 0. DASB (indirect bank) = 0.
        run_one_hdma_frame(&mut fx, 0x40, table, 0x00);
        assert_wram(&mut fx, CAPTURE, 0xDE);
        assert_wram(&mut fx, CAPTURE + 1, 0xAD);
        assert_wram(&mut fx, CAPTURE + 2, SENTINEL);
        fx.pass_marker_and_idle();
        run_fixture(fx.build(), "hdma-indirect.sfc");
    }

    /// A `$00` terminator halts the channel for the rest of the frame: no bytes
    /// are transferred on any later scanline.
    #[test]
    fn hdma_terminator_halts_channel_for_the_frame() {
        let mut fx = FixtureRom::new(b"NESER HDMA TERM");
        let table = fx.place_data(&[0x01, 0x77, 0x00]);
        run_one_hdma_frame(&mut fx, 0x00, table, 0x00);
        assert_wram(&mut fx, CAPTURE, 0x77);
        for i in 1..8 {
            assert_wram(&mut fx, CAPTURE + i, SENTINEL);
        }
        fx.pass_marker_and_idle();
        run_fixture(fx.build(), "hdma-terminator.sfc");
    }
}
