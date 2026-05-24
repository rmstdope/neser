//! GBA cartridge EEPROM (I²C bit-serial) save backend.
//!
//! GBA EEPROMs come in 512-byte and 8 KB variants and share a common
//! bit-serial protocol described at
//! <https://problemkaputt.de/gbatek.htm#gbacartbackupeeprom>.
//!
//! All accesses are funneled through a single 16-bit cart-ROM address
//! (`0x0DFFFF00–0x0DFFFFFE`). Bit 0 of each halfword written carries one
//! protocol bit; bit 0 of each halfword read returns one protocol bit.
//! A complete transaction is:
//!
//! * **Read**: send `11`, `addr` (6 or 14 bits), `0` (stop). Then read
//!   four dummy `0` bits followed by 64 data bits.
//! * **Write**: send `10`, `addr` (6 or 14 bits), 64 data bits, `0` (stop).
//!
//! Each addressable block is 64 bits (8 bytes), so the 512 B variant has
//! 64 blocks selected by 6 address bits and the 8 KB variant has 1024
//! blocks selected by 14 address bits.
//!
//! This module owns the byte buffer and a small state machine. Callers
//! pass the LSB of each halfword as a single bit into [`Eeprom::write_bit`]
//! and read one bit from [`Eeprom::read_bit`]; the bus layer is
//! responsible for actually wiring up the address comparison.

use crate::gba::cartridge::save_type::SaveType;
use serde::{Deserialize, Serialize};

/// Address width (number of bits) for the 512 B variant.
const ADDR_BITS_512: u32 = 6;
/// Address width (number of bits) for the 8 KB variant.
const ADDR_BITS_8K: u32 = 14;

/// Number of dummy zero bits returned at the start of every read.
const READ_DUMMY_BITS: u32 = 4;
/// Number of data bits per addressable block (8 bytes).
const BLOCK_BITS: u32 = 64;
/// Block size in bytes.
const BLOCK_BYTES: usize = 8;

/// EEPROM I²C state machine phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Phase {
    /// No active transaction.
    Idle,
    /// Just consumed the first request bit (`1`); waiting for direction
    /// bit (1 = read, 0 = write).
    AwaitDirection,
    /// Collecting `addr_bits` of address from the host.
    AddrIn { read: bool, remaining: u32 },
    /// Collecting 64 data bits from the host (write only).
    WriteData { addr: u32, remaining: u32 },
    /// Awaiting the trailing zero bit from the host (write only).
    WriteStop { addr: u32 },
    /// Read operation is fully prepared and `Eeprom::read_bit` is now
    /// streaming dummy + data bits back.
    ReadOut {
        addr: u32,
        dummy_remaining: u32,
        data_remaining: u32,
    },
}

/// EEPROM backend.
#[derive(Debug, Clone)]
pub struct Eeprom {
    data: Vec<u8>,
    addr_bits: u32,
    phase: Phase,
    /// Shift register that accumulates input bits while in `AddrIn`,
    /// `AwaitDirection`, etc.
    shift_in: u64,
    /// Buffer for incoming write data bits before commit.
    write_shift: u64,
}

/// Serializable EEPROM snapshot for GBA save states.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EepromState {
    data: Vec<u8>,
    addr_bits: u32,
    phase: Phase,
    shift_in: u64,
    write_shift: u64,
}

impl Eeprom {
    /// Build a new 512-byte EEPROM backend (6-bit address bus).
    pub fn new_512b() -> Self {
        Self {
            data: vec![0xFF; SaveType::Eeprom512.size_bytes()],
            addr_bits: ADDR_BITS_512,
            phase: Phase::Idle,
            shift_in: 0,
            write_shift: 0,
        }
    }

    /// Build a new 8 KB EEPROM backend (14-bit address bus).
    pub fn new_8k() -> Self {
        Self {
            data: vec![0xFF; SaveType::Eeprom8K.size_bytes()],
            addr_bits: ADDR_BITS_8K,
            phase: Phase::Idle,
            shift_in: 0,
            write_shift: 0,
        }
    }

    /// Build a new EEPROM backend for the given save type.
    ///
    /// This constructor is crate-private because only EEPROM save types are
    /// valid. External callers should use [`Eeprom::new_512b`] or
    /// [`Eeprom::new_8k`] instead.
    pub(crate) fn new(save_type: SaveType) -> Self {
        match save_type {
            SaveType::Eeprom512 => Self::new_512b(),
            SaveType::Eeprom8K => Self::new_8k(),
            other => panic!("Eeprom::new called with non-EEPROM save type: {other:?}"),
        }
    }

    /// EEPROM size in bytes.
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Address width (number of bits used to select an 8-byte block).
    pub fn addr_bits(&self) -> u32 {
        self.addr_bits
    }

