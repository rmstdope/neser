//! SNES APU (Audio Processing Unit) bootstrap path.
//!
//! This slice wires the 64 KB ARAM, clean-room IPL boot ROM overlay, SPC700 CPU,
//! and the four communication ports (`$2140-$2143` <-> `$F4-$F7`).

use crate::trace_apu;

pub mod dsp;
pub mod ipl;
pub mod spc700;
pub mod timers;

use crate::snes::apu::dsp::Sdsp;
use serde::{Deserialize, Serialize};
use spc700::{Spc700, Spc700Bus};
use std::collections::VecDeque;
use timers::SpcTimers;

const ARAM_SIZE: usize = 0x1_0000;
const MAX_PENDING_SAMPLES: usize = 16_384;
const SNES_MASTER_CLOCK_HZ: f32 = 21_477_272.0;
const NATIVE_AUDIO_SAMPLE_RATE_HZ: f32 = 32_000.0;
const SPC_PER_MASTER_NUM: i64 = 1_024_000;
const SPC_PER_MASTER_DEN: i64 = 21_477_272;

fn default_test_reg() -> u8 {
    0x0A
}

fn default_pending_samples() -> Vec<(f32, f32)> {
    Vec::new()
}

fn sanitize_non_negative_f32(value: f32) -> f32 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        0.0
    }
}

