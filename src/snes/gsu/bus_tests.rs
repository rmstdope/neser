//! The Super FX as the S-CPU sees it through [`SnesSystemBus`]: the cartridge memory map, the
//! GSU register file at `$3000-$34FF`, and the hand-over of ROM/RAM while the GSU runs.
//!
//! Expectations follow fullsnes "SNES Cart GSU-n Memory Map", "I/O Map" and "General I/O Ports".

use crate::snes::bus::{SnesBus, SnesSystemBus};
use crate::snes::cartridge::Cartridge;

/// Builds a 128 KB LoROM Super FX cartridge (chipset `$14`, 32 KB Game Pak RAM declared in the
/// extended header) whose ROM holds `fill(offset)` at every byte outside the header.
pub(super) fn gsu_cart_rom(fill: impl Fn(usize) -> u8) -> Vec<u8> {
    sized_gsu_cart_rom(0x2_0000, 0x05, fill)
}

/// Builds a 2 MB LoROM Super FX cartridge, the size fullsnes gives for GSU2 games ("Games with
/// 2MByte ROM are typically using GSU2"), declaring `ram_size_code` (`1 << n` KB) as its Game Pak
/// RAM.
pub(super) fn gsu2_cart_rom(ram_size_code: u8, fill: impl Fn(usize) -> u8) -> Vec<u8> {
    sized_gsu_cart_rom(0x20_0000, ram_size_code, fill)
}

