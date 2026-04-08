//! Integration tests for RAM initialization modes.

#![cfg(test)]

use crate::nes::cartridge::Cartridge;
use crate::nes::console::{Config, Nes, RamInitMode};

/// Helper to create a minimal NROM ROM (mapper 0) for testing.
fn create_test_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 16 + 16 * 1024]; // Header + 16KB PRG ROM

    // iNES header
    rom[0..4].copy_from_slice(b"NES\x1A");
    rom[4] = 1; // 1 * 16KB PRG ROM
    rom[5] = 0; // CHR RAM (no CHR ROM)
    rom[6] = 0x00; // Mapper 0, horizontal mirroring
    rom[7] = 0x00; // Mapper 0 (upper bits)

    rom
}

fn load_test_cartridge(rom_data: &[u8], rom_name: &str) -> Cartridge {
    Cartridge::load_from_file(rom_data, rom_name, crate::app_context::AppContext::new())
        .expect("Failed to create cartridge")
}

#[test]
fn test_cpu_ram_initialization_zero_mode() {
    let mut config = Config::with_defaults();
    config.nes.ram_init_mode = RamInitMode::Zero;
    let nes = Nes::new(crate::app_context::AppContext::new_with_config(config));

    // Check that CPU RAM is initialized to zero (first 2KB mirrored 4 times)
    let bus = nes.bus().borrow();
    let ram = bus.cpu_ram_ref();
    let ram_data = ram.borrow();

    // Check first 2KB
    for i in 0..0x800 {
        assert_eq!(ram_data[i], 0x00, "CPU RAM[{:#04X}] should be 0x00", i);
    }
}

#[test]
fn test_cpu_ram_initialization_seeded_random_mode_deterministic() {
    let mut config1 = Config::with_defaults();
    config1.nes.ram_init_mode = RamInitMode::SeededRandom(42);
    let nes1 = Nes::new(crate::app_context::AppContext::new_with_config(config1));

    let mut config2 = Config::with_defaults();
    config2.nes.ram_init_mode = RamInitMode::SeededRandom(42);
    let nes2 = Nes::new(crate::app_context::AppContext::new_with_config(config2));

    // Check that both have identical RAM
    let bus1 = nes1.bus().borrow();
    let ram1 = bus1.cpu_ram_ref();
    let ram1_data = ram1.borrow();

    let bus2 = nes2.bus().borrow();
    let ram2 = bus2.cpu_ram_ref();
    let ram2_data = ram2.borrow();

    for i in 0..0x800 {
        assert_eq!(
            ram1_data[i], ram2_data[i],
            "CPU RAM[{:#04X}] should match with same seed",
            i
        );
    }
}

#[test]
fn test_cpu_ram_initialization_different_seeds_produce_different_values() {
    let mut config1 = Config::with_defaults();
    config1.nes.ram_init_mode = RamInitMode::SeededRandom(42);
    let nes1 = Nes::new(crate::app_context::AppContext::new_with_config(config1));

    let mut config2 = Config::with_defaults();
    config2.nes.ram_init_mode = RamInitMode::SeededRandom(43);
    let nes2 = Nes::new(crate::app_context::AppContext::new_with_config(config2));

    let bus1 = nes1.bus().borrow();
    let ram1 = bus1.cpu_ram_ref();
    let ram1_data = ram1.borrow();

    let bus2 = nes2.bus().borrow();
    let ram2 = bus2.cpu_ram_ref();
    let ram2_data = ram2.borrow();

    // At least some bytes should be different
    let mut differences = 0;
    for i in 0..0x800 {
        if ram1_data[i] != ram2_data[i] {
            differences += 1;
        }
    }

    assert!(
        differences > 100,
        "Different seeds should produce many different values (got {} differences)",
        differences
    );
}

#[test]
fn test_ppu_ram_initialization_zero_mode() {
    let mut config = Config::with_defaults();
    config.nes.ram_init_mode = RamInitMode::Zero;
    let nes = Nes::new(crate::app_context::AppContext::new_with_config(config));

    // Check nametable RAM (0x2000-0x2FFF range)
    for addr in 0x2000..0x3000 {
        assert_eq!(
            nes.ppu().borrow().read_nametable_for_debug(addr),
            0x00,
            "PPU nametable[{:#04X}] should be 0x00",
            addr
        );
    }
}

