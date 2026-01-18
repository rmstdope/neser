use crate::cartridge::{Cartridge, MirroringMode};
use crate::nes::TvSystem;
use crate::ppu::{Background, Memory, Registers, Rendering, Sprites, Status, Timing};
use std::cell::RefCell;
use std::rc::Rc;

mod tick;

/// Refactored PPU using modular components
pub struct Ppu {
    /// Timing and cycle management
    timing: Timing,
    /// Status flags (VBlank, sprite 0 hit, NMI)
    status: Status,

    // If PPUSTATUS is read right at the time VBlank would be set, the NES can
    // suppress the VBlank flag for the entire frame (blargg ppu_vbl_nmi 02).
    vblank_suppressed_for_frame: bool,

    // Internal VBlank latch used specifically for immediate-NMI enable behavior.
    // This is intentionally distinct from the readable $2002 VBlank flag to model
    // subtle boundary timing near VBlank end (blargg ppu_vbl_nmi 07).
    vblank_for_nmi: bool,
    /// Register management (PPUCTRL, PPUMASK, Loopy registers)
    registers: Registers,
    /// Memory management (VRAM, palette, CHR ROM)
    memory: Memory,
    /// Background rendering
    background: Background,
    /// Sprite rendering
    sprites: Sprites,
    /// Final rendering and screen output
    rendering: Rendering,
    /// Previous A12 state for change detection (bit 12 of PPU address)
    prev_a12: bool,
    /// Cartridge reference for dynamic CHR ROM/RAM access through mapper
    ///
    /// The PPU holds a shared reference to the cartridge to enable:
    /// - Real-time CHR bank switching during rendering
    /// - Proper mapper integration for pattern table access
    /// - Hardware-accurate CHR-ROM and CHR-RAM behavior
    cartridge: Option<Rc<RefCell<Cartridge>>>,
}

impl Ppu {
    fn with_mapper_mut<F>(&mut self, f: F)
    where
        F: FnOnce(&mut dyn crate::cartridge::Mapper),
    {
        if let Some(ref cartridge) = self.cartridge {
            let mut cartridge = cartridge.borrow_mut();
            f(cartridge.mapper_mut());
        }
    }

    fn notify_chr_fetch_kind(cartridge: &Option<Rc<RefCell<Cartridge>>>, is_sprite: bool) {
        if let Some(cartridge) = cartridge {
            cartridge
                .borrow_mut()
                .mapper_mut()
                .ppu_set_chr_fetch_is_sprite(is_sprite);
        }
    }

    fn notify_chr_fetch_is_ppudata(cartridge: &Option<Rc<RefCell<Cartridge>>>) {
        if let Some(cartridge) = cartridge {
            cartridge
                .borrow_mut()
                .mapper_mut()
                .ppu_set_chr_fetch_is_ppudata();
        }
    }

    fn set_vblank_for_nmi(&mut self) {
        self.vblank_for_nmi = true;
    }

    fn clear_vblank_for_nmi(&mut self) {
        self.vblank_for_nmi = false;
    }

    /// Create a new modular PPU instance
    pub fn new(tv_system: TvSystem) -> Self {
        Self {
            timing: Timing::new(tv_system),
            status: Status::new(),
            vblank_suppressed_for_frame: false,
            vblank_for_nmi: false,
            registers: Registers::new(),
            memory: Memory::new(),
            background: Background::new(),
            sprites: Sprites::new(),
            rendering: Rendering::new(),
            prev_a12: false,
            cartridge: None,
        }
    }

    /// Reset the PPU to its initial state
    pub fn reset(&mut self) {
        self.timing.reset();
        self.status.reset();
        self.vblank_suppressed_for_frame = false;
        self.vblank_for_nmi = false;
        self.registers.reset();
        self.memory.reset();
        self.background.reset();
        self.sprites.reset();
        self.prev_a12 = false;
    }

    pub fn io_bus(&self) -> u8 {
        self.registers.io_bus()
    }

    pub fn set_io_bus(&mut self, value: u8) {
        self.registers.set_io_bus(value);
    }

    /// Run the PPU for a specified number of cycles
    pub fn run_ppu_cycles(&mut self, cycles: u64) {
        for _ in 0..cycles {
            self.tick();
        }
    }

    /// Process a single PPU cycle
    fn tick(&mut self) {
        tick::tick(self);
    }

    /// Write to control register ($2000)
    pub fn write_control(&mut self, value: u8) {
        let nmi_was_enabled = self.registers.should_generate_nmi();

        self.registers.write_control(value);
        self.registers.set_io_bus(value); // Update I/O bus

        let nmi_is_enabled = self.registers.should_generate_nmi();

        // NMI-off timing quirk: disabling NMI right around VBlank start can suppress
        // the VBlank NMI edge (blargg ppu_vbl_nmi 08).
        let is_disabling_nmi_at_vblank_nmi_latch_dot = nmi_was_enabled
            && !nmi_is_enabled
            && self.timing.scanline() == 241
            && self.timing.pixel() == 2;
        if is_disabling_nmi_at_vblank_nmi_latch_dot {
            self.status.clear_nmi();
        }

        // If NMI is enabled during VBlank (0→1 transition while VBlank flag is set),
        // the PPU should immediately assert an NMI edge.
        if !nmi_was_enabled && nmi_is_enabled && self.vblank_for_nmi {
            self.status.trigger_nmi();
        }
    }

    /// Write to mask register ($2001)
    pub fn write_mask(&mut self, value: u8) {
        self.registers.write_mask(value);
        self.registers.set_io_bus(value); // Update I/O bus
    }

    /// Read status register ($2002)
    pub fn get_status(&mut self) -> u8 {
        let scanline = self.timing.scanline();
        let pixel = self.timing.pixel();

        // VBlank suppression quirk: if $2002 is read right as VBlank is being set,
        // the flag can be suppressed for the frame.
        if scanline == 241 && (pixel == 0 || pixel == 1) {
            self.vblank_suppressed_for_frame = true;
        }

        // NMI suppression quirk: reading $2002 shortly after VBlank starts can prevent
        // the VBlank NMI edge from being observed.
        if scanline == 241 && (pixel == 2 || pixel == 3) {
            self.status.clear_nmi();
        }

        let status = self.status.read_status();
        if (status & 0x80) != 0 {
            // Reading $2002 clears the visible VBlank flag, and it should also prevent
            // any further immediate-NMI enable edge from being generated this VBlank.
            self.clear_vblank_for_nmi();
        }
        self.registers.clear_w(); // Reading status clears write toggle
        // Update I/O bus: status bits go to bits 5-7, bits 0-4 remain from previous value
        let io_bus = self.registers.io_bus();
        let new_io_bus = (status & 0xE0) | (io_bus & 0x1F);
        // Only refresh bits 5-7, not bits 0-4
        self.registers.set_io_bus_with_mask(new_io_bus, 0xE0);
        new_io_bus
    }

    /// Write to scroll register ($2005)
    pub fn write_scroll(&mut self, value: u8, is_dummy_write: bool) {
        self.registers.write_scroll(value, is_dummy_write);
        self.registers.set_io_bus(value); // Update I/O bus
    }

    /// Write to address register ($2006)
    pub fn write_address(&mut self, value: u8, is_dummy_write: bool) {
        let old_v = self.registers.v();
        self.registers.write_address(value, is_dummy_write);
        self.registers.set_io_bus(value); // Update I/O bus

        // Notify mapper if v register changed (happens on second write to $2006)
        // This is needed for MMC3 A12 detection when manually toggling address
        let new_v = self.registers.v();
        if old_v != new_v
            && let Some(ref cartridge) = self.cartridge
        {
            // When manually changing the PPU address via $2006, we need to ensure
            // the MMC3 A12 filter has enough "cycles" to detect the change properly.
            // We simulate this by calling ppu_address_changed with the old address
            // multiple times before notifying about the new address.
            let mapper = &mut *cartridge.borrow_mut();
            for _ in 0..8 {
                mapper.mapper_mut().ppu_address_changed(old_v);
            }
            mapper.mapper_mut().ppu_address_changed(new_v);
        }
    }

