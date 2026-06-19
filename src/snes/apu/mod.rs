//! SNES APU (Audio Processing Unit) bootstrap path.
//!
//! This slice wires the 64 KB ARAM, clean-room IPL boot ROM overlay, SPC700 CPU,
//! and the four communication ports (`$2140-$2143` <-> `$F4-$F7`).

pub mod ipl;
pub mod spc700;

use serde::{Deserialize, Serialize};
use spc700::{Spc700, Spc700Bus};

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
    master_ticks: u64,
    /// Signed budget in numerator units for fractional SPC catch-up.
    spc_cycle_budget: i64,
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
            master_ticks: 0,
            spc_cycle_budget: 0,
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
        }
    }

    pub fn restore_state(&mut self, state: &SnesApuState) -> Result<(), String> {
        if !state.aram.is_empty() && state.aram.len() != ARAM_SIZE {
            return Err(format!(
                "APU ARAM size mismatch (expected {ARAM_SIZE}, found {})",
                state.aram.len()
            ));
        }
        if state.aram.len() == ARAM_SIZE {
            self.aram.copy_from_slice(&state.aram);
        }
        self.main_to_spc_ports = state.main_to_spc_ports;
        self.spc_to_main_ports = state.spc_to_main_ports;
        self.control = state.control;
        self.master_ticks = state.master_ticks;
        self.spc_cycle_budget = state.spc_cycle_budget;
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
        };
        bus_view.read(addr)
    }

    #[cfg(test)]
    pub(crate) fn write_spc_control_for_test(&mut self, value: u8) {
        let mut bus_view = SpcBusView {
            aram: &mut self.aram,
            ipl: &self.ipl,
            main_to_spc_ports: &mut self.main_to_spc_ports,
            spc_to_main_ports: &mut self.spc_to_main_ports,
            control: &mut self.control,
        };
        bus_view.write(0x00F1, value);
    }
}

struct SpcBusView<'a> {
    aram: &'a mut [u8; ARAM_SIZE],
    ipl: &'a [u8; 64],
    main_to_spc_ports: &'a mut [u8; 4],
    spc_to_main_ports: &'a mut [u8; 4],
    control: &'a mut u8,
}

impl SpcBusView<'_> {
    fn ipl_enabled(&self) -> bool {
        *self.control & 0x80 != 0
    }

    fn write_control(&mut self, value: u8) {
        *self.control = value;
        if value & 0x10 != 0 {
            self.main_to_spc_ports[0] = 0;
            self.main_to_spc_ports[1] = 0;
        }
        if value & 0x20 != 0 {
            self.main_to_spc_ports[2] = 0;
            self.main_to_spc_ports[3] = 0;
        }
    }
}

impl Spc700Bus for SpcBusView<'_> {
    fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0x00F1 => *self.control,
            0x00F4..=0x00F7 => self.main_to_spc_ports[(addr - 0x00F4) as usize],
            0xFFC0..=0xFFFF if self.ipl_enabled() => self.ipl[(addr - 0xFFC0) as usize],
            _ => self.aram[addr as usize],
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x00F1 => self.write_control(value),
            0x00F4..=0x00F7 => self.spc_to_main_ports[(addr - 0x00F4) as usize] = value,
            _ => self.aram[addr as usize] = value,
        }
    }

    fn idle(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::SnesApu;

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
}