    /// Push one bit (LSB of a 16-bit cart-bus write) into the state
    /// machine.
    pub fn write_bit(&mut self, bit: u8) {
        let bit = (bit & 1) as u64;
        match self.phase {
            Phase::Idle => {
                if bit == 1 {
                    self.phase = Phase::AwaitDirection;
                }
                // Stray zero in idle is ignored.
            }
            Phase::AwaitDirection => {
                let read = bit == 1;
                self.shift_in = 0;
                self.phase = Phase::AddrIn {
                    read,
                    remaining: self.addr_bits,
                };
            }
            Phase::AddrIn { read, remaining } => {
                self.shift_in = (self.shift_in << 1) | bit;
                let remaining = remaining - 1;
                if remaining == 0 {
                    let addr = (self.shift_in & ((1u64 << self.addr_bits) - 1)) as u32;
                    if read {
                        // The terminating zero bit for read requests is
                        // consumed here in the same step (the host
                        // immediately starts clocking out bits afterwards).
                        self.phase = Phase::ReadOut {
                            addr,
                            dummy_remaining: READ_DUMMY_BITS,
                            data_remaining: BLOCK_BITS,
                        };
                    } else {
                        self.write_shift = 0;
                        self.phase = Phase::WriteData {
                            addr,
                            remaining: BLOCK_BITS,
                        };
                    }
                } else {
                    self.phase = Phase::AddrIn { read, remaining };
                }
            }
            Phase::WriteData { addr, remaining } => {
                self.write_shift = (self.write_shift << 1) | bit;
                let remaining = remaining - 1;
                if remaining == 0 {
                    self.commit_write(addr, self.write_shift);
                    self.phase = Phase::WriteStop { addr };
                } else {
                    self.phase = Phase::WriteData { addr, remaining };
                }
            }
            Phase::WriteStop { .. } => {
                // Trailing zero bit. Either way, we return to idle.
                let _ = bit;
                self.phase = Phase::Idle;
            }
            Phase::ReadOut { .. } => {
                // Spec says reads do not interleave with writes, but real
                // games sometimes drive an extra zero bit while clocking
                // out data. Ignore.
                let _ = bit;
            }
        }
    }

    /// Pop one bit (LSB of a 16-bit cart-bus read) out of the state
    /// machine. Outside of a read transaction this returns `1`, matching
    /// the GBA's pull-up on the EEPROM data line.
    pub fn read_bit(&mut self) -> u8 {
        match self.phase {
            Phase::ReadOut {
                addr,
                dummy_remaining,
                data_remaining,
            } => {
                if dummy_remaining > 0 {
                    self.phase = Phase::ReadOut {
                        addr,
                        dummy_remaining: dummy_remaining - 1,
                        data_remaining,
                    };
                    0
                } else {
                    // Stream the 64 data bits MSB first.
                    let bit_index = data_remaining - 1;
                    let block_value = self.read_block(addr);
                    let bit = ((block_value >> bit_index) & 1) as u8;
                    let new_remaining = data_remaining - 1;
                    if new_remaining == 0 {
                        self.phase = Phase::Idle;
                    } else {
                        self.phase = Phase::ReadOut {
                            addr,
                            dummy_remaining: 0,
                            data_remaining: new_remaining,
                        };
                    }
                    bit
                }
            }
            _ => 1,
        }
    }

    /// Borrow the full backing buffer (for `.sav` flush).
    pub fn snapshot(&self) -> &[u8] {
        &self.data
    }

    /// Capture EEPROM state for save-state serialization.
    pub fn capture_state(&self) -> EepromState {
        EepromState {
            data: self.data.clone(),
            addr_bits: self.addr_bits,
            phase: self.phase,
            shift_in: self.shift_in,
            write_shift: self.write_shift,
        }
    }

    /// Restore EEPROM state from a save-state snapshot.
    pub fn restore_state(&mut self, state: &EepromState) -> Result<(), String> {
        let expected_len = match state.addr_bits {
            ADDR_BITS_512 => SaveType::Eeprom512.size_bytes(),
            ADDR_BITS_8K => SaveType::Eeprom8K.size_bytes(),
            other => {
                return Err(format!(
                    "invalid EEPROM address width in save state: {other}"
                ));
            }
        };
        if state.data.len() != expected_len {
            return Err(format!(
                "EEPROM save-state length mismatch: expected {expected_len}, got {}",
                state.data.len()
            ));
        }
        if self.data.len() != state.data.len() {
            return Err(format!(
                "EEPROM save-state variant mismatch: live={} bytes, state={} bytes",
                self.data.len(),
                state.data.len()
            ));
        }
        self.data.clone_from(&state.data);
        self.addr_bits = state.addr_bits;
        self.phase = state.phase;
        self.shift_in = state.shift_in;
        self.write_shift = state.write_shift;
        Ok(())
    }

