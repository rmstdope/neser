use super::addressing::{
    dma_addr_uses_gamepak, gamepak_nonseq_wait_is_slowest, gamepak_second_access_fast,
    open_bus_no_cart_byte, open_bus_no_cart_halfword, open_bus_no_cart_word, timer_control_index,
    vram_offset,
};
use super::gba_bus::{GbaBus, emit_gba_bus_trace_line};
use super::memory::{
    EWRAM_SIZE, IWRAM_SIZE, OAM_SIZE, PRAM_SIZE, read_le_u16, read_le_u32, write_le_u16,
    write_le_u32,
};
use super::waitstates::WidthClass;
use crate::gba::cpu::bus::Bus;

impl Bus for GbaBus {
    fn read32(&mut self, addr: u32) -> u32 {
        let aligned = addr & !0x3;
        let val = match (aligned >> 24) & 0xF {
            0x0 | 0x1 => self.read_bios_u32(aligned).unwrap_or_else(|| {
                if Self::is_bios_addr(aligned) {
                    self.protected_bios_word()
                } else {
                    self.unused_open_bus_word()
                }
            }),
            0x2 => read_le_u32(&self.ewram, aligned as usize),
            0x3 => read_le_u32(&self.iwram, aligned as usize),
            0x4 => {
                if let Some(value) = self.try_read_mgba_debug16(aligned) {
                    value as u32 | ((value as u32) << 16)
                } else if Self::is_apu_open_bus_read(aligned) {
                    self.io_open_bus_word()
                } else if (0x0400_0060..=0x0400_00A6).contains(&aligned) {
                    let lo = self.apu.read16(aligned) as u32;
                    // Only read the upper halfword if it is also within range.
                    let hi = if aligned + 2 <= 0x0400_00A6 {
                        self.apu.read16(aligned + 2) as u32
                    } else {
                        0
                    };
                    lo | (hi << 16)
                } else {
                    self.io
                        .try_read32(
                            aligned,
                            &self.ic,
                            &self.timers,
                            &self.dma,
                            &self.ppu,
                            &self.keypad,
                        )
                        .unwrap_or_else(|| {
                            if (0x0400_0000..0x0400_0400).contains(&aligned) {
                                self.io_open_bus_word()
                            } else {
                                self.open_bus_word()
                            }
                        })
                }
            }
            0x5 => read_le_u32(&self.pram, aligned as usize),
            0x6 => {
                let off = vram_offset(aligned);
                read_le_u32(&self.vram, off)
            }
            0x7 => read_le_u32(&self.oam, aligned as usize),
            0x8..=0xD => {
                let off = (aligned & 0x01FF_FFFF) as usize;
                self.rom_u32(off)
                    .unwrap_or_else(|| open_bus_no_cart_word(aligned))
            }
            0xE | 0xF => {
                // SRAM is 8-bit only on real hardware; word access mirrors
                // the byte across the word.
                let b = self.cart_read8(addr);
                u32::from_le_bytes([b, b, b, b])
            }
            _ => self.unused_open_bus_word(),
        };
        self.last_bus_value = val;
        val
    }

    fn read16(&mut self, addr: u32) -> u16 {
        let aligned = addr & !0x1;
        let val = match (aligned >> 24) & 0xF {
            0x0 | 0x1 => self.read_bios_u16(aligned).unwrap_or_else(|| {
                if Self::is_bios_addr(aligned) {
                    self.protected_bios_halfword(aligned)
                } else {
                    self.unused_open_bus_halfword(aligned)
                }
            }),
            0x2 => read_le_u16(&self.ewram, aligned as usize),
            0x3 => read_le_u16(&self.iwram, aligned as usize),
            0x4 => {
                if let Some(value) = self.try_read_mgba_debug16(aligned) {
                    value
                } else if Self::is_apu_open_bus_read(aligned) {
                    self.io_open_bus_halfword()
                } else if (0x0400_0060..=0x0400_00A6).contains(&aligned) {
                    self.apu.read16(aligned)
                } else if aligned == 0x0400_0128 {
                    self.sio.read_siocnt()
                } else {
                    let raw = self
                        .io
                        .try_read16(
                            aligned,
                            &self.ic,
                            &self.timers,
                            &self.dma,
                            &self.ppu,
                            &self.keypad,
                        )
                        .unwrap_or_else(|| self.io_open_bus_halfword());
                    // WAITCNT: bits 13, 15 are unused and read as 0.
                    if aligned == 0x0400_0204 {
                        raw & 0x5FFF
                    } else {
                        raw
                    }
                }
            }
            0x5 => read_le_u16(&self.pram, aligned as usize),
            0x6 => {
                let off = vram_offset(aligned);
                read_le_u16(&self.vram, off)
            }
            0x7 => read_le_u16(&self.oam, aligned as usize),
            0x8..=0xD => {
                let off = (aligned & 0x01FF_FFFF) as usize;
                self.rom_u16(off)
                    .unwrap_or_else(|| open_bus_no_cart_halfword(aligned))
            }
            0xE | 0xF => {
                let b = self.cart_read8(addr);
                u16::from_le_bytes([b, b])
            }
            _ => self.unused_open_bus_halfword(aligned),
        };
        // Don't disturb the high half of last_bus_value: only refresh the
        // matching half. Some GBA games rely on the prefetcher's word state.
        let shift = if aligned & 0x2 == 0 { 0 } else { 16 };
        self.last_bus_value =
            (self.last_bus_value & !(0xFFFFu32 << shift)) | ((val as u32) << shift);
        val
    }

