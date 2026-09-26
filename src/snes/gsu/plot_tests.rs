//! The bitmap unit against fullsnes "SNES Cart GSU-n Bitmap I/O Ports" and "Pixel-Cache".

use super::core_tests::{PROGRAM, Rig};

const SCMR_RON_RAN: u8 = 0x18;
const DEPTH_4: u8 = 0x00;
const DEPTH_16: u8 = 0x01;
const DEPTH_256: u8 = 0x03;
const HEIGHT_160: u8 = 0x04;
const HEIGHT_OBJ: u8 = 0x24;

/// Runs `program` (STOP; NOP appended) with SCMR = RON | RAN | `mode` and `regs` preset.
fn run(mode: u8, regs: &[(u16, u16)], program: &[u8], ram_fill: u8) -> Rig {
    let mut code = program.to_vec();
    code.extend_from_slice(&[0x00, 0x01]);
    let mut rig = Rig::new(&code);
    rig.ram.borrow_mut().fill(ram_fill);
    for &(n, value) in regs {
        rig.set_reg(n, value);
    }
    rig.start_at(PROGRAM);
    rig.gsu.write_register(0x303A, SCMR_RON_RAN | mode);
    rig.run_until_stop_and_settle();
    rig
}

// COLOR from R0, then PLOT, then RPIX to flush the pixel caches.
const COLOR: u8 = 0x4E;
const CMODE: [u8; 2] = [0x3D, 0x4E];
const PLOT: u8 = 0x4C;
const RPIX: [u8; 2] = [0x3D, 0x4C];

#[test]
fn tile_row_address_for_each_height_and_depth() {
    let mut rig = Rig::new(&[]);
    let mut address = |scmr: u8, por: u8, scbr: u8, x: u8, y: u8| {
        rig.gsu.state.scmr = scmr;
        rig.gsu.state.por = por;
        rig.gsu.state.scbr = scbr;
        rig.gsu.plot_row_address(x, y)
    };
    // 128 high, 4 colours: tile (X/8)*$10 + Y/8, row TileNo*$10 + (Y&7)*2.
    assert_eq!(address(DEPTH_4, 0, 0, 17, 9), (2 * 0x10 + 1) * 0x10 + 2);
    // 160 high, 16 colours: tile (X/8)*$14 + Y/8, row TileNo*$20.
    assert_eq!(address(DEPTH_16 | HEIGHT_160, 0, 0, 8, 0), 0x14 * 0x20);
    // 192 high (HT1 only), 256 colours: tile (X/8)*$18 + Y/8, row TileNo*$40.
    assert_eq!(
        address(DEPTH_256 | 0x20, 0, 0, 8, 15),
        (0x18 + 1) * 0x40 + 7 * 2
    );
    // OBJ mode: (Y/$80)*$200 + (X/$80)*$100 + (Y/8 AND $F)*$10 + (X/8 AND $F).
    let obj_tile = 0x100 + 2 * 0x10 + 1;
    assert_eq!(
        address(DEPTH_4 | HEIGHT_OBJ, 0, 0, 0x88, 0x10),
        obj_tile * 0x10
    );
    assert_eq!(
        address(DEPTH_4, 0x10, 0, 0x88, 0x10),
        obj_tile * 0x10,
        "POR bit 4"
    );
    // SCBR is in 1 KB units.
    assert_eq!(address(DEPTH_4, 0, 2, 0, 0), 0x800);
}

#[test]
fn plot_writes_bitplanes_and_advances_r1() {
    let program = [COLOR, PLOT, RPIX[0], RPIX[1]];
    let mut rig = run(DEPTH_4, &[(0, 0x0003)], &program, 0);
    let ram = rig.ram.borrow().clone();
    assert_eq!(
        (ram[0], ram[1]),
        (0x80, 0x80),
        "colour 3 at x=0: bit 7 of both planes"
    );
    assert_eq!(rig.reg(1), 1, "PLOT increments R1");
}

#[test]
fn plot_256_colours_spreads_eight_planes() {
    let program = [COLOR, PLOT, RPIX[0], RPIX[1]];
    let rig = run(DEPTH_256, &[(0, 0x0081)], &program, 0);
    let ram = rig.ram.borrow();
    assert_eq!(ram[0x00], 0x80, "plane 0");
    assert_eq!(ram[0x31], 0x80, "plane 7 at +$31");
    assert_eq!(
        ram[0x01] | ram[0x10] | ram[0x11] | ram[0x20] | ram[0x21] | ram[0x30],
        0
    );
}

#[test]
fn plot_skips_colour_0_unless_por_bit0() {
    let program = [COLOR, PLOT, RPIX[0], RPIX[1]];
    let mut rig = run(DEPTH_4, &[(0, 0x0000)], &program, 0xFF);
    assert_eq!(rig.ram.borrow()[0], 0xFF, "transparent: nothing drawn");
    assert_eq!(rig.reg(1), 1, "but R1 still advances");

    // CMODE #1 (from R3), then COLOR 0: colour 0 is drawn.
    let program = [0xB3, CMODE[0], CMODE[1], COLOR, PLOT, RPIX[0], RPIX[1]];
    let rig = run(DEPTH_4, &[(0, 0x0000), (3, 0x0001)], &program, 0xFF);
    assert_eq!(rig.ram.borrow()[0], 0x7F);
}