#[test]
fn test_ppu_ram_initialization_seeded_random_deterministic() {
    let mut config1 = Config::with_defaults();
    config1.nes.ram_init_mode = RamInitMode::SeededRandom(42);
    let nes1 = Nes::new(crate::app_context::AppContext::new_with_config(config1));

    let mut config2 = Config::with_defaults();
    config2.nes.ram_init_mode = RamInitMode::SeededRandom(42);
    let nes2 = Nes::new(crate::app_context::AppContext::new_with_config(config2));

    // Check that nametable RAM is identical
    for addr in 0x2000..0x3000 {
        assert_eq!(
            nes1.ppu().borrow().read_nametable_for_debug(addr),
            nes2.ppu().borrow().read_nametable_for_debug(addr),
            "PPU nametable[{:#04X}] should match with same seed",
            addr
        );
    }
}

#[test]
fn test_cartridge_ram_initialization_zero_mode() {
    let mut config = Config::with_defaults();
    config.nes.ram_init_mode = RamInitMode::Zero;
    let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(config));

    let rom_data = create_test_rom();
    let cartridge = load_test_cartridge(&rom_data, "ram-init-zero.nes");
    nes.insert_cartridge(cartridge);

    let mut bus = nes.bus().borrow_mut();
    // Check PRG-RAM at $6000-$7FFF
    for addr in 0x6000..=0x7FFF {
        assert_eq!(
            bus.read(addr, false),
            0x00,
            "PRG-RAM[{:#04X}] should be 0x00",
            addr
        );
    }
}

#[test]
fn test_cartridge_ram_initialization_seeded_random_deterministic() {
    let mut config1 = Config::with_defaults();
    config1.nes.ram_init_mode = RamInitMode::SeededRandom(42);
    let mut nes1 = Nes::new(crate::app_context::AppContext::new_with_config(config1));

    let rom_data1 = create_test_rom();
    let cartridge1 = load_test_cartridge(&rom_data1, "ram-init-seeded-a.nes");
    nes1.insert_cartridge(cartridge1);

    let mut config2 = Config::with_defaults();
    config2.nes.ram_init_mode = RamInitMode::SeededRandom(42);
    let mut nes2 = Nes::new(crate::app_context::AppContext::new_with_config(config2));

    let rom_data2 = create_test_rom();
    let cartridge2 = load_test_cartridge(&rom_data2, "ram-init-seeded-b.nes");
    nes2.insert_cartridge(cartridge2);

    let mut bus1 = nes1.bus().borrow_mut();
    let mut bus2 = nes2.bus().borrow_mut();

    // Check PRG-RAM at $6000-$7FFF
    for addr in 0x6000..=0x6100 {
        assert_eq!(
            bus1.read(addr, false),
            bus2.read(addr, false),
            "PRG-RAM[{:#04X}] should match with same seed",
            addr
        );
    }
}

#[test]
fn test_hard_reset_reinitializes_ram() {
    let mut config = Config::with_defaults();
    config.nes.ram_init_mode = RamInitMode::Zero;
    let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(config));

    // Insert a cartridge so reset works
    let rom_data = create_test_rom();
    let cartridge = load_test_cartridge(&rom_data, "ram-init-hard-reset.nes");
    nes.insert_cartridge(cartridge);

    // Write some non-zero data to RAM
    {
        let bus = nes.bus().borrow();
        let ram_rc = bus.cpu_ram_ref();
        let mut ram = ram_rc.borrow_mut();
        ram[0x100] = 0xFF;
        ram[0x200] = 0xAA;
    }

    // Hard reset (soft_reset = false) should re-initialize RAM to zero
    nes.reset(false);

    let bus = nes.bus().borrow();
    let ram_rc = bus.cpu_ram_ref();
    let ram = ram_rc.borrow();
    assert_eq!(ram[0x100], 0x00, "Hard reset should zero RAM[0x100]");
    assert_eq!(ram[0x200], 0x00, "Hard reset should zero RAM[0x200]");
}