    fn read8(&mut self, addr: u32) -> u8 {
        let val = match (addr >> 24) & 0xF {
            0x0 | 0x1 => self.read_bios_byte(addr).unwrap_or_else(|| {
                if Self::is_bios_addr(addr) {
                    self.protected_bios_byte(addr)
                } else {
                    self.unused_open_bus_byte(addr)
                }
            }),
            0x2 => self.ewram[(addr as usize) % EWRAM_SIZE],
            0x3 => self.iwram[(addr as usize) % IWRAM_SIZE],
            0x4 => {
                if let Some(value) = self.try_read_mgba_debug16(addr & !1) {
                    if addr & 1 == 0 {
                        value as u8
                    } else {
                        (value >> 8) as u8
                    }
                } else if addr == 0x0400_0410 {
                    self.undoc_0x410
                } else {
                    let aligned_hw = addr & !0x1;
                    if Self::is_apu_open_bus_read(aligned_hw) {
                        self.io_open_bus_byte(addr)
                    } else if (0x0400_0060..=0x0400_00A6).contains(&aligned_hw) {
                        let hw = self.apu.read16(aligned_hw);
                        if addr & 1 == 0 {
                            hw as u8
                        } else {
                            (hw >> 8) as u8
                        }
                    } else {
                        self.io
                            .try_read8(
                                addr,
                                &self.ic,
                                &self.timers,
                                &self.dma,
                                &self.ppu,
                                &self.keypad,
                            )
                            .unwrap_or_else(|| self.io_open_bus_byte(addr))
                    }
                }
            }
            0x5 => self.pram[(addr as usize) % PRAM_SIZE],
            0x6 => self.vram[vram_offset(addr)],
            0x7 => self.oam[(addr as usize) % OAM_SIZE],
            0x8..=0xD => {
                let off = (addr & 0x01FF_FFFF) as usize;
                self.rom_byte(off).unwrap_or(open_bus_no_cart_byte(addr))
            }
            0xE | 0xF => self.cart_read8(addr),
            _ => self.unused_open_bus_byte(addr),
        };
        let shift = (addr & 3) * 8;
        self.last_bus_value = (self.last_bus_value & !(0xFFu32 << shift)) | ((val as u32) << shift);
        val
    }

    fn fetch32(&mut self, addr: u32) -> u32 {
        let aligned = addr & !0x3;
        let value = if Self::is_bios_addr(aligned) {
            self.raw_bios_u32(aligned)
                .unwrap_or_else(|| self.protected_bios_word())
        } else {
            self.executing_bios = false;
            self.bios_locked = true;
            self.read32(aligned)
        };

        if Self::is_bios_addr(aligned) {
            self.executing_bios = true;
            self.bios_open_bus_value = value;
            self.last_bus_value = value;
        }

        value
    }

    fn fetch16(&mut self, addr: u32) -> u16 {
        let aligned = addr & !0x1;
        let value = if Self::is_bios_addr(aligned) {
            self.raw_bios_u16(aligned)
                .unwrap_or_else(|| self.protected_bios_halfword(aligned))
        } else {
            self.executing_bios = false;
            self.bios_locked = true;
            self.read16(aligned)
        };

        if Self::is_bios_addr(aligned) {
            self.executing_bios = true;
            let shift = if aligned & 0x2 == 0 { 0 } else { 16 };
            self.bios_open_bus_value =
                (self.bios_open_bus_value & !(0xFFFFu32 << shift)) | ((value as u32) << shift);
            self.last_bus_value =
                (self.last_bus_value & !(0xFFFFu32 << shift)) | ((value as u32) << shift);
        }

        value
    }

