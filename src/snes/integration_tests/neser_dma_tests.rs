//! Custom, in-code general-purpose DMA (GPDMA) fixture ROMs for issue #2884.
//!
//! These are NESER-authored synthetic ROMs (built with [`FixtureRom`]), not
//! vendored external test binaries, so they carry no third-party provenance and
//! report PASS/FAIL through the `rom_runner` WRAM marker protocol rather than a
//! visual screen-CRC golden. Each test drives the real 65816 core to arm the
//! `$43xx`/`$420B` registers, lets the real bus/PPU run the transfer, then reads
//! the result back through the actual B-bus target path and asserts the bytes.
//!
//! The vectors are authored against the fullsnes/anomie register specification,
//! deliberately *without* reading the DMA implementation, so they are black-box
//! checks. They complement (do not duplicate) the white-box unit coverage in
//! `src/snes/bus/dma.rs` and `src/snes/bus/system_bus.rs`.
//!
//! Readback instrument: transfers land in low WRAM via the WMDATA auto-increment
//! port (`$2180`, B-bus address `$80`), preset with WMADD (`$2181/2/3`); the CPU
//! then reads the mirror of WRAM at `$00:0000-$1FFF` to verify each byte.

use super::fixture_rom::FixtureRom;
use super::rom_runner::{RunConfig, RunExitReason, run_rom};

/// B-bus address of the WMDATA port (`$2180`).
const BBAD_WMDATA: u8 = 0x80;

/// Presets WMADD (`$2181/2/3`) to a low-WRAM address in bank `$7E`, so
/// subsequent WMDATA writes land at `addr` and auto-increment. `addr` is a
/// 16-bit offset into the first 64 KiB of WRAM (bank bit forced to 0).
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

// --- Measurement instruments -------------------------------------------------
// These read a B-bus target back into WRAM by composing the public FixtureRom
// primitives, so a marker fixture can verify what a DMA deposited without a
// visual golden. VMAIN/CGADD/OAMADD are set to increment-after-high-byte so
// sequential words read back in order. All run during force blank.

/// CPU-writes the 16-bit `word` to VRAM word address `vaddr`
/// (`$2118`=low, `$2119`=high; VMAIN increments after the high byte).
fn cpu_write_vram(fx: &mut FixtureRom, vaddr: u16, word: u16) {
    fx.store_imm_abs(0x2115, 0x80);
    fx.store_imm_abs(0x2116, (vaddr & 0xFF) as u8);
    fx.store_imm_abs(0x2117, (vaddr >> 8) as u8);
    fx.store_imm_abs(0x2118, (word & 0xFF) as u8);
    fx.store_imm_abs(0x2119, (word >> 8) as u8);
}

/// Reads the VRAM word at `vaddr` into WRAM `dest` (low) / `dest+1` (high).
/// A dummy `$2139` read after setting VMADD primes the read latch.
fn read_vram_word(fx: &mut FixtureRom, vaddr: u16, dest: u16) {
    fx.store_imm_abs(0x2115, 0x80);
    fx.store_imm_abs(0x2116, (vaddr & 0xFF) as u8);
    fx.store_imm_abs(0x2117, (vaddr >> 8) as u8);
    fx.lda_abs(0x2139); // dummy read: prime the prefetch latch
    fx.lda_abs(0x2139);
    fx.sta_abs(dest);
    fx.lda_abs(0x213A);
    fx.sta_abs(dest + 1);
}

/// CPU-writes the 15-bit `color` to CGRAM index `index` (`$2122` low then
/// high; the CGRAM byte toggle advances the word index after the pair).
fn cpu_write_cgram(fx: &mut FixtureRom, index: u8, color: u16) {
    fx.store_imm_abs(0x2121, index);
    fx.store_imm_abs(0x2122, (color & 0xFF) as u8);
    fx.store_imm_abs(0x2122, (color >> 8) as u8);
}

/// Reads CGRAM color `index` into WRAM `dest` (low) / `dest+1` (high, masked to
/// the 7 valid bits — bit 15 reads back as PPU2 open bus).
fn read_cgram_color(fx: &mut FixtureRom, index: u8, dest: u16) {
    fx.store_imm_abs(0x2121, index);
    fx.lda_abs(0x213B); // low byte
    fx.sta_abs(dest);
    fx.lda_abs(0x213B); // high byte (bit 7 = open bus)
    fx.and_imm(0x7F);
    fx.sta_abs(dest + 1);
}