#[test]
fn test_soft_reset_preserves_ram() {
    let mut config = Config::with_defaults();
    config.nes.ram_init_mode = RamInitMode::Zero;
    let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(config));

    // Insert a cartridge so reset works
    let rom_data = create_test_rom();
    let cartridge = load_test_cartridge(&rom_data, "ram-init-soft-reset.nes");
    nes.insert_cartridge(cartridge);

    // Write some non-zero data to RAM
    {
        let bus = nes.bus().borrow();
        let ram_rc = bus.cpu_ram_ref();
        let mut ram = ram_rc.borrow_mut();
        ram[0x100] = 0xFF;
        ram[0x200] = 0xAA;
    }

    // Soft reset (soft_reset = true) should preserve RAM
    nes.reset(true);

    let bus = nes.bus().borrow();
    let ram_rc = bus.cpu_ram_ref();
    let ram = ram_rc.borrow();
    assert_eq!(ram[0x100], 0xFF, "Soft reset should preserve RAM[0x100]");
    assert_eq!(ram[0x200], 0xAA, "Soft reset should preserve RAM[0x200]");
}

#[test]
fn test_ppu_hard_reset_reinitializes_ram() {
    let mut config = Config::with_defaults();
    config.nes.ram_init_mode = RamInitMode::Zero;
    let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(config));

    // Insert a cartridge so reset works
    let rom_data = create_test_rom();
    let cartridge = load_test_cartridge(&rom_data, "ram-init-ppu-hard-reset.nes");
    nes.insert_cartridge(cartridge);

    // Write some non-zero data to PPU nametable
    {
        let mut bus = nes.bus().borrow_mut();
        // Write via PPU registers
        bus.write(0x2006, 0x20, false); // PPUADDR high
        bus.write(0x2006, 0x00, false); // PPUADDR low
        bus.write(0x2007, 0xFF, false); // PPUDATA
    }

    // Hard reset should re-initialize PPU RAM to zero
    nes.reset(false);

    assert_eq!(
        nes.ppu().borrow().read_nametable_for_debug(0x2000),
        0x00,
        "Hard reset should zero PPU nametable"
    );
}

#[test]
fn test_ppu_soft_reset_preserves_ram() {
    let mut config = Config::with_defaults();
    config.nes.ram_init_mode = RamInitMode::Zero;
    let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(config));

    // Insert a cartridge so reset works
    let rom_data = create_test_rom();
    let cartridge = load_test_cartridge(&rom_data, "ram-init-ppu-soft-reset.nes");
    nes.insert_cartridge(cartridge);

    // Write some non-zero data to PPU nametable
    {
        let mut bus = nes.bus().borrow_mut();
        bus.write(0x2006, 0x20, false); // PPUADDR high
        bus.write(0x2006, 0x00, false); // PPUADDR low
        bus.write(0x2007, 0xFF, false); // PPUDATA
    }

    // Soft reset should preserve PPU RAM
    nes.reset(true);

    assert_eq!(
        nes.ppu().borrow().read_nametable_for_debug(0x2000),
        0xFF,
        "Soft reset should preserve PPU nametable"
    );
}

#[test]
fn test_ppu_palette_ram_init_zero_mode() {
    // Verify that PPU palette RAM ($3F00-$3F1F) is initialized according to
    // the configured RAM init mode, using the PPU register interface.
    let mut config = Config::with_defaults();
    config.nes.ram_init_mode = RamInitMode::Zero;
    let nes = Nes::new(crate::app_context::AppContext::new_with_config(config));

    // Access palette RAM via PPUADDR ($2006) and PPUDATA ($2007).
    // For Zero mode, all palette entries should read back as 0x00.
    {
        let mut bus = nes.bus().borrow_mut();

        for offset in 0u16..32 {
            let addr = 0x3F00u16 + offset;
            let high = (addr >> 8) as u8;
            let low = (addr & 0x00FF) as u8;

            // Set PPU address to the palette entry.
            bus.write(0x2006, high, false);
            bus.write(0x2006, low, false);

            let value = bus.read(0x2007, false);
            assert_eq!(
                value, 0x00,
                "Palette RAM at ${:04X} should be initialized to 0x00 in Zero mode",
                addr
            );
        }
    }
}

