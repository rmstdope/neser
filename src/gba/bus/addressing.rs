pub(super) fn dma_cnt_h_channel(addr: u32) -> Option<usize> {
    if !(0x0400_00B0..=0x0400_00DF).contains(&addr) {
        return None;
    }
    let rel = addr - 0x0400_00B0;
    (rel % 12 == 10).then_some((rel / 12) as usize)
}
pub(super) fn vram_offset(addr: u32) -> usize {
    let local = (addr as usize) & 0x1FFFF; // 128 KB period
    if local < 0x18000 {
        local
    } else {
        // Mirror upper 32 KB of VRAM into the second half of each 128 KB
        // window.
        0x10000 + (local & 0x7FFF)
    }
}

pub(super) fn timer_control_index(addr: u32) -> Option<usize> {
    match addr {
        0x0400_0102 => Some(0),
        0x0400_0106 => Some(1),
        0x0400_010A => Some(2),
        0x0400_010E => Some(3),
        _ => None,
    }
}

pub(super) fn dma_control_index(addr: u32) -> Option<usize> {
    match addr {
        0x0400_00BA => Some(0),
        0x0400_00C6 => Some(1),
        0x0400_00D2 => Some(2),
        0x0400_00DE => Some(3),
        _ => None,
    }
}

pub(super) fn dma_addr_uses_gamepak(addr: u32) -> bool {
    matches!((addr >> 24) & 0xF, 0x8..=0xD)
}

pub(super) fn gamepak_second_access_fast(waitcnt: u16, addr: u32) -> bool {
    let bit = match (addr >> 24) & 0xF {
        0x8 | 0x9 => 4,
        0xA | 0xB => 7,
        0xC | 0xD => 10,
        _ => return false,
    };
    waitcnt & (1 << bit) != 0
}

pub(super) fn gamepak_nonseq_wait_is_slowest(waitcnt: u16, addr: u32) -> bool {
    let shift = match (addr >> 24) & 0xF {
        0x8 | 0x9 => 2,
        0xA | 0xB => 5,
        0xC | 0xD => 8,
        _ => return false,
    };
    ((waitcnt >> shift) & 0x3) == 0
}

/// Reading from cartridge ROM with no cart inserted returns the lower half of
/// `addr / 2` (GBATek "open bus" rule for the cart bus).
pub(super) fn open_bus_no_cart_word(addr: u32) -> u32 {
    let lo = open_bus_no_cart_halfword(addr) as u32;
    let hi = open_bus_no_cart_halfword(addr.wrapping_add(2)) as u32;
    lo | (hi << 16)
}

pub(super) fn open_bus_no_cart_halfword(addr: u32) -> u16 {
    ((addr >> 1) & 0xFFFF) as u16
}

pub(super) fn open_bus_no_cart_byte(addr: u32) -> u8 {
    let hw = open_bus_no_cart_halfword(addr & !1);
    if addr & 1 == 0 {
        hw as u8
    } else {
        (hw >> 8) as u8
    }
}
