use crate::snes::apu::SnesApu;
use crate::snes::bus::SnesBus;
use crate::snes::bus::dma::{DmaABus, DmaController};
use crate::snes::cartridge::Cartridge;
use crate::snes::cartridge::EnhancementChip;
use crate::snes::cartridge::Mapping;
use crate::snes::console::save_state::{SnesBusState, SnesPpuState, SnesRomIdentity, SnesSa1State};
use crate::snes::input::{InputPorts, SnesButton};
use crate::snes::ppu::{DRAM_REFRESH_STOLEN_CLOCKS, Ppu, SnesVideoRegion};
use crate::snes::sa1::{
    self, Sa1ControlRegisters, Sa1Core, Sa1IRam, Sa1MemoryControl, decode_mirror_offset,
};
use crate::trace_apu;
use std::cell::{Cell, RefCell};
use std::fs;
use std::rc::Rc;

const WRAM_SIZE: usize = 128 * 1024;

/// SNES system bus.
///
/// This bus currently implements:
/// - WRAM direct and low-RAM mirror windows
/// - cartridge ROM mapping (LoROM/HiROM/ExHiROM)
/// - battery SRAM windows
/// - open-bus/MDR read semantics
/// - the PPU register file (`$2100-$213F`, plus `$4200`/`$4210`/`$4211`/`$4212`), routed to the
///   owned [`Ppu`]
/// - CPU/MMIO registers needed for early bring-up (`$2180-$2183`, `$4202-$4206`,
///   `$420D`, and `$4300-$437F` register latches)
pub struct SnesSystemBus {
    _cartridge: Cartridge,
    mapping: Mapping,
    rom: Rc<Vec<u8>>,
    sram: Rc<RefCell<Vec<u8>>>,
    wram: Vec<u8>,
    wmadd: Cell<u32>,
    wrmpya: u8,
    wrdiv: u16,
    rddiv: u16,
    rdmpy: u16,
    memsel: u8,
    hdmaen: u8,
    dma: DmaController,
    apu: RefCell<SnesApu>,
    /// The PPU. Wrapped in a `RefCell` because PPU register reads have side effects
    /// (address auto-increment, RDNMI acknowledge) yet the bus read path takes `&self`, and in
    /// an `Rc` so `Sa1Core`'s own `Sa1Bus` can share it read-only for the `$2302-$2305` H/V
    /// counter registers (see `Sa1ControlRegisters::latch_hv_counter`).
    ppu: Rc<RefCell<Ppu>>,
    /// The controller ports and auto-joypad sequencer. Wrapped in a `RefCell`
    /// because manual serial reads (`$4016`/`$4017`) clock the shift register
    /// yet the bus read path takes `&self`.
    input: RefCell<InputPorts>,
    mdr: Cell<u8>,
    ticks: Cell<u64>,
    /// `$2200-$220F` SA-1 control/vector register state, shared with [`Sa1Core`]'s own
    /// `Sa1Bus` so both CPU sides see the same registers. `None` for non-SA-1 cartridges.
    sa1_registers: Option<Rc<RefCell<Sa1ControlRegisters>>>,
    /// SA-1's 2KB I-RAM, shared with `Sa1Bus` so both CPU sides see the same bytes and their
    /// own independent write-protection registers (`$2229`/`$222A`). `None` for non-SA-1
    /// cartridges.
    sa1_iram: Option<Rc<RefCell<Sa1IRam>>>,
    /// `$2220-$2228` SA-1 Super MMC ROM banking and BW-RAM mapping/write-protection registers,
    /// shared with `Sa1Bus`. `None` for non-SA-1 cartridges.
    sa1_memory_control: Option<Rc<RefCell<Sa1MemoryControl>>>,
    /// The second 65816 CPU core for SA-1. `None` for non-SA-1 cartridges.
    sa1_core: Option<Sa1Core>,
}

impl SnesSystemBus {
    pub fn new(cartridge: Cartridge) -> Self {
        Self::new_with_spc_ipl_path(cartridge, None)
    }

    pub fn new_with_spc_ipl_path(cartridge: Cartridge, spc_ipl_path: Option<&str>) -> Self {
        Self::new_with_spc_ipl_path_and_region(cartridge, spc_ipl_path, SnesVideoRegion::Ntsc)
    }

    pub fn new_with_spc_ipl_path_and_region(
        cartridge: Cartridge,
        spc_ipl_path: Option<&str>,
        video_region: SnesVideoRegion,
    ) -> Self {
        let mapping = cartridge.mapping();
        let rom = Rc::new(cartridge.rom().to_vec());
        let sram = Rc::new(RefCell::new(vec![0; cartridge.sram_size()]));
        let spc_ipl = Self::load_spc_ipl_override(spc_ipl_path);
        let ppu = Rc::new(RefCell::new(Ppu::new_with_region(video_region)));
        let (sa1_registers, sa1_iram, sa1_memory_control, sa1_core) =
            if cartridge.enhancement_chip() == Some(EnhancementChip::Sa1) {
                let registers = Rc::new(RefCell::new(Sa1ControlRegisters::new()));
                let iram = Rc::new(RefCell::new(Sa1IRam::new()));
                let memory_control = Rc::new(RefCell::new(Sa1MemoryControl::new()));
                let core = Sa1Core::new(
                    Rc::clone(&registers),
                    Rc::clone(&iram),
                    Rc::clone(&memory_control),
                    Rc::clone(&rom),
                    Rc::clone(&sram),
                    Rc::clone(&ppu),
                );
                (
                    Some(registers),
                    Some(iram),
                    Some(memory_control),
                    Some(core),
                )
            } else {
                (None, None, None, None)
            };
        Self {
            _cartridge: cartridge,
            mapping,
            rom,
            sram,
            wram: vec![0; WRAM_SIZE],
            wmadd: Cell::new(0),
            wrmpya: 0,
            wrdiv: 0,
            rddiv: 0,
            rdmpy: 0,
            memsel: 0,
            hdmaen: 0,
            dma: DmaController::new(),
            apu: RefCell::new(SnesApu::new(spc_ipl)),
            ppu,
            input: RefCell::new(InputPorts::new()),
            mdr: Cell::new(0),
            ticks: Cell::new(0),
            sa1_registers,
            sa1_iram,
            sa1_memory_control,
            sa1_core,
        }
    }

    fn load_spc_ipl_override(path: Option<&str>) -> Option<[u8; 64]> {
        let path = path?;

        let data = match fs::read(path) {
            Ok(data) => data,
            Err(err) => {
                crate::platform::debugging::log_info(format!(
                    "Warning: failed to read SNES SPC IPL file {}: {err}",
                    path
                ));
                return None;
            }
        };

        if data.len() != 64 {
            crate::platform::debugging::log_info(format!(
                "Warning: ignoring SNES SPC IPL file {} due to invalid size {} (expected 64 bytes)",
                path,
                data.len()
            ));
            return None;
        }

        let mut arr = [0u8; 64];
        arr.copy_from_slice(&data);
        Some(arr)
    }

    fn decode_wram_index(addr: u32) -> Option<usize> {
        let addr = addr & 0xFF_FFFF;
        let bank = ((addr >> 16) & 0xFF) as u8;
        let offset = (addr & 0xFFFF) as u16;

        if (0x7E..=0x7F).contains(&bank) {
            return Some((((bank as usize - 0x7E) << 16) | offset as usize) & (WRAM_SIZE - 1));
        }

        if matches!(bank, 0x00..=0x3F | 0x80..=0xBF) && offset <= 0x1FFF {
            return Some(offset as usize);
        }

        None
    }

    fn decode_rom_index(&self, addr: u32) -> Option<usize> {
        let addr = addr & 0xFF_FFFF;
        let bank = ((addr >> 16) & 0xFF) as u8;
        let offset = (addr & 0xFFFF) as u16;

        match self.mapping {
            Mapping::LoRom => {
                if (bank <= 0x7D || bank >= 0x80) && offset >= 0x8000 {
                    let bank_index = (bank & 0x7F) as usize;
                    Some(bank_index * 0x8000 + (offset as usize - 0x8000))
                } else {
                    None
                }
            }
            Mapping::HiRom => {
                if (0xC0..=0xFF).contains(&bank) {
                    Some((bank as usize - 0xC0) * 0x10000 + offset as usize)
                } else if (0x40..=0x7D).contains(&bank) {
                    Some((bank as usize - 0x40) * 0x10000 + offset as usize)
                } else if (matches!(bank, 0x00..=0x3F | 0x80..=0xBF)) && offset >= 0x8000 {
                    Some((bank as usize & 0x3F) * 0x10000 + offset as usize)
                } else {
                    None
                }
            }
            Mapping::ExHiRom => {
                if (0xC0..=0xFF).contains(&bank) {
                    Some((bank as usize - 0xC0) * 0x10000 + offset as usize)
                } else if (0x40..=0x7D).contains(&bank) {
                    Some(0x400000 + (bank as usize - 0x40) * 0x10000 + offset as usize)
                } else if (matches!(bank, 0x00..=0x3F | 0x80..=0xBF)) && offset >= 0x8000 {
                    Some((bank as usize & 0x3F) * 0x10000 + offset as usize)
                } else {
                    None
                }
            }
        }
    }

    fn decode_sram_index(&self, addr: u32) -> Option<usize> {
        let addr = addr & 0xFF_FFFF;
        let bank = ((addr >> 16) & 0xFF) as u8;
        let offset = (addr & 0xFFFF) as u16;

        match self.mapping {
            Mapping::LoRom => {
                if (0x70..=0x7D).contains(&bank) && offset <= 0x7FFF {
                    Some((bank as usize - 0x70) * 0x8000 + offset as usize)
                } else {
                    None
                }
            }
            Mapping::HiRom => {
                if (matches!(bank, 0x20..=0x3F | 0xA0..=0xBF))
                    && (0x6000..=0x7FFF).contains(&offset)
                {
                    Some((bank as usize & 0x1F) * 0x2000 + (offset as usize - 0x6000))
                } else {
                    None
                }
            }
            Mapping::ExHiRom => {
                if (matches!(bank, 0x20..=0x3F | 0xA0..=0xBF))
                    && (0x6000..=0x7FFF).contains(&offset)
                {
                    Some((bank as usize & 0x1F) * 0x2000 + (offset as usize - 0x6000))
                } else {
                    None
                }
            }
        }
    }

    fn decode_system_offset(addr: u32) -> Option<u16> {
        let addr = addr & 0xFF_FFFF;
        let bank = ((addr >> 16) & 0xFF) as u8;
        let offset = (addr & 0xFFFF) as u16;
        if matches!(bank, 0x00..=0x3F | 0x80..=0xBF) {
            Some(offset)
        } else {
            None
        }
    }

    fn is_system_bank(bank: u8) -> bool {
        matches!(bank, 0x00..=0x3F | 0x80..=0xBF)
    }

    fn is_dma_a_bus_mmio(addr: u32) -> bool {
        let bank = ((addr >> 16) & 0xFF) as u8;
        let offset = (addr & 0xFFFF) as u16;
        Self::is_system_bank(bank)
            && (matches!(offset, 0x2100..=0x21FF | 0x4000..=0x41FF | 0x4200..=0x421F)
                || (0x4300..=0x437F).contains(&offset))
    }

    /// SA-1 Super MMC ROM decode, used instead of the generic LoROM/HiROM/ExHiROM decode for
    /// SA-1-chipped cartridges (fullsnes: "The registers do affect both SNES and SA-1
    /// mapping"). `None` for non-SA-1 cartridges.
    fn sa1_rom_index(&self, addr: u32) -> Option<usize> {
        let control = self.sa1_memory_control.as_ref()?.borrow();
        sa1::decode_rom_index(addr, &control)
    }

    /// SA-1 BW-RAM linear offset from the SNES CPU's own perspective: its `$2224` BMAPS block
    /// select for the windowed `$6000-$7FFF` view, or the direct `$40-$4F` banks. `None` for
    /// non-SA-1 cartridges.
    fn sa1_bwram_index(&self, addr: u32) -> Option<usize> {
        let control = self.sa1_memory_control.as_ref()?.borrow();
        if let Some(window_offset) = sa1::decode_bwram_windowed_offset(addr) {
            Some(control.snes_bwram_block() * 0x2000 + window_offset)
        } else {
            sa1::decode_bwram_direct_offset(addr)
        }
    }

    /// SNES-side optional NMI/IRQ vector-override interception: reads of `$00FFEA`/`$00FFEB`
    /// return `$220C`/`$220D` SNV instead of ROM when `$2209` SCNT bit 4 is set; reads of
    /// `$00FFEE`/`$00FFEF` return `$220E`/`$220F` SIV instead when SCNT bit 6 is set (confirmed
    /// against bsnes's `SA1::ROM::readCPU`, since fullsnes documents the switch bits but not
    /// this exact interception mechanism). fullsnes notes real games rarely use this for actual
    /// vector redirection ("used only by Jumpin Derby") -- the absindx RAM protection test
    /// instead repurposes the IRQ pair as a data side-channel: its own IRQ handler, entered via
    /// the ROM's ordinary fixed IRQ vector, deliberately re-reads `$00FFEE` as *data* to recover
    /// a byte SA-1 stashed in SIV (see `Sa1ControlRegisters::snes_irq_vector`'s doc comment).
    /// `None` for non-SA-1 cartridges or when the matching override switch is off.
    fn sa1_snes_vector_override_byte(&self, addr: u32) -> Option<u8> {
        let registers = self.sa1_registers.as_ref()?.borrow();
        let (vector, low_byte) = match addr & 0xFF_FFFF {
            0x00_FFEA if registers.snes_nmi_vector_override_enabled() => {
                (registers.snes_nmi_vector(), true)
            }
            0x00_FFEB if registers.snes_nmi_vector_override_enabled() => {
                (registers.snes_nmi_vector(), false)
            }
            0x00_FFEE if registers.snes_irq_vector_override_enabled() => {
                (registers.snes_irq_vector(), true)
            }
            0x00_FFEF if registers.snes_irq_vector_override_enabled() => {
                (registers.snes_irq_vector(), false)
            }
            _ => return None,
        };
        Some(if low_byte {
            (vector & 0xFF) as u8
        } else {
            (vector >> 8) as u8
        })
    }

