use super::apu_device::ApuDevice;
use super::controller_device::{
    ControllerDevice, VS_INPUT_COIN_SLOT1, VS_INPUT_COIN_SLOT2, VS_INPUT_SERVICE,
};
use super::mapper_device::MapperDevice;
use super::oam_dma_device::OamDmaDevice;
use super::ppu_device::PpuDevice;
use super::ram_device::RamDevice;
use crate::nes::apu::SharedApu;
use crate::nes::cartridge::Cartridge;
use crate::nes::console::{ExpansionPort, HardwareMode};
use crate::nes::input::{
    ArkanoidController, ArkanoidState, Button, Controller, ControllerType, JoypadState, NesJoypad,
    PowerPad, PowerPadButton, PowerPadState, SnesAdapter, SnesAdapterState, SnesButton, Zapper,
    ZapperState,
};
use crate::nes::ppu::{self, SharedPpu};
use crate::platform::app_context::SharedAppContext;
use crate::platform::debugging::log_info;
use crate::platform::save_state::Stateful;
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::io;
use std::ops::RangeInclusive;

/// Wrapper for controller state to support serialization.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ControllerStateWrapper {
    Joypad(JoypadState),
    SnesAdapter(SnesAdapterState),
    Arkanoid(ArkanoidState),
    Zapper(ZapperState),
    PowerPad(PowerPadState),
}

/// Bus state for save-state support.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BusState {
    pub open_bus: u8,
    pub oam_dma_page: Option<u8>,
    pub port1_controller: ControllerStateWrapper,
    pub port2_controller: ControllerStateWrapper,
    #[serde(default)]
    pub expansion_arkanoid: Option<ArkanoidState>,
    #[serde(default)]
    pub expansion_zapper: Option<ZapperState>,
    #[serde(default)]
    pub expansion_power_pad: Option<PowerPadState>,
}

/// Mapper state (opaque serialization).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MapperState {
    pub mapper_number: u16,
    pub prg_ram: Vec<u8>,
    pub chr_ram: Vec<u8>,
    pub registers: Vec<u8>,
}
use std::path::PathBuf;
use std::rc::Rc;

/// Configuration passed to [`BusDevice::sync_controller_modes`] when the
/// emulator config changes (e.g. expansion-port auto-detection from ROM DB).
#[derive(Debug, Clone, Default)]
pub struct ControllerModes {
    pub four_score_enabled: bool,
    pub famicom_four_players_enabled: bool,
    pub famicom_mode: bool,
    pub arkanoid_famicom_enabled: bool,
    pub zapper_famicom_enabled: bool,
    pub power_pad_famicom_enabled: bool,
    pub vs_system_enabled: bool,
    pub vs_dip_switches: u8,
    pub vs_hardware_type: Option<crate::nes::cartridge::VsHardwareType>,
}

pub trait BusDevice {
    fn read(&mut self, addr: u16, open_bus: u8, is_dummy_read: bool) -> Option<u8>;
    fn write(&mut self, addr: u16, value: u8, is_dummy_write: bool) -> bool;
    fn address_range(&self) -> RangeInclusive<u16>;
    fn sync_controller_modes(&mut self, _modes: &ControllerModes) {}
}

pub type SharedBus = Rc<RefCell<Bus>>;

/// NES Memory (64KB address space)
pub struct Bus {
    cpu_ram: Rc<RefCell<Vec<u8>>>,
    cartridge: Rc<RefCell<Option<Rc<RefCell<Cartridge>>>>>,
    ppu: SharedPpu,
    apu: SharedApu,
    app_context: SharedAppContext,
    oam_dma_page: Rc<RefCell<Option<u8>>>, // Stores the page for pending OAM DMA
    dma_triggered: Rc<RefCell<bool>>,
    controllers: [Rc<RefCell<Box<dyn Controller>>>; 2], // Port 1 and Port 2 controllers
    four_score_extra_button_states: Rc<RefCell<[u8; 2]>>, // Emulated players 3 and 4 button states
    expansion_arkanoid: Rc<RefCell<ArkanoidController>>, // Famicom expansion Arkanoid controller
    expansion_zapper: Rc<RefCell<Zapper>>,              // Famicom expansion Zapper controller
    expansion_power_pad: Rc<RefCell<PowerPad>>,         // Famicom expansion Power Pad controller
    vs_arcade_input: Rc<Cell<u8>>,                      // VS System coin/service input state
    vs_hardware_type: Option<crate::nes::cartridge::VsHardwareType>, // VS System hardware type from cartridge
    open_bus: u8, // Last value on the data bus for open bus behavior
    devices: Vec<Box<dyn BusDevice>>,
}

impl Bus {
    fn build_controller(
        ppu: Rc<RefCell<ppu::Ppu>>,
        app_context: Rc<RefCell<crate::platform::app_context::AppContext>>,
        controller_type: ControllerType,
    ) -> Box<dyn Controller> {
        match controller_type {
            ControllerType::Joypad => Box::new(NesJoypad::new()),
            ControllerType::SnesAdapter => Box::new(SnesAdapter::new()),
            ControllerType::SnesController => Box::new(SnesAdapter::new_controller()),
            ControllerType::SnesMouse => Box::new(SnesAdapter::new_mouse()),
            ControllerType::Arkanoid => Box::new(ArkanoidController::new()),
            ControllerType::Zapper => Box::new(Zapper::new(ppu, app_context)),
            ControllerType::PowerPad => Box::new(PowerPad::new()),
        }
    }

    /// Create a new memory instance with 64KB address space
    pub fn new(
        ppu: Rc<RefCell<ppu::Ppu>>,
        apu: SharedApu,
        app_context: Rc<RefCell<crate::platform::app_context::AppContext>>,
    ) -> Self {
        let controllers = [
            Rc::new(RefCell::new(Self::build_controller(
                ppu.clone(),
                app_context.clone(),
                ControllerType::Joypad,
            ))),
            Rc::new(RefCell::new(Self::build_controller(
                ppu.clone(),
                app_context.clone(),
                ControllerType::Joypad,
            ))),
        ];

        // Initialize CPU RAM based on config
        let mut cpu_ram = vec![0; 0x10000];
        let ram_init_mode = app_context.borrow().config().frontend.ram_init_mode;
        crate::nes::console::initialize_ram(&mut cpu_ram[0..0x800], ram_init_mode);

        let expansion_arkanoid = Rc::new(RefCell::new(ArkanoidController::new()));
        let expansion_zapper = Rc::new(RefCell::new(Zapper::new(ppu.clone(), app_context.clone())));
        let expansion_power_pad = Rc::new(RefCell::new(PowerPad::new()));
        let vs_arcade_input = Rc::new(Cell::new(0u8));

        let mut controller = Self {
            cpu_ram: Rc::new(RefCell::new(cpu_ram)),
            cartridge: Rc::new(RefCell::new(None)),
            ppu,
            apu,
            app_context,
            oam_dma_page: Rc::new(RefCell::new(None)),
            dma_triggered: Rc::new(RefCell::new(false)),
            controllers,
            four_score_extra_button_states: Rc::new(RefCell::new([0, 0])),
            expansion_arkanoid,
            expansion_zapper,
            expansion_power_pad,
            vs_arcade_input,
            vs_hardware_type: None,
            open_bus: 0xFF, // Initialize to 0xFF (common power-on state)
            devices: Vec::new(),
        };

        controller.register_device(Box::new(RamDevice::new(controller.cpu_ram.clone())));
        controller.register_device(Box::new(PpuDevice::new(
            controller.ppu.clone(),
            controller.cartridge.clone(),
        )));
        let four_score_enabled = controller
            .app_context
            .borrow()
            .config()
            .nes
            .four_score_enabled;
        let famicom_four_players_enabled = controller.is_famicom_four_players_configured();
        let mut controller_device = ControllerDevice::new_with_four_score_state(
            controller.controllers[0].clone(),
            controller.controllers[1].clone(),
            four_score_enabled,
            famicom_four_players_enabled,
            controller.four_score_extra_button_states.clone(),
        );
        controller_device.set_four_score_enabled(four_score_enabled);
        controller_device.set_famicom_four_players_enabled(famicom_four_players_enabled);
        controller_device
            .set_arkanoid_famicom_expansion(Some(controller.expansion_arkanoid.clone()));
        controller_device.set_zapper_famicom_expansion(Some(controller.expansion_zapper.clone()));
        controller_device
            .set_power_pad_famicom_expansion(Some(controller.expansion_power_pad.clone()));
        controller_device.set_vs_arcade_input(controller.vs_arcade_input.clone());
        controller.register_device(Box::new(controller_device));
        controller.register_device(Box::new(ApuDevice::new(controller.apu.clone())));
        controller.register_device(Box::new(OamDmaDevice::new(
            controller.oam_dma_page.clone(),
            controller.dma_triggered.clone(),
        )));
        controller.register_device(Box::new(MapperDevice::new(
            controller.cartridge.clone(),
            controller.ppu.clone(),
        )));

        controller.sync_controller_modes_from_config();

        controller
    }

    pub fn sync_controller_modes_from_config(&mut self) {
        let app_context = self.app_context.borrow();
        let config = app_context.config();
        let modes = ControllerModes {
            four_score_enabled: config.nes.four_score_enabled,
            famicom_four_players_enabled: Self::is_famicom_four_players(config),
            famicom_mode: config.nes.hardware_mode == HardwareMode::Famicom,
            arkanoid_famicom_enabled: Self::is_arkanoid_famicom(config),
            zapper_famicom_enabled: Self::is_zapper_famicom(config),
            power_pad_famicom_enabled: Self::is_power_pad_famicom(config),
            vs_system_enabled: Self::is_vs_system(config),
            vs_dip_switches: config.nes.vs_dip_switches,
            vs_hardware_type: self.vs_hardware_type,
        };
        drop(app_context);

        for device in self.devices.iter_mut() {
            device.sync_controller_modes(&modes);
        }
    }

    fn is_famicom_four_players_configured(&self) -> bool {
        Self::is_famicom_four_players(self.app_context.borrow().config())
    }

    fn is_famicom_four_players(config: &crate::nes::console::Config) -> bool {
        config.nes.hardware_mode == HardwareMode::Famicom
            && config.nes.expansion_port == ExpansionPort::FamicomFourPlayers
    }

    fn is_arkanoid_famicom(config: &crate::nes::console::Config) -> bool {
        config.nes.hardware_mode == HardwareMode::Famicom
            && config.nes.expansion_port == ExpansionPort::ArkanoidFamicom
    }

    fn is_zapper_famicom(config: &crate::nes::console::Config) -> bool {
        config.nes.hardware_mode == HardwareMode::Famicom
            && config.nes.expansion_port == ExpansionPort::ZapperFamicom
    }

    fn is_power_pad_famicom(config: &crate::nes::console::Config) -> bool {
        config.nes.hardware_mode == HardwareMode::Famicom
            && config.nes.expansion_port == ExpansionPort::PowerPadFamicom
    }

    fn is_vs_system(config: &crate::nes::console::Config) -> bool {
        config.nes.expansion_port == ExpansionPort::VsSystem
    }

    fn has_player34_serial_enabled(&self) -> bool {
        let config = self.app_context.borrow();
        config.config().nes.four_score_enabled || Self::is_famicom_four_players(config.config())
    }

    pub fn register_device(&mut self, device: Box<dyn BusDevice>) {
        self.devices.push(device);
    }

    /// Map a cartridge into memory
    ///
    /// This method:
    /// 1. Wraps the cartridge in Rc<RefCell<>> for shared ownership
    /// 2. Shares the cartridge with the PPU for CHR ROM/RAM access
    /// 3. Configures initial PPU mirroring mode from mapper
    /// 4. Makes cartridge accessible to CPU for PRG ROM/RAM operations
    ///
    /// The shared reference pattern allows both CPU (via Bus) and PPU
    /// to access the cartridge mapper independently while maintaining proper
    /// bank switching state.
    pub fn map_cartridge(&mut self, cartridge: Cartridge) {
        // Extract trainer data before wrapping in Rc<RefCell<>>
        let trainer_data = cartridge.trainer().map(|t| t.to_vec());
        let vs_ppu_type = cartridge.vs_ppu_type();
        let vs_hardware_type = cartridge.vs_hardware_type();

        // Wrap cartridge in Rc<RefCell<>> for shared access between CPU and PPU
        let cartridge_rc = Rc::new(RefCell::new(cartridge));

        // Load trainer data into cartridge memory at the mapper-specified address
        if let Some(trainer_bytes) = trainer_data {
            let mut cart = cartridge_rc.borrow_mut();
            let mapper = cart.mapper_mut();
            let base = mapper.capabilities().trainer_load_address;
            // Trainer data is always exactly 512 bytes from parsing validation
            for (i, byte) in trainer_bytes.iter().enumerate() {
                mapper.write_prg(base + i as u16, *byte);
            }
        }

        // Share cartridge reference with PPU for dynamic CHR access
        {
            let mut ppu = self.ppu.borrow_mut();
            ppu.set_cartridge(cartridge_rc.clone());
            ppu.set_mirroring(cartridge_rc.borrow().mapper().get_mirroring());
            ppu.set_vs_ppu_type(vs_ppu_type);
        }

        *self.cartridge.borrow_mut() = Some(cartridge_rc);

        // Propagate VS hardware type to controller device for game-specific quirks
        self.vs_hardware_type = vs_hardware_type;
        self.sync_controller_modes_from_config();
    }