fn sized_gsu_cart_rom(len: usize, ram_size_code: u8, fill: impl Fn(usize) -> u8) -> Vec<u8> {
    let mut rom: Vec<u8> = (0..len).map(fill).collect();
    let base = 0x7FC0;
    rom[0x7FB0..0x8000].fill(0);
    rom[base..base + 21].copy_from_slice(b"GSU BUS TEST         ");
    rom[base + 0x15] = 0x20; // Slow LoROM.
    rom[base + 0x16] = 0x14; // Chipset: GSU + RAM.
    // ROM size, `1 << n` KB (fullsnes "Cartridge Header"): $07 = 128 KB, $0B = 2 MB.
    rom[base + 0x17] = (len >> 10).trailing_zeros() as u8;
    rom[base + 0x1A] = 0x33; // Extended header present.
    rom[0x7FBD] = ram_size_code; // Expansion RAM size.
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
fn gsu2_cart_maps_the_second_megabyte() {
    // fullsnes "GSU2 Memory Map (at SNES Side)": "Game Pak ROM in LoRom mapping (2Mbyte max)" and
    // its HiROM mirror. ROM `$1F_8123` is LoROM `$3F:8123` and HiROM `$5F:8123`; the `$80`/`$C0`
    // views are the mirrors NESER keeps for every Super FX cartridge (as Mesen2).
    let mut rom = gsu2_cart_rom(0x05, |_| 0);
    rom[0x1F_8123] = 0xC4;
    let bus = gsu_bus(&rom);
    for addr in [0x3F_8123, 0xBF_8123, 0x5F_8123, 0xDF_8123] {
        bus.read(0x00_8000); // `$00` on the bus, so open bus cannot pass.
        assert_eq!(bus.read(addr), 0xC4, "${addr:06X}");
    }
}

#[test]
fn gsu2_cart_128kb_ram_reaches_bank_71() {
    // fullsnes: "70-71:0000-FFFF Game Pak RAM (128Kbyte max ...)"; expansion RAM code `$07`.
    let mut bus = gsu_bus(&gsu2_cart_rom(0x07, |_| 0));
    bus.write(0x70_4321, 0x11);
    bus.write(0x71_4321, 0x22);
    assert_eq!(bus.read(0x70_4321), 0x11);
    assert_eq!(bus.read(0x71_4321), 0x22);
    assert_eq!(bus.read(0xF1_4321), 0x22, "mirror at $F1");
}

#[test]
fn gsu_cart_ram_at_70_and_first_8k_mirror_at_6000() {
    let mut bus = gsu_bus(&gsu_cart_rom(|_| 0));

    bus.write(0x70_0010, 0x5A);
    assert_eq!(
        bus.read(0x00_6010),
        0x5A,
        "$00:6000-7FFF mirrors the first 8 KB"
    );
    assert_eq!(bus.read(0xF0_0010), 0x5A, "$F0 mirrors $70");

    bus.write(0xBF_7FFF, 0xC3);
    assert_eq!(bus.read(0x70_1FFF), 0xC3);

    bus.write(0x70_7FFF, 0x99);
    assert_eq!(
        bus.read(0x71_7FFF),
        0x99,
        "32 KB of RAM mirrors through bank $71"
    );
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
fn gsu2_cart_version_code_reads_04() {
    // fullsnes "General I/O Ports", VCR: "4=GSU2"; at `$303B` and its mirror `$333B`.
    let bus = gsu_bus(&gsu2_cart_rom(0x05, |_| 0));
    assert_eq!(bus.read(0x00_303B), 0x04);
    assert_eq!(bus.read(0x80_333B), 0x04);
}

#[test]
fn gsu2_status_registers_mirror_at_3020() {
    // fullsnes "Full I/O Map with Mirrors for GSU2 (VCR=04h)": "3020h..302Fh mirror of
    // 3030h..303Fh".
    let mut rom = gsu2_cart_rom(0x05, |_| 0);
    rom[..3].copy_from_slice(&[0x05, 0xFE, 0x01]); // BRA to itself; NOP.
    let mut bus = gsu_bus(&rom);
    assert_eq!(bus.read(0x00_302B), 0x04, "VCR");
    bus.write(0x00_3034, 0x5A);
    assert_eq!(bus.read(0x00_3024), 0x5A, "PBR");
    bus.write(0x00_3024, 0x00); // PBR back to bank 0, through the mirror.
    assert_eq!(bus.read(0x00_3034), 0x00, "PBR written via $3024");

    bus.write(0x00_301E, 0x00);
    bus.write(0x00_301F, 0x80); // GO
    assert_eq!(bus.read(0x00_3020) & 0x20, 0x20, "SFR GO via $3020");
    bus.write(0x00_3020, 0x00); // GO=0 through the mirror stops the GSU.
    assert_eq!(bus.read(0x00_3030) & 0x20, 0x00, "stopped");
}

#[test]
fn gsu1_cart_3020_reads_zero_and_ignores_writes() {
    // nr-hab.1's register map for the GSU-1 (Mesen2's decode): `$3020-$302F` is unused.
    let mut bus = gsu_bus(&gsu_cart_rom(|_| 0));
    bus.write(0x00_3034, 0x5A);
    assert_eq!(bus.read(0x00_3024), 0x00);
    bus.write(0x00_3024, 0x11);
    assert_eq!(bus.read(0x00_3034), 0x5A);
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

/// A GSU cart whose program at `$00:8000` loops on itself (BRA to itself; NOP in the delay
/// slot), started with both buses handed to the GSU.
fn running_gsu_bus() -> SnesSystemBus {
    let mut rom = gsu_cart_rom(|_| 0);
    rom[..3].copy_from_slice(&[0x05, 0xFE, 0x01]);
    let mut bus = gsu_bus(&rom);
    bus.write(0x00_303A, 0x18); // RON | RAN
    bus.write(0x00_301E, 0x00);
    bus.write(0x00_301F, 0x80); // GO
    for _ in 0..100 {
        bus.tick();
    }
    bus
}

#[test]
fn snes_reads_fixed_vectors_from_rom_while_gsu_runs_with_ron() {
    let mut rom = gsu_cart_rom(|_| 0xEE);
    rom[..3].copy_from_slice(&[0x05, 0xFE, 0x01]);
    rom[0x7FEA] = 0xEE; // The fixture zeroes the header; give the NMI vector a visible value.
    let mut bus = gsu_bus(&rom);
    assert_eq!(bus.read(0x00_FFEA), 0xEE, "real ROM before the GSU starts");
    bus.write(0x00_303A, 0x18);
    bus.write(0x00_301E, 0x00);
    bus.write(0x00_301F, 0x80);
    for (addr, value) in [
        (0x00_FFEA, 0x08), // NMI -> $0108
        (0x00_FFEB, 0x01),
        (0x00_FFEE, 0x0C), // IRQ -> $010C
        (0x00_FFE4, 0x04), // COP -> $0104
        (0x00_FFFC, 0x00), // anything else -> $0100
        (0x41_1234, 0x04), // the HiROM view too
    ] {
        assert_eq!(bus.read(addr), value, "${addr:06X}");
    }
    bus.write(0x00_3030, 0x00); // Clear GO.
    assert_eq!(bus.read(0x00_FFEA), 0xEE);
}

#[test]
fn snes_ram_reads_open_bus_while_gsu_runs_with_ran() {
    let mut bus = gsu_bus(&gsu_cart_rom(|_| 0));
    bus.write(0x70_0000, 0x5A);
    bus.write(0x00_303A, 0x08); // RAN (ROM stays with the S-CPU)
    bus.write(0x00_301E, 0x00);
    bus.write(0x00_301F, 0x80);
    bus.read(0x00_8000); // Leaves $00 on the bus.
    assert_eq!(bus.read(0x70_0000), 0x00, "open bus, not the RAM's $5A");
    bus.write(0x70_0000, 0x77);
    bus.write(0x00_3030, 0x00);
    assert_eq!(
        bus.read(0x70_0000),
        0x5A,
        "the write while blocked was dropped"
    );
}

#[test]
fn register_writes_other_than_sfr_scmr_ignored_while_running() {
    let mut bus = running_gsu_bus();
    bus.write(0x00_3002, 0x34);
    bus.write(0x00_3003, 0x12);
    assert_eq!(bus.read(0x00_3002), 0x00);
    // SCMR still takes writes (to hand the buses back) and SFR can stop the GSU.
    bus.write(0x00_3030, 0x00);
    assert_eq!(bus.read(0x00_3030) & 0x20, 0);
}

#[test]
fn gsu_irq_reaches_snes_cpu_via_poll_irq() {
    let mut rom = gsu_cart_rom(|_| 0);
    rom[..2].copy_from_slice(&[0x00, 0x01]); // STOP; NOP
    let mut bus = gsu_bus(&rom);
    bus.write(0x00_303A, 0x18);
    bus.write(0x00_301E, 0x00);
    bus.write(0x00_301F, 0x80);
    let mut ticks = 0;
    while !bus.poll_irq() {
        bus.tick();
        ticks += 1;
        assert!(
            ticks < 1000,
            "the GSU's STOP never raised the S-CPU IRQ line"
        );
    }
    assert_ne!(bus.read(0x00_3031) & 0x80, 0);
    assert!(!bus.poll_irq(), "reading $3031 acknowledges");
}

/// A GSU cart whose program increments R1 1000 times in a LOOP and STOPs.
fn counting_gsu_rom() -> Vec<u8> {
    let mut rom = gsu_cart_rom(|_| 0);
    #[rustfmt::skip]
    rom[..11].copy_from_slice(&[
        0xFC, 0xE8, 0x03, // IWT R12,#1000
        0x2F, 0x1D,       // MOVE R13,R15
        0xD1,             // INC R1
        0x3C,             // LOOP
        0x01,             // delay slot
        0x00, 0x01,       // STOP; NOP
        0x01,
    ]);
    rom
}

fn start_gsu(bus: &mut SnesSystemBus) {
    bus.write(0x00_3039, 0x00); // 10.7 MHz
    bus.write(0x00_303A, 0x18);
    bus.write(0x00_301E, 0x00);
    bus.write(0x00_301F, 0x80);
}

fn read_r1(bus: &SnesSystemBus) -> u16 {
    u16::from_le_bytes([bus.read(0x00_3002), bus.read(0x00_3003)])
}

#[test]
fn gsu_state_round_trips_through_save_state() {
    let rom = counting_gsu_rom();
    let mut original = gsu_bus(&rom);
    start_gsu(&mut original);
    for _ in 0..5_000 {
        original.tick();
    }
    let saved = original.capture_state();
    assert!(saved.gsu.is_some());
    for _ in 0..5_000 {
        original.tick();
    }

    let mut restored = gsu_bus(&rom);
    restored.restore_state(&saved).expect("restore");
    for _ in 0..5_000 {
        restored.tick();
    }
    let (r1, running) = (read_r1(&restored), restored.read(0x00_3030) & 0x20);
    assert_eq!(
        (r1, running),
        (read_r1(&original), original.read(0x00_3030) & 0x20)
    );
    assert!(r1 > 0 && r1 < 1000, "caught mid-loop, got R1 = {r1}");
}

#[test]
fn reset_stops_the_gsu() {
    let mut bus = gsu_bus(&counting_gsu_rom());
    start_gsu(&mut bus);
    for _ in 0..1_000 {
        bus.tick();
    }
    bus.reset_gsu();
    assert_eq!(bus.read(0x00_3030) & 0x20, 0, "GO cleared");
    assert_eq!(read_r1(&bus), 0, "registers back to power-on");
}

#[test]
fn bus_state_without_a_gsu_section_still_loads() {
    // A state saved before Super FX support has no `gsu` key; it must load and leave the GSU
    // as it is.
    let mut bus = gsu_bus(&counting_gsu_rom());
    let mut json = serde_json::to_value(bus.capture_state()).expect("serialize");
    json.as_object_mut().expect("object").remove("gsu");
    let state: crate::snes::console::save_state::SnesBusState =
        serde_json::from_value(json).expect("deserialize");
    assert!(state.gsu.is_none());
    bus.restore_state(&state).expect("restore");
}