    fn dma_read_a_bus_impl(&self, addr: u32, open_bus: u8) -> u8 {
        if Self::is_dma_a_bus_mmio(addr) {
            return open_bus;
        }

        if let Some(byte) = self.sa1_snes_vector_override_byte(addr) {
            return byte;
        }
        if let Some(index) = Self::decode_wram_index(addr) {
            return self.wram[index];
        }
        if let (Some(iram), Some(offset)) = (&self.sa1_iram, decode_mirror_offset(addr)) {
            return iram.borrow().read(offset);
        }
        if self.sa1_memory_control.is_some() {
            if let Some(index) = self.sa1_bwram_index(addr) {
                let sram = self.sram.borrow();
                return if sram.is_empty() {
                    open_bus
                } else {
                    sram[index % sram.len()]
                };
            }
            return self
                .sa1_rom_index(addr)
                .and_then(|index| self.rom.get(index).copied())
                .unwrap_or(open_bus);
        }
        if let Some(index) = self.decode_rom_index(addr) {
            return self.rom.get(index).copied().unwrap_or(open_bus);
        }
        if let Some(index) = self.decode_sram_index(addr) {
            let sram = self.sram.borrow();
            return if sram.is_empty() {
                open_bus
            } else {
                sram[index % sram.len()]
            };
        }
        open_bus
    }

    fn read_for_debugger_impl(&self, addr: u32) -> u8 {
        if let Some(byte) = self.sa1_snes_vector_override_byte(addr) {
            return byte;
        }

        if let Some(index) = Self::decode_wram_index(addr) {
            return self.wram[index];
        }

        if let (Some(iram), Some(offset)) = (&self.sa1_iram, decode_mirror_offset(addr)) {
            return iram.borrow().read(offset);
        }

        if self.sa1_memory_control.is_some() {
            if let Some(index) = self.sa1_bwram_index(addr) {
                let sram = self.sram.borrow();
                return if sram.is_empty() {
                    self.mdr.get()
                } else {
                    sram[index % sram.len()]
                };
            }
            return self
                .sa1_rom_index(addr)
                .and_then(|index| self.rom.get(index).copied())
                .unwrap_or(self.mdr.get());
        }

        if let Some(index) = self.decode_rom_index(addr) {
            return self.rom.get(index).copied().unwrap_or(self.mdr.get());
        }

        if let Some(index) = self.decode_sram_index(addr) {
            let sram = self.sram.borrow();
            return if sram.is_empty() {
                self.mdr.get()
            } else {
                sram[index % sram.len()]
            };
        }

        self.mdr.get()
    }

    fn dma_write_a_bus_impl(&mut self, addr: u32, value: u8) {
        if Self::is_dma_a_bus_mmio(addr) {
            return;
        }

        if let Some(index) = Self::decode_wram_index(addr) {
            self.wram[index] = value;
            return;
        }
        if let (Some(iram), Some(offset)) = (&self.sa1_iram, decode_mirror_offset(addr)) {
            iram.borrow_mut().write_from_snes(offset, value);
            return;
        }
        if self.sa1_memory_control.is_some() {
            if let Some(index) = self.sa1_bwram_index(addr) {
                self.write_sa1_bwram(index, value);
            }
            // ROM is read-only.
            return;
        }
        if let Some(index) = self.decode_sram_index(addr) {
            let mut sram = self.sram.borrow_mut();
            let len = sram.len();
            if len != 0 {
                sram[index % len] = value;
            }
        }
    }

    /// Writes `value` to BW-RAM at linear offset `index`, honoring the shared write-protection
    /// rule (`$2226`/`$2227`/`$2228`; see [`Sa1MemoryControl::is_bwram_write_protected`]).
    fn write_sa1_bwram(&self, index: usize, value: u8) {
        let Some(control) = &self.sa1_memory_control else {
            return;
        };
        let mut sram = self.sram.borrow_mut();
        let len = sram.len();
        if len == 0 {
            return;
        }
        // Protection is checked against the *linear* bus offset, BEFORE physical-size wrapping:
        // BWPA's comparator sits on the address bus (fullsnes: protected bytes are "originated
        // at 400000h"), while wrapping onto a smaller chip happens afterward at the RAM's own
        // address pins. So a mirrored write addressed beyond the protected linear range goes
        // through -- and physically lands on a protected byte via wraparound. Conformance-tested
        // by absindx `SA1RamProtectionTest` TEST ID 50 (BWPA=$09 = exactly its cart's 128KB:
        // the probe write at $420000 must stick for the reported area to come out as $09).
        if control.borrow().is_bwram_write_protected(index) {
            return;
        }
        sram[index % len] = value;
    }

    fn start_dma_transfer(&mut self, mdmaen: u8) {
        let mut dma = std::mem::take(&mut self.dma);
        let (consumed_ticks, dma_open_bus) = dma.start_dma(mdmaen, self, self.mdr.get());

        self.ticks
            .set(self.ticks.get().wrapping_add(consumed_ticks));
        self.mdr.set(dma_open_bus);
        self.dma = dma;
    }

    pub fn hdma_init(&mut self) {
        let mut dma = std::mem::take(&mut self.dma);
        let (consumed_ticks, dma_open_bus) = dma.hdma_init(self.hdmaen, self, self.mdr.get());
        self.ticks
            .set(self.ticks.get().wrapping_add(consumed_ticks));
        self.mdr.set(dma_open_bus);
        self.dma = dma;
    }

    pub fn hdma_do_line(&mut self) {
        let mut dma = std::mem::take(&mut self.dma);
        let (consumed_ticks, dma_open_bus) = dma.hdma_do_line(self.hdmaen, self, self.mdr.get());
        self.ticks
            .set(self.ticks.get().wrapping_add(consumed_ticks));
        self.mdr.set(dma_open_bus);
        self.dma = dma;
    }

    /// Fires the once-per-frame HDMA channel reload and once-per-active-scanline HDMA
    /// transfer at their hardware-timed trigger clocks (see [`Ppu::hdma_init_due`] /
    /// [`Ppu::hdma_transfer_due`]). `hdma_init`/`hdma_do_line` were previously only reachable
    /// from tests -- nothing drove them during real emulation, so HDMA never actually ran.
    fn check_hdma_triggers(&mut self) {
        // Both `_due` checks are read-only; compute them under a single immutable borrow
        // instead of two separate `RefCell` runtime checks on this once-per-tick hot path.
        let (init_due, transfer_due) = {
            let ppu = self.ppu.borrow();
            (ppu.hdma_init_due(), ppu.hdma_transfer_due())
        };
        if init_due {
            self.hdma_init();
        }
        if transfer_due {
            self.hdma_do_line();
        }
    }

    /// Returns the size of the cartridge SRAM in bytes.
    pub fn sram_size(&self) -> usize {
        self._cartridge.sram_size()
    }

    /// Returns whether the cartridge has battery-backed RAM.
    pub fn has_battery(&self) -> bool {
        self._cartridge.has_battery()
    }

    /// Returns a snapshot of the current SRAM contents.
    pub fn sram_snapshot(&self) -> Vec<u8> {
        self.sram.borrow().clone()
    }

    /// Snapshot the PPU's visible framebuffer as packed RGB888.
    pub fn ppu_screen_snapshot(&self) -> Vec<u8> {
        self.ppu.borrow().screen_snapshot_rgb()
    }

    /// Return the active PPU frame dimensions.
    pub fn ppu_screen_dimensions(&self) -> (u32, u32) {
        self.ppu.borrow().frame_dimensions()
    }

    /// Returns and clears the PPU frame-complete flag (set when the PPU enters VBlank).
    pub fn take_ppu_frame_complete(&mut self) -> bool {
        self.ppu.borrow_mut().take_frame_complete()
    }

    /// Set a controller button on the given port (0 = port 1, 1 = port 2).
    pub fn set_controller_button(&mut self, port: u8, button: SnesButton, pressed: bool) {
        self.input.get_mut().set_button(port, button, pressed);
    }

    /// Configure the device plugged into each controller port.
    pub fn configure_controllers(
        &mut self,
        port1: crate::snes::input::SnesControllerType,
        port2: crate::snes::input::SnesControllerType,
    ) {
        self.input.get_mut().configure(port1, port2);
    }

    /// Bulk-set the 8 NES-convention buttons on the given port.
    pub fn set_joypad_button_states(&mut self, port: u8, state: u8) {
        self.input.get_mut().set_joypad_button_states(port, state);
    }

    /// Add relative mouse motion for the given SNES controller port.
    pub fn add_mouse_delta(&mut self, port: u8, dx: i16, dy: i16) {
        self.input.get_mut().add_mouse_delta(port, dx, dy);
    }

    /// Set SNES mouse left button state for the given port.
    pub fn set_mouse_left_button(&mut self, port: u8, pressed: bool) {
        self.input.get_mut().set_mouse_left_button(port, pressed);
    }

    /// Set SNES mouse right button state for the given port.
    pub fn set_mouse_right_button(&mut self, port: u8, pressed: bool) {
        self.input.get_mut().set_mouse_right_button(port, pressed);
    }

    /// Set Super Scope aiming coordinates for the given port.
    pub fn set_superscope_position(&mut self, port: u8, x: i16, y: i16) {
        self.input.get_mut().set_superscope_position(port, x, y);
    }

    /// Set Super Scope trigger button state for the given port.
    pub fn set_superscope_trigger(&mut self, port: u8, pressed: bool) {
        self.input.get_mut().set_superscope_trigger(port, pressed);
    }

    /// Set Super Scope cursor button state for the given port.
    pub fn set_superscope_cursor(&mut self, port: u8, pressed: bool) {
        self.input.get_mut().set_superscope_cursor(port, pressed);
    }

    /// Set Super Scope turbo switch state for the given port.
    pub fn set_superscope_turbo(&mut self, port: u8, pressed: bool) {
        self.input.get_mut().set_superscope_turbo(port, pressed);
    }

    /// Set Super Scope pause button state for the given port.
    pub fn set_superscope_pause(&mut self, port: u8, pressed: bool) {
        self.input.get_mut().set_superscope_pause(port, pressed);
    }

    /// Returns true if any SNES controller port currently hosts a mouse.
    pub fn has_mouse(&self) -> bool {
        self.input.borrow().has_mouse()
    }

    /// Returns true if the given physical SNES port currently hosts a mouse.
    pub fn has_mouse_on_port(&self, port: u8) -> bool {
        self.input.borrow().has_mouse_on_port(port)
    }

    /// Returns true if any SNES controller port currently hosts a Super Scope.
    pub fn has_superscope(&self) -> bool {
        self.input.borrow().has_superscope()
    }

    /// Returns true if the given physical SNES port currently hosts a Super Scope.
    pub fn has_superscope_on_port(&self, port: u8) -> bool {
        self.input.borrow().has_superscope_on_port(port)
    }

    /// Returns true if the given physical SNES port currently hosts a multitap.
    pub fn is_multitap_on_port(&self, port: u8) -> bool {
        self.input.borrow().is_multitap_on_port(port)
    }

    /// Return the 8 NES-convention button states for the given port.
    pub fn joypad_button_states(&self, port: u8) -> u8 {
        self.input.borrow().joypad_button_states(port)
    }

    /// Capture the PPU state for a save-state.
    pub(crate) fn ppu_capture_state(&self) -> SnesPpuState {
        self.ppu.borrow().capture_state()
    }

    /// Restore the PPU state from a save-state.
    pub(crate) fn ppu_restore_state(&mut self, state: &SnesPpuState) -> Result<(), String> {
        self.ppu.borrow_mut().restore_state(state)
    }

    /// Restores SRAM from a byte slice. If the slice is larger than SRAM,
    /// only the first `sram_size()` bytes are used.
    pub fn restore_sram(&mut self, data: &[u8]) {
        let mut sram = self.sram.borrow_mut();
        let len = sram.len().min(data.len());
        if len > 0 {
            sram[..len].copy_from_slice(&data[..len]);
        }
    }

    pub(crate) fn rom_identity(&self) -> SnesRomIdentity {
        SnesRomIdentity {
            mapping: Some(self.mapping),
            crc32: crate::platform::crc32::crc32(&[&self.rom]),
        }
    }

    pub(crate) fn capture_state(&self) -> SnesBusState {
        SnesBusState {
            wram: self.wram.clone(),
            wmadd: self.wmadd.get(),
            wrmpya: self.wrmpya,
            wrdiv: self.wrdiv,
            rddiv: self.rddiv,
            rdmpy: self.rdmpy,
            memsel: self.memsel,
            hdmaen: self.hdmaen,
            dma: self.dma.capture_state(),
            mdr: self.mdr.get(),
            ticks: self.ticks.get(),
            sram: self.sram.borrow().clone(),
            apu: self.apu.borrow().capture_state(),
            input: self.input.borrow().capture_state(),
            sa1: self.capture_sa1_state(),
        }
    }

    /// `None` for non-SA-1 cartridges; see the [`SnesSa1State`] doc comment.
    fn capture_sa1_state(&self) -> Option<SnesSa1State> {
        let registers = self.sa1_registers.as_ref()?.borrow();
        let iram = self
            .sa1_iram
            .as_ref()
            .expect("sa1_iram is constructed alongside sa1_registers")
            .borrow();
        let memory_control = self
            .sa1_memory_control
            .as_ref()
            .expect("sa1_memory_control is constructed alongside sa1_registers")
            .borrow();
        let core = self
            .sa1_core
            .as_ref()
            .expect("sa1_core is constructed alongside sa1_registers");
        Some(SnesSa1State {
            ccnt: registers.ccnt(),
            sie: registers.sie(),
            reset_vector: registers.reset_vector(),
            nmi_vector: registers.nmi_vector(),
            irq_vector: registers.irq_vector(),
            scnt: registers.scnt(),
            cie: registers.cie(),
            snes_nmi_vector: registers.snes_nmi_vector(),
            snes_irq_vector: registers.snes_irq_vector(),
            iram: iram.data().to_vec(),
            iram_snes_write_protect: iram.snes_write_protect_raw(),
            iram_sa1_write_protect: iram.sa1_write_protect_raw(),
            cxb: memory_control.cxb(),
            dxb: memory_control.dxb(),
            exb: memory_control.exb(),
            fxb: memory_control.fxb(),
            bmaps: memory_control.bmaps(),
            bmap: memory_control.bmap(),
            sbwe: memory_control.sbwe(),
            cbwe: memory_control.cbwe(),
            bwpa: memory_control.bwpa(),
            cpu: core.cpu_state(),
            booted: core.booted(),
            master_clock_debt: core.master_clock_debt(),
            sa1_irq_pending: registers.sa1_irq_pending(),
            sa1_nmi_pending: registers.sa1_nmi_pending(),
            snes_irq_pending: registers.snes_irq_pending(),
        })
    }