    fn write32(&mut self, addr: u32, value: u32) {
        let aligned = addr & !0x3;
        let region = (aligned >> 24) & 0xF;
        if region != 0x4 {
            self.last_bus_value = value;
        }
        let touches_io = region == 0x4;
        match region {
            0x0 | 0x1 => { /* BIOS is read-only */ }
            0x2 => write_le_u32(&mut self.ewram, aligned as usize, value),
            0x3 => write_le_u32(&mut self.iwram, aligned as usize, value),
            0x4 => {
                if self.write_mgba_debug32(aligned, value) {
                    return;
                }
                // FIFO A and B need full 32-bit word writes.
                if aligned == 0x0400_00A0 {
                    self.apu.write_fifo_a_word(value);
                } else if aligned == 0x0400_00A4 {
                    self.apu.write_fifo_b_word(value);
                } else if (0x0400_0060..=0x0400_00A6).contains(&aligned) {
                    self.apu.write16(aligned, value as u16);
                    // Only write the upper halfword if it is also within range.
                    if aligned + 2 <= 0x0400_00A6 {
                        self.apu.write16(aligned + 2, (value >> 16) as u16);
                    }
                } else {
                    // Intercept HALTCNT: write32 to 0x04000300 covers POSTFLG (byte 0),
                    // HALTCNT (byte 1), and two unused bytes.
                    if aligned == 0x0400_0300 {
                        let haltcnt_byte = ((value >> 8) & 0xFF) as u8;
                        if haltcnt_byte & 0x80 == 0 {
                            self.halt_requested = true;
                        }
                    }
                    let high = (value >> 16) as u16;
                    let timer_enable_phase = self.timer_enable_phase_for_write16(aligned + 2, high);
                    self.defer_active_timer_reload_write_cycle(aligned);
                    self.prestep_timer_disable_for_write16(aligned + 2, high);
                    self.mark_timer_start_delay_for_write16(aligned + 2, high);
                    self.mark_dma_start_delay_for_write16(aligned + 2, (value >> 16) as u16);
                    self.io.write32(
                        aligned,
                        value,
                        &mut self.ic,
                        &mut self.timers,
                        &mut self.dma,
                        &mut self.ppu,
                        &mut self.keypad,
                    );
                    if let Some((timer, phase)) = timer_enable_phase {
                        self.timers.align_prescaler_phase(timer, phase);
                    }
                    // WAITCNT is at 0x0400_0204; a 32-bit write spans 0x204-0x207.
                    if aligned == 0x0400_0204 {
                        self.waitstates.recalculate(value as u16);
                    }
                    // SIOCNT is at 0x0400_0128 (low halfword of a 32-bit write).
                    if aligned == 0x0400_0128 {
                        self.write_siocnt(value as u16);
                    }
                    // RCNT is at 0x0400_0134 (low halfword of a 32-bit write).
                    if aligned == 0x0400_0134 {
                        self.sio.write_rcnt(value as u16);
                    }
                }
            }
            0x5 => write_le_u32(&mut self.pram, aligned as usize, value),
            0x6 => {
                let off = vram_offset(aligned);
                write_le_u32(&mut self.vram, off, value);
            }
            0x7 => write_le_u32(&mut self.oam, aligned as usize, value),
            0x8..=0xD => { /* Cartridge ROM is read-only via the bus */ }
            0xE | 0xF => {
                // Cart RAM is an 8-bit bus: a 32-bit store writes only the addressed byte lane.
                let shift = (addr & 0x3) * 8;
                let byte = ((value >> shift) & 0xFF) as u8;
                self.cart_write8(addr, byte);
            }
            _ => {}
        }
        if touches_io && self.dma.any_pending() && self.dma_start_delay_cycles == 0 {
            self.run_pending_dma();
        }
    }

