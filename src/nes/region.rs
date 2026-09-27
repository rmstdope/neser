//! Region timing parameters: everything that differs between an NTSC, a PAL and a Dendy console.
//!
//! A region is one row here, looked up once from the [`TimingMode`] by
//! [`TimingMode::region`]. The NES subsystems read its fields rather than matching the timing
//! mode themselves, so a region is defined, and corrected, in one place.

use crate::nes::cartridge::TimingMode;

/// The APU frame counter's 4-step sequence, in CPU cycles.
#[derive(Debug, PartialEq, Eq)]
pub struct FourStepSequence {
    /// Cycles of the four quarter-frame clocks (steps 2 and 4 are also half frames).
    pub steps: [u32; 4],
    /// Cycle at which the frame IRQ starts asserting.
    pub irq_cycle: u32,
    /// Cycle at which the sequence wraps to 0.
    pub frame_cycles: u32,
}

/// Everything about a console's timing that depends on its region.
#[derive(Debug, PartialEq)]
pub struct RegionParams {
    /// The region's name as a player reads it.
    pub name: &'static str,
    pub cpu_clock_hz: f32,
    /// Master-clock ticks per CPU cycle.
    pub cpu_divider: u64,
    /// Master-clock ticks per PPU dot.
    pub ppu_divider: u64,
    /// Master-clock ticks consumed before the bus access within one CPU cycle.
    pub bus_start_clock: u64,
    pub scanlines_per_frame: u16,
    pub prerender_scanline: u16,
    /// Scanline on which VBlank (and NMI) begins.
    pub vblank_start_scanline: u16,
    /// Whether odd frames with rendering on skip the last pre-render dot.
    pub odd_frame_skip: bool,
    /// Whether OAM DRAM decays while rendering is off.
    pub oam_decay: bool,
    pub four_step: FourStepSequence,
    /// The 5-step sequence's steps 1, 2, 3 and 5 (step 4 clocks nothing).
    pub five_step: [u32; 4],
    /// Noise channel timer periods, in CPU cycles.
    pub noise_periods: [u16; 16],
    /// DMC rate periods, in CPU cycles.
    pub dmc_rates: [u16; 16],
}

impl RegionParams {
    /// PPU dots per CPU cycle: the ratio of the two master-clock dividers.
    pub fn ppu_cycles_per_cpu_cycle(&self) -> f64 {
        self.cpu_divider as f64 / self.ppu_divider as f64
    }
}

const NTSC_NOISE_PERIODS: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];
const PAL_NOISE_PERIODS: [u16; 16] = [
    4, 8, 14, 30, 60, 88, 118, 148, 188, 236, 354, 472, 708, 944, 1890, 3778,
];
const NTSC_DMC_RATES: [u16; 16] = [
    428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54,
];
const PAL_DMC_RATES: [u16; 16] = [
    398, 354, 316, 298, 276, 236, 210, 198, 176, 148, 132, 118, 98, 78, 66, 50,
];
const NTSC_FOUR_STEP: FourStepSequence = FourStepSequence {
    steps: [7457, 14913, 22371, 29829],
    irq_cycle: 29828,
    frame_cycles: 29830,
};
const NTSC_FIVE_STEP: [u32; 4] = [7457, 14913, 22371, 37281];

// Dividers from Mesen2 NesCpu.cpp SetMasterClockDivider():
//   NTSC  - cpu=12, ppu=4, start=6  (PPU:CPU = 3.0, symmetric)
//   PAL   - cpu=16, ppu=5, start=8  (PPU:CPU = 3.2, symmetric)
//   Dendy - cpu=15, ppu=5, start=7  (PPU:CPU = 3.0, asymmetric: end=8)

pub static NTSC: RegionParams = RegionParams {
    name: "NTSC",
    cpu_clock_hz: 1_789_773.0,
    cpu_divider: 12,
    ppu_divider: 4,
    bus_start_clock: 6,
    scanlines_per_frame: 262,
    prerender_scanline: 261,
    vblank_start_scanline: 241,
    odd_frame_skip: true,
    oam_decay: true,
    four_step: NTSC_FOUR_STEP,
    five_step: NTSC_FIVE_STEP,
    noise_periods: NTSC_NOISE_PERIODS,
    dmc_rates: NTSC_DMC_RATES,
};

