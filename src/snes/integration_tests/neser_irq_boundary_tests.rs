//! End-to-end fixtures pinning WHERE an already-pending IRQ dispatches
//! relative to a `$420B` (MDMAEN) or `$4200` (NMITIMEN) write (issue #3081).
//!
//! Mesen2's `$420B` handler sets only `_dmaPending`/`_dmaStartDelay`
//! (`SnesDmaController.cpp`) and its `$4200` handler has no interrupt lock at
//! all (`InternalRegisters.cpp`), so an interrupt already recognized at the
//! boundary right after the write dispatches immediately — for `$420B`, the
//! started DMA then runs *inside the interrupt entry sequence*. NESER's former
//! instruction-granular `irq_lock_step` instead deferred every interrupt (and
//! the WAI wake) past one further instruction, a behaviour with no reference
//! counterpart that no test pinned in either direction.
//!
//! Scenario (real 65816 + bus + PPU, no timing tuning required): a V-IRQ is
//! latched many scanlines before the measured window, then the program runs
//! `CLI ; STA $42xx ; WAI`. CLI's one-instruction recognition shadow makes the
//! store the single instruction that runs before the dispatch decision, so:
//!
//! - reference behaviour: the IRQ preempts `WAI` — the stacked return PC is
//!   the WAI opcode's own address;
//! - instruction-granular lock: `WAI` executes and wakes — the stacked return
//!   PC is one past the WAI opcode.
//!
//! The handler judges the stacked return PC itself and reports through the
//! WRAM marker protocol, with the raw PC bytes exposed in the result block for
//! diagnostics. The same images were the bus-trace evidence vehicles for the
//! Mesen2 diff mandated by #3081 (see the PR for the trace windows); the
//! builder is deterministic, so writing [`irq_boundary_fixture`]'s bytes to a
//! `.sfc` file reproduces the exact images Mesen2 verified.

use super::fixture_rom::FixtureRom;
use super::rom_runner::{RESULT_ADDR, RunConfig, RunExitReason, run_rom};

/// Result-block layout: `[0]` handler-ran flag, `[1]`/`[2]` stacked return
/// PC low/high.
const HANDLER_RAN: u32 = RESULT_ADDR;
const RETURN_PC_LO: u32 = RESULT_ADDR + 1;
const RETURN_PC_HI: u32 = RESULT_ADDR + 2;

/// The `$42xx` store the scenario places between `CLI` and `WAI`.
enum StoreUnderTest {
    /// `STA $420B` starting a one-byte channel-0 GPDMA (ROM -> WMDATA).
    DmaStart,
    /// `STA $4200` rewriting NMITIMEN with its current value (V-IRQ enabled).
    NmitimenRewrite,
}

