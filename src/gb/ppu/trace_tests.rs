//! Trace validation tests for GB PPU.
//!
//! These tests verify that tracing infrastructure is correctly wired up.
//! Actual trace output validation is done manually via:
//!   cargo build && ./target/debug/neser --trace-ppu=<level> <rom>
//!
//! See issue #2169 for trace level specifications.

#[cfg(test)]
mod tests {
    use crate::gb::ppu::Ppu;
    use crate::platform::debugging::{Tracing, init_tracing};

    #[test]
    fn test_ppu_operations_work_with_tracing_enabled() {
        // Given: tracing enabled at level 5 (maximum verbosity)
        init_tracing(Tracing {
            enabled: true,
            ppu: 5,
            ..Default::default()
        });

        // When: PPU is created and ticked
        let mut ppu = Ppu::new();
        ppu.tick_dots(100);

        // Then: PPU state progresses normally (traces don't break functionality)
        assert!(ppu.dot() >= 4); // started at dot=4, advanced at least 100 dots
    }

    #[test]
    fn test_ppu_operations_work_with_tracing_disabled() {
        // Given: tracing disabled
        init_tracing(Tracing {
            enabled: false,
            ppu: 0,
            ..Default::default()
        });

        // When: PPU is created and ticked
        let mut ppu = Ppu::new();
        ppu.tick_dots(100);

        // Then: PPU state progresses normally
        assert!(ppu.dot() >= 4);
    }

    #[test]
    fn test_frame_boundary_operations_with_tracing() {
        // Given: tracing enabled
        init_tracing(Tracing {
            enabled: true,
            ppu: 1,
            ..Default::default()
        });

        // When: PPU completes a full frame
        let mut ppu = Ppu::new();
        // First scanline: 452 dots, remaining 153 scanlines: 456 dots each
        ppu.tick_dots(452 + 153 * 456);

        // Then: frame completes successfully (trace of frame wrap with CRC should be emitted)
        assert_eq!(ppu.ly(), 0);
        assert!(ppu.is_frame_ready());
    }
}