    /// Restore EEPROM contents from a saved buffer. Bytes past the chip
    /// capacity are ignored.
    pub fn restore(&mut self, data: &[u8]) {
        let n = data.len().min(self.data.len());
        self.data[..n].copy_from_slice(&data[..n]);
    }

    /// Read an 8-byte block as a big-endian u64 (matching the way bits are
    /// streamed out MSB-first on the I²C bus).
    fn read_block(&self, addr: u32) -> u64 {
        let base = (addr as usize) * BLOCK_BYTES;
        if base + BLOCK_BYTES > self.data.len() {
            return 0xFFFF_FFFF_FFFF_FFFF;
        }
        let bytes: [u8; BLOCK_BYTES] = self.data[base..base + BLOCK_BYTES].try_into().unwrap();
        u64::from_be_bytes(bytes)
    }

    /// Commit a 64-bit block back to the backing store.
    fn commit_write(&mut self, addr: u32, value: u64) {
        let base = (addr as usize) * BLOCK_BYTES;
        if base + BLOCK_BYTES > self.data.len() {
            return;
        }
        let bytes = value.to_be_bytes();
        self.data[base..base + BLOCK_BYTES].copy_from_slice(&bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Send the bits of `value` MSB-first, `count` bits in total.
    fn send_bits(eeprom: &mut Eeprom, value: u64, count: u32) {
        for i in (0..count).rev() {
            eeprom.write_bit(((value >> i) & 1) as u8);
        }
    }

    /// Drive a write transaction: `10`, address (`addr_bits`), 64 data
    /// bits, `0` stop bit.
    fn perform_write(eeprom: &mut Eeprom, addr: u32, data: u64) {
        let addr_bits = eeprom.addr_bits();
        send_bits(eeprom, 0b10, 2);
        send_bits(eeprom, addr as u64, addr_bits);
        send_bits(eeprom, data, 64);
        eeprom.write_bit(0);
    }

    /// Drive a read request and clock out 4 dummy bits + 64 data bits.
    fn perform_read(eeprom: &mut Eeprom, addr: u32) -> u64 {
        let addr_bits = eeprom.addr_bits();
        send_bits(eeprom, 0b11, 2);
        send_bits(eeprom, addr as u64, addr_bits);
        // 4 dummy bits should all be zero.
        for _ in 0..4 {
            assert_eq!(eeprom.read_bit(), 0, "dummy bit must be zero");
        }
        let mut data = 0u64;
        for _ in 0..64 {
            data = (data << 1) | (eeprom.read_bit() as u64);
        }
        data
    }

    #[test]
    fn write_then_read_round_trip_512b() {
        let mut eeprom = Eeprom::new(SaveType::Eeprom512);
        perform_write(&mut eeprom, 5, 0xCAFE_BABE_DEAD_BEEF);
        assert_eq!(perform_read(&mut eeprom, 5), 0xCAFE_BABE_DEAD_BEEF);
    }

    #[test]
    fn write_then_read_round_trip_8k() {
        let mut eeprom = Eeprom::new(SaveType::Eeprom8K);
        perform_write(&mut eeprom, 0x123, 0x0123_4567_89AB_CDEF);
        assert_eq!(perform_read(&mut eeprom, 0x123), 0x0123_4567_89AB_CDEF);
    }

    #[test]
    fn freshly_initialised_block_reads_as_all_ones() {
        let mut eeprom = Eeprom::new(SaveType::Eeprom512);
        assert_eq!(perform_read(&mut eeprom, 3), 0xFFFF_FFFF_FFFF_FFFF);
    }

    #[test]
    fn writes_to_different_addresses_are_independent() {
        let mut eeprom = Eeprom::new(SaveType::Eeprom8K);
        perform_write(&mut eeprom, 0, 0xAAAA_AAAA_AAAA_AAAA);
        perform_write(&mut eeprom, 1023, 0x5555_5555_5555_5555);
        assert_eq!(perform_read(&mut eeprom, 0), 0xAAAA_AAAA_AAAA_AAAA);
        assert_eq!(perform_read(&mut eeprom, 1023), 0x5555_5555_5555_5555);
    }

    #[test]
    fn read_bit_outside_transaction_returns_one() {
        let mut eeprom = Eeprom::new(SaveType::Eeprom512);
        // Bus pull-up: idle line reads as 1.
        assert_eq!(eeprom.read_bit(), 1);
        assert_eq!(eeprom.read_bit(), 1);
    }

    #[test]
    fn snapshot_restore_preserves_data() {
        let mut a = Eeprom::new(SaveType::Eeprom512);
        perform_write(&mut a, 7, 0xDEAD_BEEF_C0DE_F00D);
        let snap = a.snapshot().to_vec();

        let mut b = Eeprom::new(SaveType::Eeprom512);
        b.restore(&snap);
        assert_eq!(perform_read(&mut b, 7), 0xDEAD_BEEF_C0DE_F00D);
    }

    #[test]
    fn boundary_addresses_round_trip() {
        // 512 B → block index range 0..63
        let mut e512 = Eeprom::new(SaveType::Eeprom512);
        perform_write(&mut e512, 0, 0x1111_1111_1111_1111);
        perform_write(&mut e512, 63, 0x2222_2222_2222_2222);
        assert_eq!(perform_read(&mut e512, 0), 0x1111_1111_1111_1111);
        assert_eq!(perform_read(&mut e512, 63), 0x2222_2222_2222_2222);

        // 8 KB → block index range 0..1023
        let mut e8k = Eeprom::new(SaveType::Eeprom8K);
        perform_write(&mut e8k, 0, 0x3333_3333_3333_3333);
        perform_write(&mut e8k, 1023, 0x4444_4444_4444_4444);
        assert_eq!(perform_read(&mut e8k, 0), 0x3333_3333_3333_3333);
        assert_eq!(perform_read(&mut e8k, 1023), 0x4444_4444_4444_4444);
    }

    #[test]
    fn save_state_restores_mid_write_transaction() {
        let mut eeprom = Eeprom::new(SaveType::Eeprom512);
        let addr_bits = eeprom.addr_bits();
        send_bits(&mut eeprom, 0b10, 2);
        send_bits(&mut eeprom, 5, addr_bits);
        send_bits(&mut eeprom, 0xCAFE_BABE, 32);

        let state = eeprom.capture_state();
        let mut restored = Eeprom::new(SaveType::Eeprom512);
        restored
            .restore_state(&state)
            .expect("restore EEPROM state");

        send_bits(&mut restored, 0xDEAD_BEEF, 32);
        restored.write_bit(0);

        assert_eq!(perform_read(&mut restored, 5), 0xCAFE_BABE_DEAD_BEEF);
    }

    #[test]
    fn save_state_restores_mid_read_transaction() {
        let mut eeprom = Eeprom::new(SaveType::Eeprom512);
        perform_write(&mut eeprom, 9, 0x0123_4567_89AB_CDEF);
        let addr_bits = eeprom.addr_bits();
        send_bits(&mut eeprom, 0b11, 2);
        send_bits(&mut eeprom, 9, addr_bits);
        for _ in 0..4 {
            assert_eq!(eeprom.read_bit(), 0);
        }
        let mut prefix = 0u64;
        for _ in 0..16 {
            prefix = (prefix << 1) | u64::from(eeprom.read_bit());
        }

        let state = eeprom.capture_state();
        let mut restored = Eeprom::new(SaveType::Eeprom512);
        restored
            .restore_state(&state)
            .expect("restore EEPROM state");
        let mut value = prefix;
        for _ in 0..48 {
            value = (value << 1) | u64::from(restored.read_bit());
        }

        assert_eq!(value, 0x0123_4567_89AB_CDEF);
    }

    #[test]
    fn save_state_roundtrips_through_json() {
        let mut eeprom = Eeprom::new(SaveType::Eeprom8K);
        perform_write(&mut eeprom, 0x123, 0x55AA_0123_CDEF_9876);

        let bytes = serde_json::to_vec(&eeprom.capture_state()).expect("serialize EEPROM state");
        let decoded: EepromState =
            serde_json::from_slice(&bytes).expect("deserialize EEPROM state");
        let mut restored = Eeprom::new(SaveType::Eeprom8K);
        restored
            .restore_state(&decoded)
            .expect("restore EEPROM state");

        assert_eq!(perform_read(&mut restored, 0x123), 0x55AA_0123_CDEF_9876);
    }

    #[test]
    fn save_state_rejects_eeprom_variant_mismatch_without_mutating() {
        let mut e512 = Eeprom::new(SaveType::Eeprom512);
        perform_write(&mut e512, 7, 0xDEAD_BEEF_C0DE_F00D);
        let state = e512.capture_state();

        let mut e8k = Eeprom::new(SaveType::Eeprom8K);
        perform_write(&mut e8k, 7, 0x1111_2222_3333_4444);

        let result = e8k.restore_state(&state);

        assert!(result.is_err());
        assert_eq!(e8k.size(), SaveType::Eeprom8K.size_bytes());
        assert_eq!(e8k.addr_bits(), ADDR_BITS_8K);
        assert_eq!(perform_read(&mut e8k, 7), 0x1111_2222_3333_4444);
    }
}