/// CPU-writes an OAM word (`low`, `high`) at OAM word address `word_addr`.
/// OAM commits a full word per even/odd `$2104` pair, so both bytes are
/// written; the word address advances afterwards.
fn cpu_write_oam_word(fx: &mut FixtureRom, word_addr: u8, low: u8, high: u8) {
    fx.store_imm_abs(0x2102, word_addr);
    fx.store_imm_abs(0x2103, 0x00);
    fx.store_imm_abs(0x2104, low);
    fx.store_imm_abs(0x2104, high);
}

/// Reads the OAM word at `word_addr` into WRAM `dest` (low) / `dest+1` (high).
fn read_oam_word(fx: &mut FixtureRom, word_addr: u8, dest: u16) {
    fx.store_imm_abs(0x2102, word_addr);
    fx.store_imm_abs(0x2103, 0x00);
    fx.lda_abs(0x2138);
    fx.sta_abs(dest);
    fx.lda_abs(0x2138);
    fx.sta_abs(dest + 1);
}

/// Spins until the PPU reports VBlank (`HVBJOY $4212` bit 7 set).
fn wait_until_vblank(fx: &mut FixtureRom) {
    let loop_top = fx.pos();
    fx.lda_abs(0x4212);
    fx.and_imm(0x80);
    fx.beq_to(loop_top); // keep looping while VBlank is clear
}

