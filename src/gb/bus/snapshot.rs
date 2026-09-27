//! Game Boy save-state capture, restore and hard reset, written once for the
//! DMG and CGB buses.
//!
//! [`BusSnapshot`] owns the rules both buses share: which hardware a save
//! state carries (PPU, HRAM, timer, joypad, APU, IF, IE, OAM DMA, serial port,
//! boot-ROM mapping), that a state from the other bus type is refused, that a
//! restored PPU is fixed up after load, that a state lacking serial fields
//! restores a fresh serial port and one lacking the boot-ROM flag restores it
//! unmapped, and that a hard reset rebuilds the bus through its constructor
//! while keeping the cartridge and the audio output sample rate. Each bus adds
//! only its model's own fields: the WRAM layout, the CGB registers and HDMA,
//! the DMG model and the SGB overlay.

use crate::gb::apu::Apu;
use crate::gb::bus::oam_dma::OamDma;
use crate::gb::bus::serial::Serial;
use crate::gb::cartridge::GbCartridge;
use crate::gb::console::save_state::{BusState, GbBusType};
use crate::gb::input::joypad::Joypad;
use crate::gb::ppu::Ppu;
use crate::gb::timer::Timer;

/// The hardware every Game Boy bus saves the same way, borrowed for capture.
pub(super) struct SharedState<'a> {
    pub ppu: &'a Ppu,
    pub hram: &'a [u8; 0x7F],
    pub timer: &'a Timer,
    pub joypad: &'a Joypad,
    pub apu: &'a Apu,
    pub if_reg: u8,
    pub ie_reg: u8,
    pub oam_dma: OamDma,
    pub serial: &'a Serial,
    pub boot_rom_active: bool,
}

/// The hardware every Game Boy bus saves the same way, borrowed for restore.
pub(super) struct SharedStateMut<'a> {
    pub ppu: &'a mut Ppu,
    pub hram: &'a mut [u8; 0x7F],
    pub timer: &'a mut Timer,
    pub joypad: &'a mut Joypad,
    pub apu: &'a mut Apu,
    pub if_reg: &'a mut u8,
    pub ie_reg: &'a mut u8,
    pub oam_dma: &'a mut OamDma,
    pub serial: &'a mut Serial,
    pub boot_rom_active: &'a mut bool,
}

/// Holds a bus's cartridge slot for the instant [`BusSnapshot::reset`] has
/// taken the real cartridge out to rebuild the bus around it; never read.
pub(super) struct NoCartridge;

impl GbCartridge for NoCartridge {
    fn read(&self, _addr: u16) -> u8 {
        0xFF
    }

    fn write(&mut self, _addr: u16, _val: u8) {}
}

/// Save-state capture, restore and hard reset for a Game Boy bus.
pub(super) trait BusSnapshot: Sized {
    /// The bus type a save state must carry to be restored here.
    const BUS_TYPE: GbBusType;

    fn shared(&self) -> SharedState<'_>;
    fn shared_mut(&mut self) -> SharedStateMut<'_>;

    /// WRAM laid out as the 32 KiB save-state array.
    fn wram_flat(&self) -> Box<[u8; 0x8000]>;
    /// Take WRAM back from the 32 KiB save-state array.
    fn restore_wram(&mut self, flat: &[u8; 0x8000]);

    /// Write the model's own fields into a state the shared capture built.
    fn capture_model(&self, state: &mut BusState);
    /// Take the model's own fields back, with defaults for older states.
    fn restore_model(&mut self, state: &BusState) -> Result<(), String>;

    /// Adjust a restored PPU before its post-load fixup.
    fn prepare_restored_ppu(&self, _ppu: &mut Ppu) {}

    /// Take the cartridge out, leaving [`NoCartridge`] in its slot.
    fn take_cartridge(&mut self) -> Box<dyn GbCartridge>;
    /// The bus as its constructor builds it around `cart`, keeping what is not
    /// machine state (the model and the player's construction-time choices).
    fn rebuilt(&self, cart: Box<dyn GbCartridge>) -> Self;

    /// Capture the full bus state for serialization.
    fn capture_bus_state(&self) -> BusState {
        let shared = self.shared();
        let (dma_active, dma_source, dma_position, dma_oam_blocked) = shared.oam_dma.parts();
        let mut state = BusState {
            bus_type: Self::BUS_TYPE,
            ppu: shared.ppu.clone(),
            wram: self.wram_flat(),
            hram: *shared.hram,
            timer: shared.timer.clone(),
            joypad: shared.joypad.clone(),
            apu: shared.apu.clone(),
            if_reg: shared.if_reg,
            ie_reg: shared.ie_reg,
            dma_active,
            dma_source,
            dma_position,
            dma_oam_blocked,
            hdma: None,
            svbk: None,
            key1: None,
            apu_tick_accumulator: None,
            rtc_tick_accumulator: None,
            ff72: None,
            ff73: None,
            ff74: None,
            ff75: None,
            key0: None,
            key0_locked: None,
            cgb_extra_oam: None,
            boot_rom_active: Some(shared.boot_rom_active),
            sb: None,
            sc: None,
            serial_buf: None,
            serial_bits_remaining: None,
            serial_master_clock: None,
            model: None,
            sgb: None,
        };
        shared.serial.capture_into(&mut state);
        self.capture_model(&mut state);
        state
    }

