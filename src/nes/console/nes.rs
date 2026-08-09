use crate::nes::apu::{Apu, ApuState, SharedApu};
use crate::nes::bus::{Bus, BusState, MapperState, SharedBus};
#[cfg(test)]
use crate::nes::cartridge::TimingMode;
use crate::nes::cartridge::{Cartridge, RomDb, load_rom_db};
use crate::nes::console::ApuChannels;
#[cfg(test)]
use crate::nes::console::Config;
#[cfg(test)]
use crate::nes::console::NesConfig;
use crate::nes::cpu::lookup;
use crate::nes::cpu::opcode::{AddrMode, Mnemonic};
use crate::nes::cpu::{Cpu, CpuState};
use crate::nes::input::ControllerType;
use crate::nes::ppu::{Ppu, PpuState, SharedPpu};
use crate::platform::app_context::{IntoSharedAppContext, SharedAppContext};
use crate::platform::debugging::{Tracing, log_info};
use crate::platform::emulator::{Emulator, SystemType};
use crate::platform::save_state::{SaveStateError, Stateful};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::io;
use std::path::PathBuf;
use std::rc::Rc;

/// Current save-state format version.
/// Increment this when making breaking changes to the state format.
pub const SAVESTATE_VERSION: u32 = 8;

/// Complete emulator state snapshot.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SaveState {
    /// Version of the save-state format
    pub version: u32,
    /// CPU state
    pub cpu: CpuState,
    /// PPU state
    pub ppu: PpuState,
    /// APU state
    pub apu: ApuState,
    /// Bus state
    pub bus: BusState,
    /// CPU RAM (2KB, mirrored to 8KB)
    pub ram: Vec<u8>,
    /// Mapper state (serialized as opaque bytes)
    pub mapper: MapperState,
}

impl SaveState {
    /// Create a new save state with the current version.
    pub fn new(
        cpu: CpuState,
        ppu: PpuState,
        apu: ApuState,
        bus: BusState,
        ram: Vec<u8>,
        mapper: MapperState,
    ) -> Self {
        Self {
            version: SAVESTATE_VERSION,
            cpu,
            ppu,
            apu,
            bus,
            ram,
            mapper,
        }
    }

    /// Serialize the save state to JSON.
    #[cfg(test)]
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize a save state from JSON.
    #[cfg(test)]
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize the save state to JSON-encoded UTF-8 bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, SaveStateError> {
        crate::platform::save_state::to_bytes(self)
    }

    /// Deserialize a save state from JSON-encoded UTF-8 bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SaveStateError> {
        crate::platform::save_state::from_bytes(bytes)
    }

    /// Serialize the save state to compact binary (postcard) bytes.
    pub fn to_binary_bytes(&self) -> Result<Vec<u8>, postcard::Error> {
        postcard::to_allocvec(self)
    }

    /// Deserialize a save state from compact binary (postcard) bytes.
    pub fn from_binary_bytes(bytes: &[u8]) -> Result<Self, postcard::Error> {
        postcard::from_bytes(bytes)
    }
}

const MAX_CPU_TRACE_LINES: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuTraceLine {
    pub addr: u16,
    pub bytes: Vec<u8>,
    pub text: String,
}

pub struct Nes {
    app_context: SharedAppContext,
    rom_db: RomDb,
    ppu: SharedPpu,
    apu: SharedApu,
    bus: SharedBus,
    cpu: Cpu,
    fractional_ppu_cycles: f64,
    ready_to_render: bool,
    recent_cpu_trace: VecDeque<CpuTraceLine>,
    /// When false, CPU trace capture is skipped for performance.
    /// Enable only when the debugger is open.
    cpu_trace_enabled: bool,
    /// Effective controller types for the current cartridge.
    /// May differ from config when auto-detection overrides user defaults.
    active_controller_port1: ControllerType,
    active_controller_port2: ControllerType,
}

impl Nes {
    /// NES screen width in pixels.
    pub const SCREEN_WIDTH: u32 = 256;
    /// NES screen height in pixels.
    pub const SCREEN_HEIGHT: u32 = 240;

    pub fn new<C: IntoSharedAppContext>(app_context: C) -> Self {
        let app_context = app_context.into_shared();
        let config = app_context.borrow().config().clone();
        let tv_system = config.nes.hardware_model.timing_mode();
        let ram_init_mode = config.frontend.ram_init_mode;
        let oam_dram_decay_enabled = config.nes.oam_dram_decay_enabled;
        let ppu = Rc::new(RefCell::new(Ppu::new(tv_system, ram_init_mode)));
        ppu.borrow_mut()
            .set_oam_dram_decay_enabled(oam_dram_decay_enabled);
        ppu.borrow_mut().set_famicom_emphasis(
            config.nes.hardware_mode == crate::nes::console::HardwareMode::Famicom,
        );
        ppu.borrow_mut().set_system_palette(config.nes.palette);
        let apu = Rc::new(RefCell::new(Apu::new_with_tv_system(tv_system)));
        {
            let channels = config.nes.apu_channels;
            let mut apu = apu.borrow_mut();
            apu.set_pulse1_enabled(channels.contains(ApuChannels::PULSE1));
            apu.set_pulse2_enabled(channels.contains(ApuChannels::PULSE2));
            apu.set_triangle_enabled(channels.contains(ApuChannels::TRIANGLE));
            apu.set_noise_enabled(channels.contains(ApuChannels::NOISE));
            apu.set_dmc_enabled(channels.contains(ApuChannels::DMC));
        }
        let memory = Rc::new(RefCell::new(Bus::new(
            ppu.clone(),
            apu.clone(),
            app_context.clone(),
        )));
        let cpu = Cpu::new(tv_system, memory.clone(), ppu.clone(), apu.clone());

        // Initialize PPU 1 cycle ahead for proper sprite 0 hit timing
        // This creates a one-cycle offset where PPU state changes become
        // visible to the CPU one cycle later, matching hardware behavior
        ppu.borrow_mut().run_ppu_cycles(1);

        Self {
            app_context,
            rom_db: load_rom_db(),
            ppu,
            apu,
            bus: memory,
            cpu,
            fractional_ppu_cycles: 0.0,
            ready_to_render: false,
            recent_cpu_trace: VecDeque::with_capacity(MAX_CPU_TRACE_LINES),
            cpu_trace_enabled: false,
            active_controller_port1: config.nes.controller_port1,
            active_controller_port2: config.nes.controller_port2,
        }
    }

    /// The ROM database used for CRC-based metadata lookups.
    pub fn rom_db(&self) -> &RomDb {
        &self.rom_db
    }

    /// The currently active preset system palette.
    pub fn current_palette(&self) -> crate::nes::ppu::NesPalette {
        self.ppu.borrow().system_palette()
    }

    /// Advances to the next preset system palette and returns the new value.
    pub fn cycle_palette(&mut self) -> crate::nes::ppu::NesPalette {
        self.ppu.borrow_mut().cycle_system_palette()
    }

    /// Insert a cartridge and map it into memory.
    /// Auto-configures Arkanoid or Zapper controllers for known ROMs only when no controller
    /// ports were explicitly configured by the user.
    pub fn insert_cartridge(&mut self, mut cartridge: Cartridge) {
        let cartridge_crc32 = cartridge.crc32();
        let zapper_port = self.rom_db.default_zapper_on_port(cartridge_crc32);
        let arkanoid_port = crate::nes::cartridge::default_arkanoid_on_port(cartridge_crc32);
        let power_pad_port = self.rom_db.default_power_pad_on_port(cartridge_crc32);
        let has_famicom_four_players_expansion = self
            .rom_db
            .has_famicom_four_players_expansion(cartridge_crc32);
        let has_arkanoid_famicom_expansion =
            self.rom_db.has_arkanoid_famicom_expansion(cartridge_crc32);
        let has_zapper_famicom_expansion =
            self.rom_db.has_zapper_famicom_expansion(cartridge_crc32);
        let has_power_pad_famicom_expansion =
            self.rom_db.has_power_pad_famicom_expansion(cartridge_crc32);
        let has_nes_four_score_expansion =
            self.rom_db.has_nes_four_score_expansion(cartridge_crc32);
        let is_japan_region = self.rom_db.is_japan_region(cartridge_crc32);

        // Auto-detect Famicom hardware mode for Japan-region ROMs
        self.app_context
            .borrow_mut()
            .config_mut()
            .apply_rom_db_famicom_region_hint(is_japan_region);

        self.app_context
            .borrow_mut()
            .config_mut()
            .apply_rom_db_famicom_four_players_hint(has_famicom_four_players_expansion);

        self.app_context
            .borrow_mut()
            .config_mut()
            .apply_rom_db_arkanoid_famicom_hint(has_arkanoid_famicom_expansion);

        self.app_context
            .borrow_mut()
            .config_mut()
            .apply_rom_db_zapper_famicom_hint(has_zapper_famicom_expansion);

        self.app_context
            .borrow_mut()
            .config_mut()
            .apply_rom_db_power_pad_famicom_hint(has_power_pad_famicom_expansion);

        self.app_context
            .borrow_mut()
            .config_mut()
            .apply_rom_db_nes_four_score_hint(has_nes_four_score_expansion);

        // Auto-detect VS System mode from cartridge VS metadata
        let is_vs_system =
            cartridge.vs_ppu_type().is_some() || cartridge.vs_hardware_type().is_some();
        self.app_context
            .borrow_mut()
            .config_mut()
            .apply_rom_db_vs_system_hint(is_vs_system);

        let is_playchoice10 = matches!(
            cartridge.hardware_type(),
            crate::nes::cartridge::HardwareType::Playchoice10
        );
        self.app_context
            .borrow_mut()
            .config_mut()
            .apply_rom_db_playchoice10_hint(is_playchoice10);

        // Auto-detect VS System swapped controller wiring from ROM DB expansion type
        let vs_controllers_swapped = self.rom_db.has_vs_swapped_controllers(cartridge_crc32);
        self.app_context
            .borrow_mut()
            .config_mut()
            .apply_rom_db_vs_controllers_swapped_hint(vs_controllers_swapped);

        // Propagate any hardware-mode change from ROM DB hint to the live PPU
        let is_famicom = self.app_context.borrow().config().nes.hardware_mode
            == crate::nes::console::HardwareMode::Famicom;
        self.ppu.borrow_mut().set_famicom_emphasis(is_famicom);

        // Initialize cartridge RAM (PRG-RAM and CHR-RAM) based on config
        let ram_init_mode = self.app_context.borrow().config().frontend.ram_init_mode;
        cartridge.initialize_ram(ram_init_mode);

        {
            let mut bus = self.bus.borrow_mut();
            bus.map_cartridge(cartridge);
            // Sync controller modes so the bus reflects any config changes from ROM DB
            // auto-detection.
            bus.sync_controller_modes_from_config();
        } // bus borrow released here

        // Update cached mapper capability flags so the CPU hot path can skip
        // unnecessary per-cycle RefCell borrows for non-IRQ / non-expansion-audio mappers.
        self.cpu.update_mapper_capability_flags();

        // Get controller config and explicit flags from stored config
        let app_context = self.app_context.borrow();
        let config = app_context.config();
        let port1_type = config.nes.controller_port1;
        let port2_type = config.nes.controller_port2;
        let port1_explicit = config.nes.controller_port1_explicit;
        let port2_explicit = config.nes.controller_port2_explicit;
        drop(app_context); // Release borrow early

        // If any controller port is explicitly configured, disable ROM DB auto-detection
        // for all ports and apply configured values directly.
        if port1_explicit || port2_explicit {
            let mut bus = self.bus.borrow_mut();
            bus.set_controller_type(1, port1_type);
            bus.set_controller_type(2, port2_type);
            self.log_hardware_summary();
            return;
        }

        // When the Zapper is on the Famicom expansion port, don't also put it on a
        // standard controller port — the expansion port read path handles it.
        let zapper_on_expansion = self.app_context.borrow().config().nes.expansion_port
            == crate::nes::console::ExpansionPort::ZapperFamicom;

        let auto_controller = if zapper_port != 0 && !zapper_on_expansion {
            Some((zapper_port, ControllerType::Zapper))
        } else if arkanoid_port != 0 {
            Some((arkanoid_port, ControllerType::Arkanoid))
        } else if power_pad_port != 0 {
            Some((power_pad_port, ControllerType::PowerPad))
        } else {
            None
        };

        // Auto-configure special controller when detected.
        // This branch only runs when neither controller port was explicitly configured.
        if let Some((auto_port, auto_type)) = auto_controller {
            let other_port_type = if auto_port == 1 {
                port2_type
            } else {
                port1_type
            };
            let auto_label = auto_type.display_label();
            log_info(format!(
                "Auto-detected {} on port {} for this cartridge. Override with --controller-port{}=<type> if needed.",
                auto_label, auto_port, auto_port
            ));
            let mut bus = self.bus.borrow_mut();
            bus.set_controller_type(auto_port, auto_type);

            // Track effective controller types as per-cartridge runtime state.
            // Config is intentionally NOT mutated so subsequent ROM loads start
            // from the user-configured defaults, not from a previous auto-detection.
            if auto_port == 1 {
                self.active_controller_port1 = auto_type;
                self.active_controller_port2 = other_port_type;
            } else {
                self.active_controller_port1 = other_port_type;
                self.active_controller_port2 = auto_type;
            }

            // Apply the other port's configuration
            let other_port = if auto_port == 1 { 2 } else { 1 };
            bus.set_controller_type(other_port, other_port_type);
        } else {
            // No special controller detected, just apply user config
            let mut bus = self.bus.borrow_mut();
            bus.set_controller_type(1, port1_type);
            bus.set_controller_type(2, port2_type);
            self.active_controller_port1 = port1_type;
            self.active_controller_port2 = port2_type;
        }

        self.log_hardware_summary();
    }

    /// Returns the effective controller type for the given port (1 or 2) for the currently
    /// loaded cartridge. May differ from config when auto-detection overrides user defaults.
    pub fn active_controller_port_type(&self, port: u8) -> ControllerType {
        match port {
            1 => self.active_controller_port1,
            2 => self.active_controller_port2,
            _ => ControllerType::Joypad,
        }
    }

    fn log_hardware_summary(&self) {
        let summary = self
            .app_context
            .borrow()
            .config()
            .hardware_summary_with(self.active_controller_port1, self.active_controller_port2);
        log_info(summary);
    }

