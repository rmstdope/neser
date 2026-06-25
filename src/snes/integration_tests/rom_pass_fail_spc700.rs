use super::rom_runner::{
    FAIL_STATUS, PASS_IDLE_PC, PASS_STATUS, RunConfig, RunExitReason, RunOracle, RunResult,
    run_rom_with_oracle,
};

#[derive(Debug, Clone, Copy)]
struct RomPassFailCase {
    name: &'static str,
    oracle: RunOracle,
    max_ticks: u64,
    max_frames: u32,
}

#[derive(Debug, Clone)]
struct RomPassFailOutcome {
    name: &'static str,
    result: RunResult,
}

impl RomPassFailOutcome {
    fn passed(&self) -> bool {
        self.result.passed && self.result.exit_reason == RunExitReason::PassMarker
    }

    fn failed_with_marker(&self) -> bool {
        !self.result.passed && self.result.exit_reason == RunExitReason::FailMarker
    }
}

fn run_case(rom: &[u8], case: &RomPassFailCase) -> RomPassFailOutcome {
    let result = run_rom_with_oracle(
        rom,
        case.name,
        RunConfig::new(case.max_ticks, case.max_frames),
        case.oracle,
    );

    RomPassFailOutcome {
        name: case.name,
        result,
    }
}

fn run_catalog<'a>(catalog: &'a [(RomPassFailCase, &'a [u8])]) -> Vec<RomPassFailOutcome> {
    catalog
        .iter()
        .map(|(case, rom)| run_case(rom, case))
        .collect()
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

    fn fail_marker_rom() -> Vec<u8> {
        use super::super::rom_runner::{FAIL_IDLE_PC, MARKER_ADDR, MARKER_MAGIC};

        let mut rom = vec![0u8; 0x10000];
        write_lorom_header(&mut rom);

        let mut cursor = 0usize;
        for (offset, byte) in MARKER_MAGIC.iter().copied().enumerate() {
            emit_write_long(&mut rom, &mut cursor, MARKER_ADDR + offset as u32, byte);
        }
        emit_write_long(&mut rom, &mut cursor, MARKER_ADDR + 4, FAIL_STATUS);
        emit_jmp_abs(&mut rom, &mut cursor, FAIL_IDLE_PC);
        write_idle_loop(&mut rom, PASS_IDLE_PC);
        write_idle_loop(&mut rom, FAIL_IDLE_PC);
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

        let outcome = run_case(&pass_bus_byte_rom(), &case);

        assert!(
            outcome.passed(),
            "expected pass outcome for {}",
            outcome.name
        );
    }

    #[test]
    fn given_marker_oracle_fail_rom_when_run_case_then_reports_marker_fail() {
        let case = RomPassFailCase {
            name: "fixture-marker-fail",
            oracle: RunOracle::Marker,
            max_ticks: 10_000,
            max_frames: 2,
        };

        let outcome = run_case(&fail_marker_rom(), &case);

        assert!(
            outcome.failed_with_marker(),
            "expected marker fail outcome for {}",
            outcome.name
        );
    }

    #[test]
    fn given_mixed_catalog_when_run_catalog_then_reports_each_case_outcome() {
        let pass_rom = pass_bus_byte_rom();
        let fail_rom = fail_marker_rom();
        let catalog: Vec<(RomPassFailCase, &[u8])> = vec![
            (
                RomPassFailCase {
                    name: "catalog-bus-pass",
                    oracle: RunOracle::BusByte {
                        addr: 0x7E_1FE1,
                        pass_value: PASS_STATUS,
                        fail_value: FAIL_STATUS,
                    },
                    max_ticks: 10_000,
                    max_frames: 2,
                },
                pass_rom.as_slice(),
            ),
            (
                RomPassFailCase {
                    name: "catalog-marker-fail",
                    oracle: RunOracle::Marker,
                    max_ticks: 10_000,
                    max_frames: 2,
                },
                fail_rom.as_slice(),
            ),
        ];

        let outcomes = run_catalog(&catalog);

        assert_eq!(outcomes.len(), 2);
        assert!(outcomes[0].passed());
        assert!(outcomes[1].failed_with_marker());
    }
}
