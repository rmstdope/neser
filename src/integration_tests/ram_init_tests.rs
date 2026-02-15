//! Integration tests for RAM initialization modes.

use crate::console::{Config, Nes, RamInitMode};
use crate::cartridge::Cartridge;

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

#[test]
fn test_cpu_ram_initialization_zero_mode() {
    let mut config = Config::with_defaults();
    config.ram_init_mode = RamInitMode::Zero;
    let nes = Nes::new(config);
    
    // Check that CPU RAM is initialized to zero (first 2KB mirrored 4 times)
    let bus = nes.bus.borrow();
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
    config1.ram_init_mode = RamInitMode::SeededRandom(42);
    let nes1 = Nes::new(config1);
    
    let mut config2 = Config::with_defaults();
    config2.ram_init_mode = RamInitMode::SeededRandom(42);
    let nes2 = Nes::new(config2);
    
    // Check that both have identical RAM
    let bus1 = nes1.bus.borrow();
    let ram1 = bus1.cpu_ram_ref();
    let ram1_data = ram1.borrow();
    
    let bus2 = nes2.bus.borrow();
    let ram2 = bus2.cpu_ram_ref();
    let ram2_data = ram2.borrow();
    
    for i in 0..0x800 {
        assert_eq!(
            ram1_data[i], ram2_data[i],
            "CPU RAM[{:#04X}] should match with same seed", i
        );
    }
}

#[test]
fn test_cpu_ram_initialization_different_seeds_produce_different_values() {
    let mut config1 = Config::with_defaults();
    config1.ram_init_mode = RamInitMode::SeededRandom(42);
    let nes1 = Nes::new(config1);
    
    let mut config2 = Config::with_defaults();
    config2.ram_init_mode = RamInitMode::SeededRandom(43);
    let nes2 = Nes::new(config2);
    
    let bus1 = nes1.bus.borrow();
    let ram1 = bus1.cpu_ram_ref();
    let ram1_data = ram1.borrow();
    
    let bus2 = nes2.bus.borrow();
    let ram2 = bus2.cpu_ram_ref();
    let ram2_data = ram2.borrow();
    
    // At least some bytes should be different
    let mut differences = 0;
    for i in 0..0x800 {
        if ram1_data[i] != ram2_data[i] {
            differences += 1;
        }
    }
    
    assert!(differences > 100, "Different seeds should produce many different values (got {} differences)", differences);
}

#[test]
fn test_ppu_ram_initialization_zero_mode() {
    let mut config = Config::with_defaults();
    config.ram_init_mode = RamInitMode::Zero;
    let nes = Nes::new(config);
    
    let ppu = nes.ppu.borrow();
    
    // Check nametable RAM (0x2000-0x2FFF range)
    for addr in 0x2000..0x3000 {
        assert_eq!(
            ppu.read_nametable_for_debug(addr), 0x00,
            "PPU nametable[{:#04X}] should be 0x00", addr
        );
    }
}

#[test]
fn test_ppu_ram_initialization_seeded_random_deterministic() {
    let mut config1 = Config::with_defaults();
    config1.ram_init_mode = RamInitMode::SeededRandom(42);
    let nes1 = Nes::new(config1);
    
    let mut config2 = Config::with_defaults();
    config2.ram_init_mode = RamInitMode::SeededRandom(42);
    let nes2 = Nes::new(config2);
    
    let ppu1 = nes1.ppu.borrow();
    let ppu2 = nes2.ppu.borrow();
    
    // Check that nametable RAM is identical
    for addr in 0x2000..0x3000 {
        assert_eq!(
            ppu1.read_nametable_for_debug(addr),
            ppu2.read_nametable_for_debug(addr),
            "PPU nametable[{:#04X}] should match with same seed", addr
        );
    }
}

#[test]
fn test_cartridge_ram_initialization_zero_mode() {
    let mut config = Config::with_defaults();
    config.ram_init_mode = RamInitMode::Zero;
    let mut nes = Nes::new(config);
    
    let rom_data = create_test_rom();
    let cartridge = Cartridge::new(&rom_data).expect("Failed to create cartridge");
    nes.insert_cartridge(cartridge);
    
    let mut bus = nes.bus.borrow_mut();
    // Check PRG-RAM at $6000-$7FFF
    for addr in 0x6000..=0x7FFF {
        assert_eq!(
            bus.read(addr, false), 0x00,
            "PRG-RAM[{:#04X}] should be 0x00", addr
        );
    }
}

