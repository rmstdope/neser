//! GBA cartridge Flash ROM save backend.
//!
//! GBA Flash chips (Atmel AT29LV512, Macronix MX29L010, Panasonic
//! MN63F805MNP, …) sit in the same 64 KB cartridge window as SRAM and use
//! the JEDEC-style "magic write sequence" command interface documented at
//! <https://problemkaputt.de/gbatek.htm#gbacartbackupflashrom>.
//!
//! Supported variants:
//!
//! * 64 KB single-bank flash
//! * 128 KB dual-bank flash, banks switched via the `0xB0` command
//!
//! Implemented commands (offsets are relative to the start of the 64 KB
//! Flash window — the bus passes addresses already masked to 16 bits):
//!
//! | Sequence (writes)                                                    | Effect                      |
//! |----------------------------------------------------------------------|-----------------------------|
//! | `5555<-AA, 2AAA<-55, 5555<-90`                                       | Enter ID readback mode      |
//! | `5555<-AA, 2AAA<-55, 5555<-F0`                                       | Exit ID readback mode       |
//! | `5555<-AA, 2AAA<-55, 5555<-80, 5555<-AA, 2AAA<-55, 5555<-10`         | Erase entire chip           |
//! | `5555<-AA, 2AAA<-55, 5555<-80, 5555<-AA, 2AAA<-55, n000<-30`         | Erase 4 KB sector at `n000` |
//! | `5555<-AA, 2AAA<-55, 5555<-A0, addr<-data`                           | Program one byte at `addr`  |
//! | `5555<-AA, 2AAA<-55, 5555<-B0, 0000<-bank` (128 KB only)             | Switch active bank          |
//!
//! In ID mode reads return the chip identification: byte `0x0000` is the
//! manufacturer ID and byte `0x0001` is the device ID. We expose the
//! 64 KB Atmel pair `(0x1F, 0x3D)` for the 64 KB variant and the 128 KB
//! Sanyo pair `(0x62, 0x13)` for the 128 KB variant — these are the IDs
//! Pokémon, F-Zero and other first-party titles probe for.

use crate::gba::cartridge::save_type::SaveType;
use serde::{Deserialize, Serialize};

/// Size of one Flash bank (always 64 KB on real hardware).
pub const FLASH_BANK_SIZE: usize = 64 * 1024;
/// Size of one Flash sector (4 KB).
pub const FLASH_SECTOR_SIZE: usize = 4 * 1024;

/// JEDEC-style command sequence offsets within a 64 KB bank.
const CMD_ADDR_1: usize = 0x5555;
const CMD_ADDR_2: usize = 0x2AAA;

/// Manufacturer/device ID for the 64 KB Atmel chip used by many first-party
/// games (matches mGBA's defaults).
const ID_64K: (u8, u8) = (0x1F, 0x3D);
/// Manufacturer/device ID for a 128 KB Sanyo chip.
const ID_128K: (u8, u8) = (0x62, 0x13);

/// Internal state machine tracking the JEDEC command parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum FlashState {
    /// Idle — first magic byte not yet seen.
    Ready,
    /// Saw `5555<-AA`, expecting `2AAA<-55`.
    Cmd1,
    /// Saw `2AAA<-55`, expecting `5555<-<op>`.
    Cmd2,
    /// Erase prepared (`80`); awaiting the second magic sequence + erase
    /// opcode.
    EraseCmd1,
    EraseCmd2,
    /// `A0` programmed; the next write becomes the data byte.
    AwaitWriteData,
    /// `B0` programmed; the next write to `0x0000` selects the bank.
    AwaitBankSelect,
    /// Both magic prefixes seen with `80` in between — awaiting a final
    /// chip-erase (`5555<-10`) or sector-erase (`n000<-30`) opcode.
    AwaitErase,
}

/// Flash ROM save backend.
#[derive(Debug, Clone)]
pub struct Flash {
    /// Total backing data: 64 KB or 128 KB.
    data: Vec<u8>,
    /// Currently selected bank (0 always; 1 only when [`size`] = 128 KB).
    bank: usize,
    /// Whether the chip is currently exposing its ID at offsets 0/1.
    id_mode: bool,
    /// Variant chip IDs returned in [`Self::id_mode`].
    id: (u8, u8),
    /// Command-sequence state machine.
    state: FlashState,
}