fn sanitize_positive_f32(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SnesApuState {
    #[serde(default)]
    pub aram: Vec<u8>,
    #[serde(default)]
    pub main_to_spc_ports: [u8; 4],
    #[serde(default)]
    pub spc_to_main_ports: [u8; 4],
    #[serde(default)]
    pub control: u8,
    #[serde(default = "default_test_reg")]
    pub test: u8,
    #[serde(default)]
    pub master_ticks: u64,
    #[serde(default)]
    pub spc_cycle_budget: i64,
    #[serde(default)]
    pub timers: SpcTimers,
    #[serde(default)]
    pub dsp: Sdsp,
    #[serde(default)]
    pub dsp_addr: u8,
    #[serde(default)]
    pub spc700: spc700::Spc700State,
    #[serde(default)]
    pub sample_acc: f32,
    #[serde(default)]
    pub cycles_per_sample: f32,
    #[serde(default)]
    pub native_sample_acc: f32,
    #[serde(default)]
    pub native_cycles_per_sample: f32,
    #[serde(default)]
    pub resample_phase: f32,
    #[serde(default = "default_pending_samples")]
    pub pending_samples: Vec<(f32, f32)>,
}

/// SNES APU bootstrap model: SPC700 + ARAM + IPL overlay + communication ports.
#[derive(Debug, Clone)]
pub struct SnesApu {
    spc700: Spc700,
    aram: [u8; ARAM_SIZE],
    ipl: [u8; 64],
    main_to_spc_ports: [u8; 4],
    spc_to_main_ports: [u8; 4],
    /// Mirrors `$F1` control bits. Bit 7 toggles IPL overlay at `$FFC0-$FFFF`.
    control: u8,
    /// Mirrors `$F0` test bits.
    test: u8,
    /// `true` once the SPC700 has deadlocked after selecting the glitchy
    /// internal-speed divider (value 2). See [`SpcBusView::internal_speed_freeze`].
    spc_frozen: bool,
    timers: SpcTimers,
    master_ticks: u64,
    /// Signed budget in numerator units for fractional SPC catch-up.
    spc_cycle_budget: i64,
    dsp: Sdsp,
    dsp_addr: u8,
    sample_acc: f32,
    cycles_per_sample: f32,
    native_sample_acc: f32,
    native_cycles_per_sample: f32,
    native_samples: VecDeque<(f32, f32)>,
    resample_phase: f32,
    pending_samples: VecDeque<(f32, f32)>,
}

impl SnesApu {
    pub fn new(ipl: Option<[u8; 64]>) -> Self {
        let mut apu = Self {
            spc700: Spc700::new(),
            aram: [0; ARAM_SIZE],
            ipl: ipl.unwrap_or(ipl::EMBEDDED_IPL),
            main_to_spc_ports: [0; 4],
            spc_to_main_ports: [0; 4],
            control: 0xB0,
            test: default_test_reg(),
            spc_frozen: false,
            timers: SpcTimers::default(),
            master_ticks: 0,
            spc_cycle_budget: 0,
            dsp: Sdsp::new(),
            dsp_addr: 0,
            sample_acc: 0.0,
            cycles_per_sample: 0.0,
            native_sample_acc: 0.0,
            native_cycles_per_sample: SNES_MASTER_CLOCK_HZ / NATIVE_AUDIO_SAMPLE_RATE_HZ,
            native_samples: VecDeque::with_capacity(MAX_PENDING_SAMPLES),
            resample_phase: 0.0,
            pending_samples: VecDeque::with_capacity(MAX_PENDING_SAMPLES),
        };
        apu.aram[0x00F8] = 0x00;
        apu.aram[0x00F9] = 0x00;
        apu.reset_spc700();
        apu
    }

    pub fn read_main_port(&self, port: usize) -> u8 {
        self.spc_to_main_ports[port]
    }

    pub fn write_main_port(&mut self, port: usize, value: u8) {
        trace_apu!(3; "CPU->SPC port[{}] <= ${:02X}", port, value);
        self.main_to_spc_ports[port] = value;
    }

    pub fn read_spc_port(&self, port: usize) -> u8 {
        self.main_to_spc_ports[port]
    }

    pub fn write_spc_port(&mut self, port: usize, value: u8) {
        self.spc_to_main_ports[port] = value;
    }

    pub fn tick(&mut self) {
        self.master_ticks = self.master_ticks.wrapping_add(1);
        self.spc_cycle_budget += SPC_PER_MASTER_NUM;

        while self.spc_cycle_budget >= SPC_PER_MASTER_DEN {
            // The SPC700 deadlocks permanently once the glitchy internal-speed
            // divider (value 2) has been engaged during an internal access
            // (blargg `speed_2_freezes`). Keep draining the budget so the rest
            // of the system advances, but never execute the halted core.
            if self.spc_frozen {
                self.spc_cycle_budget -= SPC_PER_MASTER_DEN;
                continue;
            }

            // Cycle-scripted dispatch (per-SPC-cycle stepping) is used for
            // the blargg IPL-hack trampoline (#2908), where opcodes execute
            // directly out of the I/O port region $00F4-$00F7. Restricting
            // cycle-stepping to that PC range preserves the timing of all
            // normal SPC code (which lives in ARAM or the IPL boot ROM),
            // avoiding regressions in already-passing tests.
            let use_cycle_stepper = if self.spc700.has_in_progress_op() {
                true
            } else {
                let next_pc = self.spc700.pc();
                let opcode = self.peek_opcode_at(next_pc);
                Self::pc_is_in_trampoline_region(next_pc)
                    && Spc700::opcode_is_cycle_scripted(opcode)
            };

            let consumed_cycles = {
                let mut bus_view = SpcBusView {
                    aram: &mut self.aram,
                    ipl: &self.ipl,
                    main_to_spc_ports: &mut self.main_to_spc_ports,
                    spc_to_main_ports: &mut self.spc_to_main_ports,
                    control: &mut self.control,
                    test: &mut self.test,
                    timers: &mut self.timers,
                    dsp: &mut self.dsp,
                    dsp_addr: &mut self.dsp_addr,
                    frozen: &mut self.spc_frozen,
                    tick_timers: true,
                };
                if use_cycle_stepper {
                    self.spc700.step_one_cycle(&mut bus_view);
                    1i64
                } else {
                    i64::from(self.spc700.step(&mut bus_view))
                }
            };
            self.spc_cycle_budget -= consumed_cycles * SPC_PER_MASTER_DEN;
        }

        self.step_audio_clock();
    }

    /// `true` when `pc` points into the I/O port window the blargg IPL-hack
    /// trampoline executes from. Used to gate cycle-accurate stepping to the
    /// trampoline case without regressing timing for normal ARAM/IPL code.
    fn pc_is_in_trampoline_region(pc: u16) -> bool {
        matches!(pc, 0x00F4..=0x00F7)
    }

    /// Read the byte at `addr` from the SPC700 address space without ticking
    /// timers or moving any other state. Used by the cycle-stepper dispatcher
    /// in [`Self::tick`] to peek the next opcode and decide between cycle and
    /// atomic stepping; safe because every region the SPC can have its PC in
    /// (ARAM, IPL ROM overlay, ports `$F4-$F7`) returns a value with no read
    /// side effects.
    fn peek_opcode_at(&self, addr: u16) -> u8 {
        match addr {
            0x00F4..=0x00F7 => self.main_to_spc_ports[(addr - 0x00F4) as usize],
            0xFFC0..=0xFFFF if self.control & 0x80 != 0 => self.ipl[(addr - 0xFFC0) as usize],
            _ => self.aram[addr as usize],
        }
    }

    #[cfg(test)]
    pub(crate) fn spc_pc_for_debug(&self) -> u16 {
        self.spc700.pc()
    }

    #[cfg(test)]
    pub(crate) fn peek_spc_memory_for_debug(&self, addr: u16) -> u8 {
        self.peek_opcode_at(addr)
    }

    #[cfg(test)]
    pub(crate) fn main_to_spc_ports_for_debug(&self) -> [u8; 4] {
        self.main_to_spc_ports
    }

    #[cfg(test)]
    pub(crate) fn spc_to_main_ports_for_debug(&self) -> [u8; 4] {
        self.spc_to_main_ports
    }

    pub fn master_ticks(&self) -> u64 {
        self.master_ticks
    }

    pub fn capture_state(&self) -> SnesApuState {
        SnesApuState {
            aram: self.aram.to_vec(),
            main_to_spc_ports: self.main_to_spc_ports,
            spc_to_main_ports: self.spc_to_main_ports,
            control: self.control,
            test: self.test,
            master_ticks: self.master_ticks,
            spc_cycle_budget: self.spc_cycle_budget,
            timers: self.timers.clone(),
            dsp: self.dsp.clone(),
            dsp_addr: self.dsp_addr,
            spc700: self.spc700.capture_state(),
            sample_acc: self.sample_acc,
            cycles_per_sample: self.cycles_per_sample,
            native_sample_acc: self.native_sample_acc,
            native_cycles_per_sample: self.native_cycles_per_sample,
            resample_phase: self.resample_phase,
            pending_samples: self.pending_samples.iter().copied().collect(),
        }
    }

    pub fn restore_state(&mut self, state: &SnesApuState) -> Result<(), String> {
        if !state.aram.is_empty() && state.aram.len() != ARAM_SIZE {
            return Err(format!(
                "APU ARAM size mismatch (expected {ARAM_SIZE}, found {})",
                state.aram.len()
            ));
        }
        if state.aram.is_empty() {
            // Backward-compat: older save-states didn't include APU ARAM/control.
            self.aram = [0; ARAM_SIZE];
            self.aram[0x00F8] = 0x00;
            self.aram[0x00F9] = 0x00;
            self.main_to_spc_ports = [0; 4];
            self.spc_to_main_ports = [0; 4];
            self.control = 0xB0;
            self.test = default_test_reg();
            self.master_ticks = 0;
            self.spc_cycle_budget = 0;
            self.timers = SpcTimers::default();
            self.dsp = Sdsp::new();
            self.dsp_addr = 0;
            self.sample_acc = 0.0;
            self.cycles_per_sample = 0.0;
            self.native_sample_acc = 0.0;
            self.native_cycles_per_sample = SNES_MASTER_CLOCK_HZ / NATIVE_AUDIO_SAMPLE_RATE_HZ;
            self.native_samples.clear();
            self.resample_phase = 0.0;
            self.pending_samples.clear();
            self.reset_spc700();
            return Ok(());
        }

        if state.aram.len() == ARAM_SIZE {
            self.aram.copy_from_slice(&state.aram);
        }
        let mut normalized_dsp = state.dsp.clone();
        normalized_dsp.normalize_after_restore()?;
        let restored_dsp_addr = state.dsp_addr;

        self.main_to_spc_ports = state.main_to_spc_ports;
        self.spc_to_main_ports = state.spc_to_main_ports;
        self.control = state.control;
        self.test = state.test;
        self.master_ticks = state.master_ticks;
        self.spc_cycle_budget = state.spc_cycle_budget;
        self.timers = state.timers.clone();
        self.dsp = normalized_dsp;
        self.dsp_addr = restored_dsp_addr;
        self.sample_acc = sanitize_non_negative_f32(state.sample_acc);
        self.cycles_per_sample = sanitize_non_negative_f32(state.cycles_per_sample);
        self.native_sample_acc = sanitize_non_negative_f32(state.native_sample_acc);
        self.native_cycles_per_sample = sanitize_positive_f32(
            state.native_cycles_per_sample,
            SNES_MASTER_CLOCK_HZ / NATIVE_AUDIO_SAMPLE_RATE_HZ,
        );
        self.resample_phase = sanitize_non_negative_f32(state.resample_phase);
        self.native_samples.clear();
        self.pending_samples = state
            .pending_samples
            .iter()
            .copied()
            .take(MAX_PENDING_SAMPLES)
            .collect();
        self.spc700.restore_state(&state.spc700);
        Ok(())
    }

    fn reset_spc700(&mut self) {
        self.spc_frozen = false;
        let mut bus_view = SpcBusView {
            aram: &mut self.aram,
            ipl: &self.ipl,
            main_to_spc_ports: &mut self.main_to_spc_ports,
            spc_to_main_ports: &mut self.spc_to_main_ports,
            control: &mut self.control,
            test: &mut self.test,
            timers: &mut self.timers,
            dsp: &mut self.dsp,
            dsp_addr: &mut self.dsp_addr,
            frozen: &mut self.spc_frozen,
            tick_timers: false,
        };
        self.spc700.reset(&mut bus_view);
    }

    fn step_audio_clock(&mut self) {
        self.step_native_audio_clock();
        self.step_output_audio_clock();
    }

    fn render_stereo_sample_internal(&mut self) -> (f32, f32) {
        self.dsp.current_stereo_sample()
    }

    fn step_native_audio_clock(&mut self) {
        if self.native_cycles_per_sample <= 0.0 {
            return;
        }
        self.native_sample_acc += 1.0;
        while self.native_sample_acc >= self.native_cycles_per_sample {
            self.native_sample_acc -= self.native_cycles_per_sample;
            if self.native_samples.len() < MAX_PENDING_SAMPLES {
                let sample = self.render_stereo_sample_internal();
                self.native_samples.push_back(sample);
            } else {
                break;
            }
        }
    }

    fn step_output_audio_clock(&mut self) {
        if self.cycles_per_sample <= 0.0 {
            return;
        }
        self.sample_acc += 1.0;
        while self.sample_acc >= self.cycles_per_sample {
            self.sample_acc -= self.cycles_per_sample;
            let Some(sample) = self.resample_native_audio() else {
                break;
            };
            if self.pending_samples.len() < MAX_PENDING_SAMPLES {
                self.pending_samples.push_back(sample);
            } else {
                break;
            }
        }
    }

    fn resample_native_audio(&mut self) -> Option<(f32, f32)> {
        let first = *self.native_samples.front()?;
        let second = if self.native_samples.len() > 1 {
            self.native_samples[1]
        } else {
            first
        };
        let t = self.resample_phase.clamp(0.0, 1.0);
        let left = first.0 + (second.0 - first.0) * t;
        let right = first.1 + (second.1 - first.1) * t;

        self.resample_phase += NATIVE_AUDIO_SAMPLE_RATE_HZ / self.cycles_per_sample_rate_hz();
        while self.resample_phase >= 1.0 && self.native_samples.len() > 1 {
            self.resample_phase -= 1.0;
            self.native_samples.pop_front();
        }
        Some((left, right))
    }

    fn cycles_per_sample_rate_hz(&self) -> f32 {
        if self.cycles_per_sample <= 0.0 {
            return NATIVE_AUDIO_SAMPLE_RATE_HZ;
        }
        SNES_MASTER_CLOCK_HZ / self.cycles_per_sample
    }

    pub fn sample_ready(&self) -> bool {
        !self.pending_samples.is_empty()
    }

    pub fn take_stereo_sample(&mut self) -> Option<(f32, f32)> {
        self.pending_samples.pop_front()
    }

    pub fn take_sample(&mut self) -> Option<f32> {
        self.take_stereo_sample()
            .map(|(left, right)| (left + right) * 0.5)
    }

    pub fn set_sample_rate(&mut self, rate: f32) {
        if !rate.is_finite() || rate <= 0.0 {
            return;
        }
        self.cycles_per_sample = SNES_MASTER_CLOCK_HZ / rate;
        self.sample_acc = 0.0;
        self.resample_phase = 0.0;
        self.pending_samples.clear();
    }

    #[cfg(test)]
    pub(crate) fn read_spc_memory_for_test(&mut self, addr: u16) -> u8 {
        let mut bus_view = SpcBusView {
            aram: &mut self.aram,
            ipl: &self.ipl,
            main_to_spc_ports: &mut self.main_to_spc_ports,
            spc_to_main_ports: &mut self.spc_to_main_ports,
            control: &mut self.control,
            test: &mut self.test,
            timers: &mut self.timers,
            dsp: &mut self.dsp,
            dsp_addr: &mut self.dsp_addr,
            frozen: &mut self.spc_frozen,
            tick_timers: true,
        };
        bus_view.read(addr)
    }

    #[cfg(test)]
    pub(crate) fn write_spc_memory_for_test(&mut self, addr: u16, value: u8) {
        let mut bus_view = SpcBusView {
            aram: &mut self.aram,
            ipl: &self.ipl,
            main_to_spc_ports: &mut self.main_to_spc_ports,
            spc_to_main_ports: &mut self.spc_to_main_ports,
            control: &mut self.control,
            test: &mut self.test,
            timers: &mut self.timers,
            dsp: &mut self.dsp,
            dsp_addr: &mut self.dsp_addr,
            frozen: &mut self.spc_frozen,
            tick_timers: true,
        };
        bus_view.write(addr, value);
    }

    #[cfg(test)]
    pub(crate) fn advance_spc_bus_cycles_for_test(&mut self, cycles: usize) {
        let mut bus_view = SpcBusView {
            aram: &mut self.aram,
            ipl: &self.ipl,
            main_to_spc_ports: &mut self.main_to_spc_ports,
            spc_to_main_ports: &mut self.spc_to_main_ports,
            control: &mut self.control,
            test: &mut self.test,
            timers: &mut self.timers,
            dsp: &mut self.dsp,
            dsp_addr: &mut self.dsp_addr,
            frozen: &mut self.spc_frozen,
            tick_timers: true,
        };
        for _ in 0..cycles {
            bus_view.idle();
        }
    }

    #[cfg(test)]
    pub(crate) fn write_spc_control_for_test(&mut self, value: u8) {
        let mut bus_view = SpcBusView {
            aram: &mut self.aram,
            ipl: &self.ipl,
            main_to_spc_ports: &mut self.main_to_spc_ports,
            spc_to_main_ports: &mut self.spc_to_main_ports,
            control: &mut self.control,
            test: &mut self.test,
            timers: &mut self.timers,
            dsp: &mut self.dsp,
            dsp_addr: &mut self.dsp_addr,
            frozen: &mut self.spc_frozen,
            tick_timers: true,
        };
        bus_view.write(0x00F1, value);
    }

    #[cfg(test)]
    pub(crate) fn dsp_phase_for_test(&self) -> u8 {
        self.dsp.phase()
    }

    #[cfg(test)]
    pub(crate) fn spc_frozen_for_test(&self) -> bool {
        self.spc_frozen
    }
}

struct SpcBusView<'a> {
    aram: &'a mut [u8; ARAM_SIZE],
    ipl: &'a [u8; 64],
    main_to_spc_ports: &'a mut [u8; 4],
    spc_to_main_ports: &'a mut [u8; 4],
    control: &'a mut u8,
    test: &'a mut u8,
    timers: &'a mut SpcTimers,
    dsp: &'a mut Sdsp,
    dsp_addr: &'a mut u8,
    frozen: &'a mut bool,
    tick_timers: bool,
}