/// Spins until active display resumes (`HVBJOY $4212` bit 7 clear). Called
/// right after [`wait_until_vblank`] it lands near the top of the visible
/// frame.
fn wait_until_active_display(fx: &mut FixtureRom) {
    let loop_top = fx.pos();
    fx.lda_abs(0x4212);
    fx.and_imm(0x80);
    fx.bne_to(loop_top); // keep looping while VBlank is set
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs a built fixture ROM and asserts it reached the PASS marker.
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

    /// Mode 0 (single byte per unit) A->B GPDMA with an incrementing A-bus
    /// address copies a 4-byte ROM ramp into consecutive WRAM cells through the
    /// WMDATA port.
    #[test]
    fn gpdma_mode0_copies_ramp_to_wram_via_wmdata() {
        let mut fx = FixtureRom::new(b"NESER DMA MODE0");
        fx.force_blank_on();
        let ramp = [0x11u8, 0x22, 0x33, 0x44];
        let src = fx.place_data(&ramp);
        set_wmadd(&mut fx, 0x0300);
        // DMAP $00: A->B, increment A-bus, transfer mode 0.
        fx.setup_gpdma(0, 0x00, BBAD_WMDATA, u32::from(src), ramp.len() as u16);
        fx.trigger_gpdma(0x01);
        for (i, byte) in ramp.iter().enumerate() {
            assert_wram(&mut fx, 0x0300 + i as u16, *byte);
        }
        fx.pass_marker_and_idle();
        run_fixture(fx.build(), "gpdma-mode0.sfc");
    }

    /// Mode 1 (`[low, high]`) A->B GPDMA to `$2118/9` deposits one VRAM word
    /// per two source bytes.
    #[test]
    fn gpdma_mode1_uploads_words_to_vram() {
        let mut fx = FixtureRom::new(b"NESER DMA VRAM");
        fx.force_blank_on();
        fx.store_imm_abs(0x2115, 0x80); // VMAIN: increment after high, 1 word
        fx.store_imm_abs(0x2116, 0x00);
        fx.store_imm_abs(0x2117, 0x00);
        let src = fx.place_data(&[0x11, 0x22, 0x33, 0x44]);
        // DMAP $01: A->B, increment, mode 1.
        fx.setup_gpdma(0, 0x01, 0x18, u32::from(src), 4);
        fx.trigger_gpdma(0x01);
        read_vram_word(&mut fx, 0x0000, 0x0400);
        read_vram_word(&mut fx, 0x0001, 0x0402);
        assert_wram(&mut fx, 0x0400, 0x11);
        assert_wram(&mut fx, 0x0401, 0x22);
        assert_wram(&mut fx, 0x0402, 0x33);
        assert_wram(&mut fx, 0x0403, 0x44);
        fx.pass_marker_and_idle();
        run_fixture(fx.build(), "gpdma-vram.sfc");
    }

    /// Mode 0 A->B GPDMA to CGDATA (`$2122`) fills the palette; the CGRAM byte
    /// toggle builds one colour per two source bytes.
    #[test]
    fn gpdma_uploads_palette_to_cgram() {
        let mut fx = FixtureRom::new(b"NESER DMA CGRAM");
        fx.force_blank_on();
        fx.store_imm_abs(0x2121, 0x10); // CGADD = index $10
        let src = fx.place_data(&[0x34, 0x12, 0x78, 0x56]); // colours $1234, $5678
        fx.setup_gpdma(0, 0x00, 0x22, u32::from(src), 4);
        fx.trigger_gpdma(0x01);
        read_cgram_color(&mut fx, 0x10, 0x0400);
        read_cgram_color(&mut fx, 0x11, 0x0402);
        assert_wram(&mut fx, 0x0400, 0x34);
        assert_wram(&mut fx, 0x0401, 0x12);
        assert_wram(&mut fx, 0x0402, 0x78);
        assert_wram(&mut fx, 0x0403, 0x56);
        fx.pass_marker_and_idle();
        run_fixture(fx.build(), "gpdma-cgram.sfc");
    }

    /// Mode 0 A->B GPDMA to OAMDATA (`$2104`) writes OAM words in even/odd
    /// pairs.
    #[test]
    fn gpdma_uploads_words_to_oam() {
        let mut fx = FixtureRom::new(b"NESER DMA OAM");
        fx.force_blank_on();
        fx.store_imm_abs(0x2102, 0x20); // OAMADD word $20
        fx.store_imm_abs(0x2103, 0x00);
        let src = fx.place_data(&[0x11, 0x22, 0x33, 0x44]);
        fx.setup_gpdma(0, 0x00, 0x04, u32::from(src), 4);
        fx.trigger_gpdma(0x01);
        read_oam_word(&mut fx, 0x20, 0x0400);
        read_oam_word(&mut fx, 0x21, 0x0402);
        assert_wram(&mut fx, 0x0400, 0x11);
        assert_wram(&mut fx, 0x0401, 0x22);
        assert_wram(&mut fx, 0x0402, 0x33);
        assert_wram(&mut fx, 0x0403, 0x44);
        fx.pass_marker_and_idle();
        run_fixture(fx.build(), "gpdma-oam.sfc");
    }

    /// A->B GPDMA with a *decrementing* A-bus address reads the source in
    /// reverse (DMAP address-step bits = 10).
    #[test]
    fn gpdma_address_decrement_reads_source_in_reverse() {
        let mut fx = FixtureRom::new(b"NESER DMA DEC");
        fx.force_blank_on();
        let src = fx.place_data(&[0x11, 0x22, 0x33, 0x44]);
        set_wmadd(&mut fx, 0x0300);
        // DMAP $10: A->B, decrement A-bus, mode 0. Start at the ramp's last byte.
        fx.setup_gpdma(0, 0x10, BBAD_WMDATA, u32::from(src) + 3, 4);
        fx.trigger_gpdma(0x01);
        assert_wram(&mut fx, 0x0300, 0x44);
        assert_wram(&mut fx, 0x0301, 0x33);
        assert_wram(&mut fx, 0x0302, 0x22);
        assert_wram(&mut fx, 0x0303, 0x11);
        fx.pass_marker_and_idle();
        run_fixture(fx.build(), "gpdma-dec.sfc");
    }

    /// A->B GPDMA with a *fixed* A-bus address re-reads the same source byte
    /// for every transfer (DMAP address-step bits = 01).
    #[test]
    fn gpdma_fixed_address_repeats_source_byte() {
        let mut fx = FixtureRom::new(b"NESER DMA FIX");
        fx.force_blank_on();
        let src = fx.place_data(&[0x7E]);
        set_wmadd(&mut fx, 0x0300);
        // DMAP $08: A->B, fixed A-bus, mode 0.
        fx.setup_gpdma(0, 0x08, BBAD_WMDATA, u32::from(src), 4);
        fx.trigger_gpdma(0x01);
        for i in 0..4 {
            assert_wram(&mut fx, 0x0300 + i, 0x7E);
        }
        fx.pass_marker_and_idle();
        run_fixture(fx.build(), "gpdma-fixed.sfc");
    }

    /// B->A GPDMA reads the B-bus target (WMDATA read port `$2180`, which
    /// returns WRAM and auto-increments WMADD) and writes the A-bus, copying
    /// one WRAM region into another.
    ///
    /// Ignored pending #3061: NESER's B->A path reads the controller's internal
    /// `bbus_ports` shadow (unwritten for `$2180` here, so it returns 0) instead
    /// of the live B-bus register. The assertions below are the spec-correct
    /// (hardware) expectations; un-ignore once #3061 routes B->A reads to the
    /// real bus. Unlike the screen-CRC divergence tests (which record NESER's
    /// current CRC and so pass under `--ignored`), this marker test asserts the
    /// correct result and therefore FAILs under `cargo test --include-ignored`
    /// until #3061 lands -- that is expected, not a regression.
    #[test]
    #[ignore = "B->A reads the bbus_ports shadow, not the live B-bus register; asserts correct behaviour so FAILs under --include-ignored until #3061"]
    fn gpdma_b_to_a_copies_wram_through_wmdata() {
        let mut fx = FixtureRom::new(b"NESER DMA B2A");
        fx.force_blank_on();
        // Seed the source region $0500..$0503.
        for (i, byte) in [0x11u8, 0x22, 0x33, 0x44].iter().enumerate() {
            fx.store_imm_abs(0x0500 + i as u16, *byte);
        }
        set_wmadd(&mut fx, 0x0500);
        // DMAP $80: B->A, increment A-bus, mode 0. A-bus dest $0600.
        fx.setup_gpdma(0, 0x80, BBAD_WMDATA, 0x00_0600, 4);
        fx.trigger_gpdma(0x01);
        assert_wram(&mut fx, 0x0600, 0x11);
        assert_wram(&mut fx, 0x0601, 0x22);
        assert_wram(&mut fx, 0x0602, 0x33);
        assert_wram(&mut fx, 0x0603, 0x44);
        fx.pass_marker_and_idle();
        run_fixture(fx.build(), "gpdma-b2a.sfc");
    }

    /// A byte count of 0 means 65536 bytes (not zero): a fixed-source fill of
    /// WRAM reaches well past offset 256, proving the count did not wrap to a
    /// no-op.
    #[test]
    fn gpdma_count_zero_transfers_65536_bytes() {
        let mut fx = FixtureRom::new(b"NESER DMA CNT0");
        fx.force_blank_on();
        let src = fx.place_data(&[0xC3]);
        set_wmadd(&mut fx, 0x0000);
        // DMAP $08: A->B, fixed A-bus, mode 0. Count 0 == 65536.
        fx.setup_gpdma(0, 0x08, BBAD_WMDATA, u32::from(src), 0);
        fx.trigger_gpdma(0x01);
        // Offset 0x0500 (1280) is far beyond a byte count that wrapped to 0.
        assert_wram(&mut fx, 0x0500, 0xC3);
        assert_wram(&mut fx, 0x01F0, 0xC3);
        fx.pass_marker_and_idle();
        run_fixture(fx.build(), "gpdma-count0.sfc");
    }

    /// A single `$420B` trigger with two channels runs them in ascending
    /// priority order: channel 0's bytes land before channel 1's in the shared
    /// WMDATA stream.
    #[test]
    fn gpdma_multichannel_runs_in_ascending_priority() {
        let mut fx = FixtureRom::new(b"NESER DMA PRIO");
        fx.force_blank_on();
        let src_a = fx.place_data(&[0xAA]);
        let src_b = fx.place_data(&[0xBB]);
        set_wmadd(&mut fx, 0x0300);
        // Both channels: fixed source, mode 0, -> the shared WMDATA stream.
        fx.setup_gpdma(0, 0x08, BBAD_WMDATA, u32::from(src_a), 2);
        fx.setup_gpdma(1, 0x08, BBAD_WMDATA, u32::from(src_b), 2);
        fx.trigger_gpdma(0x03);
        assert_wram(&mut fx, 0x0300, 0xAA); // channel 0 first
        assert_wram(&mut fx, 0x0301, 0xAA);
        assert_wram(&mut fx, 0x0302, 0xBB); // then channel 1
        assert_wram(&mut fx, 0x0303, 0xBB);
        fx.pass_marker_and_idle();
        run_fixture(fx.build(), "gpdma-priority.sfc");
    }

    /// PPU access gating (#2944): a VRAM-target DMA lands while force blank is
    /// on, but is dropped when it runs during active display.
    #[test]
    fn gpdma_vram_write_is_gated_by_active_display() {
        let mut fx = FixtureRom::new(b"NESER DMA GATE");
        fx.force_blank_on();
        fx.store_imm_abs(0x2115, 0x80); // VMAIN: increment after high, 1 word
        // Part 1: during force blank, a DMA to VRAM $0100 lands.
        fx.store_imm_abs(0x2116, 0x00);
        fx.store_imm_abs(0x2117, 0x01);
        let src1 = fx.place_data(&[0xEF, 0xBE]);
        fx.setup_gpdma(0, 0x01, 0x18, u32::from(src1), 2);
        fx.trigger_gpdma(0x01);
        // Part 2: during active display, the same kind of DMA to $0200 is gated.
        fx.force_blank_off();
        wait_until_vblank(&mut fx);
        wait_until_active_display(&mut fx);
        fx.store_imm_abs(0x2116, 0x00);
        fx.store_imm_abs(0x2117, 0x02);
        let src2 = fx.place_data(&[0xAD, 0xDE]);
        fx.setup_gpdma(0, 0x01, 0x18, u32::from(src2), 2);
        fx.trigger_gpdma(0x01);
        fx.force_blank_on();
        read_vram_word(&mut fx, 0x0100, 0x0400);
        read_vram_word(&mut fx, 0x0200, 0x0402);
        assert_wram(&mut fx, 0x0400, 0xEF); // landed under force blank
        assert_wram(&mut fx, 0x0401, 0xBE);
        assert_wram(&mut fx, 0x0402, 0x00); // dropped during active display
        assert_wram(&mut fx, 0x0403, 0x00);
        fx.pass_marker_and_idle();
        run_fixture(fx.build(), "gpdma-gate.sfc");
    }

    /// Calibrates the VRAM readback instrument: a CPU write followed by a CPU
    /// read of the same word must round-trip. Guards every VRAM-target DMA test
    /// below against a broken measurement path.
    #[test]
    fn vram_cpu_roundtrip_instrument_self_check() {
        let mut fx = FixtureRom::new(b"NESER VRAM RW");
        fx.force_blank_on();
        cpu_write_vram(&mut fx, 0x0010, 0xABCD);
        read_vram_word(&mut fx, 0x0010, 0x0400);
        assert_wram(&mut fx, 0x0400, 0xCD);
        assert_wram(&mut fx, 0x0401, 0xAB);
        fx.pass_marker_and_idle();
        run_fixture(fx.build(), "vram-selfcheck.sfc");
    }

    /// Calibrates the CGRAM readback instrument (write then read a colour).
    #[test]
    fn cgram_cpu_roundtrip_instrument_self_check() {
        let mut fx = FixtureRom::new(b"NESER CGRAM RW");
        fx.force_blank_on();
        cpu_write_cgram(&mut fx, 0x05, 0x1234);
        read_cgram_color(&mut fx, 0x05, 0x0400);
        assert_wram(&mut fx, 0x0400, 0x34);
        assert_wram(&mut fx, 0x0401, 0x12);
        fx.pass_marker_and_idle();
        run_fixture(fx.build(), "cgram-selfcheck.sfc");
    }

    /// Calibrates the OAM readback instrument (write then read a word).
    #[test]
    fn oam_cpu_roundtrip_instrument_self_check() {
        let mut fx = FixtureRom::new(b"NESER OAM RW");
        fx.force_blank_on();
        cpu_write_oam_word(&mut fx, 0x20, 0x5A, 0xBA);
        read_oam_word(&mut fx, 0x20, 0x0400);
        assert_wram(&mut fx, 0x0400, 0x5A);
        assert_wram(&mut fx, 0x0401, 0xBA);
        fx.pass_marker_and_idle();
        run_fixture(fx.build(), "oam-selfcheck.sfc");
    }
}