    /// Reset the bus and its components.
    ///
    /// This method handles CPU RAM initialization and delegates cartridge reset
    /// to reset_cartridge(), which handles cartridge RAM initialization.
    ///
    /// - `soft_reset`: true for a reset-button style reset, false for power-on/hard reset
    /// - `ram_init_mode`: RAM initialization mode (only used for hard reset)
    pub fn reset(&mut self, soft_reset: bool, ram_init_mode: crate::nes::console::RamInitMode) {
        // On hard reset, re-initialize CPU RAM
        if !soft_reset {
            let mut cpu_ram = self.cpu_ram.borrow_mut();
            crate::nes::console::initialize_ram(&mut cpu_ram[0..0x800], ram_init_mode);
        }

        // Reset cartridge (and its RAM on hard reset)
        self.reset_cartridge(soft_reset, ram_init_mode);
    }

    /// Reset the cartridge (if present) to its power-on state.
    ///
    /// - `soft_reset`: true for a reset-button style reset, false for power-on/hard reset
    /// - `ram_init_mode`: RAM initialization mode (only used for hard reset)
    pub fn reset_cartridge(
        &mut self,
        soft_reset: bool,
        ram_init_mode: crate::nes::console::RamInitMode,
    ) {
        let Some(cartridge) = self.cartridge.borrow().as_ref().cloned() else {
            return;
        };

        // On hard reset, re-initialize cartridge RAM before resetting mapper state
        if !soft_reset {
            cartridge.borrow_mut().initialize_ram(ram_init_mode);
        }

        cartridge.borrow_mut().reset();

        // Sync PPU mirroring after mapper reset — the mapper's reset() may
        // change mirroring (e.g. mapper 41 clears registers to vertical).
        let mirroring = cartridge.borrow().mapper().get_mirroring();
        self.ppu.borrow_mut().set_mirroring(mirroring);
    }

    /// Returns `true` if the currently mapped cartridge has a 512-byte trainer block
    /// and its mapper executes it via JSR $7003 (Mapper 6 / SMC-801).
    pub fn cartridge_has_trainer_jsr(&self) -> bool {
        self.cartridge
            .borrow()
            .as_ref()
            .map(|c| {
                let cart = c.borrow();
                cart.has_trainer() && cart.mapper().capabilities().trainer_jsr
            })
            .unwrap_or(false)
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub fn cpu_ram_ref(&self) -> Rc<RefCell<Vec<u8>>> {
        self.cpu_ram.clone()
    }

    pub fn save_ram(&self) -> io::Result<()> {
        let Some(cartridge) = self.cartridge.borrow().as_ref().cloned() else {
            return Ok(());
        };

        cartridge.borrow().save_ram()
    }

    pub fn cartridge_state_path(&self) -> Option<PathBuf> {
        self.cartridge
            .borrow()
            .as_ref()
            .and_then(|cart| cart.borrow().state_path())
    }

    pub fn cartridge_debug_path(&self) -> Option<PathBuf> {
        self.cartridge
            .borrow()
            .as_ref()
            .and_then(|cart| cart.borrow().debug_path())
    }

    /// Debug-only-ish helper: read PRG ROM without affecting open bus or joypads.
    ///
    /// This is intended for debugger visualization (e.g., PRG ROM hexdumps).
    /// It only supports the PRG ROM CPU address range ($8000-$FFFF).
    pub fn read_prg_rom_for_debugger(&self, addr: u16) -> u8 {
        if !(0x8000..=0xFFFF).contains(&addr) {
            return 0;
        }

        self.cartridge
            .borrow()
            .as_ref()
            .map(|cart| cart.borrow().mapper().read_prg(addr))
            .unwrap_or(0)
    }

    /// Side-effect-free debug read of CPU-visible memory.
    ///
    /// Intended for debugger visualization (e.g., disassembly around PC). It avoids:
    /// - updating open bus
    /// - clocking controllers
    /// - touching PPU/APU registers
    ///
    /// Supported ranges:
    /// - $0000-$1FFF (CPU RAM with mirroring)
    /// - $6000-$FFFF (PRG RAM/ROM via mapper)
    pub fn read_cpu_for_debugger(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram.borrow()[(addr & 0x07FF) as usize],
            0x6000..=0xFFFF => self
                .cartridge
                .borrow()
                .as_ref()
                .map(|cart| {
                    cart.borrow()
                        .mapper()
                        .read_prg_open_bus(addr, self.open_bus)
                })
                .unwrap_or(0),
            _ => 0,
        }
    }

    pub fn read(&mut self, addr: u16, is_dummy_read: bool) -> u8 {
        if (0xFFFA..=0xFFFB).contains(&addr)
            && let Some(cartridge) = self.cartridge.borrow().as_ref().cloned()
        {
            cartridge.borrow_mut().mapper_mut().on_irq_vector_read(addr);
        }

        if let Some(value) = self.read_from_devices(addr, is_dummy_read) {
            self.open_bus = value;
            return value;
        }

        let value = {
            log_info(format!(
                "Warning: Read from unimplemented address {:04X}, returning 0",
                addr
            ));
            0
        };

        // Update open bus with the value read
        self.open_bus = value;
        // self.print_open_bus();
        value
    }

    fn read_from_devices(&mut self, addr: u16, is_dummy_read: bool) -> Option<u8> {
        for device in self.devices.iter_mut() {
            if device.address_range().contains(&addr)
                && let Some(value) = device.read(addr, self.open_bus, is_dummy_read)
            {
                return Some(value);
            }
        }

        None
    }

    /// Sample the mapper-generated IRQ line (e.g., MMC3 scanline IRQ).
    ///
    /// This is a level-triggered signal: it remains asserted until the mapper is acknowledged.
    pub fn mapper_irq_pending(&self) -> bool {
        self.cartridge
            .borrow()
            .as_ref()
            .map(|cart| cart.borrow().mapper().irq_pending())
            .unwrap_or(false)
    }

    /// Return the active mapper's capabilities, or `None` if no cartridge is inserted.
    pub fn cartridge_mapper_capabilities(
        &self,
    ) -> Option<crate::nes::cartridge::MapperCapabilities> {
        self.cartridge
            .borrow()
            .as_ref()
            .map(|cart| cart.borrow().mapper().capabilities())
    }

    /// Sample the mapper-provided expansion-audio output.
    pub fn mapper_expansion_audio_sample(&self) -> f32 {
        self.cartridge
            .borrow()
            .as_ref()
            .map(|cart| cart.borrow().mapper().expansion_audio_sample())
            .unwrap_or(0.0)
    }

    /// Tick the active mapper for one CPU cycle.
    ///
    /// Some mappers implement CPU-cycle-driven IRQ systems (e.g., Konami VRC IRQ).
    pub fn mapper_cpu_cycle(&mut self) {
        let Some(cartridge) = self.cartridge.borrow().as_ref().cloned() else {
            return;
        };

        cartridge.borrow_mut().mapper_mut().cpu_cycle();
    }

    #[cfg(test)]
    fn has_device_for_address(&self, addr: u16) -> bool {
        self.devices
            .iter()
            .any(|device| device.address_range().contains(&addr))
    }

    #[cfg(any(test, debug_assertions))]
    #[allow(dead_code)]
    pub fn read_for_testing(&mut self, addr: u16) -> u8 {
        let old_open_bus = self.open_bus;
        let value = self.read(addr, false);
        self.open_bus = old_open_bus;
        value
    }

    #[cfg(test)]
    pub fn write_for_testing(&mut self, addr: u16, value: u8) {
        let old_open_bus = self.open_bus;
        self.write(addr, value, false);
        self.open_bus = old_open_bus;
    }

    #[cfg(test)]
    pub fn mapper_ppu_address_changed_for_test(&mut self, addr: u16) {
        let Some(cartridge) = self.cartridge.borrow().as_ref().cloned() else {
            return;
        };

        cartridge
            .borrow_mut()
            .mapper_mut()
            .ppu_address_changed(addr & 0x1FFF);
    }

    /// Write a byte to memory
    /// Returns true if an OAM DMA was triggered (at $4014)
    pub fn write(&mut self, addr: u16, value: u8, is_dummy_write: bool) -> bool {
        // Update open bus with the value being written
        self.open_bus = value;

        let wrote = self.write_to_devices(addr, value, is_dummy_write);
        if wrote {
            if addr == 0x4014
                && !is_dummy_write
                && let Some(cartridge) = self.cartridge.borrow().as_ref().cloned()
            {
                cartridge.borrow_mut().mapper_mut().on_oam_dma();
            }
            if addr == 0x4016
                && !is_dummy_write
                && let Some(cartridge) = self.cartridge.borrow().as_ref().cloned()
            {
                cartridge
                    .borrow_mut()
                    .mapper_mut()
                    .on_controller_port_write(addr, value);
            }
            return self.dma_triggered.replace(false);
        }

        {
            log_info(format!(
                "Warning: Write to unimplemented address {:04X} ignored",
                addr
            ));
        }
        false // No DMA triggered
    }

    fn write_to_devices(&mut self, addr: u16, value: u8, is_dummy_write: bool) -> bool {
        for device in self.devices.iter_mut() {
            if device.address_range().contains(&addr) && device.write(addr, value, is_dummy_write) {
                return true;
            }
        }

        false
    }

    /// Write a 16-bit word to memory (little-endian)
    #[cfg(test)]
    pub fn write_u16(&mut self, addr: u16, value: u16) {
        let lo = (value & 0xFF) as u8;
        let hi = (value >> 8) as u8;
        self.write(addr, lo, false);
        self.write(addr.wrapping_add(1), hi, false);
    }

    /// Check if an OAM DMA is pending (without consuming it)
    pub fn oam_dma_pending(&self) -> bool {
        self.oam_dma_page.borrow().is_some()
    }

    /// Check if an OAM DMA is pending and get the page value
    pub fn take_oam_dma_page(&mut self) -> Option<u8> {
        self.oam_dma_page.borrow_mut().take()
    }

    /// Execute an OAM DMA transfer from the specified page to OAM
    /// Returns the number of bytes transferred (always 256)
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn execute_oam_dma(&mut self, page: u8) {
        let source_page = (page as u16) << 8;
        for i in 0..256u16 {
            let byte = self.read(source_page + i, false);
            self.ppu.borrow_mut().write_oam_data(byte);
        }
    }

    /// Set button state for a controller.
    ///
    /// In VS System mode, two remappings are applied (matching Mesen's behavior):
    ///
    /// 1. **Start↔Select swap** (all VS games): The arcade cabinet's Start button
    ///    is wired to the NES serial protocol's Select bit position (bit 2),
    ///    and vice versa.
    ///
    /// 2. **Port swap** (VsSystem4017 / "VsSystemSwapped" games only): The arcade
    ///    cabinet reads P1 from $4017 (left stick) and P2 from $4016 (right stick).
    ///    D-pad and action buttons are swapped between ports, while Start/Select
    ///    stay on their original port (so Start↔Select swap targets the correct
    ///    controller port for the game's "1P Start" / "2P Start" detection).
    pub fn set_button(&mut self, port: u8, button: Button, pressed: bool) {
        if (1..=2).contains(&port) {
            let config = self.app_context.borrow();
            let cfg = config.config();
            let is_vs = Self::is_vs_system(cfg);
            let vs_swapped = cfg.nes.vs_controllers_swapped;
            drop(config);

            // Step 1: For VsSystem4017 games, swap ports for d-pad/action buttons only.
            // Start/Select stay on their original port.
            let effective_port = if vs_swapped && !matches!(button, Button::Start | Button::Select)
            {
                3 - port // swap: 1→2, 2→1
            } else {
                port
            };

            // Step 2: For all VS games, swap Start↔Select.
            let effective_button = if is_vs {
                match button {
                    Button::Start => Button::Select,
                    Button::Select => Button::Start,
                    other => other,
                }
            } else {
                button
            };

            self.controllers[(effective_port - 1) as usize]
                .borrow_mut()
                .set_button(effective_button, pressed);
            return;
        }

        if !self.has_player34_serial_enabled() {
            return;
        }

        if !(3..=4).contains(&port) {
            return;
        }

        let mut states = self.four_score_extra_button_states.borrow_mut();
        let player_index = (port - 3) as usize;
        let bit = button as u8;
        if pressed {
            states[player_index] |= 1 << bit;
        } else {
            states[player_index] &= !(1 << bit);
        }
    }

    /// Set VS System coin insert state for a specific slot (0 or 1).
    pub fn set_vs_coin_insert(&self, slot: u8, pressed: bool) {
        let bit = if slot == 0 {
            VS_INPUT_COIN_SLOT1
        } else {
            VS_INPUT_COIN_SLOT2
        };
        let current = self.vs_arcade_input.get();
        if pressed {
            self.vs_arcade_input.set(current | bit);
        } else {
            self.vs_arcade_input.set(current & !bit);
        }
    }

    /// Set VS System service button state.
    pub fn set_vs_service_button(&self, pressed: bool) {
        let current = self.vs_arcade_input.get();
        if pressed {
            self.vs_arcade_input.set(current | VS_INPUT_SERVICE);
        } else {
            self.vs_arcade_input.set(current & !VS_INPUT_SERVICE);
        }
    }

