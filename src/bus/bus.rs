use super::apu_device::ApuDevice;
use super::controller_device::ControllerDevice;
use super::mapper_device::MapperDevice;
use super::oam_dma_device::OamDmaDevice;
use super::ppu_device::PpuDevice;
use super::ram_device::RamDevice;
use crate::apu;
use crate::cartridge::Cartridge;
use crate::console::{BusState, MapperState};
use crate::debugging::log_info;
use crate::input::{Button, Controller, ControllerType, Joypad, Paddle, Zapper};
use crate::ppu;
use crate::trace_mapper;
use std::cell::RefCell;
use std::io;
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::rc::Rc;

pub trait BusDevice {
    fn read(&mut self, addr: u16, open_bus: u8, clock_joypads: bool) -> Option<u8>;
    fn write(&mut self, addr: u16, value: u8, is_dummy_write: bool) -> bool;
    fn address_range(&self) -> RangeInclusive<u16>;
}

/// NES Memory (64KB address space)
pub struct Bus {
    cpu_ram: Rc<RefCell<Vec<u8>>>,
    cartridge: Rc<RefCell<Option<Rc<RefCell<Cartridge>>>>>,
    ppu: Rc<RefCell<ppu::Ppu>>,
    apu: Rc<RefCell<apu::Apu>>,
    oam_dma_page: Rc<RefCell<Option<u8>>>, // Stores the page for pending OAM DMA
    dma_triggered: Rc<RefCell<bool>>,
    controllers: [Rc<RefCell<Box<dyn Controller>>>; 2], // Port 1 and Port 2 controllers
    open_bus: u8, // Last value on the data bus for open bus behavior
    devices: Vec<Box<dyn BusDevice>>,
    mmc5_scroll_log_active: bool,
}

impl Bus {
    /// Create a new memory instance with 64KB of RAM initialized to 0
    pub fn new(ppu: Rc<RefCell<ppu::Ppu>>, apu: Rc<RefCell<apu::Apu>>) -> Self {
        let controllers = [
            Rc::new(RefCell::new(Joypad::new_boxed())),
            Rc::new(RefCell::new(Joypad::new_boxed())),
        ];

        let mut controller = Self {
            cpu_ram: Rc::new(RefCell::new(vec![0; 0x10000])),
            cartridge: Rc::new(RefCell::new(None)),
            ppu,
            apu,
            oam_dma_page: Rc::new(RefCell::new(None)),
            dma_triggered: Rc::new(RefCell::new(false)),
            controllers,
            open_bus: 0xFF, // Initialize to 0xFF (common power-on state)
            devices: Vec::new(),
            mmc5_scroll_log_active: false,
        };

        controller.register_device(Box::new(RamDevice::new(controller.cpu_ram.clone())));
        controller.register_device(Box::new(PpuDevice::new(
            controller.ppu.clone(),
            controller.cartridge.clone(),
        )));
        controller.register_device(Box::new(ControllerDevice::new(
            controller.controllers[0].clone(),
            controller.controllers[1].clone(),
        )));
        controller.register_device(Box::new(ApuDevice::new(controller.apu.clone())));
        controller.register_device(Box::new(OamDmaDevice::new(
            controller.oam_dma_page.clone(),
            controller.dma_triggered.clone(),
        )));
        controller.register_device(Box::new(MapperDevice::new(
            controller.cartridge.clone(),
            controller.ppu.clone(),
        )));

        controller
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
        // Wrap cartridge in Rc<RefCell<>> for shared access between CPU and PPU
        let cartridge_rc = Rc::new(RefCell::new(cartridge));

        // Share cartridge reference with PPU for dynamic CHR access
        let mut ppu = self.ppu.borrow_mut();
        ppu.set_cartridge(cartridge_rc.clone());
        ppu.set_mirroring(cartridge_rc.borrow().mapper().get_mirroring());

        *self.cartridge.borrow_mut() = Some(cartridge_rc);
    }

    /// Reset the cartridge (if present) to its power-on state.
    ///
    /// This resets mapper state but typically preserves PRG-RAM contents.
    pub fn reset_cartridge(&mut self) {
        let Some(cartridge) = self.cartridge.borrow().as_ref().cloned() else {
            return;
        };

        cartridge.borrow_mut().reset();
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

    /// Read a byte from memory
    pub fn read(&mut self, addr: u16) -> u8 {
        self.read_internal(addr, true)
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
                .map(|cart| cart.borrow().mapper().read_prg(addr))
                .unwrap_or(0),
            _ => 0,
        }
    }

    /// Read a byte from memory, optionally clocking joypad shift registers.
    ///
    /// During DMA no-op cycles, the CPU repeats the last read externally. For joypad
    /// registers ($4016/$4017), real hardware does not necessarily clock the controller
    /// shift register on these repeated reads; callers can disable joypad clocking.
    pub fn read_without_joypad_clock(&mut self, addr: u16) -> u8 {
        self.read_internal(addr, false)
    }

