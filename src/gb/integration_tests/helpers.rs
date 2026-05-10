use crate::gb::bus::{CgbBus, DmgBus, GbBus};
use crate::gb::cartridge::load_cartridge;
use crate::gb::console::Gb;
use crate::gb::model::{CgbModel, DmgModel};

// ============================================================================
// Mooneye/SameSuite Test Oracle Constants and Types
// ============================================================================

/// Outcome of running a Mooneye or SameSuite test ROM to completion.
#[derive(Debug, PartialEq)]
pub enum MooneyeResult {
    /// B=3, C=5, D=8, E=13, H=21, L=34 at the `LD B,B` breakpoint.
    Pass,
    /// The `LD B,B` breakpoint was hit but registers did not match the Fibonacci pattern.
    Fail {
        b: u8,
        c: u8,
        d: u8,
        e: u8,
        h: u8,
        l: u8,
    },
    /// The ROM did not hit the breakpoint within the M-cycle budget.
    Timeout,
}

/// Mooneye/SameSuite pass: Fibonacci register values at `LD B,B` breakpoint.
pub const FIBO_B: u8 = 3;
pub const FIBO_C: u8 = 5;
pub const FIBO_D: u8 = 8;
pub const FIBO_E: u8 = 13;
pub const FIBO_H: u8 = 21;
pub const FIBO_L: u8 = 34;

/// LD B,B opcode used as a Mooneye/SameSuite software breakpoint.
pub const LD_B_B: u8 = 0x40;

// ============================================================================
// ROM Loading Helpers
// ============================================================================

/// Load a DMG ROM from `path` and return a ready-to-step `Gb<DmgBus>` (DMG-B model).
pub fn load_gb_rom(path: &str) -> Gb<DmgBus> {
    load_gb_rom_with_model(path, DmgModel::DmgB)
}

/// Load a DMG ROM from `path` with a specific hardware model.
pub fn load_gb_rom_with_model(path: &str, model: DmgModel) -> Gb<DmgBus> {
    let rom = std::fs::read(path).expect("ROM file should be present");
    let cart = load_cartridge(&rom).expect("valid GB ROM");
    Gb::new(DmgBus::new(cart, model))
}

/// Load a CGB ROM from `path` and return a ready-to-step `Gb<CgbBus>`.
///
/// Sets the post-boot-ROM CGB CPU register state (A=$11 = CGB hardware identifier).
pub fn load_cgb_rom(path: &str) -> Gb<CgbBus> {
    load_cgb_rom_with_model(path, CgbModel::default())
}

/// Load a CGB ROM from `path` with a specific hardware model.
///
/// Sets the post-boot-ROM CGB CPU register state based on the model variant.
/// Currently all CGB models use the same register values (Mooneye boot_regs-cgb);
/// model-specific differences may be added when verified against hardware.
pub fn load_cgb_rom_with_model(path: &str, model: CgbModel) -> Gb<CgbBus> {
    let rom = std::fs::read(path).expect("ROM file should be present");
    let cart = load_cartridge(&rom).expect("valid GB ROM");
    let mut gb = Gb::new(CgbBus::new(cart, model, true));
    // Set CGB post-boot-ROM CPU register state for the specific model variant.
    gb.cpu.reset_registers_cgb_for_model(model);
    gb
}

// ============================================================================
// Mooneye/SameSuite Test Oracle Detection
// ============================================================================

/// Step `gb` until the Mooneye/SameSuite breakpoint fires or `cycle_limit` M-cycles elapse.
///
/// Detects the `LD B,B` (0x40) breakpoint by peeking at the next opcode
/// before each step. For these tests, peeking at the opcode at `PC` is safe
/// because execution is in cartridge/boot ROM space in our bus implementation.
///
/// Works with both DMG and CGB bus implementations.
pub fn detect_mooneye_result_with_limit<B: GbBus>(
    gb: &mut Gb<B>,
    cycle_limit: u64,
) -> MooneyeResult {
    let start = gb.cycles();
    loop {
        let opcode = gb.cpu.bus.read(gb.cpu.regs.pc);
        if opcode == LD_B_B {
            let r = &gb.cpu.regs;
            if r.b == FIBO_B
                && r.c == FIBO_C
                && r.d == FIBO_D
                && r.e == FIBO_E
                && r.h == FIBO_H
                && r.l == FIBO_L
            {
                return MooneyeResult::Pass;
            }
            return MooneyeResult::Fail {
                b: r.b,
                c: r.c,
                d: r.d,
                e: r.e,
                h: r.h,
                l: r.l,
            };
        }

        if gb.cycles().saturating_sub(start) >= cycle_limit {
            return MooneyeResult::Timeout;
        }

        gb.step();
    }
}

/// Run a DMG ROM and detect Mooneye/SameSuite result with a given cycle limit.
pub fn run_and_detect_dmg(path: &str, model: DmgModel, cycle_limit: u64) -> MooneyeResult {
    let mut gb = load_gb_rom_with_model(path, model);
    detect_mooneye_result_with_limit(&mut gb, cycle_limit)
}

/// Run a CGB ROM and detect Mooneye/SameSuite result with a given cycle limit.
pub fn run_and_detect_cgb(path: &str, model: CgbModel, cycle_limit: u64) -> MooneyeResult {
    let mut gb = load_cgb_rom_with_model(path, model);
    detect_mooneye_result_with_limit(&mut gb, cycle_limit)
}

/// Run a CGB ROM until the LD B,B breakpoint; return the result AND dump `n`
/// bytes from `start_addr` (useful for diagnosing SameSuite failures).
#[cfg(test)]
pub fn run_cgb_and_dump<const N: usize>(
    path: &str,
    model: CgbModel,
    cycle_limit: u64,
    start_addr: u16,
) -> (MooneyeResult, [u8; N]) {
    let mut gb = load_cgb_rom_with_model(path, model);
    let start = gb.cycles();
    loop {
        let opcode = gb.cpu.bus.read(gb.cpu.regs.pc);
        if opcode == LD_B_B {
            let mut buf = [0u8; N];
            for (i, b) in buf.iter_mut().enumerate() {
                *b = gb.cpu.bus.read(start_addr + i as u16);
            }
            let r = &gb.cpu.regs;
            let result = if r.b == FIBO_B
                && r.c == FIBO_C
                && r.d == FIBO_D
                && r.e == FIBO_E
                && r.h == FIBO_H
                && r.l == FIBO_L
            {
                MooneyeResult::Pass
            } else {
                MooneyeResult::Fail {
                    b: r.b,
                    c: r.c,
                    d: r.d,
                    e: r.e,
                    h: r.h,
                    l: r.l,
                }
            };
            return (result, buf);
        }
        if gb.cycles().saturating_sub(start) >= cycle_limit {
            return (MooneyeResult::Timeout, [0; N]);
        }
        gb.step();
    }
}

// ============================================================================
// Frame and Screen Helpers
// ============================================================================

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