impl SpcBusView<'_> {
    fn ipl_enabled(&self) -> bool {
        *self.control & 0x80 != 0
    }

    fn test_reg(&self) -> u8 {
        *self.test
    }

    fn ram_write_enabled(&self) -> bool {
        // Per bsnes smp/memory.cpp `writeRAM`: a write only lands when
        // RAM-Writable (bit 1) is set AND RAM-Disable (bit 2) is clear.
        self.test_reg() & 0x02 != 0 && self.test_reg() & 0x04 == 0
    }

    fn ram_disabled(&self) -> bool {
        // Per bsnes smp/memory.cpp `readRAM`: when bit 2 is set, ARAM reads
        // return 0x5A (0xFF on mini-SNES). IPL ROM reads and I/O reads are
        // unaffected (the I/O range is handled by an explicit override path).
        self.test_reg() & 0x04 != 0
    }

    fn is_internal_wait_address(&self, addr: u16) -> bool {
        matches!(addr, 0x00F0..=0x00FF) || (0xFFC0..=0xFFFF).contains(&addr) && self.ipl_enabled()
    }

    /// `true` when this access must deadlock the SPC700 due to the glitchy
    /// internal-speed divider.
    ///
    /// Hardware quirk exercised by blargg's `speed_2_freezes` test: selecting
    /// internal-speed field value 2 (`$F0` bits 6-7 = `10`, clock divider 8) is
    /// glitchy and deterministically deadlocks the SPC700 as soon as it performs
    /// an internal-timed access (an idle cycle, an I/O register `$F0-$FF`, or an
    /// IPLROM fetch). Divider value 3 runs slow (~20 clocks/cycle) but keeps
    /// executing, and the external-speed field (bits 4-5) never triggers the
    /// freeze, so ordinary slow-speed tests are unaffected.
    fn internal_speed_freeze(&self, addr: Option<u16>) -> bool {
        let internal = match addr {
            None => true,
            Some(address) => self.is_internal_wait_address(address),
        };
        internal && (self.test_reg() >> 6) & 0x03 == 2
    }

    fn wait_cycles_for_addr(&self, addr: Option<u16>) -> u8 {
        let wait_bits = match addr {
            None => (self.test_reg() >> 6) & 0x03,
            Some(address) if self.is_internal_wait_address(address) => {
                (self.test_reg() >> 6) & 0x03
            }
            Some(_) => (self.test_reg() >> 4) & 0x03,
        };
        match wait_bits {
            0 => 1,
            1 => 2,
            2 => 5,
            _ => 10,
        }
    }

    fn timer_wait_cycles_for_addr(&self, addr: Option<u16>) -> u8 {
        let wait_bits = match addr {
            None => (self.test_reg() >> 6) & 0x03,
            Some(address) if self.is_internal_wait_address(address) => {
                (self.test_reg() >> 6) & 0x03
            }
            Some(_) => (self.test_reg() >> 4) & 0x03,
        };
        // Timers use non-glitchy wait-state divisors (2,4,8,16) while SMP core
        // cycles use (2,4,10,20). This scale is normalized by /2 in this core.
        match wait_bits {
            0 => 1,
            1 => 2,
            2 => 4,
            _ => 8,
        }
    }

    fn write_control(&mut self, value: u8) {
        let old_control = *self.control;
        *self.control = value;
        trace_apu!(
            4;
            "SPC write $F1 control ${:02X} -> ${:02X}",
            old_control,
            value
        );
        self.timers.write_control(old_control, value);
        if value & 0x10 != 0 {
            self.main_to_spc_ports[0] = 0;
            self.main_to_spc_ports[1] = 0;
        }
        if value & 0x20 != 0 {
            self.main_to_spc_ports[2] = 0;
            self.main_to_spc_ports[3] = 0;
        }
    }

    fn write_test(&mut self, value: u8) {
        let old_test = *self.test;
        *self.test = value;
        // If TEST transitions from "timers allowed" to "timers stopped",
        // clear TnOUT on all timers (analogous to CONTROL disable clearing TnOUT).
        let old_allows = (old_test & 0x01 == 0) && (old_test & 0x08 != 0);
        let new_allows = (value & 0x01 == 0) && (value & 0x08 != 0);
        if old_allows && !new_allows {
            self.timers.clear_all_tout();
        }
    }

    fn test_allows_timers(&self) -> bool {
        // fullsnes TEST $F0:
        //   bit 0 = Timer-Enable  (0=Normal/timers work, 1=Timers don't work)
        //   bit 3 = Timer-Disable (0=Timers don't work,  1=Normal/timers work)
        // Both must be in the "Normal" state for timers to tick.
        (self.test_reg() & 0x01 == 0) && (self.test_reg() & 0x08 != 0)
    }

    fn tick_apu_cycles(&mut self, timer_cycles: u8) {
        if !self.tick_timers {
            return;
        }
        self.dsp.step_phase_with_memory(&mut self.aram[..]);
        if self.test_allows_timers() {
            for _ in 0..timer_cycles {
                self.timers.tick_cycle();
            }
        }
    }

    fn tick_access_cycles_for_addr(&mut self, addr: Option<u16>) {
        self.tick_apu_cycles(self.timer_wait_cycles_for_addr(addr));
    }
}