/// Serializable Flash snapshot for GBA save states.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashStateSnapshot {
    data: Vec<u8>,
    bank: usize,
    id_mode: bool,
    id: (u8, u8),
    state: FlashState,
}

impl Flash {
    /// Build a new 64 KB single-bank Flash backend.
    pub fn new_64k() -> Self {
        Self {
            data: vec![0xFF; FLASH_BANK_SIZE],
            bank: 0,
            id_mode: false,
            id: ID_64K,
            state: FlashState::Ready,
        }
    }

    /// Build a new 128 KB dual-bank Flash backend.
    pub fn new_128k() -> Self {
        Self {
            data: vec![0xFF; FLASH_BANK_SIZE * 2],
            bank: 0,
            id_mode: false,
            id: ID_128K,
            state: FlashState::Ready,
        }
    }

    /// Build a new Flash backend for the given save type.
    ///
    /// This constructor is crate-private because only Flash save types are
    /// valid. External callers should use [`Flash::new_64k`] or
    /// [`Flash::new_128k`] instead.
    pub(crate) fn new(save_type: SaveType) -> Self {
        match save_type {
            SaveType::Flash64K => Self::new_64k(),
            SaveType::Flash128K => Self::new_128k(),
            other => panic!("Flash::new called with non-Flash save type: {other:?}"),
        }
    }

    /// Total Flash size in bytes (64 or 128 KB).
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Whether the chip has two banks (128 KB variant).
    pub fn has_two_banks(&self) -> bool {
        self.data.len() > FLASH_BANK_SIZE
    }

    /// Read one byte at `offset` within the 64 KB cart-RAM window.
    ///
    /// While in ID mode, offsets `0x0000` and `0x0001` return the
    /// manufacturer / device ID respectively; all other offsets fall
    /// through to the underlying flash data so reads after a programming
    /// step still see the new value, matching real-hardware behavior.
    pub fn read(&self, offset: usize) -> u8 {
        let masked = offset & (FLASH_BANK_SIZE - 1);
        if self.id_mode {
            match masked {
                0x0000 => return self.id.0,
                0x0001 => return self.id.1,
                _ => {}
            }
        }
        self.data[self.bank * FLASH_BANK_SIZE + masked]
    }

    /// Write `value` to `offset`. Drives the JEDEC command state machine.
    pub fn write(&mut self, offset: usize, value: u8) {
        let masked = offset & (FLASH_BANK_SIZE - 1);
        match self.state {
            FlashState::Ready => {
                if masked == CMD_ADDR_1 && value == 0xAA {
                    self.state = FlashState::Cmd1;
                }
            }
            FlashState::Cmd1 => {
                if masked == CMD_ADDR_2 && value == 0x55 {
                    self.state = FlashState::Cmd2;
                } else {
                    self.state = FlashState::Ready;
                }
            }
            FlashState::Cmd2 => {
                if masked == CMD_ADDR_1 {
                    match value {
                        0x90 => {
                            self.id_mode = true;
                            self.state = FlashState::Ready;
                        }
                        0xF0 => {
                            self.id_mode = false;
                            self.state = FlashState::Ready;
                        }
                        0x80 => self.state = FlashState::EraseCmd1,
                        0xA0 => self.state = FlashState::AwaitWriteData,
                        0xB0 if self.has_two_banks() => {
                            self.state = FlashState::AwaitBankSelect;
                        }
                        _ => self.state = FlashState::Ready,
                    }
                } else {
                    self.state = FlashState::Ready;
                }
            }
            FlashState::EraseCmd1 => {
                if masked == CMD_ADDR_1 && value == 0xAA {
                    self.state = FlashState::EraseCmd2;
                } else {
                    self.state = FlashState::Ready;
                }
            }
            FlashState::EraseCmd2 => {
                if masked == CMD_ADDR_2 && value == 0x55 {
                    // Now expect either chip-erase (0x10 at 0x5555) or
                    // sector-erase (0x30 at 0xn000).
                    self.state = FlashState::AwaitErase;
                } else {
                    self.state = FlashState::Ready;
                }
            }
            FlashState::AwaitErase => match (masked, value) {
                (CMD_ADDR_1, 0x10) => {
                    // Chip erase: clear all banks.
                    for byte in self.data.iter_mut() {
                        *byte = 0xFF;
                    }
                    self.state = FlashState::Ready;
                }
                (addr, 0x30) if addr & 0x0FFF == 0 => {
                    // Sector erase: clear the 4 KB sector starting at
                    // `addr` within the active bank.
                    let base = self.bank * FLASH_BANK_SIZE + addr;
                    for byte in &mut self.data[base..base + FLASH_SECTOR_SIZE] {
                        *byte = 0xFF;
                    }
                    self.state = FlashState::Ready;
                }
                _ => self.state = FlashState::Ready,
            },
            FlashState::AwaitWriteData => {
                // Programming a NOR flash byte can only clear bits, never
                // set them — model that with an AND so back-to-back writes
                // without an erase produce realistic results.
                let idx = self.bank * FLASH_BANK_SIZE + masked;
                self.data[idx] &= value;
                self.state = FlashState::Ready;
            }
            FlashState::AwaitBankSelect => {
                if masked == 0x0000 {
                    self.bank = (value as usize) & 0x1;
                }
                self.state = FlashState::Ready;
            }
        }
    }