    /// Set SNES-specific button state for a controller.
    pub fn set_snes_button(&mut self, port: u8, button: SnesButton, pressed: bool) -> bool {
        if !(1..=2).contains(&port) {
            return false;
        }

        self.controllers[(port - 1) as usize]
            .borrow_mut()
            .set_snes_button(button, pressed)
    }

    /// Set Power Pad button state for a controller.
    pub fn set_power_pad_button(
        &mut self,
        port: u8,
        button: PowerPadButton,
        pressed: bool,
    ) -> bool {
        if !(1..=2).contains(&port) {
            return false;
        }

        self.controllers[(port - 1) as usize]
            .borrow_mut()
            .set_power_pad_button(button, pressed)
    }

    pub fn set_expansion_power_pad_button(
        &mut self,
        button: PowerPadButton,
        pressed: bool,
    ) -> bool {
        if !self.is_power_pad_famicom_configured() {
            return false;
        }
        self.expansion_power_pad
            .borrow_mut()
            .set_button(button, pressed);
        true
    }

    /// Get joypad button states as a u8 bitmask (for autorun recording).
    /// Returns 0 if the controller is not a joypad.
    pub fn get_joypad_button_states(&self, port: u8) -> u8 {
        if self.has_player34_serial_enabled() && (3..=4).contains(&port) {
            return self.four_score_extra_button_states.borrow()[(port - 3) as usize];
        }

        if !(1..=2).contains(&port) {
            return 0;
        }

        let controller = self.controllers[(port - 1) as usize].borrow();
        let state = controller.capture_state();

        match state {
            crate::nes::input::ControllerState::Joypad(joypad_state) => joypad_state.button_states,
            _ => 0, // Not a joypad
        }
    }

    /// Set the controller type for a specific port.
    pub fn set_controller_type(&mut self, port: u8, controller_type: ControllerType) {
        if !(1..=2).contains(&port) {
            return;
        }

        let new_controller =
            Self::build_controller(self.ppu.clone(), self.app_context.clone(), controller_type);

        *self.controllers[(port - 1) as usize].borrow_mut() = new_controller;
    }

    /// Update mouse X position for any mouse-emulated controller (0..255).
    pub fn set_mouse_x_position(&mut self, position: u8) {
        for controller in &self.controllers {
            controller.borrow_mut().set_mouse_x_position(position);
        }
        self.expansion_arkanoid.borrow_mut().set_position(position);
        self.expansion_zapper
            .borrow_mut()
            .set_mouse_x_position(position);
    }

    /// Update mouse Y position for any mouse-emulated controller (0..255).
    pub fn set_mouse_y_position(&mut self, position: u8) {
        for controller in &self.controllers {
            controller.borrow_mut().set_mouse_y_position(position);
        }
        self.expansion_zapper
            .borrow_mut()
            .set_mouse_y_position(position);
    }

    /// Apply relative mouse delta for mouse-emulated controllers.
    pub fn add_mouse_delta(&mut self, dx: i16, dy: i16) {
        for controller in &self.controllers {
            controller.borrow_mut().add_mouse_delta(dx, dy);
        }
    }

    /// Update mouse left button state for any mouse-emulated controller.
    pub fn set_mouse_left_button(&mut self, pressed: bool) {
        for controller in &self.controllers {
            controller.borrow_mut().set_mouse_left_button(pressed);
        }
        self.expansion_arkanoid.borrow_mut().set_trigger(pressed);
        self.expansion_zapper
            .borrow_mut()
            .set_mouse_left_button(pressed);
    }

    /// Update mouse right button state for any mouse-emulated controller.
    pub fn set_mouse_right_button(&mut self, pressed: bool) {
        for controller in &self.controllers {
            controller.borrow_mut().set_mouse_right_button(pressed);
        }
    }

    /// Returns true when a Super NES mouse is active on any port.
    pub fn has_snes_mouse(&self) -> bool {
        self.controllers
            .iter()
            .any(|controller| controller.borrow().is_snes_mouse())
    }

    /// Return the input type for a controller port.
    pub fn controller_input_type(&self, port: u8) -> Option<crate::nes::input::ControllerInput> {
        if self.has_player34_serial_enabled() && (3..=4).contains(&port) {
            return Some(crate::nes::input::ControllerInput::Gamepad);
        }

        if !(1..=2).contains(&port) {
            return None;
        }

        Some(self.controllers[(port - 1) as usize].borrow().input_type())
    }

    /// Check if the expansion port has a mouse-controlled device (e.g. Famicom Arkanoid or Zapper).
    pub fn has_expansion_mouse_controller(&self) -> bool {
        self.is_arkanoid_famicom_configured() || self.is_zapper_famicom_configured()
    }

    #[allow(dead_code)]
    pub fn has_expansion_power_pad(&self) -> bool {
        self.is_power_pad_famicom_configured()
    }

    /// Check if a Zapper is connected to the Famicom expansion port.
    pub fn has_expansion_zapper(&self) -> bool {
        self.is_zapper_famicom_configured()
    }

    fn is_arkanoid_famicom_configured(&self) -> bool {
        Self::is_arkanoid_famicom(self.app_context.borrow().config())
    }

    fn is_zapper_famicom_configured(&self) -> bool {
        Self::is_zapper_famicom(self.app_context.borrow().config())
    }

    fn is_power_pad_famicom_configured(&self) -> bool {
        Self::is_power_pad_famicom(self.app_context.borrow().config())
    }

    /// Check if a Zapper is active on the specified port or expansion port.
    /// This method is primarily used by the WASM frontend.
    #[allow(dead_code)]
    pub fn is_zapper_active(&self, port: u8) -> bool {
        // Check expansion port Zapper
        if self.is_zapper_famicom_configured() {
            return true;
        }

        if !(1..=2).contains(&port) {
            return false;
        }

        matches!(
            self.controllers[(port - 1) as usize]
                .borrow()
                .capture_state(),
            crate::nes::input::ControllerState::Zapper(_)
        )
    }

    #[cfg(test)]
    fn open_bus_value_for_test(&self) -> u8 {
        self.open_bus
    }

    /// Create a snapshot of CPU RAM for save-state (first 2KB is the actual RAM).
    pub fn ram_snapshot(&self) -> Vec<u8> {
        self.cpu_ram.borrow()[..0x800].to_vec()
    }

    /// Restore CPU RAM from a save-state.
    pub fn restore_ram(&mut self, data: &[u8]) {
        let mut ram = self.cpu_ram.borrow_mut();
        let len = data.len().min(0x800);
        ram[..len].copy_from_slice(&data[..len]);
    }

    /// Capture mapper state for save-state.
    pub fn capture_mapper_state(&self) -> MapperState {
        if let Some(ref cartridge_opt) = *self.cartridge.borrow() {
            let cartridge = cartridge_opt.borrow();
            let mapper = cartridge.mapper();
            MapperState {
                mapper_number: mapper.mapper_number(),
                prg_ram: mapper.prg_ram_snapshot(),
                chr_ram: mapper.chr_ram_snapshot(),
                registers: mapper.registers_snapshot(),
            }
        } else {
            MapperState {
                mapper_number: 0,
                prg_ram: vec![],
                chr_ram: vec![],
                registers: vec![],
            }
        }
    }

    /// Restore mapper state from a save-state.
    pub fn restore_mapper_state(&mut self, state: &MapperState) {
        if let Some(ref cartridge_opt) = *self.cartridge.borrow() {
            let mut cartridge = cartridge_opt.borrow_mut();
            let mapper = cartridge.mapper_mut();
            mapper.restore_prg_ram(&state.prg_ram);
            mapper.restore_chr_ram(&state.chr_ram);
            mapper.restore_registers(&state.registers);
            let mirroring = mapper.get_mirroring();
            self.ppu.borrow_mut().set_mirroring(mirroring);
        }
    }

    /// Capture bus state for save-state.
    fn capture_state_inner(&self) -> BusState {
        let port1_state = self.controllers[0].borrow().capture_state();
        let port2_state = self.controllers[1].borrow().capture_state();

        BusState {
            open_bus: self.open_bus,
            oam_dma_page: *self.oam_dma_page.borrow(),
            port1_controller: match port1_state {
                crate::nes::input::ControllerState::Joypad(s) => ControllerStateWrapper::Joypad(s),
                crate::nes::input::ControllerState::SnesAdapter(s) => {
                    ControllerStateWrapper::SnesAdapter(s)
                }
                crate::nes::input::ControllerState::Paddle(s) => {
                    ControllerStateWrapper::Arkanoid(s)
                }
                crate::nes::input::ControllerState::Zapper(s) => ControllerStateWrapper::Zapper(s),
                crate::nes::input::ControllerState::PowerPad(s) => {
                    ControllerStateWrapper::PowerPad(s)
                }
            },
            port2_controller: match port2_state {
                crate::nes::input::ControllerState::Joypad(s) => ControllerStateWrapper::Joypad(s),
                crate::nes::input::ControllerState::SnesAdapter(s) => {
                    ControllerStateWrapper::SnesAdapter(s)
                }
                crate::nes::input::ControllerState::Paddle(s) => {
                    ControllerStateWrapper::Arkanoid(s)
                }
                crate::nes::input::ControllerState::Zapper(s) => ControllerStateWrapper::Zapper(s),
                crate::nes::input::ControllerState::PowerPad(s) => {
                    ControllerStateWrapper::PowerPad(s)
                }
            },
            expansion_arkanoid: if self.is_arkanoid_famicom_configured() {
                Some(self.expansion_arkanoid.borrow().capture_state())
            } else {
                None
            },
            expansion_zapper: if self.is_zapper_famicom_configured() {
                Some(self.expansion_zapper.borrow().capture_state())
            } else {
                None
            },
            expansion_power_pad: if self.is_power_pad_famicom_configured() {
                Some(self.expansion_power_pad.borrow().capture_state())
            } else {
                None
            },
        }
    }

    /// Restore bus state from a save-state.
    fn restore_state_inner(&mut self, state: &BusState) {
        self.open_bus = state.open_bus;
        *self.oam_dma_page.borrow_mut() = state.oam_dma_page;
        self.dma_triggered.replace(false);

        // Restore port 1 controller - replace if type changed
        match &state.port1_controller {
            ControllerStateWrapper::Joypad(s) => {
                let mut controller = Self::build_controller(
                    self.ppu.clone(),
                    self.app_context.clone(),
                    ControllerType::Joypad,
                );
                controller.restore_state(&crate::nes::input::ControllerState::Joypad(s.clone()));
                *self.controllers[0].borrow_mut() = controller;
            }
            ControllerStateWrapper::SnesAdapter(s) => {
                let mut controller = Self::build_controller(
                    self.ppu.clone(),
                    self.app_context.clone(),
                    ControllerType::SnesAdapter,
                );
                controller
                    .restore_state(&crate::nes::input::ControllerState::SnesAdapter(s.clone()));
                *self.controllers[0].borrow_mut() = controller;
            }
            ControllerStateWrapper::Arkanoid(s) => {
                let mut controller = Self::build_controller(
                    self.ppu.clone(),
                    self.app_context.clone(),
                    ControllerType::Arkanoid,
                );
                controller.restore_state(&crate::nes::input::ControllerState::Paddle(s.clone()));
                *self.controllers[0].borrow_mut() = controller;
            }
            ControllerStateWrapper::Zapper(s) => {
                let mut controller = Self::build_controller(
                    self.ppu.clone(),
                    self.app_context.clone(),
                    ControllerType::Zapper,
                );
                controller.restore_state(&crate::nes::input::ControllerState::Zapper(s.clone()));
                *self.controllers[0].borrow_mut() = controller;
            }
            ControllerStateWrapper::PowerPad(s) => {
                let mut controller = Self::build_controller(
                    self.ppu.clone(),
                    self.app_context.clone(),
                    ControllerType::PowerPad,
                );
                controller.restore_state(&crate::nes::input::ControllerState::PowerPad(s.clone()));
                *self.controllers[0].borrow_mut() = controller;
            }
        }

        // Restore port 2 controller - replace if type changed
        match &state.port2_controller {
            ControllerStateWrapper::Joypad(s) => {
                let mut controller = Self::build_controller(
                    self.ppu.clone(),
                    self.app_context.clone(),
                    ControllerType::Joypad,
                );
                controller.restore_state(&crate::nes::input::ControllerState::Joypad(s.clone()));
                *self.controllers[1].borrow_mut() = controller;
            }
            ControllerStateWrapper::SnesAdapter(s) => {
                let mut controller = Self::build_controller(
                    self.ppu.clone(),
                    self.app_context.clone(),
                    ControllerType::SnesAdapter,
                );
                controller
                    .restore_state(&crate::nes::input::ControllerState::SnesAdapter(s.clone()));
                *self.controllers[1].borrow_mut() = controller;
            }
            ControllerStateWrapper::Arkanoid(s) => {
                let mut controller = Self::build_controller(
                    self.ppu.clone(),
                    self.app_context.clone(),
                    ControllerType::Arkanoid,
                );
                controller.restore_state(&crate::nes::input::ControllerState::Paddle(s.clone()));
                *self.controllers[1].borrow_mut() = controller;
            }
            ControllerStateWrapper::Zapper(s) => {
                let mut controller = Self::build_controller(
                    self.ppu.clone(),
                    self.app_context.clone(),
                    ControllerType::Zapper,
                );
                controller.restore_state(&crate::nes::input::ControllerState::Zapper(s.clone()));
                *self.controllers[1].borrow_mut() = controller;
            }
            ControllerStateWrapper::PowerPad(s) => {
                let mut controller = Self::build_controller(
                    self.ppu.clone(),
                    self.app_context.clone(),
                    ControllerType::PowerPad,
                );
                controller.restore_state(&crate::nes::input::ControllerState::PowerPad(s.clone()));
                *self.controllers[1].borrow_mut() = controller;
            }
        }

        // Restore expansion Arkanoid state
        if let Some(ref arkanoid_state) = state.expansion_arkanoid {
            self.expansion_arkanoid
                .borrow_mut()
                .restore_state(arkanoid_state);
        }

        // Restore expansion Zapper state
        if let Some(ref zapper_state) = state.expansion_zapper {
            self.expansion_zapper
                .borrow_mut()
                .restore_state(zapper_state);
        }

        if let Some(ref power_pad_state) = state.expansion_power_pad {
            self.expansion_power_pad
                .borrow_mut()
                .restore_state(power_pad_state);
        }
    }
}

