use crate::nes::Nes;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebuggerSnapshot {
    pub cpu: String,
    pub ppu: String,
    pub apu: String,
}

pub fn snapshot(nes: &Nes) -> DebuggerSnapshot {
    let cpu_cycles = nes.cpu.get_total_cycles();

    let cpu = format!(
        "CPU\n\
PC: {pc:04X}  A: {a:02X} X: {x:02X} Y: {y:02X}  SP: {sp:02X}  P: {p:02X}\n\
CYC: {cycles}",
        pc = nes.cpu.pc,
        a = nes.cpu.a,
        x = nes.cpu.x,
        y = nes.cpu.y,
        sp = nes.cpu.sp,
        p = nes.cpu.p,
        cycles = cpu_cycles,
    );

    let (scanline, pixel) = {
        let ppu = nes.ppu.borrow();
        (ppu.scanline(), ppu.pixel())
    };

    let ppu = format!(
        "PPU\n\
scanline: {scanline:3}  pixel: {pixel:3}",
        scanline = scanline,
        pixel = pixel
    );

    let (apu_cycle, frame_counter_cycle) = {
        let apu = nes.apu.borrow();
        (apu.apu_cycle(), apu.debug_frame_counter_cycle())
    };

    let apu = format!(
        "APU\n\
apu_cycle: {apu_cycle}  frame_counter_cycle: {frame_counter_cycle}",
        apu_cycle = apu_cycle,
        frame_counter_cycle = frame_counter_cycle
    );

    DebuggerSnapshot {
        cpu,
        ppu,
        apu,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::{Nes, TvSystem};

    #[test]
    fn test_snapshot_contains_basic_cpu_ppu_apu_info() {
        let mut nes = Nes::new(TvSystem::Ntsc);

        // Seed a couple of CPU registers so the snapshot has something meaningful.
        nes.cpu.pc = 0xC000;
        nes.cpu.a = 0x12;
        nes.cpu.x = 0x34;
        nes.cpu.y = 0x56;
        nes.cpu.sp = 0xFD;
        nes.cpu.p = 0x24;

        let snap = snapshot(&nes);

        assert!(snap.cpu.contains("PC"));
        assert!(snap.cpu.contains("A"));
        assert!(snap.cpu.contains("X"));
        assert!(snap.cpu.contains("Y"));
        assert!(snap.cpu.contains("SP"));
        assert!(snap.cpu.contains("P"));

        assert!(snap.ppu.contains("scanline"));
        assert!(snap.ppu.contains("pixel"));

        assert!(snap.apu.contains("apu_cycle"));
    }
}