#[test]
fn freeze_high_makes_256_colour_transparency_test_the_low_nibble() {
    // CMODE #8 (freeze high), COLOR $50: the upper nibble is ignored, so the colour is 0.
    let program = [0xB3, CMODE[0], CMODE[1], COLOR, PLOT, RPIX[0], RPIX[1]];
    let rig = run(DEPTH_256, &[(0, 0x0050), (3, 0x0008)], &program, 0x00);
    assert!(rig.ram.borrow()[..0x40].iter().all(|&b| b == 0));
}

#[test]
fn pixel_cache_merges_partial_rows() {
    // CMODE #1, COLOR 0, PLOT at x=3: only bit 4 of each plane changes.
    let program = [0xB3, CMODE[0], CMODE[1], COLOR, PLOT, RPIX[0], RPIX[1]];
    let rig = run(DEPTH_4, &[(0, 0x0000), (1, 3), (3, 0x0001)], &program, 0xFF);
    let ram = rig.ram.borrow();
    assert_eq!((ram[0], ram[1]), (0xEF, 0xEF));
    assert_eq!(ram[2], 0xFF, "next row untouched");
}

#[test]
fn dither_uses_high_nibble_on_odd_pixels() {
    // CMODE #2 (dither), COLOR $21, PLOT twice at (0,0) and (1,0): colours 1 then 2.
    let program = [
        0xB3, CMODE[0], CMODE[1], COLOR, PLOT, PLOT, RPIX[0], RPIX[1],
    ];
    let rig = run(DEPTH_4, &[(0, 0x0021), (3, 0x0002)], &program, 0x00);
    let ram = rig.ram.borrow();
    assert_eq!(ram[0], 0x80, "plane 0: pixel 0 = 1");
    assert_eq!(ram[1], 0x40, "plane 1: pixel 1 = 2");
}

#[test]
fn dithered_colour_0_half_is_transparent() {
    // fullsnes: "Dither can mix transparent & non-transparent pixels": COLR $50 dithers to
    // 0 on even pixels (skipped) and 5 on odd ones (drawn). Transparency is tested after the
    // dither stage, as in fullsnes' COLOR -> Dither -> Transp diagram.
    let program = [
        0xB3, CMODE[0], CMODE[1], COLOR, PLOT, PLOT, RPIX[0], RPIX[1],
    ];
    let rig = run(DEPTH_16, &[(0, 0x0050), (3, 0x0002)], &program, 0x00);
    let ram = rig.ram.borrow();
    // Colour 5 = planes 0 and 2 at x=1 (bit 6); x=0 untouched.
    assert_eq!((ram[0x00], ram[0x01], ram[0x10]), (0x40, 0x00, 0x40));
}

#[test]
fn color_high_nibble_and_freeze_high() {
    let mut rig = Rig::new(&[]);
    let mut input = |por: u8, colr: u8, value: u8| {
        rig.gsu.state.por = por;
        rig.gsu.state.colr = colr;
        rig.gsu.color_register_input(value)
    };
    assert_eq!(input(0x00, 0x50, 0xAB), 0xAB);
    // High-nibble: "replace incoming LSB by incoming MSB".
    assert_eq!(input(0x04, 0x50, 0xAB), 0xAA);
    // Freeze-high: "Write-protect COLOR.MSB".
    assert_eq!(input(0x08, 0x50, 0xAB), 0x5B);
    assert_eq!(input(0x0C, 0x50, 0xAB), 0x5A);
}

#[test]
fn getc_loads_colr_from_the_rom_buffer() {
    let mut rig = Rig::with_rom(|rom| {
        #[rustfmt::skip]
        rom[..10].copy_from_slice(&[
            0xFE, 0x00, 0x81, // IWT R14,#$8100
            0xDF,             // GETC
            PLOT, RPIX[0], RPIX[1],
            0x00, 0x01, 0x01,
        ]);
        rom[0x100] = 0x02;
    });
    rig.start_at(PROGRAM);
    rig.run_until_stop_and_settle();
    assert_eq!((rig.ram.borrow()[0], rig.ram.borrow()[1]), (0x00, 0x80));
}

#[test]
fn rpix_flushes_caches_then_reads_pixel() {
    // PLOT colour 2 at x=5, move R1 back to 5, RPIX into R4.
    let program = [COLOR, PLOT, 0xA1, 0x05, 0x14, RPIX[0], RPIX[1]];
    let mut rig = run(DEPTH_4, &[(0, 0x0002), (1, 5)], &program, 0x00);
    assert_eq!(rig.reg(4), 0x0002);
    assert_eq!(rig.sfr() & 0x0A, 0, "not zero, not negative");
}