impl Stateful for Bus {
    type State = BusState;

    fn capture_state(&self) -> BusState {
        self.capture_state_inner()
    }

    fn restore_state(&mut self, state: &BusState) {
        self.restore_state_inner(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::console::TimingMode;
    use std::rc::Rc;

    struct TestBusDevice {
        range: std::ops::RangeInclusive<u16>,
        read_value: u8,
        last_write: Rc<RefCell<Option<(u16, u8)>>>,
    }

    fn create_test_base_mapper() -> crate::nes::cartridge::BaseMapper {
        let ctx = crate::nes::cartridge::MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![0; 8192],
            crate::nes::cartridge::NametableLayout::Horizontal,
        );
        crate::nes::cartridge::BaseMapper::new(
            &ctx,
            crate::nes::cartridge::MapperCapabilities::default(),
        )
    }

    struct OamDmaCountingMapper {
        base: crate::nes::cartridge::BaseMapper,
        oam_dma_calls: Rc<RefCell<u32>>,
    }

    impl OamDmaCountingMapper {
        fn new(oam_dma_calls: Rc<RefCell<u32>>) -> Self {
            Self {
                base: create_test_base_mapper(),
                oam_dma_calls,
            }
        }
    }

    impl TestBusDevice {
        fn new(
            range: std::ops::RangeInclusive<u16>,
            read_value: u8,
            last_write: Rc<RefCell<Option<(u16, u8)>>>,
        ) -> Self {
            Self {
                range,
                read_value,
                last_write,
            }
        }
    }

    impl BusDevice for TestBusDevice {
        fn read(&mut self, addr: u16, _open_bus: u8, _is_dummy_read: bool) -> Option<u8> {
            if self.range.contains(&addr) {
                return Some(self.read_value);
            }

            None
        }

        fn write(&mut self, addr: u16, value: u8, _is_dummy_write: bool) -> bool {
            if self.range.contains(&addr) {
                *self.last_write.borrow_mut() = Some((addr, value));
                return true;
            }

            false
        }

        fn address_range(&self) -> std::ops::RangeInclusive<u16> {
            self.range.clone()
        }
    }

    impl crate::nes::cartridge::Mapper for OamDmaCountingMapper {
        fn base(&self) -> &crate::nes::cartridge::BaseMapper {
            &self.base
        }

        fn base_mut(&mut self) -> &mut crate::nes::cartridge::BaseMapper {
            &mut self.base
        }

        fn read_prg(&self, _addr: u16) -> u8 {
            0
        }

        fn write_prg(&mut self, _addr: u16, _value: u8) {}

        fn read_chr(&mut self, _addr: u16) -> u8 {
            0
        }

        fn write_chr(&mut self, _addr: u16, _value: u8) {}

        fn ppu_address_changed(&mut self, _addr: u16) {}

        fn on_oam_dma(&mut self) {
            *self.oam_dma_calls.borrow_mut() += 1;
        }

        fn get_mirroring(&self) -> crate::nes::cartridge::NametableLayout {
            crate::nes::cartridge::NametableLayout::Horizontal
        }
    }

    fn create_mmc1_rom() -> Vec<u8> {
        let prg_rom_banks = 1u8;
        let chr_rom_banks = 0u8; // CHR RAM
        let flags6 = 0x10; // mapper 1, horizontal mirroring
        let flags7 = 0x00;

        let mut rom = vec![
            b'N',
            b'E',
            b'S',
            0x1A,
            prg_rom_banks,
            chr_rom_banks,
            flags6,
            flags7,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ];

        let prg_size = prg_rom_banks as usize * 0x4000;
        rom.extend(std::iter::repeat_n(0, prg_size));
        rom
    }

    fn write_mmc1_register(bus: &mut Bus, addr: u16, value: u8) {
        for i in 0..5 {
            bus.mapper_cpu_cycle();
            bus.mapper_cpu_cycle();
            let bit = (value >> i) & 0x01;
            bus.write(addr, bit, false);
        }
    }

    fn write_mmc1_control(bus: &mut Bus, value: u8) {
        write_mmc1_register(bus, 0x8000, value);
    }

    fn assert_vertical_mirroring(ppu: &mut ppu::Ppu) {
        ppu.write_address(0x20, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x42);

        ppu.write_address(0x28, false);
        ppu.write_address(0x00, false);
        let _ = ppu.read_data();
        assert_eq!(ppu.read_data(), 0x42);
    }

    fn assert_horizontal_mirroring(ppu: &mut ppu::Ppu) {
        ppu.write_address(0x20, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x55);

        ppu.write_address(0x24, false);
        ppu.write_address(0x00, false);
        let _ = ppu.read_data();
        assert_eq!(ppu.read_data(), 0x55);
    }

    #[test]
    fn test_restore_mapper_state_updates_ppu_mirroring() {
        let ppu = Rc::new(RefCell::new(ppu::Ppu::new_for_testing(TimingMode::Ntsc)));
        let apu = Rc::new(RefCell::new(crate::nes::apu::Apu::new()));
        let app_context = Rc::new(RefCell::new(crate::platform::app_context::AppContext::new()));
        let mut bus = Bus::new(ppu.clone(), apu, app_context.clone());

        let rom = create_mmc1_rom();
        let cartridge = Cartridge::load_from_file(&rom, "bus-mmc1-state-test.nes", None)
            .expect("Failed to create MMC1 ROM");
        bus.map_cartridge(cartridge);

        write_mmc1_control(&mut bus, 0x1E); // PRG mode 3, vertical mirroring
        assert_vertical_mirroring(&mut ppu.borrow_mut());
        let saved_state = bus.capture_mapper_state();

        write_mmc1_control(&mut bus, 0x1F); // PRG mode 3, horizontal mirroring
        assert_horizontal_mirroring(&mut ppu.borrow_mut());

        bus.restore_mapper_state(&saved_state);
        assert_vertical_mirroring(&mut ppu.borrow_mut());
    }

    fn create_mmc1_ines_rom_with_vertical_mirroring() -> Vec<u8> {
        // Minimal iNES ROM with mapper 1 (MMC1):
        // - 32KB PRG ROM (2 * 16KB)
        // - 8KB CHR ROM (1 * 8KB)
        // - Flags 6: mapper low nibble=1, vertical mirroring
        let prg_rom_banks = 2u8;
        let chr_rom_banks = 1u8;

        let flags6 = 0x10 | 0x01; // mapper=1 in upper nibble, vertical mirroring
        let flags7 = 0x00;

        let mut rom = vec![
            b'N',
            b'E',
            b'S',
            0x1A,          // iNES header magic
            prg_rom_banks, // PRG ROM size (16KB units)
            chr_rom_banks, // CHR ROM size (8KB units)
            flags6,        // Flags 6
            flags7,        // Flags 7
            0,             // Flags 8 (PRG RAM size)
            0,             // Flags 9
            0,             // Flags 10
            0,
            0,
            0,
            0,
            0, // Reserved
        ];

        rom.extend(vec![0u8; prg_rom_banks as usize * 16 * 1024]);
        rom.extend(vec![0u8; chr_rom_banks as usize * 8 * 1024]);
        rom
    }

    fn create_nrom_rom() -> Vec<u8> {
        let prg_rom_banks = 1u8;
        let chr_rom_banks = 1u8;
        let flags6 = 0x00;
        let flags7 = 0x00;

        let mut rom = vec![
            b'N',
            b'E',
            b'S',
            0x1A,
            prg_rom_banks,
            chr_rom_banks,
            flags6,
            flags7,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ];

        rom.extend(vec![0u8; prg_rom_banks as usize * 16 * 1024]);
        rom.extend(vec![0u8; chr_rom_banks as usize * 8 * 1024]);
        rom
    }

    fn create_test_memory() -> Bus {
        let ppu = Rc::new(RefCell::new(ppu::Ppu::new_for_testing(TimingMode::Ntsc)));
        let apu = Rc::new(RefCell::new(crate::nes::apu::Apu::new()));
        let config = crate::nes::console::Config {
            frontend: crate::platform::config::FrontendConfig {
                ram_init_mode: crate::nes::console::RamInitMode::Zero,
                ..Default::default()
            },
            ..Default::default()
        };
        let app_context = Rc::new(RefCell::new(
            crate::platform::app_context::AppContext::new_with_config(config),
        ));
        Bus::new(ppu, apu, app_context.clone())
    }

    fn create_test_memory_with_four_score_enabled() -> Bus {
        let ppu = Rc::new(RefCell::new(ppu::Ppu::new_for_testing(TimingMode::Ntsc)));
        let apu = Rc::new(RefCell::new(crate::nes::apu::Apu::new()));
        let mut config = crate::nes::console::Config {
            frontend: crate::platform::config::FrontendConfig {
                ram_init_mode: crate::nes::console::RamInitMode::Zero,
                ..Default::default()
            },
            ..Default::default()
        };
        config.nes.four_score_enabled = true;
        let app_context = Rc::new(RefCell::new(
            crate::platform::app_context::AppContext::new_with_config(config),
        ));
        Bus::new(ppu, apu, app_context.clone())
    }

    fn read_24_bits(memory: &mut Bus, addr: u16) -> u32 {
        let mut value = 0u32;
        for bit in 0..24 {
            let sample = memory.read(addr, false) & 0x01;
            value |= (sample as u32) << bit;
        }
        value
    }

    #[test]
    fn test_bus_device_dispatches_reads_and_writes() {
        let mut memory = create_test_memory();
        let last_write = Rc::new(RefCell::new(None));

        memory.devices.insert(
            0,
            Box::new(TestBusDevice::new(
                0x4100..=0x4101,
                0xAB,
                last_write.clone(),
            )),
        );

        assert_eq!(memory.read(0x4100, false), 0xAB);

        let dma = memory.write(0x4101, 0x55, false);
        assert!(!dma);
        assert_eq!(*last_write.borrow(), Some((0x4101, 0x55)));
    }

    #[test]
    fn test_bus_prefers_device_for_joypad_registers() {
        let mut memory = create_test_memory();
        let last_write = Rc::new(RefCell::new(None));

        memory.devices.insert(
            0,
            Box::new(TestBusDevice::new(
                0x4016..=0x4016,
                0xAA,
                last_write.clone(),
            )),
        );

        assert_eq!(memory.read(0x4016, false), 0xAA);

        let dma = memory.write(0x4016, 0x55, false);
        assert!(!dma);
        assert_eq!(*last_write.borrow(), Some((0x4016, 0x55)));
    }

    #[test]
    fn test_four_score_port1_includes_player3_button_state() {
        let mut memory = create_test_memory_with_four_score_enabled();

        // Player 3 A should appear as bit 8 in the 24-bit $4016 stream.
        memory.set_button(3, crate::nes::input::Button::A, true);
        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);

        let bits = read_24_bits(&mut memory, 0x4016);
        assert_eq!(bits & (1 << 8), 1 << 8);
    }

    #[test]
    fn test_four_score_port2_includes_player4_button_state() {
        let mut memory = create_test_memory_with_four_score_enabled();

        // Player 4 B should appear as bit 9 in the 24-bit $4017 stream
        // (B is bit 1 within the P4 byte at offset 8).
        memory.set_button(4, crate::nes::input::Button::B, true);
        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);

