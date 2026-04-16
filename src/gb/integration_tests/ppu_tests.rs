use crate::gb::bus::DmgBus;
use crate::gb::cartridge::load_cartridge;
use crate::gb::console::Gb;
use crate::gb::model::DmgModel;

fn load_gb_rom(path: &str) -> Gb<DmgBus> {
    let rom = std::fs::read(path).expect("ROM file should be present");
    let cart = load_cartridge(&rom).expect("valid GB ROM");
    Gb::new(DmgBus::new(cart, DmgModel::DmgAbc))
}

fn run_one_gb_frame(gb: &mut Gb<DmgBus>) {
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

/// Validate that `dmg-acid2.gb` renders a frame matching the expected CRC.
///
/// This test is ignored until a CRC baseline has been visually confirmed against
/// the published dmg-acid2 reference image and locked in.
#[test]
#[ignore = "CRC baseline not yet established — run manually and lock once confirmed"]
fn test_dmg_acid2_frame_matches_reference_crc() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/dmg-acid2/dmg-acid2.gb");

    // Run 200 frames to allow the ROM to finish booting and render its test output.
    let crc = run_frames_and_crc(&mut gb, 200);

    // Replace 0x00000000 with the visually verified CRC once the baseline is approved.
    const EXPECTED_CRC: u32 = 0x0000_0000;
    assert_eq!(
        crc, EXPECTED_CRC,
        "dmg-acid2 frame CRC mismatch: got {crc:#010X}, expected {EXPECTED_CRC:#010X}"
    );
}
