use super::rom_runner::{
    FAIL_STATUS, PASS_IDLE_PC, PASS_STATUS, RunConfig, RunExitReason, RunOracle,
    run_rom_with_oracle,
};

struct RomPassFailCase {
    name: &'static str,
    oracle: RunOracle,
    max_ticks: u64,
    max_frames: u32,
}

fn run_case(rom: &[u8], case: &RomPassFailCase) -> bool {
    let result = run_rom_with_oracle(
        rom,
        case.name,
        RunConfig::new(case.max_ticks, case.max_frames),
        case.oracle,
    );

    result.passed && result.exit_reason == RunExitReason::PassMarker
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pass_bus_byte_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x10000];
        write_lorom_header(&mut rom);

        let mut cursor = 0usize;
        emit_write_long(&mut rom, &mut cursor, 0x7E_1FE1, PASS_STATUS);
        emit_jmp_abs(&mut rom, &mut cursor, PASS_IDLE_PC);
        write_idle_loop(&mut rom, PASS_IDLE_PC);
        rom
    }

    fn write_lorom_header(rom: &mut [u8]) {
        let header = 0x7FC0;
        let title = b"SNES ROM SUITE T";
        rom[header..header + 21].fill(b' ');
        rom[header..header + title.len()].copy_from_slice(title);
        rom[header + 0x15] = 0x20;
        rom[header + 0x16] = 0x00;
        rom[header + 0x17] = 0x07;
        rom[header + 0x18] = 0x00;
        rom[header + 0x1C] = 0x34;
        rom[header + 0x1D] = 0x12;
        rom[header + 0x1E] = 0xCB;
        rom[header + 0x1F] = 0xED;
        rom[header + 0x3C] = 0x00;
        rom[header + 0x3D] = 0x80;
    }

    fn emit_write_long(rom: &mut [u8], cursor: &mut usize, addr: u32, value: u8) {
        rom[*cursor] = 0xA9;
        rom[*cursor + 1] = value;
        rom[*cursor + 2] = 0x8F;
        rom[*cursor + 3] = (addr & 0xFF) as u8;
        rom[*cursor + 4] = ((addr >> 8) & 0xFF) as u8;
        rom[*cursor + 5] = ((addr >> 16) & 0xFF) as u8;
        *cursor += 6;
    }

    fn emit_jmp_abs(rom: &mut [u8], cursor: &mut usize, addr: u16) {
        rom[*cursor] = 0x4C;
        rom[*cursor + 1] = (addr & 0x00FF) as u8;
        rom[*cursor + 2] = (addr >> 8) as u8;
        *cursor += 3;
    }

    fn write_idle_loop(rom: &mut [u8], pc: u16) {
        let mut cursor = usize::from(pc - 0x8000);
        emit_jmp_abs(rom, &mut cursor, pc);
    }

    #[test]
    fn given_bus_oracle_case_when_run_case_then_reports_pass() {
        let case = RomPassFailCase {
            name: "fixture-bus-pass",
            oracle: RunOracle::BusByte {
                addr: 0x7E_1FE1,
                pass_value: PASS_STATUS,
                fail_value: FAIL_STATUS,
            },
            max_ticks: 10_000,
            max_frames: 2,
        };

        assert!(run_case(&pass_bus_byte_rom(), &case));
    }
}