impl Spc700Bus for SpcBusView<'_> {
    fn read_cycles(&self, addr: u16) -> u8 {
        self.wait_cycles_for_addr(Some(addr))
    }

    fn read(&mut self, addr: u16) -> u8 {
        if self.internal_speed_freeze(Some(addr)) {
            *self.frozen = true;
        }
        self.tick_access_cycles_for_addr(Some(addr));
        let value = match addr {
            // I/O register reads ($F0-$FF) always return I/O values, not the RAM-disable
            // sentinel. Per bsnes smp/memory.cpp: readRAM is called first (returns $5A when
            // RAM-disabled), but readIO overrides for the entire $F0-$FF range.
            // $F0 TEST and $F1 CONTROL are write-only; reads return $00.
            0x00F0..=0x00F1 => 0x00,
            0x00F2 => *self.dsp_addr,
            0x00F3 => {
                let value = self.dsp.read_reg(*self.dsp_addr);
                trace_apu!(
                    1;
                    "SPC reads DSP[${:02X}] -> ${:02X} phase={}",
                    *self.dsp_addr,
                    value,
                    self.dsp.phase()
                );
                value
            }
            0x00F4..=0x00F7 => self.main_to_spc_ports[(addr - 0x00F4) as usize],
            // $F8-$F9 = AUXIO4/AUXIO5 (general-purpose I/O, stores value in ARAM)
            0x00F8..=0x00F9 => self.aram[addr as usize],
            // $FA-$FC = T0/T1/T2 targets (write-only; reads return 0x00)
            0x00FA..=0x00FC => 0x00,
            0x00FD..=0x00FF => self.timers.read_counter((addr - 0x00FD) as usize),
            0xFFC0..=0xFFFF if self.ipl_enabled() => self.ipl[(addr - 0xFFC0) as usize],
            _ if self.ram_disabled() => 0x5A,
            _ => self.aram[addr as usize],
        };
        if addr <= 0x0001 {
            trace_apu!(4; "SPC reads ARAM[${:04X}] -> ${:02X}", addr, value);
        }
        if (0x00F4..=0x00F7).contains(&addr) {
            trace_apu!(3; "SPC reads port[{}] -> ${:02X}", addr - 0x00F4, value);
        }
        value
    }

    fn write_cycles(&self, addr: u16) -> u8 {
        self.wait_cycles_for_addr(Some(addr))
    }

    fn write(&mut self, addr: u16, value: u8) {
        if self.internal_speed_freeze(Some(addr)) {
            *self.frozen = true;
        }
        self.tick_access_cycles_for_addr(Some(addr));
        let write_enabled = self.ram_write_enabled();
        if write_enabled {
            self.aram[addr as usize] = value;
            if addr <= 0x0001 {
                trace_apu!(4; "SPC writes ARAM[${:04X}] = ${:02X}", addr, value);
            }
        } else if addr < 0x0100 {
            trace_apu!(
                4;
                "SPC RAM write blocked addr=${:04X} value=${:02X} test=${:02X}",
                addr,
                value,
                self.test_reg()
            );
        }
        match addr {
            0x00F0 => {
                self.write_test(value);
            }
            0x00F1 => self.write_control(value),
            0x00F2 => *self.dsp_addr = value,
            0x00F3 => {
                trace_apu!(
                    1;
                    "SPC writes DSP[${:02X}] = ${:02X} phase={}",
                    *self.dsp_addr,
                    value,
                    self.dsp.phase()
                );
                self.dsp.write_reg(*self.dsp_addr, value)
            }
            0x00F4..=0x00F7 => {
                let port_idx = (addr - 0x00F4) as usize;
                trace_apu!(2; "SPC writes port[{}] = ${:02X}", port_idx, value);
                self.spc_to_main_ports[port_idx] = value;
            }
            0x00FA..=0x00FC => {
                trace_apu!(4; "SPC write timer target ${:04X} = ${:02X}", addr, value);
                self.timers.write_target((addr - 0x00FA) as usize, value);
            }
            _ => {}
        }
    }

    fn idle_cycles(&self) -> u8 {
        self.wait_cycles_for_addr(None)
    }

    fn idle(&mut self) {
        if self.internal_speed_freeze(None) {
            *self.frozen = true;
        }
        self.tick_access_cycles_for_addr(None);
    }

    fn dummy_read(&mut self, _addr: u16) {
        if self.internal_speed_freeze(None) {
            *self.frozen = true;
        }
        self.tick_access_cycles_for_addr(None);
    }
}

#[cfg(test)]
mod tests {
    use super::{SnesApu, SnesApuState};

    #[test]
    fn ipl_rom_has_sixty_four_bytes() {
        assert_eq!(super::ipl::EMBEDDED_IPL.len(), 64);
    }

    #[test]
    fn boot_rom_is_visible_at_ffc0_on_power_up() {
        let mut apu = SnesApu::new(None);
        apu.aram[0xFFC0] = 0x99;
        assert_eq!(
            apu.read_spc_memory_for_test(0xFFC0),
            super::ipl::EMBEDDED_IPL[0]
        );
    }

    #[test]
    fn control_register_bit_7_disables_boot_rom_overlay() {
        let mut apu = SnesApu::new(None);
        apu.aram[0xFFC0] = 0x42;
        apu.write_spc_control_for_test(0x00);
        assert_eq!(apu.read_spc_memory_for_test(0xFFC0), 0x42);
    }

    #[test]
    fn main_cpu_and_spc700_ports_mirror_each_other() {
        let mut apu = SnesApu::new(None);

        apu.write_main_port(0, 0xAA);
        assert_eq!(apu.read_spc_port(0), 0xAA);

        apu.write_spc_port(1, 0x55);
        assert_eq!(apu.read_main_port(1), 0x55);
    }

    #[test]
    fn restore_state_with_legacy_empty_aram_keeps_power_on_boot_rom_mapping() {
        let mut apu = SnesApu::new(None);

        let legacy_state = SnesApuState::default();
        apu.restore_state(&legacy_state)
            .expect("restore legacy state");

        assert_eq!(
            apu.read_spc_memory_for_test(0xFFFE),
            super::ipl::EMBEDDED_IPL[0x3E]
        );
        assert_eq!(
            apu.read_spc_memory_for_test(0xFFFF),
            super::ipl::EMBEDDED_IPL[0x3F]
        );
    }