#[test]
fn test_cartridge_ram_initialization_seeded_random_deterministic() {
    let mut config1 = Config::with_defaults();
    config1.ram_init_mode = RamInitMode::SeededRandom(42);
    let mut nes1 = Nes::new(config1);
    
    let rom_data1 = create_test_rom();
    let cartridge1 = Cartridge::new(&rom_data1).expect("Failed to create cartridge");
    nes1.insert_cartridge(cartridge1);
    
    let mut config2 = Config::with_defaults();
    config2.ram_init_mode = RamInitMode::SeededRandom(42);
    let mut nes2 = Nes::new(config2);
    
    let rom_data2 = create_test_rom();
    let cartridge2 = Cartridge::new(&rom_data2).expect("Failed to create cartridge");
    nes2.insert_cartridge(cartridge2);
    
    let mut bus1 = nes1.bus.borrow_mut();
    let mut bus2 = nes2.bus.borrow_mut();
    
    // Check PRG-RAM at $6000-$7FFF
    for addr in 0x6000..=0x6100 {
        assert_eq!(
            bus1.read(addr, false),
            bus2.read(addr, false),
            "PRG-RAM[{:#04X}] should match with same seed", addr
        );
    }
}

#[test]
fn test_hard_reset_reinitializes_ram() {
    let mut config = Config::with_defaults();
    config.ram_init_mode = RamInitMode::Zero;
    let mut nes = Nes::new(config);
    
    // Insert a cartridge so reset works
    let rom_data = create_test_rom();
    let cartridge = Cartridge::new(&rom_data).expect("Failed to create cartridge");
    nes.insert_cartridge(cartridge);
    
    // Write some non-zero data to RAM
    {
        let bus = nes.bus.borrow();
        let ram_rc = bus.cpu_ram_ref();
        let mut ram = ram_rc.borrow_mut();
        ram[0x100] = 0xFF;
        ram[0x200] = 0xAA;
    }
    
    // Hard reset (soft_reset = false) should re-initialize RAM to zero
    nes.reset(false);
    
    let bus = nes.bus.borrow();
    let ram_rc = bus.cpu_ram_ref();
    let ram = ram_rc.borrow();
    assert_eq!(ram[0x100], 0x00, "Hard reset should zero RAM[0x100]");
    assert_eq!(ram[0x200], 0x00, "Hard reset should zero RAM[0x200]");
}

#[test]
fn test_soft_reset_preserves_ram() {
    let mut config = Config::with_defaults();
    config.ram_init_mode = RamInitMode::Zero;
    let mut nes = Nes::new(config);
    
    // Insert a cartridge so reset works
    let rom_data = create_test_rom();
    let cartridge = Cartridge::new(&rom_data).expect("Failed to create cartridge");
    nes.insert_cartridge(cartridge);
    
    // Write some non-zero data to RAM
    {
        let bus = nes.bus.borrow();
        let ram_rc = bus.cpu_ram_ref();
        let mut ram = ram_rc.borrow_mut();
        ram[0x100] = 0xFF;
        ram[0x200] = 0xAA;
    }
    
    // Soft reset (soft_reset = true) should preserve RAM
    nes.reset(true);
    
    let bus = nes.bus.borrow();
    let ram_rc = bus.cpu_ram_ref();
    let ram = ram_rc.borrow();
    assert_eq!(ram[0x100], 0xFF, "Soft reset should preserve RAM[0x100]");
    assert_eq!(ram[0x200], 0xAA, "Soft reset should preserve RAM[0x200]");
}

#[test]
fn test_ppu_hard_reset_reinitializes_ram() {
    let mut config = Config::with_defaults();
    config.ram_init_mode = RamInitMode::Zero;
    let mut nes = Nes::new(config);
    
    // Insert a cartridge so reset works
    let rom_data = create_test_rom();
    let cartridge = Cartridge::new(&rom_data).expect("Failed to create cartridge");
    nes.insert_cartridge(cartridge);
    
    // Write some non-zero data to PPU nametable
    {
        let mut bus = nes.bus.borrow_mut();
        // Write via PPU registers
        bus.write(0x2006, 0x20, false); // PPUADDR high
        bus.write(0x2006, 0x00, false); // PPUADDR low
        bus.write(0x2007, 0xFF, false); // PPUDATA
    }
    
    // Hard reset should re-initialize PPU RAM to zero
    nes.reset(false);
    
    let ppu = nes.ppu.borrow();
    assert_eq!(
        ppu.read_nametable_for_debug(0x2000), 0x00,
        "Hard reset should zero PPU nametable"
    );
}

#[test]
fn test_ppu_soft_reset_preserves_ram() {
    let mut config = Config::with_defaults();
    config.ram_init_mode = RamInitMode::Zero;
    let mut nes = Nes::new(config);
    
    // Insert a cartridge so reset works
    let rom_data = create_test_rom();
    let cartridge = Cartridge::new(&rom_data).expect("Failed to create cartridge");
    nes.insert_cartridge(cartridge);
    
    // Write some non-zero data to PPU nametable
    {
        let mut bus = nes.bus.borrow_mut();
        bus.write(0x2006, 0x20, false); // PPUADDR high
        bus.write(0x2006, 0x00, false); // PPUADDR low
        bus.write(0x2007, 0xFF, false); // PPUDATA
    }
    
    // Soft reset should preserve PPU RAM
    nes.reset(true);
    
    let ppu = nes.ppu.borrow();
    assert_eq!(
        ppu.read_nametable_for_debug(0x2000), 0xFF,
        "Soft reset should preserve PPU nametable"
    );
}
