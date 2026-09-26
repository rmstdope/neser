//! ROM-level coverage for the OBC1 OBJ controller (nr-ufb).
//!
//! No OBC1 test ROM is known, so a hand-built fixture on an OBC1 cartridge drives the chip
//! from the 65816 the way Metal Combat: Falcon's Revenge does: it selects an object through
//! the base and index ports, writes its OAM bytes and high-table bits through `$7FF0-$7FF4`,
//! and then checks the SRAM buffer directly (fullsnes "SNES Cart OBC1 (OBJ Controller)"),
//! reporting through the `rom_runner` marker protocol.

use super::fixture_rom::FixtureRom;
use super::rom_runner::{RunConfig, RunExitReason, run_rom};

fn build_obc1_fixture() -> Vec<u8> {
    let mut fixture = FixtureRom::new(b"OBC1 FIXTURE");
    fixture.obc1_chipset();

    // Base $7C00 (bit 0 clear), object 5: OAM bytes at $7C14-$7C17, high-table byte $7E01
    // bits 2-3. Pre-fill the high-table byte so the write must keep its other bits.
    fixture.store_imm_abs(0x7E01, 0xFF);
    for (port, value) in [
        (0x7FF5, 0x00),
        (0x7FF6, 0x05),
        (0x7FF0, 0x11),
        (0x7FF1, 0x22),
        (0x7FF2, 0x33),
        (0x7FF3, 0x44),
        (0x7FF4, 0x02),
    ] {
        fixture.store_imm_abs(port, value);
    }
    for (addr, expected) in [
        (0x7C14, 0x11),
        (0x7C15, 0x22),
        (0x7C16, 0x33),
        (0x7C17, 0x44),
        (0x7E01, 0xFB),
        (0x7FF2, 0x33), // the ports read the buffer back
        (0x7FF4, 0xFB), // the whole high-table byte (Mesen2, ares)
    ] {
        fixture.lda_abs(addr);
        fixture.branch_fail_if_ne(expected);
    }

    // Base $7800, object 127: OAM bytes at $79FC-$79FF, high-table byte $7A1F bits 6-7.
    for (port, value) in [
        (0x7FF5, 0x01),
        (0x7FF6, 0x7F),
        (0x7FF0, 0xA0),
        (0x7FF3, 0xA3),
        (0x7FF4, 0x03),
    ] {
        fixture.store_imm_abs(port, value);
    }
    for (addr, expected) in [
        (0x79FC, 0xA0),
        (0x79FF, 0xA3),
        (0x7A1F, 0xC0),
        (0x7C14, 0x11),
    ] {
        fixture.lda_abs(addr);
        fixture.branch_fail_if_ne(expected);
    }
    fixture.pass_marker_and_idle();
    fixture.build()
}

#[test]
fn obc1_fixture_builds_oam_through_the_ports() {
    let result = run_rom(
        &build_obc1_fixture(),
        "obc1-fixture.sfc",
        RunConfig::new(0, 30),
    );
    assert!(
        result.passed && result.exit_reason == RunExitReason::PassMarker,
        "OBC1 fixture should pass: {result:?}"
    );
}
