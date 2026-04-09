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
        let before = self.cpu.cycles();
        self.cpu.execute();
        let delta = (self.cpu.cycles() - before) as u8;
        self.cpu.bus.tick(delta);
    }

    /// Total M-cycles elapsed.
    pub fn cycles(&self) -> u64 {
        self.cpu.cycles()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bus that counts the total M-cycles passed to `tick()`.
    struct TrackingBus {
        mem: [u8; 0x10000],
        ticked_cycles: u64,
    }

    impl TrackingBus {
        fn with_program(program: &[u8]) -> Self {
            let mut mem = [0u8; 0x10000];
            mem[..program.len()].copy_from_slice(program);
            Self {
                mem,
                ticked_cycles: 0,
            }
        }
    }

    impl GbBus for TrackingBus {
        fn read(&mut self, addr: u16) -> u8 {
            self.mem[addr as usize]
        }
        fn write(&mut self, addr: u16, val: u8) {
            self.mem[addr as usize] = val;
        }
        fn tick(&mut self, m_cycles: u8) {
            self.ticked_cycles += m_cycles as u64;
        }
    }

    #[test]
    fn test_step_ticks_bus_by_nop_m_cycle_count() {
        // Given: a NOP instruction at $0000 (costs 1 M-cycle)
        let bus = TrackingBus::with_program(&[0x00]); // NOP
        let mut console = Gb::new(bus);
        let before = console.cpu.cycles();
        // When: step executes one instruction
        console.step();
        let delta = console.cpu.cycles() - before;
        // Then: the bus was ticked by the NOP's M-cycle count (1)
        assert_eq!(delta, 1);
        assert_eq!(console.cpu.bus.ticked_cycles, delta);
    }

    #[test]
    fn test_step_ticks_bus_by_multi_cycle_instruction_cost() {
        // LD BC, nn is a 3-byte instruction costing 3 M-cycles
        let bus = TrackingBus::with_program(&[0x01, 0x00, 0x00]); // LD BC, $0000
        let mut console = Gb::new(bus);
        // When: step executes one instruction
        console.step();
        // Then: the bus was ticked by 3 M-cycles
        assert_eq!(console.cpu.bus.ticked_cycles, 3);
    }
}