    /// Restore bus state from a deserialized snapshot.
    ///
    /// Returns an error if the save state was captured from the other bus type.
    fn restore_bus_state(&mut self, state: &BusState) -> Result<(), String> {
        if state.bus_type != Self::BUS_TYPE {
            let expected = match Self::BUS_TYPE {
                GbBusType::Dmg => "DMG",
                GbBusType::Cgb => "CGB",
            };
            return Err(format!(
                "bus type mismatch: expected {expected}, found {:?}",
                state.bus_type
            ));
        }
        let mut ppu = state.ppu.clone();
        self.prepare_restored_ppu(&mut ppu);
        ppu.fixup_after_state_load();
        self.restore_wram(&state.wram);
        let shared = self.shared_mut();
        *shared.ppu = ppu;
        *shared.hram = state.hram;
        *shared.timer = state.timer.clone();
        *shared.joypad = state.joypad.clone();
        *shared.apu = state.apu.clone();
        *shared.if_reg = state.if_reg;
        *shared.ie_reg = state.ie_reg;
        *shared.oam_dma = OamDma::from_parts(
            state.dma_active,
            state.dma_source,
            state.dma_position,
            state.dma_oam_blocked,
        );
        *shared.serial = Serial::new();
        shared.serial.restore_from(state);
        *shared.boot_rom_active = state.boot_rom_active.unwrap_or(false);
        self.restore_model(state)
    }

    /// Hard reset: rebuild the bus exactly as its constructor would.
    ///
    /// The constructor is the one place that knows initial state, so a reset
    /// machine is by construction a freshly loaded one. Only what is not
    /// machine state survives: the cartridge (ROM, RAM and mapper state), what
    /// [`BusSnapshot::rebuilt`] keeps, and the APU output sample rate.
    fn reset(&mut self) {
        let cart = self.take_cartridge();
        let sample_rate = self.shared().apu.sample_rate();
        *self = self.rebuilt(cart);
        self.shared_mut().apu.set_sample_rate(sample_rate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gb::bus::{CgbBus, DmgBus, GbBus};
    use crate::gb::cartridge::load_cartridge;
    use crate::gb::model::{CgbModel, DmgModel};

    /// An MBC1+RAM+battery cartridge with 8 KiB of RAM, CGB-compatible or not.
    fn ram_cart(cgb: bool) -> Box<dyn GbCartridge> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0143] = if cgb { 0x80 } else { 0x00 };
        rom[0x0147] = 0x03; // MBC1+RAM+BATTERY
        rom[0x0148] = 0x00; // 32 KB
        rom[0x0149] = 0x02; // 8 KB RAM
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;
        load_cartridge(&rom).expect("valid ROM")
    }

    fn dmg() -> DmgBus {
        DmgBus::new(ram_cart(false), DmgModel::default())
    }

    fn cgb() -> CgbBus {
        CgbBus::new(ram_cart(true), CgbModel::CgbE, false)
    }

    /// Scribble over bus state the CPU can reach on either model.
    fn dirty<B: BusSnapshot + GbBus>(bus: &mut B, fill: u8) {
        bus.write(0xFF50, 0x01); // unmap the boot ROM
        for addr in [0xC000, 0xC123, 0xDFFF, 0xFF80, 0xFFFE] {
            bus.write(addr, fill);
        }
        bus.write(0xFF0F, 0x1F); // IF
        bus.write(0xFFFF, 0x15); // IE
        bus.write(0xFF01, fill); // SB
        bus.write(0xFF02, 0x81); // SC: internal-clock transfer running
        bus.write(0xFF06, fill); // TMA
        bus.write(0xFF07, 0x05); // TAC
        bus.write(0xFF46, 0xC0); // OAM DMA from WRAM
        bus.tick(37);
    }

    fn json<B: BusSnapshot>(bus: &B) -> serde_json::Value {
        serde_json::to_value(bus.capture_bus_state()).expect("serialisable")
    }

