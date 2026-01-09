use std::io;
use std::path::Path;
use std::{fs, io::ErrorKind};

use crate::nes::Nes;

const PRG_RAM_START: u16 = 0x6000;
const PRG_RAM_SIZE: usize = 0x2000; // 8KB ($6000-$7FFF)

pub fn default_save_path_for_rom(rom_path: &Path) -> std::path::PathBuf {
    rom_path.with_extension("sav")
}

pub fn load_battery_backed_prg_ram(nes: &mut Nes, save_path: &Path) -> io::Result<()> {
    let data = match fs::read(save_path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };

    let mut memory = nes.memory.borrow_mut();
    for (i, byte) in data.into_iter().take(PRG_RAM_SIZE).enumerate() {
        let addr = PRG_RAM_START.wrapping_add(i as u16);
        memory.write(addr, byte, false);
    }

    Ok(())
}

pub fn save_battery_backed_prg_ram(nes: &Nes, save_path: &Path) -> io::Result<()> {
    if let Some(parent) = save_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let memory = nes.memory.borrow();
    let mut out = vec![0u8; PRG_RAM_SIZE];
    for i in 0..PRG_RAM_SIZE {
        let addr = PRG_RAM_START.wrapping_add(i as u16);
        out[i] = memory.read(addr);
    }

    // Write atomically: write to a temp file then rename over the final path.
    // This reduces the chance of corrupting an existing save file if the process exits mid-write.
    let tmp_path = save_path.with_extension("sav.tmp");
    fs::write(&tmp_path, out)?;
    fs::rename(tmp_path, save_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::{Cartridge, MirroringMode};
    use crate::nes::{Nes, TvSystem};

    fn unique_temp_path(filename: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("neser-{nonce}-{filename}"));
        path
    }

    #[test]
    fn test_battery_backed_prg_ram_persists_to_disk_across_runs() {
        // Arrange: create a NES instance with a cartridge that has PRG-RAM.
        let mut nes_a = Nes::new(TvSystem::Ntsc);
        let prg_rom = vec![0xEA; 0x8000];
        let cartridge_a = Cartridge::from_parts(prg_rom.clone(), vec![], MirroringMode::Horizontal);
        nes_a.insert_cartridge(cartridge_a);

        // Write a recognizable value into PRG-RAM.
        nes_a.memory.borrow_mut().write(0x6000, 0x42, false);

        let save_path = unique_temp_path("zelda.sav");
        let _ = std::fs::remove_file(&save_path);

        // Act: save SRAM to disk.
        save_battery_backed_prg_ram(&nes_a, &save_path).unwrap();

        // Simulate a new emulator run: new NES, fresh cartridge.
        let mut nes_b = Nes::new(TvSystem::Ntsc);
        let cartridge_b = Cartridge::from_parts(prg_rom, vec![], MirroringMode::Horizontal);
        nes_b.insert_cartridge(cartridge_b);

        load_battery_backed_prg_ram(&mut nes_b, &save_path).unwrap();

        // Assert: PRG-RAM value is restored.
        assert_eq!(nes_b.memory.borrow().read(0x6000), 0x42);

        let _ = std::fs::remove_file(&save_path);
    }
}