/// Builds the fixture and returns `(rom, wai_addr)`; `wai_addr` is the CPU
/// address of the `WAI` opcode, i.e. the return PC the reference behaviour
/// stacks.
fn irq_boundary_fixture(store: StoreUnderTest) -> (Vec<u8>, u16) {
    let mut fx = FixtureRom::new(b"NESER IRQ BOUNDARY");

    // Blank screen; the PPU still counts scanlines, which is all this needs.
    fx.store_imm_abs(0x2100, 0x8F);

    let (store_target, store_value) = match store {
        StoreUnderTest::DmaStart => {
            // One-byte GPDMA: ROM byte -> WMDATA ($2180), landing at WRAM
            // $7E:0200 via WMADD. The transfer itself is incidental; what is
            // under test is where the pending IRQ dispatches around the
            // trigger write.
            fx.store_imm_abs(0x2181, 0x00);
            fx.store_imm_abs(0x2182, 0x02);
            fx.store_imm_abs(0x2183, 0x00);
            let src = fx.place_data(&[0x5A]);
            fx.setup_gpdma(0, 0x00, 0x80, u32::from(src), 1);
            (0x420B, 0x01)
        }
        // Rewriting NMITIMEN with the value it already holds changes nothing
        // on the PPU side (mode unchanged, no NMI-enable edge), isolating the
        // CPU-side dispatch question.
        StoreUnderTest::NmitimenRewrite => (0x4200, 0x20),
    };

    // V-IRQ at scanline 100, NMI and auto-joypad off ($4200 = $20). The line
    // latches at the trigger and stays asserted until $4211 is read, which
    // only the handler does.
    fx.store_imm_abs(0x4209, 100);
    fx.store_imm_abs(0x420A, 0x00);
    fx.store_imm_abs(0x4200, 0x20);

    // Wait through vblank rise, fall, rise on $4212 bit 7: a full visible
    // frame elapses with the V-IRQ enabled, so line 100 has certainly fired
    // and the IRQ line is high long before the measured window -- no HTIME
    // tuning, and no sensitivity to boot alignment.
    for wait_for_set in [true, false, true] {
        let loop_top = fx.pos();
        fx.lda_abs(0x4212);
        fx.and_imm(0x80);
        if wait_for_set {
            fx.beq_to(loop_top);
        } else {
            fx.bne_to(loop_top);
        }
    }

    // The measured window. A is loaded BEFORE the CLI so that the store is
    // the one instruction CLI's recognition shadow lets through.
    fx.lda_imm(store_value);
    fx.cli();
    fx.sta_abs(store_target);
    let wai_addr = 0x8000 + fx.pos() as u16;
    fx.wai();
    // Reachable only if the dispatch never happens (the handler never RTIs).
    fx.branch_fail_if_ne(!store_value); // A == store_value, so this always fails

    // IRQ handler: judge the stacked return PC, exposing it for diagnostics.
    let handler_addr = 0x8000 + fx.pos() as u16;
    fx.tsx();
    fx.lda_abs_x(0x0102); // stacked PCL
    fx.sta_long(RETURN_PC_LO);
    fx.lda_abs_x(0x0103); // stacked PCH
    fx.sta_long(RETURN_PC_HI);
    fx.lda_imm(0x01);
    fx.sta_long(HANDLER_RAN);
    fx.lda_abs(0x4211); // ack TIMEUP
    fx.lda_abs_x(0x0102);
    fx.branch_fail_if_ne((wai_addr & 0xFF) as u8);
    fx.lda_abs_x(0x0103);
    fx.branch_fail_if_ne((wai_addr >> 8) as u8);
    fx.pass_marker_and_idle();

    fx.set_emulation_irq_vector(handler_addr);
    (fx.build(), wai_addr)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_and_expect_reference_boundary(store: StoreUnderTest, name: &str) {
        let (rom, wai_addr) = irq_boundary_fixture(store);
        let result = run_rom(&rom, name, RunConfig::new(400_000_000, 60));
        let stacked_pc =
            u16::from(result.result_bytes[1]) | (u16::from(result.result_bytes[2]) << 8);
        assert_eq!(
            result.exit_reason,
            RunExitReason::PassMarker,
            "{name}: expected the pending IRQ to dispatch at the boundary right \
             after the store (stacked return PC ${wai_addr:04X}, the WAI opcode); \
             handler_ran={} stacked return PC=${stacked_pc:04X} (WAI+1 means the \
             write deferred dispatch by an instruction and WAI ran first)",
            result.result_bytes[0],
        );
        assert_eq!(
            stacked_pc, wai_addr,
            "{name}: handler passed but recorded an unexpected return PC"
        );
    }

    /// #3081: starting a GPDMA via `$420B` must not defer an already-pending
    /// IRQ past the next instruction; the transfer runs inside the interrupt
    /// entry sequence, as in Mesen2.
    #[test]
    fn dma_start_does_not_defer_a_pending_irq_past_the_next_instruction() {
        run_and_expect_reference_boundary(
            StoreUnderTest::DmaStart,
            "neser_irq_boundary_dma_start.sfc",
        );
    }

    /// #3081: writing NMITIMEN must not defer an already-pending IRQ past the
    /// next instruction; Mesen2's `$4200` handler has no interrupt lock.
    #[test]
    fn nmitimen_write_does_not_defer_a_pending_irq_past_the_next_instruction() {
        run_and_expect_reference_boundary(
            StoreUnderTest::NmitimenRewrite,
            "neser_irq_boundary_nmitimen.sfc",
        );
    }
}