    /// Borrow the full backing buffer (for `.sav` flush).
    pub fn snapshot(&self) -> &[u8] {
        &self.data
    }

    /// Capture Flash state for save-state serialization.
    pub fn capture_state(&self) -> FlashStateSnapshot {
        FlashStateSnapshot {
            data: self.data.clone(),
            bank: self.bank,
            id_mode: self.id_mode,
            id: self.id,
            state: self.state,
        }
    }

    /// Restore Flash state from a save-state snapshot.
    pub fn restore_state(&mut self, state: &FlashStateSnapshot) -> Result<(), String> {
        if state.data.len() != FLASH_BANK_SIZE && state.data.len() != FLASH_BANK_SIZE * 2 {
            return Err(format!(
                "Flash save-state length mismatch: expected {FLASH_BANK_SIZE} or {}, got {}",
                FLASH_BANK_SIZE * 2,
                state.data.len()
            ));
        }
        let bank_count = state.data.len() / FLASH_BANK_SIZE;
        if state.bank >= bank_count {
            return Err(format!(
                "Flash save-state bank out of range: bank {} for {bank_count} banks",
                state.bank
            ));
        }
        if self.data.len() != state.data.len() {
            return Err(format!(
                "Flash save-state variant mismatch: live={} bytes, state={} bytes",
                self.data.len(),
                state.data.len()
            ));
        }
        self.data.clone_from(&state.data);
        self.bank = state.bank;
        self.id_mode = state.id_mode;
        self.id = state.id;
        self.state = state.state;
        Ok(())
    }