    #[test]
    fn timer0_counter_is_visible_via_fd_and_clears_on_read() {
        let mut apu = SnesApu::new(None);

        apu.write_spc_memory_for_test(0x00FA, 0x01);
        apu.write_spc_control_for_test(0x81);
        apu.advance_spc_bus_cycles_for_test(128);

        assert_eq!(apu.read_spc_memory_for_test(0x00FD), 0x01);
        assert_eq!(apu.read_spc_memory_for_test(0x00FD), 0x00);
    }

    #[test]
    fn restore_state_does_not_advance_timers_during_spc_reset_vector_reads() {
        let mut apu = SnesApu::new(None);
        apu.write_spc_memory_for_test(0x00FA, 0x01);
        apu.write_spc_control_for_test(0x81);
        apu.advance_spc_bus_cycles_for_test(124);

        let state = apu.capture_state();
        apu.restore_state(&state).expect("restore should succeed");

        assert_eq!(apu.read_spc_memory_for_test(0x00FD), 0x00);
    }

    #[test]
    fn dsp_f2_f3_ports_store_values_per_selected_register() {
        let mut apu = SnesApu::new(None);

        apu.write_spc_memory_for_test(0x00F2, 0x10);
        apu.write_spc_memory_for_test(0x00F3, 0xAA);
        apu.write_spc_memory_for_test(0x00F2, 0x11);
        apu.write_spc_memory_for_test(0x00F3, 0xBB);

        apu.write_spc_memory_for_test(0x00F2, 0x10);
        assert_eq!(apu.read_spc_memory_for_test(0x00F3), 0xAA);
        apu.write_spc_memory_for_test(0x00F2, 0x11);
        assert_eq!(apu.read_spc_memory_for_test(0x00F3), 0xBB);
    }

    #[test]
    fn dsp_f2_preserves_full_readback_while_f3_masks_and_ignores_mirror_writes() {
        let mut apu = SnesApu::new(None);

        apu.write_spc_memory_for_test(0x00F2, 0x10);
        apu.write_spc_memory_for_test(0x00F3, 0x34);

        apu.write_spc_memory_for_test(0x00F2, 0x90);
        assert_eq!(apu.read_spc_memory_for_test(0x00F2), 0x90);
        assert_eq!(apu.read_spc_memory_for_test(0x00F3), 0x34);
        apu.write_spc_memory_for_test(0x00F3, 0x56);
        apu.write_spc_memory_for_test(0x00F2, 0x10);
        assert_eq!(
            apu.read_spc_memory_for_test(0x00F3),
            0x34,
            "DSP writes through read-only mirrors must be ignored"
        );
    }

    #[test]
    fn advancing_spc_bus_cycles_advances_dsp_phase_skeleton() {
        let mut apu = SnesApu::new(None);

        assert_eq!(apu.dsp_phase_for_test(), 0);

        apu.advance_spc_bus_cycles_for_test(3);
        assert_eq!(apu.dsp_phase_for_test(), 3);
    }

    #[test]
    fn writing_test_register_uses_pre_write_waitstate_cycles() {
        let mut apu = SnesApu::new(None);

        assert_eq!(apu.dsp_phase_for_test(), 0);
        apu.write_spc_memory_for_test(0x00F0, 0x3A);

        assert_eq!(apu.dsp_phase_for_test(), 1);
    }

    #[test]
    fn dsp_phase_advances_while_test_register_disables_spc_timers() {
        let mut apu = SnesApu::new(None);

        apu.write_spc_memory_for_test(0x00F0, 0x0B); // default TEST with timer-enable bit set
        let phase_after_test_write = apu.dsp_phase_for_test();
        apu.advance_spc_bus_cycles_for_test(3);

        assert_eq!(
            apu.dsp_phase_for_test(),
            (phase_after_test_write + 3) & 0x1F,
            "S-DSP should keep clocking even when TEST disables SPC timers"
        );
    }

    #[test]
    fn dsp_phase_advances_once_per_spc_memory_operation_under_test_waitstates() {
        let mut apu = SnesApu::new(None);

        apu.write_spc_memory_for_test(0x00F0, 0x2A); // external RAM access time = 5 cycles
        let phase_after_test_write = apu.dsp_phase_for_test();
        apu.write_spc_memory_for_test(0x0200, 0x55);

        assert_eq!(
            apu.dsp_phase_for_test(),
            (phase_after_test_write + 1) & 0x1F,
            "Mesen advances the S-DSP pipeline once per SPC memory operation, not once per TEST waitstate"
        );
    }

    #[test]
    fn native_audio_rendering_uses_aram_backed_echo_path() {
        let mut apu = SnesApu::new(None);
        apu.write_spc_memory_for_test(0x00F2, 0x2C); // EVOLL
        apu.write_spc_memory_for_test(0x00F3, 0x7F);
        apu.write_spc_memory_for_test(0x00F2, 0x3C); // EVOLR
        apu.write_spc_memory_for_test(0x00F3, 0x7F);
        apu.write_spc_memory_for_test(0x00F2, 0x7F); // FIR7
        apu.write_spc_memory_for_test(0x00F3, 0x7F);
        apu.write_spc_memory_for_test(0x00F2, 0x6D); // ESA
        apu.write_spc_memory_for_test(0x00F3, 0x10);
        apu.write_spc_memory_for_test(0x00F2, 0x7D); // EDL
        apu.write_spc_memory_for_test(0x00F3, 0x01);
        apu.write_spc_memory_for_test(0x00F2, 0x6C); // FLG
        apu.write_spc_memory_for_test(0x00F3, 0x00); // unmute + echo write enable

        let base = 0x1000usize;
        apu.aram[base] = 0xFE;
        apu.aram[base + 1] = 0x7F;
        apu.aram[base + 2] = 0xFE;
        apu.aram[base + 3] = 0x7F;

        apu.native_cycles_per_sample = 1.0;
        apu.advance_spc_bus_cycles_for_test(32);
        apu.step_native_audio_clock();

        let sample = apu
            .native_samples
            .back()
            .copied()
            .expect("native renderer should enqueue one sample");
        assert!(
            sample.0.abs() > 0.01 || sample.1.abs() > 0.01,
            "echo sample from ARAM should contribute to native output"
        );
    }

    #[test]
    fn restore_state_preserves_full_dsp_address_readback_byte() {
        let mut apu = SnesApu::new(None);
        let mut state = apu.capture_state();
        state.dsp_addr = 0xFF;

        apu.restore_state(&state).expect("restore should succeed");
        assert_eq!(apu.read_spc_memory_for_test(0x00F2), 0xFF);
    }

    #[test]
    fn restore_state_rejects_invalid_dsp_register_file_size() {
        let mut apu = SnesApu::new(None);
        apu.write_main_port(0, 0xCC);
        let mut state = apu.capture_state();
        state.main_to_spc_ports[0] = 0x12;
        state.dsp = serde_json::from_str(
            r#"{
                "phase":0,
                "regs":[1,2],
                "echo_state":{
                    "ring_index":0,
                    "ring_size":1,
                    "fir_pos":0,
                    "fir_left":[0,0,0,0,0,0,0,0],
                    "fir_right":[0,0,0,0,0,0,0,0],
                    "esa_latched":0,
                    "esa_initialized":false
                }
            }"#,
        )
        .expect("deserialize malformed DSP for test");

