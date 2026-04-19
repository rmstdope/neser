//! GB save-state serialization.
//!
//! Provides [`GbSaveState`], a fully serde-able snapshot of the DMG emulator.
//! Mirrors the NES approach: each component contributes a small serde-friendly
//! struct, and [`Gb<DmgBus>`](super::Gb) provides `save_state()` / `load_state()`
//! methods to capture and restore the full state.

use serde::{Deserialize, Serialize};

const GB_SAVESTATE_VERSION: u32 = 1;

/// Complete DMG emulator state snapshot.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GbSaveState {
    pub version: u32,
    pub cpu: CpuSnapshot,
    pub ppu: PpuSnapshot,
    pub timer: TimerSnapshot,
    pub joypad: JoypadSnapshot,
    pub apu: ApuSnapshot,
    pub bus: BusSnapshot,
    pub cartridge: Vec<u8>,
}

impl GbSaveState {
    /// Serialize the save state to JSON-encoded UTF-8 bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Deserialize a save state from JSON-encoded UTF-8 bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

// ── CPU snapshot ────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CpuSnapshot {
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,
    pub ime: bool,
    pub halted: bool,
    pub halt_bug: bool,
    pub ime_pending: bool,
    pub cycles: u64,
}

// ── PPU snapshot ────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PpuSnapshot {
    pub vram: Vec<u8>,
    pub oam: Vec<u8>,
    pub screen_buffer: Vec<u8>,
    /// Registers: lcdc, stat_irq_enables, scy, scx, lyc, bgp, obp0, obp1, wy, wx
    pub registers: [u8; 10],
    /// Timing state
    pub dot: u16,
    pub scanline: u8,
    pub mode: u8,
    pub stat_mode: u8,
    pub frame_ready: bool,
    pub first_scanline_after_enable: bool,
    pub second_scanline_after_enable: bool,
    pub third_scanline_after_enable: bool,
    pub mode_for_irq: i8,
    pub mode3_extra_dots: u16,
    pub ly: u8,
    /// Other PPU fields
    pub pending_interrupts: u8,
    pub window_line: u8,
    pub prev_stat_irq_line: bool,
    pub lyc_eq_ly_frozen: bool,
}

// ── Timer snapshot ──────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TimerSnapshot {
    pub div_counter: u16,
    pub tima: u8,
    pub tma: u8,
    pub tac: u8,
    pub interrupt_pending: bool,
    pub tima_overflow_pending: bool,
    pub tima_load_active: bool,
}

// ── Timing restore args (avoids too-many-arguments clippy warning) ─────────

/// Grouped arguments for [`Timing::restore`] to satisfy clippy's
/// `too_many_arguments` lint.
pub struct TimingRestoreArgs {
    pub dot: u16,
    pub scanline: u8,
    pub mode: u8,
    pub stat_mode: u8,
    pub frame_ready: bool,
    pub first_scanline: bool,
    pub second_scanline: bool,
    pub third_scanline: bool,
    pub mode_for_irq: i8,
    pub mode3_extra_dots: u16,
    pub ly: u8,
}

// ── Joypad snapshot ─────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JoypadSnapshot {
    pub select_bits: u8,
    pub p14_state: u8,
    pub p15_state: u8,
    pub prev_nibble: u8,
}

