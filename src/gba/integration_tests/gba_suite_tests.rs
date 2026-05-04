use super::gba_suite_runner::{Suite, run_suite};

#[test]
fn gba_suite_arm_rom_passes() {
    let result = run_suite(Suite::Arm);
    assert!(
        result.passed,
        "arm suite failed: index={} reg={} pc=0x{:08X} cpsr=0x{:08X} thumb={} opcode=0x{:08X} cycles={} exit={:?}",
        result.failing_index,
        result.reg_name,
        result.pc,
        result.cpsr,
        result.thumb,
        result.opcode_at_pc,
        result.cycles,
        result.exit_reason
    );
}

#[test]
fn gba_suite_thumb_rom_passes() {
    let result = run_suite(Suite::Thumb);
    assert!(
        result.passed,
        "thumb suite failed: index={} reg={} pc=0x{:08X} cpsr=0x{:08X} thumb={} opcode=0x{:08X} cycles={} exit={:?}",
        result.failing_index,
        result.reg_name,
        result.pc,
        result.cpsr,
        result.thumb,
        result.opcode_at_pc,
        result.cycles,
        result.exit_reason
    );
}

#[test]
fn gba_suite_nes_rom_passes() {
    let result = run_suite(Suite::Nes);
    assert!(
        result.passed,
        "nes suite failed: index={} reg={} pc=0x{:08X} cpsr=0x{:08X} thumb={} opcode=0x{:08X} cycles={} exit={:?}",
        result.failing_index,
        result.reg_name,
        result.pc,
        result.cpsr,
        result.thumb,
        result.opcode_at_pc,
        result.cycles,
        result.exit_reason
    );
}