    /// Restore Flash contents from a saved buffer. Bytes past the chip
    /// capacity are ignored.
    pub fn restore(&mut self, data: &[u8]) {
        let n = data.len().min(self.data.len());
        self.data[..n].copy_from_slice(&data[..n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: run the magic prefix `5555<-AA, 2AAA<-55`.
    fn magic_prefix(flash: &mut Flash) {
        flash.write(0x5555, 0xAA);
        flash.write(0x2AAA, 0x55);
    }

    #[test]
    fn fresh_flash_reads_as_0xff() {
        let flash = Flash::new(SaveType::Flash64K);
        assert_eq!(flash.read(0), 0xFF);
        assert_eq!(flash.read(0xFFFF), 0xFF);
    }

    #[test]
    fn id_mode_returns_chip_ids_for_64k() {
        let mut flash = Flash::new(SaveType::Flash64K);
        magic_prefix(&mut flash);
        flash.write(0x5555, 0x90); // enter ID mode
        assert_eq!(flash.read(0x0000), ID_64K.0);
        assert_eq!(flash.read(0x0001), ID_64K.1);
        // Exit ID mode.
        magic_prefix(&mut flash);
        flash.write(0x5555, 0xF0);
        assert_eq!(flash.read(0x0000), 0xFF);
    }

    #[test]
    fn id_mode_returns_chip_ids_for_128k() {
        let mut flash = Flash::new(SaveType::Flash128K);
        magic_prefix(&mut flash);
        flash.write(0x5555, 0x90);
        assert_eq!(flash.read(0x0000), ID_128K.0);
        assert_eq!(flash.read(0x0001), ID_128K.1);
    }

    #[test]
    fn write_byte_command_programs_single_byte() {
        let mut flash = Flash::new(SaveType::Flash64K);
        magic_prefix(&mut flash);
        flash.write(0x5555, 0xA0); // arm byte program
        flash.write(0x1234, 0x42);
        assert_eq!(flash.read(0x1234), 0x42);
        // Untouched cells stay erased.
        assert_eq!(flash.read(0x1233), 0xFF);
        assert_eq!(flash.read(0x1235), 0xFF);
    }

    #[test]
    fn programming_only_clears_bits() {
        let mut flash = Flash::new(SaveType::Flash64K);
        // First program: 0xF0 over erased 0xFF -> 0xF0
        magic_prefix(&mut flash);
        flash.write(0x5555, 0xA0);
        flash.write(0x100, 0xF0);
        assert_eq!(flash.read(0x100), 0xF0);
        // Second program: 0x33 ANDed with 0xF0 -> 0x30 (without erase)
        magic_prefix(&mut flash);
        flash.write(0x5555, 0xA0);
        flash.write(0x100, 0x33);
        assert_eq!(flash.read(0x100), 0x30);
    }

    #[test]
    fn sector_erase_clears_only_target_sector() {
        let mut flash = Flash::new(SaveType::Flash64K);
        // Program a byte in two different sectors.
        magic_prefix(&mut flash);
        flash.write(0x5555, 0xA0);
        flash.write(0x1000, 0x11);
        magic_prefix(&mut flash);
        flash.write(0x5555, 0xA0);
        flash.write(0x2000, 0x22);

        // Erase sector at 0x1000 (4 KB).
        magic_prefix(&mut flash);
        flash.write(0x5555, 0x80);
        magic_prefix(&mut flash);
        flash.write(0x1000, 0x30);

        assert_eq!(flash.read(0x1000), 0xFF, "target sector must be erased");
        assert_eq!(
            flash.read(0x1FFF),
            0xFF,
            "target sector tail must be erased"
        );
        assert_eq!(flash.read(0x2000), 0x22, "other sector must be untouched");
    }

    #[test]
    fn chip_erase_clears_all_data() {
        let mut flash = Flash::new(SaveType::Flash64K);
        magic_prefix(&mut flash);
        flash.write(0x5555, 0xA0);
        flash.write(0x0042, 0x77);

        magic_prefix(&mut flash);
        flash.write(0x5555, 0x80);
        magic_prefix(&mut flash);
        flash.write(0x5555, 0x10);

        assert_eq!(flash.read(0x0042), 0xFF);
    }

    #[test]
    fn bank_switch_isolates_two_banks_for_128k() {
        let mut flash = Flash::new(SaveType::Flash128K);
        // Program byte in bank 0.
        magic_prefix(&mut flash);
        flash.write(0x5555, 0xA0);
        flash.write(0x0000, 0x11);

        // Switch to bank 1.
        magic_prefix(&mut flash);
        flash.write(0x5555, 0xB0);
        flash.write(0x0000, 0x01);

        assert_eq!(flash.read(0x0000), 0xFF, "bank 1 must read as erased");

        // Program byte in bank 1.
        magic_prefix(&mut flash);
        flash.write(0x5555, 0xA0);
        flash.write(0x0000, 0x22);
        assert_eq!(flash.read(0x0000), 0x22);

        // Switch back to bank 0 — original byte must still be there.
        magic_prefix(&mut flash);
        flash.write(0x5555, 0xB0);
        flash.write(0x0000, 0x00);
        assert_eq!(flash.read(0x0000), 0x11);
    }

    #[test]
    fn bank_switch_ignored_on_64k_chip() {
        let mut flash = Flash::new(SaveType::Flash64K);
        magic_prefix(&mut flash);
        flash.write(0x5555, 0xB0);
        // Should remain in idle state — a follow-up "bank select" write is
        // simply a stray byte. No panic, no bank switch.
        flash.write(0x0000, 0x01);
        assert_eq!(flash.read(0x0000), 0xFF);
    }

    #[test]
    fn snapshot_restore_round_trip() {
        let mut a = Flash::new(SaveType::Flash64K);
        magic_prefix(&mut a);
        a.write(0x5555, 0xA0);
        a.write(0x0500, 0xCD);
        let snap = a.snapshot().to_vec();

        let mut b = Flash::new(SaveType::Flash64K);
        b.restore(&snap);
        assert_eq!(b.read(0x0500), 0xCD);
        assert_eq!(b.snapshot().len(), 64 * 1024);
    }

    #[test]
    fn invalid_command_resets_state_machine() {
        let mut flash = Flash::new(SaveType::Flash64K);
        // Start a sequence then drop in a bad write.
        flash.write(0x5555, 0xAA);
        flash.write(0x0000, 0x00); // wrong second-step address
        // State machine should reset; further writes must be raw stores
        // that have no effect (no A0 has been issued).
        flash.write(0x1234, 0x99);
        assert_eq!(flash.read(0x1234), 0xFF);
    }

    #[test]
    fn save_state_restores_id_mode() {
        let mut flash = Flash::new(SaveType::Flash64K);
        magic_prefix(&mut flash);
        flash.write(0x5555, 0x90);

        let state = flash.capture_state();
        magic_prefix(&mut flash);
        flash.write(0x5555, 0xF0);
        flash.restore_state(&state).expect("restore Flash state");

        assert_eq!(flash.read(0x0000), ID_64K.0);
        assert_eq!(flash.read(0x0001), ID_64K.1);
    }

    #[test]
    fn save_state_restores_await_write_data_command_state() {
        let mut flash = Flash::new(SaveType::Flash64K);
        magic_prefix(&mut flash);
        flash.write(0x5555, 0xA0);

        let state = flash.capture_state();
        let mut restored = Flash::new(SaveType::Flash64K);
        restored.restore_state(&state).expect("restore Flash state");
        restored.write(0x2468, 0x5A);

        assert_eq!(restored.read(0x2468), 0x5A);
    }

    #[test]
    fn save_state_restores_128k_bank_and_data() {
        let mut flash = Flash::new(SaveType::Flash128K);
        magic_prefix(&mut flash);
        flash.write(0x5555, 0xA0);
        flash.write(0x0000, 0x11);
        magic_prefix(&mut flash);
        flash.write(0x5555, 0xB0);
        flash.write(0x0000, 0x01);
        magic_prefix(&mut flash);
        flash.write(0x5555, 0xA0);
        flash.write(0x0000, 0x22);

        let state = flash.capture_state();
        let mut restored = Flash::new(SaveType::Flash128K);
        restored.restore_state(&state).expect("restore Flash state");

        assert_eq!(restored.read(0x0000), 0x22);
        magic_prefix(&mut restored);
        restored.write(0x5555, 0xB0);
        restored.write(0x0000, 0x00);
        assert_eq!(restored.read(0x0000), 0x11);
    }

    #[test]
    fn save_state_roundtrips_through_json() {
        let mut flash = Flash::new(SaveType::Flash64K);
        magic_prefix(&mut flash);
        flash.write(0x5555, 0xA0);
        flash.write(0x3210, 0x66);

        let bytes = serde_json::to_vec(&flash.capture_state()).expect("serialize Flash state");
        let decoded: FlashStateSnapshot =
            serde_json::from_slice(&bytes).expect("deserialize Flash state");
        let mut restored = Flash::new(SaveType::Flash64K);
        restored
            .restore_state(&decoded)
            .expect("restore Flash state");

        assert_eq!(restored.read(0x3210), 0x66);
    }

    #[test]
    fn save_state_rejects_flash_variant_mismatch_without_mutating() {
        let mut flash64 = Flash::new(SaveType::Flash64K);
        magic_prefix(&mut flash64);
        flash64.write(0x5555, 0xA0);
        flash64.write(0x3210, 0x66);
        let state = flash64.capture_state();

        let mut flash128 = Flash::new(SaveType::Flash128K);
        magic_prefix(&mut flash128);
        flash128.write(0x5555, 0xB0);
        flash128.write(0x0000, 0x01);
        magic_prefix(&mut flash128);
        flash128.write(0x5555, 0xA0);
        flash128.write(0x3210, 0x99);

        let result = flash128.restore_state(&state);

        assert!(result.is_err());
        assert_eq!(flash128.size(), FLASH_BANK_SIZE * 2);
        assert_eq!(flash128.read(0x3210), 0x99);
    }
}