// ── APU snapshot ────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApuSnapshot {
    pub ch1: Channel1Snapshot,
    pub ch2: Channel2Snapshot,
    pub ch3: Channel3Snapshot,
    pub ch4: Channel4Snapshot,
    pub nr50: u8,
    pub nr51: u8,
    pub powered: bool,
    pub fs_timer: u16,
    pub fs_step: u8,
    pub sample_acc: f32,
    pub cycles_per_sample: f32,
    pub is_cgb: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Channel1Snapshot {
    pub sweep_period: u8,
    pub sweep_negate: bool,
    pub sweep_shift: u8,
    pub duty: u8,
    pub length_load: u8,
    pub init_volume: u8,
    pub env_add: bool,
    pub env_period: u8,
    pub freq: u16,
    pub length_en: bool,
    pub active: bool,
    pub dac_on: bool,
    pub duty_pos: u8,
    pub freq_timer: u16,
    pub length_counter: u8,
    pub volume: u8,
    pub env_timer: u8,
    pub sweep_timer: u8,
    pub sweep_shadow: u16,
    pub sweep_enabled: bool,
    pub negate_used: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Channel2Snapshot {
    pub duty: u8,
    pub length_load: u8,
    pub init_volume: u8,
    pub env_add: bool,
    pub env_period: u8,
    pub freq: u16,
    pub length_en: bool,
    pub active: bool,
    pub dac_on: bool,
    pub duty_pos: u8,
    pub freq_timer: u16,
    pub length_counter: u8,
    pub volume: u8,
    pub env_timer: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Channel3Snapshot {
    pub dac_on: bool,
    pub length_load: u16,
    pub output_level: u8,
    pub freq: u16,
    pub length_en: bool,
    pub active: bool,
    pub wave_pos: u8,
    pub freq_timer: u16,
    pub length_counter: u16,
    pub wave_ram: Vec<u8>,
    pub current_sample: u8,
    pub is_cgb: bool,
    pub wave_just_read: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Channel4Snapshot {
    pub init_volume: u8,
    pub env_add: bool,
    pub env_period: u8,
    pub clock_shift: u8,
    pub lfsr_7bit: bool,
    pub divisor_code: u8,
    pub length_load: u8,
    pub length_en: bool,
    pub active: bool,
    pub dac_on: bool,
    pub lfsr: u16,
    pub freq_timer: u32,
    pub length_counter: u8,
    pub volume: u8,
    pub env_timer: u8,
}

// ── Bus snapshot ────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BusSnapshot {
    pub wram: Vec<u8>,
    pub hram: Vec<u8>,
    pub if_reg: u8,
    pub ie_reg: u8,
    pub boot_rom_active: bool,
    pub dma_active: bool,
    pub dma_source: u8,
    pub dma_position: u8,
    pub dma_oam_blocked: bool,
    pub sb: u8,
    pub sc: u8,
    pub serial_bits_remaining: u8,
    pub serial_master_clock: bool,
    pub model: u8,
}

// ── Snapshot/Restore implementations ────────────────────────────────────────

use crate::gb::apu::channel1::Channel1;
use crate::gb::apu::channel2::Channel2;
use crate::gb::apu::channel3::Channel3;
use crate::gb::apu::channel4::Channel4;
use crate::gb::bus::DmgBus;
use crate::gb::cpu::Sm83;
use crate::gb::input::joypad::Joypad;
use crate::gb::ppu::Ppu;

use super::Gb;

impl Gb<DmgBus> {
    /// Capture a full save-state snapshot.
    pub fn save_state(&self) -> GbSaveState {
        GbSaveState {
            version: GB_SAVESTATE_VERSION,
            cpu: self.cpu.snapshot(),
            ppu: self.cpu.bus.ppu.snapshot(),
            timer: self.cpu.bus.timer_snapshot(),
            joypad: self.cpu.bus.joypad.snapshot(),
            apu: self.cpu.bus.apu_snapshot(),
            bus: self.cpu.bus.bus_snapshot(),
            cartridge: self.cpu.bus.cart_save_state(),
        }
    }

    /// Restore state from a save-state snapshot.
    pub fn load_state(&mut self, state: &GbSaveState) -> Result<(), String> {
        if state.version != GB_SAVESTATE_VERSION {
            return Err(format!(
                "Save state version mismatch: expected {GB_SAVESTATE_VERSION}, got {}",
                state.version
            ));
        }
        self.cpu.restore(&state.cpu);
        self.cpu.bus.ppu.restore(&state.ppu)?;
        self.cpu.bus.restore_timer(&state.timer);
        self.cpu.bus.joypad.restore(&state.joypad);
        self.cpu.bus.restore_apu(&state.apu);
        self.cpu.bus.restore_bus(&state.bus)?;
        self.cpu.bus.cart_load_state(&state.cartridge)?;
        Ok(())
    }
}

// ── CPU snapshot/restore ────────────────────────────────────────────────────

impl<B: crate::gb::bus::GbBus> Sm83<B> {
    pub(crate) fn snapshot(&self) -> CpuSnapshot {
        CpuSnapshot {
            a: self.regs.a,
            f: self.regs.f,
            b: self.regs.b,
            c: self.regs.c,
            d: self.regs.d,
            e: self.regs.e,
            h: self.regs.h,
            l: self.regs.l,
            sp: self.regs.sp,
            pc: self.regs.pc,
            ime: self.ime,
            halted: self.halted,
            halt_bug: self.halt_bug,
            ime_pending: self.ime_pending(),
            cycles: self.cycles(),
        }
    }

    pub(crate) fn restore(&mut self, snap: &CpuSnapshot) {
        self.regs.a = snap.a;
        self.regs.f = snap.f;
        self.regs.b = snap.b;
        self.regs.c = snap.c;
        self.regs.d = snap.d;
        self.regs.e = snap.e;
        self.regs.h = snap.h;
        self.regs.l = snap.l;
        self.regs.sp = snap.sp;
        self.regs.pc = snap.pc;
        self.ime = snap.ime;
        self.halted = snap.halted;
        self.halt_bug = snap.halt_bug;
        self.set_ime_pending(snap.ime_pending);
        self.set_cycles(snap.cycles);
    }
}

// ── PPU snapshot/restore ────────────────────────────────────────────────────

impl Ppu {
    pub(crate) fn snapshot(&self) -> PpuSnapshot {
        PpuSnapshot {
            vram: self.vram.to_vec(),
            oam: self.oam.to_vec(),
            screen_buffer: self.screen_buffer().snapshot(),
            registers: self.registers_snapshot(),
            dot: self.timing_dot(),
            scanline: self.timing_scanline(),
            mode: self.timing_mode(),
            stat_mode: self.timing_stat_mode(),
            frame_ready: self.is_frame_ready(),
            first_scanline_after_enable: self.timing_first_scanline(),
            second_scanline_after_enable: self.timing_second_scanline(),
            third_scanline_after_enable: self.timing_third_scanline(),
            mode_for_irq: self.timing_mode_for_irq(),
            mode3_extra_dots: self.timing_mode3_extra_dots(),
            ly: self.timing_ly(),
            pending_interrupts: self.pending_interrupts_raw(),
            window_line: self.window_line_raw(),
            prev_stat_irq_line: self.prev_stat_irq_line_raw(),
            lyc_eq_ly_frozen: self.lyc_eq_ly_frozen_raw(),
        }
    }

    pub(crate) fn restore(&mut self, snap: &PpuSnapshot) -> Result<(), String> {
        if snap.vram.len() != self.vram.len() {
            return Err(format!(
                "VRAM size mismatch: expected {}, got {}",
                self.vram.len(),
                snap.vram.len()
            ));
        }
        self.vram.copy_from_slice(&snap.vram);
        if snap.oam.len() != self.oam.len() {
            return Err(format!(
                "OAM size mismatch: expected {}, got {}",
                self.oam.len(),
                snap.oam.len()
            ));
        }
        self.oam.copy_from_slice(&snap.oam);
        self.restore_screen_buffer(&snap.screen_buffer)?;
        self.restore_registers(&snap.registers);
        self.restore_timing(snap)?;
        self.set_pending_interrupts(snap.pending_interrupts);
        self.set_window_line(snap.window_line);
        self.set_prev_stat_irq_line(snap.prev_stat_irq_line);
        self.set_lyc_eq_ly_frozen(snap.lyc_eq_ly_frozen);
        Ok(())
    }
}

// ── Joypad snapshot/restore ─────────────────────────────────────────────────

impl Joypad {
    pub(crate) fn snapshot(&self) -> JoypadSnapshot {
        JoypadSnapshot {
            select_bits: self.select_bits_raw(),
            p14_state: self.p14_state_raw(),
            p15_state: self.p15_state_raw(),
            prev_nibble: self.prev_nibble_raw(),
        }
    }

    pub(crate) fn restore(&mut self, snap: &JoypadSnapshot) {
        self.set_select_bits(snap.select_bits);
        self.set_p14_state(snap.p14_state);
        self.set_p15_state(snap.p15_state);
        self.set_prev_nibble(snap.prev_nibble);
    }
}

// ── APU channel snapshot/restore ────────────────────────────────────────────

impl Channel1 {
    pub(crate) fn snapshot(&self) -> Channel1Snapshot {
        Channel1Snapshot {
            sweep_period: self.sweep_period_raw(),
            sweep_negate: self.sweep_negate_raw(),
            sweep_shift: self.sweep_shift_raw(),
            duty: self.duty_raw(),
            length_load: self.length_load_raw(),
            init_volume: self.init_volume_raw(),
            env_add: self.env_add_raw(),
            env_period: self.env_period_raw(),
            freq: self.freq_raw(),
            length_en: self.length_en(),
            active: self.is_active(),
            dac_on: self.dac_on_raw(),
            duty_pos: self.duty_pos_raw(),
            freq_timer: self.freq_timer_raw(),
            length_counter: self.length_counter,
            volume: self.volume_raw(),
            env_timer: self.env_timer_raw(),
            sweep_timer: self.sweep_timer_raw(),
            sweep_shadow: self.sweep_shadow_raw(),
            sweep_enabled: self.sweep_enabled_raw(),
            negate_used: self.negate_used_raw(),
        }
    }

    pub(crate) fn restore(&mut self, snap: &Channel1Snapshot) {
        self.restore_from_snapshot(snap);
    }
}

impl Channel2 {
    pub(crate) fn snapshot(&self) -> Channel2Snapshot {
        Channel2Snapshot {
            duty: self.duty_raw(),
            length_load: self.length_load_raw(),
            init_volume: self.init_volume_raw(),
            env_add: self.env_add_raw(),
            env_period: self.env_period_raw(),
            freq: self.freq_raw(),
            length_en: self.length_en(),
            active: self.is_active(),
            dac_on: self.dac_on_raw(),
            duty_pos: self.duty_pos_raw(),
            freq_timer: self.freq_timer_raw(),
            length_counter: self.length_counter,
            volume: self.volume_raw(),
            env_timer: self.env_timer_raw(),
        }
    }

    pub(crate) fn restore(&mut self, snap: &Channel2Snapshot) {
        self.restore_from_snapshot(snap);
    }
}

impl Channel3 {
    pub(crate) fn snapshot(&self) -> Channel3Snapshot {
        Channel3Snapshot {
            dac_on: self.dac_on_raw(),
            length_load: self.length_load_raw(),
            output_level: self.output_level_raw(),
            freq: self.freq_raw(),
            length_en: self.length_en(),
            active: self.is_active(),
            wave_pos: self.wave_pos_raw(),
            freq_timer: self.freq_timer_raw(),
            length_counter: self.length_counter,
            wave_ram: self.wave_ram_raw().to_vec(),
            current_sample: self.current_sample,
            is_cgb: self.is_cgb_raw(),
            wave_just_read: self.wave_just_read,
        }
    }

    pub(crate) fn restore(&mut self, snap: &Channel3Snapshot) {
        self.restore_from_snapshot(snap);
    }
}

impl Channel4 {
    pub(crate) fn snapshot(&self) -> Channel4Snapshot {
        Channel4Snapshot {
            init_volume: self.init_volume_raw(),
            env_add: self.env_add_raw(),
            env_period: self.env_period_raw(),
            clock_shift: self.clock_shift_raw(),
            lfsr_7bit: self.lfsr_7bit_raw(),
            divisor_code: self.divisor_code_raw(),
            length_load: self.length_load_raw(),
            length_en: self.length_en(),
            active: self.is_active(),
            dac_on: self.dac_on_raw(),
            lfsr: self.lfsr_raw(),
            freq_timer: self.freq_timer_raw(),
            length_counter: self.length_counter,
            volume: self.volume_raw(),
            env_timer: self.env_timer_raw(),
        }
    }

    pub(crate) fn restore(&mut self, snap: &Channel4Snapshot) {
        self.restore_from_snapshot(snap);
    }
}
