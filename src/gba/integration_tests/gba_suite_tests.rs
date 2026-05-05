use super::gba_suite_runner::{Suite, run_suite};

fn assert_suite_passes_with_crc(suite: Suite, name: &str, expected_crc32: u32) {
    let result = run_suite(suite);
    assert!(
        result.passed,
        "{name} suite failed: index={} reg={} pc=0x{:08X} cpsr=0x{:08X} thumb={} opcode=0x{:08X} fb_crc=0x{:08X} cycles={} exit={:?}",
        result.failing_index,
        result.reg_name,
        result.pc,
        result.cpsr,
        result.thumb,
        result.opcode_at_pc,
        result.framebuffer_crc32,
        result.cycles,
        result.exit_reason
    );
    assert_eq!(
        result.framebuffer_crc32, expected_crc32,
        "{name} suite framebuffer CRC mismatch: expected=0x{expected_crc32:08X} actual=0x{:08X}",
        result.framebuffer_crc32
    );
}

#[test]
fn gba_suite_arm_rom_passes() {
    assert_suite_passes_with_crc(Suite::Arm, "arm", 0x12FD_AE0B);
}

#[test]
fn gba_suite_thumb_rom_passes() {
    assert_suite_passes_with_crc(Suite::Thumb, "thumb", 0x12FD_AE0B);
}

#[test]
fn gba_suite_nes_rom_passes() {
    assert_suite_passes_with_crc(Suite::Nes, "nes", 0x12FD_AE0B);
}

#[test]
fn gba_suite_memory_rom_passes() {
    assert_suite_passes_with_crc(Suite::Memory, "memory", 0x12FD_AE0B);
}

#[test]
fn gba_suite_ppu_hello_rom_passes() {
    assert_suite_passes_with_crc(Suite::PpuHello, "ppu hello", 0x52F9_B8A4);
}

#[test]
fn gba_suite_ppu_shades_rom_passes() {
    assert_suite_passes_with_crc(Suite::PpuShades, "ppu shades", 0x9CD9_40F8);
}

#[test]
fn gba_suite_ppu_stripes_rom_passes() {
    assert_suite_passes_with_crc(Suite::PpuStripes, "ppu stripes", 0xFBAB_D04A);
}
