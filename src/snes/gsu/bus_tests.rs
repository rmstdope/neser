//! The Super FX as the S-CPU sees it through [`SnesSystemBus`]: the cartridge memory map, the
//! GSU register file at `$3000-$34FF`, and the hand-over of ROM/RAM while the GSU runs.
//!
//! Expectations follow fullsnes "SNES Cart GSU-n Memory Map", "I/O Map" and "General I/O Ports".

use crate::snes::bus::{SnesBus, SnesSystemBus};
use crate::snes::cartridge::Cartridge;

/// Builds a 128 KB LoROM Super FX cartridge (chipset `$14`, 32 KB Game Pak RAM declared in the
/// extended header) whose ROM holds `fill(offset)` at every byte outside the header.
pub(super) fn gsu_cart_rom(fill: impl Fn(usize) -> u8) -> Vec<u8> {
    let mut rom: Vec<u8> = (0..0x2_0000).map(fill).collect();
    let base = 0x7FC0;
    rom[0x7FB0..0x8000].fill(0);
    rom[base..base + 21].copy_from_slice(b"GSU BUS TEST         ");
    rom[base + 0x15] = 0x20; // Slow LoROM.
    rom[base + 0x16] = 0x14; // Chipset: GSU + RAM.
    rom[base + 0x17] = 0x07; // 128 KB.
    rom[base + 0x1A] = 0x33; // Extended header present.
    rom[0x7FBD] = 0x05; // Expansion RAM: 32 KB.
    rom[base + 0x3C] = 0x00; // Reset vector $8000.
    rom[base + 0x3D] = 0x80;
    rom
}

pub(super) fn gsu_bus(rom: &[u8]) -> SnesSystemBus {
    SnesSystemBus::new(Cartridge::from_bytes(rom).expect("valid Super FX cartridge"))
}

#[test]
fn gsu_cart_maps_lorom_and_hirom_views_of_rom() {
    let mut rom = gsu_cart_rom(|_| 0);
    rom[0x1_8123] = 0xAB; // LoROM bank 3, offset $0123.
    let bus = gsu_bus(&rom);

    // Each probe first reads a `$00` ROM byte so open bus cannot echo the previous `$AB`.
    let probe = |addr: u32| {
        bus.read(0x00_8000);
        bus.read(addr)
    };
    for addr in [0x03_8123, 0x83_8123] {
        assert_eq!(probe(addr), 0xAB, "LoROM view at ${addr:06X}");
    }
    // HiROM view: banks $40-$5F (and $C0-$DF) are linear, so $41:8123 is ROM $1_8123.
    for addr in [0x41_8123, 0xC1_8123] {
        assert_eq!(probe(addr), 0xAB, "HiROM view at ${addr:06X}");
    }
}

#[test]
fn gsu_cart_ram_at_70_and_first_8k_mirror_at_6000() {
    let mut bus = gsu_bus(&gsu_cart_rom(|_| 0));

    bus.write(0x70_0010, 0x5A);
    assert_eq!(bus.read(0x00_6010), 0x5A, "$00:6000-7FFF mirrors the first 8 KB");
    assert_eq!(bus.read(0xF0_0010), 0x5A, "$F0 mirrors $70");

    bus.write(0xBF_7FFF, 0xC3);
    assert_eq!(bus.read(0x70_1FFF), 0xC3);

    bus.write(0x70_7FFF, 0x99);
    assert_eq!(bus.read(0x71_7FFF), 0x99, "32 KB of RAM mirrors through bank $71");
}

#[test]
fn gsu_register_write_latches_low_byte_until_odd_write() {
    let mut bus = gsu_bus(&gsu_cart_rom(|_| 0));

    bus.write(0x00_3002, 0x34);
    assert_eq!(bus.read(0x00_3002), 0x00, "an even write only latches");
    bus.write(0x00_3003, 0x12);
    assert_eq!(bus.read(0x00_3002), 0x34);
    assert_eq!(bus.read(0x00_3003), 0x12);
    // $3040-$305F mirrors R0-R15, and so does $3300.
    assert_eq!(bus.read(0x00_3042), 0x34);
    assert_eq!(bus.read(0x80_3302), 0x34);
}

#[test]
fn gsu_version_code_register_reads_gsu1() {
    let bus = gsu_bus(&gsu_cart_rom(|_| 0));
    assert_eq!(bus.read(0x00_303B), 0x01);
}

#[test]
fn gsu_cache_window_reads_back_snes_writes() {
    let mut bus = gsu_bus(&gsu_cart_rom(|_| 0));
    bus.write(0x00_3100, 0x77);
    bus.write(0x00_32FF, 0x66);
    assert_eq!(bus.read(0x00_3100), 0x77);
    assert_eq!(bus.read(0x00_32FF), 0x66);
}

#[test]
fn non_gsu_cart_leaves_3000_open_bus() {
    let mut rom = gsu_cart_rom(|_| 0);
    rom[0x7FC0 + 0x16] = 0x00; // Plain ROM.
    let bus = gsu_bus(&rom);
    bus.read(0x00_8000); // Puts $00 on the bus.
    let open_bus = bus.read(0x00_8000);
    assert_eq!(bus.read(0x00_303B), open_bus);
}