    fn read_internal(&mut self, addr: u16, clock_joypads: bool) -> u8 {
        if (0xFFFA..=0xFFFB).contains(&addr)
            && let Some(cartridge) = self.cartridge.borrow().as_ref().cloned()
        {
            cartridge.borrow_mut().mapper_mut().on_irq_vector_read(addr);
        }

        if let Some(value) = self.read_from_devices(addr, clock_joypads) {
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

    fn read_from_devices(&mut self, addr: u16, clock_joypads: bool) -> Option<u8> {
        for device in self.devices.iter_mut() {
            if device.address_range().contains(&addr)
                && let Some(value) = device.read(addr, self.open_bus, clock_joypads)
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
        let value = self.read(addr);
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

        let mapper_number = self
            .cartridge
            .borrow()
            .as_ref()
            .map(|cart| cart.borrow().mapper().mapper_number())
            .unwrap_or(0);

        // TODO Clean up old MMC5 trace code or move them to the mmc5 mapper module
        if addr == 0x5105 && mapper_number == 5 {
            self.mmc5_scroll_log_active = value == 0x55;
        }

        if Self::should_log_mmc5_scroll(addr, value, mapper_number) {
            let ppu = self.ppu.borrow();
            let (t, v, fine_x, w) = ppu.scroll_state();
            trace_mapper!(1; "[mmc5][scroll] $5105=0x55 t=0x{:04X} v=0x{:04X} fine_x={} w={}",
                t, v, fine_x, w
            );
            let _ = (t, v, fine_x, w);
        }

        let wrote = self.write_to_devices(addr, value, is_dummy_write);
        if wrote
            && Self::should_log_mmc5_ppu_scroll_write(
                addr,
                mapper_number,
                self.mmc5_scroll_log_active,
            )
        {
            let ppu = self.ppu.borrow();
            let (t, v, fine_x, w) = ppu.scroll_state();
            trace_mapper!(4; "[mmc5][scroll] ${:04X}={:#04X} t=0x{:04X} v=0x{:04X} fine_x={} w={}",
                addr, value, t, v, fine_x, w
            );
            let _ = (t, v, fine_x, w);
        }

        if wrote {
            if addr == 0x4014
                && !is_dummy_write
                && let Some(cartridge) = self.cartridge.borrow().as_ref().cloned()
            {
                cartridge.borrow_mut().mapper_mut().on_oam_dma();
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

    #[cfg(debug_assertions)]
    fn should_log_mmc5_scroll(addr: u16, value: u8, mapper_number: u8) -> bool {
        mapper_number == 5 && addr == 0x5105 && value == 0x55
    }

    #[cfg(debug_assertions)]
    fn should_log_mmc5_ppu_scroll_write(addr: u16, mapper_number: u8, active: bool) -> bool {
        mapper_number == 5 && active && matches!(addr, 0x2000 | 0x2005 | 0x2006)
    }

    #[cfg(not(debug_assertions))]
    fn should_log_mmc5_scroll(_addr: u16, _value: u8, _mapper_number: u8) -> bool {
        false
    }

    #[cfg(not(debug_assertions))]
    fn should_log_mmc5_ppu_scroll_write(_addr: u16, _mapper_number: u8, _active: bool) -> bool {
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
            let byte = self.read(source_page + i);
            self.ppu.borrow_mut().write_oam_data(byte);
        }
    }

    /// Set button state for a controller
    pub fn set_button(&mut self, port: u8, button: Button, pressed: bool) {
        if !(1..=2).contains(&port) {
            return;
        }
        self.controllers[(port - 1) as usize]
            .borrow_mut()
            .set_button(button, pressed);
    }

    /// Set the controller type for a specific port.
    pub fn set_controller_type(&mut self, port: u8, controller_type: ControllerType) {
        if !(1..=2).contains(&port) {
            return;
        }

        let new_controller: Box<dyn Controller> = match controller_type {
            ControllerType::Joypad => Joypad::new_boxed(),
            ControllerType::Arkanoid => Paddle::new_boxed(),
            ControllerType::Zapper => Zapper::new_boxed(),
        };

        *self.controllers[(port - 1) as usize].borrow_mut() = new_controller;
    }

    /// Update mouse X position for any mouse-emulated controller (0..255).
    pub fn set_mouse_x_position(&mut self, position: u8) {
        for controller in &self.controllers {
            controller.borrow_mut().set_mouse_x_position(position);
        }
    }

    /// Update mouse Y position for any mouse-emulated controller (0..255).
    pub fn set_mouse_y_position(&mut self, position: u8) {
        for controller in &self.controllers {
            controller.borrow_mut().set_mouse_y_position(position);
        }
    }

    /// Update mouse left button state for any mouse-emulated controller.
    pub fn set_mouse_left_button(&mut self, pressed: bool) {
        for controller in &self.controllers {
            controller.borrow_mut().set_mouse_left_button(pressed);
        }
    }

    /// Update light detection state for all controllers based on screen buffer
    pub fn update_light_detection(&mut self, screen_buffer: &crate::ppu::ScreenBuffer) {
        for controller in &self.controllers {
            controller.borrow_mut().update_light_detection(screen_buffer);
        }
    }

    /// Return the input type for a controller port.
    pub fn controller_input_type(&self, port: u8) -> Option<crate::input::ControllerInput> {
        if !(1..=2).contains(&port) {
            return None;
        }

        Some(self.controllers[(port - 1) as usize].borrow().input_type())
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
    pub fn capture_state(&self) -> BusState {
        use crate::console::ControllerStateWrapper;

        let port1_state = self.controllers[0].borrow().capture_state();
        let port2_state = self.controllers[1].borrow().capture_state();

        BusState {
            open_bus: self.open_bus,
            oam_dma_page: *self.oam_dma_page.borrow(),
            port1_controller: match port1_state {
                crate::input::ControllerState::Joypad(s) => ControllerStateWrapper::Joypad(s),
                crate::input::ControllerState::Paddle(s) => ControllerStateWrapper::Arkanoid(s),
                crate::input::ControllerState::Zapper(s) => ControllerStateWrapper::Zapper(s),
            },
            port2_controller: match port2_state {
                crate::input::ControllerState::Joypad(s) => ControllerStateWrapper::Joypad(s),
                crate::input::ControllerState::Paddle(s) => ControllerStateWrapper::Arkanoid(s),
                crate::input::ControllerState::Zapper(s) => ControllerStateWrapper::Zapper(s),
            },
        }
    }

    /// Restore bus state from a save-state.
    pub fn restore_state(&mut self, state: &BusState) {
        use crate::console::ControllerStateWrapper;

        self.open_bus = state.open_bus;
        *self.oam_dma_page.borrow_mut() = state.oam_dma_page;
        self.dma_triggered.replace(false);

        // Restore port 1 controller - replace if type changed
        match &state.port1_controller {
            ControllerStateWrapper::Joypad(s) => {
                let mut controller = Joypad::new_boxed();
                controller.restore_state(&crate::input::ControllerState::Joypad(s.clone()));
                *self.controllers[0].borrow_mut() = controller;
            }
            ControllerStateWrapper::Arkanoid(s) => {
                let mut controller = Paddle::new_boxed();
                controller.restore_state(&crate::input::ControllerState::Paddle(s.clone()));
                *self.controllers[0].borrow_mut() = controller;
            }
            ControllerStateWrapper::Zapper(s) => {
                let mut controller = Zapper::new_boxed();
                controller.restore_state(&crate::input::ControllerState::Zapper(s.clone()));
                *self.controllers[0].borrow_mut() = controller;
            }
        }

        // Restore port 2 controller - replace if type changed
        match &state.port2_controller {
            ControllerStateWrapper::Joypad(s) => {
                let mut controller = Joypad::new_boxed();
                controller.restore_state(&crate::input::ControllerState::Joypad(s.clone()));
                *self.controllers[1].borrow_mut() = controller;
            }
            ControllerStateWrapper::Arkanoid(s) => {
                let mut controller = Paddle::new_boxed();
                controller.restore_state(&crate::input::ControllerState::Paddle(s.clone()));
                *self.controllers[1].borrow_mut() = controller;
            }
            ControllerStateWrapper::Zapper(s) => {
                let mut controller = Zapper::new_boxed();
                controller.restore_state(&crate::input::ControllerState::Zapper(s.clone()));
                *self.controllers[1].borrow_mut() = controller;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::TvSystem;
    use std::rc::Rc;

    struct TestBusDevice {
        range: std::ops::RangeInclusive<u16>,
        read_value: u8,
        last_write: Rc<RefCell<Option<(u16, u8)>>>,
    }

    struct OamDmaCountingMapper {
        oam_dma_calls: Rc<RefCell<u32>>,
    }

    impl OamDmaCountingMapper {
        fn new(oam_dma_calls: Rc<RefCell<u32>>) -> Self {
            Self { oam_dma_calls }
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
        fn read(&mut self, addr: u16, _open_bus: u8, _clock_joypads: bool) -> Option<u8> {
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

    impl crate::cartridge::Mapper for OamDmaCountingMapper {
        fn read_prg(&self, _addr: u16) -> u8 {
            0
        }

        fn write_prg(&mut self, _addr: u16, _value: u8) {}

        fn read_chr(&self, _addr: u16) -> u8 {
            0
        }

        fn write_chr(&mut self, _addr: u16, _value: u8) {}

        fn ppu_address_changed(&mut self, _addr: u16) {}

        fn on_oam_dma(&mut self) {
            *self.oam_dma_calls.borrow_mut() += 1;
        }

        fn get_mirroring(&self) -> crate::cartridge::MirroringMode {
            crate::cartridge::MirroringMode::Horizontal
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

    fn write_mmc1_control(bus: &mut Bus, value: u8) {
        for i in 0..5 {
            let bit = (value >> i) & 0x01;
            bus.write_for_testing(0x8000, bit);
        }
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
    fn test_should_log_mmc5_scroll_on_nametable_change() {
        assert!(Bus::should_log_mmc5_scroll(0x5105, 0x55, 5));
        assert!(!Bus::should_log_mmc5_scroll(0x5105, 0x44, 5));
        assert!(!Bus::should_log_mmc5_scroll(0x5104, 0x55, 5));
        assert!(!Bus::should_log_mmc5_scroll(0x5105, 0x55, 4));
    }

    #[test]
    fn test_should_log_mmc5_ppu_scroll_write() {
        assert!(Bus::should_log_mmc5_ppu_scroll_write(0x2005, 5, true));
        assert!(Bus::should_log_mmc5_ppu_scroll_write(0x2006, 5, true));
        assert!(Bus::should_log_mmc5_ppu_scroll_write(0x2000, 5, true));
        assert!(!Bus::should_log_mmc5_ppu_scroll_write(0x2001, 5, true));
        assert!(!Bus::should_log_mmc5_ppu_scroll_write(0x2005, 4, true));
        assert!(!Bus::should_log_mmc5_ppu_scroll_write(0x2005, 5, false));
    }

    #[test]
    fn test_restore_mapper_state_updates_ppu_mirroring() {
        let ppu = Rc::new(RefCell::new(ppu::Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(apu::Apu::new()));
        let mut bus = Bus::new(ppu.clone(), apu);

        let rom = create_mmc1_rom();
        let cartridge = Cartridge::new(&rom).expect("Failed to create MMC1 ROM");
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
        let ppu = Rc::new(RefCell::new(ppu::Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(apu::Apu::new()));
        Bus::new(ppu, apu)
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

        assert_eq!(memory.read(0x4100), 0xAB);

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

        assert_eq!(memory.read(0x4016), 0xAA);

        let dma = memory.write(0x4016, 0x55, false);
        assert!(!dma);
        assert_eq!(*last_write.borrow(), Some((0x4016, 0x55)));
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
        let value = memory.read(0x0000);

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
        let ppu = Rc::new(RefCell::new(ppu::Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(apu::Apu::new()));
        let mut memory = Bus::new(ppu, apu);

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
        let open_bus = memory.read(0x0000);

        assert_eq!(open_bus, 0x3C);
        assert_eq!(memory.read(0x4020), open_bus);
    }

    #[test]
    fn test_unmapped_cartridge_space_returns_open_bus_with_mapper() {
        let mut memory = create_test_memory();
        let rom = create_mmc1_rom();
        let cartridge = crate::cartridge::Cartridge::new(&rom).expect("valid cartridge");
        memory.map_cartridge(cartridge);

        memory.write(0x0000, 0x5A, false);
        let open_bus = memory.read(0x0000);

        assert_eq!(open_bus, 0x5A);
        assert_eq!(memory.read(0x4020), open_bus);
    }

    #[test]
    fn test_unmapped_cartridge_space_returns_open_bus_with_nrom() {
        let mut memory = create_test_memory();
        let rom = create_nrom_rom();
        let cartridge = crate::cartridge::Cartridge::new(&rom).expect("valid cartridge");
        memory.map_cartridge(cartridge);

        memory.write(0x0000, 0xA5, false);
        let open_bus = memory.read(0x0000);

        assert_eq!(open_bus, 0xA5);
        assert_eq!(memory.read(0x4020), open_bus);
    }

    #[test]
    fn test_bus_save_state_roundtrip_includes_internal_state() {
        let mut memory = create_test_memory();

        memory.write(0x0000, 0x3C, false);
        memory.write(0x4014, 0x22, false);

        // Test with joypad on port 1
        memory.set_button(1, crate::input::Button::A, true);
        memory.set_button(1, crate::input::Button::Right, true);
        memory.write(0x4016, 0x01, false); // Strobe high
        memory.write(0x4016, 0x00, false); // Strobe low - latches and resets index
        memory.read(0x4016); // Read A button
        memory.read(0x4016); // Read B button
        // Now button_index is 2

        let expected_open_bus = memory.open_bus_value_for_test();

        let saved_state = memory.capture_state();

        let mut restored = create_test_memory();
        restored.restore_state(&saved_state);

        assert_eq!(restored.open_bus_value_for_test(), expected_open_bus);
        assert!(restored.oam_dma_pending());
        assert_eq!(restored.take_oam_dma_page(), Some(0x22));

        // Port 1 should have Joypad with buttons A and Right pressed, button_index=2
        // Reading should continue from where we left off
        let expected_sequence = [0, 0, 0, 0, 0, 1]; // Select, Start, Up, Down, Left, Right
        for expected in expected_sequence {
            assert_eq!(restored.read(0x4016) & 0x01, expected);
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
        let expected_paddle = [0x08, 0x18];

        let saved_state = memory.capture_state();

        let mut restored = create_test_memory();
        restored.restore_state(&saved_state);

        // Port 1 should have Paddle with position 0xA5 and trigger=true
        restored.write(0x0000, 0x00, false);
        restored.read(0x0000);
        let restored_paddle = [restored.read(0x4016) & 0x18, restored.read(0x4016) & 0x18];
        assert_eq!(restored_paddle, expected_paddle);
    }

    #[test]
    fn test_mmc1_runtime_mirroring_change_propagates_to_ppu() {
        // RED: Zelda (MMC1) can change mirroring via MMC1 control register writes.
        // If we only set PPU mirroring once at cartridge load, scrolling across
        // a nametable boundary can show duplicated screens.

        let ppu = Rc::new(RefCell::new(ppu::Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(apu::Apu::new()));
        let mut mem = Bus::new(ppu.clone(), apu);

        let cart = Cartridge::new(&create_mmc1_ines_rom_with_vertical_mirroring())
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
        mem.write(0x8000, 0b0000_0001, false);
        mem.write(0x8000, 0b0000_0001, false);
        mem.write(0x8000, 0b0000_0000, false);
        mem.write(0x8000, 0b0000_0000, false);
        mem.write(0x8000, 0b0000_0000, false);

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

        let cart = Cartridge::new(&create_mmc1_ines_rom_with_vertical_mirroring())
            .expect("MMC1 test ROM should load");
        mem.map_cartridge(cart);

        // Disable WRAM by setting bit 4 of the PRG bank register via 5 writes to $E000.
        mem.write(0xE000, 0b0000_0000, false);
        mem.write(0xE000, 0b0000_0000, false);
        mem.write(0xE000, 0b0000_0000, false);
        mem.write(0xE000, 0b0000_0000, false);
        mem.write(0xE000, 0b0000_0001, false);

        // Prime open bus to a known value, then read from disabled WRAM.
        mem.write(0x0000, 0xAB, false);
        assert_eq!(mem.read(0x6000), 0xAB);
    }

    #[test]
    fn test_new_memory_is_initialized() {
        let mut memory = create_test_memory();
        assert_eq!(memory.read(0x0000), 0);
        assert_eq!(memory.read(0x1234), 0);
        assert_eq!(memory.read(0x3FFF), 0);
    }

    #[test]
    fn test_write_and_read_byte() {
        let mut memory = create_test_memory();
        let dma = memory.write(0x1234, 0x42, false);
        assert!(!dma);
        assert_eq!(memory.read(0x1234), 0x42);
    }

    #[test]
    fn test_write_u16_little_endian() {
        let mut memory = create_test_memory();
        memory.write_u16(0x1234, 0xABCD);
        assert_eq!(memory.read(0x1234), 0xCD); // Low byte
        assert_eq!(memory.read(0x1235), 0xAB); // High byte
    }

    #[test]
    fn test_ram_mirror_0800() {
        let mut memory = create_test_memory();
        memory.write(0x0000, 0x42, false);
        assert_eq!(memory.read(0x0800), 0x42);
        assert_eq!(memory.read(0x1000), 0x42);
        assert_eq!(memory.read(0x1800), 0x42);
    }

    #[test]
    fn test_ram_mirror_write_to_mirror() {
        let mut memory = create_test_memory();
        memory.write(0x0800, 0x55, false);
        assert_eq!(memory.read(0x0000), 0x55);
        assert_eq!(memory.read(0x1000), 0x55);
        assert_eq!(memory.read(0x1800), 0x55);
    }

    #[test]
    fn test_ram_mirror_different_addresses() {
        let mut memory = create_test_memory();
        memory.write(0x01FF, 0xAA, false);
        assert_eq!(memory.read(0x09FF), 0xAA);
        assert_eq!(memory.read(0x11FF), 0xAA);
        assert_eq!(memory.read(0x19FF), 0xAA);
    }

    #[test]
    fn test_cartridge_prg_rom_16kb_read() {
        use crate::cartridge::Cartridge;

        let mut memory = create_test_memory();

        // Create a simple 16KB PRG ROM cartridge
        let mut prg_rom = vec![0; 0x4000]; // 16KB
        prg_rom[0] = 0xAA; // First byte
        prg_rom[0x3FFF] = 0xBB; // Last byte of 16KB

        let cartridge =
            Cartridge::from_parts(prg_rom, vec![], crate::cartridge::MirroringMode::Horizontal);

        memory.map_cartridge(cartridge);

        // Read from $8000 (start of PRG ROM)
        assert_eq!(memory.read(0x8000), 0xAA);
        // Read from $BFFF (end of first 16KB)
        assert_eq!(memory.read(0xBFFF), 0xBB);
        // Read from $C000 (should mirror to $8000)
        assert_eq!(memory.read(0xC000), 0xAA);
        // Read from $FFFF (should mirror to $BFFF)
        assert_eq!(memory.read(0xFFFF), 0xBB);
    }

    #[test]
    fn test_cartridge_prg_rom_32kb_read() {
        use crate::cartridge::Cartridge;

        let mut memory = create_test_memory();

        // Create a 32KB PRG ROM cartridge
        let mut prg_rom = vec![0; 0x8000]; // 32KB
        prg_rom[0] = 0xAA; // First byte at $8000
        prg_rom[0x4000] = 0xCC; // First byte at $C000
        prg_rom[0x7FFF] = 0xDD; // Last byte at $FFFF

        let cartridge =
            Cartridge::from_parts(prg_rom, vec![], crate::cartridge::MirroringMode::Horizontal);

        memory.map_cartridge(cartridge);

        // Read from $8000
        assert_eq!(memory.read(0x8000), 0xAA);
        // Read from $C000 (different from $8000 in 32KB ROM)
        assert_eq!(memory.read(0xC000), 0xCC);
        // Read from $FFFF
        assert_eq!(memory.read(0xFFFF), 0xDD);
    }

    #[test]
    fn test_ram_still_writable_with_cartridge() {
        use crate::cartridge::Cartridge;

        let mut memory = create_test_memory();

        let cartridge = Cartridge::from_parts(
            vec![0; 0x4000],
            vec![],
            crate::cartridge::MirroringMode::Horizontal,
        );

        memory.map_cartridge(cartridge);

        // RAM should still be writable
        memory.write(0x0000, 0x55, false);
        assert_eq!(memory.read(0x0000), 0x55);

        // Another RAM location should still be writable
        memory.write(0x0100, 0x66, false);
        assert_eq!(memory.read(0x0100), 0x66);
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
        memory.read(0x2007); // Skip buffered read
        assert_eq!(memory.read(0x2007), 0x42);
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
        assert_eq!(memory.read(0x2004), 0xAA);
        assert_eq!(memory.read(0x2004), 0xAA); // Reading doesn't increment
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
        assert_eq!(memory.read(0x2004), 0x11);

        memory.write(0x2003, 0x01, false);
        assert_eq!(memory.read(0x2004), 0x22);

        memory.write(0x2003, 0x02, false);
        // Attribute byte: 0x33 with masking = 0x33 & 0xE3 = 0x23
        assert_eq!(memory.read(0x2004), 0x23);
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
        assert_eq!(memory.read(0x2004), 0xAA);

        memory.write(0x2003, 0x00, false);
        assert_eq!(memory.read(0x2004), 0xBB);
    }

    #[test]
    fn test_read_from_oamdata_does_not_increment() {
        let mut memory = create_test_memory();

        // Set OAM address and write data
        memory.write(0x2003, 0x10, false);
        memory.write(0x2004, 0x88, false);

        // Reset address and read multiple times
        memory.write(0x2003, 0x10, false);
        assert_eq!(memory.read(0x2004), 0x88);
        assert_eq!(memory.read(0x2004), 0x88);
        assert_eq!(memory.read(0x2004), 0x88);
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
        assert_eq!(memory.read(0x2004), 0x10);
        memory.write(0x2003, 0x01, false);
        assert_eq!(memory.read(0x2004), 0x20);
        memory.write(0x2003, 0x02, false);
        assert_eq!(memory.read(0x2004), 0xE3);
        memory.write(0x2003, 0x03, false);
        assert_eq!(memory.read(0x2004), 0x40);
    }

    #[test]
    fn test_prg_ram_write_and_read() {
        // Test basic PRG-RAM read/write at $6000-$7FFF
        let mut memory = create_test_memory();

        // Load a simple NROM cartridge with PRG-RAM
        let rom_data = create_nrom_rom_with_prg_ram();
        let cartridge = Cartridge::new(&rom_data).expect("Failed to create cartridge");
        memory.map_cartridge(cartridge);

        // Write to PRG-RAM
        memory.write(0x6000, 0x42, false);
        memory.write(0x6001, 0x43, false);
        memory.write(0x7FFF, 0xFF, false);

        // Read back from PRG-RAM
        assert_eq!(
            memory.read(0x6000),
            0x42,
            "PRG-RAM at $6000 should return written value"
        );
        assert_eq!(
            memory.read(0x6001),
            0x43,
            "PRG-RAM at $6001 should return written value"
        );
        assert_eq!(
            memory.read(0x7FFF),
            0xFF,
            "PRG-RAM at $7FFF should return written value"
        );
    }

    #[test]
    fn test_prg_ram_persistence() {
        // Test that PRG-RAM persists across multiple reads
        let mut memory = create_test_memory();

        let rom_data = create_nrom_rom_with_prg_ram();
        let cartridge = Cartridge::new(&rom_data).expect("Failed to create cartridge");
        memory.map_cartridge(cartridge);

        memory.write(0x6100, 0xAB, false);

        // Multiple reads should return the same value
        assert_eq!(memory.read(0x6100), 0xAB);
        assert_eq!(memory.read(0x6100), 0xAB);
        assert_eq!(memory.read(0x6100), 0xAB);
    }

    #[test]
    fn test_prg_ram_8kb_size() {
        // Test that PRG-RAM is 8KB ($6000-$7FFF = 8192 bytes)
        let mut memory = create_test_memory();

        let rom_data = create_nrom_rom_with_prg_ram();
        let cartridge = Cartridge::new(&rom_data).expect("Failed to create cartridge");
        memory.map_cartridge(cartridge);

        // Write to first and last byte of 8KB range
        memory.write(0x6000, 0x01, false);
        memory.write(0x7FFF, 0xFF, false);

        assert_eq!(memory.read(0x6000), 0x01);
        assert_eq!(memory.read(0x7FFF), 0xFF);

        // They should be different addresses (not mirrored)
        assert_ne!(memory.read(0x6000), memory.read(0x7FFF));
    }

    #[test]
    fn test_prg_ram_initialized_to_zero() {
        // Test that PRG-RAM starts with all zeros
        let mut memory = create_test_memory();

        let rom_data = create_nrom_rom_with_prg_ram();
        let cartridge = Cartridge::new(&rom_data).expect("Failed to create cartridge");
        memory.map_cartridge(cartridge);

        // Check various addresses are initialized to 0
        assert_eq!(memory.read(0x6000), 0x00);
        assert_eq!(memory.read(0x6100), 0x00);
        assert_eq!(memory.read(0x7000), 0x00);
        assert_eq!(memory.read(0x7FFF), 0x00);
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
        let status = memory.read(0x4015);

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
        let status = memory.read(0x4015);
        assert_eq!(status & 0b0000_0001, 0b0000_0001);
    }

    #[test]
    fn test_apu_status_register_mirrored() {
        // Test that $4015 is not mirrored (only accessible at exact address)
        let mut memory = create_test_memory();

        // Reading exactly $4015 should work
        let status = memory.read(0x4015);
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
        let status = memory.read(0x4015);
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
        memory.set_controller_type(1, crate::input::ControllerType::Arkanoid);
        memory.set_mouse_x_position(0xA5);
        memory.set_mouse_left_button(true);

        // Strobe the controller
        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);

        // Read paddle data from port 1 - bits 4 and 3
        let paddle_bits1 = memory.read(0x4016) & 0x18;
        let paddle_bits2 = memory.read(0x4016) & 0x18;

        // Verify paddle data is present
        assert_eq!(paddle_bits1, 0x08);
        assert_eq!(paddle_bits2, 0x18);
    }

    #[test]
    fn test_zapper_on_port_2_reports_trigger_and_light_bits() {
        let mut memory = create_test_memory();

        memory.set_controller_type(2, crate::input::ControllerType::Zapper);
        memory.set_mouse_x_position(0x10);
        memory.set_mouse_y_position(0x20);
        memory.set_mouse_left_button(true);

        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);
        let zapper_bits = memory.read(0x4017) & 0x18;
        assert_eq!(zapper_bits, 0x18);
    }

    #[test]
    fn test_zapper_light_detection_with_bright_pixel() {
        let mut memory = create_test_memory();

        memory.set_controller_type(2, crate::input::ControllerType::Zapper);
        memory.set_mouse_x_position(100);
        memory.set_mouse_y_position(100);

        // Create a screen buffer with a bright white pixel at position (100, 100)
        let mut screen_buffer = crate::ppu::ScreenBuffer::new();
        screen_buffer.set_pixel(100, 100, 255, 255, 255);

        // Update light detection
        memory.update_light_detection(&screen_buffer);

        // Read zapper state
        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);
        let zapper_value = memory.read(0x4017);
        
        // Light bit (bit 4) should be 0 when light is detected
        let light_bit = (zapper_value >> 4) & 0x01;
        assert_eq!(light_bit, 0, "Light bit should be 0 (light detected) for bright pixel");
    }

    #[test]
    fn test_zapper_light_detection_with_dark_pixel() {
        let mut memory = create_test_memory();

        memory.set_controller_type(2, crate::input::ControllerType::Zapper);
        memory.set_mouse_x_position(50);
        memory.set_mouse_y_position(50);

        // Create a screen buffer with all black pixels
        let screen_buffer = crate::ppu::ScreenBuffer::new();

        // Update light detection
        memory.update_light_detection(&screen_buffer);

        // Read zapper state
        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);
        let zapper_value = memory.read(0x4017);
        
        // Light bit (bit 4) should be 1 when no light is detected
        let light_bit = (zapper_value >> 4) & 0x01;
        assert_eq!(light_bit, 1, "Light bit should be 1 (no light detected) for dark pixel");
    }

    #[test]
    fn test_zapper_light_detection_with_different_positions() {
        let mut memory = create_test_memory();

        memory.set_controller_type(2, crate::input::ControllerType::Zapper);

        // Create a screen buffer with bright pixels at specific positions
        let mut screen_buffer = crate::ppu::ScreenBuffer::new();
        screen_buffer.set_pixel(10, 10, 255, 255, 255);
        screen_buffer.set_pixel(200, 150, 255, 255, 255);

        // Test position (10, 10) - should detect light
        memory.set_mouse_x_position(10);
        memory.set_mouse_y_position(10);
        memory.update_light_detection(&screen_buffer);
        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);
        let light_bit_1 = (memory.read(0x4017) >> 4) & 0x01;
        assert_eq!(light_bit_1, 0, "Should detect light at (10, 10)");

        // Test position (200, 150) - should detect light
        memory.set_mouse_x_position(200);
        memory.set_mouse_y_position(150);
        memory.update_light_detection(&screen_buffer);
        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);
        let light_bit_2 = (memory.read(0x4017) >> 4) & 0x01;
        assert_eq!(light_bit_2, 0, "Should detect light at (200, 150)");

        // Test position (100, 100) - dark pixel, should not detect light
        memory.set_mouse_x_position(100);
        memory.set_mouse_y_position(100);
        memory.update_light_detection(&screen_buffer);
        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);
        let light_bit_3 = (memory.read(0x4017) >> 4) & 0x01;
        assert_eq!(light_bit_3, 1, "Should not detect light at dark position (100, 100)");
    }

    #[test]
    fn test_paddle_on_port_2() {
        // RED: Test that paddle can be configured on port 2 (0x4017)
        let mut memory = create_test_memory();

        // Configure paddle on port 2
        memory.set_controller_type(2, crate::input::ControllerType::Arkanoid);
        memory.set_mouse_x_position(0xB3);
        memory.set_mouse_left_button(false);

        // Strobe the controller
        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);

        // Read paddle data from port 2 - bits 4 and 3
        let paddle_bits1 = memory.read(0x4017) & 0x18;
        let paddle_bits2 = memory.read(0x4017) & 0x18;

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
        memory.set_controller_type(1, crate::input::ControllerType::Joypad);
        memory.set_controller_type(2, crate::input::ControllerType::Arkanoid);

        // Set joypad buttons
        memory.set_button(1, crate::input::Button::A, true);
        memory.set_button(1, crate::input::Button::B, true);

        // Strobe the controllers
        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);

        // Read joypad from port 1
        assert_eq!(memory.read(0x4016) & 0x01, 1); // A button
        assert_eq!(memory.read(0x4016) & 0x01, 1); // B button

        // Verify port 2 returns paddle data
        memory.set_mouse_x_position(0xA5);
        memory.set_mouse_left_button(true); // Set trigger so bit 3 is set
        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);
        let paddle_bits = memory.read(0x4017) & 0x18;
        assert!(paddle_bits != 0); // Should have paddle data (at least bit 3)
    }

    #[test]
    fn test_controller_port_config_save_state_roundtrip() {
        // RED: Test that controller port configuration is saved/restored
        let mut memory = create_test_memory();

        // Configure paddle on port 2
        memory.set_controller_type(1, crate::input::ControllerType::Joypad);
        memory.set_controller_type(2, crate::input::ControllerType::Arkanoid);
        memory.set_mouse_x_position(0xC7);
        memory.set_mouse_left_button(true); // Set trigger so bit 3 is set

        // Capture state
        let saved_state = memory.capture_state();

        // Change configuration
        memory.set_controller_type(1, crate::input::ControllerType::Arkanoid);
        memory.set_controller_type(2, crate::input::ControllerType::Joypad);

        // Restore state
        memory.restore_state(&saved_state);

        // Verify port 1 has joypad and port 2 has paddle
        memory.set_button(1, crate::input::Button::A, true);
        memory.write(0x4016, 0x01, false);
        memory.write(0x4016, 0x00, false);

        // Port 1 should have joypad data (bit 0)
        let joypad_bit = memory.read(0x4016) & 0x01;
        assert_eq!(joypad_bit, 1); // A button pressed on joypad

        // Port 2 should have paddle data (bits 4 and 3)
        let paddle_bits = memory.read(0x4017) & 0x18;
        assert!(paddle_bits != 0); // Should have paddle data on port 2
    }
}