    /// Read from data register ($2007)
    pub fn read_data(&mut self) -> u8 {
        let addr = self.registers.v();
        let result = match addr {
            0x0000..=0x1FFF => {
                // CHR ROM: buffered read
                // Notify mapper this is a PPUDATA read, not a rendering fetch
                // (MMC5 extended attribute mode should NOT apply here)
                Self::notify_chr_fetch_is_ppudata(&self.cartridge);
                let buffered = self.registers.data_buffer();
                self.registers
                    .set_data_buffer(self.memory.read_chr(addr, &self.cartridge));
                buffered
            }
            0x2000..=0x3EFF => {
                // Nametable: buffered read
                let buffered = self.registers.data_buffer();
                self.registers
                    .set_data_buffer(self.memory.read_nametable_mapped(addr, &self.cartridge));
                buffered
            }
            0x3F00..=0x3FFF => {
                // Palette: immediate read
                // Bits 5-0 come from palette, bits 7-6 from open bus
                let palette_data = self.memory.read_palette(addr);
                // Update buffer with nametable data underneath
                let mirrored_addr = addr & 0x2FFF;
                self.registers.set_data_buffer(
                    self.memory
                        .read_nametable_mapped(mirrored_addr, &self.cartridge),
                );
                // Combine palette data (bits 5-0) with open bus (bits 7-6)
                let io_bus = self.registers.io_bus();
                (io_bus & 0xC0) | (palette_data & 0x3F)
            }
            _ => self.registers.data_buffer(),
        };

        // Use rendering glitch during active rendering
        let old_addr = self.registers.v();
        if self.should_use_rendering_glitch() {
            self.registers.inc_address_with_rendering_glitch();
        } else {
            self.registers.increment_vram_address();
        }

        // Notify mapper of address change after increment (for MMC3 A12 detection)
        let new_addr = self.registers.v();
        if old_addr != new_addr
            && let Some(ref cartridge) = self.cartridge
        {
            // For PPUDATA reads/writes, we need to prime the A12 filter similar
            // to PPUADDR writes, as these are also manual address changes
            let mapper = &mut *cartridge.borrow_mut();
            for _ in 0..8 {
                mapper.mapper_mut().ppu_address_changed(old_addr);
            }
            mapper.mapper_mut().ppu_address_changed(new_addr);
        }

        // Update I/O bus with value read
        // For palette reads (addr 0x3F00-0x3FFF), only refresh bits 5-0
        // For other reads, refresh all 8 bits
        if (0x3F00..=0x3FFF).contains(&addr) {
            self.registers.set_io_bus_with_mask(result, 0x3F);
        } else {
            self.registers.set_io_bus(result);
        }
        result
    }

    /// Write to data register ($2007)
    pub fn write_data(&mut self, value: u8) {
        self.registers.set_io_bus(value); // Update I/O bus
        let addr = self.registers.v();
        match addr {
            0x0000..=0x1FFF => {
                // CHR memory - routes through mapper for ROM/RAM handling
                self.memory.write_chr(addr, value, &self.cartridge);
            }
            0x2000..=0x3EFF => {
                self.memory
                    .write_nametable_mapped(addr, value, &self.cartridge);
            }
            0x3F00..=0x3FFF => {
                self.memory.write_palette(addr, value);
            }
            _ => {}
        }

        // Use rendering glitch during active rendering
        let old_addr = self.registers.v();
        if self.should_use_rendering_glitch() {
            self.registers.inc_address_with_rendering_glitch();
        } else {
            self.registers.increment_vram_address();
        }

        // Notify mapper of address change after increment (for MMC3 A12 detection)
        let new_addr = self.registers.v();
        if old_addr != new_addr
            && let Some(ref cartridge) = self.cartridge
        {
            // For PPUDATA reads/writes, we need to prime the A12 filter similar
            // to PPUADDR writes, as these are also manual address changes
            let mapper = &mut *cartridge.borrow_mut();
            for _ in 0..8 {
                mapper.mapper_mut().ppu_address_changed(old_addr);
            }
            mapper.mapper_mut().ppu_address_changed(new_addr);
        }
    }

    /// Set the cartridge reference for dynamic CHR ROM/RAM access
    ///
    /// This establishes the connection between the PPU and the cartridge mapper,
    /// enabling hardware-accurate CHR access with bank switching support.
    /// Must be called after inserting a cartridge into the system.
    pub fn set_cartridge(&mut self, cartridge: Rc<RefCell<Cartridge>>) {
        self.cartridge = Some(cartridge);
    }

    /// Set mirroring mode
    pub fn set_mirroring(&mut self, mirroring: MirroringMode) {
        self.memory.set_mirroring(mirroring);
    }

    /// Poll NMI
    pub fn poll_nmi(&mut self) -> bool {
        self.status.poll_nmi()
    }

    /// Poll frame complete
    pub fn poll_frame_complete(&mut self) -> bool {
        self.status.poll_frame_complete()
    }

    /// Get current scanline
    pub fn scanline(&self) -> u16 {
        self.timing.scanline()
    }

    /// Get current pixel
    pub fn pixel(&self) -> u16 {
        self.timing.pixel()
    }

    /// Write to OAM address register ($2003)
    pub fn write_oam_address(&mut self, value: u8) {
        self.registers.oam_address = value;
        self.registers.set_io_bus(value); // Update I/O bus
    }

    /// Write to OAM data register ($2004)
    pub fn write_oam_data(&mut self, value: u8) {
        self.registers.set_io_bus(value); // Update I/O bus
        let is_rendering = self.is_actively_rendering();

        // During rendering, writes to OAMDATA are ignored (but address still increments)
        if !is_rendering {
            self.sprites.write_oam(self.registers.oam_address, value);
            // Normal increment: add 1
            self.registers.oam_address = self.registers.oam_address.wrapping_add(1);
        } else {
            // Glitchy increment during rendering: increment only the high 6 bits (add 4)
            // This preserves the low 2 bits and bumps the sprite index
            let low_bits = self.registers.oam_address & 0x03;
            let high_bits = self.registers.oam_address.wrapping_add(4) & 0xFC;
            self.registers.oam_address = high_bits | low_bits;
        }
    }

    /// Read from OAM data register ($2004)
    pub fn read_oam_data(&mut self) -> u8 {
        let value = self.sprites.read_oam(self.registers.oam_address);
        self.registers.set_io_bus(value); // Update I/O bus
        value
    }

    /// Get reference to screen buffer
    #[cfg(test)]
    pub fn screen_buffer(&self) -> &super::screen_buffer::ScreenBuffer {
        self.rendering.screen_buffer()
    }

    /// Get mutable reference to screen buffer (for compatibility)
    pub fn screen_buffer_mut(&mut self) -> &mut super::screen_buffer::ScreenBuffer {
        self.rendering.screen_buffer_mut()
    }

    /// Check if in VBlank period
    #[cfg(test)]
    pub fn is_in_vblank(&self) -> bool {
        self.status.is_in_vblank()
    }

    /// Check if should generate NMI
    #[cfg(test)]
    pub fn should_generate_nmi(&self) -> bool {
        self.registers.should_generate_nmi()
    }

    /// Check if PPUDATA access should trigger the rendering glitch
    /// Returns true if rendering is enabled and we're on a visible scanline
    fn should_use_rendering_glitch(&self) -> bool {
        let scanline = self.timing.scanline();
        let is_visible_scanline = scanline < 240;
        self.registers.is_rendering_enabled() && is_visible_scanline
    }

    /// Check if PPU is currently on a rendering scanline (visible or pre-render)
    /// Returns true if we're on scanlines 0-239 or the pre-render scanline
    fn is_on_rendering_scanline(&self) -> bool {
        let scanline = self.timing.scanline();
        let prerender_scanline = match self.timing.tv_system() {
            TvSystem::Ntsc => 261,
            TvSystem::Pal => 311,
        };
        let is_visible_scanline = scanline < 240;
        let is_prerender = scanline == prerender_scanline;
        is_visible_scanline || is_prerender
    }

    /// Check if PPU is actively rendering (rendering enabled + on rendering scanline)
    fn is_actively_rendering(&self) -> bool {
        self.registers.is_rendering_enabled() && self.is_on_rendering_scanline()
    }

    /// Get total cycles (for testing)
    #[cfg(test)]
    pub fn total_cycles(&self) -> u64 {
        self.timing.total_cycles()
    }

    /// Get v register (for testing)
    #[cfg(test)]
    pub fn v_register(&self) -> u16 {
        self.registers.v()
    }

    /// Read nametable for debugging/testing (doesn't affect PPU state)
    #[cfg(test)]
    pub fn read_nametable_for_debug(&self, addr: u16) -> u8 {
        self.memory.read_nametable_mapped(addr, &self.cartridge)
    }