    pub(crate) fn restore_state(&mut self, state: &SnesBusState) -> Result<(), String> {
        if state.wram.len() != self.wram.len() {
            return Err(format!(
                "WRAM size mismatch (expected {}, found {})",
                self.wram.len(),
                state.wram.len()
            ));
        }
        if state.dma.regs.len() != 0x80 {
            return Err(format!(
                "DMA register state size mismatch (expected 128, found {})",
                state.dma.regs.len()
            ));
        }
        if state.dma.bbus_ports.len() != 0x100 {
            return Err(format!(
                "DMA B-bus state size mismatch (expected 256, found {})",
                state.dma.bbus_ports.len()
            ));
        }
        if state.dma.hdma_do_transfer.len() != 8
            || state.dma.hdma_repeat_mode.len() != 8
            || state.dma.hdma_lines_left.len() != 8
        {
            return Err("DMA HDMA state size mismatch".to_string());
        }
        if state.sram.len() != self.sram.borrow().len() {
            return Err(format!(
                "SRAM size mismatch (expected {}, found {})",
                self.sram.borrow().len(),
                state.sram.len()
            ));
        }

        self.wram.copy_from_slice(&state.wram);
        self.wmadd.set(state.wmadd & 0x1_FFFF);
        self.wrmpya = state.wrmpya;
        self.wrdiv = state.wrdiv;
        self.rddiv = state.rddiv;
        self.rdmpy = state.rdmpy;
        self.memsel = state.memsel & 0x01;
        self.hdmaen = state.hdmaen;
        self.dma.restore_state(&state.dma)?;
        self.mdr.set(state.mdr);
        self.ticks.set(state.ticks);
        self.sram.borrow_mut().copy_from_slice(&state.sram);
        self.apu.get_mut().restore_state(&state.apu)?;
        self.input.get_mut().restore_state(&state.input);
        self.restore_sa1_state(state.sa1.as_ref());
        Ok(())
    }

    /// `None` (e.g. non-SA-1 cartridge, or a save state predating SA-1 support) leaves SA-1 at
    /// its current power-on-reset state rather than erroring -- matches this codebase's general
    /// `#[serde(default)]` backward-compatibility approach elsewhere in save states.
    fn restore_sa1_state(&mut self, state: Option<&SnesSa1State>) {
        let Some(state) = state else { return };
        let (Some(registers), Some(iram), Some(memory_control), Some(core)) = (
            &self.sa1_registers,
            &self.sa1_iram,
            &self.sa1_memory_control,
            &mut self.sa1_core,
        ) else {
            return;
        };
        registers.borrow_mut().restore_raw(
            state.ccnt,
            state.sie,
            state.reset_vector,
            state.nmi_vector,
            state.irq_vector,
            state.scnt,
            state.cie,
            state.snes_nmi_vector,
            state.snes_irq_vector,
            state.sa1_irq_pending,
            state.sa1_nmi_pending,
            state.snes_irq_pending,
        );
        iram.borrow_mut().restore_raw(
            &state.iram,
            state.iram_snes_write_protect,
            state.iram_sa1_write_protect,
        );
        memory_control.borrow_mut().restore_raw(
            state.cxb,
            state.dxb,
            state.exb,
            state.fxb,
            state.bmaps,
            state.bmap,
            state.sbwe,
            state.cbwe,
            state.bwpa,
        );
        core.restore_cpu_state(&state.cpu);
        core.set_booted(state.booted);
        core.set_master_clock_debt(state.master_clock_debt);
    }

    pub(crate) fn sample_ready(&self) -> bool {
        self.apu.borrow().sample_ready()
    }

    pub(crate) fn take_stereo_sample(&mut self) -> Option<(f32, f32)> {
        self.apu.get_mut().take_stereo_sample()
    }

    pub(crate) fn take_sample(&mut self) -> Option<f32> {
        self.apu.get_mut().take_sample()
    }

    pub(crate) fn set_audio_sample_rate(&mut self, rate: f32) {
        self.apu.get_mut().set_sample_rate(rate);
    }

    #[cfg(test)]
    fn apu_read_spc_port_for_test(&self, port: usize) -> u8 {
        self.apu.borrow().read_spc_port(port)
    }

    #[cfg(test)]
    fn apu_write_spc_port_for_test(&mut self, port: usize, value: u8) {
        self.apu.get_mut().write_spc_port(port, value);
    }

    #[cfg(test)]
    fn apu_read_spc_memory_for_test(&mut self, addr: u16) -> u8 {
        self.apu.get_mut().read_spc_memory_for_test(addr)
    }

    /// Returns the SA-1 CPU's program counter, or `None` for non-SA-1 cartridges.
    #[cfg(test)]
    pub(crate) fn sa1_cpu_pc_for_tests(&self) -> Option<u16> {
        self.sa1_core.as_ref().map(|core| core.cpu().read_pc())
    }

    /// Returns the SA-1 CPU's accumulator, or `None` for non-SA-1 cartridges.
    #[cfg(test)]
    pub(crate) fn sa1_cpu_a_for_tests(&self) -> Option<u16> {
        self.sa1_core.as_ref().map(|core| core.cpu().read_a())
    }

    /// Simulates the SA-1 CPU itself writing one of its side-writable registers (e.g. `$2209`
    /// SCNT, `$220A` CIE, `$220B` CIC), without needing a running fixture program. No-op for
    /// non-SA-1 cartridges.
    #[cfg(test)]
    pub(crate) fn write_sa1_side_register_for_tests(&self, port: u16, value: u8) {
        if let Some(registers) = &self.sa1_registers {
            registers.borrow_mut().write(port, value);
        }
    }

    fn read_mmio(&self, addr: u32) -> Option<u8> {
        let offset = Self::decode_system_offset(addr)?;
        let open_bus = self.mdr.get();
        let value = match offset {
            0x2180 => {
                let wmadd = self.wmadd.get() & 0x1_FFFF;
                let value = self.wram[(wmadd as usize) & (WRAM_SIZE - 1)];
                self.wmadd.set((wmadd + 1) & 0x1_FFFF);
                value
            }
            0x2181 => (self.wmadd.get() & 0xFF) as u8,
            0x2182 => ((self.wmadd.get() >> 8) & 0xFF) as u8,
            0x2183 => ((self.wmadd.get() >> 16) & 0x01) as u8,
            0x4214 => (self.rddiv & 0x00FF) as u8,
            0x4215 => (self.rddiv >> 8) as u8,
            0x4216 => (self.rdmpy & 0x00FF) as u8,
            0x4217 => (self.rdmpy >> 8) as u8,
            0x420D => self.memsel,
            0x420C => self.hdmaen,
            0x2140..=0x2143 => {
                let port = (offset - 0x2140) as usize;
                let value = self.apu.borrow().read_main_port(port);
                trace_apu!(3; "CPU reads port[{}] -> ${:02X}", port, value);
                value
            }
            // SLHV: the strobe doesn't drive the PPU1/PPU2 data bus at all, so the CPU sees
            // whatever was already on the bus (real open bus) rather than a fresh value --
            // `read_register` still runs for its H/V-latch side effect, but its return value
            // is discarded here in favor of the bus's own `open_bus`.
            0x2137 => {
                self.ppu.borrow_mut().read_register(offset);
                open_bus
            }
            0x2134..=0x213F => self.ppu.borrow_mut().read_register(offset),
            // HVBJOY: bit 0 reports auto-joypad busy, owned by the input ports.
            0x4212 => {
                let raw = self.ppu.borrow_mut().read_register(offset);
                (raw & !0x01) | (self.input.borrow().auto_busy() as u8)
            }
            0x4210 | 0x4211 => self.ppu.borrow_mut().read_register(offset),
            0x4016 => self.input.borrow_mut().read_joya(open_bus),
            0x4017 => self.input.borrow_mut().read_joyb(open_bus),
            0x4218..=0x421F => self.input.borrow().read_joy_register(offset)?,
            // $2300 SFR: SNES-side status flag read (message from SA-1, vector-override
            // switches, IRQ-from-SA-1 pending). `None` for non-SA-1 cartridges, matching every
            // other SA-1 register's fall-through-to-open-bus behavior.
            0x2300 => self.sa1_registers.as_ref()?.borrow().sfr(),
            0x4300..=0x437F => self.dma.read_register(offset)?,
            _ => return None,
        };
        Some(value)
    }

    fn write_mmio(&mut self, addr: u32, value: u8) -> bool {
        let Some(offset) = Self::decode_system_offset(addr) else {
            return false;
        };

        match offset {
            0x2180 => {
                let wmadd = self.wmadd.get() & 0x1_FFFF;
                let index = (wmadd as usize) & (WRAM_SIZE - 1);
                self.wram[index] = value;
                self.wmadd.set((wmadd + 1) & 0x1_FFFF);
                true
            }
            0x2181 => {
                let wmadd = self.wmadd.get();
                self.wmadd.set((wmadd & !0x0000_00FF) | value as u32);
                true
            }
            0x2182 => {
                let wmadd = self.wmadd.get();
                self.wmadd
                    .set((wmadd & !0x0000_FF00) | ((value as u32) << 8));
                true
            }
            0x2183 => {
                let wmadd = self.wmadd.get();
                self.wmadd
                    .set((wmadd & !0x0001_0000) | (((value & 0x01) as u32) << 16));
                true
            }
            0x4202 => {
                self.wrmpya = value;
                true
            }
            0x4203 => {
                self.rdmpy = (self.wrmpya as u16).wrapping_mul(value as u16);
                true
            }
            0x4204 => {
                self.wrdiv = (self.wrdiv & 0xFF00) | value as u16;
                true
            }
            0x4205 => {
                self.wrdiv = (self.wrdiv & 0x00FF) | ((value as u16) << 8);
                true
            }
            0x4206 => {
                let dividend = self.wrdiv;
                if value == 0 {
                    self.rddiv = 0xFFFF;
                    self.rdmpy = dividend;
                } else {
                    self.rddiv = dividend / value as u16;
                    self.rdmpy = dividend % value as u16;
                }
                true
            }
            0x420D => {
                self.memsel = value & 0x01;
                true
            }
            0x420C => {
                self.hdmaen = value;
                // No special handling - HDMA logic will pick up changes naturally
                true
            }
            0x2140..=0x2143 => {
                trace_apu!(2; "CPU writes port[{}] = ${:02X}", offset - 0x2140, value);
                self.apu
                    .borrow_mut()
                    .write_main_port((offset - 0x2140) as usize, value);
                true
            }
            0x420B => {
                self.start_dma_transfer(value);
                true
            }
            0x2100..=0x213F => {
                self.ppu.borrow_mut().write_register(offset, value);
                true
            }
            0x4016 => {
                self.input.get_mut().write_joywr(value);
                true
            }
            0x4200 => {
                self.ppu.borrow_mut().write_register(offset, value);
                // NMITIMEN bit 0: auto-joypad enable.
                self.input.get_mut().set_auto_enable(value & 0x01 != 0);
                true
            }
            0x4201 => {
                self.ppu.borrow_mut().write_register(offset, value);
                self.input.get_mut().write_wrio(value);
                true
            }
            0x4207..=0x420A => {
                self.ppu.borrow_mut().write_register(offset, value);
                true
            }
            // $2200-$2208 are the SNES-side-writable subset of the control/vector register
            // block; $2209-$220F (SCNT/CIE/CIC/SNV/SIV) are SA-1-side-writable instead (handled
            // by `Sa1Bus`). All are write-only (fullsnes "Write Only Registers"), so no
            // read_mmio arm -- $2300 SFR is the separate read-only status register.
            0x2200..=0x2208 => match &self.sa1_registers {
                Some(registers) => {
                    // A CCNT write that releases a held reset also clears the SA-1-side I-RAM
                    // protection register (bsnes `SA1::writeIOCPU`: "CIWP is set to 0 at
                    // reset"); the SNES-side SIWP is untouched. Conformance-tested by absindx
                    // `SA1RamProtectionTest` TEST ID 221.
                    if offset == 0x2200
                        && registers.borrow().is_held_in_reset()
                        && value & 0x20 == 0
                        && let Some(iram) = &self.sa1_iram
                    {
                        iram.borrow_mut().set_sa1_write_protect(0x00);
                    }
                    registers.borrow_mut().write(offset, value);
                    true
                }
                None => false,
            },
            // $2229 SIWP: write-only (fullsnes "Write Only Registers"), so no read_mmio arm.
            0x2229 => match &self.sa1_iram {
                Some(iram) => {
                    iram.borrow_mut().set_snes_write_protect(value);
                    true
                }
                None => false,
            },
            // $2220-$2224/$2226/$2228 are the SNES-side-writable subset of the Super MMC/BW-RAM
            // register block; $2225 BMAP and $2227 CBWE are SA-1-side-writable instead (handled
            // by `Sa1Bus`). All are write-only (fullsnes "Write Only Registers"), so no
            // read_mmio arm.
            0x2220..=0x2224 | 0x2226 | 0x2228 => match &self.sa1_memory_control {
                Some(memory_control) => {
                    memory_control.borrow_mut().write(offset, value);
                    true
                }
                None => false,
            },
            0x4300..=0x437F => self.dma.write_register(offset, value),
            _ => false,
        }
    }

    /// Console reset: forwards the /RES line to the S-SMP (see
    /// `SnesApu::reset`); ARAM survives, ports/timers/SPC clock restart.
    pub(crate) fn reset_apu(&mut self) {
        self.apu.borrow_mut().reset();
    }

    /// Advances the APU, PPU, and input latch by exactly one master clock.
    ///
    /// DRAM refresh (see [`Ppu::dram_refresh_due`]) is a CPU/bus-wide stall, not a PPU-only
    /// event: every one of its stolen clocks must tick the APU and input the same way a normal
    /// clock does, or they'd desynchronize from the PPU's (and the bus's own `ticks` counter's)
    /// timeline. `SnesBus::tick()` loops this single-clock step an extra
    /// `DRAM_REFRESH_STOLEN_CLOCKS` times whenever the just-ticked clock was the refresh trigger.
    fn tick_one_master_clock(&mut self) {
        self.ticks.set(self.ticks.get().wrapping_add(1));
        self.apu.borrow_mut().tick();
        // Single `borrow_mut()` scope for both calls -- this runs once per master clock, so two
        // separate `RefCell` runtime checks here would be needless overhead on a hot path.
        let auto_joypad_latch = {
            let mut ppu = self.ppu.borrow_mut();
            ppu.tick();
            ppu.poll_auto_joypad_latch()
        };
        if auto_joypad_latch {
            self.input.get_mut().trigger_auto_read();
        }
        self.input.get_mut().tick();
        if let Some(sa1_core) = &mut self.sa1_core {
            sa1_core.tick_one_master_clock();
        }
    }
}

