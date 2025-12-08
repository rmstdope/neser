/// Utility to run OAM test ROMs and check for result codes in memory
/// This helps determine where test results are written
use neser::cartridge::Cartridge;
use neser::nes::{Nes, TvSystem};
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <rom_path>", args[0]);
        eprintln!("Example: {} roms/oam_read.nes", args[0]);
        std::process::exit(1);
    }

    let rom_path = &args[1];
    println!("Loading ROM: {}", rom_path);

    // Load ROM
    let rom_data = fs::read(rom_path).expect("Failed to load ROM");
    let cartridge = Cartridge::new(&rom_data).expect("Failed to parse ROM");

    // Create NES and insert cartridge
    let mut nes = Nes::new(TvSystem::Ntsc);
    nes.insert_cartridge(cartridge);
    nes.reset();

    println!("Running test ROM...");
    println!("Checking memory addresses $6000-$6010 every 60 frames\n");

    // Run for multiple frames and check for results
    for frame in 1..=300 {
        // Run one frame (roughly 29780 cycles for NTSC)
        for _ in 0..29780 {
            nes.run_cpu_tick();
        }

        // Check every 60 frames (1 second)
        if frame % 60 == 0 {
            println!("Frame {}: Checking memory...", frame);

            // Check common result addresses
            print!("  $6000-$6007: ");
            for addr in 0x6000..=0x6007 {
                let val = nes.memory.borrow().read(addr);
                if val >= 0x20 && val <= 0x7E {
                    print!("{}", val as char);
                } else {
                    print!("[{:02X}]", val);
                }
            }
            println!();

            print!("  $6008-$600F: ");
            for addr in 0x6008..=0x600F {
                let val = nes.memory.borrow().read(addr);
                if val >= 0x20 && val <= 0x7E {
                    print!("{}", val as char);
                } else {
                    print!("[{:02X}]", val);
                }
            }
            println!();

            // Check for "Pass" or "Fail" patterns
            let bytes: Vec<u8> = (0x6000..=0x600F)
                .map(|addr| nes.memory.borrow().read(addr))
                .collect();
            if bytes.starts_with(b"Pass") {
                println!("\n✅ TEST PASSED - Found 'Pass' at $6000");
                return;
            } else if bytes.windows(4).any(|w| w.starts_with(b"Fail")) {
                println!("\n❌ TEST FAILED - Found 'Fail' in results");
                return;
            }

            // Check single byte status
            let status = nes.memory.borrow().read(0x6000);
            if status == 0x00 {
                println!("  Status byte at $6000: 0x00 (might indicate pass)");
            } else if status != 0xFF && status != 0x00 {
                println!(
                    "  Status byte at $6000: 0x{:02X} (might indicate error code)",
                    status
                );
            }

            println!();
        }
    }

    println!("Test completed after 300 frames (5 seconds)");

    // Print final memory dump - extended to show full text
    println!("\nFinal memory dump $6000-$607F:");
    for addr in 0x6000..=0x607F {
        let val = nes.memory.borrow().read(addr);
        if addr % 16 == 0 {
            if addr != 0x6000 {
                println!();
            }
            print!("{:04X}: ", addr);
        }
        print!("{:02X} ", val);
    }
    println!();

    // Print as ASCII
    println!("\nError message (starting at $6004):");
    for addr in 0x6004..=0x607F {
        let val = nes.memory.borrow().read(addr);
        if val == 0 {
            break;
        }
        if val >= 0x20 && val < 0x7F {
            print!("{}", val as char);
        } else {
            print!(".");
        }
    }
    println!();
}
