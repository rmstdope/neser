//! PPU dot/scanline timing.
//!
//! The bus calls [`Ppu::tick`] once per master clock. The PPU accumulates master clocks and
//! advances one dot every [`MASTER_CYCLES_PER_DOT`] cycles, wrapping the dot counter at
//! [`DOTS_PER_SCANLINE`] and the scanline counter at [`NTSC_SCANLINES_PER_FRAME`].
//!
//! Note: long/short-dot quirks (dot 323/327, 1364-vs-1360 scanlines) are not yet modeled and are
//! a documented refinement TODO.

use super::{DOTS_PER_SCANLINE, MASTER_CYCLES_PER_DOT, NTSC_SCANLINES_PER_FRAME, Ppu};

impl Ppu {
    /// Advance the PPU by one master clock.
    pub fn tick(&mut self) {
        self.master_cycle_accumulator += 1;
        if self.master_cycle_accumulator < MASTER_CYCLES_PER_DOT {
            return;
        }
        self.master_cycle_accumulator -= MASTER_CYCLES_PER_DOT;
        self.advance_dot();
    }

    fn advance_dot(&mut self) {
        self.position.dot += 1;
        if self.position.dot >= DOTS_PER_SCANLINE {
            self.position.dot = 0;
            self.position.scanline += 1;
            if self.position.scanline >= NTSC_SCANLINES_PER_FRAME {
                self.position.scanline = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{DOTS_PER_SCANLINE, MASTER_CYCLES_PER_DOT, Ppu, ScanPosition};

    #[test]
    fn tick_should_advance_one_dot_every_four_master_clocks() {
        let mut ppu = Ppu::new();

        for _ in 0..MASTER_CYCLES_PER_DOT {
            ppu.tick();
        }

        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 0,
                dot: 1
            }
        );
    }

    #[test]
    fn tick_should_advance_scanline_and_wrap_the_dot_counter() {
        let mut ppu = Ppu::new();

        for _ in 0..(DOTS_PER_SCANLINE as u32 * MASTER_CYCLES_PER_DOT) {
            ppu.tick();
        }

        assert_eq!(
            ppu.position(),
            ScanPosition {
                scanline: 1,
                dot: 0
            }
        );
    }
}