impl DmaABus for SnesSystemBus {
    fn dma_read_a_bus(&mut self, addr: u32, open_bus: u8) -> u8 {
        self.dma_read_a_bus_impl(addr, open_bus)
    }

    fn dma_write_a_bus(&mut self, addr: u32, value: u8) {
        self.dma_write_a_bus_impl(addr, value);
    }

    fn dma_write_b_bus(&mut self, addr: u8, value: u8) {
        match addr {
            0x00..=0x3F => self
                .ppu
                .borrow_mut()
                .write_register(0x2100 + u16::from(addr), value),
            0x40..=0x43 => self
                .apu
                .borrow_mut()
                .write_main_port((addr - 0x40) as usize, value),
            // WMDATA/WMADD ($2180-$2183): DMA is a common way to fill WRAM
            // (e.g. transferring test/result tables), so it must go through
            // the same auto-incrementing WRAM port a direct CPU store would.
            0x80..=0x83 => {
                self.write_mmio(0x00_2100 + u32::from(addr), value);
            }
            _ => {}
        }
    }
}

impl SnesBus for SnesSystemBus {
    fn read(&self, addr: u32) -> u8 {
        if let Some(value) = self.read_mmio(addr) {
            self.mdr.set(value);
            return value;
        }

        if let Some(byte) = self.sa1_snes_vector_override_byte(addr) {
            self.mdr.set(byte);
            return byte;
        }

        if let Some(index) = Self::decode_wram_index(addr) {
            let value = self.wram[index];
            self.mdr.set(value);
            return value;
        }
        if let (Some(iram), Some(offset)) = (&self.sa1_iram, decode_mirror_offset(addr)) {
            let value = iram.borrow().read(offset);
            self.mdr.set(value);
            return value;
        }
        if self.sa1_memory_control.is_some() {
            if let Some(index) = self.sa1_bwram_index(addr) {
                let sram = self.sram.borrow();
                let value = if sram.is_empty() {
                    self.mdr.get()
                } else {
                    sram.get(index % sram.len())
                        .copied()
                        .unwrap_or_else(|| self.mdr.get())
                };
                self.mdr.set(value);
                return value;
            }
            return if let Some(index) = self.sa1_rom_index(addr) {
                if let Some(&value) = self.rom.get(index) {
                    self.mdr.set(value);
                    value
                } else {
                    self.mdr.get()
                }
            } else {
                self.mdr.get()
            };
        }
        if let Some(index) = self.decode_rom_index(addr) {
            if let Some(&value) = self.rom.get(index) {
                self.mdr.set(value);
                return value;
            }
            return self.mdr.get();
        }
        if let Some(index) = self.decode_sram_index(addr) {
            let sram = self.sram.borrow();
            if sram.is_empty() {
                return self.mdr.get();
            }
            if let Some(&value) = sram.get(index % sram.len()) {
                self.mdr.set(value);
                return value;
            }
            return self.mdr.get();
        }
        self.mdr.get()
    }

    fn read_for_debugger(&self, addr: u32) -> u8 {
        self.read_for_debugger_impl(addr)
    }

    fn write(&mut self, addr: u32, value: u8) {
        if self.write_mmio(addr, value) {
            return;
        }

        if let Some(index) = Self::decode_wram_index(addr) {
            self.wram[index] = value;
            return;
        }
        if let (Some(iram), Some(offset)) = (&self.sa1_iram, decode_mirror_offset(addr)) {
            iram.borrow_mut().write_from_snes(offset, value);
            return;
        }
        if self.sa1_memory_control.is_some() {
            if let Some(index) = self.sa1_bwram_index(addr) {
                self.write_sa1_bwram(index, value);
            }
            // ROM is read-only.
            return;
        }
        if let Some(index) = self.decode_sram_index(addr) {
            let mut sram = self.sram.borrow_mut();
            let len = sram.len();
            if len != 0 {
                let wrapped = index % len;
                if let Some(slot) = sram.get_mut(wrapped) {
                    *slot = value;
                }
            }
        }
    }

    fn tick(&mut self) {
        self.tick_one_master_clock();
        self.check_hdma_triggers();
        if self.ppu.borrow_mut().dram_refresh_due() {
            for _ in 0..DRAM_REFRESH_STOLEN_CLOCKS {
                self.tick_one_master_clock();
                self.check_hdma_triggers();
            }
        }
    }

    fn poll_nmi(&mut self) -> bool {
        self.ppu.borrow_mut().poll_nmi()
    }

    fn poll_irq(&self) -> bool {
        // CPU-dispatch-visible signal (one-dot delayed vs. the raw PPU line) -- see
        // `Ppu::poll_irq_dispatch` and the `SnesBus::poll_irq` trait doc. OR'd with SA-1's own
        // cross-CPU IRQ line (SCNT bit 7 pending && SIE bit 7 enabled), which has no equivalent
        // dispatch-pipeline delay to model.
        self.ppu.borrow().poll_irq_dispatch()
            || self
                .sa1_registers
                .as_ref()
                .is_some_and(|registers| registers.borrow().snes_irq_line())
    }