    /// Get base nametable address from PPUCTRL (for testing)
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn base_nametable_addr(&self) -> u16 {
        self.registers.base_nametable_addr()
    }

    /// Get w register (for testing)
    #[cfg(test)]
    pub fn w_register(&self) -> bool {
        self.registers.w()
    }

    /// Get OAM address register (for testing)
    #[cfg(test)]
    pub fn oam_address(&self) -> u8 {
        self.registers.oam_address
    }

    /// Check if A12 changed from 0 to 1 (rising edge)
    /// This is used for mapper IRQ counters (e.g., MMC3)
    /// Returns true if A12 went from 0 to 1
    #[cfg(test)]
    fn check_a12_rising_edge(&mut self, addr: u16) -> bool {
        let current_a12 = (addr & 0x1000) != 0;
        let rising_edge = !self.prev_a12 && current_a12;
        self.prev_a12 = current_a12;
        rising_edge
    }

    /// Capture the current PPU state for save-state.
    pub fn capture_state(&self) -> crate::savestate::PpuState {
        crate::savestate::PpuState {
            timing: crate::savestate::PpuTimingState {
                scanline: self.timing.scanline,
                pixel: self.timing.pixel,
                total_cycles: self.timing.total_cycles(),
                frame_count: self.timing.frame_count(),
            },
            registers: crate::savestate::PpuRegisterState {
                control: self.registers.control(),
                mask: self.registers.mask(),
                oam_addr: self.registers.oam_address,
                v: self.registers.v(),
                t: self.registers.t(),
                fine_x: self.registers.x(),
                w: self.registers.w(),
                io_bus: self.registers.io_bus(),
            },
            vblank_flag: self.status.is_in_vblank(),
            sprite_zero_hit: self.status.sprite_0_hit(),
            sprite_overflow: self.status.sprite_overflow(),
            nmi_occurred: self.status.nmi_occurred(),
            vram: self.memory.vram_snapshot(),
            palette: self.memory.palette_snapshot(),
            oam: self.sprites.oam_snapshot(),
            secondary_oam: self.sprites.secondary_oam_snapshot(),
            read_buffer: self.registers.data_buffer(),
        }
    }

    /// Restore PPU state from a save-state.
    pub fn restore_state(&mut self, state: &crate::savestate::PpuState) {
        // Restore timing
        self.timing.restore_state(state.timing.scanline, state.timing.pixel, state.timing.total_cycles, state.timing.frame_count);

        // Restore registers
        self.registers.restore_state(
            state.registers.control,
            state.registers.mask,
            state.registers.oam_addr,
            state.registers.v,
            state.registers.t,
            state.registers.fine_x,
            state.registers.w,
            state.registers.io_bus,
            state.read_buffer,
        );

        // Restore memory
        self.memory.restore_vram(&state.vram);
        self.memory.restore_palette(&state.palette);

        // Restore OAM
        self.sprites.restore_oam(&state.oam, &state.secondary_oam);

        // Restore status flags
        self.status.restore_state(state.vblank_flag, state.sprite_zero_hit, state.sprite_overflow, state.nmi_occurred);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ScanlineSpyMapper {
        calls: Rc<RefCell<Vec<(u16, bool)>>>,
    }

    impl crate::cartridge::Mapper for ScanlineSpyMapper {
        fn read_prg(&self, _addr: u16) -> u8 {
            0
        }

        fn write_prg(&mut self, _addr: u16, _value: u8) {}

        fn read_chr(&self, _addr: u16) -> u8 {
            0
        }

        fn write_chr(&mut self, _addr: u16, _value: u8) {}

        fn ppu_address_changed(&mut self, _addr: u16) {}

        fn ppu_scanline(&mut self, scanline: u16, rendering_enabled: bool) {
            self.calls.borrow_mut().push((scanline, rendering_enabled));
        }

        fn get_mirroring(&self) -> MirroringMode {
            MirroringMode::Horizontal
        }
    }

    #[test]
    fn test_ppu_new() {
        let ppu = Ppu::new(TvSystem::Ntsc);
        assert_eq!(ppu.scanline(), 0);
        assert_eq!(ppu.pixel(), 0);
    }

    #[test]
    fn test_prerender_scanline_helper() {
        assert_eq!(tick::prerender_scanline(TvSystem::Ntsc), 261);
        assert_eq!(tick::prerender_scanline(TvSystem::Pal), 311);
    }

    #[test]
    fn test_ppu_io_bus_round_trip() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        ppu.set_io_bus(0x5A);

        assert_eq!(ppu.io_bus(), 0x5A);
    }

    #[test]
    fn test_mapper_ppu_scanline_is_called_on_scanline_boundaries() {
        let calls: Rc<RefCell<Vec<(u16, bool)>>> = Rc::new(RefCell::new(Vec::new()));

        let cart = Rc::new(RefCell::new(Cartridge::from_mapper_for_test(Box::new(
            ScanlineSpyMapper {
                calls: calls.clone(),
            },
        ))));

        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.set_cartridge(cart);

        // Enable rendering so the mapper sees rendering_enabled = true.
        ppu.write_mask(0x18);

        // Run one full scanline worth of cycles; expect a scanline callback at the boundary.
        ppu.run_ppu_cycles(341);

        let calls = calls.borrow();
        assert!(!calls.is_empty());
        assert_eq!(calls.last().copied(), Some((1, true)));
    }

    #[test]
    fn test_mapper_ppu_scanline_sees_rendering_disabled() {
        let calls: Rc<RefCell<Vec<(u16, bool)>>> = Rc::new(RefCell::new(Vec::new()));

        let cart = Rc::new(RefCell::new(Cartridge::from_mapper_for_test(Box::new(
            ScanlineSpyMapper {
                calls: calls.clone(),
            },
        ))));

        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.set_cartridge(cart);

        // Rendering disabled by default.
        ppu.run_ppu_cycles(341);

        let calls = calls.borrow();
        assert!(!calls.is_empty());
        assert_eq!(calls.last().copied(), Some((1, false)));
    }

    struct EndFrameSpyMapper {
        calls: Rc<RefCell<u32>>,
    }

    impl crate::cartridge::Mapper for EndFrameSpyMapper {
        fn read_prg(&self, _addr: u16) -> u8 {
            0
        }

        fn write_prg(&mut self, _addr: u16, _value: u8) {}

        fn read_chr(&self, _addr: u16) -> u8 {
            0
        }

        fn write_chr(&mut self, _addr: u16, _value: u8) {}

        fn ppu_address_changed(&mut self, _addr: u16) {}

        fn ppu_end_frame(&mut self) {
            *self.calls.borrow_mut() += 1;
        }

        fn get_mirroring(&self) -> MirroringMode {
            MirroringMode::Horizontal
        }
    }

