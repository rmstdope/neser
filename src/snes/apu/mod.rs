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
                    tick_timers: true,
                };
                i64::from(self.spc700.step(&mut bus_view))
            };
            if self.test & 0x04 != 0 {
                self.spc700.halt();
            }
            self.spc_cycle_budget -= consumed_cycles * SPC_PER_MASTER_DEN;
        }

        self.step_audio_clock();
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
        let normalized_dsp_addr = state.dsp_addr & 0x7F;

        self.main_to_spc_ports = state.main_to_spc_ports;
        self.spc_to_main_ports = state.spc_to_main_ports;
        self.control = state.control;
        self.test = state.test;
        self.master_ticks = state.master_ticks;
        self.spc_cycle_budget = state.spc_cycle_budget;
        self.timers = state.timers.clone();
        self.dsp = normalized_dsp;
        self.dsp_addr = normalized_dsp_addr;
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
        if self.test & 0x04 != 0 {
            self.spc700.halt();
        }
        Ok(())
    }

    fn reset_spc700(&mut self) {
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
            tick_timers: false,
        };
        self.spc700.reset(&mut bus_view);
    }

    fn step_audio_clock(&mut self) {
        self.step_native_audio_clock();
        self.step_output_audio_clock();
    }

    fn render_stereo_sample_internal(&mut self) -> (f32, f32) {
        self.dsp.render_stereo_sample_with_memory(&mut self.aram)
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
            tick_timers: true,
        };
        bus_view.write(addr, value);
        if self.test & 0x04 != 0 {
            self.spc700.halt();
        }
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
            tick_timers: true,
        };
        bus_view.write(0x00F1, value);
        if self.test & 0x04 != 0 {
            self.spc700.halt();
        }
    }

    #[cfg(test)]
    pub(crate) fn dsp_phase_for_test(&self) -> u8 {
        self.dsp.phase()
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
        self.test_reg() & 0x02 != 0
    }

    fn is_internal_wait_address(&self, addr: u16) -> bool {
        matches!(addr, 0x00F0..=0x00FF) || (0xFFC0..=0xFFFF).contains(&addr) && self.ipl_enabled()
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

    fn write_control(&mut self, value: u8) {
        let old_control = *self.control;
        *self.control = value;
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
        *self.test = value;
    }

    fn tick_timers_if_enabled(&mut self) {
        if self.tick_timers {
            self.timers.tick_cycle();
            self.dsp.step_phase_with_memory(&self.aram[..]);
        }
    }

    fn tick_timers_multiple(&mut self, cycles: u8) {
        for _ in 0..cycles {
            self.tick_timers_if_enabled();
        }
    }
}

impl Spc700Bus for SpcBusView<'_> {
    fn read_cycles(&self, addr: u16) -> u8 {
        self.wait_cycles_for_addr(Some(addr))
    }

    fn read(&mut self, addr: u16) -> u8 {
        let cycles = self.read_cycles(addr);
        let value = match addr {
            0x00F0 => 0x00,
            0x00F1 => *self.control,
            0x00F2 => *self.dsp_addr,
            0x00F3 => self.dsp.read_reg(*self.dsp_addr),
            0x00F4..=0x00F7 => self.main_to_spc_ports[(addr - 0x00F4) as usize],
            0x00FD..=0x00FF => self.timers.read_counter((addr - 0x00FD) as usize),
            0xFFC0..=0xFFFF if self.ipl_enabled() => self.ipl[(addr - 0xFFC0) as usize],
            _ => self.aram[addr as usize],
        };
        if addr <= 0x0001 {
            trace_apu!(4; "SPC reads ARAM[${:04X}] -> ${:02X}", addr, value);
        }
        if (0x00F4..=0x00F7).contains(&addr) {
            trace_apu!(3; "SPC reads port[{}] -> ${:02X}", addr - 0x00F4, value);
        }
        self.tick_timers_multiple(cycles);
        value
    }

    fn write_cycles(&self, addr: u16) -> u8 {
        self.wait_cycles_for_addr(Some(addr))
    }

    fn write(&mut self, addr: u16, value: u8) {
        let cycles = self.write_cycles(addr);
        match addr {
            0x00F0 => {
                self.write_test(value);
            }
            0x00F1 => self.write_control(value),
            0x00F2 => *self.dsp_addr = value & 0x7F,
            0x00F3 => self.dsp.write_reg(*self.dsp_addr, value),
            0x00F4..=0x00F7 => {
                let port_idx = (addr - 0x00F4) as usize;
                trace_apu!(2; "SPC writes port[{}] = ${:02X}", port_idx, value);
                self.spc_to_main_ports[port_idx] = value;
            }
            0x00FA..=0x00FC => self.timers.write_target((addr - 0x00FA) as usize, value),
            _ => {
                if self.ram_write_enabled() {
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
            }
        }
        self.tick_timers_multiple(cycles);
    }

    fn idle_cycles(&self) -> u8 {
        self.wait_cycles_for_addr(None)
    }

    fn idle(&mut self) {
        let cycles = self.idle_cycles();
        self.tick_timers_multiple(cycles);
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
        apu.advance_spc_bus_cycles_for_test(126);

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
    fn dsp_f2_address_masks_to_7_bit_register_space() {
        let mut apu = SnesApu::new(None);

        apu.write_spc_memory_for_test(0x00F2, 0x10);
        apu.write_spc_memory_for_test(0x00F3, 0x34);

        apu.write_spc_memory_for_test(0x00F2, 0x90);
        assert_eq!(apu.read_spc_memory_for_test(0x00F3), 0x34);
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
    fn restore_state_masks_dsp_address_to_7_bit() {
        let mut apu = SnesApu::new(None);
        let mut state = apu.capture_state();
        state.dsp_addr = 0xFF;

        apu.restore_state(&state).expect("restore should succeed");
        assert_eq!(apu.read_spc_memory_for_test(0x00F2), 0x7F);
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
}
