use crate::gb::bus::CgbBus;
use crate::gb::bus::DmgBus;
use crate::gb::cartridge::load_cartridge;
use crate::gb::console::Gb;
use crate::gb::model::DmgModel;

fn load_gb_rom(path: &str) -> Gb<DmgBus> {
    let rom = std::fs::read(path).expect("ROM file should be present");
    let cart = load_cartridge(&rom).expect("valid GB ROM");
    Gb::new(DmgBus::new(cart, DmgModel::DmgAbc))
}

fn load_cgb_rom(path: &str) -> Gb<CgbBus> {
    let rom = std::fs::read(path).expect("ROM file should be present");
    let cart = load_cartridge(&rom).expect("valid GB ROM");
    let mut gb = Gb::new(CgbBus::new(cart));
    // Set CGB post-boot-ROM CPU register state (A=$11 = CGB hardware identifier).
    gb.cpu.reset_registers_cgb();
    gb
}

fn run_one_gb_frame(gb: &mut Gb<DmgBus>) {
    gb.clear_frame_ready();
    while !gb.is_frame_ready() {
        gb.step();
    }
}

fn run_one_cgb_frame(gb: &mut Gb<CgbBus>) {
    gb.clear_frame_ready();
    while !gb.is_frame_ready() {
        gb.step();
    }
}

/// Run `n` frames and return the CRC-32 of the screen buffer after the last frame.
fn run_frames_and_crc(gb: &mut Gb<DmgBus>, n: u32) -> u32 {
    for _ in 0..n {
        run_one_gb_frame(gb);
    }
    gb.cpu.bus.ppu.screen_buffer().crc32()
}

/// Run `n` CGB frames and return the CRC-32 of the screen buffer.
fn run_cgb_frames_and_crc(gb: &mut Gb<CgbBus>, n: u32) -> u32 {
    for _ in 0..n {
        run_one_cgb_frame(gb);
    }
    gb.cpu.bus.ppu.screen_buffer().crc32()
}

/// Save the screen buffer as a PNG to `path` for visual inspection.
fn save_screen_png(gb: &Gb<DmgBus>, path: &str) {
    use crate::gb::ppu::screen_buffer::ScreenBuffer;
    use png::{BitDepth, ColorType, Encoder};
    use std::fs::File;
    use std::io::{BufWriter, Write};

    let buf = gb.cpu.bus.ppu.screen_buffer();
    let w = ScreenBuffer::WIDTH;
    let h = ScreenBuffer::HEIGHT;
    let file = File::create(path).expect("should create PNG file");
    let mut bw = BufWriter::new(file);
    let mut enc = Encoder::new(&mut bw, w, h);
    enc.set_color(ColorType::Rgb);
    enc.set_depth(BitDepth::Eight);
    let mut png_writer = enc.write_header().expect("write PNG header");
    let raw: Vec<u8> = (0..h)
        .flat_map(|y| {
            (0..w).flat_map(move |x| {
                let (r, g, b) = buf.get_pixel(x, y);
                [r, g, b]
            })
        })
        .collect();
    png_writer.write_image_data(&raw).expect("write PNG data");
    drop(png_writer);
    bw.flush().expect("flush PNG writer");
}

/// Save the CGB screen buffer as a PNG to `path` for visual inspection.
fn save_cgb_screen_png(gb: &Gb<CgbBus>, path: &str) {
    use crate::gb::ppu::screen_buffer::ScreenBuffer;
    use png::{BitDepth, ColorType, Encoder};
    use std::fs::File;
    use std::io::{BufWriter, Write};

    let buf = gb.cpu.bus.ppu.screen_buffer();
    let w = ScreenBuffer::WIDTH;
    let h = ScreenBuffer::HEIGHT;
    let file = File::create(path).expect("should create PNG file");
    let mut bw = BufWriter::new(file);
    let mut enc = Encoder::new(&mut bw, w, h);
    enc.set_color(ColorType::Rgb);
    enc.set_depth(BitDepth::Eight);
    let mut png_writer = enc.write_header().expect("write PNG header");
    let raw: Vec<u8> = (0..h)
        .flat_map(|y| {
            (0..w).flat_map(move |x| {
                let (r, g, b) = buf.get_pixel(x, y);
                [r, g, b]
            })
        })
        .collect();
    png_writer.write_image_data(&raw).expect("write PNG data");
    drop(png_writer);
    bw.flush().expect("flush PNG writer");
}

/// Validate that `dmg-acid2.gb` renders a frame matching the expected CRC.
///
/// Baseline CRC `0x52D18222` was visually confirmed against the published
/// dmg-acid2 DMG reference image (smiley face, correct OBJ priority, no moles visible).
#[test]
fn test_dmg_acid2_frame_matches_reference_crc() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/dmg-acid2/dmg-acid2.gb");

    // Run 200 frames to allow the ROM to finish booting and render its test output.
    let crc = run_frames_and_crc(&mut gb, 200);

    const EXPECTED_CRC: u32 = 0x52D1_8222;
    assert_eq!(
        crc, EXPECTED_CRC,
        "dmg-acid2 frame CRC mismatch: got {crc:#010X}, expected {EXPECTED_CRC:#010X}"
    );
}

/// Validate that `cgb-acid2.gbc` renders a frame matching the expected CRC.
///
/// Baseline CRC established after visual confirmation against the published
/// cgb-acid2 CGB reference image (color smiley face, correct CGB palette/priority).
/// Tracking issue: https://github.com/rmstdope/neser/issues/2099
#[test]
fn test_cgb_acid2_frame_matches_reference_crc() {
    let mut gb = load_cgb_rom("roms/gb/automated_tests/cgb-acid2/cgb-acid2.gbc");

    // Run 200 frames to allow the ROM to render its test output.
    let crc = run_cgb_frames_and_crc(&mut gb, 200);

    // CRC established after visual confirmation — see PR for screenshot.
    const EXPECTED_CRC: u32 = 0x0E0B_C136;
    assert_eq!(
        crc, EXPECTED_CRC,
        "cgb-acid2 frame CRC mismatch: got {crc:#010X}, expected {EXPECTED_CRC:#010X}"
    );
}

/// Screenshot helper: saves `dmg-acid2.gb` frame 200 to `dmg-acid2-screenshot.png`.
/// Run with `cargo test -- --ignored` for visual baseline inspection.
#[test]
#[ignore = "screenshot helper — run manually for visual baseline inspection"]
fn save_dmg_acid2_screenshot() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/dmg-acid2/dmg-acid2.gb");
    for _ in 0..200 {
        run_one_gb_frame(&mut gb);
    }
    save_screen_png(&gb, "dmg-acid2-screenshot.png");
    println!("Screenshot saved to dmg-acid2-screenshot.png");
    println!("CRC: {:#010X}", gb.cpu.bus.ppu.screen_buffer().crc32());
}

/// Screenshot helper: saves `cgb-acid2.gbc` frame 200 to `cgb-acid2-screenshot.png`.
/// Run with `cargo test -- --ignored` for visual baseline inspection.
#[test]
#[ignore = "screenshot helper — run manually for visual baseline inspection"]
fn save_cgb_acid2_screenshot() {
    let mut gb = load_cgb_rom("roms/gb/automated_tests/cgb-acid2/cgb-acid2.gbc");
    for _ in 0..200 {
        run_one_cgb_frame(&mut gb);
    }
    save_cgb_screen_png(&gb, "cgb-acid2-screenshot.png");
    println!("Screenshot saved to cgb-acid2-screenshot.png");
    println!("CRC: {:#010X}", gb.cpu.bus.ppu.screen_buffer().crc32());
}