    pub fn state_path(&self) -> Option<PathBuf> {
        self.bus.borrow().cartridge_state_path()
    }

    pub fn save_ram(&self) -> io::Result<()> {
        self.bus.borrow().save_ram()
    }

    pub fn debug_path(&self) -> Option<PathBuf> {
        self.bus.borrow().cartridge_debug_path()
    }

    pub fn app_context(&self) -> &SharedAppContext {
        &self.app_context
    }

    pub fn ppu(&self) -> &SharedPpu {
        &self.ppu
    }

    pub fn apu(&self) -> &SharedApu {
        &self.apu
    }

    /// Set the audio output sample rate (Hz) for the APU's resampler.
    pub fn set_audio_sample_rate(&mut self, rate: f32) {
        self.apu.borrow_mut().set_sample_rate(rate);
    }

    pub fn bus(&self) -> &SharedBus {
        &self.bus
    }

    pub fn cpu_ref(&self) -> &Cpu {
        &self.cpu
    }

    pub fn cpu_mut(&mut self) -> &mut Cpu {
        &mut self.cpu
    }

    /// Reset the NES system.
    ///
    /// - `soft_reset`: true for a reset-button style reset, false for power-on
    pub fn reset(&mut self, soft_reset: bool) {
        // Get CPU cycle count before reset for coordinated APU timing
        let cpu_cycle = self.cpu.get_total_cycles();
        let ram_init_mode = self.app_context.borrow().config().frontend.ram_init_mode;

        // Reset components - each handles its own RAM initialization on hard reset
        self.ppu.borrow_mut().reset(soft_reset, ram_init_mode);
        self.apu.borrow_mut().reset(cpu_cycle, soft_reset);
        self.bus.borrow_mut().reset(soft_reset, ram_init_mode);
        self.cpu_mut().reset(soft_reset);

        // Reinitialize DMC timer phase after CPU reset. The CPU reset runs 7
        // internal cycles that clock the APU (including the DMC timer). On real
        // hardware the timer effectively starts from its full period value once
        // user code begins executing, so we restore it to the correct phase.
        self.apu.borrow_mut().dmc_mut().reinit_timer_after_reset();

        if !soft_reset {
            self.start_trainer_if_present();
        }

        self.fractional_ppu_cycles = 0.0;
        self.ready_to_render = false;
        self.recent_cpu_trace.clear();

        // Re-establish 1-cycle PPU offset after reset
        self.ppu.borrow_mut().run_ppu_cycles(1);
    }

    /// On hard reset with a trainer: simulate JSR $7003 so trainer code runs
    /// before the game's reset vector. Pushes (game_vector − 1) to the stack
    /// and redirects PC to $7003. The trainer must end with RTS to return.
    fn start_trainer_if_present(&mut self) {
        if !self.bus.borrow().cartridge_has_trainer_jsr() {
            return;
        }
        let game_vector = self.cpu.pc();
        self.cpu.divert_to_trainer(game_vector);
    }

    /// Run one CPU "tick", executing one opcode and the corresponding PPU cycles
    ///
    /// Returns the number of CPU cycles consumed by the opcode.
    ///
    /// The PPU runs at a different rate than the CPU:
    /// - NTSC: 3 PPU cycles per CPU cycle
    /// - PAL: 3.2 PPU cycles per CPU cycle
    ///
    /// For PAL, fractional cycles are accumulated to maintain timing accuracy.
    ///
    /// # Known Limitations
    ///
    /// This implementation ticks the PPU after each complete CPU opcode, not cycle-by-cycle.
    /// This means PPU register writes (PPUCTRL, PPUMASK, PPUSCROLL, etc.) take effect for
    /// all PPU cycles in the opcode, rather than at the exact cycle when the write occurs.
    /// This can cause timing issues with ROMs that update PPU registers mid-scanline for
    /// visual effects. Most games work fine, but some test ROMs (like palette.nes) may
    /// show minor rendering artifacts due to this limitation.
    pub fn run_cpu_tick(&mut self) -> u8 {
        if let Some(dma_cycles) = self.cpu.handle_oam_dma_if_pending() {
            // Return DMA cycles (capped at u8::MAX)
            return dma_cycles.min(255) as u8;
        }

        let cycles_before = self.cpu.get_total_cycles();

        if self.cpu_trace_enabled {
            let trace_line = self.capture_current_cpu_trace_line();
            self.push_cpu_trace_line(trace_line);
        }

        // Execute exactly one CPU instruction.
        self.cpu.execute();

        // IRQ/NMI are handled by the CPU itself at the end of `execute()`.

        if self.ppu.borrow_mut().poll_frame_complete() {
            self.ready_to_render = true;
        }

        let cycles_after = self.cpu.get_total_cycles();
        let cycles_consumed = cycles_after - cycles_before;
        cycles_consumed.min(255) as u8
    }

    /// Run a single CPU tick, optionally emitting a trace line before execution.
    pub fn run(&mut self, _tracing: &Tracing) -> u8 {
        // if tracing.enabled {
        //     println!("{}", self.trace(tracing));
        // }

        self.run_cpu_tick()
    }

    /// Enable or disable per-instruction CPU trace capture.
    /// Disable during normal emulation to avoid ~30k allocations/frame.
    pub fn set_cpu_trace_enabled(&mut self, enabled: bool) {
        self.cpu_trace_enabled = enabled;
        if !enabled {
            self.recent_cpu_trace.clear();
        }
    }

    pub fn recent_cpu_trace(&self, limit: usize) -> Vec<CpuTraceLine> {
        if limit == 0 || self.recent_cpu_trace.is_empty() {
            return Vec::new();
        }

        let start = self.recent_cpu_trace.len().saturating_sub(limit);
        self.recent_cpu_trace.iter().skip(start).cloned().collect()
    }

    fn push_cpu_trace_line(&mut self, line: CpuTraceLine) {
        if self.recent_cpu_trace.len() == MAX_CPU_TRACE_LINES {
            self.recent_cpu_trace.pop_front();
        }
        self.recent_cpu_trace.push_back(line);
    }

    fn capture_current_cpu_trace_line(&self) -> CpuTraceLine {
        let pc = self.cpu.pc();
        let memory = self.bus.borrow();
        let opcode = memory.read_cpu_for_debugger(pc);
        let instruction = lookup(opcode);
        let byte_len = instruction.bytes().max(1) as usize;

        let mut bytes = Vec::with_capacity(byte_len);
        for offset in 0..byte_len {
            bytes.push(memory.read_cpu_for_debugger(pc.wrapping_add(offset as u16)));
        }

        CpuTraceLine {
            addr: pc,
            text: format_compact_trace_instruction(instruction, pc, &bytes),
            bytes,
        }
    }

    /// NES default system palette - 64 RGB color values (0x00-0x3F).
    ///
    /// This is the historical built-in palette, also exposed as
    /// [`crate::nes::ppu::NesPalette::Default`]. Additional preset palettes live
    /// in `src/nes/ppu/system_palettes.rs` and can be selected at runtime.
    const SYSTEM_PALETTE: [(u8, u8, u8); 0x40] = crate::nes::ppu::system_palettes::PALETTE_DEFAULT;

    /// Maps NES color palette index (0-63) to RGB values using the default palette.
    ///
    /// This always uses the built-in default palette. Runtime palette selection
    /// is handled by the PPU via `Ppu::lookup_system_palette`.
    pub fn lookup_system_palette(color_index: u8) -> (u8, u8, u8) {
        Self::SYSTEM_PALETTE[(color_index & 0x3F) as usize]
    }