pub static PAL: RegionParams = RegionParams {
    name: "PAL",
    cpu_clock_hz: 1_662_607.0,
    cpu_divider: 16,
    ppu_divider: 5,
    bus_start_clock: 8,
    scanlines_per_frame: 312,
    prerender_scanline: 311,
    vblank_start_scanline: 241,
    odd_frame_skip: false,
    oam_decay: false,
    four_step: FourStepSequence {
        steps: [8313, 16627, 24939, 33253],
        irq_cycle: 33252,
        frame_cycles: 33254,
    },
    five_step: [8313, 16627, 24939, 41565],
    noise_periods: PAL_NOISE_PERIODS,
    dmc_rates: PAL_DMC_RATES,
};

/// Dendy: a PAL-length frame (312 lines) with 50 post-render lines before VBlank at 291
/// (Mesen2 NesPpu.cpp UpdateTimings), a 26.601712 MHz master clock divided by 15 for the CPU
/// (1,773,447.5 Hz, rounded), and the NTSC APU tables.
pub static DENDY: RegionParams = RegionParams {
    name: "Dendy",
    cpu_clock_hz: 1_773_448.0,
    cpu_divider: 15,
    ppu_divider: 5,
    bus_start_clock: 7,
    scanlines_per_frame: 312,
    prerender_scanline: 311,
    vblank_start_scanline: 291,
    odd_frame_skip: false,
    oam_decay: false,
    four_step: NTSC_FOUR_STEP,
    five_step: NTSC_FIVE_STEP,
    noise_periods: NTSC_NOISE_PERIODS,
    dmc_rates: NTSC_DMC_RATES,
};