        let bits = read_24_bits(&mut memory, 0x4017);
        assert_eq!(bits & (1 << 9), 1 << 9);
    }

    #[test]
    fn test_ram_device_is_registered() {
        let memory = create_test_memory();

        assert!(memory.has_device_for_address(0x0000));
        assert!(memory.has_device_for_address(0x1FFF));
    }

    #[test]
    fn test_ppu_device_is_registered() {
        let memory = create_test_memory();

        assert!(memory.has_device_for_address(0x2002));
    }

    #[test]
    fn test_apu_device_is_registered() {
        let memory = create_test_memory();

        assert!(memory.has_device_for_address(0x4015));
        assert!(memory.has_device_for_address(0x4017));
    }

    #[test]
    fn test_joypad_device_is_registered() {
        let memory = create_test_memory();

        assert!(memory.has_device_for_address(0x4016));
        assert!(memory.has_device_for_address(0x4017));
    }

    #[test]
    fn test_mapper_device_is_registered() {
        let memory = create_test_memory();

        assert!(memory.has_device_for_address(0x5000));
        assert!(memory.has_device_for_address(0x6000));
        assert!(memory.has_device_for_address(0x8000));
    }

    #[test]
    fn test_oam_dma_device_is_registered() {
        let memory = create_test_memory();

        assert!(memory.has_device_for_address(0x4014));
    }

    #[test]
    fn test_open_bus_updates_after_read() {
        let mut memory = create_test_memory();

        memory.write(0x0000, 0x3C, false);
        let value = memory.read(0x0000, false);

        assert_eq!(value, 0x3C);
        assert_eq!(memory.open_bus_value_for_test(), 0x3C);
    }

    #[test]
    fn test_cpu_memory_map_io_and_mapper_ranges() {
        let memory = create_test_memory();

        // $4018-$401F is normally disabled/test mode but is claimed by the APU device.
        assert!(memory.has_device_for_address(0x4018));
        assert!(memory.has_device_for_address(0x401F));

        // Cartridge space begins at $4020.
        assert!(memory.has_device_for_address(0x4020));
    }

    #[test]
    fn test_oam_dma_write_is_dispatched_to_devices() {
        let mut memory = create_test_memory();

        let dma = memory.write(0x4014, 0x22, false);
        assert!(dma);
        assert!(memory.oam_dma_pending());
        assert_eq!(memory.take_oam_dma_page(), Some(0x22));
    }

    #[test]
    fn test_oam_dma_write_notifies_mapper_only_on_real_write() {
        let ppu = Rc::new(RefCell::new(ppu::Ppu::new_for_testing(TimingMode::Ntsc)));
        let apu = Rc::new(RefCell::new(crate::nes::apu::Apu::new()));
        let app_context = Rc::new(RefCell::new(crate::platform::app_context::AppContext::new()));
        let mut memory = Bus::new(ppu, apu, app_context.clone());

        let oam_dma_calls = Rc::new(RefCell::new(0u32));
        let mapper = Box::new(OamDmaCountingMapper::new(oam_dma_calls.clone()));
        let cartridge = Cartridge::from_mapper_for_test(mapper);
        memory.map_cartridge(cartridge);

        memory.write(0x4014, 0x22, false);
        assert_eq!(*oam_dma_calls.borrow(), 1);

        memory.write(0x4014, 0x33, true);
        assert_eq!(*oam_dma_calls.borrow(), 1);
    }

    #[test]
    fn test_unmapped_cartridge_space_returns_open_bus() {
        let mut memory = create_test_memory();

        memory.write(0x0000, 0x3C, false);
        let open_bus = memory.read(0x0000, false);

        assert_eq!(open_bus, 0x3C);
        assert_eq!(memory.read(0x4020, false), open_bus);
    }

    #[test]
    fn test_unmapped_cartridge_space_returns_open_bus_with_mapper() {
        let mut memory = create_test_memory();
        let rom = create_mmc1_rom();
        let cartridge =
            crate::nes::cartridge::Cartridge::load_from_file(&rom, "bus-open-bus-mmc1.nes", None)
                .expect("valid cartridge");
        memory.map_cartridge(cartridge);

        memory.write(0x0000, 0x5A, false);
        let open_bus = memory.read(0x0000, false);

        assert_eq!(open_bus, 0x5A);
        assert_eq!(memory.read(0x4020, false), open_bus);
    }

    #[test]
    fn test_unmapped_cartridge_space_returns_open_bus_with_nrom() {
        let mut memory = create_test_memory();
        let rom = create_nrom_rom();
        let cartridge =
            crate::nes::cartridge::Cartridge::load_from_file(&rom, "bus-open-bus-nrom.nes", None)
                .expect("valid cartridge");
        memory.map_cartridge(cartridge);

        memory.write(0x0000, 0xA5, false);
        let open_bus = memory.read(0x0000, false);

        assert_eq!(open_bus, 0xA5);
        assert_eq!(memory.read(0x4020, false), open_bus);
    }

    #[test]
    fn test_bus_save_state_roundtrip_includes_internal_state() {
        let mut memory = create_test_memory();

        memory.write(0x0000, 0x3C, false);
        memory.write(0x4014, 0x22, false);

        // Test with joypad on port 1
        memory.set_button(1, crate::nes::input::Button::A, true);
        memory.set_button(1, crate::nes::input::Button::Right, true);
        memory.write(0x4016, 0x01, false); // Strobe high
        memory.write(0x4016, 0x00, false); // Strobe low - latches and resets index
        memory.read(0x4016, false); // Read A button
        memory.read(0x4016, false); // Read B button
        // Now button_index is 2

        let expected_open_bus = memory.open_bus_value_for_test();

        let saved_state = memory.capture_state();

        let mut restored = create_test_memory();
        restored.restore_state(&saved_state);

        assert_eq!(restored.open_bus_value_for_test(), expected_open_bus);
        assert!(restored.oam_dma_pending());
        assert_eq!(restored.take_oam_dma_page(), Some(0x22));

        // Port 1 should have Joypad with button_index=2 (strobe/index preserved),
        // but button_states cleared on restore — no buttons remain "pressed".
        // Reading should continue from index 2, all zeros (no physical key held).
        let expected_sequence = [0, 0, 0, 0, 0, 0]; // Select, Start, Up, Down, Left, Right
        for expected in expected_sequence {
            assert_eq!(restored.read(0x4016, false) & 0x01, expected);
        }
    }

    #[test]
    fn test_bus_save_state_roundtrip_with_paddle() {
        let mut memory = create_test_memory();

        memory.set_controller_type(1, ControllerType::Arkanoid);
        memory.set_mouse_x_position(0xA5);
        memory.set_mouse_left_button(true);
        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);

        let saved_state = memory.capture_state();

        let mut restored = create_test_memory();
        restored.restore_state(&saved_state);

        // Port 1 should have Paddle with position 0xA5 and trigger cleared (not restored).
        // bit 4 = position serial, bit 3 = trigger (always 0 after restore).
        // position 0xA5 → inverted 0x5A → bits (MSB first): 0,1,...
        restored.write(0x0000, 0x00, false);
        restored.read(0x0000, false);
        let restored_paddle = [
            restored.read(0x4016, false) & 0x18,
            restored.read(0x4016, false) & 0x18,
        ];
        assert_eq!(restored_paddle, [0x00, 0x10]); // trigger cleared; position bit 7=0, bit 6=1
    }

    #[test]
    fn test_bus_save_state_roundtrip_with_power_pad() {
        let mut memory = create_test_memory();
        memory.set_controller_type(1, ControllerType::PowerPad);
        assert!(memory.set_power_pad_button(1, crate::nes::input::PowerPadButton::One, true));
        assert!(memory.set_power_pad_button(1, crate::nes::input::PowerPadButton::Four, true));
        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);
        memory.read(0x4016, false);
        memory.read(0x4016, false);

        let saved_state = memory.capture_state();

        let mut restored = create_test_memory();
        restored.restore_state(&saved_state);
        restored.write(0x4016, 0x01, false);
        restored.write(0x4016, 0x00, false);

        // All buttons cleared on restore; strobe resets bit_index to 0.
        // With button_states=0: D3/D4 both false for pressed buttons (indices 0-3).
        // D4 is hardwired high (None) for indices 4-7.
        assert_eq!(restored.read(0x4016, false) & 0x18, 0x00); // button 2 on D3(off), button 4 on D4(off)
        assert_eq!(restored.read(0x4016, false) & 0x18, 0x00); // button 1 on D3(off), button 3 on D4(off)
        assert_eq!(restored.read(0x4016, false) & 0x18, 0x00); // button 5 on D3(off), button 12 on D4(off)
        assert_eq!(restored.read(0x4016, false) & 0x18, 0x00); // button 9 on D3(off), button 8 on D4(off)
        assert_eq!(restored.read(0x4016, false) & 0x18, 0x10); // button 6 on D3(off), D4 hardwired high
        assert_eq!(restored.read(0x4016, false) & 0x18, 0x10); // button 10 on D3(off), D4 hardwired high
        assert_eq!(restored.read(0x4016, false) & 0x18, 0x10); // button 11 on D3(off), D4 hardwired high
        assert_eq!(restored.read(0x4016, false) & 0x18, 0x10); // button 7 on D3(off), D4 hardwired high
    }

    #[test]
    fn test_bus_save_state_roundtrip_with_expansion_arkanoid() {
        let ppu = Rc::new(RefCell::new(ppu::Ppu::new_for_testing(TimingMode::Ntsc)));
        let apu = Rc::new(RefCell::new(crate::nes::apu::Apu::new()));
        let config = crate::nes::console::Config {
            frontend: crate::platform::config::FrontendConfig {
                ram_init_mode: crate::nes::console::RamInitMode::Zero,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut config = config;
        config.nes.hardware_mode = HardwareMode::Famicom;
        config.nes.expansion_port = ExpansionPort::ArkanoidFamicom;
        let app_context = Rc::new(RefCell::new(
            crate::platform::app_context::AppContext::new_with_config(config),
        ));
        let memory = Bus::new(ppu, apu, app_context.clone());

        // Set position on the expansion Arkanoid controller
        memory.expansion_arkanoid.borrow_mut().set_position(0xB0);
        memory.expansion_arkanoid.borrow_mut().set_trigger(true);

        let saved_state = memory.capture_state();

        // Verify the save state includes expansion Arkanoid
        assert!(saved_state.expansion_arkanoid.is_some());
        let arkanoid_state = saved_state.expansion_arkanoid.as_ref().unwrap();
        assert_eq!(arkanoid_state.position, 0xB0);
        assert!(arkanoid_state.trigger);

        // Restore and verify
        let mut restored = Bus::new(
            Rc::new(RefCell::new(ppu::Ppu::new_for_testing(TimingMode::Ntsc))),
            Rc::new(RefCell::new(crate::nes::apu::Apu::new())),
            app_context,
        );
        restored.restore_state(&saved_state);

        let restored_state = restored.expansion_arkanoid.borrow().capture_state();
        assert_eq!(restored_state.position, 0xB0);
        // trigger is cleared on restore (physical input state).
        assert!(!restored_state.trigger);
    }

    #[test]
    fn test_bus_save_state_omits_expansion_arkanoid_when_not_configured() {
        let memory = create_test_memory();
        let saved_state = memory.capture_state();
        assert!(saved_state.expansion_arkanoid.is_none());
    }

    #[test]
    fn test_mmc1_runtime_mirroring_change_propagates_to_ppu() {
        // RED: Zelda (MMC1) can change mirroring via MMC1 control register writes.
        // If we only set PPU mirroring once at cartridge load, scrolling across
        // a nametable boundary can show duplicated screens.

        let ppu = Rc::new(RefCell::new(ppu::Ppu::new_for_testing(TimingMode::Ntsc)));
        let apu = Rc::new(RefCell::new(crate::nes::apu::Apu::new()));
        let app_context = Rc::new(RefCell::new(crate::platform::app_context::AppContext::new()));
        let mut mem = Bus::new(ppu.clone(), apu, app_context.clone());

        let cart = Cartridge::load_from_file(
            &create_mmc1_ines_rom_with_vertical_mirroring(),
            "bus-mmc1-mirroring-runtime.nes",
            None,
        )
        .expect("MMC1 test ROM should load");
        mem.map_cartridge(cart);

        // Sanity: initial mirroring is vertical (tables 0 and 2 are mirrored).
        {
            let mut ppu = ppu.borrow_mut();
            ppu.write_address(0x20, false);
            ppu.write_address(0x00, false);
            ppu.write_data(0xAA);
            assert_eq!(ppu.read_nametable_for_debug(0x2000), 0xAA);
            assert_eq!(ppu.read_nametable_for_debug(0x2800), 0xAA);
        }

        // Program MMC1 control register to mirroring=horizontal (control bits 0-1 = 0b11).
        // Load value 0b00011 into $8000-$9FFF via 5 writes (see MMC1 unit tests).
        write_mmc1_control(&mut mem, 0b00011);

        // After switching to horizontal mirroring, tables 0 and 1 are mirrored.
        // Writing to $2800 should NOT affect $2000 anymore.
        {
            let mut ppu = ppu.borrow_mut();
            ppu.write_address(0x20, false);
            ppu.write_address(0x00, false);
            ppu.write_data(0x33);

            ppu.write_address(0x28, false);
            ppu.write_address(0x00, false);
            ppu.write_data(0x44);

            assert_eq!(ppu.read_nametable_for_debug(0x2000), 0x33);
            assert_eq!(ppu.read_nametable_for_debug(0x2400), 0x33);
        }
    }

    #[test]
    fn test_mmc1_wram_disabled_reads_return_open_bus() {
        let mut mem = create_test_memory();

        let cart = Cartridge::load_from_file(
            &create_mmc1_ines_rom_with_vertical_mirroring(),
            "bus-mmc1-wram-disable.nes",
            None,
        )
        .expect("MMC1 test ROM should load");
        mem.map_cartridge(cart);

        // Disable WRAM by setting bit 4 of the PRG bank register via 5 writes to $E000.
        write_mmc1_register(&mut mem, 0xE000, 0b10000);

        // Prime open bus to a known value, then read from disabled WRAM.
        mem.write(0x0000, 0xAB, false);
        assert_eq!(mem.read(0x6000, false), 0xAB);
    }

    #[test]
    fn test_debug_read_uses_open_bus_for_disabled_mmc1_wram() {
        let mut mem = create_test_memory();

        let cart = Cartridge::load_from_file(
            &create_mmc1_ines_rom_with_vertical_mirroring(),
            "bus-mmc1-debug-wram-disable.nes",
            None,
        )
        .expect("MMC1 test ROM should load");
        mem.map_cartridge(cart);

        // Disable WRAM by setting bit 4 of the PRG bank register via 5 writes to $E000.
        write_mmc1_register(&mut mem, 0xE000, 0b10000);

        // Prime bus open-bus value and ensure debugger read uses it.
        mem.write(0x0000, 0x5E, false);
        let open_bus = mem.read(0x0000, false);
        assert_eq!(open_bus, 0x5E);
        assert_eq!(mem.read_cpu_for_debugger(0x6000), open_bus);
    }

    #[test]
    fn test_new_memory_is_initialized() {
        let mut memory = create_test_memory();
        assert_eq!(memory.read(0x0000, false), 0);
        assert_eq!(memory.read(0x1234, false), 0);
        assert_eq!(memory.read(0x3FFF, false), 0);
    }

    #[test]
    fn test_write_and_read_byte() {
        let mut memory = create_test_memory();
        let dma = memory.write(0x1234, 0x42, false);
        assert!(!dma);
        assert_eq!(memory.read(0x1234, false), 0x42);
    }

    #[test]
    fn test_write_u16_little_endian() {
        let mut memory = create_test_memory();
        memory.write_u16(0x1234, 0xABCD);
        assert_eq!(memory.read(0x1234, false), 0xCD); // Low byte
        assert_eq!(memory.read(0x1235, false), 0xAB); // High byte
    }

    #[test]
    fn test_ram_mirror_0800() {
        let mut memory = create_test_memory();
        memory.write(0x0000, 0x42, false);
        assert_eq!(memory.read(0x0800, false), 0x42);
        assert_eq!(memory.read(0x1000, false), 0x42);
        assert_eq!(memory.read(0x1800, false), 0x42);
    }

    #[test]
    fn test_ram_mirror_write_to_mirror() {
        let mut memory = create_test_memory();
        memory.write(0x0800, 0x55, false);
        assert_eq!(memory.read(0x0000, false), 0x55);
        assert_eq!(memory.read(0x1000, false), 0x55);
        assert_eq!(memory.read(0x1800, false), 0x55);
    }

    #[test]
    fn test_ram_mirror_different_addresses() {
        let mut memory = create_test_memory();
        memory.write(0x01FF, 0xAA, false);
        assert_eq!(memory.read(0x09FF, false), 0xAA);
        assert_eq!(memory.read(0x11FF, false), 0xAA);
        assert_eq!(memory.read(0x19FF, false), 0xAA);
    }

    #[test]
    fn test_cartridge_prg_rom_16kb_read() {
        use crate::nes::cartridge::Cartridge;

        let mut memory = create_test_memory();

        // Create a simple 16KB PRG ROM cartridge
        let mut prg_rom = vec![0; 0x4000]; // 16KB
        prg_rom[0] = 0xAA; // First byte
        prg_rom[0x3FFF] = 0xBB; // Last byte of 16KB

        let cartridge = Cartridge::from_parts(
            prg_rom,
            vec![],
            crate::nes::cartridge::NametableLayout::Horizontal,
        );

        memory.map_cartridge(cartridge);

        // Read from $8000 (start of PRG ROM)
        assert_eq!(memory.read(0x8000, false), 0xAA);
        // Read from $BFFF (end of first 16KB)
        assert_eq!(memory.read(0xBFFF, false), 0xBB);
        // Read from $C000 (should mirror to $8000)
        assert_eq!(memory.read(0xC000, false), 0xAA);
        // Read from $FFFF (should mirror to $BFFF)
        assert_eq!(memory.read(0xFFFF, false), 0xBB);
    }

    #[test]
    fn test_cartridge_prg_rom_32kb_read() {
        use crate::nes::cartridge::Cartridge;

        let mut memory = create_test_memory();

        // Create a 32KB PRG ROM cartridge
        let mut prg_rom = vec![0; 0x8000]; // 32KB
        prg_rom[0] = 0xAA; // First byte at $8000
        prg_rom[0x4000] = 0xCC; // First byte at $C000
        prg_rom[0x7FFF] = 0xDD; // Last byte at $FFFF

        let cartridge = Cartridge::from_parts(
            prg_rom,
            vec![],
            crate::nes::cartridge::NametableLayout::Horizontal,
        );

        memory.map_cartridge(cartridge);

        // Read from $8000
        assert_eq!(memory.read(0x8000, false), 0xAA);
        // Read from $C000 (different from $8000 in 32KB ROM)
        assert_eq!(memory.read(0xC000, false), 0xCC);
        // Read from $FFFF
        assert_eq!(memory.read(0xFFFF, false), 0xDD);
    }

    #[test]
    fn test_ram_still_writable_with_cartridge() {
        use crate::nes::cartridge::Cartridge;

        let mut memory = create_test_memory();

        let cartridge = Cartridge::from_parts(
            vec![0; 0x4000],
            vec![],
            crate::nes::cartridge::NametableLayout::Horizontal,
        );

        memory.map_cartridge(cartridge);

        // RAM should still be writable
        memory.write(0x0000, 0x55, false);
        assert_eq!(memory.read(0x0000, false), 0x55);

        // Another RAM location should still be writable
        memory.write(0x0100, 0x66, false);
        assert_eq!(memory.read(0x0100, false), 0x66);
    }

    #[test]
    fn test_write_to_ppudata_writes_to_ppu() {
        let mut memory = create_test_memory();

        // Set PPU address to nametable ($2000)
        memory.write(0x2006, 0x20, false);
        memory.write(0x2006, 0x00, false);

        // Write data to PPUDATA register
        memory.write(0x2007, 0x42, false);

        // Verify the data was written to PPU memory by reading it back
        // Reset PPU address
        memory.write(0x2006, 0x20, false);
        memory.write(0x2006, 0x00, false);

        // Read from PPUDATA (first read returns buffer, second returns actual value)
        memory.read(0x2007, false); // Skip buffered read
        assert_eq!(memory.read(0x2007, false), 0x42);
    }

    #[test]
    fn test_write_to_oamaddr_sets_oam_address() {
        let mut memory = create_test_memory();

        // Write to OAMADDR register (use address 0x40 to avoid attribute byte)
        memory.write(0x2003, 0x40, false);

        // Verify by writing to OAMDATA and checking the address incremented
        memory.write(0x2004, 0xAA, false);
        memory.write(0x2004, 0xBB, false);

        // Reset OAM address and read back
        memory.write(0x2003, 0x40, false);
        assert_eq!(memory.read(0x2004, false), 0xAA);
        assert_eq!(memory.read(0x2004, false), 0xAA); // Reading doesn't increment
    }

    #[test]
    fn test_write_to_oamdata_writes_and_increments() {
        let mut memory = create_test_memory();

        // Set OAM address to 0
        memory.write(0x2003, 0x00, false);

        // Write sequence of values
        memory.write(0x2004, 0x11, false);
        memory.write(0x2004, 0x22, false);
        memory.write(0x2004, 0x33, false);

        // Reset OAM address and read back
        memory.write(0x2003, 0x00, false);
        assert_eq!(memory.read(0x2004, false), 0x11);

        memory.write(0x2003, 0x01, false);
        assert_eq!(memory.read(0x2004, false), 0x22);

        memory.write(0x2003, 0x02, false);
        // Attribute byte: 0x33 with masking = 0x33 & 0xE3 = 0x23
        assert_eq!(memory.read(0x2004, false), 0x23);
    }

    #[test]
    fn test_oamdata_write_wraps_at_256() {
        let mut memory = create_test_memory();

        // Set OAM address to 0xFF
        memory.write(0x2003, 0xFF, false);
        memory.write(0x2004, 0xAA, false);

        // Address should wrap to 0x00
        memory.write(0x2004, 0xBB, false);

        // Verify wrap
        memory.write(0x2003, 0xFF, false);
        assert_eq!(memory.read(0x2004, false), 0xAA);

        memory.write(0x2003, 0x00, false);
        assert_eq!(memory.read(0x2004, false), 0xBB);
    }

    #[test]
    fn test_read_from_oamdata_does_not_increment() {
        let mut memory = create_test_memory();

        // Set OAM address and write data
        memory.write(0x2003, 0x10, false);
        memory.write(0x2004, 0x88, false);

        // Reset address and read multiple times
        memory.write(0x2003, 0x10, false);
        assert_eq!(memory.read(0x2004, false), 0x88);
        assert_eq!(memory.read(0x2004, false), 0x88);
        assert_eq!(memory.read(0x2004, false), 0x88);
    }

    #[test]
    fn test_oam_full_sprite_write() {
        let mut memory = create_test_memory();

        // Write a full sprite (4 bytes) to OAM
        memory.write(0x2003, 0x00, false);
        memory.write(0x2004, 0x10, false); // Y position
        memory.write(0x2004, 0x20, false); // Tile index
        memory.write(0x2004, 0xE3, false); // Attributes (valid value with all implemented bits set)
        memory.write(0x2004, 0x40, false); // X position

        // Read back the sprite data
        memory.write(0x2003, 0x00, false);
        assert_eq!(memory.read(0x2004, false), 0x10);
        memory.write(0x2003, 0x01, false);
        assert_eq!(memory.read(0x2004, false), 0x20);
        memory.write(0x2003, 0x02, false);
        assert_eq!(memory.read(0x2004, false), 0xE3);
        memory.write(0x2003, 0x03, false);
        assert_eq!(memory.read(0x2004, false), 0x40);
    }

    #[test]
    fn test_prg_ram_write_and_read() {
        // Test basic PRG-RAM read/write at $6000-$7FFF
        let mut memory = create_test_memory();

        // Load a simple NROM cartridge with PRG-RAM
        let rom_data = create_nrom_rom_with_prg_ram();
        let cartridge = Cartridge::load_from_file(&rom_data, "bus-prg-ram-rw.nes", None)
            .expect("Failed to create cartridge");
        memory.map_cartridge(cartridge);

        // Write to PRG-RAM
        memory.write(0x6000, 0x42, false);
        memory.write(0x6001, 0x43, false);
        memory.write(0x7FFF, 0xFF, false);

        // Read back from PRG-RAM
        assert_eq!(
            memory.read(0x6000, false),
            0x42,
            "PRG-RAM at $6000 should return written value"
        );
        assert_eq!(
            memory.read(0x6001, false),
            0x43,
            "PRG-RAM at $6001 should return written value"
        );
        assert_eq!(
            memory.read(0x7FFF, false),
            0xFF,
            "PRG-RAM at $7FFF should return written value"
        );
    }

    #[test]
    fn test_prg_ram_persistence() {
        // Test that PRG-RAM persists across multiple reads
        let mut memory = create_test_memory();

        let rom_data = create_nrom_rom_with_prg_ram();
        let cartridge = Cartridge::load_from_file(&rom_data, "bus-prg-ram-persistence.nes", None)
            .expect("Failed to create cartridge");
        memory.map_cartridge(cartridge);

        memory.write(0x6100, 0xAB, false);

        // Multiple reads should return the same value
        assert_eq!(memory.read(0x6100, false), 0xAB);
        assert_eq!(memory.read(0x6100, false), 0xAB);
        assert_eq!(memory.read(0x6100, false), 0xAB);
    }

    #[test]
    fn test_prg_ram_8kb_size() {
        // Test that PRG-RAM is 8KB ($6000-$7FFF = 8192 bytes)
        let mut memory = create_test_memory();

        let rom_data = create_nrom_rom_with_prg_ram();
        let cartridge = Cartridge::load_from_file(&rom_data, "bus-prg-ram-size.nes", None)
            .expect("Failed to create cartridge");
        memory.map_cartridge(cartridge);

        // Write to first and last byte of 8KB range
        memory.write(0x6000, 0x01, false);
        memory.write(0x7FFF, 0xFF, false);

        assert_eq!(memory.read(0x6000, false), 0x01);
        assert_eq!(memory.read(0x7FFF, false), 0xFF);

        // They should be different addresses (not mirrored)
        assert_ne!(memory.read(0x6000, false), memory.read(0x7FFF, false));
    }

    #[test]
    fn test_prg_ram_initialized_to_zero() {
        // Test that PRG-RAM starts with all zeros
        let mut memory = create_test_memory();

        let rom_data = create_nrom_rom_with_prg_ram();
        let cartridge = Cartridge::load_from_file(&rom_data, "bus-prg-ram-zero-init.nes", None)
            .expect("Failed to create cartridge");
        memory.map_cartridge(cartridge);

        // Check various addresses are initialized to 0
        assert_eq!(memory.read(0x6000, false), 0x00);
        assert_eq!(memory.read(0x6100, false), 0x00);
        assert_eq!(memory.read(0x7000, false), 0x00);
        assert_eq!(memory.read(0x7FFF, false), 0x00);
    }

    /// Helper function to create a minimal NROM ROM with PRG-RAM support
    fn create_nrom_rom_with_prg_ram() -> Vec<u8> {
        let mut rom = Vec::new();

        // iNES header
        rom.extend_from_slice(b"NES\x1A"); // Signature
        rom.push(2); // 2 * 16KB PRG ROM
        rom.push(1); // 1 * 8KB CHR ROM
        rom.push(0x02); // Flags 6: Battery-backed PRG-RAM present (bit 1)
        rom.push(0x00); // Flags 7: Mapper 0 (NROM)
        rom.extend_from_slice(&[0; 8]); // Unused padding

        // 32KB PRG ROM (2 * 16KB) - filled with NOPs
        rom.extend_from_slice(&[0xEA; 32768]);

        // 8KB CHR ROM - filled with zeros
        rom.extend_from_slice(&[0x00; 8192]);

        rom
    }

    #[test]
    fn test_read_apu_status_register() {
        // Test reading from $4015 returns APU status
        let mut memory = create_test_memory();

        // Reading $4015 should return the APU status register
        let status = memory.read(0x4015, false);

        // Initially all channels should be disabled, so status should be 0
        // except for bit 5 which returns the current open bus value (0xFF at power-on)
        assert_eq!(status & 0b1101_1111, 0x00); // Mask out bit 5 (open bus)
        assert_eq!(status & 0b0010_0000, 0x20); // Bit 5 should be set from open bus
    }

    #[test]
    fn test_read_apu_status_after_enable() {
        // Test that reading $4015 returns the APU's status
        let mut memory = create_test_memory();

        // Directly configure pulse 1 through the APU to test reading
        {
            let mut apu = memory.apu.borrow_mut();
            apu.write_enable(0b0000_0001); // Enable pulse 1
            // Set length counter to non-zero by writing to register 3
            apu.pulse1_mut()
                .write_length_counter_timer_high(0b1111_1000);
            apu.pulse1_mut().apply_pending_length_reload();
        }

        // Read status through memory controller - pulse 1 bit should be set
        let status = memory.read(0x4015, false);
        assert_eq!(status & 0b0000_0001, 0b0000_0001);
    }

    #[test]
    fn test_apu_status_register_mirrored() {
        // Test that $4015 is not mirrored (only accessible at exact address)
        let mut memory = create_test_memory();

        // Reading exactly $4015 should work
        let status = memory.read(0x4015, false);
        // Bit 5 is open bus, so mask it out
        assert_eq!(status & 0b1101_1111, 0x00);

        // Note: $4015 is not mirrored, so other addresses in APU range
        // should not return the status register
    }

    #[test]
    fn test_write_pulse1_registers() {
        // Test writing to pulse 1 registers ($4000-$4003)
        let mut memory = create_test_memory();

        // Enable pulse 1 first
        memory.write(0x4015, 0b00000001, false);

        // Write to $4000 (control register)
        memory.write(0x4000, 0b10111111, false);

        // Write to $4001 (sweep register)
        memory.write(0x4001, 0b10101010, false);

        // Write to $4002 (timer low)
        memory.write(0x4002, 0xAB, false);

        // Write to $4003 (length/timer high)
        memory.write(0x4003, 0b11111000, false);

        memory
            .apu
            .borrow_mut()
            .pulse1_mut()
            .apply_pending_length_reload();

        // Verify writes reached the APU by checking pulse1 length counter
        let apu = memory.apu.borrow();
        assert!(apu.pulse1().get_length_counter() > 0);
    }

    #[test]
    fn test_write_pulse2_registers() {
        // Test writing to pulse 2 registers ($4004-$4007)
        let mut memory = create_test_memory();

        // Enable pulse 2 first
        memory.write(0x4015, 0b00000010, false);

        // Write to $4004 (control register)
        memory.write(0x4004, 0b11001111, false);

        // Write to $4007 (length/timer high)
        memory.write(0x4007, 0b11110000, false);

        memory
            .apu
            .borrow_mut()
            .pulse2_mut()
            .apply_pending_length_reload();

        // Verify writes reached the APU
        let apu = memory.apu.borrow();
        assert!(apu.pulse2().get_length_counter() > 0);
    }

    #[test]
    fn test_write_triangle_registers() {
        // Test writing to triangle registers ($4008-$400B)
        let mut memory = create_test_memory();

        // Enable triangle first
        memory.write(0x4015, 0b00000100, false);

        // Write to $4008 (linear counter)
        memory.write(0x4008, 0b11111111, false);

        // Write to $400B (length/timer high)
        memory.write(0x400B, 0b11110000, false);

        memory
            .apu
            .borrow_mut()
            .triangle_mut()
            .apply_pending_length_reload();

        // Verify writes reached the APU
        let apu = memory.apu.borrow();
        assert!(apu.triangle().get_length_counter() > 0);
    }

    #[test]
    fn test_triangle_period_sweep_via_bus_no_skipped_steps() {
        let mut memory = create_test_memory();

        // 1) Enable triangle ($4015)
        memory.write(0x4015, 0b0000_0100, false);

        // 2) Disable length counter halt + set linear counter reload to max ($4008)
        // control=0 (no halt), reload=127
        memory.write(0x4008, 0x7F, false);

        // 3) Write $400B to load length counter and set linear reload flag.
        // length index = 1 (upper 5 bits), timer high = 0
        memory.write(0x400B, 0x08, false);
        memory
            .apu
            .borrow_mut()
            .triangle_mut()
            .apply_pending_length_reload();

        // 4) Switch to 5-step mode ($4017) so the delayed-write effect produces an immediate
        // quarter-frame clock. This should quickly reload the linear counter.
        memory.write(0x4017, 0b1000_0000, false);
        for _ in 0..4 {
            memory.apu.borrow_mut().clock();
        }

        // Now sweep timer period low from small -> larger via $400A.
        // Triangle timer clocks every CPU cycle, so the sequencer step interval
        // should be (period + 1) CPU cycles.
        let periods = [0u8, 1, 2, 3, 7, 15, 31, 63];

        for &period in &periods {
            memory.write(0x400A, period, false);

            // Ignore the first observed step after changing period to avoid dependence on the
            // current timer countdown value.
            let prev_pos = memory.apu.borrow().triangle().debug_sequence_position();
            let mut prev_pos = prev_pos;

            // Wait for the first step.
            loop {
                memory.apu.borrow_mut().clock();
                let apu = memory.apu.borrow();
                let pos = apu.triangle().debug_sequence_position();
                if pos != prev_pos {
                    prev_pos = pos;
                    break;
                }
            }

            // Now verify a bunch of subsequent steps have correct spacing and no skips.
            let mut cycles_since_step: u32 = 0;
            let mut steps_seen: u32 = 0;
            while steps_seen < 64 {
                memory.apu.borrow_mut().clock();
                cycles_since_step += 1;

                let apu = memory.apu.borrow();
                let pos = apu.triangle().debug_sequence_position();
                drop(apu);

                if pos != prev_pos {
                    let expected = (prev_pos + 1) % 32;
                    assert_eq!(
                        pos, expected,
                        "period={}: step skipped: prev={}, got={}, expected={}",
                        period, prev_pos, pos, expected
                    );

                    let expected_cycles = period as u32 + 1;
                    assert_eq!(
                        cycles_since_step, expected_cycles,
                        "period={}: step interval mismatch",
                        period
                    );

                    prev_pos = pos;
                    cycles_since_step = 0;
                    steps_seen += 1;
                }
            }
        }
    }

    #[test]
    fn test_write_noise_registers() {
        // Test writing to noise registers ($400C-$400F)
        let mut memory = create_test_memory();

        // Enable noise first
        memory.write(0x4015, 0b00001000, false);

        // Write to $400C (control)
        memory.write(0x400C, 0b00111111, false);

        // Write to $400F (length counter load)
        memory.write(0x400F, 0b11110000, false);

        memory
            .apu
            .borrow_mut()
            .noise_mut()
            .apply_pending_length_reload();

        // Verify writes reached the APU
        let apu = memory.apu.borrow();
        assert!(apu.noise().get_length_counter() > 0);
    }

    #[test]
    fn test_write_dmc_registers() {
        // Test writing to DMC registers ($4010-$4013)
        let mut memory = create_test_memory();

        // Write to $4010 (flags and rate)
        memory.write(0x4010, 0b00001111, false);

        // Write to $4011 (direct load)
        memory.write(0x4011, 0x40, false);

        // Write to $4012 (sample address)
        memory.write(0x4012, 0xC0, false);

        // Write to $4013 (sample length)
        memory.write(0x4013, 0xFF, false);

        // Verify write reached the APU (no panic means success)
    }

    #[test]
    fn test_write_apu_enable_register() {
        // Test writing to $4015 (enable register)
        let mut memory = create_test_memory();

        // Enable pulse 1 and pulse 2
        memory.write(0x4015, 0b00000011, false);

        // Write length counters to make them non-zero
        memory.write(0x4003, 0b11110000, false);
        memory.write(0x4007, 0b11110000, false);

        {
            let mut apu = memory.apu.borrow_mut();
            apu.pulse1_mut().apply_pending_length_reload();
            apu.pulse2_mut().apply_pending_length_reload();
        }

        // Read status to verify both are enabled
        let status = memory.read(0x4015, false);
        assert_eq!(status & 0b00000011, 0b00000011);
    }

    #[test]
    fn test_write_frame_counter_register() {
        // Test writing to $4017 (frame counter)
        let mut memory = create_test_memory();

        // Write to frame counter register - 5-step mode (bit 7 set)
        memory.write(0x4017, 0b10000000, false);

        // The effects of a $4017 write occur after a 3-4 CPU cycle delay.
        // So the mode won't necessarily change immediately.
        assert!(
            !memory.apu.borrow().frame_counter().get_mode(),
            "$4017 write should not take effect immediately"
        );

        // Tick long enough for the delayed write to take effect (max 4 cycles).
        for _ in 0..4 {
            memory.apu.borrow_mut().clock();
        }

        assert!(
            memory.apu.borrow().frame_counter().get_mode(),
            "$4017 write should switch to 5-step mode after the delayed-write window"
        );
    }

    #[test]
    fn test_paddle_on_port_1() {
        // RED: Test that paddle can be configured on port 1 (0x4016)
        let mut memory = create_test_memory();
        // Configure paddle on port 1
        memory.set_controller_type(1, crate::nes::input::ControllerType::Arkanoid);
        memory.set_mouse_x_position(0xA5);
        memory.set_mouse_left_button(true);

        // Strobe the controller
        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);

        // Read paddle data from port 1 - bits 4 and 3
        let paddle_bits1 = memory.read(0x4016, false) & 0x18;
        let paddle_bits2 = memory.read(0x4016, false) & 0x18;

        // Verify paddle data is present
        assert_eq!(paddle_bits1, 0x08);
        assert_eq!(paddle_bits2, 0x18);
    }

    #[test]
    fn test_zapper_on_port_2_reports_trigger_and_light_bits() {
        let mut memory = create_test_memory();

        memory.set_controller_type(2, crate::nes::input::ControllerType::Zapper);
        memory.set_mouse_x_position(0x10);
        memory.set_mouse_y_position(0x20);
        memory.set_mouse_left_button(true);

        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);
        let zapper_bits = memory.read(0x4017, false) & 0x18;
        assert_eq!(zapper_bits, 0x18);
    }

    #[test]
    fn test_paddle_on_port_2() {
        // RED: Test that paddle can be configured on port 2 (0x4017)
        let mut memory = create_test_memory();

        // Configure paddle on port 2
        memory.set_controller_type(2, crate::nes::input::ControllerType::Arkanoid);
        memory.set_mouse_x_position(0xB3);
        memory.set_mouse_left_button(false);

        // Strobe the controller
        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);

        // Read paddle data from port 2 - bits 4 and 3
        let paddle_bits1 = memory.read(0x4017, false) & 0x18;
        let paddle_bits2 = memory.read(0x4017, false) & 0x18;

        // Verify paddle data is present
        // Position 0xB3: inverted = 0x4C = 0b01001100, trigger=false
        // MSB (bit 7) = 0, so first position bit is 0; bit 6 = 1, so second position bit is 1
        // First read:  bit 4=0 (first position bit), bit 3=0 (no trigger) = 0x00
        // Second read: bit 4=1 (second position bit), bit 3=0 (no trigger) = 0x10
        assert_eq!(paddle_bits1, 0x00);
        assert_eq!(paddle_bits2, 0x10);
    }

    #[test]
    fn test_joypad_on_port_1_while_paddle_on_port_2() {
        // RED: Test that joypad on port 1 works while paddle is on port 2
        let mut memory = create_test_memory();

        // Configure joypad on port 1, paddle on port 2
        memory.set_controller_type(1, crate::nes::input::ControllerType::Joypad);
        memory.set_controller_type(2, crate::nes::input::ControllerType::Arkanoid);

        // Set joypad buttons
        memory.set_button(1, crate::nes::input::Button::A, true);
        memory.set_button(1, crate::nes::input::Button::B, true);

        // Strobe the controllers
        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);

        // Read joypad from port 1
        assert_eq!(memory.read(0x4016, false) & 0x01, 1); // A button
        assert_eq!(memory.read(0x4016, false) & 0x01, 1); // B button

        // Verify port 2 returns paddle data
        memory.set_mouse_x_position(0xA5);
        memory.set_mouse_left_button(true); // Set trigger so bit 3 is set
        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);
        let paddle_bits = memory.read(0x4017, false) & 0x18;
        assert!(paddle_bits != 0); // Should have paddle data (at least bit 3)
    }

    #[test]
    fn test_controller_port_config_save_state_roundtrip() {
        // RED: Test that controller port configuration is saved/restored
        let mut memory = create_test_memory();

        // Configure paddle on port 2
        memory.set_controller_type(1, crate::nes::input::ControllerType::Joypad);
        memory.set_controller_type(2, crate::nes::input::ControllerType::Arkanoid);
        memory.set_mouse_x_position(0xC7);
        memory.set_mouse_left_button(true); // Set trigger so bit 3 is set

        // Capture state
        let saved_state = memory.capture_state();

        // Change configuration
        memory.set_controller_type(1, crate::nes::input::ControllerType::Arkanoid);
        memory.set_controller_type(2, crate::nes::input::ControllerType::Joypad);

        // Restore state
        memory.restore_state(&saved_state);

        // Verify port 1 has joypad and port 2 has paddle
        memory.set_button(1, crate::nes::input::Button::A, true);
        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);

        // Port 1 should have joypad data (bit 0)
        let joypad_bit = memory.read(0x4016, false) & 0x01;
        assert_eq!(joypad_bit, 1); // A button pressed on joypad

        // Port 2 should have paddle (Arkanoid) configured — trigger cleared on restore,
        // but position 0xC7 (inverted 0x38) produces bit-4 highs from read 2 onwards.
        // Read 3 times to reach bit 5 of the inverted position (=1 → bit4 set).
        let _ = memory.read(0x4017, false) & 0x18; // bit7 of inverted=0
        let _ = memory.read(0x4017, false) & 0x18; // bit6=0
        let paddle_bit4 = memory.read(0x4017, false) & 0x10; // bit5=1 → bit4 high
        assert_ne!(
            paddle_bit4, 0,
            "Port 2 position data (bit 4) should appear after restore"
        );
    }

    #[test]
    fn test_trainer_loaded_into_ram() {
        // Test that trainer data from ROM is loaded into CPU memory at $7000-$71FF
        // Use a test pattern offset to create distinctive data (arbitrary value to avoid 0x00/0xFF)
        const TEST_OFFSET: u8 = 0x42;

        let mut memory = create_test_memory();

        // Create a ROM with trainer data
        let mut rom = vec![
            b'N', b'E', b'S', 0x1A, // iNES header
            1,    // PRG ROM size (16KB units)
            1,    // CHR ROM size (8KB units)
            0x04, // Flags 6 with trainer bit set (bit 2)
            0,    // Flags 7
            0,    // Flags 8 (PRG RAM size)
            0, 0, 0, 0, 0, 0, 0, // Rest of header
        ];

        // Add 512 bytes of trainer data with a specific pattern
        // byte value = (offset + TEST_OFFSET) with wrapping
        for i in 0..512 {
            rom.push((i as u8).wrapping_add(TEST_OFFSET));
        }

        // Add PRG ROM data
        rom.extend(vec![0xAA; 16 * 1024]);

        // Add CHR ROM data
        rom.extend(vec![0xBB; 8 * 1024]);

        // Create cartridge and map it
        let cartridge =
            crate::nes::cartridge::Cartridge::load_from_file(&rom, "bus-trainer-load.nes", None)
                .unwrap();
        memory.map_cartridge(cartridge);

        // Verify trainer data was loaded into RAM at $7000-$71FF
        for i in 0..512 {
            let addr = 0x7000 + i;
            let expected = (i as u8).wrapping_add(TEST_OFFSET);
            let actual = memory.read(addr, false);
            assert_eq!(
                actual, expected,
                "Trainer data mismatch at ${:04X}: expected ${:02X}, got ${:02X}",
                addr, expected, actual
            );
        }
    }

    #[test]
    fn test_no_trainer_does_not_modify_ram() {
        // Test that when there's no trainer, RAM at $7000-$71FF remains zeroed
        let mut memory = create_test_memory();

        // Create a ROM without trainer data
        let mut rom = vec![
            b'N', b'E', b'S', 0x1A, // iNES header
            1,    // PRG ROM size (16KB units)
            1,    // CHR ROM size (8KB units)
            0x00, // Flags 6 without trainer bit
            0,    // Flags 7
            0,    // Flags 8 (PRG RAM size)
            0, 0, 0, 0, 0, 0, 0, // Rest of header
        ];

        // Add PRG ROM data
        rom.extend(vec![0xAA; 16 * 1024]);

        // Add CHR ROM data
        rom.extend(vec![0xBB; 8 * 1024]);

        // Create cartridge and map it
        let cartridge =
            crate::nes::cartridge::Cartridge::load_from_file(&rom, "bus-no-trainer.nes", None)
                .unwrap();
        memory.map_cartridge(cartridge);

        // Verify RAM at $7000-$71FF remains zero (initial state)
        for i in 0..512 {
            let addr = 0x7000 + i;
            let actual = memory.read(addr, false);
            assert_eq!(
                actual, 0,
                "RAM at ${:04X} should be zero without trainer, got ${:02X}",
                addr, actual
            );
        }
    }

    #[test]
    fn test_mapper17_submapper1_trainer_loaded_at_5d00() {
        // Mapper 17 submapper 1 routes the trainer to $5D00 (scratch RAM)
        let mut memory = create_test_memory();

        // Build a Mapper 17 (iNES mapper byte = 17) ROM with trainer
        // NES 2.0: byte 7 bits 3-2 = 0b10, byte 8 bits 7-4 = submapper = 1
        let mut rom = vec![
            b'N', b'E', b'S', 0x1A, // magic
            1,    // PRG 16KB units
            0,    // no CHR ROM → CHR-RAM
            0x14, // flags6: trainer bit | mapper bits 3-0 = 0x1 (17 & 0x0F)
            0x18, // flags7: NES 2.0 (bits 3-2=0b10) | mapper bits 7-4 = 0x1 (17 >> 4)
            0x10, // flags8: submapper 1 (bits 7-4) | mapper bits 11-8 = 0
            0, 0, 0, 0, 0, 0, 0,
        ];
        for i in 0..512u16 {
            rom.push(i as u8); // trainer bytes
        }
        rom.extend(vec![0xFF; 16 * 1024]); // PRG ROM

        let cartridge = crate::nes::cartridge::Cartridge::load_from_file(
            &rom,
            "bus-m17-sub1-trainer.nes",
            None,
        )
        .unwrap();
        memory.map_cartridge(cartridge);

        // Trainer bytes should be at $5D00–$5EFF
        for i in 0..512u16 {
            let addr = 0x5D00 + i;
            assert_eq!(
                memory.read(addr, false),
                i as u8,
                "trainer byte mismatch at ${:04X}",
                addr
            );
        }
    }

    // ── VS System controller remapping tests ─────────────────────────

    fn create_test_memory_with_vs_system() -> Bus {
        let ppu = Rc::new(RefCell::new(ppu::Ppu::new_for_testing(TimingMode::Ntsc)));
        let apu = Rc::new(RefCell::new(crate::nes::apu::Apu::new()));
        let mut config = crate::nes::console::Config {
            frontend: crate::platform::config::FrontendConfig {
                ram_init_mode: crate::nes::console::RamInitMode::Zero,
                ..Default::default()
            },
            ..Default::default()
        };
        config.nes.expansion_port = ExpansionPort::VsSystem;
        let app_context = Rc::new(RefCell::new(
            crate::platform::app_context::AppContext::new_with_config(config),
        ));
        Bus::new(ppu, apu, app_context)
    }

    fn create_test_memory_with_vs_system_swapped() -> Bus {
        let ppu = Rc::new(RefCell::new(ppu::Ppu::new_for_testing(TimingMode::Ntsc)));
        let apu = Rc::new(RefCell::new(crate::nes::apu::Apu::new()));
        let mut config = crate::nes::console::Config {
            frontend: crate::platform::config::FrontendConfig {
                ram_init_mode: crate::nes::console::RamInitMode::Zero,
                ..Default::default()
            },
            ..Default::default()
        };
        config.nes.expansion_port = ExpansionPort::VsSystem;
        config.nes.vs_controllers_swapped = true;
        let app_context = Rc::new(RefCell::new(
            crate::platform::app_context::AppContext::new_with_config(config),
        ));
        Bus::new(ppu, apu, app_context)
    }

    fn read_joypad_button_from_port(bus: &mut Bus, port_addr: u16, button: Button) -> bool {
        // Strobe controller
        bus.write(0x4016, 0x01, false);
        bus.write(0x4016, 0x00, false);

        // Button order: A(0), B(1), Select(2), Start(3), Up(4), Down(5), Left(6), Right(7)
        for _ in 0..button as u8 {
            bus.read(port_addr, false);
        }
        (bus.read(port_addr, false) & 0x01) != 0
    }

    // --- Start↔Select swap (all VS games) ---

    #[test]
    fn vs_system_swaps_start_to_select_bit_position() {
        let mut bus = create_test_memory_with_vs_system();
        bus.set_button(1, Button::Start, true);

        let select_bit = read_joypad_button_from_port(&mut bus, 0x4016, Button::Select);
        assert!(select_bit, "VS: Start should appear at Select bit position");

        let start_bit = read_joypad_button_from_port(&mut bus, 0x4016, Button::Start);
        assert!(
            !start_bit,
            "VS: Start should NOT appear at Start bit position"
        );
    }

    #[test]
    fn vs_system_swaps_select_to_start_bit_position() {
        let mut bus = create_test_memory_with_vs_system();
        bus.set_button(1, Button::Select, true);

        let start_bit = read_joypad_button_from_port(&mut bus, 0x4016, Button::Start);
        assert!(start_bit, "VS: Select should appear at Start bit position");
    }

    #[test]
    fn vs_system_does_not_swap_other_buttons() {
        let mut bus = create_test_memory_with_vs_system();
        bus.set_button(1, Button::A, true);

        let a_bit = read_joypad_button_from_port(&mut bus, 0x4016, Button::A);
        assert!(
            a_bit,
            "VS: A button should not be affected by Start/Select swap"
        );
    }

    #[test]
    fn vs_system_swaps_start_select_on_port2() {
        let mut bus = create_test_memory_with_vs_system();
        bus.set_button(2, Button::Start, true);

        let select_bit = read_joypad_button_from_port(&mut bus, 0x4017, Button::Select);
        assert!(
            select_bit,
            "VS: P2 Start should appear at Select bit on $4017"
        );
    }

    #[test]
    fn non_vs_system_does_not_swap_start_select() {
        let mut bus = create_test_memory();
        bus.set_button(1, Button::Start, true);

        let start_bit = read_joypad_button_from_port(&mut bus, 0x4016, Button::Start);
        assert!(
            start_bit,
            "Non-VS: Start should appear at Start bit position"
        );
    }

    // --- Port swap (VsSystem4017 / VsSystemSwapped games only) ---

    #[test]
    fn vs_swapped_routes_p1_action_buttons_to_port2() {
        let mut bus = create_test_memory_with_vs_system_swapped();
        bus.set_button(1, Button::A, true);

        let a_on_4017 = read_joypad_button_from_port(&mut bus, 0x4017, Button::A);
        assert!(
            a_on_4017,
            "VS swapped: P1 A should appear on $4017 (left stick)"
        );

        let a_on_4016 = read_joypad_button_from_port(&mut bus, 0x4016, Button::A);
        assert!(!a_on_4016, "VS swapped: P1 A should NOT appear on $4016");
    }

    #[test]
    fn vs_swapped_routes_p2_action_buttons_to_port1() {
        let mut bus = create_test_memory_with_vs_system_swapped();
        bus.set_button(2, Button::A, true);

        let a_on_4016 = read_joypad_button_from_port(&mut bus, 0x4016, Button::A);
        assert!(
            a_on_4016,
            "VS swapped: P2 A should appear on $4016 (right stick)"
        );
    }

    #[test]
    fn vs_swapped_keeps_start_on_original_port() {
        let mut bus = create_test_memory_with_vs_system_swapped();
        bus.set_button(1, Button::Start, true);

        // Start→Select swap puts it at Select bit on $4016 (original port, not swapped)
        let select_on_4016 = read_joypad_button_from_port(&mut bus, 0x4016, Button::Select);
        assert!(
            select_on_4016,
            "VS swapped: P1 Start→Select on $4016 (original port)"
        );

        let select_on_4017 = read_joypad_button_from_port(&mut bus, 0x4017, Button::Select);
        assert!(
            !select_on_4017,
            "VS swapped: P1 Start should NOT appear on $4017"
        );
    }

    #[test]
    fn vs_non_swapped_does_not_swap_ports() {
        let mut bus = create_test_memory_with_vs_system();
        bus.set_button(1, Button::A, true);

        let a_on_4016 = read_joypad_button_from_port(&mut bus, 0x4016, Button::A);
        assert!(a_on_4016, "VS non-swapped: P1 A should stay on $4016");
    }

    #[test]
    fn map_cartridge_sets_vs_ppu_type_on_ppu() {
        use crate::nes::cartridge::VsPpuType;

        let ppu = Rc::new(RefCell::new(ppu::Ppu::new_for_testing(TimingMode::Ntsc)));
        let apu = Rc::new(RefCell::new(crate::nes::apu::Apu::new()));
        let app_context = Rc::new(RefCell::new(crate::platform::app_context::AppContext::new()));
        let mut bus = Bus::new(ppu.clone(), apu, app_context);

        // Given: a cartridge with a VS PPU type
        let mut cartridge = Cartridge::from_parts(
            vec![0u8; 32 * 1024],
            vec![0u8; 8 * 1024],
            NametableLayout::Horizontal,
        );
        cartridge.set_vs_ppu_type_for_test(Some(VsPpuType::Rp2c04_0002));

        // When: mapping the cartridge
        bus.map_cartridge(cartridge);

        // Then: PPU should have the VS PPU type set
        assert_eq!(ppu.borrow().vs_ppu_type(), Some(VsPpuType::Rp2c04_0002));
    }
}
