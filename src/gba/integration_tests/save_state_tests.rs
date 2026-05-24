use crate::gba::Gba;
use crate::gba::cartridge::header::{
    COMPLEMENT_CHECK_OFFSET, FIXED_BYTE_OFFSET, FIXED_BYTE_VALUE, compute_complement_check,
};
use crate::gba::console::save_state::{GBA_SAVESTATE_VERSION, GbaSaveStateError};
use crate::gba::cpu::bus::Bus;
use crate::gba::ppu;
use crate::gba::ppu::{CYCLES_PER_SCANLINE, SCANLINES_PER_FRAME};
use crate::platform::app_context::AppContext;
use crate::platform::config::Config;
use crate::platform::emulator::Emulator;

fn make_gba() -> Gba {
    let mut config = Config::default();
    config.gba.bios_path = Some("embedded".to_string());
    Gba::new(AppContext::new_with_config(config))
}

fn minimal_valid_gba_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 0xC0];
    rom[FIXED_BYTE_OFFSET] = FIXED_BYTE_VALUE;
    rom[COMPLEMENT_CHECK_OFFSET] = compute_complement_check(&rom);
    rom
}

fn render_mode3_frame(gba: &mut Gba, color: u16) {
    let bus = gba.bus_mut();
    bus.write16(ppu::REG_DISPCNT, 3 | ppu::dispcnt::BG2_ENABLE);
    bus.write16(ppu::REG_BG2PA, 0x0100);
    bus.write16(ppu::REG_BG2PD, 0x0100);
    bus.write16(0x0600_0000, color);
    bus.step(CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME);
}

#[test]
fn gba_save_state_restore_returns_screen_and_state_to_saved_point() {
    let mut gba = make_gba();
    let rom = minimal_valid_gba_rom();
    gba.load_rom(&rom, "synthetic.gba").expect("valid GBA ROM");

    render_mode3_frame(&mut gba, 0x001F);
    gba.bus_mut().write32(0x0200_0100, 0x1234_5678);
    gba.bus_mut().write16(0x0400_0200, 0x0001);
    let saved_pc = gba.cpu_pc();
    let saved_crc = gba.screen_crc32();
    let saved = gba.save_state();

    render_mode3_frame(&mut gba, 0x7C00);
    gba.bus_mut().write32(0x0200_0100, 0xDEAD_BEEF);
    gba.bus_mut().write16(0x0400_0200, 0x0040);
    for _ in 0..16 {
        gba.run_tick_for_tests();
    }

    assert_ne!(
        gba.screen_crc32(),
        saved_crc,
        "test must dirty screen output"
    );
    assert_ne!(gba.cpu_pc(), saved_pc, "test must dirty CPU state");
    assert_eq!(gba.bus_mut().read32(0x0200_0100), 0xDEAD_BEEF);

    gba.load_state(&saved).expect("restore succeeds");

    assert_eq!(gba.screen_crc32(), saved_crc);
    assert_eq!(gba.cpu_pc(), saved_pc);
    assert_eq!(gba.bus_mut().read32(0x0200_0100), 0x1234_5678);
    assert_eq!(gba.bus_mut().read16(0x0400_0200), 0x0001);
}

#[test]
fn gba_save_state_rejects_incompatible_version_without_crashing() {
    let mut gba = make_gba();
    let mut saved = gba.save_state();
    saved.version = GBA_SAVESTATE_VERSION + 1;

    let result = gba.load_state(&saved);

    assert!(matches!(
        result,
        Err(GbaSaveStateError::IncompatibleVersion { .. })
    ));
}
