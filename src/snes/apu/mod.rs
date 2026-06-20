//! SNES APU (Audio Processing Unit) bootstrap path.
//!
//! This slice wires the 64 KB ARAM, clean-room IPL boot ROM overlay, SPC700 CPU,
//! and the four communication ports (`$2140-$2143` <-> `$F4-$F7`).

pub mod dsp;
pub mod ipl;
pub mod spc700;
pub mod timers;

use crate::snes::apu::dsp::Sdsp;
use serde::{Deserialize, Serialize};
use spc700::{Spc700, Spc700Bus};
use timers::SpcTimers;

const ARAM_SIZE: usize = 0x1_0000;
const SPC_PER_MASTER_NUM: i64 = 1_024_000;
const SPC_PER_MASTER_DEN: i64 = 21_477_272;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SnesApuState {
    #[serde(default)]
    pub aram: Vec<u8>,
    #[serde(default)]
    pub main_to_spc_ports: [u8; 4],
    #[serde(default)]
    pub spc_to_main_ports: [u8; 4],
    #[serde(default)]
    pub control: u8,
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
    timers: SpcTimers,
    master_ticks: u64,
    /// Signed budget in numerator units for fractional SPC catch-up.
    spc_cycle_budget: i64,
    dsp: Sdsp,
    dsp_addr: u8,
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
            timers: SpcTimers::default(),
            master_ticks: 0,
            spc_cycle_budget: 0,
            dsp: Sdsp::new(),
            dsp_addr: 0,
        };
        apu.reset_spc700();
        apu
    }

    pub fn read_main_port(&self, port: usize) -> u8 {
        self.spc_to_main_ports[port]
    }

    pub fn write_main_port(&mut self, port: usize, value: u8) {
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
                    timers: &mut self.timers,
                    dsp: &mut self.dsp,
                    dsp_addr: &mut self.dsp_addr,
                    tick_timers: true,
                };
                i64::from(self.spc700.step(&mut bus_view))
            };
            self.spc_cycle_budget -= consumed_cycles * SPC_PER_MASTER_DEN;
        }
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
            master_ticks: self.master_ticks,
            spc_cycle_budget: self.spc_cycle_budget,
            timers: self.timers.clone(),
            dsp: self.dsp.clone(),
            dsp_addr: self.dsp_addr,
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
            self.master_ticks = 0;
            self.spc_cycle_budget = 0;
            self.timers = SpcTimers::default();
            self.dsp = Sdsp::new();
            self.dsp_addr = 0;
            self.reset_spc700();
            return Ok(());
        }

        if state.aram.len() == ARAM_SIZE {
            self.aram.copy_from_slice(&state.aram);
        }
        self.main_to_spc_ports = state.main_to_spc_ports;
        self.spc_to_main_ports = state.spc_to_main_ports;
        self.control = state.control;
        self.master_ticks = state.master_ticks;
        self.spc_cycle_budget = state.spc_cycle_budget;
        self.timers = state.timers.clone();
        self.dsp = state.dsp.clone();
        self.dsp.normalize_after_restore()?;
        self.dsp_addr = state.dsp_addr & 0x7F;
        self.reset_spc700();
        Ok(())
    }

    fn reset_spc700(&mut self) {
        let mut bus_view = SpcBusView {
            aram: &mut self.aram,
            ipl: &self.ipl,
            main_to_spc_ports: &mut self.main_to_spc_ports,
            spc_to_main_ports: &mut self.spc_to_main_ports,
            control: &mut self.control,
            timers: &mut self.timers,
            dsp: &mut self.dsp,
            dsp_addr: &mut self.dsp_addr,
            tick_timers: false,
        };
        self.spc700.reset(&mut bus_view);
    }

    #[cfg(test)]
    pub(crate) fn read_spc_memory_for_test(&mut self, addr: u16) -> u8 {
        let mut bus_view = SpcBusView {
            aram: &mut self.aram,
            ipl: &self.ipl,
            main_to_spc_ports: &mut self.main_to_spc_ports,
            spc_to_main_ports: &mut self.spc_to_main_ports,
            control: &mut self.control,
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
            timers: &mut self.timers,
            dsp: &mut self.dsp,
            dsp_addr: &mut self.dsp_addr,
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
            timers: &mut self.timers,
            dsp: &mut self.dsp,
            dsp_addr: &mut self.dsp_addr,
            tick_timers: true,
        };
        bus_view.write(0x00F1, value);
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
    timers: &'a mut SpcTimers,
    dsp: &'a mut Sdsp,
    dsp_addr: &'a mut u8,
    tick_timers: bool,
}

impl SpcBusView<'_> {
    fn ipl_enabled(&self) -> bool {
        *self.control & 0x80 != 0
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

    fn tick_timers_if_enabled(&mut self) {
        if self.tick_timers {
            self.timers.tick_cycle();
            self.dsp.step_phase();
        }
    }
}

impl Spc700Bus for SpcBusView<'_> {
    fn read(&mut self, addr: u16) -> u8 {
        let value = match addr {
            0x00F1 => *self.control,
            0x00F2 => *self.dsp_addr,
            0x00F3 => self.dsp.read_reg(*self.dsp_addr),
            0x00F4..=0x00F7 => self.main_to_spc_ports[(addr - 0x00F4) as usize],
            0x00FD..=0x00FF => self.timers.read_counter((addr - 0x00FD) as usize),
            0xFFC0..=0xFFFF if self.ipl_enabled() => self.ipl[(addr - 0xFFC0) as usize],
            _ => self.aram[addr as usize],
        };
        self.tick_timers_if_enabled();
        value
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x00F1 => self.write_control(value),
            0x00F2 => *self.dsp_addr = value & 0x7F,
            0x00F3 => self.dsp.write_reg(*self.dsp_addr, value),
            0x00F4..=0x00F7 => self.spc_to_main_ports[(addr - 0x00F4) as usize] = value,
            0x00FA..=0x00FC => self.timers.write_target((addr - 0x00FA) as usize, value),
            _ => self.aram[addr as usize] = value,
        }
        self.tick_timers_if_enabled();
    }

    fn idle(&mut self) {
        self.tick_timers_if_enabled();
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
        let mut state = apu.capture_state();
        state.dsp = serde_json::from_str(r#"{"phase":0,"regs":[1,2]}"#)
            .expect("deserialize malformed DSP for test");

        let err = apu
            .restore_state(&state)
            .expect_err("restore should reject invalid DSP register length");
        assert!(err.contains("APU DSP register file size mismatch"));
    }
}