impl TimingMode {
    /// This timing mode's region parameters. The only place a timing mode is matched to its
    /// timing; `MultiRegion` and unknown values run as NTSC.
    pub fn region(self) -> &'static RegionParams {
        match self {
            Self::Pal => &PAL,
            Self::Dendy => &DENDY,
            Self::Ntsc | Self::MultiRegion | Self::Unknown(_) => &NTSC,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ntsc_row_matches_current_timing() {
        let r = TimingMode::Ntsc.region();
        assert_eq!(r.name, "NTSC");
        assert_eq!(r.cpu_clock_hz, 1_789_773.0);
        assert_eq!(
            (r.cpu_divider, r.ppu_divider, r.bus_start_clock),
            (12, 4, 6)
        );
        assert_eq!(r.scanlines_per_frame, 262);
        assert_eq!(r.prerender_scanline, 261);
        assert_eq!(r.vblank_start_scanline, 241);
        assert!(r.odd_frame_skip);
        assert!(r.oam_decay);
        assert_eq!(r.four_step.steps, [7457, 14913, 22371, 29829]);
        assert_eq!(
            (r.four_step.irq_cycle, r.four_step.frame_cycles),
            (29828, 29830)
        );
        assert_eq!(r.five_step, [7457, 14913, 22371, 37281]);
        assert_eq!(r.noise_periods[2], 16);
        assert_eq!(r.noise_periods[15], 4068);
        assert_eq!(r.dmc_rates[0], 428);
        assert_eq!(r.dmc_rates[15], 54);
    }

    #[test]
    fn pal_row_matches_current_timing() {
        let r = TimingMode::Pal.region();
        assert_eq!(r.name, "PAL");
        assert_eq!(r.cpu_clock_hz, 1_662_607.0);
        assert_eq!(
            (r.cpu_divider, r.ppu_divider, r.bus_start_clock),
            (16, 5, 8)
        );
        assert_eq!(r.scanlines_per_frame, 312);
        assert_eq!(r.prerender_scanline, 311);
        assert_eq!(r.vblank_start_scanline, 241);
        assert!(!r.odd_frame_skip);
        assert!(!r.oam_decay);
        assert_eq!(r.four_step.steps, [8313, 16627, 24939, 33253]);
        assert_eq!(
            (r.four_step.irq_cycle, r.four_step.frame_cycles),
            (33252, 33254)
        );
        assert_eq!(r.five_step, [8313, 16627, 24939, 41565]);
        assert_eq!(r.noise_periods[2], 14);
        assert_eq!(r.noise_periods[15], 3778);
        assert_eq!(r.dmc_rates[0], 398);
        assert_eq!(r.dmc_rates[15], 50);
    }

    /// Dendy: PAL frame (312 lines) with VBlank at 291, CPU /15, and the NTSC APU tables
    /// (#1889, #1890).
    #[test]
    fn dendy_row_matches_current_timing() {
        let r = TimingMode::Dendy.region();
        assert_eq!(r.name, "Dendy");
        assert_eq!(r.cpu_clock_hz, 1_773_448.0);
        assert_eq!(
            (r.cpu_divider, r.ppu_divider, r.bus_start_clock),
            (15, 5, 7)
        );
        assert_eq!(r.scanlines_per_frame, 312);
        assert_eq!(r.prerender_scanline, 311);
        assert_eq!(r.vblank_start_scanline, 291);
        assert!(!r.odd_frame_skip);
        assert!(!r.oam_decay);
        assert_eq!(r.four_step.steps, NTSC.four_step.steps);
        assert_eq!(r.four_step.irq_cycle, NTSC.four_step.irq_cycle);
        assert_eq!(r.four_step.frame_cycles, NTSC.four_step.frame_cycles);
        assert_eq!(r.five_step, NTSC.five_step);
        assert_eq!(r.noise_periods, NTSC.noise_periods);
        assert_eq!(r.dmc_rates, NTSC.dmc_rates);
    }

    #[test]
    fn multi_region_and_unknown_use_the_ntsc_row() {
        assert!(std::ptr::eq(TimingMode::MultiRegion.region(), &NTSC));
        assert!(std::ptr::eq(TimingMode::Unknown(7).region(), &NTSC));
        assert!(std::ptr::eq(TimingMode::Ntsc.region(), &NTSC));
        assert!(std::ptr::eq(TimingMode::Pal.region(), &PAL));
        assert!(std::ptr::eq(TimingMode::Dendy.region(), &DENDY));
    }

    #[test]
    fn ppu_cycles_per_cpu_cycle_is_divider_ratio() {
        assert_eq!(NTSC.ppu_cycles_per_cpu_cycle(), 3.0);
        assert_eq!(PAL.ppu_cycles_per_cpu_cycle(), 3.2);
        assert_eq!(DENDY.ppu_cycles_per_cpu_cycle(), 3.0);
    }

    /// Outside this module, no NES subsystem names a region to decide its timing: a region
    /// defined in one place cannot fall through a stale `else` in another (#1889, #1890).
    /// That covers naming PAL or Dendy, and the NTSC-only form (`== TimingMode::Ntsc`,
    /// `matches!(…, TimingMode::Ntsc)`) that sends Dendy down the PAL branch unnamed.
    /// Test code, the header parser (`cartridge/ines.rs`, `TimingMode`'s home), and the
    /// hardware-model conversions in `cartridge/hardware_type.rs` and `console/config/` (which
    /// region a ROM asks for, not its timing) may name regions.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn only_the_region_module_matches_on_pal_or_dendy() {
        fn branches_on_region(production: &str) -> bool {
            if production.contains("TimingMode::Pal")
                || production.contains("TimingMode::Dendy")
                || production.contains("== TimingMode::Ntsc")
                || production.contains("!= TimingMode::Ntsc")
            {
                return true;
            }
            production.match_indices("matches!(").any(|(i, _)| {
                let args = &production[i..];
                let end = args.find(')').unwrap_or(args.len());
                args[..end].contains("TimingMode::Ntsc")
            })
        }
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).expect("read src/nes") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/nes");
        let mut files = Vec::new();
        walk(&root, &mut files);
        let mut offenders = Vec::new();
        for path in files {
            let rel = path
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let exempt = rel == "region.rs"
                || rel.starts_with("console/config/")
                || rel == "cartridge/ines.rs"
                || rel == "cartridge/hardware_type.rs"
                || rel.starts_with("integration_tests/")
                || name == "tests.rs"
                || name.ends_with("_test.rs")
                || name.ends_with("_tests.rs");
            if exempt {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("read source");
            let production = source.split("\nmod tests {").next().unwrap_or("");
            if branches_on_region(production) {
                offenders.push(rel);
            }
        }
        offenders.sort();
        assert!(
            offenders.is_empty(),
            "read region timing from `TimingMode::region()` instead of matching the timing mode in: {offenders:?}"
        );
    }
}