    #[test]
    fn test_mapper_ppu_end_frame_is_called_when_frame_wraps() {
        let calls = Rc::new(RefCell::new(0u32));

        let cart = Rc::new(RefCell::new(Cartridge::from_mapper_for_test(Box::new(
            EndFrameSpyMapper {
                calls: calls.clone(),
            },
        ))));

        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.set_cartridge(cart);

        // Keep rendering disabled for deterministic timing (no odd-frame skip).
        ppu.run_ppu_cycles(262 * 341);

        // This should fail until PPU::tick forwards frame-wrap to the mapper.
        assert_eq!(*calls.borrow(), 1);
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ChrFetchEvent {
        SetIsSprite(bool),
        ReadChr(u16),
    }

    struct ChrFetchKindSpyMapper {
        events: Rc<RefCell<Vec<ChrFetchEvent>>>,
    }

    impl crate::cartridge::Mapper for ChrFetchKindSpyMapper {
        fn read_prg(&self, _addr: u16) -> u8 {
            0
        }

        fn write_prg(&mut self, _addr: u16, _value: u8) {}

        fn read_chr(&self, addr: u16) -> u8 {
            self.events.borrow_mut().push(ChrFetchEvent::ReadChr(addr));

            0
        }

        fn write_chr(&mut self, _addr: u16, _value: u8) {}

        fn ppu_address_changed(&mut self, _addr: u16) {}

        fn ppu_set_chr_fetch_is_sprite(&mut self, is_sprite: bool) {
            self.events
                .borrow_mut()
                .push(ChrFetchEvent::SetIsSprite(is_sprite));
        }

        fn get_mirroring(&self) -> MirroringMode {
            MirroringMode::Horizontal
        }
    }

    #[test]
    fn test_mapper_ppu_set_chr_fetch_is_sprite_is_applied_to_rendering_fetches() {
        let events: Rc<RefCell<Vec<ChrFetchEvent>>> = Rc::new(RefCell::new(Vec::new()));

        let cart = Rc::new(RefCell::new(Cartridge::from_mapper_for_test(Box::new(
            ChrFetchKindSpyMapper {
                events: events.clone(),
            },
        ))));

        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.set_cartridge(cart);

        // Force BG fetches from $0000 and sprite fetches from $1000 so the test can
        // distinguish the two types of CHR reads.
        ppu.write_control(0x08);

        // Place sprite 0 at Y=0 so it will be in range for scanline 1.
        ppu.write_oam_address(0);
        ppu.write_oam_data(0); // Y
        ppu.write_oam_data(0); // tile
        ppu.write_oam_data(0); // attr
        ppu.write_oam_data(0); // X

        // Enable background + sprites.
        ppu.write_mask(0x18);

        // Run into the sprite-pattern-fetch window (pixel 257-320), while also
        // covering the first background pattern fetches earlier in the scanline.
        ppu.run_ppu_cycles(264);

        let events = events.borrow();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ChrFetchEvent::ReadChr(addr) if *addr < 0x1000))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ChrFetchEvent::ReadChr(addr) if *addr >= 0x1000))
        );

        for (idx, event) in events.iter().copied().enumerate() {
            if let ChrFetchEvent::ReadChr(addr) = event {
                assert!(idx > 0, "ReadChr must be preceded by SetIsSprite");
                let expected = if addr < 0x1000 {
                    ChrFetchEvent::SetIsSprite(false)
                } else {
                    ChrFetchEvent::SetIsSprite(true)
                };
                assert_eq!(events[idx - 1], expected);
            }
        }
    }

    #[test]
    fn test_ppu_reset() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(100);
        ppu.reset();
        assert_eq!(ppu.scanline(), 0);
        assert_eq!(ppu.pixel(), 0);
    }

    #[test]
    fn test_ppu_write_control() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_control(0b1000_0000);
        // Control register should be set (verified internally)
    }

    #[test]
    fn test_ppu_read_write_data() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x42);

        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        // Palette RAM only stores 6 bits (0x42 & 0x3F = 0x02)
        // Reading returns palette bits 5-0 combined with open bus bits 7-6
        // After writing 0x00 to address, io_bus = 0x00, so result is 0x02
        assert_eq!(ppu.read_data(), 0x02);
    }

    #[test]
    fn test_ppu_vblank() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        // Advance to VBlank (scanline 241, pixel 2).
        // Pixel 1 is the VBlank set time and is subject to the $2002 suppression quirk.
        ppu.run_ppu_cycles(241 * 341 + 2);

        let status = ppu.get_status();
        // VBlank flag should be set (bit 7)
        assert_eq!(status & 0x80, 0x80);

        // Reading status should clear VBlank flag.
        let status_second_read = ppu.get_status();
        assert_eq!(status_second_read & 0x80, 0);
    }

    #[test]
    fn test_status_read_at_vblank_set_time_suppresses_vblank_flag() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Advance to scanline 241, pixel 0 (one PPU cycle before VBlank would normally be set).
        ppu.run_ppu_cycles(241 * 341);

        // Reading $2002 right before VBlank sets can suppress the VBlank flag for the frame.
        let status_before = ppu.get_status();
        assert_eq!(status_before & 0x80, 0);

        // Advance into the normal VBlank-set cycle (scanline 241, pixel 1).
        ppu.run_ppu_cycles(1);

        // With suppression, the VBlank flag should still be clear.
        let status_after = ppu.get_status();
        assert_eq!(status_after & 0x80, 0);
    }

    #[test]
    fn test_status_read_on_vblank_start_clears_vblank_flag() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Advance to the VBlank start dot (scanline 241, pixel 1).
        ppu.run_ppu_cycles(241 * 341 + 1);

        // First read should observe VBlank set.
        let first = ppu.get_status();
        assert_eq!(first & 0x80, 0x80);

        // Second read (same dot in this unit test) should observe that the flag was cleared.
        let second = ppu.get_status();
        assert_eq!(second & 0x80, 0);
    }

    #[test]
    fn test_vblank_flag_clears_on_prerender_dot_1() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Advance to the pre-render scanline (261), dot 0.
        ppu.run_ppu_cycles(261 * 341);
        assert_eq!(ppu.scanline(), 261);
        assert_eq!(ppu.pixel(), 0);

        // VBlank should still be set at dot 0.
        assert!(ppu.is_in_vblank());

        // It should clear at dot 1.
        ppu.run_ppu_cycles(1);
        assert_eq!(ppu.scanline(), 261);
        assert_eq!(ppu.pixel(), 1);
        assert!(!ppu.is_in_vblank());
    }

    #[test]
    fn test_enabling_nmi_while_in_vblank_triggers_nmi_edge() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Enter VBlank with NMI disabled.
        ppu.run_ppu_cycles(241 * 341 + 2);
        assert!(ppu.is_in_vblank());
        assert!(!ppu.should_generate_nmi());
        assert!(!ppu.poll_nmi());

        // Enabling NMI while VBlank flag is already set should immediately assert NMI.
        ppu.write_control(0x80);
        assert!(ppu.should_generate_nmi());
        assert!(ppu.poll_nmi());
    }

    // PPU Data tests
    #[test]
    fn test_read_data_from_palette() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x42);

        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        // Palette RAM only stores 6 bits (0x42 & 0x3F = 0x02)
        // Reading returns palette bits 5-0 combined with open bus bits 7-6
        // After writing 0x00 to address, io_bus = 0x00, so result is 0x02
        assert_eq!(ppu.read_data(), 0x02);
    }

    #[test]
    fn test_read_data_increments_address() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x10);
        ppu.write_data(0x20);

        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        assert_eq!(ppu.read_data(), 0x10);
        assert_eq!(ppu.read_data(), 0x20);
    }

    #[test]
    fn test_write_data_to_nametable() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_address(0x20, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x42);

        ppu.write_address(0x20, false);
        ppu.write_address(0x00, false);
        let _ = ppu.read_data(); // Dummy read for buffer
        assert_eq!(ppu.read_data(), 0x42);
    }

    // OAM tests
    #[test]
    fn test_oam_write_and_read() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_oam_address(0x00);
        ppu.write_oam_data(0x42);
        ppu.write_oam_address(0x00);
        assert_eq!(ppu.read_oam_data(), 0x42);
    }

    #[test]
    fn test_oam_data_increments_address() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_oam_address(0x00);
        ppu.write_oam_data(0x11); // Byte 0: Y position
        ppu.write_oam_data(0x22); // Byte 1: Tile index
        ppu.write_oam_data(0xE3); // Byte 2: Attributes (use valid bits only)
        ppu.write_oam_data(0x44); // Byte 3: X position

        ppu.write_oam_address(0x00);
        assert_eq!(ppu.read_oam_data(), 0x11);
        ppu.write_oam_address(0x01);
        assert_eq!(ppu.read_oam_data(), 0x22);
        ppu.write_oam_address(0x02);
        assert_eq!(ppu.read_oam_data(), 0xE3);
        ppu.write_oam_address(0x03);
        assert_eq!(ppu.read_oam_data(), 0x44);
    }

    #[test]
    fn test_oam_full_256_bytes() {
        // Test writing and reading all 256 bytes of OAM
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Write all 256 bytes with a pattern that accounts for attribute byte masking
        ppu.write_oam_address(0x00);
        for i in 0..256 {
            ppu.write_oam_data(i as u8);
        }

        // Verify OAMADDR wrapped around
        assert_eq!(
            ppu.oam_address(),
            0x00,
            "OAMADDR should wrap to 0 after 256 writes"
        );

        // Read all 256 bytes back, accounting for attribute byte masking
        ppu.write_oam_address(0x00);
        for i in 0..256 {
            let value = ppu.read_oam_data();
            ppu.write_oam_address((i + 1) as u8); // Manually increment since read doesn't
            let expected = if (i & 0x03) == 2 {
                (i as u8) & 0xE3 // Attribute bytes have bits 2-4 masked
            } else {
                i as u8
            };
            assert_eq!(value, expected, "OAM[{}] should be {}", i, expected);
        }
    }

    #[test]
    fn test_oamaddr_cleared_during_sprite_loading() {
        // OAMADDR is automatically set to 0 during pixels 257-320 of visible and pre-render scanlines
        // This is critical hardware behavior that many test ROMs rely on
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Enable rendering (otherwise OAMADDR clearing doesn't happen)
        ppu.write_control(0x00);
        ppu.write_mask(0x18); // Enable background and sprite rendering

        // Set OAMADDR to non-zero value
        ppu.write_oam_address(0x42);
        assert_eq!(ppu.oam_address(), 0x42);

        // Run to scanline 0, pixel 257 (start of sprite loading interval)
        ppu.run_ppu_cycles(257);

        // OAMADDR should be cleared to 0 during pixels 257-320
        assert_eq!(
            ppu.oam_address(),
            0x00,
            "OAMADDR should be cleared to 0 during sprite tile loading (pixels 257-320)"
        );

        // Set it again to verify it keeps getting cleared during the interval
        ppu.write_oam_address(0x99);
        ppu.run_ppu_cycles(1); // Still in the 257-320 interval
        assert_eq!(
            ppu.oam_address(),
            0x00,
            "OAMADDR should stay 0 during entire sprite loading interval"
        );

        // Run past pixel 320
        ppu.run_ppu_cycles(64); // Now at pixel 257+1+64 = 322

        // Now OAMADDR should stay whatever we set it to
        ppu.write_oam_address(0x55);
        ppu.run_ppu_cycles(1);
        assert_eq!(
            ppu.oam_address(),
            0x55,
            "OAMADDR should not be cleared after pixel 320"
        );
    }

    #[test]
    fn test_oamaddr_cleared_on_prerender_scanline() {
        // OAMADDR clearing also happens on the pre-render scanline
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Run to pre-render scanline (261), pixel 257
        ppu.run_ppu_cycles(261 * 341 + 257);

        ppu.write_oam_address(0x42);
        ppu.run_ppu_cycles(1); // Pixel 258, should clear OAMADDR
        assert_eq!(
            ppu.oam_address(),
            0x00,
            "OAMADDR should be cleared during pre-render scanline sprite loading"
        );
    }

    #[test]
    fn test_oamaddr_not_cleared_when_rendering_disabled() {
        // OAMADDR should NOT be cleared if rendering is disabled
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Rendering disabled (mask = 0)
        ppu.write_mask(0x00);

        ppu.write_oam_address(0x42);

        // Run through the sprite loading interval
        ppu.run_ppu_cycles(320);

        // OAMADDR should still be 0x42
        assert_eq!(
            ppu.oam_address(),
            0x42,
            "OAMADDR should not be cleared when rendering is disabled"
        );
    }

    #[test]
    fn test_oamaddr_corruption_at_rendering_start() {
        // If OAMADDR >= 8 when rendering starts (during pre-render sprite tile loading),
        // the 8 bytes at (OAMADDR & 0xF8) are copied to OAM[0..7]
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Setup: Write distinct values to different parts of OAM during vblank
        ppu.run_ppu_cycles(241 * 341 + 10); // In vblank

        // Write pattern to OAM[0..7]
        ppu.write_oam_address(0x00);
        for i in 0..8 {
            ppu.write_oam_data(i);
        }

        // Write different pattern to OAM[0x10..0x17]
        ppu.write_oam_address(0x10);
        for i in 0..8 {
            ppu.write_oam_data(0x80 + i);
        }

        // Set OAMADDR to 0x10 (>= 8) before rendering starts
        ppu.write_oam_address(0x10);

        // Run to pre-render scanline sprite tile loading (scanline 261, pixel 257)
        // At this point, OAM corruption should occur
        ppu.run_ppu_cycles((261 - 241) * 341 + 257 - 10);

        // Check that OAM[0..7] has been corrupted with data from OAM[0x10..0x17]
        // OAMADDR was 0x10, so 0x10 & 0xF8 = 0x10, meaning OAM[0x10..0x17] -> OAM[0..7]
        ppu.write_oam_address(0x00);
        for i in 0..8 {
            let value = ppu.read_oam_data();
            ppu.write_oam_address(i + 1); // Re-set address since read doesn't increment
            let expected = if (i & 0x03) == 2 {
                // Attribute byte: 0x82 with masking = 0x82 & 0xE3 = 0x82
                (0x80 + i) & 0xE3
            } else {
                0x80 + i
            };
            assert_eq!(
                value, expected,
                "OAM[{}] should be corrupted with value from OAM[0x10+{}]",
                i, i
            );
        }
    }

    #[test]
    fn test_no_oamaddr_corruption_when_less_than_8() {
        // If OAMADDR < 8 when rendering starts, no corruption occurs
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Setup OAM during vblank
        ppu.run_ppu_cycles(241 * 341 + 10);

        ppu.write_oam_address(0x00);
        for i in 0..8 {
            ppu.write_oam_data(0x40 + i); // Use values that work with attribute masking
        }

        // Set OAMADDR to value < 8
        ppu.write_oam_address(0x05);

        // Run to pre-render sprite tile loading
        ppu.run_ppu_cycles((261 - 241) * 341 + 257 - 10);

        // OAM[0..7] should be unchanged
        ppu.write_oam_address(0x00);
        for i in 0..8 {
            let value = ppu.read_oam_data();
            ppu.write_oam_address(i + 1);
            let expected = if (i & 0x03) == 2 {
                (0x40 + i) & 0xE3 // Attribute byte masking
            } else {
                0x40 + i
            };
            assert_eq!(
                value, expected,
                "OAM[{}] should not be corrupted when OAMADDR < 8",
                i
            );
        }
    }

    #[test]
    fn test_oam_write_during_rendering_ignored() {
        // Writes to OAMDATA during rendering should NOT modify OAM
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Write initial value to OAM during vblank (should work)
        ppu.run_ppu_cycles(241 * 341 + 10); // In vblank
        ppu.write_oam_address(0x05);
        ppu.write_oam_data(0x42);

        // Run to visible scanline, avoiding the OAMADDR clearing period (257-320)
        ppu.run_ppu_cycles((262 - 241) * 341 + 100); // Scanline 0, pixel 100

        // Try to write during rendering (should be ignored)
        ppu.write_oam_address(0x05);
        ppu.write_oam_data(0x99); // This write should be ignored

        // Read back - should still be 0x42
        ppu.write_oam_address(0x05);
        assert_eq!(
            ppu.read_oam_data(),
            0x42,
            "OAM write during rendering should be ignored"
        );
    }

    #[test]
    fn test_oam_write_during_rendering_increments_address() {
        // Writes to OAMDATA during rendering should still increment OAMADDR (glitchy increment)
        // The glitchy increment bumps only the high 6 bits (adds 4 instead of 1)
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Run to visible scanline
        ppu.run_ppu_cycles(100); // Scanline 0, pixel 100

        // Set OAMADDR to 0x10 and write (write ignored, but address incremented by 4)
        ppu.write_oam_address(0x10);
        ppu.write_oam_data(0x99); // Write ignored, but glitchy increment happens

        // Address should have incremented by 4 (glitchy increment - high 6 bits bumped)
        assert_eq!(
            ppu.oam_address(),
            0x14,
            "OAMADDR should increment by 4 (glitchy) during rendering"
        );

        // Test with address 0x13 (low 2 bits = 0b11)
        ppu.write_oam_address(0x13);
        ppu.write_oam_data(0x99);
        // Glitchy increment: (0x13 & 0x03) | ((0x13 + 4) & 0xFC) = 0x03 | 0x14 = 0x17
        assert_eq!(
            ppu.oam_address(),
            0x17,
            "Glitchy increment should preserve low 2 bits and add 4 to high 6 bits"
        );
    }

    #[test]
    fn test_oam_write_outside_rendering_works() {
        // Writes to OAMDATA outside rendering should work normally
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Run to vblank
        ppu.run_ppu_cycles(241 * 341 + 10);

        // Write during vblank (should work)
        ppu.write_oam_address(0x00);
        ppu.write_oam_data(0x42);

        // Read back
        ppu.write_oam_address(0x00);
        assert_eq!(
            ppu.read_oam_data(),
            0x42,
            "OAM write during vblank should work normally"
        );
    }

    // Control register tests
    #[test]
    fn test_ppuctrl_nmi_enable() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_control(0x80); // Bit 7: NMI enable
        assert!(ppu.should_generate_nmi());

        ppu.write_control(0x00);
        assert!(!ppu.should_generate_nmi());
    }

    // Address register tests
    #[test]
    fn test_address_write_sequence() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_address(0x20, false); // High byte
        ppu.write_address(0x00, false); // Low byte
        assert_eq!(ppu.v_register(), 0x2000);
    }

    #[test]
    fn test_address_wraps_correctly() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_address(0xFF, false); // High byte
        ppu.write_address(0xFF, false); // Low byte
        // Address should be masked to 14 bits (0x3FFF)
        assert_eq!(ppu.v_register() & 0x3FFF, 0x3FFF);
    }

    // Scroll register tests
    #[test]
    fn test_scroll_write_updates_registers() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_scroll(0xFF, false); // X scroll
        ppu.write_scroll(0xFF, false); // Y scroll
        // Verify write toggle was used
        assert!(!ppu.w_register()); // Should be false after two writes
    }

    // Timing tests
    #[test]
    fn test_scanline_increments() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(341); // One full scanline
        assert_eq!(ppu.scanline(), 1);
        assert_eq!(ppu.pixel(), 0);
    }

    #[test]
    fn test_frame_wraps_at_262_scanlines() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(262 * 341); // One full frame
        assert_eq!(ppu.scanline(), 0);
        assert_eq!(ppu.pixel(), 0);
    }

    // Status register tests
    #[test]
    fn test_status_read_clears_vblank() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(241 * 341 + 2); // Past vblank start

        let status1 = ppu.get_status();
        assert_eq!(status1 & 0x80, 0x80); // VBlank set

        let status2 = ppu.get_status();
        assert_eq!(status2 & 0x80, 0); // VBlank cleared
    }

    #[test]
    fn test_status_read_clears_write_toggle() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_scroll(0x00, false); // First write, sets w=true
        assert!(ppu.w_register());

        ppu.get_status(); // Should clear w
        assert!(!ppu.w_register());
    }

    // CHR ROM and mirroring tests
    // Note: CHR ROM is now loaded dynamically through cartridge mapper
    // No longer need test_load_chr_rom

    #[test]
    fn test_vertical_mirroring() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.set_mirroring(crate::cartridge::MirroringMode::Vertical);

        // Write to nametable 0
        ppu.write_address(0x20, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x42);

        // Read from nametable 2 (should mirror to 0)
        ppu.write_address(0x28, false);
        ppu.write_address(0x00, false);
        let _ = ppu.read_data(); // Dummy read
        assert_eq!(ppu.read_data(), 0x42);
    }

    #[test]
    fn test_horizontal_mirroring() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.set_mirroring(crate::cartridge::MirroringMode::Horizontal);

        // Write to nametable 0
        ppu.write_address(0x20, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x55);

        // Read from nametable 1 (should mirror to 0 in horizontal)
        ppu.write_address(0x24, false);
        ppu.write_address(0x00, false);
        let _ = ppu.read_data(); // Dummy read
        let val = ppu.read_data();
        assert_eq!(val, 0x55); // Should be mirrored
    }

    // NMI and frame complete tests
    #[test]
    fn test_nmi_polling() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.write_control(0x80); // Enable NMI
        // VBlank flag is set at scanline 241 dot 1, but the NMI edge is latched at dot 2.
        ppu.run_ppu_cycles(241 * 341 + 2);

        assert!(ppu.poll_nmi()); // Should return true once
        assert!(!ppu.poll_nmi()); // Should be cleared after polling
    }

    #[test]
    fn test_frame_complete_polling() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        ppu.run_ppu_cycles(241 * 341 + 1); // Enter VBlank

        assert!(ppu.poll_frame_complete()); // Should return true once
        assert!(!ppu.poll_frame_complete()); // Should be cleared after polling
    }

    #[test]
    fn test_pixel_zero_no_panic() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        // Enable rendering
        ppu.write_mask(0x18); // Enable background and sprite rendering

        // Run through a full scanline which includes pixel 0
        ppu.run_ppu_cycles(341);

        // Should not panic - pixel 0 is handled correctly
        assert_eq!(ppu.scanline(), 1);
        assert_eq!(ppu.pixel(), 0);
    }

    #[test]
    fn test_rendering_with_pixel_transitions() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        // Enable rendering
        ppu.write_mask(0x18);

        // Run through multiple scanlines to test pixel 0 transitions
        for _ in 0..5 {
            ppu.run_ppu_cycles(341);
        }

        // Should complete without panicking
        assert_eq!(ppu.scanline(), 5);
    }

    #[test]
    fn test_palette_access_with_correct_addressing() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Write to palette using full address
        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x30); // Write to backdrop color

        // Write to another palette entry
        ppu.write_address(0x3F, false);
        ppu.write_address(0x01, false);
        ppu.write_data(0x16);

        // Enable rendering and run one scanline
        ppu.write_mask(0x18);
        ppu.run_ppu_cycles(341);

        // Should complete without panic - palette lookups work correctly
        assert_eq!(ppu.scanline(), 1);
    }

    #[test]
    fn test_shift_register_load_timing() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        // Enable background rendering
        ppu.write_mask(0x08);

        // Set up a known scroll position
        ppu.write_scroll(0, false);
        ppu.write_scroll(0, false);

        // Run to pixel 8 of scanline 0 (first shift register load)
        ppu.run_ppu_cycles(8);
        assert_eq!(ppu.scanline(), 0);
        assert_eq!(ppu.pixel(), 8);

        // Run to pixel 16 (second shift register load)
        ppu.run_ppu_cycles(8);
        assert_eq!(ppu.pixel(), 16);

        // Run to pixel 24 (third shift register load)
        ppu.run_ppu_cycles(8);
        assert_eq!(ppu.pixel(), 24);

        // Verify we can continue through the scanline without issues
        ppu.run_ppu_cycles(256 - 24);
        assert_eq!(ppu.pixel(), 256);
    }

    #[test]
    fn test_scroll_register_updates_at_correct_pixels() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        // Enable rendering
        ppu.write_mask(0x18);

        // Set up scroll and nametable
        ppu.write_control(0x00); // Nametable at $2000
        ppu.write_scroll(0, false);
        ppu.write_scroll(0, false);

        let _v_before_256 = ppu.v_register();

        // Run to pixel 256 (increment_fine_y happens here)
        ppu.run_ppu_cycles(256);
        assert_eq!(ppu.pixel(), 256);

        // Run to pixel 257 (copy_horizontal_bits happens here)
        ppu.run_ppu_cycles(1);
        assert_eq!(ppu.pixel(), 257);

        // V register should have been updated
        let _v_after_257 = ppu.v_register();
        // At minimum, fine Y should have incremented or wrapped
        // (exact value depends on internal state, but they shouldn't be identical
        // unless at a boundary condition)

        // Just verify we can continue without panic
        ppu.run_ppu_cycles(341 - 257);
        assert_eq!(ppu.scanline(), 1);
    }

    #[test]
    fn test_pre_render_scanline_prefetch() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        // Enable rendering
        ppu.write_mask(0x18);

        // Run to pre-render scanline (261)
        ppu.run_ppu_cycles(261 * 341);
        assert_eq!(ppu.scanline(), 261);

        // Run to pixel 321 (start of pre-fetch)
        ppu.run_ppu_cycles(321);
        assert_eq!(ppu.pixel(), 321);

        // Run to pixel 328 (first pre-fetch load)
        ppu.run_ppu_cycles(7);
        assert_eq!(ppu.pixel(), 328);

        // Run to pixel 336 (second pre-fetch load)
        ppu.run_ppu_cycles(8);
        assert_eq!(ppu.pixel(), 336);

        // Complete the scanline
        ppu.run_ppu_cycles(341 - 336);
        assert_eq!(ppu.scanline(), 0); // Should wrap to scanline 0
    }

    #[test]
    fn test_rendering_enabled_background_fetch_cycles() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        // Enable background rendering
        ppu.write_mask(0x08);

        // Run through visible pixels (1-256)
        for pixel in 1..=256 {
            ppu.run_ppu_cycles(1);
            assert_eq!(ppu.pixel(), pixel);
        }

        // Continue through pre-fetch region (321-336)
        ppu.run_ppu_cycles(321 - 256);
        assert_eq!(ppu.pixel(), 321);

        for pixel in 322..=336 {
            ppu.run_ppu_cycles(1);
            assert_eq!(ppu.pixel(), pixel);
        }
    }

    #[test]
    fn test_dummy_nametable_fetches() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        // Enable rendering
        ppu.write_mask(0x18);

        // Run to pixel 337
        ppu.run_ppu_cycles(337);
        assert_eq!(ppu.pixel(), 337);

        // Run to pixel 339
        ppu.run_ppu_cycles(2);
        assert_eq!(ppu.pixel(), 339);

        // Complete the scanline without panic
        ppu.run_ppu_cycles(341 - 339);
        assert_eq!(ppu.scanline(), 1);
    }

    #[test]
    fn test_coarse_x_increment_timing() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);
        // Enable rendering
        ppu.write_mask(0x18);

        // Set up scroll
        ppu.write_scroll(0, false);
        ppu.write_scroll(0, false);

        let v_initial = ppu.v_register();

        // Run to pixel 9 (first coarse X increment)
        // Per NES Dev wiki, shifters are reloaded at ticks 9, 17, 25, ..., 257
        ppu.run_ppu_cycles(9);
        let v_after_9 = ppu.v_register();

        // Coarse X should have incremented (bits 0-4 of v register)
        let coarse_x_initial = v_initial & 0x001F;
        let coarse_x_after_9 = v_after_9 & 0x001F;
        assert_eq!(coarse_x_after_9, (coarse_x_initial + 1) & 0x001F);

        // Run to pixel 17 (second coarse X increment)
        ppu.run_ppu_cycles(8);
        let v_after_17 = ppu.v_register();
        let coarse_x_after_17 = v_after_17 & 0x001F;
        assert_eq!(coarse_x_after_17, (coarse_x_initial + 2) & 0x001F);
    }

    #[test]
    fn test_a12_rising_edge_detection() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // A12 is bit 12 of address (0x1000)
        // Initially prev_a12 should be false

        // Access $0000 (A12=0) - no rising edge
        assert!(!ppu.check_a12_rising_edge(0x0000));

        // Access $0FFF (A12=0) - no rising edge
        assert!(!ppu.check_a12_rising_edge(0x0FFF));

        // Access $1000 (A12=1) - rising edge!
        assert!(ppu.check_a12_rising_edge(0x1000));

        // Access $1FFF (A12=1) - no rising edge (already high)
        assert!(!ppu.check_a12_rising_edge(0x1FFF));

        // Access $0000 (A12=0) - no rising edge (falling edge)
        assert!(!ppu.check_a12_rising_edge(0x0000));

        // Access $1800 (A12=1) - rising edge!
        assert!(ppu.check_a12_rising_edge(0x1800));

        // Access $1000 (A12=1) - no rising edge
        assert!(!ppu.check_a12_rising_edge(0x1000));
    }

    #[test]
    fn test_background_rendering_alignment() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Create a simple iNES ROM with known CHR ROM data
        let mut ines_data = Vec::new();

        // iNES header
        ines_data.extend_from_slice(b"NES\x1A"); // Magic number
        ines_data.push(2); // 2 * 16KB PRG ROM
        ines_data.push(1); // 1 * 8KB CHR ROM
        ines_data.push(0); // Mapper 0 (NROM), horizontal mirroring
        ines_data.push(0); // Mapper upper bits
        ines_data.extend_from_slice(&[0; 8]); // Padding

        // PRG ROM (32KB)
        ines_data.extend_from_slice(&vec![0u8; 0x8000]);

        // CHR ROM (8KB) with known tiles
        let mut chr_rom = vec![0u8; 0x2000];

        // Tile 0 (at $0000): Empty tile (all transparent)
        // Pattern low and high bytes are all 0

        // Tile 1 (at $0010): Solid tile with pattern value 3 (color 3 in palette)
        // Each byte represents one row of 8 pixels
        // Pattern low = 0xFF (all bits set)
        // Pattern high = 0xFF (all bits set)
        // This gives pattern value 3 (both bits set) for all pixels
        for row in 0..8 {
            chr_rom[0x10 + row] = 0xFF; // Pattern low
            chr_rom[0x18 + row] = 0xFF; // Pattern high
        }

        // Tile 2 (at $0020): Tile with pattern value 1 (only low bit set)
        for row in 0..8 {
            chr_rom[0x20 + row] = 0xFF; // Pattern low
            chr_rom[0x28 + row] = 0x00; // Pattern high
        }

        // Tile 3 (at $0030): Tile with pattern value 2 (only high bit set)
        for row in 0..8 {
            chr_rom[0x30 + row] = 0x00; // Pattern low
            chr_rom[0x38 + row] = 0xFF; // Pattern high
        }

        ines_data.extend_from_slice(&chr_rom);

        // Create cartridge from iNES data
        let cartridge = Cartridge::new(&ines_data).expect("Failed to create cartridge");
        ppu.set_cartridge(Rc::new(RefCell::new(cartridge)));

        // Set up palette - use distinct colors for each palette entry
        // Palette 0 will be: backdrop (black), red, green, blue
        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x0F); // Universal backdrop (black)
        ppu.write_address(0x3F, false);
        ppu.write_address(0x01, false);
        ppu.write_data(0x16); // Palette 0, color 1 (red)
        ppu.write_address(0x3F, false);
        ppu.write_address(0x02, false);
        ppu.write_data(0x2A); // Palette 0, color 2 (green)
        ppu.write_address(0x3F, false);
        ppu.write_address(0x03, false);
        ppu.write_data(0x12); // Palette 0, color 3 (blue)

        // Set up nametable - create a known pattern
        // Place tile 1 (solid, pattern 3) at position (0,0) - top-left corner
        ppu.write_address(0x20, false);
        ppu.write_address(0x00, false);
        ppu.write_data(1); // Tile 1 at (0,0)

        // Place tile 2 (pattern 1) at position (1,0) - second tile in first row
        ppu.write_data(2); // Tile 2 at (1,0)

        // Place tile 3 (pattern 2) at position (2,0) - third tile in first row
        ppu.write_data(3); // Tile 3 at (2,0)

        // Fill rest of first row with tile 0 (empty/transparent)
        for _ in 3..32 {
            ppu.write_data(0); // Empty tiles
        }

        // Set up attribute table - palette 0 for all tiles
        ppu.write_address(0x23, false);
        ppu.write_address(0xC0, false);
        for _ in 0..64 {
            ppu.write_data(0x00); // Palette 0 for all
        }

        // Set scroll position to 0,0
        // This ensures t register is properly initialized
        ppu.write_scroll(0, false); // X scroll = 0
        ppu.write_scroll(0, false); // Y scroll = 0

        // Enable rendering
        ppu.write_control(0b0000_0000); // BG pattern table at $0000, no NMI, nametable $2000
        ppu.write_mask(0b0000_1010); // Enable background rendering, no clipping

        // Debug: Check if rendering is enabled
        // println!(
        //     "Rendering enabled: {}",
        //     ppu.registers.is_rendering_enabled()
        // );
        // println!(
        //     "Background enabled: {}",
        //     ppu.registers.is_background_enabled()
        // );

        // Run PPU to render two complete frames
        // NTSC: 262 scanlines * 341 dots/scanline
        // First frame: renders with empty shift registers (will show offset)
        // Second frame: pre-render scanline 261 of first frame loads shift registers,
        //               so second frame renders correctly with tiles at positions 0-7, 8-15, etc.
        ppu.run_ppu_cycles(2 * 262 * 341);

        // println!("After rendering:");
        // println!("Scanline: {}, Pixel: {}", ppu.scanline(), ppu.pixel());

        // Debug: Check if palette was actually written
        // Use direct memory access to check
        // println!("Palette check after rendering:");
        ppu.write_address(0x3F, false);
        ppu.write_address(0x03, false);
        let _pal3 = ppu.read_data();
        // println!("Palette $3F03 (should be 0x12): {:02X}", pal3);

        // Now check the screen buffer for expected colors
        // println!("Before checking screen buffer:");
        let screen_buffer = ppu.screen_buffer();

        // Get the system palette colors for our palette entries
        let (red_r, red_g, red_b) = crate::nes::Nes::lookup_system_palette(0x16);
        let (green_r, green_g, green_b) = crate::nes::Nes::lookup_system_palette(0x2A);
        let (blue_r, blue_g, blue_b) = crate::nes::Nes::lookup_system_palette(0x12);
        let (black_r, black_g, black_b) = crate::nes::Nes::lookup_system_palette(0x0F);

        // Debug: Print first 32 pixels of row 0 for analysis
        // println!("\nFirst 32 pixels of row 0:");
        // for x in 0..32 {
        //     let (r, g, b) = screen_buffer.get_pixel(x, 0);
        //     let color_name = if (r, g, b) == (blue_r, blue_g, blue_b) {
        //         "BLUE"
        //     } else if (r, g, b) == (red_r, red_g, red_b) {
        //         "RED"
        //     } else if (r, g, b) == (green_r, green_g, green_b) {
        //         "GREEN"
        //     } else if (r, g, b) == (black_r, black_g, black_b) {
        //         "BLACK"
        //     } else {
        //         "UNKNOWN"
        //     };
        //     println!("  Pixel {}: ({},{},{}) = {}", x, r, g, b, color_name);
        // }

        // Verify all pixels in the topmost 16 rows
        // After running two complete frames, the pre-render scanline has properly loaded
        // the shift registers, so tiles should appear at their correct pixel positions
        for row in 0..16 {
            for x in 0..256 {
                let (r, g, b) = screen_buffer.get_pixel(x, row);
                let expected_color = match row {
                    0..=7 => {
                        // First tile row - should show tiles at their correct positions:
                        // Nametable position 0: tile 1 (blue) at pixels 0-7
                        // Nametable position 1: tile 2 (red) at pixels 8-15
                        // Nametable position 2: tile 3 (green) at pixels 16-23
                        // Rest: tile 0 (black/empty)
                        if x <= 7 {
                            (blue_r, blue_g, blue_b) // Tile 1 from nametable position 0
                        } else if (8..=15).contains(&x) {
                            (red_r, red_g, red_b) // Tile 2 from nametable position 1
                        } else if (16..=23).contains(&x) {
                            (green_r, green_g, green_b) // Tile 3 from nametable position 2
                        } else {
                            (black_r, black_g, black_b) // Empty tiles (positions 3+)
                        }
                    }
                    _ => (black_r, black_g, black_b), // Second tile row (nametable row 1), all empty
                };

                assert_eq!(
                    (r, g, b),
                    expected_color,
                    "Pixel ({}, {}) has wrong color",
                    x,
                    row
                );
            }
        }
    }

    #[test]
    fn test_sprite_rendering_alignment() {
        let mut ppu = Ppu::new(TvSystem::Ntsc);

        // Create a simple iNES ROM with known CHR ROM data
        let mut ines_data = Vec::new();

        // iNES header
        ines_data.extend_from_slice(b"NES\x1A"); // Magic number
        ines_data.push(2); // 2 * 16KB PRG ROM
        ines_data.push(1); // 1 * 8KB CHR ROM
        ines_data.push(0); // Mapper 0 (NROM), horizontal mirroring
        ines_data.push(0); // Mapper upper bits
        ines_data.extend_from_slice(&[0; 8]); // Padding

        // PRG ROM (32KB)
        ines_data.extend_from_slice(&vec![0u8; 0x8000]);

        // CHR ROM (8KB) with known sprite tiles
        let mut chr_rom = vec![0u8; 0x2000];

        // Tile 0 (at $0000): Empty tile (all transparent)
        // Pattern low and high bytes are all 0

        // Tile 1 (at $0010): Solid tile with pattern value 3 (color 3 in palette)
        for row in 0..8 {
            chr_rom[0x10 + row] = 0xFF; // Pattern low
            chr_rom[0x18 + row] = 0xFF; // Pattern high
        }

        // Tile 2 (at $0020): Tile with pattern value 1 (only low bit set)
        for row in 0..8 {
            chr_rom[0x20 + row] = 0xFF; // Pattern low
            chr_rom[0x28 + row] = 0x00; // Pattern high
        }

        // Tile 3 (at $0030): Tile with pattern value 2 (only high bit set)
        for row in 0..8 {
            chr_rom[0x30 + row] = 0x00; // Pattern low
            chr_rom[0x38 + row] = 0xFF; // Pattern high
        }

        ines_data.extend_from_slice(&chr_rom);

        // Create cartridge from iNES data
        let cartridge = Cartridge::new(&ines_data).expect("Failed to create cartridge");
        ppu.set_cartridge(Rc::new(RefCell::new(cartridge)));

        // Set up sprite palette - use distinct colors
        // Palette 0 will be: backdrop (black), yellow, cyan, magenta
        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x0F); // Universal backdrop (black)
        ppu.write_address(0x3F, false);
        ppu.write_address(0x11, false);
        ppu.write_data(0x28); // Sprite palette 0, color 1 (yellow)
        ppu.write_address(0x3F, false);
        ppu.write_address(0x12, false);
        ppu.write_data(0x2C); // Sprite palette 0, color 2 (cyan)
        ppu.write_address(0x3F, false);
        ppu.write_address(0x13, false);
        ppu.write_data(0x14); // Sprite palette 0, color 3 (magenta)

        // Set up sprites in OAM
        // Sprite 0: tile 1 (pattern 3 = magenta) at position (16, 16)
        ppu.write_oam_address(0x00);
        ppu.write_oam_data(16); // Y position
        ppu.write_oam_data(1); // Tile index 1
        ppu.write_oam_data(0x00); // Attributes: palette 0, no flip
        ppu.write_oam_data(16); // X position

        // Sprite 1: tile 2 (pattern 1 = yellow) at position (32, 16)
        ppu.write_oam_data(16); // Y position
        ppu.write_oam_data(2); // Tile index 2
        ppu.write_oam_data(0x00); // Attributes: palette 0, no flip
        ppu.write_oam_data(32); // X position

        // Sprite 2: tile 3 (pattern 2 = cyan) at position (48, 16)
        ppu.write_oam_data(16); // Y position
        ppu.write_oam_data(3); // Tile index 3
        ppu.write_oam_data(0x00); // Attributes: palette 0, no flip
        ppu.write_oam_data(48); // X position

        // Fill rest of OAM with off-screen sprites (Y = 0xFF)
        for _ in 3..64 {
            ppu.write_oam_data(0xFF); // Y position (off-screen)
            ppu.write_oam_data(0); // Tile index
            ppu.write_oam_data(0); // Attributes
            ppu.write_oam_data(0); // X position
        }

        // Set scroll position to 0,0
        ppu.write_scroll(0, false);
        ppu.write_scroll(0, false);

        // Enable rendering - sprites only, use sprite pattern table at $0000
        ppu.write_control(0b0000_0000); // Sprite pattern table at $0000, no NMI
        ppu.write_mask(0b0001_0100); // Enable sprite rendering, no clipping

        // Run PPU to render two complete frames
        ppu.run_ppu_cycles(2 * 262 * 341);

        let screen_buffer = ppu.screen_buffer();

        // Get the system palette colors for our sprite palette entries
        let (yellow_r, yellow_g, yellow_b) = crate::nes::Nes::lookup_system_palette(0x28);
        let (cyan_r, cyan_g, cyan_b) = crate::nes::Nes::lookup_system_palette(0x2C);
        let (magenta_r, magenta_g, magenta_b) = crate::nes::Nes::lookup_system_palette(0x14);
        let (black_r, black_g, black_b) = crate::nes::Nes::lookup_system_palette(0x0F);

        // Verify sprite rendering according to NES hardware specification:
        // - X coordinate: Direct mapping, screen_x = OAM.X (no offset)
        // - Y coordinate: +1 offset, screen_y = OAM.Y + 1
        //
        // Sprites with Y=N are rendered on scanlines N+1 to N+8
        //
        // Expected correct behavior:
        // Sprite 0 (magenta) at OAM (X=16, Y=16) should render at pixels (16-23, 17-24)
        // Sprite 1 (yellow) at OAM (X=32, Y=16) should render at pixels (32-39, 17-24)
        // Sprite 2 (cyan) at OAM (X=48, Y=16) should render at pixels (48-55, 17-24)

        for y in 0..240 {
            for x in 0..256 {
                let (r, g, b) = screen_buffer.get_pixel(x, y);
                let expected_color = if (17..=24).contains(&y) {
                    // Scanlines where sprites are visible (Y position 16 + 1, for 8 rows)
                    // Using CORRECT X coordinates per hardware specification
                    if (16..=23).contains(&x) {
                        (magenta_r, magenta_g, magenta_b) // Sprite 0 (correct position)
                    } else if (32..=39).contains(&x) {
                        (yellow_r, yellow_g, yellow_b) // Sprite 1 (correct position)
                    } else if (48..=55).contains(&x) {
                        (cyan_r, cyan_g, cyan_b) // Sprite 2 (correct position)
                    } else {
                        (black_r, black_g, black_b) // Backdrop
                    }
                } else {
                    (black_r, black_g, black_b) // Backdrop
                };

                assert_eq!(
                    (r, g, b),
                    expected_color,
                    "Sprite pixel ({}, {}) has wrong color",
                    x,
                    y
                );
            }
        }
    }
}
