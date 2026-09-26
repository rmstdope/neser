//! Nintendo's OBC1 "OBJ controller" (Metal Combat: Falcon's Revenge), fullsnes "SNES Cart
//! OBC1 (OBJ Controller)".
//!
//! The chip sits over the cartridge's 8 KiB battery-backed SRAM at `$6000-$7FFF` and adds
//! ports that address an OAM-shaped buffer in it: `$7FF0-$7FF3` are the four OAM bytes of
//! object `Index` at `Base+Index*4`, `$7FF4` its 2-bit field of the high table at
//! `Base+Index/4+$200`, `$7FF5` bit 0 the base (0 = `$7C00`, 1 = `$7800`) and `$7FF6` the
//! index (0-127, never auto-incremented).
//!
//! fullsnes leaves three details open; these follow Mesen2 `Coprocessors/OBC1/Obc1.cpp`, and
//! ares `obc1.cpp` agrees: a `$7FF4` read returns the whole high-table byte, unmasked and
//! unshifted; `$7FF5`-`$7FF7` are ordinary SRAM bytes the chip reads its base and index back
//! from; and every other address in the window is plain SRAM. So the chip has no state of its
//! own, and the `.sav` file and save states carry all of it.

/// SRAM offset of the byte holding the base select (`$7FF5`).
const BASE_REGISTER: u16 = 0x1FF5;
/// SRAM offset of the byte holding the object index (`$7FF6`).
const INDEX_REGISTER: u16 = 0x1FF6;

/// Reads `offset` (the address within the window, `addr & 0x1FFF`) through the chip.
/// `sram` must not be empty.
pub fn read(sram: &[u8], offset: u16) -> u8 {
    match offset {
        0x1FF0..=0x1FF3 => ram_read(sram, oam_address(sram) + (offset - 0x1FF0)),
        0x1FF4 => ram_read(sram, high_table_address(sram)),
        _ => ram_read(sram, offset),
    }
}

/// Writes `value` to `offset` (the address within the window, `addr & 0x1FFF`) through the
/// chip. `sram` must not be empty.
pub fn write(sram: &mut [u8], offset: u16, value: u8) {
    match offset {
        0x1FF0..=0x1FF3 => {
            let address = oam_address(sram) + (offset - 0x1FF0);
            ram_write(sram, address, value);
        }
        0x1FF4 => {
            let address = high_table_address(sram);
            let shift = (ram_read(sram, INDEX_REGISTER) & 0x03) << 1;
            let kept = ram_read(sram, address) & !(0x03 << shift);
            ram_write(sram, address, kept | ((value & 0x03) << shift));
        }
        _ => ram_write(sram, offset, value),
    }
}

/// `$7C00` (offset `$1C00`) with base bit 0 clear, `$7800` (offset `$1800`) with it set.
fn base_address(sram: &[u8]) -> u16 {
    if ram_read(sram, BASE_REGISTER) & 0x01 == 0 {
        0x1C00
    } else {
        0x1800
    }
}

fn index(sram: &[u8]) -> u16 {
    u16::from(ram_read(sram, INDEX_REGISTER) & 0x7F)
}

fn oam_address(sram: &[u8]) -> u16 {
    base_address(sram) + index(sram) * 4
}

fn high_table_address(sram: &[u8]) -> u16 {
    base_address(sram) + 0x200 + index(sram) / 4
}

fn ram_read(sram: &[u8], offset: u16) -> u8 {
    sram[usize::from(offset) % sram.len()]
}

fn ram_write(sram: &mut [u8], offset: u16, value: u8) {
    let len = sram.len();
    sram[usize::from(offset) % len] = value;
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: u16 = 0x1FF5;
    const INDEX: u16 = 0x1FF6;

    fn sram() -> Vec<u8> {
        vec![0; 0x2000]
    }

    #[test]
    fn oam_ports_address_base_plus_index_times_four() {
        let mut ram = sram();
        write(&mut ram, INDEX, 5);
        for (port, value) in [
            (0x1FF0, 0x11),
            (0x1FF1, 0x22),
            (0x1FF2, 0x33),
            (0x1FF3, 0x44),
        ] {
            write(&mut ram, port, value);
        }
        // Base bit 0 clear: $7C00, i.e. SRAM offset $1C00; object 5 at +$14.
        assert_eq!(&ram[0x1C14..0x1C18], &[0x11, 0x22, 0x33, 0x44]);
        ram[0x1C16] = 0x99;
        assert_eq!(read(&ram, 0x1FF2), 0x99, "ports read the buffer back");
    }

    #[test]
    fn base_bit_selects_7c00_or_7800() {
        let mut ram = sram();
        write(&mut ram, BASE, 0x01);
        write(&mut ram, INDEX, 127);
        write(&mut ram, 0x1FF3, 0xA5);
        assert_eq!(ram[0x1800 + 127 * 4 + 3], 0xA5, "bit 0 set: $7800");
        write(&mut ram, BASE, 0xFE);
        write(&mut ram, 0x1FF3, 0x5A);
        assert_eq!(ram[0x1C00 + 127 * 4 + 3], 0x5A, "only bit 0 counts");
    }

    #[test]
    fn high_table_write_replaces_only_the_objects_two_bits() {
        let mut ram = sram();
        write(&mut ram, INDEX, 5); // high-table byte $200 + 5/4 = $201, bits 2-3
        ram[0x1E01] = 0xFF;
        write(&mut ram, 0x1FF4, 0x02);
        assert_eq!(ram[0x1E01], 0xFB);
        write(&mut ram, INDEX, 127); // byte $200 + 31, bits 6-7
        write(&mut ram, 0x1FF4, 0xFD); // only the low two bits of the value are used
        assert_eq!(ram[0x1E1F], 0x40);
        write(&mut ram, BASE, 0x01);
        write(&mut ram, 0x1FF4, 0x03);
        assert_eq!(ram[0x1A1F], 0xC0, "the high table follows the base");
    }

    #[test]
    fn high_table_read_returns_the_whole_byte() {
        let mut ram = sram();
        write(&mut ram, INDEX, 6);
        ram[0x1E01] = 0xB7;
        assert_eq!(read(&ram, 0x1FF4), 0xB7);
    }

    #[test]
    fn index_is_seven_bits_and_not_incremented() {
        let mut ram = sram();
        write(&mut ram, INDEX, 0x81); // object 1
        write(&mut ram, 0x1FF0, 0x10);
        write(&mut ram, 0x1FF0, 0x20);
        assert_eq!(ram[0x1C04], 0x20, "the same object twice");
        assert_eq!(ram[0x1C08], 0x00);
        assert_eq!(read(&ram, INDEX), 0x81, "the index byte is plain SRAM");
    }

    #[test]
    fn other_addresses_are_plain_sram() {
        let mut ram = sram();
        write(&mut ram, 0x0123, 0x77);
        write(&mut ram, 0x1FF7, 0x0A);
        write(&mut ram, 0x1FFF, 0x42);
        assert_eq!(ram[0x0123], 0x77);
        assert_eq!(read(&ram, 0x1FF7), 0x0A);
        assert_eq!(read(&ram, 0x1FFF), 0x42);
    }

    #[test]
    fn a_smaller_sram_wraps_instead_of_panicking() {
        let mut ram = vec![0; 0x800];
        write(&mut ram, 0x1FF0, 0x33);
        assert_eq!(read(&ram, 0x1FF0), 0x33);
    }
}
