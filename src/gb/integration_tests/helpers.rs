use crate::gb::bus::{CgbBus, DmgBus, GbBus};
use crate::gb::cartridge::load_cartridge;
use crate::gb::console::Gb;
use crate::gb::model::{CgbModel, DmgModel};

/// Load a DMG ROM from `path` and return a ready-to-step `Gb<DmgBus>` (DMG-B model).
pub fn load_gb_rom(path: &str) -> Gb<DmgBus> {
    let rom = std::fs::read(path).expect("ROM file should be present");
    let cart = load_cartridge(&rom).expect("valid GB ROM");
    Gb::new(DmgBus::new(cart, DmgModel::DmgB))
}

/// Load a CGB ROM from `path` and return a ready-to-step `Gb<CgbBus>`.
///
/// Sets the post-boot-ROM CGB CPU register state (A=$11 = CGB hardware identifier).
pub fn load_cgb_rom(path: &str) -> Gb<CgbBus> {
    let rom = std::fs::read(path).expect("ROM file should be present");
    let cart = load_cartridge(&rom).expect("valid GB ROM");
    let mut gb = Gb::new(CgbBus::new(cart, CgbModel::default()));
    // Set CGB post-boot-ROM CPU register state (A=$11 = CGB hardware identifier).
    gb.cpu.reset_registers_cgb();
    gb
}

/// Advance `gb` by exactly one full frame (until `is_frame_ready` is set).
pub fn run_one_frame<B: GbBus>(gb: &mut Gb<B>) {
    gb.clear_frame_ready();
    while !gb.is_frame_ready() {
        gb.step();
    }
}

/// Run `n` frames and return the CRC-32 of the screen buffer after the last frame.
pub fn run_frames_and_crc<B: GbBus>(gb: &mut Gb<B>, n: u32) -> u32 {
    for _ in 0..n {
        run_one_frame(gb);
    }
    gb.cpu.bus.ppu().screen_buffer().crc32()
}

/// Save the screen buffer as a PNG to `path` for visual inspection.
pub fn save_screen_png<B: GbBus>(gb: &Gb<B>, path: &str) {
    use crate::gb::ppu::screen_buffer::ScreenBuffer;
    use png::{BitDepth, ColorType, Encoder};
    use std::fs::File;
    use std::io::{BufWriter, Write};

    let buf = gb.cpu.bus.ppu().screen_buffer();
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
