use crate::gb::bus::GbBus;
use crate::gb::cpu::Sm83;

/// Game Boy (DMG) console stub.
///
/// Wraps the SM83 CPU and a bus. This is a minimal integration shell;
/// rendering, audio, and input are out of scope for the initial CPU sub-issue.
pub struct Gb<B: GbBus> {
    pub cpu: Sm83<B>,
}

impl<B: GbBus> Gb<B> {
    pub fn new(bus: B) -> Self {
        Self {
            cpu: Sm83::new(bus),
        }
    }

    /// Step one CPU instruction.
    pub fn step(&mut self) {
        self.cpu.execute();
    }

    /// Total M-cycles elapsed.
    pub fn cycles(&self) -> u64 {
        self.cpu.cycles()
    }
}
