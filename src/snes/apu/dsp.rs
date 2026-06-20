//! SNES S-DSP voice pipeline (work in progress).

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedBrrBlock {
    pub samples: [i16; 16],
    pub loop_flag: bool,
    pub end_flag: bool,
}

#[derive(Debug, Clone)]
pub struct Sdsp {
    phase: u8,
    regs: [u8; 0x80],
}

impl Default for Sdsp {
    fn default() -> Self {
        Self::new()
    }
}

impl Sdsp {
    #[must_use]
    pub fn new() -> Self {
        Self {
            phase: 0,
            regs: [0; 0x80],
        }
    }

    #[must_use]
    pub fn phase(&self) -> u8 {
        self.phase
    }

    pub fn step_phase(&mut self) {
        self.phase = self.phase.wrapping_add(1) & 0x1F;
    }

    pub fn write_reg(&mut self, addr: u8, value: u8) {
        self.regs[usize::from(addr & 0x7F)] = value;
    }

    #[must_use]
    pub fn read_reg(&self, addr: u8) -> u8 {
        self.regs[usize::from(addr & 0x7F)]
    }

    #[must_use]
    pub fn decode_brr_block(header: u8, data: [u8; 8], prev1: i16, prev2: i16) -> DecodedBrrBlock {
        let _ = (data, prev1, prev2);
        DecodedBrrBlock {
            samples: [0; 16],
            loop_flag: header & 0x02 != 0,
            end_flag: header & 0x01 != 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Sdsp;

    #[test]
    fn given_phase_31_when_step_phase_then_wraps_to_0() {
        let mut dsp = Sdsp::new();
        for _ in 0..31 {
            dsp.step_phase();
        }
        assert_eq!(dsp.phase(), 31);

        dsp.step_phase();
        assert_eq!(dsp.phase(), 0);
    }

    #[test]
    fn given_all_register_addresses_when_written_then_reads_back_same_value() {
        let mut dsp = Sdsp::new();
        for addr in 0u8..=0x7F {
            let value = addr.wrapping_mul(3).wrapping_add(1);
            dsp.write_reg(addr, value);
        }

        for addr in 0u8..=0x7F {
            let value = addr.wrapping_mul(3).wrapping_add(1);
            assert_eq!(dsp.read_reg(addr), value, "addr=0x{addr:02X}");
        }
    }

    #[test]
    fn given_mirrored_register_addresses_when_written_then_base_registers_match() {
        let mut dsp = Sdsp::new();

        dsp.write_reg(0x95, 0xAB);
        assert_eq!(dsp.read_reg(0x15), 0xAB);
        assert_eq!(dsp.read_reg(0x95), 0xAB);
    }

    #[test]
    fn given_brr_header_with_loop_and_end_bits_when_decoded_then_flags_are_exposed() {
        let header = 0b0000_0011;
        let decoded = Sdsp::decode_brr_block(header, [0; 8], 0, 0);
        assert!(decoded.loop_flag);
        assert!(decoded.end_flag);
    }
}
