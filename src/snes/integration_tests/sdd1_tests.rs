//! ROM-level coverage for the S-DD1 (nr-10g).
//!
//! No S-DD1 test ROM exists (none is vendored, and none is published), so a hand-built
//! fixture drives the chip the way Star Ocean and Street Fighter Alpha 2 do: 65816 code
//! programs a fixed-address DMA from the `$C0` bank, enables decompression through `$4800`
//! and `$4801`, and lets the DMA controller copy the decompressed bytes into WRAM through
//! WMDATA. It then checks them itself and reports through the `rom_runner` marker protocol.

use super::fixture_rom::FixtureRom;
use super::rom_runner::{RunConfig, RunExitReason, run_rom};

/// A 4-bitplane stream (header `$81`) and its first 64 decompressed bytes, from Mesen2's
/// `Sdd1Decomp` (the same vector as `decompressor.rs`' `MESEN2_VECTORS`, header `$8x`).
const STREAM: &str = concat!(
    "81FC05219559FBA8787AA25DC2905AEDF5862E6716D875FF4E738BF42BC4BB82",
    "961C703B8113A709CD232F753C9E1F440F9819A24142F3919CA2449340C4F9B0",
    "693A9C589582A480D9D9E2EE6E9A6CA5FEE32E0CA32398785A43A6C41C428281",
);
const DECOMPRESSED: &str = concat!(
    "3060278C93DDC7F4287891EA22EE90635C810B69E091AE8CCE4E4AAF85CF76C7",
    "416320F094104208A44053370B0E8217B3E8C314242B965747280DAD164DA2DF",
);

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
        .collect()
}

/// Where the stream sits: LoROM file offset `$8000`, which bank register `$4804` = 0 (its
/// power-on value) maps at `$C0:8000`.
const STREAM_ADDR: u32 = 0xC0_8000;

/// Emits a DMA of `count` decompressed bytes on channel 0 into WRAM `$00:dest`.
fn decompress_into_wram(fixture: &mut FixtureRom, dest: u16, count: u16) {
    // DMAP $08: A->B, fixed A-bus address; B-bus $80 = WMDATA.
    fixture.setup_gpdma(0, 0x08, 0x80, STREAM_ADDR, count);
    fixture.store_imm_abs(0x2181, (dest & 0xFF) as u8);
    fixture.store_imm_abs(0x2182, (dest >> 8) as u8);
    fixture.store_imm_abs(0x2183, 0x00);
    fixture.store_imm_abs(0x4800, 0x01);
    fixture.store_imm_abs(0x4801, 0x01);
    fixture.trigger_gpdma(0x01);
}

fn build_sdd1_fixture() -> Vec<u8> {
    let mut fixture = FixtureRom::new(b"SDD1 DMA FIXTURE");
    fixture.sdd1_chipset();
    fixture.place_in_bank1(0x0000, &hex(STREAM));
    let expected = hex(DECOMPRESSED);

    decompress_into_wram(&mut fixture, 0x1000, 64);
    for (i, &byte) in expected.iter().enumerate() {
        fixture.lda_abs(0x1000 + i as u16);
        fixture.branch_fail_if_ne(byte);
    }
    // The transfer's end clears its $4801 bit; $4800 is kept.
    fixture.lda_abs(0x4801);
    fixture.branch_fail_if_ne(0x00);
    fixture.lda_abs(0x4800);
    fixture.branch_fail_if_ne(0x01);

    // Re-armed, the next transfer decompresses the stream from its start again.
    decompress_into_wram(&mut fixture, 0x1100, 8);
    for (i, &byte) in expected.iter().take(8).enumerate() {
        fixture.lda_abs(0x1100 + i as u16);
        fixture.branch_fail_if_ne(byte);
    }
    fixture.pass_marker_and_idle();
    fixture.build()
}

#[test]
fn sdd1_fixture_decompresses_through_dma() {
    let result = run_rom(
        &build_sdd1_fixture(),
        "sdd1-dma-fixture.sfc",
        RunConfig::new(0, 30),
    );
    assert!(
        result.passed && result.exit_reason == RunExitReason::PassMarker,
        "S-DD1 DMA fixture should pass: {result:?}"
    );
}