#[test]
fn test_oam_ram_initialization_zero_mode() {
    let mut config = Config::with_defaults();
    config.nes.ram_init_mode = RamInitMode::Zero;
    let nes = Nes::new(crate::app_context::AppContext::new_with_config(config));

    // Access OAM via OAMDATA ($2004) after setting OAMADDR ($2003)
    {
        let mut bus = nes.bus().borrow_mut();

        // Check all 256 bytes of OAM
        for addr in 0u8..=255 {
            bus.write(0x2003, addr, false); // OAMADDR
            let value = bus.read(0x2004, false); // OAMDATA
            assert_eq!(
                value, 0x00,
                "OAM[{:#04X}] should be initialized to 0x00 in Zero mode",
                addr
            );
        }
    }
}

#[test]
fn test_oam_ram_initialization_seeded_random_deterministic() {
    let mut config1 = Config::with_defaults();
    config1.nes.ram_init_mode = RamInitMode::SeededRandom(42);
    let nes1 = Nes::new(crate::app_context::AppContext::new_with_config(config1));

    let mut config2 = Config::with_defaults();
    config2.nes.ram_init_mode = RamInitMode::SeededRandom(42);
    let nes2 = Nes::new(crate::app_context::AppContext::new_with_config(config2));

    // OAM should be identical for same seed
    {
        let mut bus1 = nes1.bus().borrow_mut();
        let mut bus2 = nes2.bus().borrow_mut();

        // Sample OAM values at various addresses
        for addr in [0u8, 1, 10, 50, 100, 200, 255] {
            bus1.write(0x2003, addr, false);
            bus2.write(0x2003, addr, false);

            let value1 = bus1.read(0x2004, false);
            let value2 = bus2.read(0x2004, false);

            assert_eq!(
                value1, value2,
                "OAM[{:#04X}] should match with same seed",
                addr
            );
        }
    }
}

#[test]
fn test_oam_hard_reset_reinitializes() {
    let mut config = Config::with_defaults();
    config.nes.ram_init_mode = RamInitMode::Zero;
    let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(config));

    // Insert a cartridge so reset works
    let rom_data = create_test_rom();
    let cartridge = load_test_cartridge(&rom_data, "ram-init-oam-hard-reset.nes");
    nes.insert_cartridge(cartridge);

    // Write some non-zero data to OAM
    {
        let mut bus = nes.bus().borrow_mut();
        bus.write(0x2003, 5, false); // OAMADDR = 5
        bus.write(0x2004, 0xFF, false); // OAMDATA = 0xFF (writes to addr 5, increments to 6)
        bus.write(0x2003, 20, false); // OAMADDR = 20
        bus.write(0x2004, 0xAA, false); // OAMDATA = 0xAA (writes to addr 20, increments to 21)
    }

    // Hard reset should re-initialize OAM to zero
    nes.reset(false);

    {
        let mut bus = nes.bus().borrow_mut();
        bus.write(0x2003, 5, false);
        assert_eq!(
            bus.read(0x2004, false),
            0x00,
            "Hard reset should zero OAM[5]"
        );
        bus.write(0x2003, 20, false);
        assert_eq!(
            bus.read(0x2004, false),
            0x00,
            "Hard reset should zero OAM[20]"
        );
    }
}

#[test]
fn test_oam_soft_reset_preserves() {
    let mut config = Config::with_defaults();
    config.nes.ram_init_mode = RamInitMode::Zero;
    let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(config));

    // Insert a cartridge so reset works
    let rom_data = create_test_rom();
    let cartridge = load_test_cartridge(&rom_data, "ram-init-oam-soft-reset.nes");
    nes.insert_cartridge(cartridge);

    // Write some non-zero data to OAM
    {
        let mut bus = nes.bus().borrow_mut();
        bus.write(0x2003, 5, false); // OAMADDR = 5
        bus.write(0x2004, 0xFF, false); // OAMDATA = 0xFF (writes to addr 5, increments to 6)
        bus.write(0x2003, 20, false); // OAMADDR = 20
        bus.write(0x2004, 0xAA, false); // OAMDATA = 0xAA (writes to addr 20, increments to 21)
    }

    // Soft reset should preserve OAM
    nes.reset(true);

    {
        let mut bus = nes.bus().borrow_mut();
        bus.write(0x2003, 5, false);
        assert_eq!(
            bus.read(0x2004, false),
            0xFF,
            "Soft reset should preserve OAM[5]"
        );
        bus.write(0x2003, 20, false);
        assert_eq!(
            bus.read(0x2004, false),
            0xAA,
            "Soft reset should preserve OAM[20]"
        );
    }
}