    /// Get a reference to the PPU's screen buffer
    ///
    /// Returns a mutable reference to the 256x240 RGB buffer containing the current frame.
    pub fn get_screen_buffer(&self) -> std::cell::RefMut<'_, crate::nes::ppu::ScreenBuffer> {
        std::cell::RefMut::map(self.ppu.borrow_mut(), |ppu| ppu.screen_buffer_mut())
    }

    /// Check if a frame is ready to be rendered
    ///
    /// Returns true when the PPU has completed a full frame (reached VBlank at scanline 241).
    /// After checking this flag, call `clear_ready_to_render()` to reset it for the next frame.
    pub fn is_ready_to_render(&self) -> bool {
        self.ready_to_render
    }

    /// Clear the ready-to-render flag after rendering a frame
    pub fn clear_ready_to_render(&mut self) {
        self.ready_to_render = false;
    }

    /// Check if an audio sample is ready for retrieval
    ///
    /// Returns true when the APU has generated a new audio sample.
    /// After checking this flag, call `get_sample()` to retrieve the sample.
    pub fn sample_ready(&self) -> bool {
        self.apu.borrow().sample_ready()
    }

    /// Get the next audio sample if one is ready
    ///
    /// Returns `Some(sample)` if a sample is available, `None` otherwise.
    /// After calling this, `sample_ready()` will return false until the next sample is generated.
    pub fn get_sample(&mut self) -> Option<f32> {
        self.apu.borrow_mut().get_sample()
    }

    /// Set button state for a controller
    ///
    /// # Arguments
    /// * `controller` - Controller number (1 or 2)
    /// * `button` - Which button to set
    /// * `pressed` - true if pressed, false if released
    pub fn set_button(&mut self, controller: u8, button: crate::nes::input::Button, pressed: bool) {
        self.bus
            .borrow_mut()
            .set_button(controller, button, pressed);
    }

    /// Set SNES-specific button state for a controller.
    pub fn set_snes_button(
        &mut self,
        controller: u8,
        button: crate::nes::input::SnesButton,
        pressed: bool,
    ) -> bool {
        self.bus
            .borrow_mut()
            .set_snes_button(controller, button, pressed)
    }

    /// Set Power Pad button state for a controller.
    pub fn set_power_pad_button(
        &mut self,
        controller: u8,
        button: crate::nes::input::PowerPadButton,
        pressed: bool,
    ) -> bool {
        self.bus
            .borrow_mut()
            .set_power_pad_button(controller, button, pressed)
    }

    pub fn set_expansion_power_pad_button(
        &mut self,
        button: crate::nes::input::PowerPadButton,
        pressed: bool,
    ) -> bool {
        self.bus
            .borrow_mut()
            .set_expansion_power_pad_button(button, pressed)
    }

    /// Set VS System coin insert state for a specific slot (0 or 1).
    pub fn set_vs_coin_insert(&self, slot: u8, pressed: bool) {
        self.bus.borrow().set_vs_coin_insert(slot, pressed);
    }

    /// Set VS System service button state.
    pub fn set_vs_service_button(&self, pressed: bool) {
        self.bus.borrow().set_vs_service_button(pressed);
    }

    /// Set all joypad button states from a u8 bitmask (for autorun playback).
    ///
    /// Each bit corresponds to a [`crate::nes::input::Button`] variant by its discriminant index.
    #[allow(dead_code)]
    pub fn set_joypad_button_states(&mut self, port: u8, state: u8) {
        use crate::nes::input::Button;
        for button in [
            Button::A,
            Button::B,
            Button::Select,
            Button::Start,
            Button::Up,
            Button::Down,
            Button::Left,
            Button::Right,
        ] {
            let pressed = (state & (1 << (button as u8))) != 0;
            self.set_button(port, button, pressed);
        }
    }

    /// Get joypad button states as a u8 bitmask (for autorun recording).
    pub fn get_joypad_button_states(&self, port: u8) -> u8 {
        self.bus.borrow().get_joypad_button_states(port)
    }

    /// Return the input type for a controller port.
    pub fn controller_input_type(&self, port: u8) -> Option<crate::nes::input::ControllerInput> {
        self.bus.borrow().controller_input_type(port)
    }

    /// Check if the expansion port has a mouse-controlled device.
    pub fn has_expansion_mouse_controller(&self) -> bool {
        self.bus.borrow().has_expansion_mouse_controller()
    }

    /// Check if a Zapper is connected to the Famicom expansion port.
    pub fn has_expansion_zapper(&self) -> bool {
        self.bus.borrow().has_expansion_zapper()
    }

    #[allow(dead_code)]
    pub fn has_expansion_power_pad(&self) -> bool {
        self.bus.borrow().has_expansion_power_pad()
    }

    /// Check if a Zapper is active on the specified port.
    /// This method is primarily used by the WASM frontend.
    #[allow(dead_code)]
    pub fn is_zapper_active(&self, port: u8) -> bool {
        self.bus.borrow().is_zapper_active(port)
    }

    /// Update the mouse X position for any mouse-emulated controller.
    ///
    /// # Arguments
    /// * `position` - The Arkanoid controller position value (typically 0–255).
    pub fn set_mouse_x_position(&mut self, position: u8) {
        self.bus.borrow_mut().set_mouse_x_position(position);
    }

    /// Update the mouse Y position for any mouse-emulated controller.
    ///
    /// # Arguments
    /// * `position` - The mouse position value (typically 0–255).
    #[allow(dead_code)]
    pub fn set_mouse_y_position(&mut self, position: u8) {
        self.bus.borrow_mut().set_mouse_y_position(position);
    }

    /// Apply relative mouse delta for mouse-emulated controllers.
    pub fn add_mouse_delta(&mut self, dx: i16, dy: i16) {
        self.bus.borrow_mut().add_mouse_delta(dx, dy);
    }

    /// Update the left mouse button status for any mouse-emulated controller.
    ///
    /// # Arguments
    /// * `pressed` - `true` if the Arkanoid controller trigger is pressed, `false` if released.
    pub fn set_mouse_left_button(&mut self, pressed: bool) {
        self.bus.borrow_mut().set_mouse_left_button(pressed);
    }

    /// Update the right mouse button status for any mouse-emulated controller.
    pub fn set_mouse_right_button(&mut self, pressed: bool) {
        self.bus.borrow_mut().set_mouse_right_button(pressed);
    }

    /// Returns true when a Super NES mouse is active on any controller port.
    pub fn has_snes_mouse(&self) -> bool {
        self.bus.borrow().has_snes_mouse()
    }

    /// Generate a trace line for the current CPU state
    ///
    /// Returns a string in the nestest.log format showing the current instruction,
    /// registers, and PPU state. Useful for debugging and comparing against reference logs.
    ///
    /// Format: `PC  OPCODE  INSTRUCTION                 A:XX X:XX Y:XX P:XX SP:XX PPU:SSS,PPP CYC:C`
    #[allow(dead_code)]
    pub fn trace(&mut self, tracing: &Tracing) -> String {
        let nestest = tracing.nestest;
        let cpu_state = self.cpu.state();
        let pc = cpu_state.pc;
        let mut memory = self.bus.borrow_mut();
        // Read the opcode and determine instruction size
        let opcode_byte = memory.read(pc, false);
        let instruction = lookup(opcode_byte);

        // Read operand bytes
        let byte1 = if instruction.bytes() > 1 {
            memory.read(pc.wrapping_add(1), false)
        } else {
            0
        };
        let byte2 = if instruction.bytes() > 2 {
            memory.read(pc.wrapping_add(2), false)
        } else {
            0
        };

        // Build the hex dump string based on instruction bytes
        let hex_dump = match instruction.bytes() {
            1 => format!("{:02X}      ", opcode_byte),
            2 => format!("{:02X} {:02X}   ", opcode_byte, byte1),
            3 => format!("{:02X} {:02X} {:02X}", opcode_byte, byte1, byte2),
            _ => panic!("Invalid instruction byte count"),
        };

        // Build the assembly instruction string
        let asm = match instruction.mode {
            AddrMode::IMP => instruction.mnemonic.to_string(),
            AddrMode::ACC => format!("{} A", instruction.mnemonic),
            AddrMode::IMM => format!("{} #${:02X}", instruction.mnemonic, byte1),
            AddrMode::ZP => {
                let addr = byte1 as u16;
                if nestest {
                    let mut value = memory.read(addr, false);
                    if (0x4000..0x4100).contains(&addr) {
                        value = 0xFF;
                    }
                    format!("{} ${:02X} = {:02X}", instruction.mnemonic, byte1, value)
                } else {
                    format!("{} ${:02X}", instruction.mnemonic, byte1)
                }
            }
            AddrMode::ZPX => {
                let addr = byte1.wrapping_add(cpu_state.x) as u16;
                if nestest {
                    let mut value = memory.read(addr, false);
                    if (0x4000..0x4100).contains(&addr) {
                        value = 0xFF;
                    }
                    format!(
                        "{} ${:02X},X @ {:02X} = {:02X}",
                        instruction.mnemonic, byte1, addr as u8, value
                    )
                } else {
                    format!("{} ${:02X},X", instruction.mnemonic, byte1)
                }
            }
            AddrMode::ZPY => {
                let addr = byte1.wrapping_add(cpu_state.y) as u16;
                if nestest {
                    let mut value = memory.read(addr, false);
                    if (0x4000..0x4100).contains(&addr) {
                        value = 0xFF;
                    }
                    format!(
                        "{} ${:02X},Y @ {:02X} = {:02X}",
                        instruction.mnemonic, byte1, addr as u8, value
                    )
                } else {
                    format!("{} ${:02X},Y", instruction.mnemonic, byte1)
                }
            }
            AddrMode::ABS => {
                let addr = u16::from_le_bytes([byte1, byte2]);
                // JMP and JSR don't show memory value for ABS addressing
                if instruction.mnemonic == Mnemonic::JMP || instruction.mnemonic == Mnemonic::JSR {
                    format!("{} ${:04X}", instruction.mnemonic, addr)
                } else if nestest {
                    let mut value = memory.read(addr, false);
                    if (0x4000..0x4100).contains(&addr) {
                        value = 0xFF;
                    }
                    format!("{} ${:04X} = {:02X}", instruction.mnemonic, addr, value)
                } else {
                    format!("{} ${:04X}", instruction.mnemonic, addr)
                }
            }
            AddrMode::ABSX => {
                let addr = u16::from_le_bytes([byte1, byte2]);
                if nestest {
                    let effective_addr = addr.wrapping_add(cpu_state.x as u16);
                    let value = memory.read(effective_addr, false);
                    format!(
                        "{} ${:04X},X @ {:04X} = {:02X}",
                        instruction.mnemonic, addr, effective_addr, value
                    )
                } else {
                    format!("{} ${:04X},X", instruction.mnemonic, addr)
                }
            }
            AddrMode::ABSY => {
                let addr = u16::from_le_bytes([byte1, byte2]);
                if nestest {
                    let effective_addr = addr.wrapping_add(cpu_state.y as u16);
                    let value = memory.read(effective_addr, false);
                    format!(
                        "{} ${:04X},Y @ {:04X} = {:02X}",
                        instruction.mnemonic, addr, effective_addr, value
                    )
                } else {
                    format!("{} ${:04X},Y", instruction.mnemonic, addr)
                }
            }
            AddrMode::INDX => {
                if nestest {
                    let zp_addr = byte1.wrapping_add(cpu_state.x);
                    let addr_lo = memory.read(zp_addr as u16, false);
                    let addr_hi = memory.read(zp_addr.wrapping_add(1) as u16, false);
                    let addr = u16::from_le_bytes([addr_lo, addr_hi]);
                    let value = memory.read(addr, false);
                    format!(
                        "{} (${:02X},X) @ {:02X} = {:04X} = {:02X}",
                        instruction.mnemonic, byte1, zp_addr, addr, value
                    )
                } else {
                    format!("{} (${:02X},X)", instruction.mnemonic, byte1)
                }
            }
            AddrMode::INDY => {
                if nestest {
                    let addr_lo = memory.read(byte1 as u16, false);
                    let addr_hi = memory.read(byte1.wrapping_add(1) as u16, false);
                    let base_addr = u16::from_le_bytes([addr_lo, addr_hi]);
                    let effective_addr = base_addr.wrapping_add(cpu_state.y as u16);
                    let value = memory.read(effective_addr, false);
                    format!(
                        "{} (${:02X}),Y = {:04X} @ {:04X} = {:02X}",
                        instruction.mnemonic, byte1, base_addr, effective_addr, value
                    )
                } else {
                    format!("{} (${:02X}),Y", instruction.mnemonic, byte1)
                }
            }
            AddrMode::IND => {
                if nestest {
                    let ptr_addr = u16::from_le_bytes([byte1, byte2]);
                    let addr_lo = memory.read(ptr_addr, false);
                    // 6502 bug: if ptr_addr is at page boundary (e.g., $02FF),
                    // high byte wraps within same page instead of crossing to next page
                    let hi_addr = if ptr_addr & 0xFF == 0xFF {
                        ptr_addr & 0xFF00 // Wrap to start of same page
                    } else {
                        ptr_addr.wrapping_add(1)
                    };
                    let addr_hi = memory.read(hi_addr, false);
                    let target_addr = u16::from_le_bytes([addr_lo, addr_hi]);
                    format!(
                        "{} (${:04X}) = {:04X}",
                        instruction.mnemonic, ptr_addr, target_addr
                    )
                } else {
                    let ptr_addr = u16::from_le_bytes([byte1, byte2]);
                    format!("{} (${:04X})", instruction.mnemonic, ptr_addr)
                }
            }
            AddrMode::REL => {
                let offset = byte1 as i8;
                let target = pc.wrapping_add(2).wrapping_add(offset as u16);
                format!("{} ${:04X}", instruction.mnemonic, target)
            }
            AddrMode::ABSXW => {
                // Same as ABSX but for write/RMW instructions
                let addr = u16::from_le_bytes([byte1, byte2]);
                if nestest {
                    let effective_addr = addr.wrapping_add(cpu_state.x as u16);
                    let value = memory.read(effective_addr, false);
                    format!(
                        "{} ${:04X},X @ {:04X} = {:02X}",
                        instruction.mnemonic, addr, effective_addr, value
                    )
                } else {
                    format!("{} ${:04X},X", instruction.mnemonic, addr)
                }
            }
            AddrMode::ABSYW => {
                // Same as ABSY but for write/RMW instructions
                let addr = u16::from_le_bytes([byte1, byte2]);
                if nestest {
                    let effective_addr = addr.wrapping_add(cpu_state.y as u16);
                    let value = memory.read(effective_addr, false);
                    format!(
                        "{} ${:04X},Y @ {:04X} = {:02X}",
                        instruction.mnemonic, addr, effective_addr, value
                    )
                } else {
                    format!("{} ${:04X},Y", instruction.mnemonic, addr)
                }
            }
            AddrMode::INDYW => {
                // Same as INDY but for write/RMW instructions
                if nestest {
                    let addr_lo = memory.read(byte1 as u16, false);
                    let addr_hi = memory.read(byte1.wrapping_add(1) as u16, false);
                    let base_addr = u16::from_le_bytes([addr_lo, addr_hi]);
                    let effective_addr = base_addr.wrapping_add(cpu_state.y as u16);
                    let value = memory.read(effective_addr, false);
                    format!(
                        "{} (${:02X}),Y = {:04X} @ {:04X} = {:02X}",
                        instruction.mnemonic, byte1, base_addr, effective_addr, value
                    )
                } else {
                    format!("{} (${:02X}),Y", instruction.mnemonic, byte1)
                }
            }
        };

        if nestest {
            let total_cycles = self.cpu.get_total_cycles();
            let ppu = self.ppu.borrow();
            let mut scanline = ppu.scanline();
            let mut pixel = ppu.pixel();
            // Compensate for the +1 PPU tick
            if pixel == 0 {
                scanline -= 1;
                pixel = 340;
            } else {
                pixel -= 1;
            }
            // Adjust spacing for 4-character mnemonics (starts one character earlier)
            let mnem_str = instruction.mnemonic.to_string();
            let (pad_before, width) = if mnem_str.len() == 4 {
                (" ", 32)
            } else {
                ("  ", 31)
            };
            format!(
                "{:04X}  {}{}{:<width$} A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X} PPU:{:3},{:3} CYC:{}",
                pc,
                hex_dump,
                pad_before,
                asm,
                cpu_state.a,
                cpu_state.x,
                cpu_state.y,
                cpu_state.p,
                cpu_state.sp,
                scanline,
                pixel,
                total_cycles,
                width = width
            )
        } else {
            let total_cycles = self.cpu.get_total_cycles();
            let ppu = self.ppu.borrow();
            let scanline = ppu.scanline();
            let pixel = ppu.pixel();
            let apu_ticks = self.apu.borrow().debug_frame_counter_cycle();
            let apu_cycles = self.apu.borrow().apu_cycle();
            format!(
                "{:04X} {} {:10} A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X} PPU:{:3},{:3} CPU:{:8} PPU:{}/{}",
                pc,
                hex_dump,
                asm,
                cpu_state.a,
                cpu_state.x,
                cpu_state.y,
                cpu_state.p,
                cpu_state.sp,
                scanline,
                pixel,
                total_cycles,
                apu_ticks,
                apu_cycles,
            )
        }
    }

    /// Get base nametable address from PPUCTRL (for testing)
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn base_nametable_addr(&self) -> u16 {
        self.ppu.borrow().base_nametable_addr()
    }

    /// Read nametable text for automated test verification
    ///
    /// Reads tile indices from the nametable and converts them to ASCII text.
    /// This is useful for Blargg tests that output results to the screen instead of $6000.
    ///
    /// # Arguments
    /// * `nametable_addr` - Starting address in nametable (e.g., 0x2081)
    /// * `length` - Number of tiles to read
    ///
    /// # Returns
    /// String containing the decoded text
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn read_nametable_text(&self, nametable_addr: u16, length: usize) -> String {
        let ppu = self.ppu.borrow();
        let mut text = String::new();

        for i in 0..length {
            let addr = nametable_addr.wrapping_add(i as u16);
            let tile_index = ppu.read_nametable_for_debug(addr);

            // Decode tile index to character
            // Blargg's branch timing tests store ASCII values directly as tiles
            let ch = if (0x20..=0x7E).contains(&tile_index) {
                tile_index as char
            } else if tile_index == 0x00 {
                ' ' // Treat 0x00 as space
            } else {
                // Tile indices 0x10-0x1F map to ASCII characters [0..9,A..F]
                // (used by Blargg-style mapper verification ROMs)
                if (0x10..=0x19).contains(&tile_index) {
                    (tile_index + 0x20) as char
                } else if (0x1A..=0x1F).contains(&tile_index) {
                    (tile_index + 0x27) as char
                // Tile indices 0xE0-0xEF map to hex digits [0..9,A..F]
                // (used by vrc6test ROM: donumber adds $E0 to each nibble)
                } else if (0xE0..=0xE9).contains(&tile_index) {
                    (tile_index - 0xE0 + b'0') as char
                } else if (0xEA..=0xEF).contains(&tile_index) {
                    (tile_index - 0xEA + b'A') as char
                } else {
                    '?' // Unknown character
                }
            };
            text.push(ch);
        }

        text
    }

    /// Read raw nametable tile indices as a byte vector.
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn read_nametable_raw(&self, nametable_addr: u16, length: usize) -> Vec<u8> {
        let ppu = self.ppu.borrow();
        (0..length)
            .map(|i| {
                let addr = nametable_addr.wrapping_add(i as u16);
                ppu.read_nametable_for_debug(addr)
            })
            .collect()
    }

    /// Create a complete save-state snapshot of the emulator.
    ///
    /// This captures the full state of CPU, PPU, APU, RAM, and mapper,
    /// allowing the emulator to be restored to this exact state later.
    pub fn save_state(&self) -> SaveState {
        SaveState::new(
            self.cpu.capture_state(),
            self.ppu.borrow().capture_state(),
            self.apu.borrow().capture_state(),
            self.bus.borrow().capture_state(),
            self.bus.borrow().ram_snapshot(),
            self.bus.borrow().capture_mapper_state(),
        )
    }

    /// Restore emulator state from a save-state.
    ///
    /// This restores the full state of CPU, PPU, APU, RAM, and mapper
    /// from a previously saved state.
    ///
    /// # Errors
    ///
    /// Returns an error if the save-state version is incompatible or if
    /// the mapper number doesn't match the currently loaded cartridge.
    pub fn load_state(&mut self, state: &SaveState) -> Result<(), NesSaveStateError> {
        // Check version compatibility
        crate::platform::save_state::check_version(state.version, &[SAVESTATE_VERSION])?;

        // Check mapper compatibility
        let current_mapper = self.bus.borrow().capture_mapper_state().mapper_number;
        if state.mapper.mapper_number != current_mapper {
            return Err(NesSaveStateError::MapperMismatch {
                expected: current_mapper,
                found: state.mapper.mapper_number,
            });
        }

        // Restore all components
        self.cpu.restore_state(&state.cpu);
        self.ppu.borrow_mut().restore_state(&state.ppu);
        self.apu.borrow_mut().restore_state(&state.apu);
        self.bus.borrow_mut().restore_state(&state.bus);
        self.bus.borrow_mut().restore_ram(&state.ram);
        self.bus.borrow_mut().restore_mapper_state(&state.mapper);

        // Clear derived state
        self.fractional_ppu_cycles = 0.0;
        self.ready_to_render = false;
        self.recent_cpu_trace.clear();

        Ok(())
    }

    /// Load a ROM from raw bytes, creating and inserting the cartridge.
    ///
    /// This is a convenience method that combines [`Cartridge::load_from_file`]
    /// and [`insert_cartridge`](Self::insert_cartridge).
    pub fn load_rom(&mut self, bytes: &[u8], name: &str) -> Result<(), String> {
        let cart = Cartridge::load_from_file(bytes, name, Some(&self.rom_db))
            .map_err(|e| e.to_string())?;
        self.insert_cartridge(cart);
        Ok(())
    }

    /// Serialize emulator state to bytes (JSON).
    pub fn save_state_bytes(&self) -> Result<Vec<u8>, String> {
        self.save_state()
            .to_bytes()
            .map_err(|e| format!("save state serialization failed: {e}"))
    }

    /// Restore emulator state from previously serialized bytes (JSON).
    pub fn load_state_bytes(&mut self, data: &[u8]) -> Result<(), String> {
        let state = SaveState::from_bytes(data)
            .map_err(|e| format!("save state deserialization failed: {e}"))?;
        self.load_state(&state).map_err(|e| e.to_string())
    }

    /// Set a button state using a numeric button ID.
    ///
    /// Returns `true` if the button ID was valid, `false` otherwise.
    pub fn set_button_by_id(&mut self, port: u8, button_id: u8, pressed: bool) -> bool {
        if let Some(button) = crate::nes::input::button_from_id(button_id) {
            self.set_button(port, button, pressed);
            true
        } else {
            false
        }
    }
}