    fn write16(&mut self, addr: u32, value: u16) {
        let aligned = addr & !0x1;
        let region = (aligned >> 24) & 0xF;
        if region != 0x4 {
            let shift = if aligned & 0x2 == 0 { 0 } else { 16 };
            self.last_bus_value =
                (self.last_bus_value & !(0xFFFFu32 << shift)) | ((value as u32) << shift);
        }
        let touches_io = region == 0x4;
        match region {
            0x0 | 0x1 => {}
            0x2 => write_le_u16(&mut self.ewram, aligned as usize, value),
            0x3 => write_le_u16(&mut self.iwram, aligned as usize, value),
            0x4 => {
                if self.write_mgba_debug16(aligned, value) {
                    return;
                }
                if (0x0400_0060..=0x0400_00A6).contains(&aligned) {
                    self.apu.write16(aligned, value);
                } else {
                    // Intercept HALTCNT: write16 to 0x04000300 covers POSTFLG (low byte)
                    // and HALTCNT (high byte).
                    if aligned == 0x0400_0300 {
                        let haltcnt_byte = (value >> 8) as u8;
                        if haltcnt_byte & 0x80 == 0 {
                            self.halt_requested = true;
                        }
                    }
                    let timer_enable_phase = self.timer_enable_phase_for_write16(aligned, value);
                    self.defer_active_timer_reload_write_cycle(aligned);
                    self.prestep_timer_disable_for_write16(aligned, value);
                    self.mark_timer_start_delay_for_write16(aligned, value);
                    self.mark_dma_start_delay_for_write16(aligned, value);
                    self.io.write16(
                        aligned,
                        value,
                        &mut self.ic,
                        &mut self.timers,
                        &mut self.dma,
                        &mut self.ppu,
                        &mut self.keypad,
                    );
                    if let Some((timer, phase)) = timer_enable_phase {
                        self.timers.align_prescaler_phase(timer, phase);
                    }
                    self.trace_dma_cnt_h_write(aligned, value);
                    if aligned == 0x0400_0204 {
                        self.waitstates.recalculate(value);
                    }
                    if aligned == 0x0400_0128 {
                        self.write_siocnt(value);
                    }
                    if aligned == 0x0400_0134 {
                        self.sio.write_rcnt(value);
                    }
                }
            }
            0x5 => write_le_u16(&mut self.pram, aligned as usize, value),
            0x6 => {
                let off = vram_offset(aligned);
                write_le_u16(&mut self.vram, off, value);
            }
            0x7 => write_le_u16(&mut self.oam, aligned as usize, value),
            0x8..=0xD => {}
            0xE | 0xF => {
                // Cart RAM is an 8-bit bus: halfword stores write only the addressed byte lane.
                let shift = (addr & 0x1) * 8;
                let byte = ((value as u32 >> shift) & 0xFF) as u8;
                self.cart_write8(addr, byte);
            }
            _ => {}
        }
        if touches_io && self.dma.any_pending() && self.dma_start_delay_cycles == 0 {
            self.run_pending_dma();
        }
    }