    /// Restoring a state reproduces the captured bus, whatever the target
    /// held before. The PPU is compared against a second restore rather than
    /// against the source, since a restore fixes up the PPU's derived sampler
    /// configuration, which a bus fresh from its constructor has not had.
    fn round_trip<B: BusSnapshot + GbBus>(make: fn() -> B) {
        let mut source = make();
        dirty(&mut source, 0x5A);
        let state = source.capture_bus_state();
        let mut target = make();
        dirty(&mut target, 0xA5);
        target.restore_bus_state(&state).expect("same bus type");
        let mut clean_target = make();
        clean_target
            .restore_bus_state(&state)
            .expect("same bus type");

        let mut differing = Vec::new();
        diff_paths("", &json(&target), &json(&clean_target), &mut differing);
        let mut from_source = json(&source);
        let mut restored = json(&target);
        from_source.as_object_mut().expect("object").remove("ppu");
        restored.as_object_mut().expect("object").remove("ppu");
        diff_paths("", &restored, &from_source, &mut differing);
        assert!(
            differing.is_empty(),
            "restored state differs at {differing:?}"
        );
        assert_eq!(target.shared().ppu.vram, source.shared().ppu.vram);
        assert_eq!(target.shared().ppu.oam, source.shared().ppu.oam);
    }

    /// Collect the JSON paths at which `a` and `b` differ.
    fn diff_paths(path: &str, a: &serde_json::Value, b: &serde_json::Value, out: &mut Vec<String>) {
        match (a, b) {
            (serde_json::Value::Object(x), serde_json::Value::Object(y)) => {
                for (key, value) in x {
                    match y.get(key) {
                        Some(other) => diff_paths(&format!("{path}.{key}"), value, other, out),
                        None => out.push(format!("{path}.{key}")),
                    }
                }
            }
            _ if a != b => out.push(path.to_string()),
            _ => {}
        }
    }

    #[test]
    fn capture_then_restore_reproduces_the_bus() {
        round_trip(dmg);
        round_trip(cgb);
    }

    fn refuse<A: BusSnapshot, B: BusSnapshot>(from: A, mut into: B, expected: &str) {
        let error = into
            .restore_bus_state(&from.capture_bus_state())
            .expect_err("other bus type");
        assert!(
            error.contains(&format!("bus type mismatch: expected {expected}")),
            "{error}"
        );
    }

    #[test]
    fn a_state_from_the_other_bus_type_is_refused_by_name() {
        refuse(cgb(), dmg(), "DMG");
        refuse(dmg(), cgb(), "CGB");
    }

    fn fresh_serial<B: BusSnapshot + GbBus>(make: fn() -> B) {
        let mut source = make();
        dirty(&mut source, 0x5A);
        let mut state = source.capture_bus_state();
        state.sb = None;
        state.sc = None;
        state.serial_buf = None;
        state.serial_bits_remaining = None;
        state.serial_master_clock = None;

        let mut target = make();
        dirty(&mut target, 0xA5);
        target.restore_bus_state(&state).expect("same bus type");

        let restored = target.capture_bus_state();
        let fresh = make().capture_bus_state();
        assert_eq!(
            (restored.sb, restored.sc, restored.serial_bits_remaining),
            (fresh.sb, fresh.sc, fresh.serial_bits_remaining)
        );
        assert_eq!(restored.serial_buf, fresh.serial_buf);
        assert_eq!(restored.serial_master_clock, fresh.serial_master_clock);
    }

    #[test]
    fn a_state_without_serial_fields_restores_a_fresh_serial() {
        fresh_serial(dmg);
        fresh_serial(cgb);
    }

    fn boot_rom_unmapped<B: BusSnapshot>(make: fn() -> B) {
        let mut state = make().capture_bus_state();
        assert_eq!(state.boot_rom_active, Some(true), "fresh bus runs boot ROM");
        state.boot_rom_active = None;
        let mut target = make();
        target.restore_bus_state(&state).expect("same bus type");
        assert!(!target.shared().boot_rom_active);
    }

    #[test]
    fn a_state_without_boot_rom_active_restores_the_boot_rom_unmapped() {
        boot_rom_unmapped(dmg);
        boot_rom_unmapped(cgb);
    }

    fn reset_keeps<B: BusSnapshot + GbBus>(make: fn() -> B) {
        let mut bus = make();
        bus.shared_mut().apu.set_sample_rate(22_050.0);
        bus.write(0x0000, 0x0A); // enable cartridge RAM
        bus.write(0xA000, 0x5A);
        dirty(&mut bus, 0x33);
        bus.reset();
        assert_eq!(bus.shared().apu.sample_rate(), 22_050.0);
        bus.write(0x0000, 0x0A);
        assert_eq!(bus.read(0xA000), 0x5A);
        assert!(bus.shared().boot_rom_active, "reset maps the boot ROM");
    }

    #[test]
    fn reset_keeps_the_audio_sample_rate_and_the_cartridge_ram() {
        reset_keeps(dmg);
        reset_keeps(cgb);
    }
}