impl Emulator for Nes {
    fn system_type(&self) -> SystemType {
        SystemType::Nes
    }

    fn allowed_shaders(&self) -> &'static [&'static str] {
        &["none", "crt", "smooth", "ntsc", "pal"]
    }

    fn load_rom(&mut self, bytes: &[u8], name: &str) -> Result<(), String> {
        Nes::load_rom(self, bytes, name)
    }

    fn run_tick(&mut self) -> u8 {
        self.run_cpu_tick()
    }

    fn is_ready_to_render(&self) -> bool {
        Nes::is_ready_to_render(self)
    }

    fn clear_ready_to_render(&mut self) {
        Nes::clear_ready_to_render(self)
    }

    fn screen_width(&self) -> u32 {
        Nes::SCREEN_WIDTH
    }

    fn screen_height(&self) -> u32 {
        Nes::SCREEN_HEIGHT
    }

    fn screen_snapshot(&self) -> Vec<u8> {
        self.get_screen_buffer().snapshot()
    }

    fn cropped_screen_snapshot(&self, h_overscan: u32, v_overscan: u32) -> Vec<u8> {
        self.get_screen_buffer()
            .cropped_snapshot(h_overscan, v_overscan)
    }

    fn screen_crc32(&self) -> u32 {
        self.get_screen_buffer().crc32()
    }

    fn sample_ready(&self) -> bool {
        Nes::sample_ready(self)
    }

    fn get_sample(&mut self) -> Option<f32> {
        Nes::get_sample(self)
    }

    fn set_audio_sample_rate(&mut self, rate: f32) {
        Nes::set_audio_sample_rate(self, rate)
    }

    fn set_button(&mut self, port: u8, button_id: u8, pressed: bool) {
        if !self.set_button_by_id(port, button_id, pressed) {
            #[cfg(debug_assertions)]
            eprintln!("warning: invalid NES button_id: {button_id}");
        }
    }

    fn set_joypad_button_states(&mut self, port: u8, state: u8) {
        Nes::set_joypad_button_states(self, port, state)
    }

    fn get_joypad_button_states(&self, port: u8) -> u8 {
        Nes::get_joypad_button_states(self, port)
    }

    fn save_state_bytes(&self) -> Result<Vec<u8>, String> {
        Nes::save_state_bytes(self)
    }

    fn load_state_bytes(&mut self, data: &[u8]) -> Result<(), String> {
        Nes::load_state_bytes(self, data)
    }

    fn reset(&mut self, soft_reset: bool) {
        Nes::reset(self, soft_reset)
    }

    fn save_ram(&self) -> Result<(), String> {
        Nes::save_ram(self).map_err(|e| e.to_string())
    }

    fn app_context(&self) -> &SharedAppContext {
        Nes::app_context(self)
    }

    fn target_frame_duration(&self) -> std::time::Duration {
        let hz = self
            .app_context()
            .borrow()
            .config()
            .nes
            .hardware_model
            .timing_mode()
            .frame_rate_hz();
        std::time::Duration::from_secs_f64(1.0 / hz)
    }
}

fn format_compact_trace_instruction(
    meta: &crate::nes::cpu::OpCode,
    addr: u16,
    bytes: &[u8],
) -> String {
    let operand = match meta.mode {
        AddrMode::IMP => String::new(),
        AddrMode::ACC => "A".to_string(),
        AddrMode::IMM => format!("#${:02X}", bytes.get(1).copied().unwrap_or(0)),
        AddrMode::ZP => format!("${:02X}", bytes.get(1).copied().unwrap_or(0)),
        AddrMode::ZPX => format!("${:02X},X", bytes.get(1).copied().unwrap_or(0)),
        AddrMode::ZPY => format!("${:02X},Y", bytes.get(1).copied().unwrap_or(0)),
        AddrMode::INDX => format!("(${:02X},X)", bytes.get(1).copied().unwrap_or(0)),
        AddrMode::INDY | AddrMode::INDYW => {
            format!("(${:02X}),Y", bytes.get(1).copied().unwrap_or(0))
        }
        AddrMode::REL => {
            let off = bytes.get(1).copied().unwrap_or(0) as i8;
            let next = addr.wrapping_add(2);
            let target = next.wrapping_add(off as i16 as u16);
            format!("${:04X}", target)
        }
        AddrMode::ABS => {
            let lo = bytes.get(1).copied().unwrap_or(0);
            let hi = bytes.get(2).copied().unwrap_or(0);
            format!("${:04X}", u16::from_le_bytes([lo, hi]))
        }
        AddrMode::ABSX | AddrMode::ABSXW => {
            let lo = bytes.get(1).copied().unwrap_or(0);
            let hi = bytes.get(2).copied().unwrap_or(0);
            format!("${:04X},X", u16::from_le_bytes([lo, hi]))
        }
        AddrMode::ABSY | AddrMode::ABSYW => {
            let lo = bytes.get(1).copied().unwrap_or(0);
            let hi = bytes.get(2).copied().unwrap_or(0);
            format!("${:04X},Y", u16::from_le_bytes([lo, hi]))
        }
        AddrMode::IND => {
            let lo = bytes.get(1).copied().unwrap_or(0);
            let hi = bytes.get(2).copied().unwrap_or(0);
            format!("(${:04X})", u16::from_le_bytes([lo, hi]))
        }
    };

    if operand.is_empty() {
        meta.mnemonic.to_string()
    } else {
        format!("{} {}", meta.mnemonic, operand)
    }
}

mod savestate_error {
    use crate::platform::save_state::SaveStateError;

    /// Errors that can occur when loading an NES save-state.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum NesSaveStateError {
        /// A shared save-state error (version, (de)serialization, or restore).
        Common(SaveStateError),
        /// The mapper number doesn't match the currently loaded cartridge.
        MapperMismatch { expected: u16, found: u16 },
    }

    impl std::fmt::Display for NesSaveStateError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                NesSaveStateError::Common(err) => write!(f, "{err}"),
                NesSaveStateError::MapperMismatch { expected, found } => {
                    write!(
                        f,
                        "Mapper mismatch: cartridge uses mapper {}, but save-state is for mapper {}",
                        expected, found
                    )
                }
            }
        }
    }

    impl std::error::Error for NesSaveStateError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            match self {
                NesSaveStateError::Common(err) => Some(err),
                NesSaveStateError::MapperMismatch { .. } => None,
            }
        }
    }

    impl From<SaveStateError> for NesSaveStateError {
        fn from(err: SaveStateError) -> Self {
            NesSaveStateError::Common(err)
        }
    }
}

