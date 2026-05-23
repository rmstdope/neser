use crate::gb::bus::{CgbBus, DmgBus, GbBus};
use crate::gb::cartridge::load_cartridge;
use crate::gb::console::Gb;
use crate::gb::model::{CgbModel, DmgModel};
use crate::gb::ppu::screen_buffer::ScreenBuffer;
use std::path::Path;

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
    if !step_until_breakpoint(gb, cycle_limit) {
        return MooneyeResult::Timeout;
    }
    let r = &gb.cpu.regs;
    if r.b == FIBO_B
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
    }
}

/// Step `gb` until the `LD B,B` (0x40) opcode is about to execute, or until
/// `cycle_limit` M-cycles have elapsed.
///
/// Returns `true` if the breakpoint was reached, `false` on timeout.
fn step_until_breakpoint<B: GbBus>(gb: &mut Gb<B>, cycle_limit: u64) -> bool {
    let start = gb.cycles();
    loop {
        if gb.cpu.bus.read(gb.cpu.regs.pc) == LD_B_B {
            return true;
        }
        if gb.cycles().saturating_sub(start) >= cycle_limit {
            return false;
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

/// Decode a Game Boy-sized PNG to RGB8 bytes and return its screen-buffer CRC.
pub fn decoded_png_rgb_crc(path: &Path) -> u32 {
    let file = std::fs::File::open(path)
        .unwrap_or_else(|err| panic!("open PNG {}: {err}", path.display()));
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);

    let mut reader = decoder
        .read_info()
        .unwrap_or_else(|err| panic!("read PNG info {}: {err}", path.display()));
    let mut raw = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut raw)
        .unwrap_or_else(|err| panic!("decode PNG {}: {err}", path.display()));
    let raw = &raw[..info.buffer_size()];

    assert_eq!(
        (info.width, info.height),
        (ScreenBuffer::WIDTH, ScreenBuffer::HEIGHT),
        "{} should have Game Boy screen dimensions",
        path.display()
    );

    let rgb = match info.color_type {
        png::ColorType::Rgb => raw.to_vec(),
        png::ColorType::Rgba => raw
            .chunks_exact(4)
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect(),
        png::ColorType::Grayscale => raw.iter().flat_map(|value| [*value; 3]).collect(),
        png::ColorType::GrayscaleAlpha => raw
            .chunks_exact(2)
            .flat_map(|pixel| [pixel[0]; 3])
            .collect(),
        png::ColorType::Indexed => {
            panic!("{} should be expanded from indexed to RGB", path.display())
        }
    };

    assert_eq!(
        rgb.len(),
        (ScreenBuffer::WIDTH * ScreenBuffer::HEIGHT * 3) as usize,
        "{} should decode to RGB8 screen-buffer bytes",
        path.display()
    );

    crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC).checksum(&rgb)
}

// ============================================================================
// In-memory ROM loading helpers (for zip-bundled test suites)
// ============================================================================

/// Load a DMG ROM from raw bytes and return a ready-to-step `Gb<DmgBus>`.
pub fn load_gb_rom_from_bytes(bytes: &[u8], model: DmgModel) -> Gb<DmgBus> {
    let cart = load_cartridge(bytes).expect("valid GB ROM bytes");
    Gb::new(DmgBus::new(cart, model))
}

/// Load a CGB ROM from raw bytes and return a ready-to-step `Gb<CgbBus>`.
pub fn load_cgb_rom_from_bytes(bytes: &[u8], model: CgbModel) -> Gb<CgbBus> {
    let cart = load_cartridge(bytes).expect("valid CGB ROM bytes");
    let mut gb = Gb::new(CgbBus::new(cart, model, true));
    gb.cpu.reset_registers_cgb_for_model(model);
    gb
}

// ============================================================================
// Breakpoint-based screen capture helper
// ============================================================================

/// Run `gb` until the `LD B,B` (0x40) software breakpoint fires and return the
/// CRC-32 of the screen buffer at that moment.
///
/// Panics if the breakpoint is not reached within `cycle_limit` M-cycles, so
/// that a hanging ROM is always a test failure rather than a silent pass.
///
/// When the `NESER_CAPTURE_SCREEN` environment variable is set, saves a PNG to
/// `target/mealybug-captures/<capture_name>.png` for visual baseline comparison.
pub fn run_to_breakpoint_and_crc<B: GbBus>(
    gb: &mut Gb<B>,
    cycle_limit: u64,
    capture_name: &str,
) -> u32 {
    assert!(
        step_until_breakpoint(gb, cycle_limit),
        "{capture_name}: LD B,B breakpoint not reached within {cycle_limit} M-cycles"
    );

    let crc = gb.cpu.bus.ppu().screen_buffer().crc32();

    if std::env::var_os("NESER_CAPTURE_SCREEN").is_some() {
        let dir = std::path::Path::new("target/mealybug-captures");
        std::fs::create_dir_all(dir).expect("create mealybug-captures dir");
        let path = dir.join(format!("{capture_name}.png"));
        save_screen_png(gb, path.to_str().expect("valid path"));
        println!("[mealybug] {capture_name}: CRC={crc:#010X}, PNG saved to {path:?}");
    }

    crc
}