    fn screen_dimensions(&self) -> (u32, u32) {
        self.ppu_screen_dimensions()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snes::input::SnesControllerType;
    use crate::snes::ppu::{DOTS_PER_SCANLINE, MASTER_CYCLES_PER_DOT, NTSC_SCANLINES_PER_FRAME};

    fn build_cart(
        rom: &mut [u8],
        header_base: usize,
        map_mode: u8,
        ram_size_field: u8,
    ) -> Cartridge {
        let base = header_base;
        rom[base..base + 21].copy_from_slice(b"SYSTEM BUS TEST      ");
        rom[base + 0x3C] = 0x00;
        rom[base + 0x3D] = 0x80;
        rom[base + 0x15] = map_mode;
        rom[base + 0x16] = 0x00;
        rom[base + 0x17] = 0x07;
        rom[base + 0x18] = ram_size_field;
        rom[base + 0x1C] = 0x34;
        rom[base + 0x1D] = 0x12;
        rom[base + 0x1E] = 0xCB;
        rom[base + 0x1F] = 0xED;
        Cartridge::from_bytes(rom).expect("valid test cartridge")
    }

    fn lorom_test_cart() -> Cartridge {
        let mut rom = vec![0u8; 0x10000];
        build_cart(&mut rom, 0x7FC0, 0x20, 0x00)
    }

    fn lorom_cart_with_sram() -> Cartridge {
        let mut rom = vec![0u8; 0x20000];
        build_cart(&mut rom, 0x7FC0, 0x20, 0x05)
    }

    fn sa1_test_cart() -> Cartridge {
        let mut rom = vec![0u8; 0x10000];
        let base = 0x7FC0;
        rom[base..base + 21].copy_from_slice(b"SYSTEM BUS TEST      ");
        rom[base + 0x3C] = 0x00;
        rom[base + 0x3D] = 0x80;
        rom[base + 0x15] = 0x20;
        rom[base + 0x16] = 0x35; // SA-1 chipset (matches the vendored absindx ROMs' own header)
        rom[base + 0x17] = 0x07;
        rom[base + 0x18] = 0x00;
        rom[base + 0x1C] = 0x34;
        rom[base + 0x1D] = 0x12;
        rom[base + 0x1E] = 0xCB;
        rom[base + 0x1F] = 0xED;
        Cartridge::from_bytes(&rom).expect("valid SA-1 test cartridge")
    }

    /// Same as [`sa1_test_cart`] but with a non-zero BW-RAM (SRAM) size, for BW-RAM tests.
    fn sa1_test_cart_with_bwram() -> Cartridge {
        let mut rom = vec![0u8; 0x10000];
        let base = 0x7FC0;
        rom[base..base + 21].copy_from_slice(b"SYSTEM BUS TEST      ");
        rom[base + 0x3C] = 0x00;
        rom[base + 0x3D] = 0x80;
        rom[base + 0x15] = 0x20;
        rom[base + 0x16] = 0x35; // SA-1 chipset
        rom[base + 0x17] = 0x07;
        rom[base + 0x18] = 0x05; // 32 KB BW-RAM
        rom[base + 0x1C] = 0x34;
        rom[base + 0x1D] = 0x12;
        rom[base + 0x1E] = 0xCB;
        rom[base + 0x1F] = 0xED;
        Cartridge::from_bytes(&rom).expect("valid SA-1 test cartridge with BW-RAM")
    }

    fn lorom_cart_with_battery_sram() -> Cartridge {
        let mut rom = vec![0u8; 0x20000];
        let base = 0x7FC0;
        rom[base..base + 21].copy_from_slice(b"SYSTEM BUS TEST      ");
        rom[base + 0x3C] = 0x00;
        rom[base + 0x3D] = 0x80;
        rom[base + 0x15] = 0x20;
        rom[base + 0x16] = 0x02; // Battery-backed RAM chipset
        rom[base + 0x17] = 0x07;
        rom[base + 0x18] = 0x05; // 32 KB SRAM
        rom[base + 0x1C] = 0x34;
        rom[base + 0x1D] = 0x12;
        rom[base + 0x1E] = 0xCB;
        rom[base + 0x1F] = 0xED;
        Cartridge::from_bytes(&rom).expect("valid test cartridge")
    }

    fn write_dma_channel(
        bus: &mut SnesSystemBus,
        channel: u8,
        dmap: u8,
        bbad: u8,
        a_addr: u32,
        count: u16,
    ) {
        let base = 0x004300u32 + (channel as u32) * 0x10;
        bus.write(base, dmap);
        bus.write(base + 0x1, bbad);
        bus.write(base + 0x2, (a_addr & 0xFF) as u8);
        bus.write(base + 0x3, ((a_addr >> 8) & 0xFF) as u8);
        bus.write(base + 0x4, ((a_addr >> 16) & 0xFF) as u8);
        bus.write(base + 0x5, (count & 0xFF) as u8);
        bus.write(base + 0x6, (count >> 8) as u8);
    }

    fn write_hdma_channel(bus: &mut SnesSystemBus, channel: u8, dmap: u8, bbad: u8, a_addr: u32) {
        let base = 0x004300u32 + (channel as u32) * 0x10;
        bus.write(base, dmap);
        bus.write(base + 0x1, bbad);
        bus.write(base + 0x2, (a_addr & 0xFF) as u8);
        bus.write(base + 0x3, ((a_addr >> 8) & 0xFF) as u8);
        bus.write(base + 0x4, ((a_addr >> 16) & 0xFF) as u8);
    }

    fn tick_one_ppu_frame(bus: &mut SnesSystemBus) {
        let ticks =
            DOTS_PER_SCANLINE as u32 * NTSC_SCANLINES_PER_FRAME as u32 * MASTER_CYCLES_PER_DOT;
        for _ in 0..ticks {
            bus.ppu.borrow_mut().tick();
        }
    }

    fn read_joyb_pair_words(bus: &mut SnesSystemBus) -> (u16, u16) {
        bus.write(0x004016, 0x01);
        bus.write(0x004016, 0x00);

        let mut joy2 = 0u16;
        let mut joy4 = 0u16;
        for _ in 0..16 {
            let value = bus.read(0x004017);
            joy2 = (joy2 << 1) | (value & 0x01) as u16;
            joy4 = (joy4 << 1) | ((value >> 1) & 0x01) as u16;
        }
        (joy2, joy4)
    }

    #[test]
    fn wram_direct_region_round_trips() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x7E0000, 0x5A);
        assert_eq!(bus.read(0x7E0000), 0x5A);
    }

    #[test]
    fn read_for_debugger_on_mmio_returns_mdr_without_side_effects() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x002181, 0x00);
        bus.write(0x002182, 0x00);
        bus.write(0x002183, 0x00);
        bus.write(0x7E0000, 0x42);
        bus.mdr.set(0xA5);

        assert_eq!(bus.read_for_debugger(0x002180), 0xA5);
        assert_eq!(bus.wmadd.get(), 0);
        assert_eq!(bus.mdr.get(), 0xA5);
    }

    #[test]
    fn low_ram_mirror_region_maps_to_wram_base() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x000123, 0x3C);
        assert_eq!(bus.read(0x7E0123), 0x3C);
        assert_eq!(bus.read(0x800123), 0x3C);
    }

    #[test]
    fn unmapped_reads_return_mdr_from_last_successful_read() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x7E0010, 0xA7);
        assert_eq!(bus.read(0x7E0010), 0xA7);
        assert_eq!(bus.read(0x002100), 0xA7);
    }

    #[test]
    fn lorom_reads_from_upper_32k_windows() {
        let mut rom = vec![0u8; 0x40000];
        rom[0x000000] = 0x11; // bank 00, addr 8000
        rom[0x008000] = 0x22; // bank 01, addr 8000
        let cart = build_cart(&mut rom, 0x7FC0, 0x20, 0x00);
        let bus = SnesSystemBus::new(cart);

        assert_eq!(bus.read(0x008000), 0x11);
        assert_eq!(bus.read(0x018000), 0x22);
    }

    #[test]
    fn hirom_reads_from_full_64k_windows() {
        let mut rom = vec![0u8; 0x30000];
        rom[0x000000] = 0x33; // C0:0000
        rom[0x010000] = 0x44; // C1:0000
        let cart = build_cart(&mut rom, 0xFFC0, 0x21, 0x00);
        let bus = SnesSystemBus::new(cart);

        assert_eq!(bus.read(0xC00000), 0x33);
        assert_eq!(bus.read(0xC10000), 0x44);
    }

    #[test]
    fn exhirom_maps_c0_ff_to_lower_4mb_and_40_7d_to_upper_4mb() {
        let mut rom = vec![0u8; 0x800000];
        rom[0x000000] = 0x55; // C0:0000
        rom[0x400000] = 0x66; // 40:0000
        let cart = build_cart(&mut rom, 0x40FFC0, 0x35, 0x00);
        let bus = SnesSystemBus::new(cart);

        assert_eq!(bus.read(0xC00000), 0x55);
        assert_eq!(bus.read(0x400000), 0x66);
    }

    #[test]
    fn lorom_sram_window_round_trips() {
        let mut bus = SnesSystemBus::new(lorom_cart_with_sram());
        bus.write(0x700123, 0x7D);
        assert_eq!(bus.read(0x700123), 0x7D);
    }

    #[test]
    fn hirom_reads_from_40_7d_window() {
        let mut rom = vec![0u8; 0x20000];
        rom[0x000000] = 0x77; // 40:0000
        let cart = build_cart(&mut rom, 0xFFC0, 0x21, 0x00);
        let bus = SnesSystemBus::new(cart);
        assert_eq!(bus.read(0x400000), 0x77);
    }

    #[test]
    fn exhirom_reads_from_low_bank_upper_window() {
        let mut rom = vec![0u8; 0x800000];
        rom[0x008000] = 0x88; // 00:8000
        let cart = build_cart(&mut rom, 0x40FFC0, 0x35, 0x00);
        let bus = SnesSystemBus::new(cart);
        assert_eq!(bus.read(0x008000), 0x88);
    }

    #[test]
    fn hirom_sram_window_mirrors_to_a0_bf_banks() {
        let mut rom = vec![0u8; 0x40000];
        let cart = build_cart(&mut rom, 0xFFC0, 0x21, 0x05);
        let mut bus = SnesSystemBus::new(cart);
        bus.write(0x206123, 0x91);
        assert_eq!(bus.read(0xA06123), 0x91);
    }

    #[test]
    fn sram_window_wraps_for_small_sram_sizes() {
        let mut rom = vec![0u8; 0x20000];
        let cart = build_cart(&mut rom, 0x7FC0, 0x20, 0x01); // 2 KiB SRAM
        let mut bus = SnesSystemBus::new(cart);
        bus.write(0x700000, 0xA1);
        bus.write(0x700800, 0xB2); // +2 KiB -> wraps to same byte
        assert_eq!(bus.read(0x700000), 0xB2);
    }

    #[test]
    fn wram_ports_auto_increment_and_write_through() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        // WMADD = 0x000123
        bus.write(0x002181, 0x23);
        bus.write(0x002182, 0x01);
        bus.write(0x002183, 0x00);
        bus.write(0x002180, 0xAA); // write at 0x0123, increment
        bus.write(0x002180, 0xBB); // write at 0x0124

        assert_eq!(bus.read(0x7E0123), 0xAA);
        assert_eq!(bus.read(0x7E0124), 0xBB);
    }

    #[test]
    fn multiply_registers_update_rdmpy() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x004202, 6);
        bus.write(0x004203, 7);

        assert_eq!(bus.read(0x004216), 42);
        assert_eq!(bus.read(0x004217), 0);
    }

    #[test]
    fn divide_registers_update_rddiv_and_remainder() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x004204, 0x34);
        bus.write(0x004205, 0x12);
        bus.write(0x004206, 0x10);

        assert_eq!(bus.read(0x004214), 0x23);
        assert_eq!(bus.read(0x004215), 0x01);
        assert_eq!(bus.read(0x004216), 0x04);
        assert_eq!(bus.read(0x004217), 0x00);
    }

    #[test]
    fn memsel_register_reads_back_last_written_value() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x00420D, 0x01);
        assert_eq!(bus.read(0x00420D), 0x01);
        bus.write(0x80420D, 0x00);
        assert_eq!(bus.read(0x00420D), 0x00);
    }

    #[test]
    fn dma_register_file_latches_and_mirrors_across_system_banks() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x004300, 0xAB);
        bus.write(0x00430A, 0x55);

        assert_eq!(bus.read(0x004300), 0xAB);
        assert_eq!(bus.read(0x804300), 0xAB);
        assert_eq!(bus.read(0x00430A), 0x55);
    }

    #[test]
    fn dma_channel_register_blocks_do_not_alias() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());

        for channel in 0u32..8 {
            for reg in 0u32..=0x0B {
                let addr = 0x004300 + channel * 0x10 + reg;
                let value = (channel as u8).wrapping_mul(0x10).wrapping_add(reg as u8);
                bus.write(addr, value);
            }
        }

        for channel in 0u32..8 {
            for reg in 0u32..=0x0B {
                let addr = 0x004300 + channel * 0x10 + reg;
                let expected = (channel as u8).wrapping_mul(0x10).wrapping_add(reg as u8);
                assert_eq!(bus.read(addr), expected);
            }
        }
    }

    #[test]
    fn wram_port_reads_auto_increment_address() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x002181, 0x00);
        bus.write(0x002182, 0x02);
        bus.write(0x002183, 0x00);
        bus.write(0x002180, 0xC1);
        bus.write(0x002180, 0xD2);
        // Reset WMADD to the first byte and verify consecutive reads advance.
        bus.write(0x002181, 0x00);
        bus.write(0x002182, 0x02);
        bus.write(0x002183, 0x00);
        assert_eq!(bus.read(0x002180), 0xC1);
        assert_eq!(bus.read(0x002180), 0xD2);
    }

    #[test]
    fn wrdiv_writes_do_not_change_rddiv_until_divide_trigger() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        assert_eq!(bus.read(0x004214), 0x00);
        assert_eq!(bus.read(0x004215), 0x00);
        bus.write(0x004204, 0x34);
        bus.write(0x004205, 0x12);
        assert_eq!(bus.read(0x004214), 0x00);
        assert_eq!(bus.read(0x004215), 0x00);
        bus.write(0x004206, 0x10);
        assert_eq!(bus.read(0x004214), 0x23);
        assert_eq!(bus.read(0x004215), 0x01);
    }

    #[test]
    fn apu_port_writes_are_visible_through_the_main_cpu_bus() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());

        bus.write(0x002140, 0xA5);
        assert_eq!(bus.apu_read_spc_port_for_test(0), 0xA5);

        bus.apu_write_spc_port_for_test(1, 0x5A);
        assert_eq!(bus.read(0x002141), 0x5A);
    }

    #[test]
    fn custom_spc_ipl_path_overrides_embedded_ipl() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("ipl.bin");
        std::fs::write(&path, [0xEAu8; 64]).expect("write custom ipl");

        let mut bus = SnesSystemBus::new_with_spc_ipl_path(
            lorom_test_cart(),
            Some(path.to_str().expect("utf8 path")),
        );
        assert_eq!(bus.apu_read_spc_memory_for_test(0xFFC0), 0xEA);
    }

    #[test]
    fn mdmaen_runs_dma_synchronously_and_updates_channel_registers() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x7E0100, 0x3A);
        write_dma_channel(&mut bus, 0, 0x00, 0x00, 0x7E0100, 1);
        bus.write(0x00420B, 0x01);

        // Read back from B-bus via reverse DMA to verify data landed at $2100.
        write_dma_channel(&mut bus, 0, 0x80, 0x00, 0x7E0200, 1);
        bus.write(0x00420B, 0x01);
        assert_eq!(bus.read(0x7E0200), 0x3A);

        // DAS reaches zero and A1T advances by transfer byte count.
        assert_eq!(bus.read(0x004305), 0x00);
        assert_eq!(bus.read(0x004306), 0x00);
        assert_eq!(bus.read(0x004302), 0x01);
        assert_eq!(bus.read(0x004303), 0x02);
    }

    #[test]
    fn dma_a_to_b_wmdata_writes_wram_and_advances_wmadd() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        // Point WMDATA's auto-increment pointer at $000123.
        bus.write(0x002181, 0x23);
        bus.write(0x002182, 0x01);
        bus.write(0x002183, 0x00);

        bus.write(0x7E0100, 0x42);
        write_dma_channel(&mut bus, 0, 0x00, 0x80, 0x7E0100, 1);
        bus.write(0x00420B, 0x01);

        // The byte must land at $000123 (via WRAM, not silently dropped).
        assert_eq!(bus.read(0x7E0123), 0x42);
        // wmadd must have advanced by 1, just like a direct CPU write to $2180 would.
        assert_eq!(bus.read(0x002181), 0x24);
        assert_eq!(bus.read(0x002182), 0x01);
    }

    #[test]
    fn dma_a_to_b_to_cgram_updates_backdrop_color_visible_output() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x002100, 0x0F); // visible output
        bus.write(0x002121, 0x00); // CGADD = color 0
        bus.write(0x7E0190, 0x1F); // BGR555 low byte (red)
        bus.write(0x7E0191, 0x00); // high byte
        write_dma_channel(&mut bus, 0, 0x00, 0x22, 0x7E0190, 2);
        bus.write(0x00420B, 0x01);
        tick_one_ppu_frame(&mut bus);

        let rgb = bus.ppu_screen_snapshot();
        assert_eq!(&rgb[0..3], &[255, 0, 0]);
    }

    #[test]
    fn mdmaen_executes_channels_in_priority_order_and_accounts_cycles() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x7E0100, 0x11);
        bus.write(0x7E0200, 0x22);
        write_dma_channel(&mut bus, 0, 0x00, 0x10, 0x7E0100, 1);
        write_dma_channel(&mut bus, 1, 0x00, 0x10, 0x7E0200, 1);

        let ticks_before = bus.ticks.get();
        bus.write(0x00420B, 0x03);
        let ticks_after = bus.ticks.get();

        // Channel 1 must run after channel 0, so final B-bus value is from channel 1.
        write_dma_channel(&mut bus, 0, 0x80, 0x10, 0x7E0300, 1);
        bus.write(0x00420B, 0x01);
        assert_eq!(bus.read(0x7E0300), 0x22);

        // 8/byte + 8/channel + fixed 16 global transfer overhead.
        assert_eq!(ticks_after - ticks_before, 16 + 2 * 8 + 2 * 8);
    }

    #[test]
    fn dma_modes_5_6_7_alias_modes_1_2_3() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());

        // mode 5 should alias mode 1 (p, p+1)
        bus.write(0x7E1000, 0xA1);
        bus.write(0x7E1001, 0xB2);
        write_dma_channel(&mut bus, 0, 0x05, 0x20, 0x7E1000, 2);
        bus.write(0x00420B, 0x01);
        write_dma_channel(&mut bus, 0, 0x81, 0x20, 0x7E1100, 2);
        bus.write(0x00420B, 0x01);
        assert_eq!(bus.read(0x7E1100), 0xA1);
        assert_eq!(bus.read(0x7E1101), 0xB2);

        // mode 6 should alias mode 2 (p, p)
        bus.write(0x7E1200, 0xC3);
        bus.write(0x7E1201, 0xD4);
        write_dma_channel(&mut bus, 0, 0x06, 0x24, 0x7E1200, 2);
        bus.write(0x00420B, 0x01);
        write_dma_channel(&mut bus, 0, 0x82, 0x24, 0x7E1300, 2);
        bus.write(0x00420B, 0x01);
        assert_eq!(bus.read(0x7E1300), 0xD4);
        assert_eq!(bus.read(0x7E1301), 0xD4);

        // mode 7 should alias mode 3 (p, p, p+1, p+1)
        bus.write(0x7E1400, 0x10);
        bus.write(0x7E1401, 0x20);
        bus.write(0x7E1402, 0x30);
        bus.write(0x7E1403, 0x40);
        write_dma_channel(&mut bus, 0, 0x07, 0x28, 0x7E1400, 4);
        bus.write(0x00420B, 0x01);
        write_dma_channel(&mut bus, 0, 0x83, 0x28, 0x7E1500, 4);
        bus.write(0x00420B, 0x01);
        assert_eq!(bus.read(0x7E1500), 0x20);
        assert_eq!(bus.read(0x7E1501), 0x20);
        assert_eq!(bus.read(0x7E1502), 0x40);
        assert_eq!(bus.read(0x7E1503), 0x40);
    }

    #[test]
    fn slhv_read_returns_open_bus_instead_of_a_fixed_zero() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());

        // Prime MDR with a nonzero value; SLHV ($2137) doesn't drive the data bus, so reading
        // it must retain this value rather than clobbering it with a fixed 0.
        bus.write(0x7E1500, 0x9A);
        assert_eq!(bus.read(0x7E1500), 0x9A);

        assert_eq!(bus.read(0x002137), 0x9A);
        // The retained open-bus value carries over to the next open-bus-style read too.
        assert_eq!(bus.read(0x002137), 0x9A);
    }

    #[test]
    fn dma_a_bus_mmio_regions_are_treated_as_open_bus() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());

        // Prime MDR with a value that differs from DMA register file contents.
        bus.write(0x7E0010, 0x9A);
        assert_eq!(bus.read(0x7E0010), 0x9A);
        bus.write(0x004300, 0x55);

        // A-bus source points to excluded MMIO space ($4300); DMA must read open bus (MDR=0x9A).
        write_dma_channel(&mut bus, 0, 0x00, 0x30, 0x004300, 1);
        bus.write(0x00420B, 0x01);
        write_dma_channel(&mut bus, 0, 0x80, 0x30, 0x7E1600, 1);
        bus.write(0x00420B, 0x01);

        assert_eq!(bus.read(0x7E1600), 0x9A);
    }

    #[test]
    fn dma_byte_count_zero_means_65536_bytes() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x7E1700, 0x6E);
        write_dma_channel(&mut bus, 0, 0x08, 0x38, 0x7E1700, 0x0000); // fixed A-bus step
        bus.write(0x00420B, 0x01);

        assert_eq!(bus.read(0x004305), 0x00);
        assert_eq!(bus.read(0x004306), 0x00);
        // Fixed addressing keeps A1T unchanged after a 65536-byte transfer.
        assert_eq!(bus.read(0x004302), 0x00);
        assert_eq!(bus.read(0x004303), 0x17);
    }

    #[test]
    fn dma_updates_mdr_with_last_transferred_byte() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x7E1800, 0x5C);
        write_dma_channel(&mut bus, 0, 0x00, 0x40, 0x7E1800, 1);
        bus.write(0x00420B, 0x01);

        // Unmapped read returns MDR; after DMA it should be the last transferred byte.
        assert_eq!(bus.read(0x002200), 0x5C);
    }

    #[test]
    fn hdmaen_register_latches_written_value() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x00420C, 0x81);
        assert_eq!(bus.read(0x00420C), 0x81);
    }

    #[test]
    fn hdma_init_loads_a2a_and_line_counter_from_table() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        write_hdma_channel(&mut bus, 0, 0x00, 0x20, 0x7E2100);
        bus.write(0x7E2100, 0x02); // line descriptor (repeat clear, 2 lines)
        bus.write(0x7E2101, 0xAA); // first direct data byte
        bus.write(0x00420C, 0x01);

        bus.hdma_init();

        assert_eq!(bus.read(0x004308), 0x01); // table current ptr low advanced past descriptor
        assert_eq!(bus.read(0x004309), 0x21);
        assert_eq!(bus.read(0x00430A), 0x02);
    }

    #[test]
    fn tick_automatically_runs_hdma_init_and_transfer_without_manual_calls() {
        // Regression test for #2947: hdma_init/hdma_do_line were fully implemented
        // and unit-tested, but nothing in the real per-master-clock tick loop ever
        // called them, so HDMA never actually ran during emulation. This drives
        // the system purely through `tick()` (as real emulation does) and expects
        // the transfer to land without any manual hdma_init()/hdma_do_line() call.
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        // Point WMDATA's auto-increment pointer at $000200.
        bus.write(0x002181, 0x00);
        bus.write(0x002182, 0x02);
        bus.write(0x002183, 0x00);

        // HDMA channel 0: direct mode, A->B, targets WMDATA ($2180, B-bus offset 0x80).
        write_hdma_channel(&mut bus, 0, 0x00, 0x80, 0x7E3000);
        bus.write(0x7E3000, 0xFF); // repeat mode, plenty of lines
        bus.write(0x7E3001, 0x7A); // data byte
        bus.write(0x00420C, 0x01); // enable HDMA channel 0

        // Advance one full scanline purely via tick() -- past both the once-per-frame
        // HDMA init point and the once-per-scanline HDMA transfer point.
        let ticks_per_line = u32::from(DOTS_PER_SCANLINE) * MASTER_CYCLES_PER_DOT;
        for _ in 0..ticks_per_line {
            bus.tick();
        }

        assert_eq!(
            bus.read(0x7E0200),
            0x7A,
            "HDMA transfer should have run automatically via tick(), with no manual \
             hdma_init()/hdma_do_line() call"
        );
    }

    #[test]
    fn hdma_do_line_direct_mode_transfers_then_pauses_when_repeat_clear() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        write_hdma_channel(&mut bus, 0, 0x00, 0x30, 0x7E2200); // mode0 direct, A->B
        bus.write(0x7E2200, 0x02); // transfer once, then pause one line
        bus.write(0x7E2201, 0x5A); // data for first line
        bus.write(0x7E2202, 0x01); // next descriptor
        bus.write(0x7E2203, 0xC3); // second transfer data
        bus.write(0x7E2204, 0x00); // terminator
        bus.write(0x00420C, 0x01);

        bus.hdma_init();
        bus.hdma_do_line(); // transfer 0x5A
        bus.hdma_do_line(); // pause
        bus.hdma_do_line(); // transfer 0xC3

        write_dma_channel(&mut bus, 0, 0x80, 0x30, 0x7E2210, 1);
        bus.write(0x00420B, 0x01);
        assert_eq!(bus.read(0x7E2210), 0xC3);
    }

    #[test]
    fn hdma_do_line_indirect_mode_loads_pointer_and_advances_das() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        write_hdma_channel(&mut bus, 0, 0x40, 0x34, 0x7E2300); // indirect mode, mode0
        bus.write(0x004307, 0x7E); // DASB
        bus.write(0x7E2300, 0x81); // repeat set, 1 line
        bus.write(0x7E2301, 0x20); // indirect low
        bus.write(0x7E2302, 0x23); // indirect high -> 7E2320
        bus.write(0x7E2320, 0x9B); // indirect data byte
        bus.write(0x7E2303, 0x00); // next descriptor terminator
        bus.write(0x00420C, 0x01);

        bus.hdma_init();
        bus.hdma_do_line();

        assert_eq!(bus.read(0x004305), 0x21); // DAS advanced after one byte transfer
        assert_eq!(bus.read(0x004306), 0x23);
    }

    #[test]
    fn hdma_do_line_without_explicit_init_does_not_transfer() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        write_hdma_channel(&mut bus, 0, 0x00, 0x50, 0x7E2400);
        bus.write(0x7E2400, 0x01);
        bus.write(0x7E2401, 0x99);
        bus.write(0x00420C, 0x01);

        bus.hdma_do_line();

        write_dma_channel(&mut bus, 0, 0x80, 0x50, 0x7E2410, 1);
        bus.write(0x00420B, 0x01);
        assert_eq!(bus.read(0x7E2410), 0x00);
    }

    #[test]
    fn hdma_channels_execute_in_ascending_order_each_line() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        write_hdma_channel(&mut bus, 0, 0x00, 0x60, 0x7E2500);
        write_hdma_channel(&mut bus, 1, 0x00, 0x60, 0x7E2600);
        bus.write(0x7E2500, 0x01);
        bus.write(0x7E2501, 0x11);
        bus.write(0x7E2502, 0x00);
        bus.write(0x7E2600, 0x01);
        bus.write(0x7E2601, 0x22);
        bus.write(0x7E2602, 0x00);
        bus.write(0x00420C, 0x03);

        bus.hdma_init();
        bus.hdma_do_line();

        write_dma_channel(&mut bus, 0, 0x80, 0x60, 0x7E2610, 1);
        bus.write(0x00420B, 0x01);
        assert_eq!(bus.read(0x7E2610), 0x22);
    }

    #[test]
    fn hdma_mode5_transfers_all_four_bytes_and_advances_table_pointer_correctly() {
        // Regression test for #2952: HDMA transfer mode 5 ("2 registers, written
        // twice each": B, B+1, B, B+1 -- 4 bytes per line) was reusing the
        // general-purpose DMA controller's `canonical_mode` simplification
        // (5 -> 1), which is only valid for GPDMA's cyclic multi-byte transfers
        // (mode 1's [0,1] pattern cycled over many bytes is byte-identical to
        // mode 5's [0,1,0,1] pattern cycled the same way). HDMA instead performs
        // exactly one full pattern per scanline with no cycling, so collapsing
        // mode 5 to mode 1's 2-byte pattern silently drops 2 of every 4 table
        // bytes -- both the wrong data reaching the B-bus *and* the per-channel
        // table pointer under-advancing, so the next reload misreads a leftover
        // data byte as the following entry's descriptor. This exact shape (a
        // one-line, non-repeat, mode-5 entry followed immediately by another
        // entry) is what the vendored `hvdma.sfc` test ROM's 5 VMDATA HDMA
        // channels use, and the pointer corruption made every one of them
        // mistake a leftover zero data byte for the table terminator right
        // after their first line, silently dropping the ROM's entire mid-frame
        // VRAM tile update (see the 93143-hblank-dma-vram README).
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        write_hdma_channel(&mut bus, 0, 0x05, 0x60, 0x7E2B00); // mode5, direct, A->B
        bus.write(0x7E2B00, 0x01); // entry1: repeat clear, 1 line
        bus.write(0x7E2B01, 0x11);
        bus.write(0x7E2B02, 0x22);
        bus.write(0x7E2B03, 0x33);
        bus.write(0x7E2B04, 0x44);
        bus.write(0x7E2B05, 0x01); // entry2: repeat clear, 1 line
        bus.write(0x7E2B06, 0x55);
        bus.write(0x7E2B07, 0x66);
        bus.write(0x7E2B08, 0x77);
        bus.write(0x7E2B09, 0x88);
        bus.write(0x7E2B0A, 0x00); // terminator
        bus.write(0x00420C, 0x01);

        bus.hdma_init();
        bus.hdma_do_line(); // transfers entry1's 4 bytes; reloads entry2 at the end
        bus.hdma_do_line(); // transfers entry2's 4 bytes

        // The B-bus port sees the *last* write to each of the 2 registers in
        // the B,B+1,B,B+1 pattern, so entry2's transfer should leave 0x77 (3rd
        // byte) / 0x88 (4th byte) behind -- not entry1's leftover bytes or a
        // misread descriptor, which is what the truncated-pattern bug produces.
        write_dma_channel(&mut bus, 0, 0x80, 0x60, 0x7E2B10, 1);
        write_dma_channel(&mut bus, 1, 0x80, 0x61, 0x7E2B11, 1);
        bus.write(0x00420B, 0x03);
        assert_eq!(
            bus.read(0x7E2B10),
            0x77,
            "B-bus port 0x60 after entry2's transfer"
        );
        assert_eq!(
            bus.read(0x7E2B11),
            0x88,
            "B-bus port 0x61 after entry2's transfer"
        );
    }

    #[test]
    fn hdma_descriptor_80_transfers_once_then_pauses() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        write_hdma_channel(&mut bus, 0, 0x00, 0x70, 0x7E2700);
        bus.write(0x7E2700, 0x80);
        bus.write(0x7E2701, 0x55);
        bus.write(0x7E2702, 0x01);
        bus.write(0x7E2703, 0x77);
        bus.write(0x7E2704, 0x00);
        bus.write(0x00420C, 0x01);

        bus.hdma_init();
        bus.hdma_do_line(); // transfer 0x55
        bus.hdma_do_line(); // should pause, not transfer 0x77

        write_dma_channel(&mut bus, 0, 0x80, 0x70, 0x7E2710, 1);
        bus.write(0x00420B, 0x01);
        assert_eq!(bus.read(0x7E2710), 0x55);
    }

    #[test]
    fn hdma_init_cycle_accounting_matches_direct_and_indirect_costs() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        write_hdma_channel(&mut bus, 0, 0x00, 0x20, 0x7E2800);
        bus.write(0x7E2800, 0x01);
        bus.write(0x7E2801, 0xAA);
        bus.write(0x7E2802, 0x00);

        write_hdma_channel(&mut bus, 1, 0x40, 0x24, 0x7E2900);
        bus.write(0x004317, 0x7E);
        bus.write(0x7E2900, 0x01);
        bus.write(0x7E2901, 0x40);
        bus.write(0x7E2902, 0x29);
        bus.write(0x7E2940, 0xBB);
        bus.write(0x7E2903, 0x00);

        bus.write(0x00420C, 0x03);
        let ticks_before = bus.ticks.get();
        bus.hdma_init();
        let ticks_after = bus.ticks.get();

        assert_eq!(ticks_after - ticks_before, 18 + 8 + 24);
    }

    #[test]
    fn hdma_init_indirect_channel_with_terminator_does_not_charge_pointer_load_cycles() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        write_hdma_channel(&mut bus, 0, 0x40, 0x20, 0x7E2A00);
        bus.write(0x004307, 0x7E);
        bus.write(0x7E2A00, 0x00);
        bus.write(0x00420C, 0x01);

        let ticks_before = bus.ticks.get();
        bus.hdma_init();
        let ticks_after = bus.ticks.get();

        assert_eq!(ticks_after - ticks_before, 18 + 8);
    }

    #[test]
    fn sram_size_returns_zero_for_cartridge_without_sram() {
        let bus = SnesSystemBus::new(lorom_test_cart());
        assert_eq!(bus.sram_size(), 0);
    }

    #[test]
    fn sram_size_returns_correct_size_for_cartridge_with_sram() {
        let bus = SnesSystemBus::new(lorom_cart_with_sram());
        // ram_size_field = 0x05 → 32 KB
        assert_eq!(bus.sram_size(), 32 * 1024);
    }

    #[test]
    fn has_battery_returns_false_for_cartridge_without_battery() {
        let bus = SnesSystemBus::new(lorom_test_cart());
        assert!(!bus.has_battery());
    }

    #[test]
    fn has_battery_returns_true_for_cartridge_with_battery() {
        let bus = SnesSystemBus::new(lorom_cart_with_battery_sram());
        assert!(bus.has_battery());
    }

    #[test]
    fn sram_snapshot_returns_empty_for_cartridge_without_sram() {
        let bus = SnesSystemBus::new(lorom_test_cart());
        assert!(bus.sram_snapshot().is_empty());
    }

    #[test]
    fn sram_snapshot_returns_current_sram_contents() {
        let bus = SnesSystemBus::new(lorom_cart_with_battery_sram());
        // Write some values to SRAM directly
        if bus.sram_size() > 0 {
            let mut sram = bus.sram.borrow_mut();
            sram[0] = 0xAA;
            sram[1] = 0xBB;
            sram[2] = 0xCC;
        }

        let snapshot = bus.sram_snapshot();
        assert_eq!(snapshot.len(), 32 * 1024);
        assert_eq!(snapshot[0], 0xAA);
        assert_eq!(snapshot[1], 0xBB);
        assert_eq!(snapshot[2], 0xCC);
    }

    #[test]
    fn restore_sram_overwrites_sram_contents() {
        let mut bus = SnesSystemBus::new(lorom_cart_with_battery_sram());
        let mut data = vec![0u8; 32 * 1024];
        data[0] = 0x11;
        data[1] = 0x22;
        data[2] = 0x33;

        bus.restore_sram(&data);

        let snapshot = bus.sram_snapshot();
        assert_eq!(snapshot[0], 0x11);
        assert_eq!(snapshot[1], 0x22);
        assert_eq!(snapshot[2], 0x33);
    }

    #[test]
    fn restore_sram_handles_partial_data() {
        let mut bus = SnesSystemBus::new(lorom_cart_with_battery_sram());
        let data = vec![0x55u8; 100]; // Only 100 bytes

        bus.restore_sram(&data);

        let snapshot = bus.sram_snapshot();
        // First 100 bytes should match
        assert_eq!(&snapshot[..100], &data[..100]);
        // Rest should be zeros
        assert_eq!(snapshot[100], 0x00);
    }

    #[test]
    fn restore_sram_ignores_oversized_data() {
        let mut bus = SnesSystemBus::new(lorom_cart_with_battery_sram());
        let mut data = vec![0u8; 64 * 1024]; // 64 KB, larger than SRAM
        data[0] = 0x77;
        data[32 * 1024 - 1] = 0x88;

        bus.restore_sram(&data);

        let snapshot = bus.sram_snapshot();
        assert_eq!(snapshot.len(), 32 * 1024);
        assert_eq!(snapshot[0], 0x77);
        assert_eq!(snapshot[32 * 1024 - 1], 0x88);
    }

    #[test]
    fn ppu_cgram_round_trips_through_the_bus() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());

        // CGADD = color 0x10, then write low/high CGRAM bytes via CGDATA.
        bus.write(0x002121, 0x10);
        bus.write(0x002122, 0x34);
        bus.write(0x002122, 0x12);

        // Re-point CGADD and read back via RDCGRAM.
        bus.write(0x002121, 0x10);
        assert_eq!(bus.read(0x00213B), 0x34);
        assert_eq!(bus.read(0x00213B), 0x12);
    }

    #[test]
    fn ppu_vram_round_trips_through_the_bus() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());

        // VMAIN: increment after high byte, step 1.
        bus.write(0x002115, 0x80);
        // VMADD = word $1000.
        bus.write(0x002116, 0x00);
        bus.write(0x002117, 0x10);
        // VMDATA low/high.
        bus.write(0x002118, 0xAA);
        bus.write(0x002119, 0xBB);

        // Re-point VMADD and read back (first read returns the prefetched word).
        bus.write(0x002116, 0x00);
        bus.write(0x002117, 0x10);
        assert_eq!(bus.read(0x002139), 0xAA);
        assert_eq!(bus.read(0x00213A), 0xBB);
    }

    #[test]
    fn ppu_register_access_is_mirrored_into_the_high_system_banks() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());

        // Write CGRAM via bank $80 mirror, read back via bank $00.
        bus.write(0x802121, 0x20);
        bus.write(0x802122, 0x5A);
        bus.write(0x802122, 0x3C);

        bus.write(0x002121, 0x20);
        assert_eq!(bus.read(0x00213B), 0x5A);
        assert_eq!(bus.read(0x00213B), 0x3C);
    }

    #[test]
    fn bus_tick_advances_the_ppu_counters() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());

        // 10 dots = 40 master clocks.
        for _ in 0..40 {
            bus.tick();
        }

        // SLHV strobe latches H/V; OPHCT low byte should read the dot counter.
        let _ = bus.read(0x002137);
        assert_eq!(bus.read(0x00213C), 10);
    }

    #[test]
    fn bus_poll_nmi_fires_at_vblank_when_enabled() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x004200, 0x80); // enable VBlank NMI

        // Advance to VBlank entry: 225 scanlines * 341 dots * 4 master clocks.
        for _ in 0..(225 * 341 * 4) {
            bus.tick();
        }

        assert!(
            bus.poll_nmi(),
            "NMI edge delivered through the bus at VBlank"
        );
        assert!(!bus.poll_nmi(), "edge consumed once");
    }

    #[test]
    fn bus_timeup_reflects_irq_line_immediately_at_the_trigger_clock() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x004207, 0x01);
        bus.write(0x004208, 0x00);
        bus.write(0x004200, 0x10); // H-IRQ mode

        // HTIME=1 fires at intra-scanline clock (1+1)*4 + 10 = 18.
        for _ in 0..18 {
            bus.tick(); // one master clock each
        }
        assert_ne!(
            bus.read(0x004211) & 0x80,
            0,
            "TIMEUP reflects the raw IRQ line the instant it triggers, unlike poll_irq()"
        );
    }

    #[test]
    fn bus_poll_irq_has_a_one_dot_dispatch_delay_then_read_ack_clears_it() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x004207, 0x01);
        bus.write(0x004208, 0x00);
        bus.write(0x004200, 0x10); // H-IRQ mode

        assert!(!bus.poll_irq(), "IRQ line starts deasserted");
        // HTIME=1 fires at intra-scanline clock (1+1)*4 + 10 = 18.
        for _ in 0..18 {
            bus.tick(); // one master clock each
        }
        // bsnes' `CPU::irqPoll` only turns a freshly-risen IRQ line into a CPU-visible
        // dispatch/WAI-wake transition on the *next* 4-clock poll (it samples the
        // previous poll's still-stale line value first) -- a fixed one-dot pipeline
        // delay. `poll_irq()` (unlike TIMEUP) must not be visible yet at the exact
        // trigger clock.
        assert!(
            !bus.poll_irq(),
            "poll_irq() is not yet visible at the exact IRQ-line trigger clock"
        );
        for _ in 0..4 {
            bus.tick();
        }
        assert!(
            bus.poll_irq(),
            "poll_irq() becomes visible one dot (4 master clocks) after the trigger"
        );

        assert_ne!(bus.read(0x004211) & 0x80, 0, "TIMEUP read sees pending IRQ");
        assert!(
            !bus.poll_irq(),
            "TIMEUP read acknowledges and deasserts IRQ line"
        );
    }

    /// Master cycles in one NTSC frame (341 dots * 262 lines * 4 cycles/dot).
    const FRAME_MASTER_CYCLES: u32 = 341 * 262 * 4;

    #[test]
    fn auto_joypad_reads_buttons_into_joy1_over_a_frame() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x004200, 0x01); // enable auto-joypad
        bus.set_controller_button(0, SnesButton::B, true);
        bus.set_controller_button(0, SnesButton::A, true);

        for _ in 0..(FRAME_MASTER_CYCLES + 4224) {
            bus.tick();
        }

        let joy1 = (bus.read(0x004219) as u16) << 8 | bus.read(0x004218) as u16;
        // B = serial bit 15, A = serial bit 7.
        assert_eq!(joy1, 0x8080);
    }

    #[test]
    fn auto_joypad_maps_port2_into_joy2() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x004200, 0x01);
        bus.set_controller_button(1, SnesButton::Start, true);

        for _ in 0..(FRAME_MASTER_CYCLES + 4224) {
            bus.tick();
        }

        let joy2 = (bus.read(0x00421B) as u16) << 8 | bus.read(0x00421A) as u16;
        // Start = serial bit 12.
        assert_eq!(joy2, 0x1000);
        let joy1 = (bus.read(0x004219) as u16) << 8 | bus.read(0x004218) as u16;
        assert_eq!(joy1, 0x0000, "port 1 untouched");
    }

    #[test]
    fn hvbjoy_reports_auto_joypad_busy_then_clears() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x004200, 0x01);

        let mut saw_busy = false;
        for _ in 0..FRAME_MASTER_CYCLES {
            bus.tick();
            if bus.read(0x004212) & 0x01 != 0 {
                saw_busy = true;
                break;
            }
        }
        assert!(saw_busy, "HVBJOY bit 0 set during the auto-joypad window");

        // Run out the busy window and confirm it clears.
        for _ in 0..4224 {
            bus.tick();
        }
        assert_eq!(bus.read(0x004212) & 0x01, 0, "busy clears after the window");
    }

    #[test]
    fn manual_serial_read_matches_auto_joypad_over_the_bus() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x004200, 0x01);
        bus.set_controller_button(0, SnesButton::Y, true);
        bus.set_controller_button(0, SnesButton::Left, true);
        bus.set_controller_button(0, SnesButton::L, true);

        for _ in 0..(FRAME_MASTER_CYCLES + 4224) {
            bus.tick();
        }
        let auto = (bus.read(0x004219) as u16) << 8 | bus.read(0x004218) as u16;

        // Manual strobe + 16 serial reads of $4016 bit 0.
        bus.write(0x004016, 0x01);
        bus.write(0x004016, 0x00);
        let mut manual = 0u16;
        for _ in 0..16 {
            manual = (manual << 1) | (bus.read(0x004016) & 0x01) as u16;
        }
        assert_eq!(manual, auto);
    }

    #[test]
    fn multitap_pair_select_switches_between_the_two_controller_pairs() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.configure_controllers(SnesControllerType::Standard, SnesControllerType::Multitap);

        // Given four controllers plugged into the multitap on port 2.
        bus.set_controller_button(1, SnesButton::B, true);
        bus.set_controller_button(2, SnesButton::A, true);
        bus.set_controller_button(3, SnesButton::Start, true);
        bus.set_controller_button(4, SnesButton::L, true);

        // When the select line is high, the first pair should be visible.
        bus.write(0x004201, 0x80);
        let high = read_joyb_pair_words(&mut bus);
        assert_eq!(high, (0x8000, 0x0080), "selected pair 2/3 should read back");

        // When the select line is low, the second pair should be visible instead.
        bus.write(0x004201, 0x00);
        let low = read_joyb_pair_words(&mut bus);
        assert_eq!(low, (0x1000, 0x0020), "selected pair 4/5 should read back");
    }

    #[test]
    fn multitap_on_port1_is_rejected_and_falls_back_to_standard() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.configure_controllers(SnesControllerType::Multitap, SnesControllerType::Standard);

        let state = bus.capture_state();
        assert_eq!(state.input.port1_type, SnesControllerType::Standard);
        assert_eq!(state.input.port2_type, SnesControllerType::Standard);
    }

    #[test]
    fn multitap_save_state_round_trips_all_subcontrollers() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.configure_controllers(SnesControllerType::Standard, SnesControllerType::Multitap);
        bus.write(0x004201, 0x80);
        bus.set_controller_button(1, SnesButton::B, true);
        bus.set_controller_button(2, SnesButton::A, true);
        bus.set_controller_button(3, SnesButton::Start, true);
        bus.set_controller_button(4, SnesButton::L, true);

        let state = bus.capture_state();

        let mut restored = SnesSystemBus::new(lorom_test_cart());
        restored.restore_state(&state).expect("restore");
        restored.write(0x004201, 0x80);
        assert_eq!(read_joyb_pair_words(&mut restored), (0x8000, 0x0080));

        restored.write(0x004201, 0x00);
        assert_eq!(read_joyb_pair_words(&mut restored), (0x1000, 0x0020));
    }

    #[test]
    fn multitap_auto_read_uses_the_selected_pair() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.configure_controllers(SnesControllerType::Standard, SnesControllerType::Multitap);
        bus.write(0x004200, 0x01); // enable auto-joypad

        bus.set_controller_button(1, SnesButton::B, true);
        bus.set_controller_button(2, SnesButton::A, true);
        bus.write(0x004201, 0x80);

        for _ in 0..(FRAME_MASTER_CYCLES + 4224) {
            bus.tick();
        }

        let joy2 = (bus.read(0x00421B) as u16) << 8 | bus.read(0x00421A) as u16;
        let joy4 = (bus.read(0x00421F) as u16) << 8 | bus.read(0x00421E) as u16;
        assert_eq!(joy2, 0x8000, "player 2 should be latched into JOY2");
        assert_eq!(joy4, 0x0080, "player 3 should be latched into JOY4");

        bus.write(0x004201, 0x00);
        bus.set_controller_button(3, SnesButton::Start, true);
        bus.set_controller_button(4, SnesButton::L, true);

        for _ in 0..(FRAME_MASTER_CYCLES + 4224) {
            bus.tick();
        }

        let joy2 = (bus.read(0x00421B) as u16) << 8 | bus.read(0x00421A) as u16;
        let joy4 = (bus.read(0x00421F) as u16) << 8 | bus.read(0x00421E) as u16;
        assert_eq!(joy2, 0x1000, "player 4 should be latched into JOY2");
        assert_eq!(joy4, 0x0020, "player 5 should be latched into JOY4");
    }

    #[test]
    fn joyb_grounded_bits_read_one_over_the_bus() {
        let bus = SnesSystemBus::new(lorom_test_cart());
        assert_eq!(bus.read(0x004017) & 0x1C, 0x1C, "$4017 bits 2-4 read 1");
    }

    #[test]
    fn save_state_round_trips_input() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x004200, 0x01); // auto-joypad enabled
        bus.set_controller_button(0, SnesButton::X, true);
        bus.set_controller_button(1, SnesButton::Down, true);
        let state = bus.capture_state();

        let mut restored = SnesSystemBus::new(lorom_test_cart());
        restored.restore_state(&state).expect("restore");
        assert_eq!(
            restored.joypad_button_states(1),
            bus.joypad_button_states(1)
        );
        // X (serial bit 9) survives a full auto-read after restore.
        for _ in 0..(FRAME_MASTER_CYCLES + 4224) {
            restored.tick();
        }
        let joy1 = (restored.read(0x004219) as u16) << 8 | restored.read(0x004218) as u16;
        assert_ne!(joy1 & (1 << 6), 0, "X preserved across save-state");
    }

    #[test]
    fn sa1_iram_mirror_write_is_blocked_until_siwp_enables_its_chunk() {
        let mut bus = SnesSystemBus::new(sa1_test_cart());
        bus.write(0x00_3000, 0x42); // SIWP is $00 at reset: write must be dropped.
        assert_eq!(bus.read(0x00_3000), 0x00);

        bus.write(0x00_2229, 0x01); // enable chunk 0 ($3000-$30FF)
        bus.write(0x00_3000, 0x42);
        assert_eq!(bus.read(0x00_3000), 0x42);
    }

    #[test]
    fn sa1_iram_mirror_is_visible_through_the_debugger_read_path() {
        let mut bus = SnesSystemBus::new(sa1_test_cart());
        bus.write(0x00_2229, 0xFF);
        bus.write(0x80_31FF, 0x7A); // banks $80-$BF mirror the same window
        assert_eq!(bus.read_for_debugger(0x00_31FF), 0x7A);
    }

    #[test]
    fn sa1_iram_mirror_is_reachable_as_a_general_dma_a_bus_source() {
        // Modeled directly on `dma_a_to_b_wmdata_writes_wram_and_advances_wmadd` above, with the
        // A-bus source moved from plain WRAM to the SA-1 I-RAM mirror.
        let mut bus = SnesSystemBus::new(sa1_test_cart());
        bus.write(0x00_2229, 0xFF); // write-enable all I-RAM chunks from the SNES side
        bus.write(0x00_3000, 0x55); // I-RAM source byte, via the mirror window

        // Point WMDATA's auto-increment pointer at $000010.
        bus.write(0x002181, 0x10);
        bus.write(0x002182, 0x00);
        bus.write(0x002183, 0x00);

        write_dma_channel(&mut bus, 0, 0x00, 0x80, 0x00_3000, 1);
        bus.write(0x00420B, 0x01);

        assert_eq!(bus.read(0x7E0010), 0x55);
    }

    #[test]
    fn non_sa1_cartridge_iram_mirror_addresses_are_open_bus() {
        let mut bus = SnesSystemBus::new(lorom_test_cart());
        bus.write(0x00_3000, 0x42); // no SA-1 I-RAM backing this cartridge: must be a no-op
        assert_eq!(bus.read(0x00_3000), 0x00);
    }

    #[test]
    fn save_state_round_trips_sa1_registers_iram_and_cpu() {
        let mut bus = SnesSystemBus::new(sa1_test_cart());
        // I-RAM: write-enable only chunks 0-1 ($3000-$31FF) and store distinct marker bytes,
        // leaving chunk 7 ($3700-$37FF) protected so the protection state itself is exercised.
        bus.write(0x00_2229, 0x03);
        bus.write(0x00_3000, 0x11);
        bus.write(0x00_3100, 0x22);
        // Boot SA-1: point its reset vector at a real (if trivial) idle loop and release it.
        bus.write(0x00_2203, 0x00);
        bus.write(0x00_2204, 0x80); // SA-1 reset vector = $8000 (ROM is zeroed: JMP $0000 loop)
        bus.write(0x00_2200, 0x00);
        for _ in 0..300 {
            bus.tick();
        }
        let pc_before = bus.sa1_cpu_pc_for_tests();
        let a_before = bus.sa1_cpu_a_for_tests();

        let state = bus.capture_state();

        let mut restored = SnesSystemBus::new(sa1_test_cart());
        restored.restore_state(&state).expect("restore");

        assert_eq!(restored.read_for_debugger(0x00_3000), 0x11);
        assert_eq!(restored.read_for_debugger(0x00_3100), 0x22);
        assert_eq!(restored.sa1_cpu_pc_for_tests(), pc_before);
        assert_eq!(restored.sa1_cpu_a_for_tests(), a_before);

        // The restored I-RAM protection state must survive too: a chunk that was never
        // enabled stays protected.
        restored.write(0x00_3700, 0x99);
        assert_eq!(restored.read(0x00_3700), 0x00);
    }

    #[test]
    fn save_state_round_trip_is_a_no_op_for_non_sa1_cartridges() {
        let bus = SnesSystemBus::new(lorom_test_cart());
        let state = bus.capture_state();
        assert_eq!(state.sa1, None);

        let mut restored = SnesSystemBus::new(lorom_test_cart());
        restored.restore_state(&state).expect("restore");
        assert_eq!(restored.sa1_cpu_pc_for_tests(), None);
    }

    #[test]
    fn sa1_bwram_windowed_write_succeeds_once_either_side_enables_writes() {
        let mut bus = SnesSystemBus::new(sa1_test_cart_with_bwram());
        bus.write(0x00_6000, 0x42); // BWPA defaults to $FF (protect everything); write dropped.
        assert_eq!(bus.read(0x00_6000), 0x00);

        // Confirmed against bsnes (KDL3 quirk comment in SA1::BWRAM::writeLinear): BWPA only
        // protects when *both* SBWE and CBWE are disabled -- enabling just one side already
        // lifts protection entirely, it isn't independent per-side gating like I-RAM's.
        bus.write(0x00_2226, 0x80); // SNES side enables writes; SA-1 side (CBWE) stays disabled.
        bus.write(0x00_6000, 0x42);
        assert_eq!(bus.read(0x00_6000), 0x42);
    }

    #[test]
    fn sa1_bwram_windowed_write_succeeds_once_bwpa_shrinks_below_the_target_offset() {
        let mut bus = SnesSystemBus::new(sa1_test_cart_with_bwram());
        bus.write(0x00_2228, 0x00); // protect only the first 256 bytes
        bus.write(0x00_6100, 0x42); // offset $100, outside the protected region
        assert_eq!(bus.read(0x00_6100), 0x42);
    }

    #[test]
    fn sa1_bwram_windowed_access_honors_bmaps_block_select() {
        let mut bus = SnesSystemBus::new(sa1_test_cart_with_bwram());
        bus.write(0x00_2228, 0x00); // shrink protection so the write below succeeds
        bus.write(0x00_2224, 0x01); // BMAPS: block 1 ($2000-$3FFF of BW-RAM)
        bus.write(0x00_6000, 0x77);
        // The same byte must be visible directly at BW-RAM offset $2000 (bank $40).
        assert_eq!(bus.read(0x40_2000), 0x77);
    }

    #[test]
    fn sa1_bwram_write_protection_is_checked_against_the_linear_bus_offset_before_wrapping() {
        // BWPA's comparator sits on the address bus (fullsnes: protected bytes are "originated
        // at 400000h"), so it sees the *linear* BW-RAM offset; wrapping onto the smaller
        // physical chip happens afterward, at the RAM's own address pins. A mirrored write
        // beyond the protected linear range therefore SUCCEEDS -- and physically lands on a
        // protected byte via wraparound. This is real, conformance-tested hardware behavior:
        // absindx `SA1RamProtectionTest` TEST ID 50 sets BWPA=$09 (protects $400000-$41FFFF,
        // exactly its cart's full 128KB) and requires the probe write at $420000 to stick
        // (readable back through the wrap) for the reported protection area to come out as $09.
        let mut bus = SnesSystemBus::new(sa1_test_cart_with_bwram());
        bus.write(0x00_2228, 0x00); // protect only the first 256 bytes ($400000-$4000FF)
        bus.write(0x00_2224, 0x04); // BMAPS: block 4 (linear $8000; wraps to physical 0 in 32KB)
        bus.write(0x00_6000, 0x42);
        assert_eq!(
            bus.read(0x00_6000),
            0x42,
            "linear offset $8000 is outside BWPA's range, so the write lands (wrapped)"
        );
        // The direct view of the wrapped physical byte sees the same value...
        assert_eq!(bus.read(0x40_0000), 0x42);
        // ...but a write addressed *inside* the protected linear range is still blocked.
        bus.write(0x40_0000, 0x99);
        assert_eq!(bus.read(0x40_0000), 0x42);
    }

    #[test]
    fn sa1_reset_release_clears_ciwp_but_not_snes_side_protection_registers() {
        // bsnes `SA1::writeIOCPU` case $2200: on the reset-RELEASE edge (resb held, then a CCNT
        // write with bit 5 clear), "CIWP is set to 0 at reset" -- and only CIWP; the SNES-side
        // SIWP survives. absindx `SA1RamProtectionTest` TEST ID 221 (`* reset? CIWP = $00`)
        // sets CIWP=$33 before rebooting SA-1 and expects the post-reboot SA-1-side probe to
        // find nothing writable, while TEST 219 expects the pre-reboot SIWP=$AA still in force.
        let mut bus = SnesSystemBus::new(sa1_test_cart());
        bus.sa1_iram
            .as_ref()
            .unwrap()
            .borrow_mut()
            .set_sa1_write_protect(0x33);
        bus.write(0x00_2229, 0xAA); // SIWP (SNES-side)

        bus.write(0x00_2200, 0x20); // re-assert reset (power-on default is also held)
        bus.write(0x00_2200, 0x00); // release: the edge that clears CIWP

        let iram = bus.sa1_iram.as_ref().unwrap().borrow();
        assert_eq!(iram.sa1_write_protect_raw(), 0x00, "CIWP cleared by reset");
        assert_eq!(iram.snes_write_protect_raw(), 0xAA, "SIWP survives");
    }

    #[test]
    fn sa1_ccnt_write_without_a_reset_release_edge_leaves_ciwp_alone() {
        let mut bus = SnesSystemBus::new(sa1_test_cart());
        bus.write(0x00_2200, 0x00); // release reset (power-on default is held)
        bus.sa1_iram
            .as_ref()
            .unwrap()
            .borrow_mut()
            .set_sa1_write_protect(0x33);

        bus.write(0x00_2200, 0x05); // message-only write while already released
        assert_eq!(
            bus.sa1_iram
                .as_ref()
                .unwrap()
                .borrow()
                .sa1_write_protect_raw(),
            0x33,
            "no reset-release edge, CIWP untouched"
        );
    }

    #[test]
    fn sa1_bwram_direct_banks_cover_the_full_range() {
        let mut bus = SnesSystemBus::new(sa1_test_cart_with_bwram());
        bus.write(0x00_2228, 0x00); // shrink protection
        bus.write(0x00_2226, 0x80); // and enable writes outright
        bus.write(0x41_0000, 0x99); // bank $41 = BW-RAM offset $10000
        assert_eq!(bus.read(0x41_0000), 0x99);
    }

    #[test]
    fn sa1_rom_banking_hirom_quarter_always_honors_the_bank_select_field() {
        let mut rom = vec![0u8; 0x20_0000]; // 2MB: room for ROM slots 0 and 1
        write_sa1_header(&mut rom);
        rom[0x10_0000..0x10_0003].copy_from_slice(&[0x11, 0x22, 0x33]); // ROM slot 1
        let cartridge = Cartridge::from_bytes(&rom).expect("valid SA-1 ROM");
        let mut bus = SnesSystemBus::new(cartridge);

        bus.write(0x00_2220, 0x01); // CXB: slot 1 (bit 7 clear)
        assert_eq!(bus.read(0xC0_0000), 0x11);
        assert_eq!(bus.read(0xC0_0001), 0x22);
    }

    #[test]
    fn sa1_rom_banking_lorom_quarter_remaps_once_bit7_is_set() {
        let mut rom = vec![0u8; 0x20_0000]; // 2MB: room for ROM slots 0 and 1
        write_sa1_header(&mut rom);
        rom[0x10_0000] = 0xAB; // ROM slot 1, byte 0
        let cartridge = Cartridge::from_bytes(&rom).expect("valid SA-1 ROM");
        let mut bus = SnesSystemBus::new(cartridge);

        bus.write(0x00_2220, 0x81); // CXB: slot 1, bit 7 set (remap LoROM too)
        assert_eq!(bus.read(0x00_8000), 0xAB);
    }

    #[test]
    fn sa1_irq_vector_override_redirects_00ffee_ffef_to_siv() {
        let bus = SnesSystemBus::new(sa1_test_cart());
        // Before the override switch (SCNT bit 6) is set, $00FFEE/FFEF read real ROM (zeroed).
        assert_eq!(bus.read(0x00_FFEE), 0x00);

        bus.write_sa1_side_register_for_tests(0x220E, 0x34); // SIV low
        bus.write_sa1_side_register_for_tests(0x220F, 0x12); // SIV high
        bus.write_sa1_side_register_for_tests(0x2209, 0x40); // SCNT bit 6: enable IRQ vector override
        assert_eq!(bus.read(0x00_FFEE), 0x34);
        assert_eq!(bus.read(0x00_FFEF), 0x12);
        assert_eq!(
            bus.read_for_debugger(0x00_FFEE),
            0x34,
            "debugger path sees the same override"
        );
    }

    #[test]
    fn sa1_nmi_vector_override_redirects_00ffea_ffeb_to_snv() {
        let bus = SnesSystemBus::new(sa1_test_cart());
        bus.write_sa1_side_register_for_tests(0x220C, 0x78); // SNV low
        bus.write_sa1_side_register_for_tests(0x220D, 0x56); // SNV high
        bus.write_sa1_side_register_for_tests(0x2209, 0x10); // SCNT bit 4: enable NMI vector override
        assert_eq!(bus.read(0x00_FFEA), 0x78);
        assert_eq!(bus.read(0x00_FFEB), 0x56);
    }

    #[test]
    fn sa1_vector_override_does_not_affect_non_sa1_cartridges() {
        let bus = SnesSystemBus::new(lorom_test_cart());
        assert_eq!(bus.read(0x00_FFEE), 0x00);
    }

    #[test]
    fn sa1_irq_from_sa1_asserts_main_bus_poll_irq_once_enabled() {
        let mut bus = SnesSystemBus::new(sa1_test_cart());
        bus.write_sa1_side_register_for_tests(0x2209, 0x80); // SCNT: SA-1 raises IRQ
        assert!(!bus.poll_irq(), "SIE not yet enabled");

        bus.write(0x00_2201, 0x80); // SIE: SNES side enables IRQ-from-SA-1
        assert!(bus.poll_irq());

        bus.write(0x00_2202, 0x80); // SIC: acknowledge
        assert!(!bus.poll_irq());
    }

    #[test]
    fn sa1_sfr_reflects_message_and_pending_from_scnt() {
        let bus = SnesSystemBus::new(sa1_test_cart());
        bus.write_sa1_side_register_for_tests(0x2209, 0x87); // message=7, IRQ trigger
        assert_eq!(bus.read(0x00_2300), 0x87);
    }

    #[test]
    fn sa1_230e_version_code_register_always_reads_open_bus() {
        // Confirmed by bsnes's `SA1::readIOCPU` ("does not actually exist on real hardware ...
        // always returns open bus") and fullsnes ("Existing value(s) are unknown") -- unlike
        // every other SA-1 register, there is deliberately no read arm for $230E at all.
        let bus = SnesSystemBus::new(sa1_test_cart());
        bus.mdr.set(0xA5);
        assert_eq!(bus.read(0x00_230E), 0xA5);
    }

    /// Writes a minimal valid SA-1 LoROM header into `rom` (64 KiB+, chipset `$35`), matching
    /// the pattern in [`sa1_test_cart`] but as a free function so ROM-banking tests can build a
    /// larger backing buffer (needed to address ROM slot 1 at file offset `$100000`).
    fn write_sa1_header(rom: &mut [u8]) {
        let base = 0x7FC0;
        rom[base..base + 21].copy_from_slice(b"SYSTEM BUS TEST      ");
        rom[base + 0x3C] = 0x00;
        rom[base + 0x3D] = 0x80;
        rom[base + 0x15] = 0x20;
        rom[base + 0x16] = 0x35;
        rom[base + 0x17] = 0x07;
        rom[base + 0x18] = 0x00;
        rom[base + 0x1C] = 0x34;
        rom[base + 0x1D] = 0x12;
        rom[base + 0x1E] = 0xCB;
        rom[base + 0x1F] = 0xED;
    }

    #[test]
    fn save_state_round_trips_sa1_memory_control_and_bwram() {
        let mut bus = SnesSystemBus::new(sa1_test_cart_with_bwram());
        bus.write(0x00_2228, 0x00); // shrink BWPA protection
        bus.write(0x00_2226, 0x80); // SBWE
        bus.write(0x00_2224, 0x03); // BMAPS
        bus.write(0x00_2220, 0x81); // CXB: slot 1, LoROM remap enabled
        bus.write(0x00_6000, 0x5A); // BW-RAM byte via the (now block-3) window

        let state = bus.capture_state();
        let mut restored = SnesSystemBus::new(sa1_test_cart_with_bwram());
        restored.restore_state(&state).expect("restore");

        assert_eq!(restored.read(0x00_6000), 0x5A);
        // Protection state must survive: still shrunk to 256 bytes, still write-enabled.
        restored.write(0x00_6100, 0x11);
        assert_eq!(restored.read(0x00_6100), 0x11);
    }

    #[test]
    fn save_state_round_trips_sa1_cross_cpu_interrupt_pending_flags() {
        let mut bus = SnesSystemBus::new(sa1_test_cart());
        bus.write(0x00_2200, 0x90); // CCNT: latch sa1_irq_pending and sa1_nmi_pending
        bus.write_sa1_side_register_for_tests(0x2209, 0x80); // SCNT: latch snes_irq_pending

        let state = bus.capture_state();
        let mut restored = SnesSystemBus::new(sa1_test_cart());
        restored.restore_state(&state).expect("restore");

        let registers = restored
            .sa1_registers
            .as_ref()
            .expect("SA-1 cart should have control registers")
            .borrow();
        assert!(registers.sa1_irq_pending());
        assert!(registers.sa1_nmi_pending());
        assert!(registers.snes_irq_pending());
    }
}