pub use savestate_error::NesSaveStateError;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::config::ParseResult;

    fn parse_cli_config(mut args: Vec<String>) -> Config {
        use tempfile::NamedTempFile;

        let file = NamedTempFile::new().unwrap();

        args.push("--config".to_string());
        args.push(file.path().to_string_lossy().to_string());

        match Config::new(&args).unwrap() {
            ParseResult::Config(config) => *config,
            ParseResult::Help => panic!("Expected Config, got Help"),
            ParseResult::Version => panic!("Expected Config, got Version"),
        }
    }

    fn load_test_cartridge(rom_data: &[u8]) -> Cartridge {
        Cartridge::load_from_file(rom_data, "nes-test-rom.nes", None)
            .expect("Failed to create cartridge")
    }

    #[test]
    fn test_ntsc_ppu_cycles_per_cpu_cycle() {
        let ntsc = TimingMode::Ntsc;
        // NTSC: 3 PPU cycles per CPU cycle
        assert_eq!(ntsc.ppu_cycles_per_cpu_cycle(), 3.0);
    }

    #[test]
    fn test_pal_ppu_cycles_per_cpu_cycle() {
        let pal = TimingMode::Pal;
        // PAL: 3.2 PPU cycles per CPU cycle
        assert_eq!(pal.ppu_cycles_per_cpu_cycle(), 3.2);
    }

    #[test]
    fn test_ntsc_scanlines_per_frame() {
        let ntsc = TimingMode::Ntsc;
        // NTSC: 262 scanlines per frame
        assert_eq!(ntsc.scanlines_per_frame(), 262);
    }

    #[test]
    fn test_pal_scanlines_per_frame() {
        let pal = TimingMode::Pal;
        // PAL: 312 scanlines per frame
        assert_eq!(pal.scanlines_per_frame(), 312);
    }

    #[test]
    fn test_ntsc_frame_rate_hz() {
        let ntsc = TimingMode::Ntsc;
        let fps = ntsc.frame_rate_hz();
        assert!(
            (fps - 60.0988).abs() < 0.01,
            "NTSC frame rate should be ~60.0988 Hz, got {fps}"
        );
    }

    #[test]
    fn test_pal_frame_rate_hz() {
        let pal = TimingMode::Pal;
        let fps = pal.frame_rate_hz();
        assert!(
            (fps - 50.00698).abs() < 0.01,
            "PAL frame rate should be ~50.00698 Hz, got {fps}"
        );
    }

    #[test]
    fn test_nes_new_applies_cli_disabled_apu_channels() {
        let config = parse_cli_config(vec![
            "neser".to_string(),
            "--disable-nes-pulse1".to_string(),
            "--disable-nes-pulse2".to_string(),
            "--disable-nes-triangle".to_string(),
            "--disable-nes-noise".to_string(),
            "--disable-nes-dmc".to_string(),
        ]);

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            config,
        ));
        nes.insert_cartridge(load_test_cartridge(&create_minimal_rom()));

        assert_all_apu_channels_disabled(&nes);
        nes.reset(true);
        assert_all_apu_channels_disabled(&nes);
        nes.reset(false);
        assert_all_apu_channels_disabled(&nes);
    }

    fn assert_all_apu_channels_disabled(nes: &Nes) {
        let apu_state = nes.apu.borrow().capture_state();
        assert!(!apu_state.pulse1_enabled);
        assert!(!apu_state.pulse2_enabled);
        assert!(!apu_state.triangle_enabled);
        assert!(!apu_state.noise_enabled);
        assert!(!apu_state.dmc_enabled);
    }

    #[test]
    fn test_ntsc_ppu_runs_3x_cpu_cycles() {
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        // Write NOP to RAM and set PC directly (skip reset to avoid ROM requirement)
        nes.bus.borrow_mut().write(0x0000, 0xEA, false); // NOP in RAM
        nes.cpu.set_pc(0x0000); // Set PC to RAM address

        // NOP takes 2 CPU cycles, so PPU should run 6 cycles (3x ratio for NTSC)
        // Plus 1 cycle for initial PPU offset (sprite 0 hit timing correction)
        nes.run_cpu_tick();
        assert_eq!(nes.ppu.borrow().total_cycles(), 7);
    }

    #[test]
    fn test_pal_ppu_runs_3_2x_cpu_cycles() {
        let config = Config {
            nes: NesConfig {
                hardware_model: crate::nes::console::HardwareModel::NesPal,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            config,
        ));
        // Write NOP to RAM and set PC directly
        nes.bus.borrow_mut().write(0x0000, 0xEA, false); // NOP in RAM
        nes.cpu.set_pc(0x0000);

        // NOP takes 2 CPU cycles, PAL ratio is 3.2, so 2 * 3.2 = 6.4
        // Plus 1 cycle for initial PPU offset
        // Should accumulate fractional part
        nes.run_cpu_tick();
        assert_eq!(nes.ppu.borrow().total_cycles(), 7);
    }

    #[test]
    fn test_pal_ppu_accumulates_fractional_cycles() {
        let config = Config {
            nes: NesConfig {
                hardware_model: crate::nes::console::HardwareModel::NesPal,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            config,
        ));
        // Write NOP instructions to RAM
        for i in 0..10 {
            nes.bus.borrow_mut().write(i, 0xEA, false); // NOP
        }
        nes.cpu.set_pc(0x0000);

        // Run 5 NOPs: 5 instructions * 2 cycles = 10 CPU cycles
        // 10 * 3.2 = 32 PPU cycles, plus 1 cycle initial offset
        for _ in 0..5 {
            nes.run_cpu_tick();
        }
        assert_eq!(nes.ppu.borrow().total_cycles(), 33);
    }

    #[test]
    fn test_ntsc_ppu_accumulates_over_multiple_instructions() {
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        // Write NOP instructions to RAM
        for i in 0..3 {
            nes.bus.borrow_mut().write(i, 0xEA, false); // NOP (2 cycles each)
        }
        nes.cpu.set_pc(0x0000);

        // 3 NOPs = 6 CPU cycles, 18 PPU cycles (6 * 3), plus 1 cycle initial offset
        nes.run_cpu_tick();
        nes.run_cpu_tick();
        nes.run_cpu_tick();
        assert_eq!(nes.ppu.borrow().total_cycles(), 19);
    }

    #[test]
    fn test_ppu_cycles_reset_on_nes_reset() {
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.bus.borrow_mut().write(0x0000, 0xEA, false); // NOP
        nes.cpu.set_pc(0x0000);

        nes.run_cpu_tick();
        assert_eq!(nes.ppu.borrow().total_cycles(), 7); // 6 + 1 offset

        // Reset just the PPU to test the counter is cleared
        nes.ppu
            .borrow_mut()
            .reset(false, crate::nes::console::RamInitMode::Zero);
        assert_eq!(nes.ppu.borrow().total_cycles(), 0);
    }

    #[test]
    fn test_nes_reset_reestablishes_1_cycle_ppu_offset() {
        let rom_data = create_minimal_rom();
        let cartridge = load_test_cartridge(&rom_data);

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cartridge);

        // After a power-on reset, CPU reset consumes 7 cycles (21 PPU cycles at 3x).
        // The emulator maintains a 1-cycle PPU lead for timing quirks (sprite-0 hit, etc).
        nes.reset(false);
        assert_eq!(nes.ppu.borrow().total_cycles(), 22);
    }

    #[test]
    fn test_nes_color_to_rgb() {
        // Test a few key colors from the NES palette
        assert_eq!(Nes::lookup_system_palette(0x00), (0x54, 0x54, 0x54)); // Gray
        assert_eq!(Nes::lookup_system_palette(0x01), (0x00, 0x1E, 0x74)); // Blue
        assert_eq!(Nes::lookup_system_palette(0x16), (0x98, 0x22, 0x20)); // Red
        assert_eq!(Nes::lookup_system_palette(0x2A), (0x4C, 0xD0, 0x20)); // Green
        assert_eq!(Nes::lookup_system_palette(0x30), (0xEC, 0xEE, 0xEC)); // White
        assert_eq!(Nes::lookup_system_palette(0x0D), (0x00, 0x00, 0x00)); // Black

        // Test that values above 0x3F are masked
        assert_eq!(Nes::lookup_system_palette(0x40), (0x54, 0x54, 0x54)); // Same as 0x00
        assert_eq!(Nes::lookup_system_palette(0xFF), (0x00, 0x00, 0x00)); // Same as 0x3F
    }

    #[test]
    fn test_nes_provides_access_to_ppu_screen_buffer() {
        let nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        // Should be able to access the PPU's screen buffer
        let screen_buffer = nes.get_screen_buffer();

        // Verify it has the correct dimensions
        assert_eq!(screen_buffer.width(), 256);
        assert_eq!(screen_buffer.height(), 240);
    }

    /// Helper function to create a minimal iNES ROM with a simple infinite loop
    /// This ROM just executes JMP $8000 (4C 00 80) forever
    fn create_minimal_rom() -> Vec<u8> {
        let mut rom = Vec::new();

        // iNES header
        rom.extend_from_slice(b"NES\x1A"); // Magic bytes
        rom.push(1); // 1x 16 KB PRG ROM
        rom.push(0); // 0x 8 KB CHR ROM (no CHR)
        rom.push(0); // Flags 6 (horizontal mirroring, no other features)
        rom.push(0); // Flags 7
        rom.extend_from_slice(&[0; 8]); // Padding to complete 16-byte header

        // PRG ROM (16 KB = 16384 bytes)
        let mut prg_rom = vec![0; 16384];

        // Reset vector at $FFFC-$FFFD points to $8000
        prg_rom[0x3FFC] = 0x00; // Low byte
        prg_rom[0x3FFD] = 0x80; // High byte

        // Code at $8000: JMP $8000 (infinite loop)
        prg_rom[0] = 0x4C; // JMP absolute
        prg_rom[1] = 0x00; // Low byte of address
        prg_rom[2] = 0x80; // High byte of address

        rom.extend_from_slice(&prg_rom);
        rom
    }

    fn create_minimal_playchoice10_rom() -> Vec<u8> {
        let mut rom = Vec::new();

        // iNES header with PlayChoice-10 bit set in flags7.
        rom.extend_from_slice(b"NES\x1A");
        rom.push(1); // 1x 16 KB PRG ROM
        rom.push(0); // 0x 8 KB CHR ROM (no CHR)
        rom.push(0); // Flags 6 (horizontal mirroring, no other features)
        rom.push(0x02); // Flags 7 (PlayChoice-10)
        rom.extend_from_slice(&[0; 8]); // Padding to complete 16-byte header

        // PRG ROM (16 KB = 16384 bytes)
        let mut prg_rom = vec![0; 16384];

        // Reset vector at $FFFC-$FFFD points to $8000
        prg_rom[0x3FFC] = 0x00;
        prg_rom[0x3FFD] = 0x80;

        // Code at $8000: JMP $8000 (infinite loop)
        prg_rom[0] = 0x4C;
        prg_rom[1] = 0x00;
        prg_rom[2] = 0x80;

        rom.extend_from_slice(&prg_rom);
        rom
    }

    #[test]
    fn test_insert_cartridge_auto_detects_playchoice10_expansion() {
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        let rom_data = create_minimal_playchoice10_rom();
        let cartridge = load_test_cartridge(&rom_data);
        nes.insert_cartridge(cartridge);

        assert_eq!(
            nes.app_context.borrow().config().nes.expansion_port,
            crate::nes::console::ExpansionPort::Playchoice10,
            "PlayChoice ROM should auto-select PlayChoice expansion mode"
        );
    }

    #[test]
    fn test_insert_cartridge_hardware_summary_reports_playchoice10() {
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        let rom_data = create_minimal_playchoice10_rom();
        let cartridge = load_test_cartridge(&rom_data);
        nes.insert_cartridge(cartridge);

        let summary = nes.app_context.borrow().config().hardware_summary();
        assert!(
            summary.contains("PlayChoice-10"),
            "hardware summary should mention PlayChoice-10 when PC10 ROM is loaded: {summary}"
        );
    }

    #[test]
    fn test_nes_reset_resets_apu_before_cpu_reset_ticks() {
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let rom_data = create_minimal_rom();
        let cartridge = load_test_cartridge(&rom_data);
        nes.insert_cartridge(cartridge);

        // Ensure cpu_cycle passed into APU reset is non-zero (so we don't take the power-on
        // early-return path inside APU reset).
        // Use odd to get the 3-cycle write delay.
        nes.cpu.set_total_cycles(1);

        nes.reset(true);

        // APU reset queues a delayed $4017 write (3 cycles on odd CPU cycle), and the write
        // takes effect during the delay, resetting the frame counter. With our current
        // implementation, this leaves the frame counter at cycle 1 when the APU reset returns.
        // CPU reset then advances the master clock by 7 CPU cycles, which should tick the APU
        // 7 more times IF the APU was reset before CPU reset.
        let expected_cycle_counter = 1 + 7;
        assert_eq!(
            nes.apu.borrow().frame_counter().get_cycle_counter(),
            expected_cycle_counter,
            "APU should have advanced during CPU reset cycles"
        );
    }

    #[test]
    fn test_nes_reset_soft_reset_rewrites_last_4017_value() {
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let rom_data = create_minimal_rom();
        let cartridge = load_test_cartridge(&rom_data);
        nes.insert_cartridge(cartridge);

        // Set a non-default $4017 value.
        nes.apu.borrow_mut().write_frame_counter(0x80);

        // Soft reset should rewrite last $4017 value, keeping 5-step mode.
        nes.reset(true);
        assert!(
            nes.apu.borrow().frame_counter().get_mode(),
            "Soft reset should keep the last-written $4017 mode"
        );

        // Power-on reset should behave as if $4017=$00.
        nes.reset(false);
        assert!(
            !nes.apu.borrow().frame_counter().get_mode(),
            "Power-on reset should behave as if $4017 was written with $00"
        );
    }

    #[test]
    fn test_oam_dma_takes_513_cycles_on_even_cpu_cycle() {
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let rom_data = create_minimal_rom();
        let cartridge = load_test_cartridge(&rom_data);
        nes.insert_cartridge(cartridge);
        nes.cpu.reset(false);

        // Set CPU to an even cycle (8)
        nes.cpu.set_total_cycles(8);

        let cycles_before = nes.cpu.get_total_cycles();
        assert_eq!(cycles_before % 2, 0, "Should start on even cycle");

        // Trigger OAM DMA by writing to $4014
        nes.bus.borrow_mut().write(0x4014, 0x02, false);

        // Run one CPU tick which should process the DMA
        nes.run_cpu_tick();

        // On even alignment, DMA begins on the next cycle (odd) and takes 514 CPU cycles
        let cycles_after = nes.cpu.get_total_cycles();
        assert_eq!(
            cycles_after - cycles_before,
            514,
            "DMA should take 514 cycles on even alignment"
        );
    }

    #[test]
    fn test_oam_dma_takes_514_cycles_on_odd_cpu_cycle() {
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let rom_data = create_minimal_rom();
        let cartridge = load_test_cartridge(&rom_data);
        nes.insert_cartridge(cartridge);
        nes.cpu.reset(false);

        // Set CPU to an odd cycle (7)
        nes.cpu.set_total_cycles(7);

        let cycles_before = nes.cpu.get_total_cycles();
        assert_eq!(cycles_before % 2, 1, "Should start on odd cycle");

        // Trigger OAM DMA by writing to $4014
        nes.bus.borrow_mut().write(0x4014, 0x02, false);

        // Run one CPU tick which should process the DMA
        nes.run_cpu_tick();

        // On odd alignment, DMA begins on the next cycle (even) and takes 513 CPU cycles
        let cycles_after = nes.cpu.get_total_cycles();
        assert_eq!(
            cycles_after - cycles_before,
            513,
            "DMA should take 513 cycles on odd alignment"
        );
    }

    #[test]
    fn test_oam_dma_transfers_256_bytes() {
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let rom_data = create_minimal_rom();
        let cartridge = load_test_cartridge(&rom_data);
        nes.insert_cartridge(cartridge);
        nes.cpu.reset(false);

        // Set up test data in RAM at page $02 ($0200-$02FF)
        for i in 0..256u16 {
            nes.bus
                .borrow_mut()
                .write(0x0200 + i, (i & 0xFF) as u8, false);
        }

        // Trigger OAM DMA from page $02
        nes.bus.borrow_mut().write(0x4014, 0x02, false);
        nes.run_cpu_tick();

        // Verify all 256 bytes were copied to OAM by reading through $2004
        for i in 0..256 {
            // Set OAM address via $2003
            nes.bus.borrow_mut().write(0x2003, i as u8, false);
            // Read OAM data via $2004
            let oam_byte = nes.bus.borrow_mut().read(0x2004, false);
            let expected = if (i & 0x03) == 2 {
                // Attribute byte: mask bits 2-4
                ((i & 0xFF) as u8) & 0xE3
            } else {
                (i & 0xFF) as u8
            };
            assert_eq!(
                oam_byte, expected,
                "OAM byte {} should match source data (with attribute masking)",
                i
            );
        }
    }

    #[test]
    fn test_oam_dma_uses_correct_source_page() {
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let rom_data = create_minimal_rom();
        let cartridge = load_test_cartridge(&rom_data);
        nes.insert_cartridge(cartridge);
        nes.cpu.reset(false);

        // Set up distinct data in different pages
        // Page $03: $0300-$03FF
        for i in 0..256u16 {
            nes.bus.borrow_mut().write(0x0300 + i, 0xAA, false); // Marker value
        }

        // Trigger OAM DMA from page $03
        nes.bus.borrow_mut().write(0x4014, 0x03, false);
        nes.run_cpu_tick();

        // Verify bytes came from page $03 by reading through $2004
        for i in 0..256 {
            // Set OAM address via $2003
            nes.bus.borrow_mut().write(0x2003, i as u8, false);
            // Read OAM data via $2004
            let oam_byte = nes.bus.borrow_mut().read(0x2004, false);
            let expected = if (i & 0x03) == 2 {
                // Attribute byte: 0xAA with masking = 0xAA & 0xE3 = 0xA2
                0xA2
            } else {
                0xAA
            };
            assert_eq!(
                oam_byte, expected,
                "OAM byte {} should be from page $03 (with attribute masking)",
                i
            );
        }
    }

    #[test]
    fn test_ppu_advances_during_oam_dma() {
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let rom_data = create_minimal_rom();
        let cartridge = load_test_cartridge(&rom_data);
        nes.insert_cartridge(cartridge);
        nes.cpu.reset(false);

        // Set CPU to an even cycle (8)
        nes.cpu.set_total_cycles(8);

        // Get initial PPU state
        let initial_ppu_cycles = nes.ppu.borrow().total_cycles();

        // Trigger OAM DMA
        nes.bus.borrow_mut().write(0x4014, 0x02, false);
        nes.run_cpu_tick();

        // With CPU-owned DMA ticking using MasterClock, the PPU advances by at least
        // 514 CPU cycles * 3 PPU cycles per CPU cycle (DMA starts on the next cycle).
        //
        // Depending on where the CPU was in its internal master-clock phase when DMA
        // begins, up to 2 additional PPU cycles may be flushed as part of the DMA tick.
        let expected_ppu_cycles = initial_ppu_cycles + (514 * 3);
        let actual_ppu_cycles = nes.ppu.borrow().total_cycles();

        assert!(
            actual_ppu_cycles >= expected_ppu_cycles
                && actual_ppu_cycles <= expected_ppu_cycles + 2,
            "PPU should advance by 514*3 cycles during DMA on even alignment (got {}, expected {}..={})",
            actual_ppu_cycles,
            expected_ppu_cycles,
            expected_ppu_cycles + 2
        );
    }

    #[test]
    fn test_ntsc_refresh_rate_calculation() {
        // NTSC CPU runs at approximately 1.789773 MHz
        // Even frame: 89342 PPU cycles / 3 = 29780.67 CPU cycles
        // Odd frame:  89341 PPU cycles / 3 = 29780.33 CPU cycles
        // Average: ~29780.5 CPU cycles per frame
        // Refresh rate: 1789773 / 29780.5 ≈ 60.10 Hz

        let tv_system = TimingMode::Ntsc;
        let even_frame_ppu_cycles = 262 * 341; // 89342
        let odd_frame_ppu_cycles = 262 * 341 - 1; // 89341 (with odd frame skip)
        let avg_ppu_cycles = (even_frame_ppu_cycles + odd_frame_ppu_cycles) as f64 / 2.0;
        let avg_cpu_cycles = avg_ppu_cycles / tv_system.ppu_cycles_per_cpu_cycle();

        assert_eq!(even_frame_ppu_cycles, 89342);
        assert_eq!(odd_frame_ppu_cycles, 89341);
        assert!(
            (avg_cpu_cycles - 29780.5).abs() < 0.01,
            "NTSC should average ~29780.5 CPU cycles per frame"
        );
    }

    #[test]
    fn test_pal_refresh_rate_calculation() {
        // PAL CPU runs at approximately 1.662607 MHz
        // PAL frame: 312 scanlines * 341 dots = 106392 PPU cycles
        // 106392 PPU cycles / 3.2 = 33247.5 CPU cycles per frame
        // Refresh rate: 1662607 / 33247.5 ≈ 50.00 Hz

        let tv_system = TimingMode::Pal;
        let frame_ppu_cycles = 312 * 341; // 106392
        let cpu_cycles_per_frame = frame_ppu_cycles as f64 / tv_system.ppu_cycles_per_cpu_cycle();

        assert_eq!(frame_ppu_cycles, 106392);
        assert!(
            (cpu_cycles_per_frame - 33247.5).abs() < 0.01,
            "PAL should have ~33247.5 CPU cycles per frame"
        );
    }

    #[test]
    fn test_apu_clocked_every_cpu_cycle() {
        // Test that the APU is clocked once for every CPU cycle
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        // Load a simple NOP program that executes predictably
        let rom_data = create_minimal_nrom_rom();
        let cartridge = load_test_cartridge(&rom_data);
        nes.insert_cartridge(cartridge);

        // Reset to start execution
        nes.reset(false);

        // Get initial frame counter cycle count
        let initial_cycle = nes.apu.borrow().frame_counter().get_cycle_counter();

        // Execute one CPU instruction (NOP = 2 cycles)
        let cpu_cycles = nes.run_cpu_tick();

        // APU should have been clocked once per CPU cycle
        let final_cycle = nes.apu.borrow().frame_counter().get_cycle_counter();
        let apu_cycles_elapsed = final_cycle - initial_cycle;

        assert_eq!(
            apu_cycles_elapsed, cpu_cycles as u32,
            "APU should be clocked once per CPU cycle"
        );
    }

    #[test]
    fn test_apu_clocked_for_nmi_cycles_once_per_cpu_cycle() {
        // NMI handling consumes 7 CPU cycles. Regardless of how the CPU services the NMI,
        // the APU should still be clocked exactly once per CPU cycle overall.
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        let rom_data = create_minimal_nrom_rom();
        let cartridge = load_test_cartridge(&rom_data);
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Force an NMI pending edge before executing the next instruction.
        nes.cpu.set_nmi_pending(true);

        let initial_cycle = nes.apu.borrow().frame_counter().get_cycle_counter();
        let cpu_cycles = nes.run_cpu_tick();
        let final_cycle = nes.apu.borrow().frame_counter().get_cycle_counter();
        let apu_cycles_elapsed = final_cycle - initial_cycle;

        assert_eq!(
            apu_cycles_elapsed, cpu_cycles as u32,
            "APU should be clocked once per CPU cycle even when NMI is serviced"
        );
    }

    #[test]
    fn test_apu_clocked_during_oam_dma() {
        // Test that the APU is clocked during OAM DMA cycles
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        let rom_data = create_minimal_nrom_rom();
        let cartridge = load_test_cartridge(&rom_data);
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Get initial APU cycle count
        let initial_cycle = nes.apu.borrow().frame_counter().get_cycle_counter();

        // Trigger an OAM DMA by writing to $4014
        nes.bus.borrow_mut().write(0x4014, 0x02, false);

        // Run a CPU tick which should execute the DMA
        let dma_cycles = nes.run_cpu_tick();

        // APU should have been clocked for all DMA cycles
        let final_cycle = nes.apu.borrow().frame_counter().get_cycle_counter();
        let apu_cycles_elapsed = final_cycle - initial_cycle;

        // OAM DMA takes 513 or 514 cycles, but returns min(cycles, 255) as u8
        assert!(
            dma_cycles == 255,
            "OAM DMA should return 255 (capped from 513/514 cycles)"
        );
        // APU should have been clocked for the actual DMA cycles (513 or 514)
        assert!(
            apu_cycles_elapsed == 513 || apu_cycles_elapsed == 514,
            "APU should be clocked 513 or 514 times during OAM DMA, got {}",
            apu_cycles_elapsed
        );
    }

    #[test]
    fn test_dmc_dma_stalls_cpu_on_sample_fetch() {
        // DMC DMA reads should stall the CPU (RDY low) for 1-4 cycles.
        // After set_enabled, there is a transfer_start_delay of 2-3 cycles
        // before the DMA request becomes visible. Run enough ticks for the
        // delay to expire and the stall to occur.
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        let rom_data = create_minimal_nrom_rom();
        let cartridge = load_test_cartridge(&rom_data);
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Configure DMC to play 1 byte starting at $C000, at the fastest rate.
        // The sample buffer starts empty, so the DMC should attempt a memory fetch
        // once the transfer_start_delay expires.
        {
            let mut apu = nes.apu.borrow_mut();
            apu.dmc_mut().write_flags_and_rate(0x0F);
            apu.dmc_mut().write_sample_address(0x00);
            apu.dmc_mut().write_sample_length(0x00);
            apu.write_enable(0x10);
        }

        // Run up to 4 CPU ticks; at least one should show the DMA stall
        let mut found_stall = false;
        for _ in 0..4 {
            let cpu_cycles = nes.run_cpu_tick();
            if cpu_cycles > 2 {
                found_stall = true;
                assert!(
                    (3..=6).contains(&cpu_cycles),
                    "expected DMC DMA stalling to add 1-4 cycles to a 2-cycle NOP; got {cpu_cycles}"
                );
                break;
            }
        }

        assert!(
            found_stall,
            "expected DMC DMA stall within 4 CPU ticks after enabling DMC"
        );
    }

    #[test]
    fn test_sample_ready_initially_false() {
        // Test that sample_ready returns false initially
        let nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        assert!(!nes.sample_ready());
    }

    #[test]
    fn test_sample_ready_after_clocking() {
        // Test that sample_ready returns true after enough APU clocks
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        let rom_data = create_minimal_nrom_rom();
        let cartridge = load_test_cartridge(&rom_data);
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Clock the APU until a sample is ready
        // At 44100 Hz sample rate and 1789773 Hz CPU clock:
        // cycles_per_sample = 1789773 / 44100 ≈ 40.59 cycles
        // So we need to run at least 41 CPU cycles
        for _ in 0..50 {
            nes.run_cpu_tick();
            if nes.sample_ready() {
                break;
            }
        }

        assert!(nes.sample_ready());
    }

    #[test]
    fn test_get_sample_returns_value() {
        // Test that get_sample returns a valid audio sample
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        let rom_data = create_minimal_nrom_rom();
        let cartridge = load_test_cartridge(&rom_data);
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Clock until a sample is ready
        for _ in 0..50 {
            nes.run_cpu_tick();
            if nes.sample_ready() {
                break;
            }
        }

        // Get the sample
        let sample = nes.get_sample();
        assert!(sample.is_some());

        // Sample should be in valid range 0.0 to 1.0
        let sample_value = sample.unwrap();
        assert!((0.0..=1.0).contains(&sample_value));
    }

    #[test]
    fn test_get_sample_clears_ready_flag() {
        // Test that get_sample clears the sample_ready flag
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        let rom_data = create_minimal_nrom_rom();
        let cartridge = load_test_cartridge(&rom_data);
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Clock until a sample is ready
        for _ in 0..50 {
            nes.run_cpu_tick();
            if nes.sample_ready() {
                break;
            }
        }

        assert!(nes.sample_ready());

        // Get the sample
        nes.get_sample();

        // sample_ready should now return false
        assert!(!nes.sample_ready());
    }

    #[test]
    fn test_get_sample_returns_none_when_not_ready() {
        // Test that get_sample returns None when no sample is ready
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        let sample = nes.get_sample();
        assert!(sample.is_none());
    }

    /// Helper function to create a minimal NROM ROM for testing
    fn create_minimal_nrom_rom() -> Vec<u8> {
        let mut rom = Vec::new();

        // iNES header
        rom.extend_from_slice(b"NES\x1A"); // Signature
        rom.push(2); // 2 * 16KB PRG ROM
        rom.push(1); // 1 * 8KB CHR ROM
        rom.push(0x00); // Flags 6: Mapper 0 (NROM)
        rom.push(0x00); // Flags 7
        rom.extend_from_slice(&[0; 8]); // Unused padding

        // 32KB PRG ROM (2 * 16KB) - filled with NOPs
        rom.extend_from_slice(&[0xEA; 32768]); // NOP instruction

        // 8KB CHR ROM - filled with zeros
        rom.extend_from_slice(&[0x00; 8192]);

        rom
    }

    #[test]
    fn test_nes_reset_resets_cartridge() {
        let rom_data = create_minimal_nrom_rom();
        let cartridge = load_test_cartridge(&rom_data);

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Write to PRG-RAM (would be affected by a mapper that has reset state)
        nes.bus.borrow_mut().write(0x6000, 0xAB, false);
        assert_eq!(nes.bus.borrow_mut().read(0x6000, false), 0xAB);

        // Soft reset should call cartridge reset
        // For NROM this doesn't change much, but the call chain should work
        nes.reset(true);

        // PRG-RAM should still have the value (NROM doesn't clear RAM on reset)
        // This test verifies the reset call chain works without crashing
        assert_eq!(nes.bus.borrow_mut().read(0x6000, false), 0xAB);
    }

    #[test]
    fn test_save_state_roundtrip() {
        // Create and initialize NES
        let rom_data = create_minimal_nrom_rom();
        let cartridge = load_test_cartridge(&rom_data);

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Run some cycles to get the emulator into an interesting state
        for _ in 0..1000 {
            nes.run_cpu_tick();
        }

        // Capture state after some execution
        let state1 = nes.save_state();

        // Run more cycles
        for _ in 0..500 {
            nes.run_cpu_tick();
        }

        // Capture state after more execution (should be different)
        let state2 = nes.save_state();

        // States should be different
        assert_ne!(state1.cpu.pc, state2.cpu.pc);

        // Now restore to state1
        nes.load_state(&state1).expect("load_state should succeed");

        // Capture state again - should match state1
        let state_restored = nes.save_state();
        assert_eq!(state_restored.cpu.a, state1.cpu.a);
        assert_eq!(state_restored.cpu.x, state1.cpu.x);
        assert_eq!(state_restored.cpu.y, state1.cpu.y);
        assert_eq!(state_restored.cpu.sp, state1.cpu.sp);
        assert_eq!(state_restored.cpu.pc, state1.cpu.pc);
        assert_eq!(state_restored.cpu.p, state1.cpu.p);
        assert_eq!(state_restored.cpu.total_cycles, state1.cpu.total_cycles);
    }

    #[test]
    fn test_save_state_version_check() {
        let rom_data = create_minimal_nrom_rom();
        let cartridge = load_test_cartridge(&rom_data);

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Create a state with an incompatible version
        let mut state = nes.save_state();
        state.version = 9999; // Invalid version

        // Loading should fail with version error
        let result = nes.load_state(&state);
        assert!(result.is_err());

        if let Err(super::NesSaveStateError::Common(
            crate::platform::save_state::SaveStateError::IncompatibleVersion { found, supported },
        )) = result
        {
            assert_eq!(found, 9999);
            assert_eq!(supported, vec![SAVESTATE_VERSION]);
        } else {
            panic!("Expected IncompatibleVersion error");
        }
    }

    #[test]
    fn test_save_state_mapper_mismatch() {
        let rom_data = create_minimal_nrom_rom();
        let cartridge = load_test_cartridge(&rom_data);

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Create a state with a different mapper number
        let mut state = nes.save_state();
        state.mapper.mapper_number = 4; // MMC3 instead of NROM (0)

        // Loading should fail with mapper mismatch error
        let result = nes.load_state(&state);
        assert!(result.is_err());

        if let Err(super::NesSaveStateError::MapperMismatch { expected, found }) = result {
            assert_eq!(expected, 0); // NROM
            assert_eq!(found, 4); // MMC3
        } else {
            panic!("Expected MapperMismatch error");
        }
    }

    /// Fixed RAM seed for the golden fixture, so regenerating produces a
    /// reviewable diff instead of fresh noise. `Config::default()` uses
    /// `RamInitMode::Random` off wasm, which made every regeneration differ in
    /// its RAM contents and hid whether the *format* had actually changed --
    /// unhelpful when the diff is an opaque `.gz` (#3107).
    const GOLDEN_FIXTURE_RAM_SEED: u64 = 0x3107;

    /// Regenerates the committed golden save-state fixture used by
    /// `test_golden_save_state_v8_loads`.
    ///
    /// Appropriate only when `SAVESTATE_VERSION` is bumped. After running this
    /// helper you MUST manually add `"_neser_fixture_marker": "nes-v8-golden"`
    /// back into the JSON before recompressing, so that the sentinel assertion
    /// in the load test continues to detect future accidental regenerations.
    ///
    /// Writing is therefore opt-in, so `cargo test -- --include-ignored` cannot
    /// clobber the fixture (#3107):
    /// `NESER_REGENERATE_FIXTURES=1 cargo test --no-default-features --lib \
    ///   nes::console::nes::tests::regenerate_golden_save_state_fixture -- --ignored`
    #[test]
    #[ignore = "regenerates a committed fixture; run manually with NESER_REGENERATE_FIXTURES=1"]
    fn regenerate_golden_save_state_fixture() {
        let rom_data = create_minimal_nrom_rom();
        let cartridge = load_test_cartridge(&rom_data);
        let mut config = Config::default();
        config.frontend.ram_init_mode =
            crate::platform::config::RamInitMode::SeededRandom(GOLDEN_FIXTURE_RAM_SEED);
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            config,
        ));
        nes.insert_cartridge(cartridge);
        nes.reset(false);
        for _ in 0..1000 {
            nes.run_cpu_tick();
        }
        let bytes = nes.save_state().to_bytes().expect("serialize save state");
        let compressed = crate::platform::save_state::gzip_compress(&bytes);
        if !crate::platform::save_state::fixture_regeneration_enabled() {
            return;
        }
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/nes/console/testdata");
        std::fs::create_dir_all(&dir).expect("create testdata dir");
        std::fs::write(dir.join("savestate_golden_v8.json.gz"), compressed).expect("write fixture");
    }

    #[test]
    fn test_golden_save_state_v8_loads() {
        // Round-trip stability test: the committed fixture must deserialize and
        // restore successfully with the current loader. This proves the
        // serialized format is stable across refactors that do not bump
        // SAVESTATE_VERSION.
        //
        // The fixture carries a `_neser_fixture_marker` key that current code
        // never emits; serde ignores it on load (unknown-field tolerance). Its
        // presence here detects accidental regeneration -- if the marker
        // disappears the fixture has been silently replaced by current output
        // and no longer tests anything meaningful.
        //
        // NES save states have never supported loading an older version, so
        // there is no genuinely-old fixture to keep. If backward compat across
        // a schema change without a version bump ever needs to be proven, add a
        // JSON-manipulation test (see the GB system for examples).
        let compressed = include_bytes!("testdata/savestate_golden_v8.json.gz");
        let bytes = crate::platform::save_state::gzip_decompress(compressed);

        assert!(
            bytes
                .windows(b"\"_neser_fixture_marker\"".len())
                .any(|w| w == b"\"_neser_fixture_marker\""),
            "golden fixture must contain the sentinel; do not replace it with plain current output"
        );

        let state = SaveState::from_bytes(&bytes).expect("golden save state should deserialize");
        assert_eq!(state.version, SAVESTATE_VERSION);

        let rom_data = create_minimal_nrom_rom();
        let cartridge = load_test_cartridge(&rom_data);
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cartridge);
        nes.reset(false);
        nes.load_state(&state)
            .expect("golden save state should load");
    }

    #[test]
    fn test_main_components_implement_stateful() {
        // The top-level save-state is assembled through the `Stateful` trait, so
        // every main state-owning component must implement it.
        fn assert_stateful<T: crate::platform::save_state::Stateful>() {}
        assert_stateful::<Cpu>();
        assert_stateful::<Ppu>();
        assert_stateful::<Apu>();
        assert_stateful::<Bus>();
    }

    #[test]
    fn test_load_state_clears_joypad_button_states() {
        // Regression: pressing a D-pad button, saving state, then loading that
        // state used to leave the button stuck pressed even with no key held.
        let rom_data = create_minimal_nrom_rom();
        let cartridge = load_test_cartridge(&rom_data);

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Press Up on P1 and save the state while it is held.
        nes.set_button(1, crate::nes::input::Button::Up, true);
        assert_ne!(
            nes.get_joypad_button_states(1) & (1 << crate::nes::input::Button::Up as u8),
            0,
            "Up should be pressed before save"
        );
        let state = nes.save_state();

        // Release the button physically (simulates the player's finger lifting
        // before or after the restore — neither should leave it stuck).
        nes.set_button(1, crate::nes::input::Button::Up, false);

        // Re-press the button, then restore — the restored state must not carry
        // the pressed bit forward.
        nes.set_button(1, crate::nes::input::Button::Up, true);
        nes.load_state(&state).expect("load_state should succeed");

        assert_eq!(
            nes.get_joypad_button_states(1),
            0,
            "All joypad buttons must be cleared after loading a save state"
        );
    }

    #[test]
    fn test_save_state_deterministic_execution() {
        // This test verifies that loading a state and running produces identical results
        let rom_data = create_minimal_nrom_rom();
        let cartridge = load_test_cartridge(&rom_data);

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Run to get to an interesting state
        for _ in 0..500 {
            nes.run_cpu_tick();
        }

        // Save state
        let saved_state = nes.save_state();

        // Run 100 more cycles and capture the resulting state
        for _ in 0..100 {
            nes.run_cpu_tick();
        }
        let end_state1 = nes.save_state();

        // Restore to saved state
        nes.load_state(&saved_state).unwrap();

        // Run the same 100 cycles
        for _ in 0..100 {
            nes.run_cpu_tick();
        }
        let end_state2 = nes.save_state();

        // Both end states should be identical (deterministic execution)
        assert_eq!(end_state1.cpu.pc, end_state2.cpu.pc);
        assert_eq!(end_state1.cpu.a, end_state2.cpu.a);
        assert_eq!(end_state1.cpu.x, end_state2.cpu.x);
        assert_eq!(end_state1.cpu.y, end_state2.cpu.y);
        assert_eq!(end_state1.cpu.sp, end_state2.cpu.sp);
        assert_eq!(end_state1.cpu.p, end_state2.cpu.p);
        assert_eq!(end_state1.cpu.total_cycles, end_state2.cpu.total_cycles);
        assert_eq!(
            end_state1.ppu.timing.scanline,
            end_state2.ppu.timing.scanline
        );
        assert_eq!(end_state1.ppu.timing.pixel, end_state2.ppu.timing.pixel);
    }

    #[test]
    fn test_insert_cartridge_enables_paddle_for_known_crc() {
        let rom_data = create_minimal_nrom_rom();
        let mut cartridge = load_test_cartridge(&rom_data);
        cartridge.set_crc32_for_test(0x32FB0583);

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cartridge);

        // Should configure paddle on port 2 for Arkanoid ROM
        nes.bus.borrow_mut().set_mouse_x_position(0xA5);
        nes.bus.borrow_mut().set_mouse_left_button(true);

        // Verify port 2 reads paddle data
        nes.bus.borrow_mut().write(0x4016, 0x01, false);
        nes.bus.borrow_mut().write(0x4016, 0x00, false);
        let paddle_bits = nes.bus.borrow_mut().read(0x4017, false) & 0x18;
        assert_eq!(paddle_bits, 0x08); // Should have paddle data on port 2
    }

    #[test]
    fn test_insert_cartridge_disables_paddle_for_unknown_crc() {
        let rom_data = create_minimal_nrom_rom();
        let mut cartridge = load_test_cartridge(&rom_data);
        cartridge.set_crc32_for_test(0xDEADBEEF);

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cartridge);

        // Should have joypad on both ports by default
        nes.bus
            .borrow_mut()
            .set_button(2, crate::nes::input::Button::A, true);
        nes.bus.borrow_mut().write(0x4016, 0x01, false);
        nes.bus.borrow_mut().write(0x4016, 0x00, false);
        let joypad_bit = nes.bus.borrow_mut().read(0x4017, false) & 0x01;
        assert_eq!(joypad_bit, 1); // Should have joypad data on port 2
    }

    #[test]
    fn test_insert_cartridge_does_not_add_second_arkanoid_port() {
        let rom_data = create_minimal_nrom_rom();
        let mut cartridge = load_test_cartridge(&rom_data);
        cartridge.set_crc32_for_test(0x32FB0583);

        let config = Config {
            nes: NesConfig {
                controller_port1: crate::nes::input::ControllerType::Arkanoid,
                controller_port1_explicit: true,
                controller_port2: crate::nes::input::ControllerType::Joypad,
                controller_port2_explicit: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            config,
        ));
        nes.insert_cartridge(cartridge);

        let bus_state = nes.bus.borrow().capture_state();
        assert!(matches!(
            bus_state.port1_controller,
            crate::nes::bus::ControllerStateWrapper::Arkanoid(_)
        ));
        assert!(matches!(
            bus_state.port2_controller,
            crate::nes::bus::ControllerStateWrapper::Joypad(_)
        ));
    }

    #[test]
    fn test_insert_cartridge_enables_zapper_for_known_crc() {
        let rom_data = create_minimal_nrom_rom();
        let mut cartridge = load_test_cartridge(&rom_data);
        // Duck Hunt (Japan) — Famicom ROM with Zapper4017 expansion type.
        // On Famicom, the Zapper goes to the expansion port, not a controller port.
        cartridge.set_crc32_for_test(0x24598791);

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cartridge);

        // Both controller ports should remain Joypads
        let bus_state = nes.bus.borrow().capture_state();
        assert!(matches!(
            bus_state.port1_controller,
            crate::nes::bus::ControllerStateWrapper::Joypad(_)
        ));
        assert!(matches!(
            bus_state.port2_controller,
            crate::nes::bus::ControllerStateWrapper::Joypad(_)
        ));

        // Expansion port should be configured for Zapper
        assert_eq!(
            nes.app_context.borrow().config().nes.expansion_port,
            crate::nes::console::ExpansionPort::ZapperFamicom
        );
    }

    #[test]
    fn test_insert_cartridge_keeps_explicit_port2_when_zapper_detected() {
        let rom_data = create_minimal_nrom_rom();
        let mut cartridge = load_test_cartridge(&rom_data);
        cartridge.set_crc32_for_test(0x24598791);

        let config = Config {
            nes: NesConfig {
                controller_port1: crate::nes::input::ControllerType::Joypad,
                controller_port1_explicit: false,
                controller_port2: crate::nes::input::ControllerType::Joypad,
                controller_port2_explicit: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            config,
        ));
        nes.insert_cartridge(cartridge);

        let bus_state = nes.bus.borrow().capture_state();
        assert!(matches!(
            bus_state.port2_controller,
            crate::nes::bus::ControllerStateWrapper::Joypad(_)
        ));
    }

    #[test]
    fn test_insert_cartridge_does_not_apply_rom_db_controller_when_any_port_is_explicit() {
        let rom_data = create_minimal_nrom_rom();
        let mut cartridge = load_test_cartridge(&rom_data);
        cartridge.set_crc32_for_test(0x24598791); // ROM DB maps this to Zapper on port 2

        let config = Config {
            nes: NesConfig {
                controller_port1: crate::nes::input::ControllerType::Joypad,
                controller_port1_explicit: true,
                controller_port2: crate::nes::input::ControllerType::Joypad,
                controller_port2_explicit: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            config,
        ));
        nes.insert_cartridge(cartridge);

        let bus_state = nes.bus.borrow().capture_state();
        assert!(matches!(
            bus_state.port1_controller,
            crate::nes::bus::ControllerStateWrapper::Joypad(_)
        ));
        assert!(matches!(
            bus_state.port2_controller,
            crate::nes::bus::ControllerStateWrapper::Joypad(_)
        ));
    }

    #[test]
    fn test_insert_cartridge_auto_detects_power_pad_on_port2() {
        let rom_data = create_minimal_nrom_rom();
        let mut cartridge = load_test_cartridge(&rom_data);
        // World Class Track Meet (U) — PowerPadSideA (expansion_type=11)
        cartridge.set_crc32_for_test(0x5734EB9E);

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cartridge);

        let bus_state = nes.bus.borrow().capture_state();
        assert!(
            matches!(
                bus_state.port2_controller,
                crate::nes::bus::ControllerStateWrapper::PowerPad(_)
            ),
            "Power Pad should be auto-detected on port 2 for World Class Track Meet"
        );
        assert!(
            matches!(
                bus_state.port1_controller,
                crate::nes::bus::ControllerStateWrapper::Joypad(_)
            ),
            "Port 1 should remain Joypad when Power Pad is on port 2"
        );
    }

    #[test]
    fn test_insert_cartridge_does_not_auto_detect_power_pad_when_port_is_explicit() {
        let rom_data = create_minimal_nrom_rom();
        let mut cartridge = load_test_cartridge(&rom_data);
        cartridge.set_crc32_for_test(0x5734EB9E); // World Class Track Meet

        let config = Config {
            nes: NesConfig {
                controller_port2: crate::nes::input::ControllerType::Joypad,
                controller_port2_explicit: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            config,
        ));
        nes.insert_cartridge(cartridge);

        let bus_state = nes.bus.borrow().capture_state();
        assert!(
            matches!(
                bus_state.port2_controller,
                crate::nes::bus::ControllerStateWrapper::Joypad(_)
            ),
            "Explicit port config should override Power Pad auto-detection"
        );
    }

    #[test]
    fn test_switch_cartridge_from_joypad_game_to_arkanoid_auto_detects_controller() {
        // Reproduce: starting a NES joypad game then switching to Arkanoid via
        // Ctrl-O should auto-detect the Arkanoid controller just as it would when
        // loading Arkanoid directly from the command line.
        let rom_data = create_minimal_nrom_rom();

        // Step 1: Insert a non-Arkanoid cartridge (simulates an NES game running).
        let mut joypad_cartridge = load_test_cartridge(&rom_data);
        joypad_cartridge.set_crc32_for_test(0xDEADBEEF); // unknown CRC → joypad default
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(joypad_cartridge);

        // Confirm the baseline: port 2 is a joypad.
        let bus_state = nes.bus.borrow().capture_state();
        assert!(
            matches!(
                bus_state.port2_controller,
                crate::nes::bus::ControllerStateWrapper::Joypad(_)
            ),
            "Expected joypad on port 2 after non-Arkanoid ROM, got {:?}",
            bus_state.port2_controller
        );

        // Step 2: Switch to Arkanoid (simulates Ctrl-O ROM switch).
        let mut arkanoid_cartridge = load_test_cartridge(&rom_data);
        arkanoid_cartridge.set_crc32_for_test(0x32FB0583); // NES Arkanoid (port 2)
        nes.insert_cartridge(arkanoid_cartridge);

        // Port 2 should now be an Arkanoid controller.
        let bus_state = nes.bus.borrow().capture_state();
        assert!(
            matches!(
                bus_state.port2_controller,
                crate::nes::bus::ControllerStateWrapper::Arkanoid(_)
            ),
            "Expected Arkanoid on port 2 after ROM switch, got {:?}",
            bus_state.port2_controller
        );
        // Port 1 should remain joypad.
        assert!(
            matches!(
                bus_state.port1_controller,
                crate::nes::bus::ControllerStateWrapper::Joypad(_)
            ),
            "Expected joypad on port 1 after ROM switch"
        );
    }

    #[test]
    fn test_switch_cartridge_from_power_pad_to_normal_resets_to_joypad() {
        // Regression: after auto-detecting Power Pad for ROM A, loading a normal
        // ROM B (no special controller) must NOT keep Power Pad on port 2.
        let rom_data = create_minimal_nrom_rom();

        // Step 1: Load a Power Pad game.
        let mut power_pad_cartridge = load_test_cartridge(&rom_data);
        power_pad_cartridge.set_crc32_for_test(0x5734EB9E); // World Class Track Meet
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(power_pad_cartridge);

        assert_eq!(
            nes.active_controller_port_type(2),
            ControllerType::PowerPad,
            "Power Pad should be active after World Class Track Meet"
        );

        // Step 2: Switch to a normal joypad game.
        let mut joypad_cartridge = load_test_cartridge(&rom_data);
        joypad_cartridge.set_crc32_for_test(0xDEADBEEF); // Unknown CRC → joypad default
        nes.insert_cartridge(joypad_cartridge);

        // Bus and active types must both revert to joypad.
        let bus_state = nes.bus.borrow().capture_state();
        assert!(
            matches!(
                bus_state.port2_controller,
                crate::nes::bus::ControllerStateWrapper::Joypad(_)
            ),
            "Port 2 bus controller should reset to joypad after switching to a normal game, got {:?}",
            bus_state.port2_controller
        );
        assert_eq!(
            nes.active_controller_port_type(2),
            ControllerType::Joypad,
            "active_controller_port2 should reset to joypad"
        );
    }

    //
    // On hard reset, if the cartridge has a trainer, the CPU must start at
    // $7003 (not the game's reset vector). The game's reset vector is pushed
    // to the stack so the trainer's RTS returns to it.
    // Soft reset must NOT re-execute the trainer.

    /// Builds a minimal valid iNES ROM for Mapper 6 (submapper 1).
    /// The game reset vector at $FFFC–$FFFD is set to `game_reset_vector`.
    /// When `include_trainer` is true, a 512-byte trainer block is included.
    fn build_smc_rom(game_reset_vector: u16, include_trainer: bool) -> Vec<u8> {
        // 256KB PRG ROM (16 × 16 KiB). CHR RAM (0 CHR ROM).
        // Mapper 6: byte6 bits[7:4] = 0x6.  Trainer flag = bit 2 of byte 6.
        let prg_size_units = 16u8;
        let trainer_flag = if include_trainer { 0x04 } else { 0x00 };
        let flags6 = 0x60 | trainer_flag; // mapper lower nibble = 6, trainer bit
        let mut rom = vec![
            b'N',
            b'E',
            b'S',
            0x1A,           // iNES magic
            prg_size_units, // PRG ROM: 16 × 16 KiB = 256 KiB
            0,              // CHR ROM: 0 (uses CHR RAM)
            flags6,         // Flags 6
            0x00,           // Flags 7: mapper upper nibble = 0
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0, // Bytes 8–15: unused
        ];
        if include_trainer {
            // 512-byte trainer: simple pattern
            for i in 0u16..512 {
                rom.push((i as u8).wrapping_add(0x55));
            }
        }
        // 256 KiB PRG ROM, all NOP ($EA) so execution doesn't crash
        let prg_len = prg_size_units as usize * 16 * 1024;
        let mut prg = vec![0xEA_u8; prg_len]; // NOP sled
        // Submapper 1 → latch_mode 1 (UN1ROM+CHRSW):
        //   $C000–$FFFF fixed to absolute 16 KiB bank #7 (8 KiB banks 14+15)
        //   Bank #7 starts at offset 7 * 16384 within PRG.
        //   $FFFC is at offset 0x3FFC = 16380 within a 16 KiB bank.
        let reset_vec_offset = 7 * 16384 + 16380;
        prg[reset_vec_offset] = (game_reset_vector & 0xFF) as u8;
        prg[reset_vec_offset + 1] = (game_reset_vector >> 8) as u8;
        rom.extend(prg);
        rom
    }

    #[test]
    fn test_hard_reset_with_trainer_sets_pc_to_7003() {
        let game_vector = 0xC000u16;
        let rom = build_smc_rom(game_vector, true);
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new());
        nes.insert_cartridge(load_test_cartridge(&rom));
        nes.reset(false); // hard reset
        assert_eq!(
            nes.cpu.pc(),
            0x7003,
            "CPU must start at $7003 (trainer entry) on hard reset with trainer"
        );
    }

    #[test]
    fn test_hard_reset_with_trainer_pushes_game_vector_to_stack() {
        let game_vector = 0xC123u16;
        let rom = build_smc_rom(game_vector, true);
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new());
        nes.insert_cartridge(load_test_cartridge(&rom));
        nes.reset(false); // hard reset
        // JSR convention: stack holds (game_vector − 1).
        // After hard reset SP was 0xFD; two pushes bring it to 0xFB.
        let sp = nes.cpu.sp();
        let return_addr = game_vector.wrapping_sub(1);
        let stacked_lo = nes
            .bus
            .borrow_mut()
            .read(0x0100 | (sp.wrapping_add(1)) as u16, false);
        let stacked_hi = nes
            .bus
            .borrow_mut()
            .read(0x0100 | (sp.wrapping_add(2)) as u16, false);
        let stacked = (stacked_hi as u16) << 8 | stacked_lo as u16;
        assert_eq!(
            stacked, return_addr,
            "Stack must hold game_vector − 1 = ${:04X} for trainer RTS",
            return_addr
        );
    }

    #[test]
    fn test_soft_reset_with_trainer_does_not_jump_to_7003() {
        let game_vector = 0xC000u16;
        let rom = build_smc_rom(game_vector, true);
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new());
        nes.insert_cartridge(load_test_cartridge(&rom));
        nes.reset(false); // hard reset — goes to $7003
        nes.reset(true); // soft reset — must go to game reset vector
        assert_eq!(
            nes.cpu.pc(),
            game_vector,
            "Soft reset must NOT re-execute trainer; must go to game reset vector"
        );
    }

    #[test]
    fn test_hard_reset_without_trainer_uses_game_reset_vector() {
        let game_vector = 0xD000u16;
        let rom = build_smc_rom(game_vector, false);
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new());
        nes.insert_cartridge(load_test_cartridge(&rom));
        nes.reset(false); // hard reset — no trainer
        assert_eq!(
            nes.cpu.pc(),
            game_vector,
            "Hard reset without trainer must go directly to game reset vector"
        );
    }

    #[test]
    fn test_recent_cpu_trace_is_bounded_and_returns_recent_tail() {
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        let mut prg_rom = vec![0xEAu8; 32 * 1024];
        // Reset vector -> $8000
        prg_rom[0x7FFC] = 0x00;
        prg_rom[0x7FFD] = 0x80;
        let cartridge = crate::nes::cartridge::Cartridge::from_parts(
            prg_rom,
            vec![],
            crate::nes::cartridge::NametableLayout::Horizontal,
        );
        nes.insert_cartridge(cartridge);
        nes.cpu.set_pc(0x8000);
        nes.set_cpu_trace_enabled(true);

        let executed = MAX_CPU_TRACE_LINES + 20;
        for _ in 0..executed {
            nes.run_cpu_tick();
        }

        let full = nes.recent_cpu_trace(usize::MAX);
        assert_eq!(full.len(), MAX_CPU_TRACE_LINES);

        let recent = nes.recent_cpu_trace(32);
        assert_eq!(recent.len(), 32);
        let expected_first = 0x8000u16.wrapping_add((executed - 32) as u16);
        assert_eq!(recent[0].addr, expected_first);
    }

    #[test]
    fn test_savestate_version() {
        assert_eq!(SAVESTATE_VERSION, 8);
    }

    #[test]
    fn test_savestate_json_roundtrip() {
        let rom_data = create_minimal_nrom_rom();
        let cartridge = load_test_cartridge(&rom_data);
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cartridge);
        nes.reset(false);
        for _ in 0..100 {
            nes.run_cpu_tick();
        }

        let state = nes.save_state();
        let json = state.to_json().expect("serialization should succeed");
        let restored = SaveState::from_json(&json).expect("deserialization should succeed");

        assert_eq!(restored.version, state.version);
        assert_eq!(restored.cpu.a, state.cpu.a);
        assert_eq!(restored.cpu.pc, state.cpu.pc);
        assert_eq!(restored.ppu.timing.scanline, state.ppu.timing.scanline);
    }

    #[test]
    fn test_savestate_bytes_roundtrip() {
        let rom_data = create_minimal_nrom_rom();
        let cartridge = load_test_cartridge(&rom_data);
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cartridge);
        nes.reset(false);
        for _ in 0..100 {
            nes.run_cpu_tick();
        }

        let state = nes.save_state();
        let bytes = state.to_bytes().expect("serialization should succeed");
        let restored = SaveState::from_bytes(&bytes).expect("deserialization should succeed");

        assert_eq!(restored.version, state.version);
        assert_eq!(restored.cpu.x, state.cpu.x);
        assert_eq!(restored.cpu.y, state.cpu.y);
    }

    #[test]
    fn test_savestate_binary_bytes_roundtrip() {
        let rom_data = create_minimal_nrom_rom();
        let cartridge = load_test_cartridge(&rom_data);
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cartridge);
        nes.reset(false);
        for _ in 0..100 {
            nes.run_cpu_tick();
        }

        let state = nes.save_state();
        let bytes = state
            .to_binary_bytes()
            .expect("binary serialization should succeed");
        let restored =
            SaveState::from_binary_bytes(&bytes).expect("binary deserialization should succeed");

        assert_eq!(restored.version, state.version);
        assert_eq!(restored.cpu.a, state.cpu.a);
        assert_eq!(restored.cpu.x, state.cpu.x);
        assert_eq!(restored.cpu.y, state.cpu.y);
        assert_eq!(restored.ppu.timing.scanline, state.ppu.timing.scanline);
        assert_eq!(restored.ram, state.ram);
    }

    #[test]
    fn test_savestate_binary_bytes_are_smaller_than_json() {
        let rom_data = create_minimal_nrom_rom();
        let cartridge = load_test_cartridge(&rom_data);
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cartridge);
        nes.reset(false);
        for _ in 0..100 {
            nes.run_cpu_tick();
        }

        let state = nes.save_state();
        let json_bytes = state.to_bytes().expect("json serialization should succeed");
        let binary_bytes = state
            .to_binary_bytes()
            .expect("binary serialization should succeed");

        assert!(
            binary_bytes.len() < json_bytes.len(),
            "binary ({} bytes) should be smaller than JSON ({} bytes)",
            binary_bytes.len(),
            json_bytes.len()
        );
    }

    #[test]
    fn test_insert_cartridge_syncs_famicom_four_player_mode_to_bus() {
        use crate::nes::console::{ExpansionPort, HardwareMode};
        use crate::nes::input::Button;

        // Start with default NES mode — bus is constructed in NES mode
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        // Manually apply the Famicom four-player hint (simulating ROM DB auto-detect)
        // This updates the config but insert_cartridge must also sync the bus.
        nes.app_context
            .borrow_mut()
            .config_mut()
            .apply_rom_db_famicom_four_players_hint(true);

        // Verify config was set correctly
        assert_eq!(
            nes.app_context.borrow().config().nes.hardware_mode,
            HardwareMode::Famicom
        );
        assert_eq!(
            nes.app_context.borrow().config().nes.expansion_port,
            ExpansionPort::FamicomFourPlayers
        );

        // Now insert cartridge — this should sync the bus with the updated config
        let rom_data = create_minimal_rom();
        let cartridge = load_test_cartridge(&rom_data);
        nes.insert_cartridge(cartridge);

        // Set player 3's A button (port 3 maps to four_score_extra_button_states[0])
        nes.bus.borrow_mut().set_button(3, Button::A, true);

        // Strobe controller
        nes.bus.borrow_mut().write_for_testing(0x4016, 1);
        nes.bus.borrow_mut().write_for_testing(0x4016, 0);

        // Read $4016 — in Famicom four-player mode, bit 1 should carry player 3 serial data.
        // Player 3 has A button pressed, so the first serial bit (bit 0 of button state) should be 1.
        let value = nes.bus.borrow_mut().read_for_testing(0x4016);
        assert_eq!(
            value & 0x02,
            0x02,
            "Player 3 A button should appear on bit 1 of $4016 in Famicom four-player mode"
        );
    }

    #[test]
    fn test_nes_new_applies_configured_palette() {
        let mut config = Config::default();
        config.nes.palette = crate::nes::ppu::NesPalette::Smooth;
        let nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            config,
        ));
        assert_eq!(nes.current_palette(), crate::nes::ppu::NesPalette::Smooth);
        assert_eq!(
            nes.ppu.borrow().lookup_system_palette(0x00),
            (0x6A, 0x6A, 0x6A)
        );
    }

    #[test]
    fn test_nes_cycle_palette_advances() {
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        assert_eq!(nes.current_palette(), crate::nes::ppu::NesPalette::Default);
        assert_eq!(nes.cycle_palette(), crate::nes::ppu::NesPalette::NesDev);
        assert_eq!(nes.current_palette(), crate::nes::ppu::NesPalette::NesDev);
    }

    #[test]
    fn test_insert_cartridge_propagates_famicom_emphasis_to_ppu() {
        // Start with default NES mode — PPU has famicom_emphasis=false
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        assert!(
            !nes.ppu.borrow().famicom_emphasis,
            "PPU should start without Famicom emphasis in NES mode"
        );

        // Manually set the ROM DB hint so insert_cartridge triggers Famicom auto-detect
        nes.app_context
            .borrow_mut()
            .config_mut()
            .apply_rom_db_famicom_four_players_hint(true);

        let rom_data = create_minimal_rom();
        let cartridge = load_test_cartridge(&rom_data);
        nes.insert_cartridge(cartridge);

        // After insert_cartridge, the PPU emphasis should now reflect Famicom mode
        assert!(
            nes.ppu.borrow().famicom_emphasis,
            "PPU should have Famicom emphasis after ROM DB hint sets Famicom mode"
        );
    }

    #[test]
    fn test_save_ram_returns_ok_with_no_cartridge() {
        // When no cartridge is inserted, save_ram() has nothing to persist
        // and should return Ok(()) rather than an error.
        let nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        assert!(
            nes.save_ram().is_ok(),
            "save_ram with no cartridge must return Ok"
        );
    }

    #[test]
    fn test_nes_allowed_shaders_includes_expected_presets() {
        use crate::platform::emulator::Emulator;
        let nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let shaders = nes.allowed_shaders();
        assert!(
            shaders.contains(&"none"),
            "NES must allow the 'none' (stock) shader"
        );
        assert!(shaders.contains(&"crt"), "NES must allow the 'crt' shader");
        assert!(
            shaders.contains(&"smooth"),
            "NES must allow the 'smooth' shader"
        );
        assert!(
            shaders.contains(&"ntsc"),
            "NES must allow the 'ntsc' shader"
        );
        assert!(shaders.contains(&"pal"), "NES must allow the 'pal' shader");
        assert!(
            !shaders.contains(&"dmg"),
            "NES must NOT allow the 'dmg' shader"
        );
    }
}