        let err = apu
            .restore_state(&state)
            .expect_err("restore should reject invalid DSP register length");
        assert!(err.contains("APU DSP register file size mismatch"));
        assert_eq!(
            apu.read_spc_port(0),
            0xCC,
            "failed restore must not partially mutate APU state"
        );
    }

    #[test]
    fn save_state_round_trips_spc700_register_state() {
        let mut apu = SnesApu::new(None);
        apu.spc700
            .load_state_for_processor_test(0x12, 0x34, 0x56, 0x78, 0x2468, 0xAF);

        let state = apu.capture_state();

        apu.spc700
            .load_state_for_processor_test(0xFF, 0xEE, 0xDD, 0xCC, 0x0000, 0x00);

        apu.restore_state(&state).expect("restore should succeed");

        assert_eq!(apu.spc700.a(), 0x12);
        assert_eq!(apu.spc700.x(), 0x34);
        assert_eq!(apu.spc700.y(), 0x56);
        assert_eq!(apu.spc700.sp(), 0x78);
        assert_eq!(apu.spc700.pc(), 0x2468);
        assert_eq!(apu.spc700.psw(), 0xAF);
    }

    // -----------------------------------------------------------------------
    // Spec: $F0 TEST register has a power-on default of 0Ah internally, while
    // SPC-visible $F0 TEST and $F1 CONTROL reads are write-only and return 0.
    // -----------------------------------------------------------------------

    #[test]
    fn test_register_at_f0_power_on_default_is_0a() {
        let apu = SnesApu::new(None);
        assert_eq!(
            apu.test, 0x0A,
            "power-on TEST register must be 0x0A per fullsnes spec"
        );
    }

    #[test]
    fn test_and_control_registers_are_write_only_when_read_back() {
        let mut apu = SnesApu::new(None);
        apu.write_spc_memory_for_test(0x00F0, 0x3A);
        apu.write_spc_memory_for_test(0x00F1, 0x80);
        assert_eq!(
            apu.read_spc_memory_for_test(0x00F0),
            0x00,
            "$F0 TEST register is write-only and reads as zero"
        );
        assert_eq!(
            apu.read_spc_memory_for_test(0x00F1),
            0x00,
            "$F1 CONTROL register is write-only and reads as zero"
        );
    }

    #[test]
    fn timer_target_registers_are_write_only_when_read_back() {
        let mut apu = SnesApu::new(None);

        apu.aram[0x00FA] = 0x12;
        apu.aram[0x00FB] = 0x34;
        apu.aram[0x00FC] = 0x56;

        assert_eq!(apu.read_spc_memory_for_test(0x00FA), 0x00);
        assert_eq!(apu.read_spc_memory_for_test(0x00FB), 0x00);
        assert_eq!(apu.read_spc_memory_for_test(0x00FC), 0x00);
    }

    #[test]
    fn auxio_registers_power_on_as_zero_and_read_back_written_values() {
        let mut apu = SnesApu::new(None);

        assert_eq!(apu.read_spc_memory_for_test(0x00F8), 0x00);
        assert_eq!(apu.read_spc_memory_for_test(0x00F9), 0x00);

        apu.write_spc_memory_for_test(0x00F8, 0x12);
        apu.write_spc_memory_for_test(0x00F9, 0x34);

        assert_eq!(apu.read_spc_memory_for_test(0x00F8), 0x12);
        assert_eq!(apu.read_spc_memory_for_test(0x00F9), 0x34);
    }

    #[test]
    fn spc_io_writes_also_update_underlying_aram_when_ram_writable() {
        let mut apu = SnesApu::new(None);

        for (addr, value) in [
            (0x00F0, 0x2A),
            (0x00F1, 0x80),
            (0x00F2, 0x10),
            (0x00F3, 0x55),
            (0x00F4, 0x66),
            (0x00FA, 0x01),
            (0x00FD, 0x77),
            (0x00FE, 0x88),
            (0x00FF, 0x99),
        ] {
            apu.write_spc_memory_for_test(addr, value);
            assert_eq!(
                apu.aram[addr as usize], value,
                "write to ${addr:04X} should also land in underlying ARAM"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Reproducer for #2911. Per bsnes `smp/io.cpp` + `smp/memory.cpp`:
    //   $F0 bit 1 = RAM-Writable (1 = writes go to ARAM)
    //   $F0 bit 2 = RAM-Disable  (1 = reads return 0x5A, writes blocked)
    // Our previous code interpreted bit 2 as a "Crash/Halt SPC" sentinel, so
    // blargg's 4-test_ram_disable.smc hung after the micro-op trampoline
    // wrote $0E to $F0. After the fix, bit 2 simply gates ARAM reads/writes.
    // -----------------------------------------------------------------------

    #[test]
    fn ram_disable_bit_makes_aram_reads_return_5a() {
        let mut apu = SnesApu::new(None);
        // Seed an ARAM byte we can distinguish from the 0x5A sentinel.
        apu.aram[0x0200] = 0x37;
        // Default test = 0x0A (bit 1 = RAM-Writable, bit 3 = Timer-Normal).
        assert_eq!(apu.read_spc_memory_for_test(0x0200), 0x37);
        // Set bit 2 (RAM-Disable). Keep bit 1 and bit 3 as before.
        apu.write_spc_memory_for_test(0x00F0, 0x0E);
        assert_eq!(
            apu.read_spc_memory_for_test(0x0200),
            0x5A,
            "ARAM reads must return 0x5A while TEST bit 2 (RAM-Disable) is set",
        );
        // Clear bit 2 again: real ARAM data is visible.
        apu.write_spc_memory_for_test(0x00F0, 0x0A);
        assert_eq!(apu.read_spc_memory_for_test(0x0200), 0x37);
    }

    #[test]
    fn ram_disable_bit_blocks_aram_writes_even_when_ram_writable_is_set() {
        let mut apu = SnesApu::new(None);
        apu.aram[0x0200] = 0x11;
        // Set bit 1 (RAM-Writable) AND bit 2 (RAM-Disable). Writes must drop.
        apu.write_spc_memory_for_test(0x00F0, 0x0E);
        apu.write_spc_memory_for_test(0x0200, 0xAB);
        // Disable the disable bit so reads expose underlying ARAM.
        apu.write_spc_memory_for_test(0x00F0, 0x0A);
        assert_eq!(
            apu.read_spc_memory_for_test(0x0200),
            0x11,
            "writes must be blocked while RAM-Disable bit is set",
        );
    }

    #[test]
    fn setting_ram_disable_bit_does_not_halt_spc700() {
        let mut apu = SnesApu::new(None);
        // Halt-status hasn't changed before the write.
        assert!(!apu.spc700.is_halted());
        apu.write_spc_memory_for_test(0x00F0, 0x0E);
        assert!(
            !apu.spc700.is_halted(),
            "writing $F0 with bit 2 set must NOT halt the SPC700 (#2911)",
        );
    }

    // -----------------------------------------------------------------------
    // Hardware quirk (blargg `speed_2_freezes`): internal-speed field value 2
    // ($F0 bits 6-7 = 10, clock divider 8) is glitchy and deadlocks the SPC700
    // on the next internal-timed access. Value 3 runs slow but keeps executing,
    // and the external-speed field never triggers the freeze.
    // -----------------------------------------------------------------------

    #[test]
    fn internal_speed_2_freezes_spc_on_next_io_access() {
        let mut apu = SnesApu::new(None);
        // Select internal speed 2 (bit 7). The write itself runs at the old
        // internal speed (0), so it must not freeze yet.
        apu.write_spc_memory_for_test(0x00F0, 0x8A);
        assert!(
            !apu.spc_frozen_for_test(),
            "selecting internal speed 2 must not freeze until an internal access"
        );
        // The next access to an internal address ($F0-$FF) now happens at the
        // glitchy internal speed 2 and must deadlock the SPC700.
        apu.write_spc_memory_for_test(0x00F0, 0x0A);
        assert!(
            apu.spc_frozen_for_test(),
            "an internal access at internal speed 2 must freeze the SPC700"
        );
    }

    #[test]
    fn internal_speed_2_freeze_halts_spc_execution() {
        let mut apu = SnesApu::new(None);
        // Program at $0000: set internal speed 2, then write $F0 (freezes),
        // then write a marker to port 1 ($F5) that must never run.
        apu.aram[0x0000] = 0x8F; // MOV $F0,#$8A
        apu.aram[0x0001] = 0x8A;
        apu.aram[0x0002] = 0xF0;
        apu.aram[0x0003] = 0x8F; // MOV $F0,#$0A  (internal access -> freeze)
        apu.aram[0x0004] = 0x0A;
        apu.aram[0x0005] = 0xF0;
        apu.aram[0x0006] = 0x8F; // MOV $F5,#$AA  (must never execute)
        apu.aram[0x0007] = 0xAA;
        apu.aram[0x0008] = 0xF5;
        apu.aram[0x0009] = 0x2F; // BRA *
        apu.aram[0x000A] = 0xFE;
        apu.write_spc_control_for_test(0x00); // disable IPL overlay
        apu.spc700.set_pc_for_test(0x0000);

        for _ in 0..2_000_000 {
            apu.tick();
        }

        assert!(apu.spc_frozen_for_test(), "SPC must be frozen");
        assert_eq!(
            apu.spc_to_main_ports[1], 0x00,
            "instruction after the freezing write must never execute"
        );
    }

    #[test]
    fn internal_speed_2_freezes_spc_on_dummy_read_cycle() {
        let mut apu = SnesApu::new(None);
        // Program at $0000: set internal speed 2, execute NOP (dummy-read
        // internal cycle freezes), then write a marker that must never run.
        apu.aram[0x0000] = 0x8F; // MOV $F0,#$8A
        apu.aram[0x0001] = 0x8A;
        apu.aram[0x0002] = 0xF0;
        apu.aram[0x0003] = 0x00; // NOP dummy-read cycle freezes
        apu.aram[0x0004] = 0x8F; // MOV $F5,#$AA (must never execute)
        apu.aram[0x0005] = 0xAA;
        apu.aram[0x0006] = 0xF5;
        apu.write_spc_control_for_test(0x00); // disable IPL overlay
        apu.spc700.set_pc_for_test(0x0000);

        for _ in 0..2_000_000 {
            apu.tick();
        }

        assert!(
            apu.spc_frozen_for_test(),
            "SPC must freeze on NOP's internal dummy-read cycle"
        );
        assert_eq!(
            apu.spc_to_main_ports[1], 0x00,
            "instruction after the freezing dummy read must never execute"
        );
    }

    #[test]
    fn internal_speed_3_does_not_freeze_spc() {
        let mut apu = SnesApu::new(None);
        // Internal speed 3 (bits 6-7 = 11 -> $CA) runs slow but keeps executing.
        apu.write_spc_memory_for_test(0x00F0, 0xCA);
        apu.write_spc_memory_for_test(0x00F0, 0x0A);
        assert!(
            !apu.spc_frozen_for_test(),
            "internal speed 3 must not freeze the SPC700"
        );
    }

    #[test]
    fn external_speed_2_does_not_freeze_spc() {
        let mut apu = SnesApu::new(None);
        // External speed 2 (bits 4-5 = 10 -> $2A) plus RAM-Writable so the ARAM
        // write lands. External accesses never trigger the internal-speed glitch.
        apu.write_spc_memory_for_test(0x00F0, 0x2A);
        apu.write_spc_memory_for_test(0x0200, 0x55);
        assert!(
            !apu.spc_frozen_for_test(),
            "an external access at external speed 2 must not freeze the SPC700"
        );
    }

    // -----------------------------------------------------------------------
    // Spec: $F0 TEST bits 0 and 3 gate whether timers tick at all.
    //   bit 0 = Timer-Enable  (0=Normal, 1=Timers don't work)
    //   bit 3 = Timer-Disable (0=Timers don't work, 1=Normal)
    // Default TEST=0x0A keeps both in "Normal" state.
    // -----------------------------------------------------------------------

    #[test]
    fn timers_tick_normally_with_default_test_register() {
        let mut apu = SnesApu::new(None);
        // TEST = 0x0A by default; enable T2 with target=1
        apu.write_spc_memory_for_test(0x00FC, 0x01); // T2DIV = 1
        apu.write_spc_control_for_test(0x04); // enable T2 (bit 2)
        apu.advance_spc_bus_cycles_for_test(16);
        assert_eq!(
            apu.read_spc_memory_for_test(0x00FF),
            0x01,
            "T2 should fire once after 16 cycles with default TEST=0x0A"
        );
    }

    #[test]
    fn setting_test_bit0_stops_timer_ticking() {
        let mut apu = SnesApu::new(None);
        // Enable T2 with target=1
        apu.write_spc_memory_for_test(0x00FC, 0x01); // T2DIV = 1
        apu.write_spc_control_for_test(0x04); // enable T2 (bit 2)
        // Set TEST bit 0 = 1 to stop timers (Timer-Enable = "don't work")
        apu.write_spc_memory_for_test(0x00F0, 0x0B); // 0x0A | 0x01
        // Run many cycles — timer should not tick
        apu.advance_spc_bus_cycles_for_test(100);
        assert_eq!(
            apu.read_spc_memory_for_test(0x00FF),
            0x00,
            "Timer must not tick when TEST bit 0 = 1 (Timer-Enable = don't work)"
        );
    }

    #[test]
    fn clearing_test_bit3_stops_timer_ticking() {
        let mut apu = SnesApu::new(None);
        // Enable T2 with target=1
        apu.write_spc_memory_for_test(0x00FC, 0x01); // T2DIV = 1
        apu.write_spc_control_for_test(0x04); // enable T2 (bit 2)
        // Clear TEST bit 3 to stop timers (Timer-Disable = "don't work")
        apu.write_spc_memory_for_test(0x00F0, 0x02); // 0x0A & ~0x08
        // Run many cycles — timer should not tick
        apu.advance_spc_bus_cycles_for_test(100);
        assert_eq!(
            apu.read_spc_memory_for_test(0x00FF),
            0x00,
            "Timer must not tick when TEST bit 3 = 0 (Timer-Disable = don't work)"
        );
    }

    #[test]
    fn timer_ticks_use_nonglitchy_waitstate_divider_under_test_wait_bits() {
        let mut apu = SnesApu::new(None);
        // Enable T2 with target=1 (one increment per 16 timer cycles).
        apu.write_spc_memory_for_test(0x00FC, 0x01);
        apu.write_spc_control_for_test(0x04);
        // Keep timers enabled (bit3=1, bit0=0), but set external wait bits=2.
        apu.write_spc_memory_for_test(0x00F0, 0x2A);
        // External RAM access should advance timers by 4 cycles each (not 5).
        // Three such writes plus the surrounding register access cycles put T2
        // exactly on its first tick when $FF is read below.
        for _ in 0..3 {
            apu.write_spc_memory_for_test(0x0200, 0x55);
        }
        assert_eq!(
            apu.read_spc_memory_for_test(0x00FF),
            0x01,
            "T2 should tick once after 3 external writes at wait-bits=2"
        );
    }

    // -----------------------------------------------------------------------
    // When TEST transitions from "timers allowed" to "timers stopped", TnOUT
    // must be cleared to 0.  The test_timer_stop ROM fires T2 four times
    // before calling STOP via TEST, and the test expects TnOUT = 0 afterwards
    // — consistent with TEST stop behaving analogously to CONTROL disable
    // ("set TnOUT=0") for the accumulated-but-unread counter.
    // -----------------------------------------------------------------------

    #[test]
    fn writing_test_to_stop_timers_clears_accumulated_tout() {
        let mut apu = SnesApu::new(None);
        // Enable T2 with target=1 (fires every 16 cycles)
        apu.write_spc_memory_for_test(0x00FC, 0x01); // T2DIV = 1
        apu.write_spc_control_for_test(0x04); // enable T2 (bit 2)
        // Let T2 fire 4 times (4 × 16 = 64 cycles; do NOT read TnOUT yet)
        apu.advance_spc_bus_cycles_for_test(64);
        // Now stop timers via TEST bit 0 = 1 (Timer-Enable = "don't work")
        apu.write_spc_memory_for_test(0x00F0, 0x0B); // 0x0A | 0x01
        // TnOUT must be 0 immediately after the TEST stop
        assert_eq!(
            apu.read_spc_memory_for_test(0x00FF),
            0x00,
            "TnOUT must be cleared when TEST transitions to stop state"
        );
    }

    // Reproducer for #2908. Several blargg SPC test ROMs (4-test_ram_disable,
    // spc_smp, spc_timer, ...) skip the regular IPL upload and instead use the
    // "IPL-hack trampoline": after the standard $AA/$BB/$CC handshake, the
    // host loads $00F5 into the IPL entry-point ports, the IPL's
    // `jmp [$0000+x]` lands the SPC PC at $00F5 (= port-1 register itself),
    // and the SPC sits at $00F6 in a `BRA $FE` (-2) wait loop reading the BRA
    // operand from port-3.
    //
    // The host releases one micro-op at a time by:
    //   1. loading port-0 (opcode) and port-1 (operand)
    //   2. briefly writing port-3 = $FC (BRA operand $FC = -4 = branch to $00F4)
    //   3. writing port-3 = $FE again
    //
    // SPC must observe port-3 = $FC during the BRA's operand-fetch sub-cycle
    // (one SPC cycle after the opcode fetch), then branch to $00F4 and execute
    // the micro-op (port-0 = opcode, port-1 = operand).
    //
    // This is cycle-precise: the host's port-3 = $FC write window can be just
    // a few master cycles wide -- shorter than one SPC instruction. With
    // current atomic per-instruction SPC stepping, the SPC samples port-3 at
    // each instruction boundary -- which is mostly $FE because the host
    // restores it before the SPC's next boundary arrives. Result: SPC never
    // branches, micro-ops are not executed.
    //
    // EXPECTED with cycle-accurate per-SPC-cycle stepping:
    //   A receives the operand value from each release in turn.
    // CURRENT (atomic) behavior: A stays at the sentinel.
    fn ipl_hack_trampoline_state() -> SnesApuState {
        let mut state = SnesApuState {
            aram: vec![0u8; super::ARAM_SIZE],
            // control bit 7 (= 0x80) gates IPL overlay -- clear so PC=$00F5..
            // reads come from the I/O port region, not boot ROM.
            control: 0x00,
            // Default TEST: internal 1 cycle, external 1 cycle (no wait
            // states). This is what the failing ROMs use.
            test: super::default_test_reg(),
            ..SnesApuState::default()
        };
        // port-0/1 = opcode/operand (set per release).
        // port-2 = $2F = BRA opcode.
        // port-3 = $FE = -2 operand (loop forever at $00F6 without help).
        state.main_to_spc_ports = [0xE8, 0x00, 0x2F, 0xFE];
        // SPC PC at $00F6 (BRA opcode lives in port-2). Sentinel A=$CC so any
        // executed `MOV A,#imm` micro-op is detectable.
        state.spc700.pc = 0x00F6;
        state.spc700.a = 0xCC;
        state.spc700.sp = 0xEF;
        state
    }

    #[test]
    fn trampoline_idles_in_bra_wait_loop_when_port3_holds_fe() {
        // Smoke check: without any host release pulses, the SPC must NOT
        // execute any queued micro-op. A must remain at the sentinel.
        let mut apu = SnesApu::new(None);
        apu.restore_state(&ipl_hack_trampoline_state())
            .expect("restore trampoline state");

        for _ in 0..10_000 {
            apu.tick();
        }

        assert_eq!(
            apu.spc700.a(),
            0xCC,
            "SPC must idle in BRA $FE wait loop and not consume the queued micro-op"
        );
    }

    #[test]
    #[ignore = "Aspirational: brief (<1 SPC cycle) host pulses require full \
                cycle-stepping AND alignment helpers in the host. Tracked under \
                #2908; the cycle-precise variant below is the proximate goal."]
    fn trampoline_executes_one_micro_op_per_brief_port3_pulse() {
        // Discriminating reproducer for #2908. Releases 10 distinct
        // `MOV A,#imm` micro-ops via the blargg-style brief port-3 pulse and
        // asserts the final A == the last operand released.
        //
        // The pulse window (4 master cycles, ≈ 20% of one SPC sub-cycle) is
        // intentionally narrow: the host writes port-3 = $FE again well before
        // the next atomic SPC instruction boundary, so atomic stepping samples
        // port-3 = $FE and never branches. Cycle-accurate stepping samples
        // port-3 at the BRA's operand-fetch sub-cycle, which falls inside the
        // pulse window with high probability across 10 pulses.
        let mut apu = SnesApu::new(None);
        apu.restore_state(&ipl_hack_trampoline_state())
            .expect("restore trampoline state");

        // Let the SPC settle in the BRA wait loop.
        for _ in 0..500 {
            apu.tick();
        }

        // 10 distinct micro-ops, each loading a unique value into A.
        let releases = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA];
        for &imm in &releases {
            // Stage the next micro-op while SPC is in the wait loop.
            apu.write_main_port(0, 0xE8); // MOV A,#imm
            apu.write_main_port(1, imm);
            // Brief release pulse: $FC for 4 master cycles, then $FE.
            apu.write_main_port(3, 0xFC);
            for _ in 0..4 {
                apu.tick();
            }
            apu.write_main_port(3, 0xFE);
            // Wait long enough for the SPC to execute the micro-op and return
            // to the BRA wait loop before the next release.
            for _ in 0..2_000 {
                apu.tick();
            }
        }

        assert_eq!(
            apu.spc700.a(),
            0xAA,
            "SPC must execute the queued MOV A,#imm micro-op at every release pulse; \
             final A should match the last operand"
        );
    }

    /// Cycle-precise variant of the trampoline reproducer: synchronizes the
    /// host's port-3 toggle with the SPC's BRA operand-fetch sub-cycle by
    /// observing [`crate::snes::apu::spc700::Spc700::has_in_progress_op`] and
    /// the in-progress [`crate::snes::apu::spc700::cpu::InProgressOp::cycle`]
    /// counter.
    ///
    /// This test discriminates: it can only pass if the SPC's port-3 read
    /// (BRA cycle 2) happens AFTER the host has written `$FC` AND BEFORE the
    /// host restores `$FE`. Under fully-atomic stepping the BRA's reads of
    /// both opcode ($00F6 = port-2) and operand ($00F7 = port-3) happen at
    /// the same master cycle, so writing `$FC` between the two reads is
    /// impossible. Under per-SPC-cycle stepping the reads are separated by
    /// one SPC cycle (~21 master cycles), opening exactly the window the
    /// blargg trampoline ROMs depend on.
    #[test]
    fn cycle_stepper_observes_port3_pulse_between_bra_opcode_and_operand_fetch() {
        let mut apu = SnesApu::new(None);
        apu.restore_state(&ipl_hack_trampoline_state())
            .expect("restore trampoline state");

        // Stage one micro-op: MOV A,#$5A.
        apu.write_main_port(0, 0xE8);
        apu.write_main_port(1, 0x5A);
        apu.write_main_port(3, 0xFE);

        // Walk the SPC forward until it has just finished cycle 1 (opcode
        // fetch) of a BRA at $00F6. At this point PC has been bumped to
        // $00F7 and the next cycle to run is the operand fetch which reads
        // port-3.
        let mut found = false;
        for _ in 0..10_000 {
            if let Some(ref op) = apu.spc700.in_progress
                && op.opcode == 0x2F
                && op.cycle == 1
                && apu.spc700.pc() == 0x00F7
            {
                found = true;
                break;
            }
            apu.tick();
        }
        assert!(
            found,
            "cycle stepper never reached BRA opcode-fetch boundary; \
             cycle-scripted dispatch may be broken"
        );

        // Open the release window: write $FC while SPC is between opcode-fetch
        // and operand-fetch cycles. The next cycle-stepper step will read $FC.
        apu.write_main_port(3, 0xFC);

        // Advance enough master cycles to retire the BRA (3 remaining cycles
        // ≈ 64 master cycles) and the queued MOV A,#imm (2 cycles ≈ 43 master
        // cycles), plus margin. Hold $FC for the BRA's operand fetch only.
        for _ in 0..32 {
            apu.tick();
        }
        apu.write_main_port(3, 0xFE);
        for _ in 0..500 {
            apu.tick();
        }

        assert_eq!(
            apu.spc700.a(),
            0x5A,
            "BRA operand fetch must observe port-3=$FC and branch to $00F4, \
             then MOV A,#$5A must execute"
        );
    }
}