    fn write8(&mut self, addr: u32, value: u8) {
        if self.trace_config.bus > 0 {
            emit_gba_bus_trace_line(format!("[GBA BUS] W8 {addr:08X}={value:02X}"));
        }

        let region = (addr >> 24) & 0xF;
        if region != 0x4 {
            let shift = (addr & 3) * 8;
            self.last_bus_value =
                (self.last_bus_value & !(0xFFu32 << shift)) | ((value as u32) << shift);
        }
        let touches_io = region == 0x4;
        match region {
            0x0 | 0x1 => {}
            0x2 => self.ewram[(addr as usize) % EWRAM_SIZE] = value,
            0x3 => self.iwram[(addr as usize) % IWRAM_SIZE] = value,
            0x4 => {
                if self.write_mgba_debug8(addr, value) {
                    return;
                }
                if addr == 0x0400_0410 {
                    self.undoc_0x410 = value;
                } else if addr == 0x0400_0301 {
                    // HALTCNT — bit 7 clear = halt mode, bit 7 set = stop mode (deferred).
                    if value & 0x80 == 0 {
                        self.halt_requested = true;
                    }
                } else if (0x0400_0060..=0x0400_00A7).contains(&addr) {
                    self.apu.write8(addr, value);
                } else {
                    let aligned = addr & !1;
                    let old =
                        self.timers.channels[timer_control_index(aligned).unwrap_or(0)].control;
                    let shift = (addr & 1) * 8;
                    let merged = (old & !(0xFFu16 << shift)) | ((value as u16) << shift);
                    let timer_enable_phase = self.timer_enable_phase_for_write16(aligned, merged);
                    self.prestep_timer_disable_for_write16(aligned, merged);
                    self.mark_timer_start_delay_for_write8(addr, value);
                    self.mark_dma_start_delay_for_write8(addr, value);
                    self.io.write8(
                        addr,
                        value,
                        &mut self.ic,
                        &mut self.timers,
                        &mut self.dma,
                        &mut self.ppu,
                        &mut self.keypad,
                    );
                    if let Some((timer, phase)) = timer_enable_phase {
                        self.timers.align_prescaler_phase(timer, phase);
                    }
                    if aligned == 0x0400_0204 {
                        self.waitstates
                            .recalculate(self.io.backing_u16(0x0400_0204));
                    }
                    // Byte writes to SIOCNT/RCNT must update the Sio module.
                    // Merge the written byte into the current register value.
                    if aligned == 0x0400_0128 {
                        let merged = self.io.backing_u16(0x0400_0128);
                        self.write_siocnt(merged);
                    }
                    if aligned == 0x0400_0134 {
                        let merged = self.io.backing_u16(0x0400_0134);
                        self.sio.write_rcnt(merged);
                    }
                }
            }
            0x5 => {
                // Byte writes to PRAM duplicate the byte to a halfword.
                let off = (addr as usize & !1) % PRAM_SIZE;
                self.pram[off] = value;
                self.pram[off + 1] = value;
            }
            0x6 => {
                // Byte writes to BG VRAM duplicate the byte to a halfword.
                // Byte writes to OBJ VRAM (offset >= 0x10000) are ignored.
                // TODO: In bitmap modes 3/5, the BG/OBJ boundary is 0x14000
                // rather than 0x10000. This threshold is correct for tile modes
                // (0-2) and mode 4, but needs DISPCNT check for full accuracy.
                let off = vram_offset(addr);
                if off < 0x10000 {
                    let aligned = off & !1;
                    self.vram[aligned] = value;
                    self.vram[aligned + 1] = value;
                }
            }
            0x7 => { /* OAM ignores byte writes */ }
            0x8..=0xD => {}
            0xE | 0xF => self.cart_write8(addr, value),
            _ => {}
        }
        if touches_io && self.dma.any_pending() && self.dma_start_delay_cycles == 0 {
            self.run_pending_dma();
        }
    }

    fn n_cycles(&self, addr: u32, width: WidthClass) -> u32 {
        self.n_cycles_width(addr, width)
    }

    fn s_cycles(&self, addr: u32, width: WidthClass) -> u32 {
        self.s_cycles_width(addr, width)
    }

    fn gamepak_prefetch_enabled(&self) -> bool {
        self.waitstates.prefetch_enabled
    }

    fn immediate_gamepak_dma_prefetch_penalty(&self, code_width: WidthClass) -> bool {
        if self.dma_start_delay_cycles == 0 {
            return false;
        }
        let Some((src, dst)) = self.dma.pending_immediate_src_dst() else {
            return false;
        };
        let src_gamepak = dma_addr_uses_gamepak(src);
        let dst_gamepak = dma_addr_uses_gamepak(dst);
        let src_second_fast = gamepak_second_access_fast(self.waitstates.waitcnt, src);
        let dst_second_fast = gamepak_second_access_fast(self.waitstates.waitcnt, dst);

        // mGBA Timing shows a one-cycle arbitration bubble when immediate
        // DMA steals the Game Pak bus from the opcode prefetcher.
        (src_gamepak && code_width == WidthClass::Word && src_second_fast)
            || (!src_gamepak && dst_gamepak && !dst_second_fast)
    }

    fn embedded_bios_hle_enabled(&self) -> bool {
        self.embedded_bios_loaded
    }

    fn embedded_bios_hle_entry_penalty(&self, addr: u32, width: WidthClass) -> u32 {
        match (addr >> 24) & 0xF {
            0x2 => self.n_cycles_width(addr, width) + 2 * self.s_cycles_width(addr, width) - 1,
            0x8..=0xD => {
                let n = self.n_cycles_width(addr, width);
                let s = self.s_cycles_width(addr, width);
                n + 2 * s
                    + u32::from(gamepak_nonseq_wait_is_slowest(
                        self.waitstates.waitcnt,
                        addr,
                    ))
            }
            _ => 0,
        }
    }
}
