//! WDC 65C816 CPU core.

use crate::platform::save_state::{SaveStateError, Stateful};
use crate::snes::bus::SnesBus;
use crate::snes::bus::SnesSystemBus;
use crate::snes::console::save_state::{
    SnesBlockMoveDirection, SnesBlockMoveState, SnesCpuState, SnesSaveState, SnesSaveStateError,
};
use crate::snes::cpu::mem_speed::mem_access_cycles;
use crate::trace_cpu;

// Status register P flags (8 bits)
// Bit 7: N (Negative)
// Bit 6: V (Overflow)
// Bit 5: M (Accumulator/Memory width: 1=8-bit, 0=16-bit)
// Bit 4: X (Index register width: 1=8-bit, 0=16-bit)
// Bit 3: D (Decimal mode)
// Bit 2: I (Interrupt disable)
// Bit 1: Z (Zero)
// Bit 0: C (Carry)
const FLAG_CARRY: u8 = 0b0000_0001;
const FLAG_ZERO: u8 = 0b0000_0010;
const FLAG_INTERRUPT: u8 = 0b0000_0100;
const FLAG_DECIMAL: u8 = 0b0000_1000;
const FLAG_INDEX_WIDTH: u8 = 0b0001_0000; // X flag
const FLAG_ACCUM_WIDTH: u8 = 0b0010_0000; // M flag
const FLAG_OVERFLOW: u8 = 0b0100_0000;
const FLAG_NEGATIVE: u8 = 0b1000_0000;

/// Fixed master-clock delay the real 5A22 needs to come out of reset before its first
/// instruction fetch. Mesen models this as a flat 186-clock delay applied right after
/// reset/power-on (`SnesMemoryManager::IncMasterClockStartup`); bsnes reaches an equivalent
/// total via a 22-internal-cycle pre-delay plus its interrupt-dispatch sequence. The two
/// $FFFC/$FFFD vector-byte reads in [`Cpu::do_reset`] already charge 16 clocks (2 SlowROM
/// reads @ 8 clocks each), so only the remaining 170 clocks are ticked separately here to
/// land on the same 186-clock total observed in Mesen.
const RESET_STARTUP_DELAY_CLOCKS: u32 = 170;

#[derive(Clone, Copy)]
enum BlockMoveDirection {
    Increment,
    Decrement,
}

#[derive(Clone, Copy)]
struct BlockMoveState {
    dst_bank: u8,
    src_bank: u8,
    direction: BlockMoveDirection,
}

/// WDC 65C816 CPU
pub struct Cpu<B: SnesBus> {
    /// Accumulator (16-bit: B:A)
    /// When M=1 (8-bit mode), only low byte (A) is used; B is preserved
    a: u16,

    /// X index register (16-bit)
    /// When X=1 (8-bit mode), high byte is forced to 0
    x: u16,

    /// Y index register (16-bit)
    /// When X=1 (8-bit mode), high byte is forced to 0
    y: u16,

    /// Direct page register (16-bit)
    /// Relocates "zero page" to D:$00–D:$FF
    d: u16,

    /// Data bank register (8-bit)
    /// Default bank for data accesses
    dbr: u8,

    /// Program bank register (8-bit)
    /// Bank for current PC (24-bit address = PBR:PC)
    pbr: u8,

    /// Stack pointer (16-bit)
    /// In emulation mode, high byte forced to $01
    s: u16,

    /// Program counter (16-bit offset within PBR)
    pc: u16,

    /// Processor status register (8 bits: N V M X D I Z C)
    p: u8,

    /// Emulation flag (hidden, not in P)
    /// E=1: emulation mode (6502-compatible)
    /// E=0: native mode (full 65816)
    e: bool,

    /// Accumulated extra cycles for the current instruction (DP/M/X/page-cross penalties).
    /// Reset at the start of each `step()` call.
    extra_cycles: u8,

    /// Whether the most recent abs,X / abs,Y / (dp),Y address calculation crossed a page.
    /// Used by read instructions to conditionally add a cycle.
    last_page_crossed: bool,

    /// Pending hardware interrupts — set by external hardware (console / PPU / etc.)
    nmi_pending: bool,
    irq_pending: bool,
    abort_pending: bool,

    /// One-CPU-cycle NMI edge-to-dispatch latch (mirrors Mesen2's `NmiFlagCounter`).
    /// 0 = idle. [`Self::poll_and_arm_nmi_edge`] arms this to 1 on a freshly
    /// polled bus edge; the *next* cycle's [`Self::resolve_nmi_arm_counter`]
    /// decrements it to 0 and sets `nmi_pending`, modeling the real one-cycle
    /// latency between the NMI line rising and the CPU's edge detector
    /// recognizing it (#3049).
    nmi_arm_counter: u8,

    /// Most recently sampled H/V-IRQ line level (mirrors Mesen2's
    /// `PrevIrqSource`). Unlike NMI's edge, the line is non-consuming and
    /// live, so no arm/latch counter is needed -- this is simply resampled
    /// from `bus.poll_irq()` once per CPU cycle, at the end of
    /// [`Self::tick_read`]/[`Self::tick_write`]/[`Self::tick_internal_cycle`]
    /// (mirroring Mesen2's `DetectNmiSignalEdge`, called once per
    /// `ProcessCpuCycle`), instead of once per `step()` call (#3049). See
    /// [`Self::resample_irq_line`] for why end-of-cycle placement matters.
    irq_line_shadow: bool,

    /// True while the CPU is halted by a WAI instruction, waiting for a
    /// hardware interrupt (NMI, IRQ, or ABORT) to be asserted.
    waiting: bool,

    /// FastROM flag: mirrors MEMSEL $420D bit 0.
    /// When true, WS2 ROM regions ($80–$BF:$8000–$FFFF, $C0–$FF) run at 6 master clocks.
    fast_rom: bool,

    /// Count of memory bus accesses (tick_read/tick_write calls) in the current step.
    /// Reset at the start of each step() call; used to compute internal-cycle tick counts.
    memory_bus_cycles: u8,
    /// Number of upcoming CPU cycles whose interrupt sample is suppressed
    /// because a GPDMA held the bus. A transfer that runs mid-instruction (after
    /// the current opcode was fetched) halts the CPU for its cycle and one more,
    /// so a line asserted during the DMA is recognized only from the second
    /// cycle after it -- reproducing hardware/Mesen2 (Sour dma_irq_test, #3065),
    /// where e.g. a 2-cycle `INC A` split by a DMA yields recognition one
    /// instruction later while a 3-cycle `LDA #imm16` still recognizes at its
    /// last cycle. A DMA that runs *before* an opcode read leaves that
    /// instruction as the first full one after the transfer (no suppression).
    /// Set at each cycle a GPDMA runs; decremented per cycle. Can be nonzero at
    /// an instruction boundary (a DMA on the previous instruction's last cycle),
    /// so it is part of the save state.
    dma_suppress_cycles: u8,

    /// One-step interrupt lock used to emulate delayed IRQ/NMI recognition after
    /// specific hardware events (e.g. writes to `$4200` and DMA start via `$420B`).
    irq_lock_step: bool,

    /// The I flag value the IRQ recognition logic sees, sampled per the 65816's
    /// pre-effect rule: CLI/SEI/PLP/REP/SEP change I only for the poll AFTER the
    /// following instruction (their flag write lands after the recognition cycle),
    /// while RTI's mid-instruction P pull and the hardware interrupt sequences are
    /// visible immediately (Mesen2 DetectNmiSignalEdge `PrevIrqSource`). Without
    /// this, a pending IRQ dispatches between a CLI;RTI epilogue pair, nesting
    /// frames real hardware never creates (#2985, absindx SA1RamProtectionTest).
    irq_i_shadow: bool,

    /// In-progress MVN/MVP transfer state. When present, each `step()` performs one
    /// transfer unit and keeps architectural PC at the post-operand address.
    block_move_state: Option<BlockMoveState>,

    /// Bus for memory access
    bus: B,
}

impl<B: SnesBus> Cpu<B> {
    /// Create a new 65816 CPU in reset state (emulation mode).
    pub fn new(bus: B) -> Self {
        Self {
            a: 0,
            x: 0,
            y: 0,
            d: 0,
            dbr: 0,
            pbr: 0,
            s: 0x01FF, // Emulation mode starts with S at top of page 1
            pc: 0,
            p: FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH | FLAG_INTERRUPT, // M=1, X=1, I=1
            e: true,                                                 // Start in emulation mode
            extra_cycles: 0,
            dma_suppress_cycles: 0,
            last_page_crossed: false,
            nmi_pending: false,
            irq_pending: false,
            abort_pending: false,
            nmi_arm_counter: 0,
            irq_line_shadow: false,
            waiting: false,
            fast_rom: false,
            memory_bus_cycles: 0,
            irq_lock_step: false,
            irq_i_shadow: true,
            block_move_state: None,
            bus,
        }
    }

    /// Read accumulator value (respects M flag width).
    /// Returns 16-bit value; in 8-bit mode (M=1), high byte is B (preserved).
    pub fn read_a(&self) -> u16 {
        self.a
    }

    /// Write accumulator value (respects M flag width).
    /// In 8-bit mode (M=1), only low byte is updated; B (high byte) preserved.
    pub fn write_a(&mut self, value: u16) {
        if self.m_flag() {
            // 8-bit mode: update low byte only, preserve B
            self.a = (self.a & 0xFF00) | (value & 0x00FF);
        } else {
            // 16-bit mode: update full 16 bits
            self.a = value;
        }
    }

    /// Read X register value (respects X flag width).
    pub fn read_x(&self) -> u16 {
        self.x
    }

    /// Write X register value (respects X flag width).
    /// In 8-bit mode (X=1), high byte forced to 0.
    pub fn write_x(&mut self, value: u16) {
        if self.x_flag() {
            // 8-bit mode: force high byte to 0
            self.x = value & 0x00FF;
        } else {
            // 16-bit mode: full 16 bits
            self.x = value;
        }
    }

    /// Read Y register value (respects X flag width).
    pub fn read_y(&self) -> u16 {
        self.y
    }

    /// Write Y register value (respects X flag width).
    /// In 8-bit mode (X=1), high byte forced to 0.
    pub fn write_y(&mut self, value: u16) {
        if self.x_flag() {
            // 8-bit mode: force high byte to 0
            self.y = value & 0x00FF;
        } else {
            // 16-bit mode: full 16 bits
            self.y = value;
        }
    }

    /// Read direct page register.
    pub fn read_d(&self) -> u16 {
        self.d
    }

    /// Write direct page register.
    pub fn write_d(&mut self, value: u16) {
        self.d = value;
    }

    /// Read data bank register.
    pub fn read_dbr(&self) -> u8 {
        self.dbr
    }

    /// Write data bank register.
    pub fn write_dbr(&mut self, value: u8) {
        self.dbr = value;
    }

    /// Read program bank register.
    pub fn read_pbr(&self) -> u8 {
        self.pbr
    }

    /// Write program bank register.
    pub fn write_pbr(&mut self, value: u8) {
        self.pbr = value;
    }

    /// Read stack pointer.
    pub fn read_s(&self) -> u16 {
        self.s
    }

    /// Write stack pointer.
    /// In emulation mode, high byte forced to $01.
    pub fn write_s(&mut self, value: u16) {
        if self.e {
            // Emulation mode: force high byte to $01
            self.s = 0x0100 | (value & 0x00FF);
        } else {
            // Native mode: full 16 bits
            self.s = value;
        }
    }

    /// Read program counter.
    pub fn read_pc(&self) -> u16 {
        self.pc
    }

    /// Write program counter.
    pub fn write_pc(&mut self, value: u16) {
        self.pc = value;
    }

    /// Read processor status register.
    pub fn read_p(&self) -> u8 {
        self.p
    }

    pub(crate) fn bus(&self) -> &B {
        &self.bus
    }

    pub(crate) fn bus_mut(&mut self) -> &mut B {
        &mut self.bus
    }

    pub(crate) fn capture_state_inner(&self) -> SnesCpuState {
        SnesCpuState {
            a: self.a,
            x: self.x,
            y: self.y,
            d: self.d,
            dbr: self.dbr,
            pbr: self.pbr,
            s: self.s,
            pc: self.pc,
            p: self.p,
            e: self.e,
            extra_cycles: self.extra_cycles,
            last_page_crossed: self.last_page_crossed,
            nmi_pending: self.nmi_pending,
            irq_pending: self.irq_pending,
            abort_pending: self.abort_pending,
            nmi_arm_counter: self.nmi_arm_counter,
            irq_line_shadow: self.irq_line_shadow,
            dma_suppress_cycles: self.dma_suppress_cycles,
            waiting: self.waiting,
            fast_rom: self.fast_rom,
            memory_bus_cycles: self.memory_bus_cycles,
            irq_lock_step: self.irq_lock_step,
            irq_i_shadow: self.irq_i_shadow,
            block_move_state: self.block_move_state.map(|state| SnesBlockMoveState {
                dst_bank: state.dst_bank,
                src_bank: state.src_bank,
                direction: match state.direction {
                    BlockMoveDirection::Increment => SnesBlockMoveDirection::Increment,
                    BlockMoveDirection::Decrement => SnesBlockMoveDirection::Decrement,
                },
            }),
        }
    }

    pub(crate) fn restore_state_inner(&mut self, state: &SnesCpuState) {
        self.a = state.a;
        self.x = state.x;
        self.y = state.y;
        self.d = state.d;
        self.dbr = state.dbr;
        self.pbr = state.pbr;
        self.s = state.s;
        self.pc = state.pc;
        self.p = state.p;
        self.e = state.e;
        self.extra_cycles = state.extra_cycles;
        self.last_page_crossed = state.last_page_crossed;
        self.nmi_pending = state.nmi_pending;
        self.irq_pending = state.irq_pending;
        self.abort_pending = state.abort_pending;
        self.nmi_arm_counter = state.nmi_arm_counter;
        self.irq_line_shadow = state.irq_line_shadow;
        self.dma_suppress_cycles = state.dma_suppress_cycles;
        self.waiting = state.waiting;
        self.fast_rom = state.fast_rom;
        self.memory_bus_cycles = state.memory_bus_cycles;
        self.irq_lock_step = state.irq_lock_step;
        self.irq_i_shadow = state.irq_i_shadow;
        self.block_move_state = state.block_move_state.map(|state| BlockMoveState {
            dst_bank: state.dst_bank,
            src_bank: state.src_bank,
            direction: match state.direction {
                SnesBlockMoveDirection::Increment => BlockMoveDirection::Increment,
                SnesBlockMoveDirection::Decrement => BlockMoveDirection::Decrement,
            },
        });

        if self.e {
            self.p |= FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
            self.s = 0x0100 | (self.s & 0x00FF);
        }

        if self.x_flag() {
            self.x &= 0x00FF;
            self.y &= 0x00FF;
        }
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn load_state_for_processor_test(
        &mut self,
        a: u16,
        x: u16,
        y: u16,
        d: u16,
        dbr: u8,
        pbr: u8,
        s: u16,
        pc: u16,
        p: u8,
        e: bool,
    ) {
        self.a = a;
        self.x = x;
        self.y = y;
        self.d = d;
        self.dbr = dbr;
        self.pbr = pbr;
        self.s = s;
        self.pc = pc;
        self.p = p;
        self.e = e;

        if self.e {
            self.p |= FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
            self.s = 0x0100 | (self.s & 0x00FF);
        }

        if self.x_flag() {
            self.x &= 0x00FF;
            self.y &= 0x00FF;
        }
    }

    /// Check if in emulation mode.
    pub fn emulation_mode(&self) -> bool {
        self.e
    }

    /// Assert or deassert the NMI line (edge-triggered; pending is cleared after dispatch).
    pub fn set_nmi(&mut self, pending: bool) {
        self.nmi_pending = pending;
    }

    /// Assert or deassert the IRQ line (level-triggered; stays asserted until caller clears it).
    pub fn set_irq(&mut self, pending: bool) {
        self.irq_pending = pending;
    }

    /// Assert or deassert the ABORT line (edge-triggered; pending is cleared after dispatch).
    pub fn set_abort(&mut self, pending: bool) {
        self.abort_pending = pending;
    }

    /// Force the Fast/Slow memory-access speed classification (mirrors MEMSEL `$420D` bit 0).
    ///
    /// Used by the SA-1 core, which has no MEMSEL of its own but always accesses memory at
    /// the uniformly-fast rate (see fullsnes: SA-1 CPU at 10.74MHz vs the main CPU's
    /// 2.68/3.58MHz), so its `Cpu` instance is forced into the "Fast" classification once at
    /// construction rather than toggled at runtime like the main CPU's.
    pub fn set_fast_rom(&mut self, value: bool) {
        self.fast_rom = value;
    }

    /// Perform a hardware RESET.
    ///
    /// No bytes are pushed. The CPU enters emulation mode, sets I=1, clears D, PBR, DBR,
    /// forces S to $01FF, clears pending interrupt latches, and loads PC from $FFFC/$FFFD.
    /// Before fetching the vector, the bus is ticked [`RESET_STARTUP_DELAY_CLOCKS`] times to
    /// model the fixed delay the real 5A22 takes to come out of reset (see the constant's doc
    /// comment for the derivation against Mesen/bsnes ground truth).
    pub fn do_reset(&mut self) {
        self.e = true;
        self.p |= FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH | FLAG_INTERRUPT;
        self.p &= !FLAG_DECIMAL;
        self.pbr = 0x00;
        self.dbr = 0x00;
        self.s = 0x01FF;
        self.nmi_pending = false;
        self.irq_pending = false;
        self.abort_pending = false;
        self.nmi_arm_counter = 0;
        self.dma_suppress_cycles = 0;
        self.irq_line_shadow = false;
        self.irq_lock_step = false;
        self.irq_i_shadow = true;
        for _ in 0..RESET_STARTUP_DELAY_CLOCKS {
            self.bus.tick();
        }
        let lo = self.read8(0x00FFFC) as u16;
        let hi = self.read8(0x00FFFD) as u16;
        self.pc = lo | hi << 8;
    }

    /// Get M flag (accumulator/memory width: 1=8-bit, 0=16-bit).
    pub fn m_flag(&self) -> bool {
        self.p & FLAG_ACCUM_WIDTH != 0
    }

    /// Get X flag (index width: 1=8-bit, 0=16-bit).
    pub fn x_flag(&self) -> bool {
        self.p & FLAG_INDEX_WIDTH != 0
    }

    /// Get carry flag.
    pub fn flag_c(&self) -> bool {
        self.p & FLAG_CARRY != 0
    }

    /// Set carry flag.
    pub fn set_flag_c(&mut self, value: bool) {
        if value {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }
    }

    /// Get zero flag.
    pub fn flag_z(&self) -> bool {
        self.p & FLAG_ZERO != 0
    }

    /// Set zero flag.
    pub fn set_flag_z(&mut self, value: bool) {
        if value {
            self.p |= FLAG_ZERO;
        } else {
            self.p &= !FLAG_ZERO;
        }
    }

    /// Get interrupt disable flag.
    pub fn flag_i(&self) -> bool {
        self.p & FLAG_INTERRUPT != 0
    }

    /// Set interrupt disable flag.
    pub fn set_flag_i(&mut self, value: bool) {
        // Out-of-band pokes (tests, interrupt sequences) are recognition-visible
        // immediately; for CLI/SEI/PLP/REP/SEP the post-instruction shadow update
        // in step() overrides this with the pre-effect value afterwards.
        self.irq_i_shadow = value;
        if value {
            self.p |= FLAG_INTERRUPT;
        } else {
            self.p &= !FLAG_INTERRUPT;
        }
    }

    /// Get decimal mode flag.
    pub fn flag_d(&self) -> bool {
        self.p & FLAG_DECIMAL != 0
    }

    /// Set decimal mode flag.
    pub fn set_flag_d(&mut self, value: bool) {
        if value {
            self.p |= FLAG_DECIMAL;
        } else {
            self.p &= !FLAG_DECIMAL;
        }
    }

    /// Get overflow flag.
    pub fn flag_v(&self) -> bool {
        self.p & FLAG_OVERFLOW != 0
    }

    /// Set overflow flag.
    pub fn set_flag_v(&mut self, value: bool) {
        if value {
            self.p |= FLAG_OVERFLOW;
        } else {
            self.p &= !FLAG_OVERFLOW;
        }
    }

    /// Get negative flag.
    pub fn flag_n(&self) -> bool {
        self.p & FLAG_NEGATIVE != 0
    }

    /// Set negative flag.
    pub fn set_flag_n(&mut self, value: bool) {
        if value {
            self.p |= FLAG_NEGATIVE;
        } else {
            self.p &= !FLAG_NEGATIVE;
        }
    }

    /// XCE - Exchange Carry with Emulation flag.
    /// Swaps the C flag (bit 0 of P) with the hidden E flag.
    /// When E transitions:
    /// - E 0→1 (native→emulation): force M=1, X=1, S high byte→$01
    /// - E 1→0 (emulation→native): M/X remain 1 until cleared by REP
    pub fn xce(&mut self) {
        let old_c = self.flag_c();
        let old_e = self.e;

        // Swap C and E
        self.set_flag_c(old_e);
        self.e = old_c;

        // Enforce mode constraints when entering emulation mode
        if !old_e && self.e {
            // Entering emulation mode (E 0→1)
            self.p |= FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH; // Force M=1, X=1
            self.s = 0x0100 | (self.s & 0x00FF); // Force S high byte to $01
            self.x &= 0x00FF; // X/Y become 8-bit in emulation mode
            self.y &= 0x00FF;
        }
        // Note: When leaving emulation mode (E 1→0), M/X remain 1 until REP clears them
    }

    /// REP - Reset Processor Status Bits.
    /// Clears bits in P specified by the immediate byte.
    /// In emulation mode, M and X flags cannot be cleared (remain 1).
    pub fn rep(&mut self, mask: u8) {
        if self.e {
            // Emulation mode: cannot clear M or X
            let protected_mask = mask & !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);
            self.p &= !protected_mask;
        } else {
            // Native mode: can clear any bits including M and X
            self.p &= !mask;

            // When X flag transitions from 1→0 (8→16 bit), high bytes of X/Y start at 0
            // (they were already 0 due to write_x/write_y forcing)
            // No action needed here as write_x/y already enforce this
        }
    }

    /// SEP - Set Processor Status Bits.
    /// Sets bits in P specified by the immediate byte.
    pub fn sep(&mut self, mask: u8) {
        let old_x = self.x_flag();

        self.p |= mask;

        // Handle width transitions
        // When X flag transitions from 0→1 (16→8 bit), force high bytes of X/Y to 0
        if !old_x && self.x_flag() {
            self.x &= 0x00FF;
            self.y &= 0x00FF;
        }
        // When M flag transitions from 0→1 (16→8 bit), B (high byte of A) is preserved
        // No action needed - read_a/write_a already handle this

        // Note: M 1→0 transition handled naturally by read_a/write_a
    }

    /// Execute one instruction: fetch opcode at PBR:PC, advance PC, dispatch.
    ///
    /// Before fetching the opcode, hardware interrupts are polled in priority order:
    /// ABORT > NMI > IRQ (masked by I flag).  Returns the number of bus cycles consumed.
    ///
    /// This method also drives `SnesBus::tick()` for every master clock cycle:
    /// - Memory accesses tick at the speed of the target address (6/8/12 master clocks).
    /// - Internal (non-memory) cycles always tick at 6 master clocks.
    pub fn step(&mut self) -> u8 {
        self.extra_cycles = 0;
        self.last_page_crossed = false;
        self.memory_bus_cycles = 0;
        let interrupts_locked = self.irq_lock_step;
        self.irq_lock_step = false;
        let mut wai_wake_cycles: u8 = 0;

        if let Some(state) = self.block_move_state {
            return self.step_block_move_unit(state);
        }

        // `nmi_pending` is kept continuously up to date by
        // `resolve_nmi_arm_counter`/`poll_and_arm_nmi_edge`, and
        // `irq_line_shadow` by `resample_irq_line` -- both called once per
        // CPU cycle from `tick_read`/`tick_write`/`tick_internal_cycle`,
        // mirroring Mesen2's `DetectNmiSignalEdge`/`PrevIrqSource`, so by the
        // time this `step()` call starts they already reflect state as of
        // the end of the previous cycle -- proven via a bus-trace diff
        // against Mesen2 on the KungFuFurby nmi.smc ROM (#2883/#3049): a
        // freshly polled edge/level now resolves in time to dispatch right
        // after the instruction it occurred in, not an entire extra
        // instruction later. No separate top-of-step() poll is needed.
        let irq_line_asserted = self.irq_line_shadow || self.irq_pending;

        // WAI: while halted, the CPU idles (advancing the master clock) until any
        // hardware interrupt (NMI, IRQ, or ABORT) is asserted — regardless of the
        // I flag. Once an interrupt is pending we clear the wait state and fall
        // through to the normal dispatch logic below (which services the interrupt
        // if unmasked, or simply resumes the next instruction if the I flag masks it).
        if self.waiting {
            if !interrupts_locked && (self.nmi_pending || irq_line_asserted || self.abort_pending) {
                // Hardware spends TWO idle cycles between the wait-loop poll
                // that first sees the interrupt and resuming execution: the
                // detecting idle completes, and one more idle runs before the
                // core leaves the halted state (Mesen ProcessHaltedState
                // samples `_waiOver` before each Idle). Verified against
                // Mesen with the #2914 boot bus trace.
                self.waiting = false;
                self.tick_internal_cycle();
                self.tick_internal_cycle();
                wai_wake_cycles = 2;
            } else {
                self.tick_internal_cycle();
                return 1;
            }
        }

        // Poll hardware interrupts (higher priority than opcode fetch)
        if !interrupts_locked && self.abort_pending {
            self.abort_pending = false;
            let base = self.dispatch_abort();
            return self
                .tick_internal_cycles_for(base)
                .saturating_add(wai_wake_cycles);
        }
        if !interrupts_locked && self.nmi_pending {
            self.nmi_pending = false;
            let base = self.dispatch_nmi();
            return self
                .tick_internal_cycles_for(base)
                .saturating_add(wai_wake_cycles);
        }
        if !interrupts_locked && irq_line_asserted && !self.irq_i_shadow {
            // IRQ is level-triggered: do NOT clear irq_pending here; caller must deassert
            let base = self.dispatch_irq();
            return self
                .tick_internal_cycles_for(base)
                .saturating_add(wai_wake_cycles);
        }

        let pc_before = ((self.pbr as u32) << 16) | self.pc as u32;
        let i_before = self.flag_i();
        let opcode = self.fetch_opcode();
        if crate::platform::debugging::cpu_trace_level() >= 1 {
            let operands = self.exec_trace_operands(opcode);
            trace_cpu!(1; "{}", self.format_exec_trace_line(pc_before, &operands));
        }
        let base = match opcode {
            0x00 => self.op_brk(),
            0x01 => self.op_ora_dp_x_ind(),
            0x02 => self.op_cop(),
            0x03 => self.op_ora_sr(),
            0x05 => self.op_ora_dp(),
            0x04 => self.op_tsb_dp(),
            0x06 => self.op_asl_dp(),
            0x08 => self.op_php(),
            0x0B => self.op_phd(),
            0x0C => self.op_tsb_abs(),
            0x14 => self.op_trb_dp(),
            0x1C => self.op_trb_abs(),
            0x20 => self.op_jsr_abs(),
            0x07 => self.op_ora_dp_ind_long(),
            0x09 => self.op_ora_imm(),
            0x0A => self.op_asl_acc(),
            0x0D => self.op_ora_abs(),
            0x0E => self.op_asl_abs(),
            0x0F => self.op_ora_abs_long(),
            0x10 => self.op_bpl(),
            0x11 => self.op_ora_dp_ind_y(),
            0x12 => self.op_ora_dp_ind(),
            0x13 => self.op_ora_sr_ind_y(),
            0x15 => self.op_ora_dp_x(),
            0x16 => self.op_asl_dp_x(),
            0x17 => self.op_ora_dp_ind_long_y(),
            0x18 => self.op_clc(),
            0x19 => self.op_ora_abs_y(),
            0x1A => self.op_inc_acc(),
            0x1B => self.op_tcs(),
            0x1D => self.op_ora_abs_x(),
            0x1E => self.op_asl_abs_x(),
            0x1F => self.op_ora_abs_long_x(),
            0x21 => self.op_and_dp_x_ind(),
            0x22 => self.op_jsl_abs_long(),
            0x23 => self.op_and_sr(),
            0x24 => self.op_bit_dp(),
            0x25 => self.op_and_dp(),
            0x26 => self.op_rol_dp(),
            0x27 => self.op_and_dp_ind_long(),
            0x28 => self.op_plp(),
            0x29 => self.op_and_imm(),
            0x2A => self.op_rol_acc(),
            0x2B => self.op_pld(),
            0x2C => self.op_bit_abs(),
            0x2D => self.op_and_abs(),
            0x2E => self.op_rol_abs(),
            0x2F => self.op_and_abs_long(),
            0x30 => self.op_bmi(),
            0x31 => self.op_and_dp_ind_y(),
            0x32 => self.op_and_dp_ind(),
            0x33 => self.op_and_sr_ind_y(),
            0x34 => self.op_bit_dp_x(),
            0x35 => self.op_and_dp_x(),
            0x36 => self.op_rol_dp_x(),
            0x37 => self.op_and_dp_ind_long_y(),
            0x38 => self.op_sec(),
            0x39 => self.op_and_abs_y(),
            0x3A => self.op_dec_acc(),
            0x3B => self.op_tsc(),
            0x3C => self.op_bit_abs_x(),
            0x3D => self.op_and_abs_x(),
            0x3E => self.op_rol_abs_x(),
            0x3F => self.op_and_abs_long_x(),
            0x40 => self.op_rti(),
            0x41 => self.op_eor_dp_x_ind(),
            0x42 => self.op_wdm(),
            0x43 => self.op_eor_sr(),
            0x44 => self.op_mvp(),
            0x45 => self.op_eor_dp(),
            0x46 => self.op_lsr_dp(),
            0x47 => self.op_eor_dp_ind_long(),
            0x48 => self.op_pha(),
            0x49 => self.op_eor_imm(),
            0x4A => self.op_lsr_acc(),
            0x4B => self.op_phk(),
            0x4C => self.op_jmp_abs(),
            0x4D => self.op_eor_abs(),
            0x4E => self.op_lsr_abs(),
            0x4F => self.op_eor_abs_long(),
            0x50 => self.op_bvc(),
            0x51 => self.op_eor_dp_ind_y(),
            0x52 => self.op_eor_dp_ind(),
            0x53 => self.op_eor_sr_ind_y(),
            0x55 => self.op_eor_dp_x(),
            0x56 => self.op_lsr_dp_x(),
            0x57 => self.op_eor_dp_ind_long_y(),
            0x58 => self.op_cli(),
            0x59 => self.op_eor_abs_y(),
            0x5A => self.op_phy(),
            0x5B => self.op_tcd(),
            0x54 => self.op_mvn(),
            0x5C => self.op_jmp_abs_long(),
            0x5D => self.op_eor_abs_x(),
            0x5E => self.op_lsr_abs_x(),
            0x5F => self.op_eor_abs_long_x(),
            0x60 => self.op_rts(),
            0x61 => self.op_adc_dp_x_ind(),
            0x62 => self.op_per(),
            0x63 => self.op_adc_sr(),
            0x64 => self.op_stz_dp(),
            0x65 => self.op_adc_dp(),
            0x66 => self.op_ror_dp(),
            0x67 => self.op_adc_dp_ind_long(),
            0x68 => self.op_pla(),
            0x69 => self.op_adc_imm(),
            0x6A => self.op_ror_acc(),
            0x6B => self.op_rtl(),
            0x6C => self.op_jmp_abs_ind(),
            0x6D => self.op_adc_abs(),
            0x6E => self.op_ror_abs(),
            0x6F => self.op_adc_abs_long(),
            0x70 => self.op_bvs(),
            0x71 => self.op_adc_dp_ind_y(),
            0x72 => self.op_adc_dp_ind(),
            0x73 => self.op_adc_sr_ind_y(),
            0x74 => self.op_stz_dp_x(),
            0x75 => self.op_adc_dp_x(),
            0x76 => self.op_ror_dp_x(),
            0x77 => self.op_adc_dp_ind_long_y(),
            0x78 => self.op_sei(),
            0x79 => self.op_adc_abs_y(),
            0x7A => self.op_ply(),
            0x7B => self.op_tdc(),
            0x7C => self.op_jmp_abs_x_ind(),
            0x7D => self.op_adc_abs_x(),
            0x7E => self.op_ror_abs_x(),
            0x7F => self.op_adc_abs_long_x(),
            0x80 => self.op_bra(),
            0x81 => self.op_sta_dp_x_ind(),
            0x82 => self.op_brl(),
            0x83 => self.op_sta_sr(),
            0x84 => self.op_sty_dp(),
            0x85 => self.op_sta_dp(),
            0x86 => self.op_stx_dp(),
            0x87 => self.op_sta_dp_ind_long(),
            0x88 => self.op_dey(),
            0x89 => self.op_bit_imm(),
            0x8A => self.op_txa(),
            0x8B => self.op_phb(),
            0x8C => self.op_sty_abs(),
            0x8D => self.op_sta_abs(),
            0x8E => self.op_stx_abs(),
            0x8F => self.op_sta_abs_long(),
            0x90 => self.op_bcc(),
            0x91 => self.op_sta_dp_ind_y(),
            0x92 => self.op_sta_dp_ind(),
            0x93 => self.op_sta_sr_ind_y(),
            0x94 => self.op_sty_dp_x(),
            0x95 => self.op_sta_dp_x(),
            0x96 => self.op_stx_dp_y(),
            0x97 => self.op_sta_dp_ind_long_y(),
            0x98 => self.op_tya(),
            0x99 => self.op_sta_abs_y(),
            0x9A => self.op_txs(),
            0x9B => self.op_txy(),
            0x9C => self.op_stz_abs(),
            0x9D => self.op_sta_abs_x(),
            0x9E => self.op_stz_abs_x(),
            0x9F => self.op_sta_abs_long_x(),
            0xA0 => self.op_ldy_imm(),
            0xA1 => self.op_lda_dp_x_ind(),
            0xA2 => self.op_ldx_imm(),
            0xA3 => self.op_lda_sr(),
            0xA4 => self.op_ldy_dp(),
            0xA5 => self.op_lda_dp(),
            0xA6 => self.op_ldx_dp(),
            0xA7 => self.op_lda_dp_ind_long(),
            0xA8 => self.op_tay(),
            0xA9 => self.op_lda_imm(),
            0xAA => self.op_tax(),
            0xAB => self.op_plb(),
            0xAC => self.op_ldy_abs(),
            0xAD => self.op_lda_abs(),
            0xAE => self.op_ldx_abs(),
            0xAF => self.op_lda_abs_long(),
            0xB0 => self.op_bcs(),
            0xB1 => self.op_lda_dp_ind_y(),
            0xB2 => self.op_lda_dp_ind(),
            0xB3 => self.op_lda_sr_ind_y(),
            0xB4 => self.op_ldy_dp_x(),
            0xB5 => self.op_lda_dp_x(),
            0xB6 => self.op_ldx_dp_y(),
            0xB7 => self.op_lda_dp_ind_long_y(),
            0xB8 => self.op_clv(),
            0xB9 => self.op_lda_abs_y(),
            0xBA => self.op_tsx(),
            0xBB => self.op_tyx(),
            0xBC => self.op_ldy_abs_x(),
            0xBD => self.op_lda_abs_x(),
            0xBE => self.op_ldx_abs_y(),
            0xBF => self.op_lda_abs_long_x(),
            0xC0 => self.op_cpy_imm(),
            0xC1 => self.op_cmp_dp_x_ind(),
            0xC2 => self.op_rep(),
            0xC3 => self.op_cmp_sr(),
            0xC4 => self.op_cpy_dp(),
            0xC5 => self.op_cmp_dp(),
            0xC6 => self.op_dec_dp(),
            0xC7 => self.op_cmp_dp_ind_long(),
            0xC8 => self.op_iny(),
            0xC9 => self.op_cmp_imm(),
            0xCA => self.op_dex(),
            0xCB => self.op_wai(),
            0xCC => self.op_cpy_abs(),
            0xCD => self.op_cmp_abs(),
            0xCE => self.op_dec_abs(),
            0xCF => self.op_cmp_abs_long(),
            0xD0 => self.op_bne(),
            0xD1 => self.op_cmp_dp_ind_y(),
            0xD2 => self.op_cmp_dp_ind(),
            0xD3 => self.op_cmp_sr_ind_y(),
            0xD4 => self.op_pei(),
            0xD5 => self.op_cmp_dp_x(),
            0xD6 => self.op_dec_dp_x(),
            0xD7 => self.op_cmp_dp_ind_long_y(),
            0xD8 => self.op_cld(),
            0xD9 => self.op_cmp_abs_y(),
            0xDA => self.op_phx(),
            0xDB => self.op_stp(),
            0xDC => self.op_jmp_abs_ind_long(),
            0xDD => self.op_cmp_abs_x(),
            0xDE => self.op_dec_abs_x(),
            0xDF => self.op_cmp_abs_long_x(),
            0xE0 => self.op_cpx_imm(),
            0xE1 => self.op_sbc_dp_x_ind(),
            0xE2 => self.op_sep(),
            0xE3 => self.op_sbc_sr(),
            0xE4 => self.op_cpx_dp(),
            0xE5 => self.op_sbc_dp(),
            0xE6 => self.op_inc_dp(),
            0xE7 => self.op_sbc_dp_ind_long(),
            0xE8 => self.op_inx(),
            0xE9 => self.op_sbc_imm(),
            0xEA => self.op_nop(),
            0xEB => self.op_xba(),
            0xEC => self.op_cpx_abs(),
            0xED => self.op_sbc_abs(),
            0xEE => self.op_inc_abs(),
            0xEF => self.op_sbc_abs_long(),
            0xF0 => self.op_beq(),
            0xF1 => self.op_sbc_dp_ind_y(),
            0xF2 => self.op_sbc_dp_ind(),
            0xF3 => self.op_sbc_sr_ind_y(),
            0xF4 => self.op_pea(),
            0xF5 => self.op_sbc_dp_x(),
            0xF6 => self.op_inc_dp_x(),
            0xF7 => self.op_sbc_dp_ind_long_y(),
            0xF8 => self.op_sed(),
            0xF9 => self.op_sbc_abs_y(),
            0xFA => self.op_plx(),
            0xFB => self.op_xce(),
            0xFC => self.op_jsr_abs_x_ind(),
            0xFD => self.op_sbc_abs_x(),
            0xFE => self.op_inc_abs_x(),
            0xFF => self.op_sbc_abs_long_x(),
        };

        // 65816 IRQ recognition pre-effect rule: CLI/SEI/PLP/REP/SEP flag writes
        // land after the recognition poll, so their I change is dispatch-visible
        // only after the NEXT instruction; everything else (incl. RTI's restored
        // P and BRK/COP's I set) is visible immediately.
        self.irq_i_shadow = match opcode {
            0x58 | 0x78 | 0x28 | 0xC2 | 0xE2 => i_before,
            _ => self.flag_i(),
        };
        let total_bus_cycles = base + self.extra_cycles;

        // Tick bus for internal (non-memory-access) cycles.
        self.tick_internal_cycles_for(total_bus_cycles)
            .saturating_add(wai_wake_cycles)
    }

    /// Tick the bus for the CPU-internal (non-memory-access) cycles of an
    /// operation whose total length is `total_bus_cycles`.
    ///
    /// Memory accesses already tick the bus at their region speed and increment
    /// `memory_bus_cycles`; the remaining cycles are CPU-internal and each
    /// consume 6 master clocks. Returns `total_bus_cycles` for convenience so
    /// callers can `return self.tick_internal_cycles_for(n);`.
    ///
    /// This applies uniformly to normal opcodes and to hardware interrupt
    /// dispatch (IRQ/NMI/ABORT), whose 8/7-cycle sequences include two internal
    /// cycles that must advance the PPU/APU alongside the stack pushes and
    /// vector reads.
    fn tick_internal_cycles_for(&mut self, total_bus_cycles: u8) -> u8 {
        let internal_cycles = total_bus_cycles.saturating_sub(self.memory_bus_cycles);
        for _ in 0..internal_cycles {
            self.tick_internal_cycle();
        }
        total_bus_cycles
    }

    fn exec_trace_operands(&self, opcode: u8) -> [u8; 4] {
        let bank = (self.pbr as u32) << 16;
        [
            opcode,
            self.bus.read_for_debugger(bank | u32::from(self.pc)),
            self.bus
                .read_for_debugger(bank | u32::from(self.pc.wrapping_add(1))),
            self.bus
                .read_for_debugger(bank | u32::from(self.pc.wrapping_add(2))),
        ]
    }

    /// Fetch the byte at PBR:PC and advance PC by 1.
    pub fn fetch_byte(&mut self) -> u8 {
        let addr = (self.pbr as u32) << 16 | self.pc as u32;
        let byte = self.tick_read(addr);
        self.pc = self.pc.wrapping_add(1);
        byte
    }

    /// Fetches the next instruction's opcode byte. Identical to
    /// [`Self::fetch_byte`] except the read is flagged as an opcode fetch so
    /// interrupt sampling resumes correctly after a GPDMA (#3065).
    fn fetch_opcode(&mut self) -> u8 {
        let addr = (self.pbr as u32) << 16 | self.pc as u32;
        let byte = self.tick_read_opcode(addr);
        self.pc = self.pc.wrapping_add(1);
        byte
    }

    /// Run one read bus cycle for `addr` and return the byte.
    ///
    /// The 65816 samples the data bus 4 master clocks before the end of the
    /// bus cycle (writes drive it until the very end, see [`Self::tick_write`];
    /// Mesen: `_execRead` runs `speed - 4` clocks, the handler read happens,
    /// then IncMasterClock4 finishes the cycle). The split matters for reads
    /// that race asynchronous hardware: APU port polls at $2140-$2143 (#2914)
    /// and the $2137 H/V counter latch both observe the sampling instant.
    fn tick_read(&mut self, addr: u32) -> u8 {
        self.tick_read_inner(addr, false)
    }

    /// A read whose byte is the next instruction's opcode. Interrupt sampling
    /// resumes here (a new instruction begins) unless a GPDMA runs during this
    /// cycle *after* the opcode read, which splits the new instruction (#3065).
    fn tick_read_opcode(&mut self, addr: u32) -> u8 {
        self.tick_read_inner(addr, true)
    }

    fn tick_read_inner(&mut self, addr: u32, is_opcode: bool) -> u8 {
        self.resolve_nmi_arm_counter();
        self.bus.gpdma_cycle_hook();
        let cycles = mem_access_cycles(addr, self.fast_rom);
        trace_cpu!(2; "      read  ${:06X}", addr);
        for _ in 0..cycles - 4 {
            self.bus.tick();
        }
        // Did a GPDMA run before this opcode is read? Then the transfer precedes
        // the new instruction, which becomes the first full instruction after
        // the DMA and samples normally (#3065).
        let dma_before_read = is_opcode && self.bus.peek_dma_ran_this_cpu_cycle();
        self.memory_bus_cycles += 1;
        let value = self.bus.read(addr);
        for _ in 0..4 {
            self.bus.tick();
        }
        if is_opcode {
            // A GPDMA that ran *before* the opcode read leaves this new
            // instruction as the first full one after the transfer (samples
            // normally); one that ran *after* the read splits it (opens a fresh
            // suppression window).
            let dma_ran = self.bus.take_dma_ran_this_cpu_cycle();
            self.sample_interrupts_end_of_cycle(dma_ran && !dma_before_read);
        } else {
            self.end_of_cycle_interrupt_poll();
        }
        value
    }

    /// Advance the master clock N cycles for `addr`, then write one byte.
    /// Also intercepts MEMSEL ($420D) writes to update the fast_rom flag.
    fn tick_write(&mut self, addr: u32, value: u8) {
        self.resolve_nmi_arm_counter();
        self.bus.gpdma_cycle_hook();
        let cycles = mem_access_cycles(addr, self.fast_rom);
        trace_cpu!(2; "      write ${:06X} = ${:02X}", addr, value);
        for _ in 0..cycles {
            self.bus.tick();
        }
        self.memory_bus_cycles += 1;
        let bank = (addr >> 16) as u8;
        if bank <= 0x3F || (0x80..=0xBF).contains(&bank) {
            match (addr & 0xFFFF) as u16 {
                0x4200 => {
                    // Writes to NMITIMEN introduce a one-step interrupt-lock window.
                    self.irq_lock_step = true;
                }
                0x420B => {
                    // Starting DMA introduces the same lock window before IRQ/NMI recognition.
                    if value != 0 {
                        self.irq_lock_step = true;
                    }
                }
                // MEMSEL $420D: bit 0 controls WS2 ROM speed.
                0x420D => self.fast_rom = value & 0x01 != 0,
                _ => {}
            }
        }
        self.bus.write(addr, value);
        self.end_of_cycle_interrupt_poll();
    }

    /// Fetch a 16-bit little-endian word at PBR:PC and advance PC by 2.
    fn fetch_word(&mut self) -> u16 {
        let lo = self.fetch_byte() as u16;
        let hi = self.fetch_byte() as u16;
        lo | hi << 8
    }

    /// Fetch a 24-bit little-endian address at PBR:PC and advance PC by 3.
    fn fetch_addr24(&mut self) -> u32 {
        let lo = self.fetch_byte() as u32;
        let mid = self.fetch_byte() as u32;
        let hi = self.fetch_byte() as u32;
        lo | mid << 8 | hi << 16
    }

    // -------------------------------------------------------------------------
    // Flag helpers
    // -------------------------------------------------------------------------

    /// Update N and Z flags based on a value and a bit-width mask.
    /// `width_mask` is 0x80 for 8-bit mode, 0x8000 for 16-bit mode.
    fn set_nz(&mut self, value: u16, width_mask: u16) {
        self.set_flag_n(value & width_mask != 0);
        let z_mask = if width_mask == 0x80 { 0x00FF } else { 0xFFFF };
        self.set_flag_z(value & z_mask == 0);
    }

    fn set_nz_m(&mut self, value: u16) {
        if self.m_flag() {
            self.set_nz(value, 0x80);
        } else {
            self.set_nz(value, 0x8000);
        }
    }

    fn set_nz_x(&mut self, value: u16) {
        if self.x_flag() {
            self.set_nz(value, 0x80);
        } else {
            self.set_nz(value, 0x8000);
        }
    }

    /// Write `val` into A (respecting M width) and update N/Z flags.
    fn lda_store(&mut self, val: u16) {
        self.write_a(val);
        let a = self.a;
        self.set_nz_m(a);
    }

    /// Write `val` into X (respecting X width) and update N/Z flags.
    fn ldx_store(&mut self, val: u16) {
        self.write_x(val);
        self.set_nz_x(self.x);
    }

    /// Write `val` into Y (respecting X width) and update N/Z flags.
    fn ldy_store(&mut self, val: u16) {
        self.write_y(val);
        self.set_nz_x(self.y);
    }

    // -------------------------------------------------------------------------
    // Implied-mode opcodes
    // -------------------------------------------------------------------------

    fn op_nop(&mut self) -> u8 {
        2
    }

    /// WDM — reserved 2-byte NOP (consumes one operand byte, 2 cycles, no flags).
    fn op_wdm(&mut self) -> u8 {
        self.fetch_byte(); // consume operand
        2
    }

    fn op_wai(&mut self) -> u8 {
        self.waiting = true;
        4
    }

    fn op_stp(&mut self) -> u8 {
        4
    }

    fn op_mvn(&mut self) -> u8 {
        let state = BlockMoveState {
            dst_bank: self.fetch_byte(),
            src_bank: self.fetch_byte(),
            direction: BlockMoveDirection::Increment,
        };
        self.pc = self.pc.wrapping_sub(1);
        self.block_move_state = Some(state);
        self.step_block_move_unit(state) + if self.e { 2 } else { 0 }
    }

    fn op_mvp(&mut self) -> u8 {
        let state = BlockMoveState {
            dst_bank: self.fetch_byte(),
            src_bank: self.fetch_byte(),
            direction: BlockMoveDirection::Decrement,
        };
        self.pc = self.pc.wrapping_sub(1);
        self.block_move_state = Some(state);
        self.step_block_move_unit(state) + if self.e { 2 } else { 0 }
    }

    fn step_block_move_unit(&mut self, state: BlockMoveState) -> u8 {
        let src_addr = (state.src_bank as u32) << 16 | self.x as u32;
        let dst_addr = (state.dst_bank as u32) << 16 | self.y as u32;
        let byte = self.read8(src_addr);
        self.write8(dst_addr, byte);
        self.dbr = state.dst_bank;

        match state.direction {
            BlockMoveDirection::Increment => {
                self.write_x(self.read_x().wrapping_add(1));
                self.write_y(self.read_y().wrapping_add(1));
            }
            BlockMoveDirection::Decrement => {
                self.write_x(self.read_x().wrapping_sub(1));
                self.write_y(self.read_y().wrapping_sub(1));
            }
        }

        self.a = self.a.wrapping_sub(1);
        if self.a == 0xFFFF {
            self.block_move_state = None;
            self.pc = self.pc.wrapping_add(1);
        }

        7
    }

    fn op_rep(&mut self) -> u8 {
        let mask = self.fetch_byte();
        self.rep(mask);
        3
    }

    fn op_sep(&mut self) -> u8 {
        let mask = self.fetch_byte();
        self.sep(mask);
        3
    }

    fn op_xce(&mut self) -> u8 {
        self.xce();
        2
    }

    fn op_clc(&mut self) -> u8 {
        self.set_flag_c(false);
        2
    }

    fn op_sec(&mut self) -> u8 {
        self.set_flag_c(true);
        2
    }

    fn op_cli(&mut self) -> u8 {
        self.set_flag_i(false);
        2
    }

    fn op_sei(&mut self) -> u8 {
        self.set_flag_i(true);
        2
    }

    fn op_clv(&mut self) -> u8 {
        self.set_flag_v(false);
        2
    }

    fn op_cld(&mut self) -> u8 {
        self.set_flag_d(false);
        2
    }

    fn op_sed(&mut self) -> u8 {
        self.set_flag_d(true);
        2
    }

    fn op_tax(&mut self) -> u8 {
        let val = if self.x_flag() {
            self.a & 0x00FF
        } else {
            self.a
        };
        self.ldx_store(val);
        2
    }

    fn op_txa(&mut self) -> u8 {
        let val = if self.m_flag() {
            self.x & 0x00FF
        } else {
            self.x
        };
        self.lda_store(val);
        2
    }

    fn op_tay(&mut self) -> u8 {
        let val = if self.x_flag() {
            self.a & 0x00FF
        } else {
            self.a
        };
        self.ldy_store(val);
        2
    }

    fn op_tya(&mut self) -> u8 {
        let val = if self.m_flag() {
            self.y & 0x00FF
        } else {
            self.y
        };
        self.lda_store(val);
        2
    }

    fn op_txs(&mut self) -> u8 {
        // TXS does not set flags. In emulation mode write_s forces high byte to $01.
        let val = self.x;
        self.write_s(val);
        2
    }

    fn op_tsx(&mut self) -> u8 {
        let val = if self.x_flag() {
            self.s & 0x00FF
        } else {
            self.s
        };
        self.ldx_store(val);
        2
    }

    fn op_txy(&mut self) -> u8 {
        let val = self.x;
        self.ldy_store(val);
        2
    }

    fn op_tyx(&mut self) -> u8 {
        let val = self.y;
        self.ldx_store(val);
        2
    }

    fn op_tcd(&mut self) -> u8 {
        // Always 16-bit regardless of M flag
        self.d = self.a;
        self.set_nz(self.d, 0x8000);
        2
    }

    fn op_tdc(&mut self) -> u8 {
        // Always 16-bit regardless of M flag; loads into full C (A register)
        self.a = self.d;
        let a = self.a;
        self.set_nz(a, 0x8000);
        2
    }

    fn op_tcs(&mut self) -> u8 {
        // Always uses full 16-bit A; no flags set.
        // write_s() enforces emulation-mode page-1 clamping (high byte = $01).
        let val = self.a;
        self.write_s(val);
        2
    }

    fn op_tsc(&mut self) -> u8 {
        // Always 16-bit; loads S into full C (A register)
        self.a = self.s;
        let a = self.a;
        self.set_nz(a, 0x8000);
        2
    }

    fn op_xba(&mut self) -> u8 {
        let lo = (self.a & 0x00FF) as u8;
        let hi = ((self.a >> 8) & 0xFF) as u8;
        self.a = (lo as u16) << 8 | hi as u16;
        // N and Z are set based on the new low byte (hi of original)
        let new_lo = hi as u16;
        self.set_nz(new_lo, 0x80);
        3
    }

    // -------------------------------------------------------------------------
    // LDA — load accumulator
    // -------------------------------------------------------------------------

    fn op_lda_imm(&mut self) -> u8 {
        let val = if self.m_flag() {
            self.fetch_byte() as u16
        } else {
            self.fetch_word()
        };
        self.lda_store(val);
        2 + !self.m_flag() as u8
    }

    fn op_lda_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.read_m(ea);
        self.lda_store(val);
        3
    }

    fn op_lda_dp_x(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x(off);
        let val = self.read_m(ea);
        self.lda_store(val);
        4
    }

    fn op_lda_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.read_m(ea);
        self.lda_store(val);
        4
    }

    fn op_lda_abs_x(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_x(abs);
        let val = self.read_m(ea);
        self.lda_store(val);
        4 + self.last_page_crossed as u8
    }

    fn op_lda_abs_y(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_y(abs);
        let val = self.read_m(ea);
        self.lda_store(val);
        4 + self.last_page_crossed as u8
    }

    fn op_lda_abs_long(&mut self) -> u8 {
        let addr = self.fetch_addr24();
        let ea = self.addr_abs_long(addr);
        let val = self.read_m(ea);
        self.lda_store(val);
        5
    }

    fn op_lda_abs_long_x(&mut self) -> u8 {
        let addr = self.fetch_addr24();
        let ea = self.addr_abs_long_x(addr);
        let val = self.read_m(ea);
        self.lda_store(val);
        5
    }

    fn op_lda_dp_x_ind(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x_ind(off);
        let val = self.read_m(ea);
        self.lda_store(val);
        6
    }

    fn op_lda_dp_ind_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_y(off);
        let val = self.read_m(ea);
        self.lda_store(val);
        5 + self.last_page_crossed as u8
    }

    fn op_lda_dp_ind(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind(off);
        let val = self.read_m(ea);
        self.lda_store(val);
        5
    }

    fn op_lda_dp_ind_long(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_long(off);
        let val = self.read_m(ea);
        self.lda_store(val);
        6
    }

    fn op_lda_dp_ind_long_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_long_y(off);
        let val = self.read_m(ea);
        self.lda_store(val);
        6
    }

    fn op_lda_sr(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_sr(off);
        let val = self.read_m(ea);
        self.lda_store(val);
        4
    }

    fn op_lda_sr_ind_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_sr_ind_y(off);
        let val = self.read_m(ea);
        self.lda_store(val);
        7
    }

    // -------------------------------------------------------------------------
    // LDX — load X index register
    // -------------------------------------------------------------------------

    fn op_ldx_imm(&mut self) -> u8 {
        let val = if self.x_flag() {
            self.fetch_byte() as u16
        } else {
            self.fetch_word()
        };
        self.ldx_store(val);
        2 + !self.x_flag() as u8
    }

    fn op_ldx_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.read_idx(ea);
        self.ldx_store(val);
        3
    }

    fn op_ldx_dp_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_y(off);
        let val = self.read_idx(ea);
        self.ldx_store(val);
        4
    }

    fn op_ldx_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.read_idx(ea);
        self.ldx_store(val);
        4
    }

    fn op_ldx_abs_y(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_y(abs);
        let val = self.read_idx(ea);
        self.ldx_store(val);
        4 + self.last_page_crossed as u8
    }

    // -------------------------------------------------------------------------
    // LDY — load Y index register
    // -------------------------------------------------------------------------

    fn op_ldy_imm(&mut self) -> u8 {
        let val = if self.x_flag() {
            self.fetch_byte() as u16
        } else {
            self.fetch_word()
        };
        self.ldy_store(val);
        2 + !self.x_flag() as u8
    }

    fn op_ldy_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.read_idx(ea);
        self.ldy_store(val);
        3
    }

    fn op_ldy_dp_x(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x(off);
        let val = self.read_idx(ea);
        self.ldy_store(val);
        4
    }

    fn op_ldy_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.read_idx(ea);
        self.ldy_store(val);
        4
    }

    fn op_ldy_abs_x(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_x(abs);
        let val = self.read_idx(ea);
        self.ldy_store(val);
        4 + self.last_page_crossed as u8
    }

    // -------------------------------------------------------------------------
    // STA — store accumulator (no flags affected)
    // -------------------------------------------------------------------------

    fn op_sta_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.a;
        self.write_m(ea, val);
        3
    }

    fn op_sta_dp_x(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x(off);
        let val = self.a;
        self.write_m(ea, val);
        4
    }

    fn op_sta_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.a;
        self.write_m(ea, val);
        4
    }

    fn op_sta_abs_x(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_x(abs);
        let val = self.a;
        self.write_m(ea, val);
        5
    }

    fn op_sta_abs_y(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_y(abs);
        let val = self.a;
        self.write_m(ea, val);
        5
    }

    fn op_sta_abs_long(&mut self) -> u8 {
        let addr = self.fetch_addr24();
        let ea = self.addr_abs_long(addr);
        let val = self.a;
        self.write_m(ea, val);
        5
    }

    fn op_sta_abs_long_x(&mut self) -> u8 {
        let addr = self.fetch_addr24();
        let ea = self.addr_abs_long_x(addr);
        let val = self.a;
        self.write_m(ea, val);
        5
    }

    fn op_sta_dp_x_ind(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x_ind(off);
        let val = self.a;
        self.write_m(ea, val);
        6
    }

    fn op_sta_dp_ind_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_y(off);
        let val = self.a;
        self.write_m(ea, val);
        6
    }

    fn op_sta_dp_ind(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind(off);
        let val = self.a;
        self.write_m(ea, val);
        5
    }

    fn op_sta_dp_ind_long(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_long(off);
        let val = self.a;
        self.write_m(ea, val);
        6
    }

    fn op_sta_dp_ind_long_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_long_y(off);
        let val = self.a;
        self.write_m(ea, val);
        6
    }

    fn op_sta_sr(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_sr(off);
        let val = self.a;
        self.write_m(ea, val);
        4
    }

    fn op_sta_sr_ind_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_sr_ind_y(off);
        let val = self.a;
        self.write_m(ea, val);
        7
    }

    // -------------------------------------------------------------------------
    // STX — store X index register (no flags affected)
    // -------------------------------------------------------------------------

    fn op_stx_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.x;
        self.write_idx(ea, val);
        3
    }

    fn op_stx_dp_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_y(off);
        let val = self.x;
        self.write_idx(ea, val);
        4
    }

    fn op_stx_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.x;
        self.write_idx(ea, val);
        4
    }

    // -------------------------------------------------------------------------
    // STY — store Y index register (no flags affected)
    // -------------------------------------------------------------------------

    fn op_sty_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.y;
        self.write_idx(ea, val);
        3
    }

    fn op_sty_dp_x(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x(off);
        let val = self.y;
        self.write_idx(ea, val);
        4
    }

    fn op_sty_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.y;
        self.write_idx(ea, val);
        4
    }

    // -------------------------------------------------------------------------
    // STZ — store zero (no flags affected)
    // -------------------------------------------------------------------------

    fn op_stz_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        self.write_m(ea, 0);
        3
    }

    fn op_stz_dp_x(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x(off);
        self.write_m(ea, 0);
        4
    }

    fn op_stz_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        self.write_m(ea, 0);
        4
    }

    fn op_stz_abs_x(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_x(abs);
        self.write_m(ea, 0);
        5
    }

    // -------------------------------------------------------------------------
    // ADC — add with carry
    // -------------------------------------------------------------------------

    fn adc_perform(&mut self, operand: u16) {
        if self.m_flag() {
            let a = (self.a & 0x00FF) as u32;
            let op = (operand & 0x00FF) as u32;
            let c = self.flag_c() as u32;
            if self.flag_d() {
                // BCD adjustment: low nibble first, then high nibble
                let mut lo = (a & 0x0F) + (op & 0x0F) + c;
                if lo > 9 {
                    lo = (lo + 6) & 0x0F | 0x10; // carry into high nibble
                }
                let mut result = (a & 0xF0) + (op & 0xF0) + lo;
                let v = ((!(a ^ op) & (a ^ result)) & 0x80) != 0;
                self.set_flag_v(v);
                if result > 0x9F {
                    result += 0x60;
                }
                self.set_flag_c(result > 0xFF);
                self.write_a(result as u16);
                self.set_nz_m(self.a);
            } else {
                let result = a + op + c;
                let v = ((!(a ^ op) & (a ^ result)) & 0x80) != 0;
                self.set_flag_c(result > 0xFF);
                self.set_flag_v(v);
                self.write_a(result as u16);
                let new_a = self.a;
                self.set_nz_m(new_a);
            }
        } else {
            let a = self.a as u32;
            let op = operand as u32;
            let c = self.flag_c() as u32;
            if self.flag_d() {
                // 16-bit BCD: 4 packed decimal digits
                let mut lo = (a & 0x000F) + (op & 0x000F) + c;
                if lo > 9 {
                    lo = (lo + 6) & 0x000F | 0x0010;
                }
                let mut mid_lo = (a & 0x00F0) + (op & 0x00F0) + lo;
                if mid_lo > 0x9F {
                    mid_lo = (mid_lo + 0x60) & 0x00FF | 0x0100;
                }
                let mut mid_hi = (a & 0x0F00) + (op & 0x0F00) + mid_lo;
                if mid_hi > 0x09FF {
                    mid_hi = (mid_hi + 0x0600) & 0x0FFF | 0x1000;
                }
                let mut result = (a & 0xF000) + (op & 0xF000) + mid_hi;
                let v = ((!(a ^ op) & (a ^ result)) & 0x8000) != 0;
                self.set_flag_v(v);
                if result > 0x9FFF {
                    result += 0x6000;
                }
                self.set_flag_c(result > 0xFFFF);
                self.a = result as u16;
                self.set_nz_m(self.a);
            } else {
                let result = a + op + c;
                let v = ((!(a ^ op) & (a ^ result)) & 0x8000) != 0;
                self.set_flag_c(result > 0xFFFF);
                self.set_flag_v(v);
                self.a = result as u16;
                self.set_nz_m(self.a);
            }
        }
    }

    fn sbc_perform(&mut self, operand: u16) {
        if self.flag_d() {
            // BCD subtraction: A - M - (1 - C) = A + ~M + C, then decimal adjust.
            // V is derived from the binary intermediate result (WDC spec).
            if self.m_flag() {
                let a = (self.a & 0x00FF) as u32;
                let op = (operand & 0x00FF) as u32;
                let c = self.flag_c() as u32;
                let not_op = (!op) & 0xFF;
                let bin = a + not_op + c;
                let v = ((!(a ^ not_op) & (a ^ bin)) & 0x80) != 0;
                self.set_flag_v(v);

                let a = a as i32;
                let op = op as i32;
                let c = c as i32;
                let mut lo = (a & 0x0F) - (op & 0x0F) + c - 1;
                if lo < 0 {
                    lo = ((lo - 6) & 0x0F) - 0x10;
                }
                let mut result = (a & 0xF0) - (op & 0xF0) + lo;
                if result < 0 {
                    result -= 0x60;
                }
                self.set_flag_c(result >= 0);
                let r = result as u16;
                self.write_a(r);
                let new_a = self.a;
                self.set_nz_m(new_a);
            } else {
                let a = self.a as u32;
                let op = operand as u32;
                let c = self.flag_c() as u32;
                let not_op = (!op) & 0xFFFF;
                let bin = a + not_op + c;
                let v = ((!(a ^ not_op) & (a ^ bin)) & 0x8000) != 0;
                self.set_flag_v(v);

                let a = a as i32;
                let op = op as i32;
                let c = c as i32;
                let mut lo = (a & 0x000F) - (op & 0x000F) + c - 1;
                if lo < 0 {
                    lo = ((lo - 6) & 0x000F) - 0x10;
                }
                let mut mid_lo = (a & 0x00F0) - (op & 0x00F0) + lo;
                if (mid_lo & 0xFF) < 0 || mid_lo < 0 {
                    mid_lo = ((mid_lo - 0x60) & 0x00FF) - 0x100;
                }
                let mut mid_hi = (a & 0x0F00) - (op & 0x0F00) + mid_lo;
                if mid_hi < 0 {
                    mid_hi = ((mid_hi - 0x0600) & 0x0FFF) - 0x1000;
                }
                let mut result = (a & 0xF000) - (op & 0xF000) + mid_hi;
                if result < 0 {
                    result -= 0x6000;
                }
                self.set_flag_c(result >= 0);
                self.a = result as u16;
                self.set_nz_m(self.a);
            }
        } else {
            self.adc_perform(!operand);
        }
    }

    fn op_adc_imm(&mut self) -> u8 {
        let val = if self.m_flag() {
            self.fetch_byte() as u16
        } else {
            self.fetch_word()
        };
        self.adc_perform(val);
        2 + !self.m_flag() as u8
    }

    fn op_adc_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.read_m(ea);
        self.adc_perform(val);
        3
    }

    fn op_adc_dp_x(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x(off);
        let val = self.read_m(ea);
        self.adc_perform(val);
        4
    }

    fn op_adc_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.read_m(ea);
        self.adc_perform(val);
        4
    }

    fn op_adc_abs_x(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_x(abs);
        let val = self.read_m(ea);
        self.adc_perform(val);
        4 + self.last_page_crossed as u8
    }

    fn op_adc_abs_y(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_y(abs);
        let val = self.read_m(ea);
        self.adc_perform(val);
        4 + self.last_page_crossed as u8
    }

    fn op_adc_abs_long(&mut self) -> u8 {
        let addr = self.fetch_addr24();
        let val = self.read_m(addr);
        self.adc_perform(val);
        5
    }

    fn op_adc_abs_long_x(&mut self) -> u8 {
        let addr = self.fetch_addr24();
        let ea = self.addr_abs_long_x(addr);
        let val = self.read_m(ea);
        self.adc_perform(val);
        5
    }

    fn op_adc_dp_x_ind(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x_ind(off);
        let val = self.read_m(ea);
        self.adc_perform(val);
        6
    }

    fn op_adc_dp_ind_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_y(off);
        let val = self.read_m(ea);
        self.adc_perform(val);
        5 + self.last_page_crossed as u8
    }

    fn op_adc_dp_ind(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind(off);
        let val = self.read_m(ea);
        self.adc_perform(val);
        5
    }

    fn op_adc_dp_ind_long(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_long(off);
        let val = self.read_m(ea);
        self.adc_perform(val);
        6
    }

    fn op_adc_dp_ind_long_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_long_y(off);
        let val = self.read_m(ea);
        self.adc_perform(val);
        6
    }

    fn op_adc_sr(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_sr(off);
        let val = self.read_m(ea);
        self.adc_perform(val);
        4
    }

    fn op_adc_sr_ind_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_sr_ind_y(off);
        let val = self.read_m(ea);
        self.adc_perform(val);
        7
    }

    // -------------------------------------------------------------------------
    // SBC — subtract with borrow (implemented as ADC with one's complement)
    // -------------------------------------------------------------------------

    fn op_sbc_imm(&mut self) -> u8 {
        let val = if self.m_flag() {
            self.fetch_byte() as u16
        } else {
            self.fetch_word()
        };
        self.sbc_perform(val);
        2 + !self.m_flag() as u8
    }

    fn op_sbc_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.read_m(ea);
        self.sbc_perform(val);
        3
    }

    fn op_sbc_dp_x(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x(off);
        let val = self.read_m(ea);
        self.sbc_perform(val);
        4
    }

    fn op_sbc_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.read_m(ea);
        self.sbc_perform(val);
        4
    }

    fn op_sbc_abs_x(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_x(abs);
        let val = self.read_m(ea);
        self.sbc_perform(val);
        4 + self.last_page_crossed as u8
    }

    fn op_sbc_abs_y(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_y(abs);
        let val = self.read_m(ea);
        self.sbc_perform(val);
        4 + self.last_page_crossed as u8
    }

    fn op_sbc_abs_long(&mut self) -> u8 {
        let addr = self.fetch_addr24();
        let val = self.read_m(addr);
        self.sbc_perform(val);
        5
    }

    fn op_sbc_abs_long_x(&mut self) -> u8 {
        let addr = self.fetch_addr24();
        let ea = self.addr_abs_long_x(addr);
        let val = self.read_m(ea);
        self.sbc_perform(val);
        5
    }

    fn op_sbc_dp_x_ind(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x_ind(off);
        let val = self.read_m(ea);
        self.sbc_perform(val);
        6
    }

    fn op_sbc_dp_ind_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_y(off);
        let val = self.read_m(ea);
        self.sbc_perform(val);
        5 + self.last_page_crossed as u8
    }

    fn op_sbc_dp_ind(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind(off);
        let val = self.read_m(ea);
        self.sbc_perform(val);
        5
    }

    fn op_sbc_dp_ind_long(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_long(off);
        let val = self.read_m(ea);
        self.sbc_perform(val);
        6
    }

    fn op_sbc_dp_ind_long_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_long_y(off);
        let val = self.read_m(ea);
        self.sbc_perform(val);
        6
    }

    fn op_sbc_sr(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_sr(off);
        let val = self.read_m(ea);
        self.sbc_perform(val);
        4
    }

    fn op_sbc_sr_ind_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_sr_ind_y(off);
        let val = self.read_m(ea);
        self.sbc_perform(val);
        7
    }

    // -------------------------------------------------------------------------
    // AND — bitwise AND with accumulator
    // -------------------------------------------------------------------------

    fn and_perform(&mut self, operand: u16) {
        let result = self.a & operand;
        self.write_a(result);
        let a = self.a;
        self.set_nz_m(a);
    }

    fn op_and_imm(&mut self) -> u8 {
        let val = if self.m_flag() {
            self.fetch_byte() as u16
        } else {
            self.fetch_word()
        };
        self.and_perform(val);
        2 + !self.m_flag() as u8
    }

    fn op_and_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.read_m(ea);
        self.and_perform(val);
        3
    }

    fn op_and_dp_x(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x(off);
        let val = self.read_m(ea);
        self.and_perform(val);
        4
    }

    fn op_and_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.read_m(ea);
        self.and_perform(val);
        4
    }

    fn op_and_abs_x(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_x(abs);
        let val = self.read_m(ea);
        self.and_perform(val);
        4 + self.last_page_crossed as u8
    }

    fn op_and_abs_y(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_y(abs);
        let val = self.read_m(ea);
        self.and_perform(val);
        4 + self.last_page_crossed as u8
    }

    fn op_and_abs_long(&mut self) -> u8 {
        let addr = self.fetch_addr24();
        let val = self.read_m(addr);
        self.and_perform(val);
        5
    }

    fn op_and_abs_long_x(&mut self) -> u8 {
        let addr = self.fetch_addr24();
        let ea = self.addr_abs_long_x(addr);
        let val = self.read_m(ea);
        self.and_perform(val);
        5
    }

    fn op_and_dp_x_ind(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x_ind(off);
        let val = self.read_m(ea);
        self.and_perform(val);
        6
    }

    fn op_and_dp_ind_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_y(off);
        let val = self.read_m(ea);
        self.and_perform(val);
        5 + self.last_page_crossed as u8
    }

    fn op_and_dp_ind(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind(off);
        let val = self.read_m(ea);
        self.and_perform(val);
        5
    }

    fn op_and_dp_ind_long(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_long(off);
        let val = self.read_m(ea);
        self.and_perform(val);
        6
    }

    fn op_and_dp_ind_long_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_long_y(off);
        let val = self.read_m(ea);
        self.and_perform(val);
        6
    }

    fn op_and_sr(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_sr(off);
        let val = self.read_m(ea);
        self.and_perform(val);
        4
    }

    fn op_and_sr_ind_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_sr_ind_y(off);
        let val = self.read_m(ea);
        self.and_perform(val);
        7
    }

    // -------------------------------------------------------------------------
    // ORA — bitwise OR with accumulator
    // -------------------------------------------------------------------------

    fn ora_perform(&mut self, operand: u16) {
        let result = self.a | operand;
        self.write_a(result);
        let a = self.a;
        self.set_nz_m(a);
    }

    fn op_ora_imm(&mut self) -> u8 {
        let val = if self.m_flag() {
            self.fetch_byte() as u16
        } else {
            self.fetch_word()
        };
        self.ora_perform(val);
        2 + !self.m_flag() as u8
    }

    fn op_ora_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.read_m(ea);
        self.ora_perform(val);
        3
    }

    fn op_ora_dp_x(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x(off);
        let val = self.read_m(ea);
        self.ora_perform(val);
        4
    }

    fn op_ora_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.read_m(ea);
        self.ora_perform(val);
        4
    }

    fn op_ora_abs_x(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_x(abs);
        let val = self.read_m(ea);
        self.ora_perform(val);
        4 + self.last_page_crossed as u8
    }

    fn op_ora_abs_y(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_y(abs);
        let val = self.read_m(ea);
        self.ora_perform(val);
        4 + self.last_page_crossed as u8
    }

    fn op_ora_abs_long(&mut self) -> u8 {
        let addr = self.fetch_addr24();
        let val = self.read_m(addr);
        self.ora_perform(val);
        5
    }

    fn op_ora_abs_long_x(&mut self) -> u8 {
        let addr = self.fetch_addr24();
        let ea = self.addr_abs_long_x(addr);
        let val = self.read_m(ea);
        self.ora_perform(val);
        5
    }

    fn op_ora_dp_x_ind(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x_ind(off);
        let val = self.read_m(ea);
        self.ora_perform(val);
        6
    }

    fn op_ora_dp_ind_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_y(off);
        let val = self.read_m(ea);
        self.ora_perform(val);
        5 + self.last_page_crossed as u8
    }

    fn op_ora_dp_ind(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind(off);
        let val = self.read_m(ea);
        self.ora_perform(val);
        5
    }

    fn op_ora_dp_ind_long(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_long(off);
        let val = self.read_m(ea);
        self.ora_perform(val);
        6
    }

    fn op_ora_dp_ind_long_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_long_y(off);
        let val = self.read_m(ea);
        self.ora_perform(val);
        6
    }

    fn op_ora_sr(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_sr(off);
        let val = self.read_m(ea);
        self.ora_perform(val);
        4
    }

    fn op_ora_sr_ind_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_sr_ind_y(off);
        let val = self.read_m(ea);
        self.ora_perform(val);
        7
    }

    // -------------------------------------------------------------------------
    // EOR — bitwise XOR with accumulator
    // -------------------------------------------------------------------------

    fn eor_perform(&mut self, operand: u16) {
        let result = self.a ^ operand;
        self.write_a(result);
        let a = self.a;
        self.set_nz_m(a);
    }

    fn op_eor_imm(&mut self) -> u8 {
        let val = if self.m_flag() {
            self.fetch_byte() as u16
        } else {
            self.fetch_word()
        };
        self.eor_perform(val);
        2 + !self.m_flag() as u8
    }

    fn op_eor_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.read_m(ea);
        self.eor_perform(val);
        3
    }

    fn op_eor_dp_x(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x(off);
        let val = self.read_m(ea);
        self.eor_perform(val);
        4
    }

    fn op_eor_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.read_m(ea);
        self.eor_perform(val);
        4
    }

    fn op_eor_abs_x(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_x(abs);
        let val = self.read_m(ea);
        self.eor_perform(val);
        4 + self.last_page_crossed as u8
    }

    fn op_eor_abs_y(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_y(abs);
        let val = self.read_m(ea);
        self.eor_perform(val);
        4 + self.last_page_crossed as u8
    }

    fn op_eor_abs_long(&mut self) -> u8 {
        let addr = self.fetch_addr24();
        let val = self.read_m(addr);
        self.eor_perform(val);
        5
    }

    fn op_eor_abs_long_x(&mut self) -> u8 {
        let addr = self.fetch_addr24();
        let ea = self.addr_abs_long_x(addr);
        let val = self.read_m(ea);
        self.eor_perform(val);
        5
    }

    fn op_eor_dp_x_ind(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x_ind(off);
        let val = self.read_m(ea);
        self.eor_perform(val);
        6
    }

    fn op_eor_dp_ind_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_y(off);
        let val = self.read_m(ea);
        self.eor_perform(val);
        5 + self.last_page_crossed as u8
    }

    fn op_eor_dp_ind(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind(off);
        let val = self.read_m(ea);
        self.eor_perform(val);
        5
    }

    fn op_eor_dp_ind_long(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_long(off);
        let val = self.read_m(ea);
        self.eor_perform(val);
        6
    }

    fn op_eor_dp_ind_long_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_long_y(off);
        let val = self.read_m(ea);
        self.eor_perform(val);
        6
    }

    fn op_eor_sr(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_sr(off);
        let val = self.read_m(ea);
        self.eor_perform(val);
        4
    }

    fn op_eor_sr_ind_y(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_sr_ind_y(off);
        let val = self.read_m(ea);
        self.eor_perform(val);
        7
    }

    // -------------------------------------------------------------------------
    // BIT — test bits
    // Immediate: Z = !(A & imm); N and V are NOT changed.
    // Memory:    Z = !(A & mem); N = bit(width-1) of mem; V = bit(width-2) of mem.
    // -------------------------------------------------------------------------

    fn bit_imm_perform(&mut self, operand: u16) {
        let masked = if self.m_flag() {
            (self.a & 0xFF) & (operand & 0xFF)
        } else {
            self.a & operand
        };
        self.set_flag_z(masked == 0);
        // N and V flags are NOT updated by BIT immediate
    }

    fn bit_mem_perform(&mut self, operand: u16) {
        let (masked, n_bit, v_bit) = if self.m_flag() {
            let op8 = operand & 0xFF;
            ((self.a & 0xFF) & op8, op8 & 0x80 != 0, op8 & 0x40 != 0)
        } else {
            (
                self.a & operand,
                operand & 0x8000 != 0,
                operand & 0x4000 != 0,
            )
        };
        self.set_flag_z(masked == 0);
        self.set_flag_n(n_bit);
        self.set_flag_v(v_bit);
    }

    fn op_bit_imm(&mut self) -> u8 {
        let val = if self.m_flag() {
            self.fetch_byte() as u16
        } else {
            self.fetch_word()
        };
        self.bit_imm_perform(val);
        2 + !self.m_flag() as u8
    }

    fn op_bit_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.read_m(ea);
        self.bit_mem_perform(val);
        3
    }

    fn op_bit_dp_x(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x(off);
        let val = self.read_m(ea);
        self.bit_mem_perform(val);
        4
    }

    fn op_bit_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.read_m(ea);
        self.bit_mem_perform(val);
        4
    }

    fn op_bit_abs_x(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_x(abs);
        let val = self.read_m(ea);
        self.bit_mem_perform(val);
        4 + self.last_page_crossed as u8
    }

    // -------------------------------------------------------------------------
    // CMP/CPX/CPY — compare (sets N, Z, C; does not change registers)
    // Result = reg - operand (discarded). C=1 if reg >= operand (unsigned).
    // -------------------------------------------------------------------------

    fn cmp_perform(&mut self, reg: u16, operand: u16, wide: bool) {
        let (result, c) = if wide {
            let r = (reg as u32).wrapping_sub(operand as u32);
            (r as u16, reg >= operand)
        } else {
            let reg8 = reg & 0xFF;
            let op8 = operand & 0xFF;
            let r = (reg8 as u32).wrapping_sub(op8 as u32);
            (r as u16, reg8 >= op8)
        };
        self.set_flag_c(c);
        self.set_flag_z(result == 0);
        let n = if wide {
            result & 0x8000 != 0
        } else {
            result & 0x80 != 0
        };
        self.set_flag_n(n);
    }

    fn op_cmp_imm(&mut self) -> u8 {
        let wide = !self.m_flag();
        let val = if self.m_flag() {
            self.fetch_byte() as u16
        } else {
            self.fetch_word()
        };
        let a = self.a;
        self.cmp_perform(a, val, wide);
        2 + !self.m_flag() as u8
    }

    fn op_cmp_dp(&mut self) -> u8 {
        let wide = !self.m_flag();
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.read_m(ea);
        let a = self.a;
        self.cmp_perform(a, val, wide);
        3
    }

    fn op_cmp_dp_x(&mut self) -> u8 {
        let wide = !self.m_flag();
        let off = self.fetch_byte();
        let ea = self.addr_dp_x(off);
        let val = self.read_m(ea);
        let a = self.a;
        self.cmp_perform(a, val, wide);
        4
    }

    fn op_cmp_abs(&mut self) -> u8 {
        let wide = !self.m_flag();
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.read_m(ea);
        let a = self.a;
        self.cmp_perform(a, val, wide);
        4
    }

    fn op_cmp_abs_x(&mut self) -> u8 {
        let wide = !self.m_flag();
        let abs = self.fetch_word();
        let ea = self.addr_abs_x(abs);
        let val = self.read_m(ea);
        let a = self.a;
        self.cmp_perform(a, val, wide);
        4 + self.last_page_crossed as u8
    }

    fn op_cmp_abs_y(&mut self) -> u8 {
        let wide = !self.m_flag();
        let abs = self.fetch_word();
        let ea = self.addr_abs_y(abs);
        let val = self.read_m(ea);
        let a = self.a;
        self.cmp_perform(a, val, wide);
        4 + self.last_page_crossed as u8
    }

    fn op_cmp_abs_long(&mut self) -> u8 {
        let wide = !self.m_flag();
        let addr = self.fetch_addr24();
        let val = self.read_m(addr);
        let a = self.a;
        self.cmp_perform(a, val, wide);
        5
    }

    fn op_cmp_abs_long_x(&mut self) -> u8 {
        let wide = !self.m_flag();
        let addr = self.fetch_addr24();
        let ea = self.addr_abs_long_x(addr);
        let val = self.read_m(ea);
        let a = self.a;
        self.cmp_perform(a, val, wide);
        5
    }

    fn op_cmp_dp_x_ind(&mut self) -> u8 {
        let wide = !self.m_flag();
        let off = self.fetch_byte();
        let ea = self.addr_dp_x_ind(off);
        let val = self.read_m(ea);
        let a = self.a;
        self.cmp_perform(a, val, wide);
        6
    }

    fn op_cmp_dp_ind_y(&mut self) -> u8 {
        let wide = !self.m_flag();
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_y(off);
        let val = self.read_m(ea);
        let a = self.a;
        self.cmp_perform(a, val, wide);
        5 + self.last_page_crossed as u8
    }

    fn op_cmp_dp_ind(&mut self) -> u8 {
        let wide = !self.m_flag();
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind(off);
        let val = self.read_m(ea);
        let a = self.a;
        self.cmp_perform(a, val, wide);
        5
    }

    fn op_cmp_dp_ind_long(&mut self) -> u8 {
        let wide = !self.m_flag();
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_long(off);
        let val = self.read_m(ea);
        let a = self.a;
        self.cmp_perform(a, val, wide);
        6
    }

    fn op_cmp_dp_ind_long_y(&mut self) -> u8 {
        let wide = !self.m_flag();
        let off = self.fetch_byte();
        let ea = self.addr_dp_ind_long_y(off);
        let val = self.read_m(ea);
        let a = self.a;
        self.cmp_perform(a, val, wide);
        6
    }

    fn op_cmp_sr(&mut self) -> u8 {
        let wide = !self.m_flag();
        let off = self.fetch_byte();
        let ea = self.addr_sr(off);
        let val = self.read_m(ea);
        let a = self.a;
        self.cmp_perform(a, val, wide);
        4
    }

    fn op_cmp_sr_ind_y(&mut self) -> u8 {
        let wide = !self.m_flag();
        let off = self.fetch_byte();
        let ea = self.addr_sr_ind_y(off);
        let val = self.read_m(ea);
        let a = self.a;
        self.cmp_perform(a, val, wide);
        7
    }

    fn op_cpx_imm(&mut self) -> u8 {
        let wide = !self.x_flag();
        let val = if self.x_flag() {
            self.fetch_byte() as u16
        } else {
            self.fetch_word()
        };
        let x = self.x;
        self.cmp_perform(x, val, wide);
        2 + !self.x_flag() as u8
    }

    fn op_cpx_dp(&mut self) -> u8 {
        let wide = !self.x_flag();
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.read_idx(ea);
        let x = self.x;
        self.cmp_perform(x, val, wide);
        3
    }

    fn op_cpx_abs(&mut self) -> u8 {
        let wide = !self.x_flag();
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.read_idx(ea);
        let x = self.x;
        self.cmp_perform(x, val, wide);
        4
    }

    fn op_cpy_imm(&mut self) -> u8 {
        let wide = !self.x_flag();
        let val = if self.x_flag() {
            self.fetch_byte() as u16
        } else {
            self.fetch_word()
        };
        let y = self.y;
        self.cmp_perform(y, val, wide);
        2 + !self.x_flag() as u8
    }

    fn op_cpy_dp(&mut self) -> u8 {
        let wide = !self.x_flag();
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.read_idx(ea);
        let y = self.y;
        self.cmp_perform(y, val, wide);
        3
    }

    fn op_cpy_abs(&mut self) -> u8 {
        let wide = !self.x_flag();
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.read_idx(ea);
        let y = self.y;
        self.cmp_perform(y, val, wide);
        4
    }

    // -------------------------------------------------------------------------
    // INC / DEC — increment / decrement memory or accumulator
    // -------------------------------------------------------------------------

    fn inc_perform_m(&mut self, val: u16) -> u16 {
        let result = if self.m_flag() {
            ((val as u8).wrapping_add(1)) as u16
        } else {
            val.wrapping_add(1)
        };
        self.set_nz_m(result);
        result
    }

    fn dec_perform_m(&mut self, val: u16) -> u16 {
        let result = if self.m_flag() {
            ((val as u8).wrapping_sub(1)) as u16
        } else {
            val.wrapping_sub(1)
        };
        self.set_nz_m(result);
        result
    }

    fn op_inc_acc(&mut self) -> u8 {
        let val = self.a;
        let result = self.inc_perform_m(val);
        self.write_a(result);
        2
    }

    fn op_dec_acc(&mut self) -> u8 {
        let val = self.a;
        let result = self.dec_perform_m(val);
        self.write_a(result);
        2
    }

    fn op_inc_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.read_m(ea);
        let result = self.inc_perform_m(val);
        self.write_m(ea, result);
        5
    }

    fn op_inc_dp_x(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x(off);
        let val = self.read_m(ea);
        let result = self.inc_perform_m(val);
        self.write_m(ea, result);
        6
    }

    fn op_inc_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.read_m(ea);
        let result = self.inc_perform_m(val);
        self.write_m(ea, result);
        6
    }

    fn op_inc_abs_x(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_x(abs);
        let val = self.read_m(ea);
        let result = self.inc_perform_m(val);
        self.write_m(ea, result);
        7
    }

    fn op_dec_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.read_m(ea);
        let result = self.dec_perform_m(val);
        self.write_m(ea, result);
        5
    }

    fn op_dec_dp_x(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x(off);
        let val = self.read_m(ea);
        let result = self.dec_perform_m(val);
        self.write_m(ea, result);
        6
    }

    fn op_dec_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.read_m(ea);
        let result = self.dec_perform_m(val);
        self.write_m(ea, result);
        6
    }

    fn op_dec_abs_x(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_x(abs);
        let val = self.read_m(ea);
        let result = self.dec_perform_m(val);
        self.write_m(ea, result);
        7
    }

    // -------------------------------------------------------------------------
    // INX / DEX / INY / DEY — implied register increment/decrement
    // -------------------------------------------------------------------------

    fn inc_perform_x(&mut self, val: u16) -> u16 {
        let result = if self.x_flag() {
            ((val as u8).wrapping_add(1)) as u16
        } else {
            val.wrapping_add(1)
        };
        self.set_nz_x(result);
        result
    }

    fn dec_perform_x(&mut self, val: u16) -> u16 {
        let result = if self.x_flag() {
            ((val as u8).wrapping_sub(1)) as u16
        } else {
            val.wrapping_sub(1)
        };
        self.set_nz_x(result);
        result
    }

    fn op_inx(&mut self) -> u8 {
        let val = self.x;
        self.x = self.inc_perform_x(val);
        2
    }

    fn op_dex(&mut self) -> u8 {
        let val = self.x;
        self.x = self.dec_perform_x(val);
        2
    }

    fn op_iny(&mut self) -> u8 {
        let val = self.y;
        self.y = self.inc_perform_x(val);
        2
    }

    fn op_dey(&mut self) -> u8 {
        let val = self.y;
        self.y = self.dec_perform_x(val);
        2
    }

    // -------------------------------------------------------------------------
    // ASL — arithmetic shift left (C <- [high bit], bit 0 <- 0)
    // -------------------------------------------------------------------------

    fn asl_perform(&mut self, val: u16) -> u16 {
        let (result, c) = if self.m_flag() {
            let v = val & 0xFF;
            (((v << 1) & 0xFF), v & 0x80 != 0)
        } else {
            ((val << 1), val & 0x8000 != 0)
        };
        self.set_flag_c(c);
        self.set_nz_m(result);
        result
    }

    fn op_asl_acc(&mut self) -> u8 {
        let val = self.a;
        let result = self.asl_perform(val);
        self.write_a(result);
        2
    }

    fn op_asl_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.read_m(ea);
        let result = self.asl_perform(val);
        self.write_m(ea, result);
        5
    }

    fn op_asl_dp_x(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x(off);
        let val = self.read_m(ea);
        let result = self.asl_perform(val);
        self.write_m(ea, result);
        6
    }

    fn op_asl_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.read_m(ea);
        let result = self.asl_perform(val);
        self.write_m(ea, result);
        6
    }

    fn op_asl_abs_x(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_x(abs);
        let val = self.read_m(ea);
        let result = self.asl_perform(val);
        self.write_m(ea, result);
        7
    }

    // -------------------------------------------------------------------------
    // LSR — logical shift right (bit(width-1) <- 0, C <- bit 0)
    // -------------------------------------------------------------------------

    fn lsr_perform(&mut self, val: u16) -> u16 {
        let c = val & 0x01 != 0;
        let result = if self.m_flag() {
            (val & 0xFF) >> 1
        } else {
            val >> 1
        };
        self.set_flag_c(c);
        self.set_nz_m(result);
        result
    }

    fn op_lsr_acc(&mut self) -> u8 {
        let val = self.a;
        let result = self.lsr_perform(val);
        self.write_a(result);
        2
    }

    fn op_lsr_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.read_m(ea);
        let result = self.lsr_perform(val);
        self.write_m(ea, result);
        5
    }

    fn op_lsr_dp_x(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x(off);
        let val = self.read_m(ea);
        let result = self.lsr_perform(val);
        self.write_m(ea, result);
        6
    }

    fn op_lsr_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.read_m(ea);
        let result = self.lsr_perform(val);
        self.write_m(ea, result);
        6
    }

    fn op_lsr_abs_x(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_x(abs);
        let val = self.read_m(ea);
        let result = self.lsr_perform(val);
        self.write_m(ea, result);
        7
    }

    // -------------------------------------------------------------------------
    // ROL — rotate left through carry (C <- high bit, bit 0 <- old C)
    // -------------------------------------------------------------------------

    fn rol_perform(&mut self, val: u16) -> u16 {
        let old_c = self.flag_c() as u16;
        let (result, c) = if self.m_flag() {
            let v = val & 0xFF;
            (((v << 1) & 0xFF) | old_c, v & 0x80 != 0)
        } else {
            ((val << 1) | old_c, val & 0x8000 != 0)
        };
        self.set_flag_c(c);
        self.set_nz_m(result);
        result
    }

    fn op_rol_acc(&mut self) -> u8 {
        let val = self.a;
        let result = self.rol_perform(val);
        self.write_a(result);
        2
    }

    fn op_rol_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.read_m(ea);
        let result = self.rol_perform(val);
        self.write_m(ea, result);
        5
    }

    fn op_rol_dp_x(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x(off);
        let val = self.read_m(ea);
        let result = self.rol_perform(val);
        self.write_m(ea, result);
        6
    }

    fn op_rol_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.read_m(ea);
        let result = self.rol_perform(val);
        self.write_m(ea, result);
        6
    }

    fn op_rol_abs_x(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_x(abs);
        let val = self.read_m(ea);
        let result = self.rol_perform(val);
        self.write_m(ea, result);
        7
    }

    // -------------------------------------------------------------------------
    // ROR — rotate right through carry (C <- bit 0, high bit <- old C)
    // -------------------------------------------------------------------------

    fn ror_perform(&mut self, val: u16) -> u16 {
        let old_c = self.flag_c() as u16;
        let c = val & 0x01 != 0;
        let result = if self.m_flag() {
            ((val & 0xFF) >> 1) | (old_c << 7)
        } else {
            (val >> 1) | (old_c << 15)
        };
        self.set_flag_c(c);
        self.set_nz_m(result);
        result
    }

    fn op_ror_acc(&mut self) -> u8 {
        let val = self.a;
        let result = self.ror_perform(val);
        self.write_a(result);
        2
    }

    fn op_ror_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let val = self.read_m(ea);
        let result = self.ror_perform(val);
        self.write_m(ea, result);
        5
    }

    fn op_ror_dp_x(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp_x(off);
        let val = self.read_m(ea);
        let result = self.ror_perform(val);
        self.write_m(ea, result);
        6
    }

    fn op_ror_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let val = self.read_m(ea);
        let result = self.ror_perform(val);
        self.write_m(ea, result);
        6
    }

    fn op_ror_abs_x(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs_x(abs);
        let val = self.read_m(ea);
        let result = self.ror_perform(val);
        self.write_m(ea, result);
        7
    }

    // -------------------------------------------------------------------------
    // TSB — test and set bits: Z = !(A & mem); mem |= A
    // TRB — test and reset bits: Z = !(A & mem); mem &= ~A
    // -------------------------------------------------------------------------

    fn tsb_trb_z(&mut self, a: u16, mem: u16) {
        let masked = if self.m_flag() {
            (a & 0xFF) & (mem & 0xFF)
        } else {
            a & mem
        };
        self.set_flag_z(masked == 0);
    }

    fn op_tsb_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let mem = self.read_m(ea);
        let a = self.a;
        self.tsb_trb_z(a, mem);
        self.write_m(ea, mem | a);
        5
    }

    fn op_tsb_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let mem = self.read_m(ea);
        let a = self.a;
        self.tsb_trb_z(a, mem);
        self.write_m(ea, mem | a);
        6
    }

    fn op_trb_dp(&mut self) -> u8 {
        let off = self.fetch_byte();
        let ea = self.addr_dp(off);
        let mem = self.read_m(ea);
        let a = self.a;
        self.tsb_trb_z(a, mem);
        self.write_m(ea, mem & !a);
        5
    }

    fn op_trb_abs(&mut self) -> u8 {
        let abs = self.fetch_word();
        let ea = self.addr_abs(abs);
        let mem = self.read_m(ea);
        let a = self.a;
        self.tsb_trb_z(a, mem);
        self.write_m(ea, mem & !a);
        6
    }

    // -------------------------------------------------------------------------
    // Branches — 8-bit signed relative offset (BCC/BCS/BEQ/BNE/BMI/BPL/BVC/BVS/BRA)
    // BRL — 16-bit signed relative offset
    // -------------------------------------------------------------------------

    fn branch_if(&mut self, taken: bool) -> u8 {
        let offset = self.fetch_byte() as i8;
        if taken {
            let old_pc = self.pc;
            self.pc = self.pc.wrapping_add(offset as u16);
            // +1 cycle for taken; +1 more in emulation mode if page crossed
            let page_cross = (old_pc ^ self.pc) & 0xFF00 != 0;
            if self.e && page_cross { 4 } else { 3 }
        } else {
            2
        }
    }

    fn op_bcc(&mut self) -> u8 {
        let c = !self.flag_c();
        self.branch_if(c)
    }
    fn op_bcs(&mut self) -> u8 {
        let c = self.flag_c();
        self.branch_if(c)
    }
    fn op_beq(&mut self) -> u8 {
        let z = self.flag_z();
        self.branch_if(z)
    }
    fn op_bne(&mut self) -> u8 {
        let z = !self.flag_z();
        self.branch_if(z)
    }
    fn op_bmi(&mut self) -> u8 {
        let n = self.flag_n();
        self.branch_if(n)
    }
    fn op_bpl(&mut self) -> u8 {
        let n = !self.flag_n();
        self.branch_if(n)
    }
    fn op_bvc(&mut self) -> u8 {
        let v = !self.flag_v();
        self.branch_if(v)
    }
    fn op_bvs(&mut self) -> u8 {
        let v = self.flag_v();
        self.branch_if(v)
    }
    fn op_bra(&mut self) -> u8 {
        self.branch_if(true)
    }

    fn op_brl(&mut self) -> u8 {
        let offset = self.fetch_word() as i16;
        self.pc = self.pc.wrapping_add(offset as u16);
        4
    }
}

// Private helpers — suppressed until opcode dispatch is wired up.
#[allow(dead_code)]
impl<B: SnesBus> Cpu<B> {
    // -------------------------------------------------------------------------
    // Addressing mode helpers
    // Each returns a 24-bit effective address (u32, upper byte always 0 for
    // bank-0 modes).  Indirect helpers read pointer bytes via self.bus.
    // -------------------------------------------------------------------------

    /// Direct Page: EA = (D + offset) & 0xFFFF  [bank 0]
    fn addr_dp(&mut self, offset: u8) -> u32 {
        if self.d & 0xFF != 0 {
            self.extra_cycles += 1;
        }
        (self.d as u32 + offset as u32) & 0xFFFF
    }

    /// Direct Page Indexed X: EA = (D + offset + X) & 0xFFFF  [bank 0]
    /// In emulation mode with D low byte = $00, wraps offset indexing within D page.
    fn addr_dp_x(&mut self, offset: u8) -> u32 {
        if self.d & 0xFF != 0 {
            self.extra_cycles += 1;
        }
        let ea = (self.d as u32 + offset as u32 + self.x as u32) & 0xFFFF;
        if self.e && (self.d & 0x00FF) == 0 {
            ((self.d & 0xFF00) as u32) | (ea & 0x00FF)
        } else {
            ea
        }
    }

    /// Direct Page Indexed Y: EA = (D + offset + Y) & 0xFFFF  [bank 0]
    /// In emulation mode with D low byte = $00, wraps offset indexing within D page.
    fn addr_dp_y(&mut self, offset: u8) -> u32 {
        if self.d & 0xFF != 0 {
            self.extra_cycles += 1;
        }
        let ea = (self.d as u32 + offset as u32 + self.y as u32) & 0xFFFF;
        if self.e && (self.d & 0x00FF) == 0 {
            ((self.d & 0xFF00) as u32) | (ea & 0x00FF)
        } else {
            ea
        }
    }

    /// Absolute: EA = DBR:abs
    fn addr_abs(&self, abs: u16) -> u32 {
        (self.dbr as u32) << 16 | abs as u32
    }

    /// Absolute Indexed X: EA = (DBR:abs + X) & 0xFF_FFFF
    fn addr_abs_x(&mut self, abs: u16) -> u32 {
        let ea = ((self.dbr as u32) << 16 | abs as u32).wrapping_add(self.x as u32) & 0xFF_FFFF;
        self.last_page_crossed =
            !self.x_flag() || (abs & 0xFF00) != (abs.wrapping_add(self.x) & 0xFF00);
        ea
    }

    /// Absolute Indexed Y: EA = (DBR:abs + Y) & 0xFF_FFFF
    fn addr_abs_y(&mut self, abs: u16) -> u32 {
        let ea = ((self.dbr as u32) << 16 | abs as u32).wrapping_add(self.y as u32) & 0xFF_FFFF;
        self.last_page_crossed =
            !self.x_flag() || (abs & 0xFF00) != (abs.wrapping_add(self.y) & 0xFF00);
        ea
    }

    /// Absolute Long: EA = 24-bit operand (pass-through, masked to 24 bits)
    fn addr_abs_long(&self, addr: u32) -> u32 {
        addr & 0xFF_FFFF
    }

    /// Absolute Long Indexed X: EA = (24-bit operand + X) & 0xFF_FFFF
    fn addr_abs_long_x(&self, addr: u32) -> u32 {
        addr.wrapping_add(self.x as u32) & 0xFF_FFFF
    }

    /// Stack Relative: EA = (S + offset) & 0xFFFF  [bank 0]
    fn addr_sr(&self, offset: u8) -> u32 {
        (self.s as u32 + offset as u32) & 0xFFFF
    }

    /// Direct Page Indirect: pointer at (D+offset), EA = DBR:ptr16
    fn addr_dp_ind(&mut self, offset: u8) -> u32 {
        if self.d & 0xFF != 0 {
            self.extra_cycles += 1;
        }
        let ptr_addr = if self.e && (self.d & 0x00FF) == 0 {
            ((self.d & 0xFF00) | offset as u16) as u32
        } else {
            (self.d as u32 + offset as u32) & 0xFFFF
        };
        let lo = self.tick_read(ptr_addr);
        let hi_addr = if self.e && (self.d & 0x00FF) == 0 {
            ((self.d & 0xFF00) | (((ptr_addr as u16) + 1) & 0x00FF)) as u32
        } else {
            (ptr_addr + 1) & 0xFFFF
        };
        let hi = self.tick_read(hi_addr);
        let ptr = lo as u32 | (hi as u32) << 8;
        (self.dbr as u32) << 16 | ptr
    }

    /// Direct Page Indirect Long: 24-bit pointer at (D+offset)
    fn addr_dp_ind_long(&mut self, offset: u8) -> u32 {
        if self.d & 0xFF != 0 {
            self.extra_cycles += 1;
        }
        let ptr_addr = (self.d as u32 + offset as u32) & 0xFFFF;
        let lo = self.tick_read(ptr_addr);
        let mid_addr = (ptr_addr + 1) & 0xFFFF;
        let hi_addr = (ptr_addr + 2) & 0xFFFF;
        let mid = self.tick_read(mid_addr);
        let hi = self.tick_read(hi_addr);
        lo as u32 | (mid as u32) << 8 | (hi as u32) << 16
    }

    /// Direct Page Indexed Indirect X: pointer at (D+offset+X). In emulation
    /// mode the low-byte-of-D-is-zero case additionally wraps `offset+X` to
    /// 8 bits (6502-style zero-page indexing), and — regardless of D's low
    /// byte — the pointer's high-byte read always wraps within its own page
    /// instead of carrying into the next page. Cross-verified against the
    /// vendored snes-tests cputest ROM (test 0024) and Mesen2.
    fn addr_dp_x_ind(&mut self, offset: u8) -> u32 {
        if self.d & 0xFF != 0 {
            self.extra_cycles += 1;
        }
        let wrap_low_byte = self.e && (self.d & 0x00FF) == 0;
        let ptr_addr = if wrap_low_byte {
            let dp_index = (offset as u16).wrapping_add(self.x) & 0x00FF;
            (self.d & 0xFF00).wrapping_add(dp_index) as u32
        } else {
            self.d.wrapping_add(offset as u16).wrapping_add(self.x) as u32
        };

        let lo = self.tick_read(ptr_addr & 0xFFFF);
        let hi_addr = if self.e {
            (ptr_addr & 0xFF00) | ((ptr_addr + 1) & 0x00FF)
        } else {
            (ptr_addr + 1) & 0xFFFF
        };
        let hi = self.tick_read(hi_addr);
        let ptr = lo as u32 | (hi as u32) << 8;
        (self.dbr as u32) << 16 | ptr
    }

    /// Direct Page Indirect Indexed Y: ptr16 at (D+offset), EA = (DBR:ptr16+Y) & 0xFF_FFFF
    fn addr_dp_ind_y(&mut self, offset: u8) -> u32 {
        if self.d & 0xFF != 0 {
            self.extra_cycles += 1;
        }
        let ptr_addr = if self.e && (self.d & 0x00FF) == 0 {
            ((self.d & 0xFF00) | offset as u16) as u32
        } else {
            (self.d as u32 + offset as u32) & 0xFFFF
        };
        let lo = self.tick_read(ptr_addr);
        let hi_addr = if self.e && (self.d & 0x00FF) == 0 {
            ((self.d & 0xFF00) | (((ptr_addr as u16) + 1) & 0x00FF)) as u32
        } else {
            (ptr_addr + 1) & 0xFFFF
        };
        let hi = self.tick_read(hi_addr);
        let ptr16 = lo as u16 | (hi as u16) << 8;
        self.last_page_crossed =
            !self.x_flag() || (ptr16 & 0xFF00) != (ptr16.wrapping_add(self.y) & 0xFF00);
        ((self.dbr as u32) << 16 | ptr16 as u32).wrapping_add(self.y as u32) & 0xFF_FFFF
    }

    /// Direct Page Indirect Long Indexed Y: 24-bit ptr at (D+offset), EA = (ptr+Y) & 0xFF_FFFF
    ///
    /// Cross-verified against the vendored snes-tests cputest ROM (test
    /// 0042) and Mesen2: unlike the 6502-heritage 16-bit-pointer indirect
    /// modes, this 65816-only long-indirect pointer read never wraps within
    /// D's page, even in emulation mode with a zero D low byte.
    fn addr_dp_ind_long_y(&mut self, offset: u8) -> u32 {
        if self.d & 0xFF != 0 {
            self.extra_cycles += 1;
        }
        let ptr_addr = (self.d as u32 + offset as u32) & 0xFFFF;
        let lo = self.tick_read(ptr_addr);
        let mid_addr = (ptr_addr + 1) & 0xFFFF;
        let hi_addr = (ptr_addr + 2) & 0xFFFF;
        let mid = self.tick_read(mid_addr);
        let hi = self.tick_read(hi_addr);
        let base = lo as u32 | (mid as u32) << 8 | (hi as u32) << 16;
        base.wrapping_add(self.y as u32) & 0xFF_FFFF
    }

    /// Stack Relative Indirect Indexed Y: ptr16 at (S+offset), EA = (DBR:ptr16+Y) & 0xFF_FFFF
    fn addr_sr_ind_y(&mut self, offset: u8) -> u32 {
        let ptr_addr = (self.s as u32 + offset as u32) & 0xFFFF;
        let lo = self.tick_read(ptr_addr);
        let hi = self.tick_read((ptr_addr + 1) & 0xFFFF);
        let ptr = lo as u32 | (hi as u32) << 8;
        ((self.dbr as u32) << 16 | ptr).wrapping_add(self.y as u32) & 0xFF_FFFF
    }

    // -------------------------------------------------------------------------
    // Width-aware memory access helpers
    // -------------------------------------------------------------------------

    /// Read one byte from the bus, ticking the master clock per access speed.
    fn read8(&mut self, addr: u32) -> u8 {
        self.tick_read(addr & 0xFF_FFFF)
    }

    /// Write one byte to the bus, ticking the master clock per access speed.
    fn write8(&mut self, addr: u32, value: u8) {
        self.tick_write(addr & 0xFF_FFFF, value);
    }

    fn tick_internal_cycle(&mut self) {
        self.resolve_nmi_arm_counter();
        self.bus.gpdma_cycle_hook();
        trace_cpu!(2; "      internal");
        for _ in 0..6u8 {
            self.bus.tick();
        }
        self.end_of_cycle_interrupt_poll();
    }

    /// Ticks one internal cycle that hardware spends BEFORE a subsequent bus
    /// access within the same opcode/dispatch sequence (e.g. PHA/PHX/PHY's
    /// pre-push idle, or a hardware interrupt's pre-push idle) -- as opposed
    /// to the generic leftover-internal-cycle tick the caller normally adds
    /// AFTER an opcode function returns. Bumping `memory_bus_cycles` here
    /// prevents that generic leftover computation from also re-ticking this
    /// already-ticked cycle (double counting it). See `op_pha`/`op_phx`/
    /// `op_phy`/`dispatch_hw_interrupt` for the Mesen2 references (#3049).
    fn tick_pre_access_internal_cycle(&mut self) {
        self.tick_internal_cycle();
        self.memory_bus_cycles += 1;
    }

    /// Advances the NMI edge-to-dispatch latch by one CPU cycle. Called once
    /// from each of the three CPU-cycle-boundary functions ([`Self::tick_read`],
    /// [`Self::tick_write`], [`Self::tick_internal_cycle`]) -- i.e. exactly once
    /// per real CPU cycle. Split into two halves, called at the start and end
    /// of each of the three CPU-cycle-boundary functions, mirroring Mesen2's
    /// `DetectNmiSignalEdge` (`ProcessCpuCycle`, called from every
    /// `Read`/`Write`/`Idle` *before* that call's own clock-advance) plus the
    /// PPU's eager, mid-clock-advance `SetNmiFlag` push:
    ///
    /// - [`Self::resolve_nmi_arm_counter`] runs BEFORE this cycle's own
    ///   master-clock ticking, resolving whatever was armed by the
    ///   *previous* cycle's poll.
    /// - [`Self::poll_and_arm_nmi_edge`] runs AFTER this cycle's own
    ///   master-clock ticking, discovering an edge that rose *during* this
    ///   cycle's own ticking and arming it for the next cycle to resolve.
    ///
    /// Splitting these matters: a single combined poll-then-resolve call
    /// (both running before the clock advances) discovers an edge one whole
    /// cycle later than this -- the edge can't be seen until the *following*
    /// cycle's own pre-advance poll, adding an extra cycle of latency beyond
    /// the intended one and reproducing the exact one-instruction overshoot
    /// this fix targets (proven via a Mesen2 bus-trace diff on KungFuFurby's
    /// nmi.smc, #3049: Mesen2 pushes the not-yet-executed branch opcode's own
    /// address as the interrupted PC, meaning it recognizes NMI before that
    /// instruction starts at all).
    fn resolve_nmi_arm_counter(&mut self) {
        if self.nmi_arm_counter > 0 {
            self.nmi_arm_counter -= 1;
            if self.nmi_arm_counter == 0 {
                self.nmi_pending = true;
            }
        }
    }

    fn poll_and_arm_nmi_edge(&mut self) {
        if self.bus.poll_nmi() {
            self.nmi_arm_counter = 1;
        }
    }

    /// Resamples the H/V-IRQ line into `irq_line_shadow`, once per CPU cycle
    /// (mirrors Mesen2's `PrevIrqSource` update inside `DetectNmiSignalEdge`).
    /// Unlike NMI's edge, `bus.poll_irq()` is a live, non-consuming level, so
    /// there's no arm/latch counter to split across cycle start/end -- but
    /// the single resample must still run AFTER this cycle's own
    /// master-clock ticking (mirroring [`Self::poll_and_arm_nmi_edge`]'s
    /// placement), not before. Placing it before observes the line as of the
    /// end of the cycle *before* the previous one -- one whole cycle (6
    /// master clocks) stale -- which delays WAI's wake by one extra idle
    /// iteration whenever the wait loop is the one polling. Caught via a
    /// bus-trace diff against a pre-fix build on undisbeliever's
    /// `inidisp_forgot_to_force_blank.sfc` (#3049): the stale wake corrupted
    /// a DMA-timed palette/tile setup that follows, leaving the screen
    /// permanently unpainted. See
    /// `interrupt_dispatch_tests::irq_level_during_wai_wakes_as_soon_as_the_preceding_cycle_observes_it`.
    fn resample_irq_line(&mut self) {
        self.irq_line_shadow = self.bus.poll_irq();
    }

    /// End-of-CPU-cycle interrupt sampling for a non-opcode cycle (operand read,
    /// internal cycle, or write). A GPDMA running this cycle opens a fresh
    /// suppression window (#3065).
    fn end_of_cycle_interrupt_poll(&mut self) {
        let dma_ran = self.bus.take_dma_ran_this_cpu_cycle();
        self.sample_interrupts_end_of_cycle(dma_ran);
    }

    /// Samples the NMI edge and IRQ level at the end of a CPU cycle, honouring
    /// the GPDMA suppression window (#3065). `open_suppression` starts a fresh
    /// 2-cycle window (a DMA ran this cycle and splits the current instruction).
    /// The NMI edge is un-suppressed one cycle before the IRQ level, because its
    /// arm-then-resolve latch adds a cycle of latency -- so both dispatch at the
    /// same instruction boundary coming out of a DMA (proven by the Sour
    /// dma_irq_test's IRQ vs NMI 16-bit-load sub-cases #3/#9).
    fn sample_interrupts_end_of_cycle(&mut self, open_suppression: bool) {
        if open_suppression {
            self.dma_suppress_cycles = 2;
        }
        let remaining = self.dma_suppress_cycles;
        if remaining > 0 {
            self.dma_suppress_cycles = remaining - 1;
        }
        if remaining < 2 {
            self.poll_and_arm_nmi_edge();
        }
        if remaining == 0 {
            self.resample_irq_line();
        }
    }

    /// Read two bytes little-endian using linear 24-bit addressing.
    fn read16(&mut self, addr: u32) -> u16 {
        let lo_addr = addr & 0xFF_FFFF;
        let hi_addr = lo_addr.wrapping_add(1) & 0xFF_FFFF;
        let lo = self.tick_read(lo_addr);
        let hi = self.tick_read(hi_addr);
        lo as u16 | (hi as u16) << 8
    }

    /// Write two bytes little-endian using linear 24-bit addressing.
    fn write16(&mut self, addr: u32, value: u16) {
        let lo_addr = addr & 0xFF_FFFF;
        let hi_addr = lo_addr.wrapping_add(1) & 0xFF_FFFF;
        self.tick_write(lo_addr, value as u8);
        self.tick_write(hi_addr, (value >> 8) as u8);
    }

    fn format_exec_trace_line(&self, pc: u32, bytes: &[u8; 4]) -> String {
        let disassembly = self.format_exec_disassembly(pc, bytes);
        format!(
            "exec PC={:02X}:{:04X} {:<18} A={:04X} X={:04X} Y={:04X} D={:04X} DBR={:02X} S={:04X} P={:02X} E={}",
            self.pbr,
            pc as u16,
            disassembly,
            self.a,
            self.x,
            self.y,
            self.d,
            self.dbr,
            self.s,
            self.p,
            self.e as u8,
        )
    }

    fn format_exec_disassembly(&self, pc: u32, bytes: &[u8; 4]) -> String {
        let opcode = bytes[0];
        let operand8 = bytes[1];
        let operand16 = u16::from_le_bytes([bytes[1], bytes[2]]);
        let operand24 = u32::from_le_bytes([bytes[1], bytes[2], bytes[3], 0]);

        match opcode {
            0x00 => "BRK".to_string(),
            0x08 => "PHP".to_string(),
            0x10 => format!("BPL ${:04X}", Self::branch_target(pc, operand8)),
            0x18 => "CLC".to_string(),
            0x20 => format!("JSR ${:04X}", operand16),
            0x22 => format!("JSL ${:06X}", operand24),
            0x28 => "PLP".to_string(),
            0x38 => "SEC".to_string(),
            0x40 => "RTI".to_string(),
            0x48 => "PHA".to_string(),
            0x4C => format!("JMP ${:04X}", operand16),
            0x5C => format!("JMP ${:06X}", operand24),
            0x60 => "RTS".to_string(),
            0x68 => "PLA".to_string(),
            0x69 => {
                if self.m_flag() {
                    format!("ADC #${:02X}", operand8)
                } else {
                    format!("ADC #${:04X}", operand16)
                }
            }
            0x6B => "RTL".to_string(),
            0x7A => "PLY".to_string(),
            0x8D => format!("STA ${:04X}", operand16),
            0x8F => format!("STA ${:06X}", operand24),
            0x90 => format!("BCC ${:04X}", Self::branch_target(pc, operand8)),
            0xA0 => {
                if self.x_flag() {
                    format!("LDY #${:02X}", operand8)
                } else {
                    format!("LDY #${:04X}", operand16)
                }
            }
            0xA2 => {
                if self.x_flag() {
                    format!("LDX #${:02X}", operand8)
                } else {
                    format!("LDX #${:04X}", operand16)
                }
            }
            0xA9 => {
                if self.m_flag() {
                    format!("LDA #${:02X}", operand8)
                } else {
                    format!("LDA #${:04X}", operand16)
                }
            }
            0xAD => format!("LDA ${:04X}", operand16),
            0xAF => format!("LDA ${:06X}", operand24),
            0xB0 => format!("BCS ${:04X}", Self::branch_target(pc, operand8)),
            0xC2 => format!("REP #${:02X}", operand8),
            0xD0 => format!("BNE ${:04X}", Self::branch_target(pc, operand8)),
            0xE2 => format!("SEP #${:02X}", operand8),
            0xEA => "NOP".to_string(),
            0xF0 => format!("BEQ ${:04X}", Self::branch_target(pc, operand8)),
            0xFB => "XCE".to_string(),
            _ => format!("OP{:02X}", opcode),
        }
    }

    fn branch_target(pc: u32, offset: u8) -> u16 {
        let displacement = (offset as i8) as i16 as u16;
        let pc = pc as u16;
        pc.wrapping_add(2).wrapping_add(displacement)
    }

    /// Read M-flag width: 8-bit when M=1, 16-bit when M=0.
    /// Adds +1 to extra_cycles when M=0 (16-bit mode requires an extra byte fetch).
    fn read_m(&mut self, addr: u32) -> u16 {
        if self.m_flag() {
            self.read8(addr) as u16
        } else {
            self.extra_cycles += 1;
            self.read16(addr)
        }
    }

    /// Write M-flag width: 8-bit when M=1, 16-bit when M=0.
    /// Adds +1 to extra_cycles when M=0 (16-bit mode requires an extra byte write).
    fn write_m(&mut self, addr: u32, value: u16) {
        if self.m_flag() {
            self.write8(addr, value as u8);
        } else {
            self.extra_cycles += 1;
            self.write16(addr, value);
        }
    }

    /// Read X-flag width: 8-bit when X=1, 16-bit when X=0.
    /// Adds +1 to extra_cycles when X=0 (16-bit index requires an extra byte fetch).
    fn read_idx(&mut self, addr: u32) -> u16 {
        if self.x_flag() {
            self.read8(addr) as u16
        } else {
            self.extra_cycles += 1;
            if addr <= 0xFFFF {
                let lo = self.tick_read(addr & 0xFFFF);
                let hi = self.tick_read((addr.wrapping_add(1)) & 0xFFFF);
                lo as u16 | (hi as u16) << 8
            } else {
                self.read16(addr)
            }
        }
    }

    /// Write X-flag width: 8-bit when X=1, 16-bit when X=0.
    /// Adds +1 to extra_cycles when X=0 (16-bit index requires an extra byte write).
    fn write_idx(&mut self, addr: u32, value: u16) {
        if self.x_flag() {
            self.write8(addr, value as u8);
        } else {
            self.extra_cycles += 1;
            if addr <= 0xFFFF {
                self.tick_write(addr & 0xFFFF, value as u8);
                self.tick_write((addr.wrapping_add(1)) & 0xFFFF, (value >> 8) as u8);
            } else {
                self.write16(addr, value);
            }
        }
    }

    // -------------------------------------------------------------------------
    // Stack helpers.
    // PUSH: write to S, then decrement S.
    // PULL: increment S, then read from S.
    // In emulation mode, 8-bit stack operations stay in page 1. 16-bit stack
    // operations use consecutive addresses based on the current S value and then
    // normalize the architectural S back to page 1.
    // -------------------------------------------------------------------------

    fn push8(&mut self, val: u8) {
        self.write8(self.s as u32, val);
        self.s = if self.e {
            0x0100 | (self.s.wrapping_sub(1) & 0xFF)
        } else {
            self.s.wrapping_sub(1)
        };
    }

    fn push16(&mut self, val: u16) {
        if self.e {
            self.write8(self.s as u32, (val >> 8) as u8);
            self.write8(self.s.wrapping_sub(1) as u32, val as u8);
            self.s = 0x0100 | (self.s.wrapping_sub(2) & 0x00FF);
        } else {
            self.push8((val >> 8) as u8);
            self.push8(val as u8);
        }
    }

    fn push16_bytes(&mut self, val: u16) {
        self.push8((val >> 8) as u8);
        self.push8(val as u8);
    }

    fn pull8(&mut self) -> u8 {
        self.s = if self.e {
            0x0100 | (self.s.wrapping_add(1) & 0xFF)
        } else {
            self.s.wrapping_add(1)
        };
        self.read8(self.s as u32)
    }

    fn pull16(&mut self) -> u16 {
        if self.e {
            let s1 = self.s.wrapping_add(1);
            let lo = self.read8(s1 as u32) as u16;
            let s2 = s1.wrapping_add(1);
            let hi = self.read8(s2 as u32) as u16;
            self.s = 0x0100 | (s2 & 0x00FF);
            hi << 8 | lo
        } else {
            let lo = self.pull8() as u16;
            let hi = self.pull8() as u16;
            hi << 8 | lo
        }
    }

    fn pull16_bytes(&mut self) -> u16 {
        let lo = self.pull8() as u16;
        let hi = self.pull8() as u16;
        hi << 8 | lo
    }

    fn push8_linear_e(&mut self, val: u8) {
        self.write8(self.s as u32, val);
        self.s = self.s.wrapping_sub(1);
    }

    // -------------------------------------------------------------------------
    // JMP — jump (no stack change)
    // -------------------------------------------------------------------------

    fn op_jmp_abs(&mut self) -> u8 {
        let addr = self.fetch_word();
        self.pc = addr;
        3
    }

    fn op_jmp_abs_ind(&mut self) -> u8 {
        let ptr_addr = self.fetch_word() as u32; // bank 0
        let lo = self.tick_read(ptr_addr);
        let hi = self.tick_read((ptr_addr + 1) & 0xFFFF);
        self.pc = lo as u16 | (hi as u16) << 8;
        5
    }

    fn op_jmp_abs_x_ind(&mut self) -> u8 {
        let base = self.fetch_word();
        let ptr = base.wrapping_add(self.x);
        let bank_base = (self.pbr as u32) << 16;
        let lo = self.tick_read(bank_base | ptr as u32);
        let hi = self.tick_read(bank_base | ptr.wrapping_add(1) as u32);
        self.pc = lo as u16 | (hi as u16) << 8;
        6
    }

    fn op_jmp_abs_long(&mut self) -> u8 {
        let addr = self.fetch_addr24();
        self.pbr = (addr >> 16) as u8;
        self.pc = addr as u16;
        4
    }

    fn op_jmp_abs_ind_long(&mut self) -> u8 {
        let ptr_addr = self.fetch_word() as u32; // bank 0
        let lo = self.tick_read(ptr_addr);
        let mid = self.tick_read((ptr_addr + 1) & 0xFFFF);
        let hi = self.tick_read((ptr_addr + 2) & 0xFFFF);
        self.pc = lo as u16 | (mid as u16) << 8;
        self.pbr = hi;
        6
    }

    // -------------------------------------------------------------------------
    // JSR / JSL — jump to subroutine (push return address - 1)
    // -------------------------------------------------------------------------

    fn op_jsr_abs(&mut self) -> u8 {
        let target = self.fetch_word();
        let ret = self.pc.wrapping_sub(1);
        self.push16_bytes(ret);
        self.pc = target;
        6
    }

    fn op_jsr_abs_x_ind(&mut self) -> u8 {
        let base = self.fetch_word();
        let ret = self.pc.wrapping_sub(1);
        // Unlike plain JSR absolute, this form's return-address push uses a
        // natural (unclamped) intermediate stack address that may cross out
        // of page 1 in emulation mode, clamping only the final S. Verified
        // against the vendored snes-tests cputest ROM (test 0277) and
        // Mesen2.
        self.push16(ret);
        let ptr = base.wrapping_add(self.x);
        let bank_base = (self.pbr as u32) << 16;
        let lo = self.tick_read(bank_base | ptr as u32);
        let hi = self.tick_read(bank_base | ptr.wrapping_add(1) as u32);
        self.pc = lo as u16 | (hi as u16) << 8;
        8
    }

    fn op_jsl_abs_long(&mut self) -> u8 {
        let addr = self.fetch_addr24();
        let ret = self.pc.wrapping_sub(1);
        if self.e {
            self.push8_linear_e(self.pbr);
            self.push8_linear_e((ret >> 8) as u8);
            self.push8_linear_e(ret as u8);
            self.s = 0x0100 | (self.s & 0x00FF);
        } else {
            self.push8(self.pbr);
            self.push16_bytes(ret);
        }
        self.pbr = (addr >> 16) as u8;
        self.pc = addr as u16;
        8
    }

    // -------------------------------------------------------------------------
    // RTS / RTL / RTI — return from subroutine / interrupt
    // -------------------------------------------------------------------------

    fn op_rts(&mut self) -> u8 {
        let addr = self.pull16_bytes();
        self.pc = addr.wrapping_add(1);
        6
    }

    fn op_rtl(&mut self) -> u8 {
        let (addr, bank) = if self.e {
            let s1 = self.s.wrapping_add(1);
            let lo = self.read8(s1 as u32) as u16;
            let s2 = s1.wrapping_add(1);
            let hi = self.read8(s2 as u32) as u16;
            let s3 = s2.wrapping_add(1);
            let bank = self.read8(s3 as u32);
            self.s = 0x0100 | (s3 & 0x00FF);
            (hi << 8 | lo, bank)
        } else {
            (self.pull16_bytes(), self.pull8())
        };
        self.pc = addr.wrapping_add(1);
        self.pbr = bank;
        6
    }

    fn op_rti(&mut self) -> u8 {
        let old_x = self.x_flag();
        let p = self.pull8();
        self.p = p;
        if self.e {
            self.p |= FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
        }
        if !old_x && self.x_flag() {
            self.x &= 0x00FF;
            self.y &= 0x00FF;
        }
        let pc = self.pull16_bytes();
        self.pc = pc;
        if !self.e {
            self.pbr = self.pull8();
        }
        6 + (!self.e) as u8
    }

    fn op_brk(&mut self) -> u8 {
        let _sig = self.fetch_byte(); // consume signature byte (BRK is 2-byte instruction)
        let pc_ret = self.pc;
        if self.e {
            // Emulation mode: push PC+2, push P with B flag set, set I=1, clear D, vector $FFFE
            self.push8((pc_ret >> 8) as u8);
            self.push8(pc_ret as u8);
            self.push8(self.p | FLAG_INDEX_WIDTH); // B flag = bit 4 in emulation mode
            self.set_flag_i(true);
            self.set_flag_d(false);
            let lo = self.read8(0x00FFFE);
            let hi = self.read8(0x00FFFF);
            self.pbr = 0x00;
            self.pc = lo as u16 | (hi as u16) << 8;
            7
        } else {
            // Native mode: push PBR, push PC+2, push P, set I=1, clear D, vector $FFE6
            self.push8(self.pbr);
            self.push8((pc_ret >> 8) as u8);
            self.push8(pc_ret as u8);
            self.push8(self.p);
            self.set_flag_i(true);
            self.set_flag_d(false);
            let lo = self.read8(0x00FFE6);
            let hi = self.read8(0x00FFE7);
            self.pbr = 0x00;
            self.pc = lo as u16 | (hi as u16) << 8;
            8
        }
    }

    fn op_cop(&mut self) -> u8 {
        let _sig = self.fetch_byte(); // consume signature byte
        let pc_ret = self.pc;
        if self.e {
            // Emulation mode: push PC+2, push P, set I=1, clear D, vector $FFF4
            self.push8((pc_ret >> 8) as u8);
            self.push8(pc_ret as u8);
            self.push8(self.p);
            self.set_flag_i(true);
            self.set_flag_d(false);
            let lo = self.read8(0x00FFF4);
            let hi = self.read8(0x00FFF5);
            self.pbr = 0x00;
            self.pc = lo as u16 | (hi as u16) << 8;
            7
        } else {
            // Native mode: push PBR, push PC+2, push P, set I=1, clear D, vector $FFE4
            self.push8(self.pbr);
            self.push8((pc_ret >> 8) as u8);
            self.push8(pc_ret as u8);
            self.push8(self.p);
            self.set_flag_i(true);
            self.set_flag_d(false);
            let lo = self.read8(0x00FFE4);
            let hi = self.read8(0x00FFE5);
            self.pbr = 0x00;
            self.pc = lo as u16 | (hi as u16) << 8;
            8
        }
    }

    // -------------------------------------------------------------------------
    // Hardware interrupt dispatch — NMI, IRQ, ABORT
    // -------------------------------------------------------------------------

    /// Shared hardware interrupt dispatch sequence.
    ///
    /// Native mode (E=0): push PBR, PCH, PCL, P; set PBR=0, I=1, D=0; load `native_vector`.  8 cycles.
    /// Emulation mode (E=1): push PCH, PCL, P (B=0); set PBR=0, I=1, D=0; load `emu_vector`.  7 cycles.
    fn dispatch_hw_interrupt(&mut self, native_vector: u32, emu_vector: u32) -> u8 {
        let pc = self.pc;
        // Hardware interrupts (NMI/IRQ/ABORT) waste their first two cycles
        // before pushing the return state: a dummy re-read of the interrupted
        // PC's own opcode (result discarded), then one internal/idle cycle
        // (Mesen2 `ProcessInterrupt`: "IRQ/NMI waste 2 cycles here ... BRK/COP
        // do not, because they do those 2 cycles while loading the OP code +
        // signature byte"). Both must happen HERE, before the pushes -- not
        // via the caller's generic leftover-internal-cycle tick added after
        // this function returns, which would tick them too late and desync
        // every subsequent bus timestamp from Mesen2's (proven via a bus-trace
        // diff on KungFuFurby's nmi.smc, #3049).
        self.read8(((self.pbr as u32) << 16) | pc as u32);
        self.tick_pre_access_internal_cycle();
        if self.e {
            // Emulation mode: 3 pushes, no PBR push, B flag cleared; PBR forced to bank 0
            self.push8((pc >> 8) as u8);
            self.push8(pc as u8);
            self.push8(self.p & !FLAG_INDEX_WIDTH); // B=0 for hardware interrupts
            self.pbr = 0x00;
            self.set_flag_i(true);
            self.irq_i_shadow = true;
            self.set_flag_d(false);
            let lo = self.read8(emu_vector) as u16;
            let hi = self.read8(emu_vector + 1) as u16;
            self.pc = lo | hi << 8;
            7
        } else {
            // Native mode: push PBR then PC then P, clear PBR
            self.push8(self.pbr);
            self.push8((pc >> 8) as u8);
            self.push8(pc as u8);
            self.push8(self.p);
            self.pbr = 0x00;
            self.set_flag_i(true);
            self.irq_i_shadow = true;
            self.set_flag_d(false);
            let lo = self.read8(native_vector) as u16;
            let hi = self.read8(native_vector + 1) as u16;
            self.pc = lo | hi << 8;
            8
        }
    }

    fn dispatch_nmi(&mut self) -> u8 {
        let cycles = self.dispatch_hw_interrupt(0x00FFEA, 0x00FFFA);
        if crate::platform::debugging::cpu_trace_level() >= 1 {
            trace_cpu!(1; "NMI -> PC={:02X}:{:04X}", self.pbr, self.pc);
        }
        cycles
    }

    fn dispatch_irq(&mut self) -> u8 {
        let cycles = self.dispatch_hw_interrupt(0x00FFEE, 0x00FFFE);
        if crate::platform::debugging::cpu_trace_level() >= 1 {
            trace_cpu!(1; "IRQ -> PC={:02X}:{:04X}", self.pbr, self.pc);
        }
        cycles
    }

    fn dispatch_abort(&mut self) -> u8 {
        let cycles = self.dispatch_hw_interrupt(0x00FFE8, 0x00FFF8);
        if crate::platform::debugging::cpu_trace_level() >= 1 {
            trace_cpu!(1; "ABORT -> PC={:02X}:{:04X}", self.pbr, self.pc);
        }
        cycles
    }

    // -------------------------------------------------------------------------
    // Stack push / pull opcodes
    // -------------------------------------------------------------------------

    fn op_pha(&mut self) -> u8 {
        // Mesen2 PHA: Idle() before PushRegister -- the internal cycle ticks
        // before the push, not after (#3049).
        self.tick_pre_access_internal_cycle();
        if self.m_flag() {
            self.push8(self.a as u8);
        } else {
            self.push16(self.a);
        }
        3 + !self.m_flag() as u8
    }

    fn op_pla(&mut self) -> u8 {
        if self.m_flag() {
            let val = self.pull8() as u16;
            self.write_a(val);
            let a = self.a;
            self.set_nz_m(a);
        } else {
            let val = self.pull16();
            self.write_a(val);
            let a = self.a;
            self.set_nz_m(a);
        }
        4 + !self.m_flag() as u8
    }

    fn op_phx(&mut self) -> u8 {
        // See op_pha: Mesen2 PHX ticks its internal cycle before the push (#3049).
        self.tick_pre_access_internal_cycle();
        if self.x_flag() {
            self.push8(self.x as u8);
        } else {
            self.push16(self.x);
        }
        3 + !self.x_flag() as u8
    }

    fn op_plx(&mut self) -> u8 {
        if self.x_flag() {
            let val = self.pull8() as u16;
            self.write_x(val);
        } else {
            let val = self.pull16();
            self.write_x(val);
        }
        self.set_nz_x(self.x);
        4 + !self.x_flag() as u8
    }

    fn op_phy(&mut self) -> u8 {
        // See op_pha: Mesen2 PHY ticks its internal cycle before the push (#3049).
        self.tick_pre_access_internal_cycle();
        if self.x_flag() {
            self.push8(self.y as u8);
        } else {
            self.push16(self.y);
        }
        3 + !self.x_flag() as u8
    }

    fn op_ply(&mut self) -> u8 {
        if self.x_flag() {
            let val = self.pull8() as u16;
            self.write_y(val);
        } else {
            let val = self.pull16();
            self.write_y(val);
        }
        self.set_nz_x(self.y);
        4 + !self.x_flag() as u8
    }

    fn op_php(&mut self) -> u8 {
        self.push8(self.p);
        3
    }

    fn op_plp(&mut self) -> u8 {
        let old_x = self.x_flag();
        let p = self.pull8();
        self.p = p;
        if self.e {
            self.p |= FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
        }
        if !old_x && self.x_flag() {
            self.x &= 0x00FF;
            self.y &= 0x00FF;
        }
        4
    }

    fn op_phb(&mut self) -> u8 {
        self.push8(self.dbr);
        3
    }

    fn op_plb(&mut self) -> u8 {
        // Unlike the 6502-heritage single-byte pulls (PLA/PLX/PLY/PLP), PLB
        // is 65816-exclusive: it reads from the natural (unclamped)
        // incremented address in emulation mode, clamping only the final S
        // afterward. Cross-verified against the vendored snes-tests cputest
        // ROM (test 0x3D9) and Mesen2.
        let val = if self.e {
            let new_s = self.s.wrapping_add(1);
            // read8 delegates to tick_read, the same ticked bus access
            // pull8 itself uses for its read — no cycle-accuracy difference,
            // just a different address (unclamped `new_s` vs pull8's
            // already-clamped self.s).
            let val = self.read8(new_s as u32);
            self.s = 0x0100 | (new_s & 0x00FF);
            val
        } else {
            self.pull8()
        };
        self.dbr = val;
        self.set_nz(val as u16, 0x80);
        4
    }

    fn op_phd(&mut self) -> u8 {
        self.push16(self.d);
        4
    }

    fn op_pld(&mut self) -> u8 {
        let val = self.pull16();
        self.d = val;
        self.set_nz(val, 0x8000);
        5
    }

    fn op_phk(&mut self) -> u8 {
        self.push8(self.pbr);
        3
    }

    fn op_pea(&mut self) -> u8 {
        let val = self.fetch_word();
        self.push16(val);
        5
    }

    fn op_pei(&mut self) -> u8 {
        let off = self.fetch_byte();
        // Unlike the 6502-heritage (dp) indirect opcodes (ADC, AND, CMP,
        // ...), PEI's pointer read never wraps within D's page in emulation
        // mode. Cross-verified against the vendored snes-tests cputest ROM
        // (test 0x3C4) and Mesen2.
        if self.d & 0xFF != 0 {
            self.extra_cycles += 1;
        }
        let ptr_addr = (self.d as u32 + off as u32) & 0xFFFF;
        let lo = self.tick_read(ptr_addr);
        let hi = self.tick_read((ptr_addr + 1) & 0xFFFF);
        let val = lo as u16 | (hi as u16) << 8;
        self.push16(val);
        6
    }

    fn op_per(&mut self) -> u8 {
        let offset = self.fetch_word() as i16;
        let ea = self.pc.wrapping_add(offset as u16);
        self.push16(ea);
        6
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snes::bus::StubBus;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct TraceProbeBus {
        mem: BTreeMap<u32, u8>,
        debugger_reads: RefCell<Vec<u32>>,
    }

    impl TraceProbeBus {
        fn load(&mut self, addr: u32, data: &[u8]) {
            for (offset, byte) in data.iter().enumerate() {
                self.mem.insert((addr + offset as u32) & 0xFF_FFFF, *byte);
            }
        }

        fn debugger_reads(&self) -> Vec<u32> {
            self.debugger_reads.borrow().clone()
        }
    }

    impl SnesBus for TraceProbeBus {
        fn read(&self, addr: u32) -> u8 {
            *self.mem.get(&(addr & 0xFF_FFFF)).unwrap_or(&0)
        }

        fn read_for_debugger(&self, addr: u32) -> u8 {
            let addr = addr & 0xFF_FFFF;
            self.debugger_reads.borrow_mut().push(addr);
            self.read(addr)
        }

        fn write(&mut self, addr: u32, value: u8) {
            self.mem.insert(addr & 0xFF_FFFF, value);
        }

        fn tick(&mut self) {}
    }

    /// Records the interleaving of master-clock ticks and bus data accesses.
    #[derive(Default)]
    struct BusCycleRecordingBus {
        mem: BTreeMap<u32, u8>,
        events: RefCell<Vec<BusCycleEvent>>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum BusCycleEvent {
        Tick,
        Read(u32),
    }

    impl BusCycleRecordingBus {
        fn load(&mut self, addr: u32, data: &[u8]) {
            for (offset, byte) in data.iter().enumerate() {
                self.mem.insert((addr + offset as u32) & 0xFF_FFFF, *byte);
            }
        }
    }

    impl SnesBus for BusCycleRecordingBus {
        fn read(&self, addr: u32) -> u8 {
            self.events
                .borrow_mut()
                .push(BusCycleEvent::Read(addr & 0xFF_FFFF));
            *self.mem.get(&(addr & 0xFF_FFFF)).unwrap_or(&0)
        }

        fn read_for_debugger(&self, addr: u32) -> u8 {
            *self.mem.get(&(addr & 0xFF_FFFF)).unwrap_or(&0)
        }

        fn write(&mut self, addr: u32, value: u8) {
            self.mem.insert(addr & 0xFF_FFFF, value);
        }

        fn tick(&mut self) {
            self.events.borrow_mut().push(BusCycleEvent::Tick);
        }
    }

    // #2914: see `Cpu::tick_read` — read data is sampled 4 master clocks
    // before the end of the bus cycle; writes land at the end.
    #[test]
    fn read_bus_cycle_samples_data_four_clocks_before_cycle_end() {
        let mut bus = BusCycleRecordingBus::default();
        // $00:8000 (WS1 ROM, 8 clocks/access): LDA $2140 ($2140 = 6 clocks).
        bus.load(0x00_8000, &[0xAD, 0x40, 0x21]);
        let mut cpu = Cpu::new(bus);
        cpu.pc = 0x8000;

        cpu.step();

        use BusCycleEvent::{Read, Tick};
        let mut expected = Vec::new();
        for operand in 0u32..3 {
            // 8-clock fetch: 4 ticks, sample, 4 ticks.
            expected.extend([Tick; 4]);
            expected.push(Read(0x00_8000 + operand));
            expected.extend([Tick; 4]);
        }
        // 6-clock data read: 2 ticks, sample, 4 ticks.
        expected.extend([Tick; 2]);
        expected.push(Read(0x00_2140));
        expected.extend([Tick; 4]);
        assert_eq!(cpu.bus.events.borrow().as_slice(), expected.as_slice());
    }

    struct TraceReset;

    impl TraceReset {
        fn cpu_enabled() -> Self {
            crate::platform::debugging::init_tracing(crate::platform::debugging::Tracing {
                enabled: true,
                cpu: 1,
                ..Default::default()
            });
            Self
        }
    }

    impl Drop for TraceReset {
        fn drop(&mut self) {
            crate::platform::debugging::init_tracing(Default::default());
        }
    }

    #[test]
    fn reset_state_is_emulation_mode() {
        let cpu = Cpu::new(StubBus);
        assert!(cpu.emulation_mode());
        assert!(cpu.m_flag());
        assert!(cpu.x_flag());
        assert_eq!(cpu.read_s(), 0x01FF);
        assert!(cpu.flag_i());
    }

    #[test]
    fn step_does_not_fetch_trace_operands_when_cpu_tracing_is_disabled() {
        crate::platform::debugging::init_tracing(Default::default());
        let mut bus = TraceProbeBus::default();
        bus.load(0x12_8000, &[0xEA]);
        let mut cpu = Cpu::new(bus);
        cpu.pbr = 0x12;
        cpu.pc = 0x8000;

        cpu.step();

        assert_eq!(cpu.bus.debugger_reads(), Vec::<u32>::new());
    }

    #[test]
    fn step_trace_operand_reads_wrap_pc_within_current_program_bank() {
        let _trace_reset = TraceReset::cpu_enabled();
        let mut bus = TraceProbeBus::default();
        bus.load(0x12_FFFF, &[0xEA]);
        bus.load(0x12_0000, &[0x11, 0x22, 0x33]);
        let mut cpu = Cpu::new(bus);
        cpu.pbr = 0x12;
        cpu.pc = 0xFFFF;

        cpu.step();

        assert_eq!(
            cpu.bus.debugger_reads(),
            vec![0x12_0000, 0x12_0001, 0x12_0002]
        );
    }

    #[test]
    fn write_a_8bit_preserves_b() {
        let mut cpu = Cpu::new(StubBus);
        // Start in emulation mode (M=1, 8-bit A)
        cpu.a = 0x1234; // Set B:A
        cpu.write_a(0x56); // Write only A
        assert_eq!(cpu.read_a(), 0x1256); // B preserved
    }

    #[test]
    fn write_a_16bit_updates_full() {
        let mut cpu = Cpu::new(StubBus);
        // Switch to native mode with M=0 (16-bit)
        cpu.e = false;
        cpu.p &= !FLAG_ACCUM_WIDTH; // M=0
        cpu.a = 0x1234;
        cpu.write_a(0x5678);
        assert_eq!(cpu.read_a(), 0x5678);
    }

    #[test]
    fn write_x_8bit_clears_high_byte() {
        let mut cpu = Cpu::new(StubBus);
        // Start in emulation mode (X=1, 8-bit X)
        cpu.x = 0x1234;
        cpu.write_x(0xFF56);
        assert_eq!(cpu.read_x(), 0x0056); // High byte forced to 0
    }

    #[test]
    fn write_x_16bit_updates_full() {
        let mut cpu = Cpu::new(StubBus);
        // Switch to native mode with X=0 (16-bit)
        cpu.e = false;
        cpu.p &= !FLAG_INDEX_WIDTH; // X=0
        cpu.write_x(0x5678);
        assert_eq!(cpu.read_x(), 0x5678);
    }

    #[test]
    fn write_y_8bit_clears_high_byte() {
        let mut cpu = Cpu::new(StubBus);
        // Start in emulation mode (X=1, 8-bit Y)
        cpu.y = 0x1234;
        cpu.write_y(0xFF56);
        assert_eq!(cpu.read_y(), 0x0056); // High byte forced to 0
    }

    #[test]
    fn write_y_16bit_updates_full() {
        let mut cpu = Cpu::new(StubBus);
        // Switch to native mode with X=0 (16-bit)
        cpu.e = false;
        cpu.p &= !FLAG_INDEX_WIDTH; // X=0
        cpu.write_y(0x5678);
        assert_eq!(cpu.read_y(), 0x5678);
    }

    #[test]
    fn emulation_mode_forces_stack_high_byte_01() {
        let mut cpu = Cpu::new(StubBus);
        // Emulation mode (E=1)
        cpu.write_s(0x5678);
        assert_eq!(cpu.read_s(), 0x0178); // High byte forced to $01
    }

    #[test]
    fn native_mode_allows_full_16bit_stack() {
        let mut cpu = Cpu::new(StubBus);
        cpu.e = false; // Native mode
        cpu.write_s(0x5678);
        assert_eq!(cpu.read_s(), 0x5678); // Full 16-bit
    }

    #[test]
    fn exec_trace_line_formats_registers_and_pc() {
        let mut cpu = Cpu::new(StubBus);
        cpu.pbr = 0x80;
        cpu.pc = 0x1234;
        cpu.a = 0xABCD;
        cpu.x = 0x1111;
        cpu.y = 0x2222;
        cpu.d = 0x3333;
        cpu.dbr = 0x44;
        cpu.s = 0x5555;
        cpu.p = 0x66;
        cpu.e = false;

        let line = cpu.format_exec_trace_line(0x801234, &[0xEA, 0x00, 0x00, 0x00]);
        assert_eq!(
            line,
            "exec PC=80:1234 NOP                A=ABCD X=1111 Y=2222 D=3333 DBR=44 S=5555 P=66 E=0"
        );
    }

    #[test]
    fn exec_trace_line_formats_immediate_instruction() {
        let mut cpu = Cpu::new(StubBus);
        cpu.pbr = 0x80;
        cpu.pc = 0x1234;
        cpu.e = false;
        cpu.p &= !FLAG_ACCUM_WIDTH;

        let line = cpu.format_exec_trace_line(0x801234, &[0xA9, 0x34, 0x12, 0x00]);
        assert!(line.contains("LDA #$1234"));
    }

    #[test]
    fn flag_accessors_work() {
        let mut cpu = Cpu::new(StubBus);

        cpu.set_flag_c(true);
        assert!(cpu.flag_c());
        cpu.set_flag_c(false);
        assert!(!cpu.flag_c());

        cpu.set_flag_z(true);
        assert!(cpu.flag_z());
        cpu.set_flag_z(false);
        assert!(!cpu.flag_z());

        cpu.set_flag_i(true);
        assert!(cpu.flag_i());
        cpu.set_flag_i(false);
        assert!(!cpu.flag_i());

        cpu.set_flag_d(true);
        assert!(cpu.flag_d());
        cpu.set_flag_d(false);
        assert!(!cpu.flag_d());

        cpu.set_flag_v(true);
        assert!(cpu.flag_v());
        cpu.set_flag_v(false);
        assert!(!cpu.flag_v());

        cpu.set_flag_n(true);
        assert!(cpu.flag_n());
        cpu.set_flag_n(false);
        assert!(!cpu.flag_n());
    }

    #[test]
    fn xce_emulation_to_native() {
        let mut cpu = Cpu::new(StubBus);
        // Start in emulation mode (E=1)
        assert!(cpu.emulation_mode());
        assert!(cpu.m_flag());
        assert!(cpu.x_flag());

        // Set C=0 before XCE (to switch to native mode)
        cpu.set_flag_c(false);

        // Execute XCE: swap E and C
        // Before: E=1, C=0
        // After:  E=0 (takes C's value), C=1 (takes E's value)
        cpu.xce();

        assert!(!cpu.emulation_mode()); // Now in native mode (E=0)
        assert!(cpu.flag_c()); // C now has old E value (1)
        assert!(cpu.m_flag()); // M still 1 (not auto-cleared)
        assert!(cpu.x_flag()); // X still 1 (not auto-cleared)
    }

    #[test]
    fn xce_native_to_emulation() {
        let mut cpu = Cpu::new(StubBus);
        // Start in native mode with M=0, X=0
        cpu.e = false;
        cpu.p &= !FLAG_ACCUM_WIDTH; // M=0
        cpu.p &= !FLAG_INDEX_WIDTH; // X=0
        cpu.s = 0x2345; // Full 16-bit stack
        cpu.x = 0x0335;
        cpu.y = 0x8FB3;
        cpu.set_flag_c(true); // C=1 (to switch to emulation mode)

        // Execute XCE: swap E and C
        // Before: E=0, C=1
        // After:  E=1 (takes C's value), C=0 (takes E's value)
        cpu.xce();

        assert!(cpu.emulation_mode()); // Now in emulation mode (E=1)
        assert!(!cpu.flag_c()); // C now has old E value (0)
        assert!(cpu.m_flag()); // M forced to 1
        assert!(cpu.x_flag()); // X forced to 1
        assert_eq!(cpu.read_s(), 0x0145); // S high byte forced to $01
        assert_eq!(cpu.read_x(), 0x0035);
        assert_eq!(cpu.read_y(), 0x00B3);
    }

    #[test]
    fn xce_preserves_other_flags() {
        let mut cpu = Cpu::new(StubBus);
        cpu.set_flag_n(true);
        cpu.set_flag_v(true);
        cpu.set_flag_d(true);
        cpu.set_flag_i(false);
        cpu.set_flag_z(true);
        cpu.set_flag_c(true);

        cpu.xce();

        // All flags except C should be preserved
        assert!(cpu.flag_n());
        assert!(cpu.flag_v());
        assert!(cpu.flag_d());
        assert!(!cpu.flag_i());
        assert!(cpu.flag_z());
    }

    #[test]
    fn rep_in_native_mode_clears_m_and_x() {
        let mut cpu = Cpu::new(StubBus);
        // Switch to native mode
        cpu.e = false;
        cpu.p |= FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH; // M=1, X=1

        // REP to clear M and X
        cpu.rep(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);

        assert!(!cpu.m_flag());
        assert!(!cpu.x_flag());
    }

    #[test]
    fn rep_in_emulation_mode_cannot_clear_m_and_x() {
        let mut cpu = Cpu::new(StubBus);
        // Emulation mode (E=1, M=1, X=1)
        assert!(cpu.emulation_mode());
        assert!(cpu.m_flag());
        assert!(cpu.x_flag());

        // Try to REP M and X - should have no effect
        cpu.rep(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);

        assert!(cpu.m_flag()); // Still 1
        assert!(cpu.x_flag()); // Still 1
    }

    #[test]
    fn rep_clears_other_flags() {
        let mut cpu = Cpu::new(StubBus);
        cpu.e = false;
        cpu.set_flag_c(true);
        cpu.set_flag_z(true);
        cpu.set_flag_i(true);
        cpu.set_flag_d(true);
        cpu.set_flag_v(true);
        cpu.set_flag_n(true);

        // Clear C, Z, I flags
        cpu.rep(FLAG_CARRY | FLAG_ZERO | FLAG_INTERRUPT);

        assert!(!cpu.flag_c());
        assert!(!cpu.flag_z());
        assert!(!cpu.flag_i());
        // Others preserved
        assert!(cpu.flag_d());
        assert!(cpu.flag_v());
        assert!(cpu.flag_n());
    }

    #[test]
    fn sep_sets_m_and_x() {
        let mut cpu = Cpu::new(StubBus);
        cpu.e = false;
        cpu.p &= !FLAG_ACCUM_WIDTH; // M=0
        cpu.p &= !FLAG_INDEX_WIDTH; // X=0

        // Set 16-bit values
        cpu.write_x(0x1234);
        cpu.write_y(0x5678);

        // SEP to set M and X (switch to 8-bit)
        cpu.sep(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);

        assert!(cpu.m_flag());
        assert!(cpu.x_flag());

        // X and Y high bytes should be forced to 0
        assert_eq!(cpu.read_x(), 0x0034);
        assert_eq!(cpu.read_y(), 0x0078);
    }

    #[test]
    fn sep_m_transition_preserves_b() {
        let mut cpu = Cpu::new(StubBus);
        cpu.e = false;
        cpu.p &= !FLAG_ACCUM_WIDTH; // M=0 (16-bit)

        cpu.write_a(0x1234); // Set full 16-bit value

        // SEP to set M (switch to 8-bit)
        cpu.sep(FLAG_ACCUM_WIDTH);

        assert!(cpu.m_flag());
        assert_eq!(cpu.read_a(), 0x1234); // B preserved (full value readable)
    }

    #[test]
    fn sep_sets_other_flags() {
        let mut cpu = Cpu::new(StubBus);
        cpu.e = false;
        cpu.set_flag_c(false);
        cpu.set_flag_z(false);
        cpu.set_flag_i(false);

        // Set C, Z, I flags
        cpu.sep(FLAG_CARRY | FLAG_ZERO | FLAG_INTERRUPT);

        assert!(cpu.flag_c());
        assert!(cpu.flag_z());
        assert!(cpu.flag_i());
    }

    #[test]
    fn integration_full_mode_switching_cycle() {
        let mut cpu = Cpu::new(StubBus);

        // Start in emulation mode (E=1, M=1, X=1)
        assert!(cpu.emulation_mode());
        assert!(cpu.m_flag());
        assert!(cpu.x_flag());
        assert_eq!(cpu.read_s(), 0x01FF);

        // Set A/X/Y with 8-bit values (B:A = 0x00:42, X = 0x00:34, Y = 0x00:56)
        cpu.write_a(0x42);
        cpu.write_x(0x34);
        cpu.write_y(0x56);
        assert_eq!(cpu.read_a(), 0x0042);
        assert_eq!(cpu.read_x(), 0x0034);
        assert_eq!(cpu.read_y(), 0x0056);

        // Switch to native mode via XCE (C=0, E=1 → C=1, E=0)
        cpu.set_flag_c(false);
        cpu.xce();
        assert!(!cpu.emulation_mode());
        assert!(cpu.flag_c()); // Got old E=1
        assert!(cpu.m_flag()); // Still 1 (not auto-cleared)
        assert!(cpu.x_flag()); // Still 1 (not auto-cleared)

        // Use REP to switch to 16-bit mode (M=0, X=0)
        cpu.rep(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);
        assert!(!cpu.m_flag());
        assert!(!cpu.x_flag());

        // Write full 16-bit values
        cpu.write_a(0x1234);
        cpu.write_x(0x5678);
        cpu.write_y(0x9ABC);
        assert_eq!(cpu.read_a(), 0x1234);
        assert_eq!(cpu.read_x(), 0x5678);
        assert_eq!(cpu.read_y(), 0x9ABC);

        // Switch back to 8-bit via SEP
        cpu.sep(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);
        assert!(cpu.m_flag());
        assert!(cpu.x_flag());

        // Verify width behavior:
        // - A: B preserved (0x1234 → 0x1234, but only low byte accessible in 8-bit mode)
        // - X/Y: high bytes forced to 0 (0x5678 → 0x0078, 0x9ABC → 0x00BC)
        assert_eq!(cpu.read_a(), 0x1234); // B preserved
        assert_eq!(cpu.read_x(), 0x0078); // High byte cleared
        assert_eq!(cpu.read_y(), 0x00BC); // High byte cleared

        // Write 8-bit values
        cpu.write_a(0xFF56);
        cpu.write_x(0xFF34);
        cpu.write_y(0xFF12);
        assert_eq!(cpu.read_a(), 0x1256); // B (0x12) preserved, A updated to 0x56
        assert_eq!(cpu.read_x(), 0x0034); // High byte forced to 0
        assert_eq!(cpu.read_y(), 0x0012); // High byte forced to 0

        // Set stack to arbitrary value in native mode
        cpu.s = 0x2345;
        assert_eq!(cpu.read_s(), 0x2345);

        // Switch back to emulation mode via XCE (C=1, E=0 → C=0, E=1)
        cpu.set_flag_c(true);
        cpu.xce();
        assert!(cpu.emulation_mode());
        assert!(!cpu.flag_c()); // Got old E=0
        assert!(cpu.m_flag()); // Forced to 1
        assert!(cpu.x_flag()); // Forced to 1
        assert_eq!(cpu.read_s(), 0x0145); // S high byte forced to $01
    }

    // -------------------------------------------------------------------------
    // Addressing mode tests
    // -------------------------------------------------------------------------

    mod addr_modes {
        use super::*;
        use crate::snes::bus::TestBus;

        fn cpu_with_bus() -> Cpu<TestBus> {
            let mut cpu = Cpu::new(TestBus::default());
            // Switch to native mode, 16-bit A and X/Y by default
            cpu.e = false;
            cpu.p &= !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);
            cpu
        }

        // -- Direct Page -------------------------------------------------------

        #[test]
        fn addr_dp_basic() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0x0200);
            assert_eq!(cpu.addr_dp(0x10), 0x0000_0210);
        }

        #[test]
        fn addr_dp_wraps_at_16bit() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0xFF00);
            assert_eq!(cpu.addr_dp(0xFF), 0x0000_FFFF);
        }

        #[test]
        fn addr_dp_x_adds_x_register() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0x0200);
            cpu.write_x(0x0010);
            assert_eq!(cpu.addr_dp_x(0x10), 0x0000_0220);
        }

        #[test]
        fn addr_dp_x_wraps_at_16bit() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0xFF00);
            cpu.write_x(0x0001);
            // 0xFF00 + 0xFF + 0x01 = 0x10000 → wraps to 0x0000
            assert_eq!(cpu.addr_dp_x(0xFF), 0x0000_0000);
        }

        #[test]
        fn addr_dp_y_adds_y_register() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0x0200);
            cpu.write_y(0x0005);
            assert_eq!(cpu.addr_dp_y(0x10), 0x0000_0215);
        }

        // -- Absolute ----------------------------------------------------------

        #[test]
        fn addr_abs_uses_dbr_as_bank() {
            let mut cpu = cpu_with_bus();
            cpu.write_dbr(0x03);
            assert_eq!(cpu.addr_abs(0x1234), 0x03_1234);
        }

        #[test]
        fn addr_abs_x_adds_x_and_can_cross_bank() {
            let mut cpu = cpu_with_bus();
            cpu.write_dbr(0x01);
            cpu.write_x(0x0100);
            // 0x01_FF00 + 0x100 = 0x02_0000
            assert_eq!(cpu.addr_abs_x(0xFF00), 0x02_0000);
        }

        #[test]
        fn addr_abs_y_adds_y_and_can_cross_bank() {
            let mut cpu = cpu_with_bus();
            cpu.write_dbr(0x02);
            cpu.write_y(0x0050);
            assert_eq!(cpu.addr_abs_y(0x1200), 0x02_1250);
        }

        // -- Absolute Long -----------------------------------------------------

        #[test]
        fn addr_abs_long_passes_through_24bit_addr() {
            let cpu = cpu_with_bus();
            assert_eq!(cpu.addr_abs_long(0x12_3456), 0x12_3456);
        }

        #[test]
        fn addr_abs_long_x_adds_x() {
            let mut cpu = cpu_with_bus();
            cpu.write_x(0x0010);
            assert_eq!(cpu.addr_abs_long_x(0x12_3456), 0x12_3466);
        }

        #[test]
        fn addr_abs_long_x_wraps_at_24bit() {
            let mut cpu = cpu_with_bus();
            cpu.write_x(0x0001);
            assert_eq!(cpu.addr_abs_long_x(0xFF_FFFF), 0x00_0000);
        }

        // -- Stack Relative ----------------------------------------------------

        #[test]
        fn addr_sr_adds_offset_to_s() {
            let mut cpu = cpu_with_bus();
            cpu.write_s(0x01F0);
            assert_eq!(cpu.addr_sr(0x10), 0x0000_0200);
        }

        #[test]
        fn addr_sr_wraps_at_16bit() {
            let mut cpu = cpu_with_bus();
            cpu.write_s(0xFF01);
            // 0xFF01 + 0xFF = 0x1_0000 → wraps to 0x0000
            assert_eq!(cpu.addr_sr(0xFF), 0x0000_0000);
        }

        // -- Direct Page Indirect (dp) -----------------------------------------

        #[test]
        fn addr_dp_ind_reads_ptr16_and_adds_dbr() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0x0200);
            cpu.write_dbr(0x05);
            // Place 16-bit pointer $1234 at DP address $0210
            cpu.bus.load(0x0000_0210, &[0x34, 0x12]);
            assert_eq!(cpu.addr_dp_ind(0x10), 0x05_1234);
        }

        // -- Direct Page Indirect Long [dp] ------------------------------------

        #[test]
        fn addr_dp_ind_long_reads_ptr24() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0x0200);
            // Place 24-bit pointer $78_1234 at DP address $0210
            cpu.bus.load(0x0000_0210, &[0x34, 0x12, 0x78]);
            assert_eq!(cpu.addr_dp_ind_long(0x10), 0x78_1234);
        }

        // -- Direct Page Indexed Indirect (dp,X) -------------------------------

        #[test]
        fn addr_dp_x_ind_adds_x_then_reads_ptr16() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0x0200);
            cpu.write_x(0x0010);
            cpu.write_dbr(0x03);
            // Place pointer $ABCD at D + ((offset + X) & $FF) = $0200 + $20 = $0220
            cpu.bus.load(0x0000_0220, &[0xCD, 0xAB]);
            assert_eq!(cpu.addr_dp_x_ind(0x10), 0x03_ABCD);
        }

        #[test]
        fn addr_dp_x_wraps_within_d_page_in_emulation_when_d_low_zero() {
            let mut cpu = cpu_with_bus();
            cpu.e = true;
            cpu.write_d(0x8300);
            cpu.write_x(0x00CC);
            assert_eq!(cpu.addr_dp_x(0xBC), 0x0000_8388);
        }

        #[test]
        fn addr_dp_y_wraps_within_d_page_in_emulation_when_d_low_zero() {
            let mut cpu = cpu_with_bus();
            cpu.e = true;
            cpu.write_d(0x1200);
            cpu.write_y(0x00F9);
            assert_eq!(cpu.addr_dp_y(0x80), 0x0000_1279);
        }

        #[test]
        fn addr_dp_x_ind_wraps_offset_plus_x_to_8bit() {
            let mut cpu = cpu_with_bus();
            cpu.e = true;
            cpu.write_d(0xB200);
            cpu.write_x(0x00F9);
            cpu.write_dbr(0xB8);
            // (offset + X) wraps: $B6 + $F9 = $1AF -> $AF, pointer read from $B2AF/$B2B0.
            cpu.bus.load(0x0000_B2AF, &[0x6C, 0x8B]);
            assert_eq!(cpu.addr_dp_x_ind(0xB6), 0xB8_8B6C);
        }

        #[test]
        fn addr_dp_x_ind_uses_full_add_when_d_low_nonzero() {
            let mut cpu = cpu_with_bus();
            cpu.e = false;
            cpu.write_d(0x61D2);
            cpu.write_x(0x0056);
            cpu.write_dbr(0xAF);
            // Full add: $61D2 + $F6 + $56 = $631E.
            cpu.bus.load(0x0000_631E, &[0x79, 0x14]);
            assert_eq!(cpu.addr_dp_x_ind(0xF6), 0xAF_1479);
        }

        #[test]
        fn addr_dp_x_ind_wraps_hi_byte_in_page_when_emulation_and_d_low_zero() {
            // snes-tests cputest test 0024 (adc ($EF,x), D=$0100, X=$10, E=1),
            // cross-verified against Mesen2: (offset+X) wraps to $FF within
            // D's page, landing the low-byte read at $01FF. The high-byte
            // read must then wrap within that same page back to $0100, not
            // carry into $0200.
            let mut cpu = cpu_with_bus();
            cpu.e = true;
            cpu.write_d(0x0100);
            cpu.write_x(0x0010);
            cpu.write_dbr(0x7F);
            cpu.bus.load(0x0000_01FF, &[0x34]);
            cpu.bus.load(0x0000_0100, &[0x12]);
            assert_eq!(cpu.addr_dp_x_ind(0xEF), 0x7F_1234);
        }

        #[test]
        fn addr_dp_x_ind_wraps_hi_byte_in_page_when_emulation_and_d_low_nonzero() {
            // Documented snes-tests cputest quirk (cputest/README.md),
            // cross-verified against Mesen2: in emulation mode with a
            // nonzero D low byte, (dp,X)'s low-byte pointer read uses a
            // full, non-wrapping add, but the high-byte read wraps within
            // that address's own page instead of carrying into the next
            // page.
            let mut cpu = cpu_with_bus();
            cpu.e = true;
            cpu.write_d(0x011A);
            cpu.write_x(0x00EE);
            cpu.write_dbr(0x00);
            // $011A + $F7 + $EE = $02FF: low byte read from $02FF, high byte
            // wraps back to $0200 (not $0300).
            cpu.bus.load(0x0000_02FF, &[0x6C]);
            cpu.bus.load(0x0000_0200, &[0x8B]);
            assert_eq!(cpu.addr_dp_x_ind(0xF7), 0x00_8B6C);
        }

        // -- Direct Page Indirect Indexed Y (dp),Y -----------------------------

        #[test]
        fn addr_dp_ind_y_reads_ptr16_then_adds_y() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0x0200);
            cpu.write_dbr(0x02);
            cpu.write_y(0x0004);
            // Place ptr $1000 at D + offset = $0210
            cpu.bus.load(0x0000_0210, &[0x00, 0x10]);
            // EA = DBR:$1000 + Y = $02_1004
            assert_eq!(cpu.addr_dp_ind_y(0x10), 0x02_1004);
        }

        #[test]
        fn addr_dp_ind_y_bank_crosses_allowed() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0x0200);
            cpu.write_dbr(0x01);
            cpu.write_y(0x0100);
            // ptr = $FF00, EA = $01_FF00 + $100 = $02_0000
            cpu.bus.load(0x0000_0210, &[0x00, 0xFF]);
            assert_eq!(cpu.addr_dp_ind_y(0x10), 0x02_0000);
        }

        // -- Direct Page Indirect Long Indexed Y [dp],Y ------------------------

        #[test]
        fn addr_dp_ind_long_y_reads_ptr24_then_adds_y() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0x0200);
            cpu.write_y(0x0010);
            // Place 24-bit ptr $05_1200 at $0210
            cpu.bus.load(0x0000_0210, &[0x00, 0x12, 0x05]);
            assert_eq!(cpu.addr_dp_ind_long_y(0x10), 0x05_1210);
        }

        #[test]
        fn addr_dp_ind_long_y_does_not_wrap_within_page_in_emulation_when_d_low_zero() {
            // snes-tests cputest test 0042 (adc [$FF],y, D=$0100, E=1),
            // cross-verified against Mesen2: unlike the 6502-heritage
            // 16-bit-pointer indirect modes, the 65816-only 24-bit
            // long-indirect pointer read never wraps within D's page, even
            // in emulation mode with a zero D low byte.
            let mut cpu = cpu_with_bus();
            cpu.e = true;
            cpu.write_d(0x0100);
            cpu.write_y(0x0010);
            cpu.write_dbr(0x00);
            cpu.bus.load(0x0000_01FF, &[0x34]);
            cpu.bus.load(0x0000_0200, &[0x12]);
            cpu.bus.load(0x0000_0201, &[0x7F]);
            assert_eq!(cpu.addr_dp_ind_long_y(0xFF), 0x7F_1244);
        }

        // -- Stack Relative Indirect Indexed Y (sr,S),Y ------------------------

        #[test]
        fn addr_sr_ind_y_reads_ptr16_at_s_plus_offset_then_adds_y() {
            let mut cpu = cpu_with_bus();
            cpu.write_s(0x01F0);
            cpu.write_dbr(0x04);
            cpu.write_y(0x0008);
            // ptr_addr = S + offset = $01F0 + $10 = $0200
            cpu.bus.load(0x0000_0200, &[0x00, 0x30]);
            // EA = DBR:$3000 + Y = $04_3008
            assert_eq!(cpu.addr_sr_ind_y(0x10), 0x04_3008);
        }

        // -- Pointer byte read wrapping at bank-0 $FFFF boundary ---------------

        #[test]
        fn addr_dp_ind_ptr_wraps_at_ffff() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0xFFFF);
            cpu.write_dbr(0x02);
            // Place low byte at $FFFF, high byte wraps to $0000
            cpu.bus.load(0x0000_FFFF, &[0xCD]);
            cpu.bus.load(0x0000_0000, &[0xAB]);
            assert_eq!(cpu.addr_dp_ind(0x00), 0x02_ABCD);
        }

        #[test]
        fn addr_dp_ind_long_ptr_wraps_at_ffff() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0xFFFE);
            // 3-byte pointer: $FE→lo, $FF→mid, $00→hi (wraps)
            cpu.bus.load(0x0000_FFFE, &[0x11, 0x22]);
            cpu.bus.load(0x0000_0000, &[0x33]);
            assert_eq!(cpu.addr_dp_ind_long(0x00), 0x33_2211);
        }

        #[test]
        fn addr_dp_x_ind_ptr_wraps_at_ffff() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0xFF00);
            cpu.write_x(0x00FF); // D + offset + X = $FF00 + $00 + $FF = $FFFF
            cpu.write_dbr(0x05);
            cpu.bus.load(0x0000_FFFF, &[0x78]);
            cpu.bus.load(0x0000_0000, &[0x56]);
            assert_eq!(cpu.addr_dp_x_ind(0x00), 0x05_5678);
        }

        #[test]
        fn addr_dp_ind_y_ptr_wraps_at_ffff() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0xFFFF);
            cpu.write_dbr(0x01);
            cpu.write_y(0x0001);
            cpu.bus.load(0x0000_FFFF, &[0xFF]);
            cpu.bus.load(0x0000_0000, &[0x00]);
            // ptr = $00FF, EA = $01_00FF + 1 = $01_0100
            assert_eq!(cpu.addr_dp_ind_y(0x00), 0x01_0100);
        }

        #[test]
        fn addr_dp_ind_long_y_ptr_wraps_at_ffff() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0xFFFE);
            cpu.write_y(0x0002);
            cpu.bus.load(0x0000_FFFE, &[0x00, 0x10]);
            cpu.bus.load(0x0000_0000, &[0x07]);
            // base = $07_1000, EA = $07_1002
            assert_eq!(cpu.addr_dp_ind_long_y(0x00), 0x07_1002);
        }

        #[test]
        fn addr_sr_ind_y_ptr_wraps_at_ffff() {
            let mut cpu = cpu_with_bus();
            cpu.write_s(0xFFFF);
            cpu.write_dbr(0x03);
            cpu.write_y(0x0000);
            cpu.bus.load(0x0000_FFFF, &[0x34]);
            cpu.bus.load(0x0000_0000, &[0x12]);
            assert_eq!(cpu.addr_sr_ind_y(0x00), 0x03_1234);
        }
    }
}

#[cfg(test)]
mod mem_helpers_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn make_cpu() -> Cpu<TestBus> {
        Cpu::new(TestBus::default())
    }

    #[test]
    fn read8_returns_byte_at_address() {
        let mut cpu = make_cpu();
        cpu.bus.load(0x01_2000, &[0xAB]);
        assert_eq!(cpu.read8(0x01_2000), 0xAB);
    }

    #[test]
    fn write8_stores_byte_at_address() {
        let mut cpu = make_cpu();
        cpu.write8(0x01_3000, 0x55);
        assert_eq!(cpu.bus.read(0x01_3000), 0x55);
    }

    #[test]
    fn read16_little_endian() {
        let mut cpu = make_cpu();
        cpu.bus.load(0x02_1000, &[0x34, 0x12]);
        assert_eq!(cpu.read16(0x02_1000), 0x1234);
    }

    #[test]
    fn read16_carries_high_byte_into_next_bank() {
        let mut cpu = make_cpu();
        cpu.bus.load(0x02_FFFF, &[0x78]);
        cpu.bus.load(0x03_0000, &[0x56]);
        assert_eq!(cpu.read16(0x02_FFFF), 0x5678);
    }

    #[test]
    fn write16_little_endian() {
        let mut cpu = make_cpu();
        cpu.write16(0x03_2000, 0xBEEF);
        assert_eq!(cpu.bus.read(0x03_2000), 0xEF);
        assert_eq!(cpu.bus.read(0x03_2001), 0xBE);
    }

    #[test]
    fn write16_carries_high_byte_into_next_bank() {
        let mut cpu = make_cpu();
        cpu.write16(0x04_FFFF, 0xCAFE);
        assert_eq!(cpu.bus.read(0x04_FFFF), 0xFE);
        assert_eq!(cpu.bus.read(0x05_0000), 0xCA);
    }

    #[test]
    fn read_m_16bit_preserves_bank_for_banked_addresses() {
        let mut cpu = make_cpu();
        cpu.e = false;
        cpu.p &= !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);
        cpu.dbr = 0x6B;
        cpu.bus.load(0x6B1234, &[0x78, 0x56]);

        assert_eq!(cpu.read_m(0x6B1234), 0x5678);
    }

    #[test]
    fn read_m_reads_8bit_when_m_flag_set() {
        let mut cpu = make_cpu(); // reset default: M=1
        cpu.bus.load(0x00_1000, &[0x42, 0xFF]);
        assert_eq!(cpu.read_m(0x00_1000), 0x0042);
    }

    #[test]
    fn read_m_reads_16bit_when_m_flag_clear() {
        let mut cpu = make_cpu();
        cpu.e = false;
        cpu.rep(FLAG_ACCUM_WIDTH);
        cpu.bus.load(0x00_1000, &[0x34, 0x12]);
        assert_eq!(cpu.read_m(0x00_1000), 0x1234);
    }

    #[test]
    fn write_m_writes_8bit_when_m_flag_set() {
        let mut cpu = make_cpu(); // default: M=1
        cpu.write_m(0x00_2000, 0x1234);
        assert_eq!(cpu.bus.read(0x00_2000), 0x34);
        assert_eq!(cpu.bus.read(0x00_2001), 0x00); // high byte not written
    }

    #[test]
    fn write_m_writes_16bit_when_m_flag_clear() {
        let mut cpu = make_cpu();
        cpu.e = false;
        cpu.rep(FLAG_ACCUM_WIDTH);
        cpu.write_m(0x00_3000, 0xABCD);
        assert_eq!(cpu.bus.read(0x00_3000), 0xCD);
        assert_eq!(cpu.bus.read(0x00_3001), 0xAB);
    }

    #[test]
    fn read_idx_reads_8bit_when_x_flag_set() {
        let mut cpu = make_cpu(); // reset default: X=1
        cpu.bus.load(0x00_4000, &[0x77, 0xFF]);
        assert_eq!(cpu.read_idx(0x00_4000), 0x0077);
    }

    #[test]
    fn read_idx_reads_16bit_when_x_flag_clear() {
        let mut cpu = make_cpu();
        cpu.e = false;
        cpu.rep(FLAG_INDEX_WIDTH);
        cpu.bus.load(0x00_4000, &[0x34, 0x12]);
        assert_eq!(cpu.read_idx(0x00_4000), 0x1234);
    }

    #[test]
    fn write_idx_writes_8bit_when_x_flag_set() {
        let mut cpu = make_cpu(); // default: X=1
        cpu.write_idx(0x00_5000, 0x1234);
        assert_eq!(cpu.bus.read(0x00_5000), 0x34);
        assert_eq!(cpu.bus.read(0x00_5001), 0x00); // high byte not written
    }

    #[test]
    fn write_idx_writes_16bit_when_x_flag_clear() {
        let mut cpu = make_cpu();
        cpu.e = false;
        cpu.rep(FLAG_INDEX_WIDTH);
        cpu.write_idx(0x00_6000, 0xDEAD);
        assert_eq!(cpu.bus.read(0x00_6000), 0xAD);
        assert_eq!(cpu.bus.read(0x00_6001), 0xDE);
    }
}

#[cfg(test)]
mod step_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn make_native_cpu() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p &= !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);
        cpu
    }

    fn make_8bit_cpu() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p |= FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
        cpu
    }

    // -------------------------------------------------------------------------
    // NOP
    // -------------------------------------------------------------------------

    #[test]
    fn nop_advances_pc_by_1() {
        let mut cpu = make_native_cpu();
        cpu.pc = 0x1000;
        cpu.bus.load(0x1000, &[0xEA]); // NOP
        let flags_before = cpu.p;
        cpu.step();
        assert_eq!(cpu.pc, 0x1001);
        assert_eq!(cpu.p, flags_before); // no flag changes
    }

    // -------------------------------------------------------------------------
    // TAX  ($AA) — A→X, X-width, sets N,Z
    // -------------------------------------------------------------------------

    #[test]
    fn tax_16bit_transfers_a_to_x() {
        let mut cpu = make_native_cpu();
        cpu.a = 0x1234;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0xAA]); // TAX
        cpu.step();
        assert_eq!(cpu.x, 0x1234);
    }

    #[test]
    fn tax_8bit_transfers_low_byte_of_a_to_x() {
        let mut cpu = make_8bit_cpu();
        cpu.a = 0x1234;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0xAA]); // TAX
        cpu.step();
        assert_eq!(cpu.x, 0x0034); // only low byte, high forced to 0
    }

    #[test]
    fn tax_sets_n_flag_when_result_negative() {
        let mut cpu = make_native_cpu();
        cpu.a = 0x8001;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0xAA]);
        cpu.step();
        assert!(cpu.flag_n());
        assert!(!cpu.flag_z());
    }

    #[test]
    fn tax_sets_z_flag_when_result_zero() {
        let mut cpu = make_native_cpu();
        cpu.a = 0x0000;
        cpu.p |= FLAG_NEGATIVE; // pre-set N to verify it clears
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0xAA]);
        cpu.step();
        assert!(!cpu.flag_n());
        assert!(cpu.flag_z());
    }

    // -------------------------------------------------------------------------
    // TXA  ($8A) — X→A, M-width, sets N,Z
    // -------------------------------------------------------------------------

    #[test]
    fn txa_16bit_transfers_x_to_a() {
        let mut cpu = make_native_cpu();
        cpu.x = 0x5678;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0x8A]); // TXA
        cpu.step();
        assert_eq!(cpu.a, 0x5678);
    }

    #[test]
    fn txa_8bit_transfers_x_to_low_byte_of_a() {
        let mut cpu = make_8bit_cpu();
        cpu.a = 0x1200; // B=0x12 preserved
        cpu.x = 0x0056;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0x8A]); // TXA
        cpu.step();
        assert_eq!(cpu.a, 0x1256); // B preserved, A=0x56
    }

    #[test]
    fn txa_8bit_sets_z_flag_when_low_byte_zero_even_with_nonzero_b() {
        let mut cpu = make_8bit_cpu();
        cpu.a = 0x1200; // B=0x12
        cpu.x = 0x0000; // X low byte = 0
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0x8A]);
        cpu.step();
        assert_eq!(cpu.a, 0x1200); // B preserved, A=0x00
        assert!(cpu.flag_z()); // Z set because 8-bit result is 0x00
        assert!(!cpu.flag_n());
    }

    #[test]
    fn txa_sets_n_flag() {
        let mut cpu = make_native_cpu();
        cpu.x = 0x8000;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0x8A]);
        cpu.step();
        assert!(cpu.flag_n());
    }

    #[test]
    fn txa_sets_z_flag_when_zero() {
        let mut cpu = make_native_cpu();
        cpu.x = 0x0000;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0x8A]);
        cpu.step();
        assert!(cpu.flag_z());
    }

    // -------------------------------------------------------------------------
    // TAY  ($A8) — A→Y, X-width, sets N,Z
    // -------------------------------------------------------------------------

    #[test]
    fn tay_16bit_transfers_a_to_y() {
        let mut cpu = make_native_cpu();
        cpu.a = 0xABCD;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0xA8]); // TAY
        cpu.step();
        assert_eq!(cpu.y, 0xABCD);
    }

    #[test]
    fn tay_8bit_truncates_to_low_byte() {
        let mut cpu = make_8bit_cpu();
        cpu.a = 0x1234;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0xA8]);
        cpu.step();
        assert_eq!(cpu.y, 0x0034);
    }

    #[test]
    fn tay_sets_n_and_z_flags() {
        let mut cpu = make_native_cpu();
        cpu.a = 0x0000;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0xA8]);
        cpu.step();
        assert!(cpu.flag_z());
        assert!(!cpu.flag_n());
    }

    // -------------------------------------------------------------------------
    // TYA  ($98) — Y→A, M-width, sets N,Z
    // -------------------------------------------------------------------------

    #[test]
    fn tya_16bit_transfers_y_to_a() {
        let mut cpu = make_native_cpu();
        cpu.y = 0x1357;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0x98]); // TYA
        cpu.step();
        assert_eq!(cpu.a, 0x1357);
    }

    #[test]
    fn tya_8bit_transfers_y_to_low_a_preserves_b() {
        let mut cpu = make_8bit_cpu();
        cpu.a = 0x2200;
        cpu.y = 0x0077;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0x98]);
        cpu.step();
        assert_eq!(cpu.a, 0x2277); // B preserved
    }

    // -------------------------------------------------------------------------
    // TXS  ($9A) — X→S, no flags  (in native: full 16-bit; emulation: low byte)
    // -------------------------------------------------------------------------

    #[test]
    fn txs_native_transfers_full_x_to_s_no_flags() {
        let mut cpu = make_native_cpu();
        cpu.x = 0x1234;
        cpu.p = FLAG_NEGATIVE | FLAG_ZERO; // pre-set flags
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0x9A]); // TXS
        cpu.step();
        assert_eq!(cpu.s, 0x1234);
        assert_eq!(cpu.p, FLAG_NEGATIVE | FLAG_ZERO); // flags unchanged
    }

    #[test]
    fn txs_emulation_forces_high_byte_01() {
        let mut cpu = Cpu::new(TestBus::default()); // emulation mode
        cpu.x = 0x0056;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0x9A]);
        cpu.step();
        assert_eq!(cpu.s, 0x0156); // high byte forced to $01 in emulation
    }

    // -------------------------------------------------------------------------
    // TSX  ($BA) — S→X, X-width, sets N,Z
    // -------------------------------------------------------------------------

    #[test]
    fn tsx_16bit_transfers_s_to_x_sets_flags() {
        let mut cpu = make_native_cpu();
        cpu.s = 0x8001;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0xBA]); // TSX
        cpu.step();
        assert_eq!(cpu.x, 0x8001);
        assert!(cpu.flag_n());
        assert!(!cpu.flag_z());
    }

    #[test]
    fn tsx_8bit_transfers_low_byte_of_s() {
        let mut cpu = make_8bit_cpu();
        cpu.s = 0x01AB;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0xBA]);
        cpu.step();
        assert_eq!(cpu.x, 0x00AB); // only low byte
    }

    // -------------------------------------------------------------------------
    // TXY  ($9B) — X→Y, X-width, sets N,Z
    // -------------------------------------------------------------------------

    #[test]
    fn txy_16bit_transfers_x_to_y() {
        let mut cpu = make_native_cpu();
        cpu.x = 0x4321;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0x9B]); // TXY
        cpu.step();
        assert_eq!(cpu.y, 0x4321);
        assert!(!cpu.flag_n());
        assert!(!cpu.flag_z());
    }

    // -------------------------------------------------------------------------
    // TYX  ($BB) — Y→X, X-width, sets N,Z
    // -------------------------------------------------------------------------

    #[test]
    fn tyx_16bit_transfers_y_to_x() {
        let mut cpu = make_native_cpu();
        cpu.y = 0xFFFF;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0xBB]); // TYX
        cpu.step();
        assert_eq!(cpu.x, 0xFFFF);
        assert!(cpu.flag_n());
        assert!(!cpu.flag_z());
    }

    // -------------------------------------------------------------------------
    // TCD  ($5B) — C(16-bit A)→D, always 16-bit, sets N,Z
    // -------------------------------------------------------------------------

    #[test]
    fn tcd_always_16bit_transfers_a_to_d() {
        let mut cpu = make_8bit_cpu(); // even in 8-bit mode, TCD is always 16-bit
        cpu.a = 0x1234;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0x5B]); // TCD
        cpu.step();
        assert_eq!(cpu.d, 0x1234);
    }

    #[test]
    fn tcd_sets_n_flag_for_negative_value() {
        let mut cpu = make_native_cpu();
        cpu.a = 0x8000;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0x5B]);
        cpu.step();
        assert!(cpu.flag_n());
    }

    #[test]
    fn tcd_sets_z_flag_for_zero() {
        let mut cpu = make_native_cpu();
        cpu.a = 0x0000;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0x5B]);
        cpu.step();
        assert!(cpu.flag_z());
    }

    // -------------------------------------------------------------------------
    // TDC  ($7B) — D→C(16-bit A), always 16-bit, sets N,Z
    // -------------------------------------------------------------------------

    #[test]
    fn tdc_always_16bit_transfers_d_to_a() {
        let mut cpu = make_8bit_cpu();
        cpu.d = 0xABCD;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0x7B]); // TDC
        cpu.step();
        assert_eq!(cpu.a, 0xABCD);
    }

    #[test]
    fn tdc_sets_n_z_flags() {
        let mut cpu = make_native_cpu();
        cpu.d = 0x0000;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0x7B]);
        cpu.step();
        assert!(cpu.flag_z());
        assert!(!cpu.flag_n());
    }

    // -------------------------------------------------------------------------
    // TCS  ($1B) — C(16-bit A)→S, always 16-bit in native, no flags
    // -------------------------------------------------------------------------

    #[test]
    fn tcs_native_transfers_a_to_s_no_flags() {
        let mut cpu = make_native_cpu();
        cpu.a = 0x1FFF;
        cpu.p = FLAG_ZERO; // pre-set flags
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0x1B]); // TCS
        cpu.step();
        assert_eq!(cpu.s, 0x1FFF);
        assert_eq!(cpu.p, FLAG_ZERO); // flags unchanged
    }

    // -------------------------------------------------------------------------
    // TSC  ($3B) — S→C(16-bit A), always 16-bit, sets N,Z
    // -------------------------------------------------------------------------

    #[test]
    fn tsc_always_16bit_transfers_s_to_a() {
        let mut cpu = make_8bit_cpu();
        cpu.s = 0x01FF;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0x3B]); // TSC
        cpu.step();
        assert_eq!(cpu.a, 0x01FF);
    }

    #[test]
    fn tsc_sets_flags() {
        let mut cpu = make_native_cpu();
        cpu.s = 0x8000;
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0x3B]);
        cpu.step();
        assert!(cpu.flag_n());
        assert!(!cpu.flag_z());
    }

    // -------------------------------------------------------------------------
    // XBA  ($EB) — exchange B and A bytes, sets N,Z on new low byte
    // -------------------------------------------------------------------------

    #[test]
    fn xba_swaps_b_and_a_bytes() {
        let mut cpu = make_native_cpu();
        cpu.a = 0x1234; // B=0x12, A=0x34
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0xEB]); // XBA
        cpu.step();
        assert_eq!(cpu.a, 0x3412); // B=0x34, A=0x12
    }

    #[test]
    fn xba_sets_n_z_flags_on_new_low_byte() {
        let mut cpu = make_native_cpu();
        cpu.a = 0x0080; // B=0x00, A=0x80 → after swap: B=0x80, A=0x00
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0xEB]);
        cpu.step();
        assert_eq!(cpu.a, 0x8000);
        assert!(cpu.flag_z()); // new low byte (0x00) is zero
        assert!(!cpu.flag_n()); // new low byte is not negative
    }

    #[test]
    fn xba_n_flag_on_new_low_byte_negative() {
        let mut cpu = make_native_cpu();
        cpu.a = 0x0090; // B=0x00, A=0x90 → after: B=0x90, A=0x00; wait - swap B and A
        // A=0x00AB: B=0x00, A=0xAB → after swap: B=0xAB, A=0x00? No.
        // Actually: B is high byte, A is low byte of the 16-bit register
        // a = 0xBBAA: BB = high = B, AA = low = A
        // XBA: swap → a = 0xAABB
        cpu.a = 0x00AB; // B=0x00, A(low)=0xAB → swap → B=0xAB, A(low)=0x00
        // Hmm, let me reconsider. In the register: a stores B:A where B=high byte.
        // XBA swaps high and low bytes.
        // So 0x00AB → 0xAB00: new low byte = 0x00 (not negative)
        // Let me use a = 0x3490: low = 0x90 (negative), high = 0x34
        // After XBA: 0x9034, new low byte = 0x34 (not negative)
        // Let me use a value where new low byte is >= 0x80
        cpu.a = 0x9034; // B=0x90, A=0x34 → swap → B=0x34, A=0x90
        cpu.pc = 0x0000;
        cpu.bus.load(0x0000, &[0xEB]);
        cpu.step();
        assert_eq!(cpu.a, 0x3490);
        assert!(cpu.flag_n()); // new low byte 0x90 is negative
        assert!(!cpu.flag_z());
    }
}

#[cfg(test)]
mod lda_ldx_ldy_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn native16() -> Cpu<TestBus> {
        // native mode, M=0 (16-bit A), X=0 (16-bit X/Y)
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p &= !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);
        cpu
    }

    fn native8() -> Cpu<TestBus> {
        // native mode, M=1 (8-bit A), X=1 (8-bit X/Y)
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p |= FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
        cpu
    }

    // =========================================================================
    // LDA — all addressing modes
    // =========================================================================

    #[test]
    fn lda_immediate_16bit_loads_two_bytes() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0xA9, 0x34, 0x12]); // LDA #$1234
        cpu.step();
        assert_eq!(cpu.a, 0x1234);
        assert_eq!(cpu.pc, 0x0003);
    }

    #[test]
    fn lda_immediate_8bit_loads_one_byte_preserves_b() {
        let mut cpu = native8();
        cpu.a = 0xBB00; // B=0xBB
        cpu.bus.load(0x0000, &[0xA9, 0x42]); // LDA #$42
        cpu.step();
        assert_eq!(cpu.a, 0xBB42); // B preserved
        assert_eq!(cpu.pc, 0x0002);
    }

    #[test]
    fn lda_dp_loads_from_direct_page() {
        let mut cpu = native16();
        cpu.d = 0x0200;
        cpu.bus.load(0x0210, &[0x78, 0x56]); // $5678 at DP+$10
        cpu.bus.load(0x0000, &[0xA5, 0x10]); // LDA $10
        cpu.step();
        assert_eq!(cpu.a, 0x5678);
    }

    #[test]
    fn lda_dp_x_loads_from_direct_page_indexed() {
        let mut cpu = native16();
        cpu.d = 0x0200;
        cpu.x = 0x0004;
        cpu.bus.load(0x0214, &[0xCD, 0xAB]); // $ABCD at DP+$10+X
        cpu.bus.load(0x0000, &[0xB5, 0x10]); // LDA $10,X
        cpu.step();
        assert_eq!(cpu.a, 0xABCD);
    }

    #[test]
    fn lda_abs_uses_dbr() {
        let mut cpu = native16();
        cpu.dbr = 0x03;
        cpu.bus.load(0x03_1234, &[0xEF, 0xBE]); // $BEEF at bank 3
        cpu.bus.load(0x0000, &[0xAD, 0x34, 0x12]); // LDA $1234
        cpu.step();
        assert_eq!(cpu.a, 0xBEEF);
    }

    #[test]
    fn lda_abs_x_adds_x_to_absolute_address() {
        let mut cpu = native16();
        cpu.dbr = 0x01;
        cpu.x = 0x0010;
        cpu.bus.load(0x01_1010, &[0x78, 0x56]);
        cpu.bus.load(0x0000, &[0xBD, 0x00, 0x10]); // LDA $1000,X
        cpu.step();
        assert_eq!(cpu.a, 0x5678);
    }

    #[test]
    fn lda_abs_y_adds_y_to_absolute_address() {
        let mut cpu = native16();
        cpu.dbr = 0x02;
        cpu.y = 0x0008;
        cpu.bus.load(0x02_2008, &[0x21, 0x43]);
        cpu.bus.load(0x0000, &[0xB9, 0x00, 0x20]); // LDA $2000,Y
        cpu.step();
        assert_eq!(cpu.a, 0x4321);
    }

    #[test]
    fn lda_abs_long_uses_explicit_bank() {
        let mut cpu = native16();
        cpu.bus.load(0x05_4000, &[0x11, 0x22]);
        cpu.bus.load(0x0000, &[0xAF, 0x00, 0x40, 0x05]); // LDA $054000
        cpu.step();
        assert_eq!(cpu.a, 0x2211);
    }

    #[test]
    fn lda_abs_long_x_adds_x_to_24bit_addr() {
        let mut cpu = native16();
        cpu.x = 0x0002;
        cpu.bus.load(0x05_4002, &[0x99, 0x88]);
        cpu.bus.load(0x0000, &[0xBF, 0x00, 0x40, 0x05]); // LDA $054000,X
        cpu.step();
        assert_eq!(cpu.a, 0x8899);
    }

    #[test]
    fn lda_dp_x_ind_reads_via_pointer() {
        let mut cpu = native16();
        cpu.d = 0x0200;
        cpu.x = 0x0004;
        cpu.dbr = 0x07;
        // pointer at D+offset+X = $0200+$10+$04 = $0214 → $3456
        cpu.bus.load(0x0214, &[0x56, 0x34]);
        cpu.bus.load(0x07_3456, &[0xAA, 0xBB]);
        cpu.bus.load(0x0000, &[0xA1, 0x10]); // LDA ($10,X)
        cpu.step();
        assert_eq!(cpu.a, 0xBBAA);
    }

    #[test]
    fn lda_dp_ind_y_reads_via_pointer_plus_y() {
        let mut cpu = native16();
        cpu.d = 0x0200;
        cpu.dbr = 0x04;
        cpu.y = 0x0006;
        // pointer at D+offset = $0210 → $1000
        cpu.bus.load(0x0210, &[0x00, 0x10]);
        cpu.bus.load(0x04_1006, &[0xCC, 0xDD]);
        cpu.bus.load(0x0000, &[0xB1, 0x10]); // LDA ($10),Y
        cpu.step();
        assert_eq!(cpu.a, 0xDDCC);
    }

    #[test]
    fn lda_dp_ind_reads_via_dp_pointer() {
        let mut cpu = native16();
        cpu.d = 0x0200;
        cpu.dbr = 0x06;
        cpu.bus.load(0x0210, &[0x00, 0x30]);
        cpu.bus.load(0x06_3000, &[0xFF, 0x00]);
        cpu.bus.load(0x0000, &[0xB2, 0x10]); // LDA ($10)
        cpu.step();
        assert_eq!(cpu.a, 0x00FF);
    }

    #[test]
    fn lda_sr_reads_stack_relative() {
        let mut cpu = native16();
        cpu.s = 0x01F0;
        cpu.bus.load(0x0200, &[0x12, 0x34]); // S+$10 = $0200
        cpu.bus.load(0x0000, &[0xA3, 0x10]); // LDA $10,S
        cpu.step();
        assert_eq!(cpu.a, 0x3412);
    }

    #[test]
    fn lda_sr_ind_y_reads_via_sr_pointer_plus_y() {
        let mut cpu = native16();
        cpu.s = 0x01F0;
        cpu.dbr = 0x02;
        cpu.y = 0x0008;
        cpu.bus.load(0x0200, &[0x00, 0x50]); // ptr = $5000
        cpu.bus.load(0x02_5008, &[0x77, 0x66]);
        cpu.bus.load(0x0000, &[0xB3, 0x10]); // LDA ($10,S),Y
        cpu.step();
        assert_eq!(cpu.a, 0x6677);
    }

    #[test]
    fn lda_dp_ind_long_reads_24bit_pointer() {
        let mut cpu = native16();
        cpu.d = 0x0200;
        cpu.bus.load(0x0210, &[0x00, 0x60, 0x05]); // 24-bit ptr = $05_6000
        cpu.bus.load(0x05_6000, &[0x11, 0x22]);
        cpu.bus.load(0x0000, &[0xA7, 0x10]); // LDA [$10]
        cpu.step();
        assert_eq!(cpu.a, 0x2211);
    }

    #[test]
    fn lda_dp_ind_long_y_reads_24bit_pointer_plus_y() {
        let mut cpu = native16();
        cpu.d = 0x0200;
        cpu.y = 0x0004;
        cpu.bus.load(0x0210, &[0x00, 0x70, 0x03]); // ptr = $03_7000
        cpu.bus.load(0x03_7004, &[0x55, 0x44]);
        cpu.bus.load(0x0000, &[0xB7, 0x10]); // LDA [$10],Y
        cpu.step();
        assert_eq!(cpu.a, 0x4455);
    }

    #[test]
    fn lda_sets_n_flag() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0xA9, 0x00, 0x80]); // LDA #$8000
        cpu.step();
        assert!(cpu.flag_n());
        assert!(!cpu.flag_z());
    }

    #[test]
    fn lda_sets_z_flag() {
        let mut cpu = native16();
        cpu.p |= FLAG_NEGATIVE; // pre-set N
        cpu.bus.load(0x0000, &[0xA9, 0x00, 0x00]); // LDA #$0000
        cpu.step();
        assert!(cpu.flag_z());
        assert!(!cpu.flag_n());
    }

    // =========================================================================
    // LDX — immediate, dp, dp+Y, abs, abs+Y
    // =========================================================================

    #[test]
    fn ldx_immediate_16bit() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0xA2, 0xCD, 0xAB]); // LDX #$ABCD
        cpu.step();
        assert_eq!(cpu.x, 0xABCD);
    }

    #[test]
    fn ldx_immediate_8bit() {
        let mut cpu = native8();
        cpu.bus.load(0x0000, &[0xA2, 0x77]); // LDX #$77
        cpu.step();
        assert_eq!(cpu.x, 0x0077); // high byte forced to 0
    }

    #[test]
    fn ldx_dp_loads_from_direct_page() {
        let mut cpu = native16();
        cpu.d = 0x0300;
        cpu.bus.load(0x0310, &[0x34, 0x12]);
        cpu.bus.load(0x0000, &[0xA6, 0x10]); // LDX $10
        cpu.step();
        assert_eq!(cpu.x, 0x1234);
    }

    #[test]
    fn ldx_dp_y_loads_from_direct_page_indexed_y() {
        let mut cpu = native16();
        cpu.d = 0x0300;
        cpu.y = 0x0002;
        cpu.bus.load(0x0312, &[0x56, 0x78]);
        cpu.bus.load(0x0000, &[0xB6, 0x10]); // LDX $10,Y
        cpu.step();
        assert_eq!(cpu.x, 0x7856);
    }

    #[test]
    fn ldx_abs_uses_dbr() {
        let mut cpu = native16();
        cpu.dbr = 0x02;
        cpu.bus.load(0x02_5678, &[0xAB, 0xCD]);
        cpu.bus.load(0x0000, &[0xAE, 0x78, 0x56]); // LDX $5678
        cpu.step();
        assert_eq!(cpu.x, 0xCDAB);
    }

    #[test]
    fn ldx_abs_y_adds_y() {
        let mut cpu = native16();
        cpu.dbr = 0x01;
        cpu.y = 0x0010;
        cpu.bus.load(0x01_1010, &[0x22, 0x11]);
        cpu.bus.load(0x0000, &[0xBE, 0x00, 0x10]); // LDX $1000,Y
        cpu.step();
        assert_eq!(cpu.x, 0x1122);
    }

    #[test]
    fn ldx_sets_n_flag() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0xA2, 0x00, 0x80]); // LDX #$8000
        cpu.step();
        assert!(cpu.flag_n());
    }

    #[test]
    fn ldx_sets_z_flag() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0xA2, 0x00, 0x00]); // LDX #$0000
        cpu.step();
        assert!(cpu.flag_z());
    }

    // =========================================================================
    // LDY — immediate, dp, dp+X, abs, abs+X
    // =========================================================================

    #[test]
    fn ldy_immediate_16bit() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0xA0, 0x21, 0x43]); // LDY #$4321
        cpu.step();
        assert_eq!(cpu.y, 0x4321);
    }

    #[test]
    fn ldy_immediate_8bit() {
        let mut cpu = native8();
        cpu.bus.load(0x0000, &[0xA0, 0x55]); // LDY #$55
        cpu.step();
        assert_eq!(cpu.y, 0x0055);
    }

    #[test]
    fn ldy_dp_loads_from_direct_page() {
        let mut cpu = native16();
        cpu.d = 0x0400;
        cpu.bus.load(0x0420, &[0x78, 0x56]);
        cpu.bus.load(0x0000, &[0xA4, 0x20]); // LDY $20
        cpu.step();
        assert_eq!(cpu.y, 0x5678);
    }

    #[test]
    fn ldy_dp_x_loads_from_direct_page_indexed_x() {
        let mut cpu = native16();
        cpu.d = 0x0400;
        cpu.x = 0x0004;
        cpu.bus.load(0x0424, &[0xEF, 0xCD]);
        cpu.bus.load(0x0000, &[0xB4, 0x20]); // LDY $20,X
        cpu.step();
        assert_eq!(cpu.y, 0xCDEF);
    }

    #[test]
    fn ldy_abs_uses_dbr() {
        let mut cpu = native16();
        cpu.dbr = 0x04;
        cpu.bus.load(0x04_ABCD, &[0x12, 0x34]);
        cpu.bus.load(0x0000, &[0xAC, 0xCD, 0xAB]); // LDY $ABCD
        cpu.step();
        assert_eq!(cpu.y, 0x3412);
    }

    #[test]
    fn ldy_abs_x_adds_x() {
        let mut cpu = native16();
        cpu.dbr = 0x03;
        cpu.x = 0x0020;
        cpu.bus.load(0x03_2020, &[0x66, 0x77]);
        cpu.bus.load(0x0000, &[0xBC, 0x00, 0x20]); // LDY $2000,X
        cpu.step();
        assert_eq!(cpu.y, 0x7766);
    }

    #[test]
    fn ldy_sets_n_flag() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0xA0, 0x00, 0xFF]); // LDY #$FF00
        cpu.step();
        assert!(cpu.flag_n());
    }

    #[test]
    fn ldy_sets_z_flag() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0xA0, 0x00, 0x00]); // LDY #$0000
        cpu.step();
        assert!(cpu.flag_z());
    }
}

#[cfg(test)]
mod sta_stx_sty_stz_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn native16() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p &= !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);
        cpu
    }

    fn native8() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p |= FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
        cpu
    }

    // =========================================================================
    // STA — store accumulator
    // =========================================================================

    #[test]
    fn sta_dp_stores_a_16bit() {
        let mut cpu = native16();
        cpu.a = 0xABCD;
        cpu.d = 0x0200;
        cpu.bus.load(0x0000, &[0x85, 0x10]); // STA $10
        cpu.step();
        assert_eq!(cpu.bus.read(0x0210), 0xCD);
        assert_eq!(cpu.bus.read(0x0211), 0xAB);
    }

    #[test]
    fn sta_dp_stores_a_8bit() {
        let mut cpu = native8();
        cpu.a = 0x1234; // B=0x12, A=0x34
        cpu.d = 0x0200;
        cpu.bus.load(0x0000, &[0x85, 0x10]); // STA $10
        cpu.step();
        assert_eq!(cpu.bus.read(0x0210), 0x34); // only low byte
        assert_eq!(cpu.bus.read(0x0211), 0x00); // high byte untouched
    }

    #[test]
    fn sta_dp_x_stores_a_indexed() {
        let mut cpu = native16();
        cpu.a = 0x1234;
        cpu.d = 0x0200;
        cpu.x = 0x0008;
        cpu.bus.load(0x0000, &[0x95, 0x10]); // STA $10,X
        cpu.step();
        assert_eq!(cpu.bus.read(0x0218), 0x34);
        assert_eq!(cpu.bus.read(0x0219), 0x12);
    }

    #[test]
    fn sta_abs_stores_a_using_dbr() {
        let mut cpu = native16();
        cpu.a = 0xBEEF;
        cpu.dbr = 0x03;
        cpu.bus.load(0x0000, &[0x8D, 0x00, 0x10]); // STA $1000
        cpu.step();
        assert_eq!(cpu.bus.read(0x03_1000), 0xEF);
        assert_eq!(cpu.bus.read(0x03_1001), 0xBE);
    }

    #[test]
    fn sta_abs_x_stores_a_indexed() {
        let mut cpu = native16();
        cpu.a = 0x1111;
        cpu.dbr = 0x01;
        cpu.x = 0x0010;
        cpu.bus.load(0x0000, &[0x9D, 0x00, 0x20]); // STA $2000,X
        cpu.step();
        assert_eq!(cpu.bus.read(0x01_2010), 0x11);
        assert_eq!(cpu.bus.read(0x01_2011), 0x11);
    }

    #[test]
    fn sta_abs_y_stores_a_indexed() {
        let mut cpu = native16();
        cpu.a = 0x2222;
        cpu.dbr = 0x02;
        cpu.y = 0x0004;
        cpu.bus.load(0x0000, &[0x99, 0x00, 0x30]); // STA $3000,Y
        cpu.step();
        assert_eq!(cpu.bus.read(0x02_3004), 0x22);
        assert_eq!(cpu.bus.read(0x02_3005), 0x22);
    }

    #[test]
    fn sta_abs_long_stores_a_24bit_addr() {
        let mut cpu = native16();
        cpu.a = 0xCAFE;
        cpu.bus.load(0x0000, &[0x8F, 0x00, 0x40, 0x05]); // STA $054000
        cpu.step();
        assert_eq!(cpu.bus.read(0x05_4000), 0xFE);
        assert_eq!(cpu.bus.read(0x05_4001), 0xCA);
    }

    #[test]
    fn sta_abs_long_x_stores_a_24bit_indexed() {
        let mut cpu = native16();
        cpu.a = 0x1234;
        cpu.x = 0x0002;
        cpu.bus.load(0x0000, &[0x9F, 0x00, 0x50, 0x06]); // STA $065000,X
        cpu.step();
        assert_eq!(cpu.bus.read(0x06_5002), 0x34);
        assert_eq!(cpu.bus.read(0x06_5003), 0x12);
    }

    #[test]
    fn sta_dp_x_ind_stores_via_pointer() {
        let mut cpu = native16();
        cpu.a = 0x5678;
        cpu.d = 0x0200;
        cpu.x = 0x0004;
        cpu.dbr = 0x07;
        cpu.bus.load(0x0214, &[0x56, 0x34]); // pointer $3456 at D+$10+X
        cpu.bus.load(0x0000, &[0x81, 0x10]); // STA ($10,X)
        cpu.step();
        assert_eq!(cpu.bus.read(0x07_3456), 0x78);
        assert_eq!(cpu.bus.read(0x07_3457), 0x56);
    }

    #[test]
    fn sta_dp_ind_y_stores_via_pointer_plus_y() {
        let mut cpu = native16();
        cpu.a = 0xDEAD;
        cpu.d = 0x0200;
        cpu.dbr = 0x04;
        cpu.y = 0x0006;
        cpu.bus.load(0x0210, &[0x00, 0x10]); // pointer $1000
        cpu.bus.load(0x0000, &[0x91, 0x10]); // STA ($10),Y
        cpu.step();
        assert_eq!(cpu.bus.read(0x04_1006), 0xAD);
        assert_eq!(cpu.bus.read(0x04_1007), 0xDE);
    }

    #[test]
    fn sta_dp_ind_stores_via_dp_pointer() {
        let mut cpu = native16();
        cpu.a = 0x9999;
        cpu.d = 0x0200;
        cpu.dbr = 0x06;
        cpu.bus.load(0x0210, &[0x00, 0x30]); // pointer $3000
        cpu.bus.load(0x0000, &[0x92, 0x10]); // STA ($10)
        cpu.step();
        assert_eq!(cpu.bus.read(0x06_3000), 0x99);
        assert_eq!(cpu.bus.read(0x06_3001), 0x99);
    }

    #[test]
    fn sta_sr_stores_stack_relative() {
        let mut cpu = native16();
        cpu.a = 0x3344;
        cpu.s = 0x01F0;
        cpu.bus.load(0x0000, &[0x83, 0x10]); // STA $10,S
        cpu.step();
        assert_eq!(cpu.bus.read(0x0200), 0x44);
        assert_eq!(cpu.bus.read(0x0201), 0x33);
    }

    #[test]
    fn sta_sr_ind_y_stores_via_sr_pointer_plus_y() {
        let mut cpu = native16();
        cpu.a = 0x1122;
        cpu.s = 0x01F0;
        cpu.dbr = 0x02;
        cpu.y = 0x0008;
        cpu.bus.load(0x0200, &[0x00, 0x50]); // ptr $5000
        cpu.bus.load(0x0000, &[0x93, 0x10]); // STA ($10,S),Y
        cpu.step();
        assert_eq!(cpu.bus.read(0x02_5008), 0x22);
        assert_eq!(cpu.bus.read(0x02_5009), 0x11);
    }

    #[test]
    fn sta_dp_ind_long_stores_via_24bit_pointer() {
        let mut cpu = native16();
        cpu.a = 0xABCD;
        cpu.d = 0x0200;
        cpu.bus.load(0x0210, &[0x00, 0x60, 0x05]); // 24-bit ptr $05_6000
        cpu.bus.load(0x0000, &[0x87, 0x10]); // STA [$10]
        cpu.step();
        assert_eq!(cpu.bus.read(0x05_6000), 0xCD);
        assert_eq!(cpu.bus.read(0x05_6001), 0xAB);
    }

    #[test]
    fn sta_dp_ind_long_y_stores_via_24bit_pointer_plus_y() {
        let mut cpu = native16();
        cpu.a = 0x1357;
        cpu.d = 0x0200;
        cpu.y = 0x0004;
        cpu.bus.load(0x0210, &[0x00, 0x70, 0x03]); // ptr $03_7000
        cpu.bus.load(0x0000, &[0x97, 0x10]); // STA [$10],Y
        cpu.step();
        assert_eq!(cpu.bus.read(0x03_7004), 0x57);
        assert_eq!(cpu.bus.read(0x03_7005), 0x13);
    }

    #[test]
    fn sta_does_not_affect_flags() {
        let mut cpu = native16();
        cpu.a = 0x8000;
        cpu.p = 0b0000_0000; // no flags set
        cpu.d = 0x0200;
        cpu.bus.load(0x0000, &[0x85, 0x10]);
        let flags_before = cpu.p;
        cpu.step();
        assert_eq!(cpu.p, flags_before); // STA does not set flags
    }

    // =========================================================================
    // STX — store X index register
    // =========================================================================

    #[test]
    fn stx_dp_stores_x_16bit() {
        let mut cpu = native16();
        cpu.x = 0x1234;
        cpu.d = 0x0300;
        cpu.bus.load(0x0000, &[0x86, 0x10]); // STX $10
        cpu.step();
        assert_eq!(cpu.bus.read(0x0310), 0x34);
        assert_eq!(cpu.bus.read(0x0311), 0x12);
    }

    #[test]
    fn stx_dp_stores_x_8bit() {
        let mut cpu = native8();
        cpu.x = 0x0056;
        cpu.d = 0x0300;
        cpu.bus.load(0x0000, &[0x86, 0x10]); // STX $10
        cpu.step();
        assert_eq!(cpu.bus.read(0x0310), 0x56);
        assert_eq!(cpu.bus.read(0x0311), 0x00); // high byte not written
    }

    #[test]
    fn stx_dp_y_stores_x_indexed() {
        let mut cpu = native16();
        cpu.x = 0xABCD;
        cpu.d = 0x0300;
        cpu.y = 0x0004;
        cpu.bus.load(0x0000, &[0x96, 0x10]); // STX $10,Y
        cpu.step();
        assert_eq!(cpu.bus.read(0x0314), 0xCD);
        assert_eq!(cpu.bus.read(0x0315), 0xAB);
    }

    #[test]
    fn stx_abs_stores_x_using_dbr() {
        let mut cpu = native16();
        cpu.x = 0x5678;
        cpu.dbr = 0x04;
        cpu.bus.load(0x0000, &[0x8E, 0x00, 0x20]); // STX $2000
        cpu.step();
        assert_eq!(cpu.bus.read(0x04_2000), 0x78);
        assert_eq!(cpu.bus.read(0x04_2001), 0x56);
    }

    #[test]
    fn stx_does_not_affect_flags() {
        let mut cpu = native16();
        cpu.x = 0xFFFF;
        cpu.p = 0b0000_0000;
        cpu.bus.load(0x0000, &[0x8E, 0x00, 0x20]);
        let flags_before = cpu.p;
        cpu.step();
        assert_eq!(cpu.p, flags_before);
    }

    // =========================================================================
    // STY — store Y index register
    // =========================================================================

    #[test]
    fn sty_dp_stores_y_16bit() {
        let mut cpu = native16();
        cpu.y = 0xFEDC;
        cpu.d = 0x0400;
        cpu.bus.load(0x0000, &[0x84, 0x20]); // STY $20
        cpu.step();
        assert_eq!(cpu.bus.read(0x0420), 0xDC);
        assert_eq!(cpu.bus.read(0x0421), 0xFE);
    }

    #[test]
    fn sty_dp_x_stores_y_indexed() {
        let mut cpu = native16();
        cpu.y = 0x1111;
        cpu.d = 0x0400;
        cpu.x = 0x0002;
        cpu.bus.load(0x0000, &[0x94, 0x20]); // STY $20,X
        cpu.step();
        assert_eq!(cpu.bus.read(0x0422), 0x11);
        assert_eq!(cpu.bus.read(0x0423), 0x11);
    }

    #[test]
    fn sty_abs_stores_y_using_dbr() {
        let mut cpu = native16();
        cpu.y = 0x9876;
        cpu.dbr = 0x05;
        cpu.bus.load(0x0000, &[0x8C, 0x00, 0x30]); // STY $3000
        cpu.step();
        assert_eq!(cpu.bus.read(0x05_3000), 0x76);
        assert_eq!(cpu.bus.read(0x05_3001), 0x98);
    }

    // =========================================================================
    // STZ — store zero
    // =========================================================================

    #[test]
    fn stz_dp_stores_zero_16bit() {
        let mut cpu = native16();
        cpu.d = 0x0200;
        cpu.bus.load(0x0210, &[0xFF, 0xFF]); // pre-fill with non-zero
        cpu.bus.load(0x0000, &[0x64, 0x10]); // STZ $10
        cpu.step();
        assert_eq!(cpu.bus.read(0x0210), 0x00);
        assert_eq!(cpu.bus.read(0x0211), 0x00);
    }

    #[test]
    fn stz_dp_stores_zero_8bit() {
        let mut cpu = native8();
        cpu.d = 0x0200;
        cpu.bus.load(0x0210, &[0xFF, 0xFF]);
        cpu.bus.load(0x0000, &[0x64, 0x10]); // STZ $10
        cpu.step();
        assert_eq!(cpu.bus.read(0x0210), 0x00);
        assert_eq!(cpu.bus.read(0x0211), 0xFF); // 8-bit: high byte untouched
    }

    #[test]
    fn stz_dp_x_stores_zero_indexed() {
        let mut cpu = native16();
        cpu.d = 0x0200;
        cpu.x = 0x0004;
        cpu.bus.load(0x0000, &[0x74, 0x10]); // STZ $10,X
        cpu.step();
        assert_eq!(cpu.bus.read(0x0214), 0x00);
        assert_eq!(cpu.bus.read(0x0215), 0x00);
    }

    #[test]
    fn stz_abs_stores_zero_at_absolute() {
        let mut cpu = native16();
        cpu.dbr = 0x02;
        cpu.bus.load(0x02_5000, &[0xFF, 0xFF]);
        cpu.bus.load(0x0000, &[0x9C, 0x00, 0x50]); // STZ $5000
        cpu.step();
        assert_eq!(cpu.bus.read(0x02_5000), 0x00);
        assert_eq!(cpu.bus.read(0x02_5001), 0x00);
    }

    #[test]
    fn stz_abs_x_stores_zero_indexed() {
        let mut cpu = native16();
        cpu.dbr = 0x03;
        cpu.x = 0x0010;
        cpu.bus.load(0x0000, &[0x9E, 0x00, 0x60]); // STZ $6000,X
        cpu.step();
        assert_eq!(cpu.bus.read(0x03_6010), 0x00);
        assert_eq!(cpu.bus.read(0x03_6011), 0x00);
    }

    #[test]
    fn stz_does_not_affect_flags() {
        let mut cpu = native16();
        cpu.p = FLAG_NEGATIVE | FLAG_ZERO;
        cpu.bus.load(0x0000, &[0x64, 0x10]);
        let flags_before = cpu.p;
        cpu.step();
        assert_eq!(cpu.p, flags_before);
    }
}

#[cfg(test)]
mod adc_sbc_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn native16() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p &= !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH | FLAG_DECIMAL);
        cpu
    }

    fn native8() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p |= FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
        cpu.p &= !FLAG_DECIMAL;
        cpu
    }

    // =========================================================================
    // ADC immediate, 16-bit
    // =========================================================================

    #[test]
    fn adc_imm_16bit_basic_addition() {
        let mut cpu = native16();
        cpu.a = 0x0001;
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x69, 0x02, 0x00]); // ADC #$0002
        cpu.step();
        assert_eq!(cpu.a, 0x0003);
        assert!(!cpu.flag_c());
        assert!(!cpu.flag_z());
        assert!(!cpu.flag_n());
        assert!(!cpu.flag_v());
    }

    #[test]
    fn adc_imm_16bit_carry_in_adds_one() {
        let mut cpu = native16();
        cpu.a = 0x0001;
        cpu.set_flag_c(true);
        cpu.bus.load(0x0000, &[0x69, 0x01, 0x00]); // ADC #$0001
        cpu.step();
        assert_eq!(cpu.a, 0x0003);
    }

    #[test]
    fn adc_imm_16bit_sets_carry_on_unsigned_overflow() {
        let mut cpu = native16();
        cpu.a = 0xFFFF;
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x69, 0x01, 0x00]); // ADC #$0001
        cpu.step();
        assert_eq!(cpu.a, 0x0000);
        assert!(cpu.flag_c());
        assert!(cpu.flag_z());
    }

    #[test]
    fn adc_imm_16bit_sets_n_flag() {
        // 0x7FFF + 0x0001 = 0x8000
        let mut cpu = native16();
        cpu.a = 0x7FFF;
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x69, 0x01, 0x00]);
        cpu.step();
        assert_eq!(cpu.a, 0x8000);
        assert!(cpu.flag_n());
        assert!(!cpu.flag_c());
    }

    #[test]
    fn adc_imm_16bit_sets_v_flag_positive_overflow() {
        // 0x7FFF + 0x0001 =  positive + positive = negative: overflow0x8000
        let mut cpu = native16();
        cpu.a = 0x7FFF;
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x69, 0x01, 0x00]);
        cpu.step();
        assert!(cpu.flag_v());
    }

    #[test]
    fn adc_imm_16bit_sets_v_flag_negative_overflow() {
        // 0x8000 + 0xFFFF =  negative + negative = positive: overflow0x7FFF
        let mut cpu = native16();
        cpu.a = 0x8000;
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x69, 0xFF, 0xFF]); // ADC #$FFFF
        cpu.step();
        assert_eq!(cpu.a, 0x7FFF);
        assert!(cpu.flag_v());
        assert!(cpu.flag_c());
    }

    #[test]
    fn adc_imm_16bit_no_v_flag_when_no_signed_overflow() {
        let mut cpu = native16();
        cpu.a = 0x0001;
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x69, 0x01, 0x00]);
        cpu.step();
        assert!(!cpu.flag_v());
    }

    // =========================================================================
    // ADC immediate, 8-bit
    // =========================================================================

    #[test]
    fn adc_imm_8bit_basic_addition() {
        let mut cpu = native8();
        cpu.a = 0x0001;
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x69, 0x02]); // ADC #$02
        cpu.step();
        assert_eq!(cpu.a & 0xFF, 0x03);
        assert!(!cpu.flag_c());
    }

    #[test]
    fn adc_imm_8bit_sets_carry_on_unsigned_overflow() {
        let mut cpu = native8();
        cpu.a = 0x00FF;
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x69, 0x01]); // ADC #$01
        cpu.step();
        assert_eq!(cpu.a & 0xFF, 0x00);
        assert!(cpu.flag_c());
        assert!(cpu.flag_z());
    }

    #[test]
    fn adc_imm_8bit_sets_v_flag() {
        // 0x7F + 0x01 =  signed overflow in 8-bit0x80
        let mut cpu = native8();
        cpu.a = 0x007F;
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x69, 0x01]);
        cpu.step();
        assert_eq!(cpu.a & 0xFF, 0x80);
        assert!(cpu.flag_v());
        assert!(cpu.flag_n());
    }

    #[test]
    fn adc_imm_8bit_preserves_b_register() {
        let mut cpu = native8();
        cpu.a = 0x1200; // B=0x12
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x69, 0x05]);
        cpu.step();
        assert_eq!(cpu.a, 0x1205); // B preserved
    }

    // =========================================================================
    // ADC memory modes
    // =========================================================================

    #[test]
    fn adc_dp_reads_from_direct_page() {
        let mut cpu = native16();
        cpu.a = 0x0001;
        cpu.d = 0x0200;
        cpu.set_flag_c(false);
        cpu.bus.load(0x0210, &[0x05, 0x00]);
        cpu.bus.load(0x0000, &[0x65, 0x10]); // ADC $10
        cpu.step();
        assert_eq!(cpu.a, 0x0006);
    }

    #[test]
    fn adc_abs_reads_from_absolute() {
        let mut cpu = native16();
        cpu.a = 0x0100;
        cpu.dbr = 0x01;
        cpu.set_flag_c(false);
        cpu.bus.load(0x01_2000, &[0x00, 0x01]);
        cpu.bus.load(0x0000, &[0x6D, 0x00, 0x20]); // ADC $2000
        cpu.step();
        assert_eq!(cpu.a, 0x0200);
    }

    // =========================================================================
    // SBC immediate, 16-bit
    // =========================================================================

    #[test]
    fn sbc_imm_16bit_basic_subtraction() {
        // A - operand - (1-C). With C=1: A - operand
        let mut cpu = native16();
        cpu.a = 0x0005;
        cpu.set_flag_c(true);
        cpu.bus.load(0x0000, &[0xE9, 0x02, 0x00]); // SBC #$0002
        cpu.step();
        assert_eq!(cpu.a, 0x0003);
        assert!(cpu.flag_c()); // no borrow
        assert!(!cpu.flag_z());
        assert!(!cpu.flag_n());
        assert!(!cpu.flag_v());
    }

    #[test]
    fn sbc_imm_16bit_borrow_clears_carry() {
        // 0x0000 - 0x0001 = 0xFFFF with borrow
        let mut cpu = native16();
        cpu.a = 0x0000;
        cpu.set_flag_c(true); // no borrow in
        cpu.bus.load(0x0000, &[0xE9, 0x01, 0x00]); // SBC #$0001
        cpu.step();
        assert_eq!(cpu.a, 0xFFFF);
        assert!(!cpu.flag_c()); // borrow occurred
        assert!(cpu.flag_n());
    }

    #[test]
    fn sbc_imm_16bit_carry_in_0_subtracts_extra_one() {
        let mut cpu = native16();
        cpu.a = 0x0005;
        cpu.set_flag_c(false); // borrow in: subtract extra 1
        cpu.bus.load(0x0000, &[0xE9, 0x02, 0x00]); // SBC #$0002: 5 - 2 - 1 = 2
        cpu.step();
        assert_eq!(cpu.a, 0x0002);
    }

    #[test]
    fn sbc_imm_16bit_sets_z_flag() {
        let mut cpu = native16();
        cpu.a = 0x0005;
        cpu.set_flag_c(true);
        cpu.bus.load(0x0000, &[0xE9, 0x05, 0x00]); // SBC #$0005
        cpu.step();
        assert_eq!(cpu.a, 0x0000);
        assert!(cpu.flag_z());
        assert!(cpu.flag_c());
    }

    #[test]
    fn sbc_imm_16bit_sets_v_flag_on_signed_overflow() {
        // 0x8000 - 0x0001 = 0x7FFF: negative - positive = positive: overflow
        let mut cpu = native16();
        cpu.a = 0x8000;
        cpu.set_flag_c(true);
        cpu.bus.load(0x0000, &[0xE9, 0x01, 0x00]);
        cpu.step();
        assert_eq!(cpu.a, 0x7FFF);
        assert!(cpu.flag_v());
        assert!(!cpu.flag_n());
    }

    #[test]
    fn sbc_imm_16bit_no_v_flag_no_overflow() {
        let mut cpu = native16();
        cpu.a = 0x0010;
        cpu.set_flag_c(true);
        cpu.bus.load(0x0000, &[0xE9, 0x05, 0x00]);
        cpu.step();
        assert!(!cpu.flag_v());
    }

    // =========================================================================
    // SBC immediate, 8-bit
    // =========================================================================

    #[test]
    fn sbc_imm_8bit_basic() {
        let mut cpu = native8();
        cpu.a = 0x000A;
        cpu.set_flag_c(true);
        cpu.bus.load(0x0000, &[0xE9, 0x03]); // SBC #$03
        cpu.step();
        assert_eq!(cpu.a & 0xFF, 0x07);
        assert!(cpu.flag_c());
    }

    #[test]
    fn sbc_imm_8bit_borrow() {
        // 0x00 - 0x01 = 0xFF with borrow
        let mut cpu = native8();
        cpu.a = 0x0000;
        cpu.set_flag_c(true);
        cpu.bus.load(0x0000, &[0xE9, 0x01]); // SBC #$01
        cpu.step();
        assert_eq!(cpu.a & 0xFF, 0xFF);
        assert!(!cpu.flag_c());
        assert!(cpu.flag_n());
    }

    #[test]
    fn sbc_imm_8bit_sets_v_flag() {
        // 0x80 - 0x01 = 0x7F: negative - positive = positive: overflow
        let mut cpu = native8();
        cpu.a = 0x0080;
        cpu.set_flag_c(true);
        cpu.bus.load(0x0000, &[0xE9, 0x01]);
        cpu.step();
        assert_eq!(cpu.a & 0xFF, 0x7F);
        assert!(cpu.flag_v());
        assert!(!cpu.flag_n());
    }

    #[test]
    fn sbc_imm_8bit_preserves_b_register() {
        let mut cpu = native8();
        cpu.a = 0x120A; // B=0x12
        cpu.set_flag_c(true);
        cpu.bus.load(0x0000, &[0xE9, 0x05]); // SBC #$05
        cpu.step();
        assert_eq!(cpu.a, 0x1205); // B preserved
    }

    // =========================================================================
    // SBC memory modes
    // =========================================================================

    #[test]
    fn sbc_dp_reads_from_direct_page() {
        let mut cpu = native16();
        cpu.a = 0x0010;
        cpu.d = 0x0200;
        cpu.set_flag_c(true);
        cpu.bus.load(0x0210, &[0x04, 0x00]);
        cpu.bus.load(0x0000, &[0xE5, 0x10]); // SBC $10
        cpu.step();
        assert_eq!(cpu.a, 0x000C);
    }

    #[test]
    fn sbc_abs_reads_from_absolute() {
        let mut cpu = native16();
        cpu.a = 0x0100;
        cpu.dbr = 0x01;
        cpu.set_flag_c(true);
        cpu.bus.load(0x01_3000, &[0x50, 0x00]);
        cpu.bus.load(0x0000, &[0xED, 0x00, 0x30]); // SBC $3000
        cpu.step();
        assert_eq!(cpu.a, 0x00B0);
    }
}

#[cfg(test)]
mod and_ora_eor_bit_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn native16() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p &= !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH | FLAG_DECIMAL);
        cpu
    }

    fn native8() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p |= FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
        cpu.p &= !FLAG_DECIMAL;
        cpu
    }

    // =========================================================================
    // AND immediate
    // =========================================================================

    #[test]
    fn and_imm_16bit_basic() {
        let mut cpu = native16();
        cpu.a = 0xFF0F;
        cpu.bus.load(0x0000, &[0x29, 0x0F, 0xF0]); // AND #$F00F
        cpu.step();
        assert_eq!(cpu.a, 0xF00F);
        assert!(!cpu.flag_z());
        assert!(cpu.flag_n());
    }

    #[test]
    fn and_imm_16bit_sets_z_flag() {
        let mut cpu = native16();
        cpu.a = 0xAAAA;
        cpu.bus.load(0x0000, &[0x29, 0x55, 0x55]); // AND #$5555
        cpu.step();
        assert_eq!(cpu.a, 0x0000);
        assert!(cpu.flag_z());
        assert!(!cpu.flag_n());
    }

    #[test]
    fn and_imm_8bit_basic() {
        let mut cpu = native8();
        cpu.a = 0x12FF;
        cpu.bus.load(0x0000, &[0x29, 0x0F]); // AND #$0F
        cpu.step();
        assert_eq!(cpu.a & 0xFF, 0x0F);
        assert_eq!(cpu.a >> 8, 0x12); // B preserved
        assert!(!cpu.flag_n());
    }

    #[test]
    fn and_imm_8bit_sets_z_flag() {
        let mut cpu = native8();
        cpu.a = 0x00AA;
        cpu.bus.load(0x0000, &[0x29, 0x55]); // AND #$55
        cpu.step();
        assert_eq!(cpu.a & 0xFF, 0x00);
        assert!(cpu.flag_z());
    }

    #[test]
    fn and_dp_reads_from_direct_page() {
        let mut cpu = native16();
        cpu.a = 0xFF00;
        cpu.d = 0x0200;
        cpu.bus.load(0x0210, &[0xAA, 0xFF]); // operand = $FFAA
        cpu.bus.load(0x0000, &[0x25, 0x10]); // AND $10
        cpu.step();
        assert_eq!(cpu.a, 0xFF00);
    }

    #[test]
    fn and_abs_reads_from_absolute() {
        let mut cpu = native16();
        cpu.a = 0x0F0F;
        cpu.dbr = 0x00;
        cpu.bus.load(0x1000, &[0xF0, 0x0F]);
        cpu.bus.load(0x0000, &[0x2D, 0x00, 0x10]); // AND $1000
        cpu.step();
        assert_eq!(cpu.a, 0x0F00);
    }

    // =========================================================================
    // ORA immediate
    // =========================================================================

    #[test]
    fn ora_imm_16bit_basic() {
        let mut cpu = native16();
        cpu.a = 0x00FF;
        cpu.bus.load(0x0000, &[0x09, 0x00, 0xFF]); // ORA #$FF00
        cpu.step();
        assert_eq!(cpu.a, 0xFFFF);
        assert!(cpu.flag_n());
        assert!(!cpu.flag_z());
    }

    #[test]
    fn ora_imm_16bit_sets_z_flag() {
        let mut cpu = native16();
        cpu.a = 0x0000;
        cpu.bus.load(0x0000, &[0x09, 0x00, 0x00]); // ORA #$0000
        cpu.step();
        assert_eq!(cpu.a, 0x0000);
        assert!(cpu.flag_z());
    }

    #[test]
    fn ora_imm_8bit_basic() {
        let mut cpu = native8();
        cpu.a = 0x1200;
        cpu.bus.load(0x0000, &[0x09, 0x0F]); // ORA #$0F
        cpu.step();
        assert_eq!(cpu.a & 0xFF, 0x0F);
        assert_eq!(cpu.a >> 8, 0x12); // B preserved
    }

    #[test]
    fn ora_dp_reads_from_direct_page() {
        let mut cpu = native16();
        cpu.a = 0x0000;
        cpu.d = 0x0300;
        cpu.bus.load(0x0310, &[0x34, 0x12]);
        cpu.bus.load(0x0000, &[0x05, 0x10]); // ORA $10
        cpu.step();
        assert_eq!(cpu.a, 0x1234);
    }

    #[test]
    fn ora_abs_reads_from_absolute() {
        let mut cpu = native16();
        cpu.a = 0xFF00;
        cpu.dbr = 0x00;
        cpu.bus.load(0x2000, &[0x00, 0x00]);
        cpu.bus.load(0x0000, &[0x0D, 0x00, 0x20]); // ORA $2000
        cpu.step();
        assert_eq!(cpu.a, 0xFF00);
        assert!(cpu.flag_n());
        assert!(!cpu.flag_z());
    }

    // =========================================================================
    // EOR immediate
    // =========================================================================

    #[test]
    fn eor_imm_16bit_basic() {
        let mut cpu = native16();
        cpu.a = 0xFFFF;
        cpu.bus.load(0x0000, &[0x49, 0xFF, 0xFF]); // EOR #$FFFF
        cpu.step();
        assert_eq!(cpu.a, 0x0000);
        assert!(cpu.flag_z());
        assert!(!cpu.flag_n());
    }

    #[test]
    fn eor_imm_16bit_sets_n_flag() {
        let mut cpu = native16();
        cpu.a = 0x0000;
        cpu.bus.load(0x0000, &[0x49, 0x00, 0x80]); // EOR #$8000
        cpu.step();
        assert_eq!(cpu.a, 0x8000);
        assert!(cpu.flag_n());
        assert!(!cpu.flag_z());
    }

    #[test]
    fn eor_imm_8bit_basic() {
        let mut cpu = native8();
        cpu.a = 0x12AA;
        cpu.bus.load(0x0000, &[0x49, 0xFF]); // EOR #$FF
        cpu.step();
        assert_eq!(cpu.a & 0xFF, 0x55);
        assert_eq!(cpu.a >> 8, 0x12); // B preserved
    }

    #[test]
    fn eor_dp_reads_from_direct_page() {
        let mut cpu = native16();
        cpu.a = 0xFFFF;
        cpu.d = 0x0400;
        cpu.bus.load(0x0410, &[0x0F, 0xF0]);
        cpu.bus.load(0x0000, &[0x45, 0x10]); // EOR $10
        cpu.step();
        assert_eq!(cpu.a, 0x0FF0);
    }

    #[test]
    fn eor_abs_reads_from_absolute() {
        let mut cpu = native16();
        cpu.a = 0x1234;
        cpu.dbr = 0x00;
        cpu.bus.load(0x5000, &[0x34, 0x12]);
        cpu.bus.load(0x0000, &[0x4D, 0x00, 0x50]); // EOR $5000
        cpu.step();
        assert_eq!(cpu.a, 0x0000);
        assert!(cpu.flag_z());
    }

    // =========================================================================
    // BIT
    // =========================================================================

    #[test]
    fn bit_imm_16bit_sets_z_when_and_is_zero() {
        // BIT immediate: only Z flag updated (N and V NOT updated by imm mode)
        let mut cpu = native16();
        cpu.a = 0x00FF;
        cpu.bus.load(0x0000, &[0x89, 0x00, 0xFF]); // BIT #$FF00
        cpu.step();
        assert!(cpu.flag_z()); // A & imm = 0
    }

    #[test]
    fn bit_imm_16bit_clears_z_when_and_nonzero() {
        let mut cpu = native16();
        cpu.a = 0x00FF;
        cpu.bus.load(0x0000, &[0x89, 0xFF, 0x00]); // BIT #$00FF
        cpu.step();
        assert!(!cpu.flag_z());
    }

    #[test]
    fn bit_imm_does_not_affect_n_or_v() {
        // In immediate mode BIT does NOT transfer bits 7/6 to N/V
        let mut cpu = native16();
        cpu.a = 0xFFFF;
        cpu.p &= !(FLAG_NEGATIVE | FLAG_OVERFLOW); // N=0, V=0
        cpu.bus.load(0x0000, &[0x89, 0xFF, 0xFF]); // BIT #$FFFF
        cpu.step();
        assert!(!cpu.flag_n()); // N unchanged
        assert!(!cpu.flag_v()); // V unchanged
    }

    #[test]
    fn bit_dp_16bit_sets_n_v_from_operand() {
        // BIT memory: N = bit15 of operand, V = bit14 of operand (16-bit mode)
        let mut cpu = native16();
        cpu.a = 0xFFFF;
        cpu.d = 0x0500;
        cpu.bus.load(0x0510, &[0x00, 0xC0]); // operand = $C000 => bit15=1, bit14=1
        cpu.bus.load(0x0000, &[0x24, 0x10]); // BIT $10
        cpu.step();
        assert!(cpu.flag_n()); // bit 15 set
        assert!(cpu.flag_v()); // bit 14 set
        assert!(!cpu.flag_z()); // A & $C000 != 0
    }

    #[test]
    fn bit_dp_16bit_sets_z_when_and_is_zero() {
        let mut cpu = native16();
        cpu.a = 0x00FF;
        cpu.d = 0x0500;
        cpu.bus.load(0x0510, &[0x00, 0xFF]); // operand = $FF00
        cpu.bus.load(0x0000, &[0x24, 0x10]); // BIT $10
        cpu.step();
        assert!(cpu.flag_z()); // $00FF & $FF00 = 0
        assert!(cpu.flag_n()); // bit 15 of $FF00 set
        assert!(cpu.flag_v()); // bit 14 of $FF00 set ($FF00 & $4000 != 0)
    }

    #[test]
    fn bit_abs_8bit_sets_n_v_from_operand() {
        // 8-bit mode: N = bit7, V = bit6 of memory operand
        let mut cpu = native8();
        cpu.a = 0x00FF;
        cpu.bus.load(0x1000, &[0xC0]); // operand = $C0 => bit7=1, bit6=1
        cpu.bus.load(0x0000, &[0x2C, 0x00, 0x10]); // BIT $1000
        cpu.step();
        assert!(cpu.flag_n());
        assert!(cpu.flag_v());
        assert!(!cpu.flag_z()); // $FF & $C0 != 0
    }

    #[test]
    fn bit_does_not_change_accumulator() {
        let mut cpu = native16();
        cpu.a = 0x1234;
        cpu.d = 0x0500;
        cpu.bus.load(0x0510, &[0x00, 0x80]); // operand = $8000
        cpu.bus.load(0x0000, &[0x24, 0x10]); // BIT $10
        cpu.step();
        assert_eq!(cpu.a, 0x1234); // A unchanged
    }
}

#[cfg(test)]
mod cmp_cpx_cpy_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn native16() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p &= !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH | FLAG_DECIMAL);
        cpu
    }

    fn native8() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p |= FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
        cpu.p &= !FLAG_DECIMAL;
        cpu
    }

    // =========================================================================
    // CMP  16-bitimmediate
    // =========================================================================

    #[test]
    fn cmp_imm_16bit_equal_sets_z_c_clears_n() {
        let mut cpu = native16();
        cpu.a = 0x0005;
        cpu.bus.load(0x0000, &[0xC9, 0x05, 0x00]); // CMP #$0005
        cpu.step();
        assert!(cpu.flag_z());
        assert!(cpu.flag_c()); // A >= operand
        assert!(!cpu.flag_n());
        assert_eq!(cpu.a, 0x0005); // A unchanged
    }

    #[test]
    fn cmp_imm_16bit_greater_sets_c_clears_z_n() {
        let mut cpu = native16();
        cpu.a = 0x0010;
        cpu.bus.load(0x0000, &[0xC9, 0x05, 0x00]); // CMP #$0005
        cpu.step();
        assert!(!cpu.flag_z());
        assert!(cpu.flag_c()); // A > operand: no borrow
        assert!(!cpu.flag_n());
    }

    #[test]
    fn cmp_imm_16bit_less_clears_c_sets_n() {
        let mut cpu = native16();
        cpu.a = 0x0001;
        cpu.bus.load(0x0000, &[0xC9, 0x05, 0x00]); // CMP #$0005: 1-5 = -4
        cpu.step();
        assert!(!cpu.flag_z());
        assert!(!cpu.flag_c()); // A < operand: borrow
        assert!(cpu.flag_n()); // result is negative
    }

    #[test]
    fn cmp_imm_16bit_sets_n_when_high_bit_set_in_result() {
        // 0x8000 - 0x0001 = 0x7FFF: C=1, N=0
        let mut cpu = native16();
        cpu.a = 0x8000;
        cpu.bus.load(0x0000, &[0xC9, 0x01, 0x00]);
        cpu.step();
        assert!(cpu.flag_c());
        assert!(!cpu.flag_n()); // result 0x7FFF: bit15 = 0
    }

    // =========================================================================
    // CMP  8-bitimmediate
    // =========================================================================

    #[test]
    fn cmp_imm_8bit_equal_sets_z_c() {
        let mut cpu = native8();
        cpu.a = 0x1205; // B=0x12
        cpu.bus.load(0x0000, &[0xC9, 0x05]); // CMP #$05
        cpu.step();
        assert!(cpu.flag_z());
        assert!(cpu.flag_c());
        assert_eq!(cpu.a, 0x1205); // A unchanged
    }

    #[test]
    fn cmp_imm_8bit_less_clears_c() {
        let mut cpu = native8();
        cpu.a = 0x0001;
        cpu.bus.load(0x0000, &[0xC9, 0x05]); // CMP #$05
        cpu.step();
        assert!(!cpu.flag_c());
        assert!(cpu.flag_n());
    }

    // =========================================================================
    // CMP memory modes
    // =========================================================================

    #[test]
    fn cmp_dp_reads_from_direct_page() {
        let mut cpu = native16();
        cpu.a = 0x0010;
        cpu.d = 0x0200;
        cpu.bus.load(0x0210, &[0x10, 0x00]); // operand = $0010
        cpu.bus.load(0x0000, &[0xC5, 0x10]); // CMP $10
        cpu.step();
        assert!(cpu.flag_z());
        assert!(cpu.flag_c());
    }

    #[test]
    fn cmp_abs_reads_from_absolute() {
        let mut cpu = native16();
        cpu.a = 0x0100;
        cpu.dbr = 0x00;
        cpu.bus.load(0x2000, &[0x00, 0x02]); // operand = $0200
        cpu.bus.load(0x0000, &[0xCD, 0x00, 0x20]); // CMP $2000
        cpu.step();
        assert!(!cpu.flag_c()); // 0x0100 < 0x0200
        assert!(cpu.flag_n());
    }

    // =========================================================================
    //  compare X registerCPX
    // =========================================================================

    #[test]
    fn cpx_imm_16bit_equal_sets_z_c() {
        let mut cpu = native16();
        cpu.x = 0x0042;
        cpu.bus.load(0x0000, &[0xE0, 0x42, 0x00]); // CPX #$0042
        cpu.step();
        assert!(cpu.flag_z());
        assert!(cpu.flag_c());
        assert!(!cpu.flag_n());
        assert_eq!(cpu.x, 0x0042); // X unchanged
    }

    #[test]
    fn cpx_imm_16bit_greater_sets_c() {
        let mut cpu = native16();
        cpu.x = 0x0050;
        cpu.bus.load(0x0000, &[0xE0, 0x42, 0x00]); // CPX #$0042
        cpu.step();
        assert!(!cpu.flag_z());
        assert!(cpu.flag_c());
        assert!(!cpu.flag_n());
    }

    #[test]
    fn cpx_imm_16bit_less_clears_c_sets_n() {
        let mut cpu = native16();
        cpu.x = 0x0001;
        cpu.bus.load(0x0000, &[0xE0, 0x05, 0x00]); // CPX #$0005
        cpu.step();
        assert!(!cpu.flag_c());
        assert!(cpu.flag_n());
    }

    #[test]
    fn cpx_imm_8bit_equal_sets_z_c() {
        let mut cpu = native8();
        cpu.x = 0x0042;
        cpu.bus.load(0x0000, &[0xE0, 0x42]); // CPX #$42
        cpu.step();
        assert!(cpu.flag_z());
        assert!(cpu.flag_c());
    }

    #[test]
    fn cpx_dp_reads_from_direct_page() {
        let mut cpu = native16();
        cpu.x = 0x0020;
        cpu.d = 0x0300;
        cpu.bus.load(0x0310, &[0x20, 0x00]);
        cpu.bus.load(0x0000, &[0xE4, 0x10]); // CPX $10
        cpu.step();
        assert!(cpu.flag_z());
        assert!(cpu.flag_c());
    }

    #[test]
    fn cpx_abs_reads_from_absolute() {
        let mut cpu = native16();
        cpu.x = 0x0100;
        cpu.bus.load(0x4000, &[0x00, 0x02]); // operand = $0200
        cpu.bus.load(0x0000, &[0xEC, 0x00, 0x40]); // CPX $4000
        cpu.step();
        assert!(!cpu.flag_c()); // 0x0100 < 0x0200
    }

    // =========================================================================
    //  compare Y registerCPY
    // =========================================================================

    #[test]
    fn cpy_imm_16bit_equal_sets_z_c() {
        let mut cpu = native16();
        cpu.y = 0x00FF;
        cpu.bus.load(0x0000, &[0xC0, 0xFF, 0x00]); // CPY #$00FF
        cpu.step();
        assert!(cpu.flag_z());
        assert!(cpu.flag_c());
        assert_eq!(cpu.y, 0x00FF); // Y unchanged
    }

    #[test]
    fn cpy_imm_16bit_less_clears_c_sets_n() {
        let mut cpu = native16();
        cpu.y = 0x0001;
        cpu.bus.load(0x0000, &[0xC0, 0x05, 0x00]); // CPY #$0005
        cpu.step();
        assert!(!cpu.flag_c());
        assert!(cpu.flag_n());
    }

    #[test]
    fn cpy_imm_8bit_equal_sets_z_c() {
        let mut cpu = native8();
        cpu.y = 0x0077;
        cpu.bus.load(0x0000, &[0xC0, 0x77]); // CPY #$77
        cpu.step();
        assert!(cpu.flag_z());
        assert!(cpu.flag_c());
    }

    #[test]
    fn cpy_dp_reads_from_direct_page() {
        let mut cpu = native16();
        cpu.y = 0x0030;
        cpu.d = 0x0400;
        cpu.bus.load(0x0410, &[0x30, 0x00]);
        cpu.bus.load(0x0000, &[0xC4, 0x10]); // CPY $10
        cpu.step();
        assert!(cpu.flag_z());
        assert!(cpu.flag_c());
    }

    #[test]
    fn cpy_abs_reads_from_absolute() {
        let mut cpu = native16();
        cpu.y = 0x0200;
        cpu.bus.load(0x5000, &[0x00, 0x01]); // operand = $0100
        cpu.bus.load(0x0000, &[0xCC, 0x00, 0x50]); // CPY $5000
        cpu.step();
        assert!(cpu.flag_c()); // 0x0200 > 0x0100
        assert!(!cpu.flag_z());
    }
}

#[cfg(test)]
mod inc_dec_shift_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn native16() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p &= !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH | FLAG_DECIMAL);
        cpu
    }

    fn native8() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p |= FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
        cpu.p &= !FLAG_DECIMAL;
        cpu
    }

    // =========================================================================
    // INC accumulator and memory
    // =========================================================================

    #[test]
    fn inc_acc_16bit_basic() {
        let mut cpu = native16();
        cpu.a = 0x0041;
        cpu.bus.load(0x0000, &[0x1A]); // INC A
        cpu.step();
        assert_eq!(cpu.a, 0x0042);
        assert!(!cpu.flag_z());
        assert!(!cpu.flag_n());
    }

    #[test]
    fn inc_acc_16bit_wraps_to_zero() {
        let mut cpu = native16();
        cpu.a = 0xFFFF;
        cpu.bus.load(0x0000, &[0x1A]);
        cpu.step();
        assert_eq!(cpu.a, 0x0000);
        assert!(cpu.flag_z());
        assert!(!cpu.flag_n());
    }

    #[test]
    fn inc_acc_16bit_sets_n_flag() {
        let mut cpu = native16();
        cpu.a = 0x7FFF;
        cpu.bus.load(0x0000, &[0x1A]);
        cpu.step();
        assert_eq!(cpu.a, 0x8000);
        assert!(cpu.flag_n());
    }

    #[test]
    fn inc_acc_8bit_preserves_b() {
        let mut cpu = native8();
        cpu.a = 0x1200;
        cpu.bus.load(0x0000, &[0x1A]);
        cpu.step();
        assert_eq!(cpu.a, 0x1201); // B preserved
    }

    #[test]
    fn inc_dp_increments_memory() {
        let mut cpu = native16();
        cpu.d = 0x0200;
        cpu.bus.load(0x0210, &[0x05, 0x00]); // value = $0005
        cpu.bus.load(0x0000, &[0xE6, 0x10]); // INC $10
        cpu.step();
        assert_eq!(cpu.bus.read(0x0210), 0x06);
        assert_eq!(cpu.bus.read(0x0211), 0x00);
        assert!(!cpu.flag_z());
    }

    #[test]
    fn inc_abs_increments_memory() {
        let mut cpu = native16();
        cpu.bus.load(0x3000, &[0xFF, 0x00]); // value = $00FF
        cpu.bus.load(0x0000, &[0xEE, 0x00, 0x30]); // INC $3000
        cpu.step();
        assert_eq!(cpu.bus.read(0x3000), 0x00);
        assert_eq!(cpu.bus.read(0x3001), 0x01); // $00FF -> $0100
    }

    // =========================================================================
    // DEC accumulator and memory
    // =========================================================================

    #[test]
    fn dec_acc_16bit_basic() {
        let mut cpu = native16();
        cpu.a = 0x0005;
        cpu.bus.load(0x0000, &[0x3A]); // DEC A
        cpu.step();
        assert_eq!(cpu.a, 0x0004);
        assert!(!cpu.flag_z());
        assert!(!cpu.flag_n());
    }

    #[test]
    fn dec_acc_16bit_wraps() {
        let mut cpu = native16();
        cpu.a = 0x0000;
        cpu.bus.load(0x0000, &[0x3A]);
        cpu.step();
        assert_eq!(cpu.a, 0xFFFF);
        assert!(cpu.flag_n());
        assert!(!cpu.flag_z());
    }

    #[test]
    fn dec_acc_sets_z_flag() {
        let mut cpu = native16();
        cpu.a = 0x0001;
        cpu.bus.load(0x0000, &[0x3A]);
        cpu.step();
        assert_eq!(cpu.a, 0x0000);
        assert!(cpu.flag_z());
    }

    #[test]
    fn dec_dp_decrements_memory() {
        let mut cpu = native16();
        cpu.d = 0x0200;
        cpu.bus.load(0x0210, &[0x00, 0x01]); // value = $0100
        cpu.bus.load(0x0000, &[0xC6, 0x10]); // DEC $10
        cpu.step();
        assert_eq!(cpu.bus.read(0x0210), 0xFF);
        assert_eq!(cpu.bus.read(0x0211), 0x00); // $0100 -> $00FF
    }

    // =========================================================================
    // INX / DEX / INY / DEY
    // =========================================================================

    #[test]
    fn inx_16bit_increments_x() {
        let mut cpu = native16();
        cpu.x = 0x0041;
        cpu.bus.load(0x0000, &[0xE8]); // INX
        cpu.step();
        assert_eq!(cpu.x, 0x0042);
        assert!(!cpu.flag_z());
    }

    #[test]
    fn inx_8bit_wraps_and_sets_z() {
        let mut cpu = native8();
        cpu.x = 0x00FF;
        cpu.bus.load(0x0000, &[0xE8]);
        cpu.step();
        assert_eq!(cpu.x & 0xFF, 0x00);
        assert!(cpu.flag_z());
    }

    #[test]
    fn dex_16bit_decrements_x() {
        let mut cpu = native16();
        cpu.x = 0x0001;
        cpu.bus.load(0x0000, &[0xCA]); // DEX
        cpu.step();
        assert_eq!(cpu.x, 0x0000);
        assert!(cpu.flag_z());
    }

    #[test]
    fn iny_16bit_increments_y() {
        let mut cpu = native16();
        cpu.y = 0x00FF;
        cpu.bus.load(0x0000, &[0xC8]); // INY
        cpu.step();
        assert_eq!(cpu.y, 0x0100);
        assert!(!cpu.flag_z());
    }

    #[test]
    fn dey_16bit_decrements_y() {
        let mut cpu = native16();
        cpu.y = 0x0000;
        cpu.bus.load(0x0000, &[0x88]); // DEY
        cpu.step();
        assert_eq!(cpu.y, 0xFFFF);
        assert!(cpu.flag_n());
    }

    // =========================================================================
    //  arithmetic shift leftASL
    // =========================================================================

    #[test]
    fn asl_acc_16bit_shifts_left() {
        let mut cpu = native16();
        cpu.a = 0x0001;
        cpu.bus.load(0x0000, &[0x0A]); // ASL A
        cpu.step();
        assert_eq!(cpu.a, 0x0002);
        assert!(!cpu.flag_c());
        assert!(!cpu.flag_n());
        assert!(!cpu.flag_z());
    }

    #[test]
    fn asl_acc_16bit_sets_c_from_high_bit() {
        let mut cpu = native16();
        cpu.a = 0x8000;
        cpu.bus.load(0x0000, &[0x0A]);
        cpu.step();
        assert_eq!(cpu.a, 0x0000);
        assert!(cpu.flag_c());
        assert!(cpu.flag_z());
    }

    #[test]
    fn asl_acc_8bit_preserves_b() {
        let mut cpu = native8();
        cpu.a = 0x1201; // B=0x12
        cpu.bus.load(0x0000, &[0x0A]);
        cpu.step();
        assert_eq!(cpu.a & 0xFF, 0x02);
        assert_eq!(cpu.a >> 8, 0x12); // B preserved
        assert!(!cpu.flag_c());
    }

    #[test]
    fn asl_dp_shifts_memory() {
        let mut cpu = native16();
        cpu.d = 0x0200;
        cpu.bus.load(0x0210, &[0x01, 0x00]); // value = $0001
        cpu.bus.load(0x0000, &[0x06, 0x10]); // ASL $10
        cpu.step();
        assert_eq!(cpu.bus.read(0x0210), 0x02);
        assert_eq!(cpu.bus.read(0x0211), 0x00);
    }

    // =========================================================================
    //  logical shift rightLSR
    // =========================================================================

    #[test]
    fn lsr_acc_16bit_shifts_right() {
        let mut cpu = native16();
        cpu.a = 0x0004;
        cpu.bus.load(0x0000, &[0x4A]); // LSR A
        cpu.step();
        assert_eq!(cpu.a, 0x0002);
        assert!(!cpu.flag_c());
        assert!(!cpu.flag_n()); // high bit is always 0 after LSR
    }

    #[test]
    fn lsr_acc_16bit_sets_c_from_low_bit() {
        let mut cpu = native16();
        cpu.a = 0x0001;
        cpu.bus.load(0x0000, &[0x4A]);
        cpu.step();
        assert_eq!(cpu.a, 0x0000);
        assert!(cpu.flag_c());
        assert!(cpu.flag_z());
    }

    #[test]
    fn lsr_dp_shifts_memory() {
        let mut cpu = native16();
        cpu.d = 0x0200;
        cpu.bus.load(0x0210, &[0x04, 0x00]); // value = $0004
        cpu.bus.load(0x0000, &[0x46, 0x10]); // LSR $10
        cpu.step();
        assert_eq!(cpu.bus.read(0x0210), 0x02);
    }

    // =========================================================================
    //  rotate left through carryROL
    // =========================================================================

    #[test]
    fn rol_acc_16bit_rotates_with_carry_in() {
        let mut cpu = native16();
        cpu.a = 0x0001;
        cpu.set_flag_c(true);
        cpu.bus.load(0x0000, &[0x2A]); // ROL A
        cpu.step();
        assert_eq!(cpu.a, 0x0003); // shifted left + carry in
        assert!(!cpu.flag_c()); // old bit 15 = 0
    }

    #[test]
    fn rol_acc_16bit_carries_out_high_bit() {
        let mut cpu = native16();
        cpu.a = 0x8000;
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x2A]);
        cpu.step();
        assert_eq!(cpu.a, 0x0000);
        assert!(cpu.flag_c()); // old bit 15 -> carry
        assert!(cpu.flag_z());
    }

    #[test]
    fn rol_dp_rotates_memory() {
        let mut cpu = native16();
        cpu.d = 0x0200;
        cpu.bus.load(0x0210, &[0x01, 0x00]); // value = $0001
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x26, 0x10]); // ROL $10
        cpu.step();
        assert_eq!(cpu.bus.read(0x0210), 0x02);
    }

    // =========================================================================
    //  rotate right through carryROR
    // =========================================================================

    #[test]
    fn ror_acc_16bit_rotates_with_carry_in() {
        let mut cpu = native16();
        cpu.a = 0x0002;
        cpu.set_flag_c(true);
        cpu.bus.load(0x0000, &[0x6A]); // ROR A
        cpu.step();
        assert_eq!(cpu.a, 0x8001); // carry in -> bit 15; bit 0 was 0 -> no carry out
        assert!(!cpu.flag_c());
        assert!(cpu.flag_n());
    }

    #[test]
    fn ror_acc_16bit_carries_out_low_bit() {
        let mut cpu = native16();
        cpu.a = 0x0001;
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x6A]);
        cpu.step();
        assert_eq!(cpu.a, 0x0000);
        assert!(cpu.flag_c()); // old bit 0 -> carry
        assert!(cpu.flag_z());
    }

    #[test]
    fn ror_dp_rotates_memory() {
        let mut cpu = native16();
        cpu.d = 0x0200;
        cpu.bus.load(0x0210, &[0x04, 0x00]); // value = $0004
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x66, 0x10]); // ROR $10
        cpu.step();
        assert_eq!(cpu.bus.read(0x0210), 0x02);
    }
}

#[cfg(test)]
mod tsb_trb_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn native16() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p &= !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH | FLAG_DECIMAL);
        cpu
    }

    fn native8() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p |= FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
        cpu.p &= !FLAG_DECIMAL;
        cpu
    }

    // =========================================================================
    //  test and set bits: mem |= A; Z = !(A & old_mem)TSB
    // =========================================================================

    #[test]
    fn tsb_dp_16bit_sets_bits_and_clears_z_when_overlap() {
        let mut cpu = native16();
        cpu.a = 0x0FF0;
        cpu.d = 0x0200;
        cpu.bus.load(0x0210, &[0xF0, 0x00]); // mem = $00F0
        cpu.bus.load(0x0000, &[0x04, 0x10]); // TSB $10
        cpu.step();
        // mem = $00F0 | $0FF0 = $0FF0
        assert_eq!(cpu.bus.read(0x0210), 0xF0);
        assert_eq!(cpu.bus.read(0x0211), 0x0F);
        // Z = !($0FF0 & $00F0) = !($00F0) = false (overlap exists -> Z clear)
        assert!(!cpu.flag_z());
        assert_eq!(cpu.a, 0x0FF0); // A unchanged
    }

    #[test]
    fn tsb_dp_16bit_sets_z_when_no_overlap() {
        let mut cpu = native16();
        cpu.a = 0x0F00;
        cpu.d = 0x0200;
        cpu.bus.load(0x0210, &[0xF0, 0x00]); // mem = $00F0
        cpu.bus.load(0x0000, &[0x04, 0x10]); // TSB $10
        cpu.step();
        // Z = !($0F00 & $00F0) = !(0) = true
        assert!(cpu.flag_z());
        // mem = $00F0 | $0F00 = $0FF0
        assert_eq!(cpu.bus.read(0x0210), 0xF0);
        assert_eq!(cpu.bus.read(0x0211), 0x0F);
    }

    #[test]
    fn tsb_dp_8bit_sets_bits() {
        let mut cpu = native8();
        cpu.a = 0x000F; // only low byte matters
        cpu.d = 0x0200;
        cpu.bus.load(0x0210, &[0xF0]);
        cpu.bus.load(0x0000, &[0x04, 0x10]); // TSB $10
        cpu.step();
        assert_eq!(cpu.bus.read(0x0210), 0xFF); // $F0 | $0F
        assert!(cpu.flag_z()); // $0F & $F0 = 0 -> Z set
    }

    #[test]
    fn tsb_abs_16bit_sets_bits() {
        let mut cpu = native16();
        cpu.a = 0x00FF;
        cpu.bus.load(0x3000, &[0xFF, 0x00]); // mem = $00FF
        cpu.bus.load(0x0000, &[0x0C, 0x00, 0x30]); // TSB $3000
        cpu.step();
        assert_eq!(cpu.bus.read(0x3000), 0xFF);
        assert_eq!(cpu.bus.read(0x3001), 0x00);
        assert!(!cpu.flag_z()); // overlap: $00FF & $00FF != 0
    }

    // =========================================================================
    //  test and reset bits: mem &= ~A; Z = !(A & old_mem)TRB
    // =========================================================================

    #[test]
    fn trb_dp_16bit_clears_bits_and_clears_z_when_overlap() {
        let mut cpu = native16();
        cpu.a = 0x00FF;
        cpu.d = 0x0200;
        cpu.bus.load(0x0210, &[0xFF, 0xFF]); // mem = $FFFF
        cpu.bus.load(0x0000, &[0x14, 0x10]); // TRB $10
        cpu.step();
        // mem = $FFFF & ~$00FF = $FF00
        assert_eq!(cpu.bus.read(0x0210), 0x00);
        assert_eq!(cpu.bus.read(0x0211), 0xFF);
        // Z = !($00FF & $FFFF) = !(nonzero) = false
        assert!(!cpu.flag_z());
        assert_eq!(cpu.a, 0x00FF); // A unchanged
    }

    #[test]
    fn trb_dp_16bit_sets_z_when_no_overlap() {
        let mut cpu = native16();
        cpu.a = 0x00FF;
        cpu.d = 0x0200;
        cpu.bus.load(0x0210, &[0x00, 0xFF]); // mem = $FF00
        cpu.bus.load(0x0000, &[0x14, 0x10]); // TRB $10
        cpu.step();
        // Z = !($00FF & $FF00) = !(0) = true
        assert!(cpu.flag_z());
        // mem = $FF00 & ~$00FF = $FF00 (unchanged)
        assert_eq!(cpu.bus.read(0x0210), 0x00);
        assert_eq!(cpu.bus.read(0x0211), 0xFF);
    }

    #[test]
    fn trb_dp_8bit_clears_bits() {
        let mut cpu = native8();
        cpu.a = 0x000F;
        cpu.d = 0x0200;
        cpu.bus.load(0x0210, &[0xFF]);
        cpu.bus.load(0x0000, &[0x14, 0x10]); // TRB $10
        cpu.step();
        assert_eq!(cpu.bus.read(0x0210), 0xF0); // $FF & ~$0F = $F0
        assert!(!cpu.flag_z()); // $0F & $FF != 0
    }

    #[test]
    fn trb_abs_16bit_clears_bits() {
        let mut cpu = native16();
        cpu.a = 0xF0F0;
        cpu.bus.load(0x4000, &[0xFF, 0xFF]); // mem = $FFFF
        cpu.bus.load(0x0000, &[0x1C, 0x00, 0x40]); // TRB $4000
        cpu.step();
        // mem = $FFFF & ~$F0F0 = $0F0F
        assert_eq!(cpu.bus.read(0x4000), 0x0F);
        assert_eq!(cpu.bus.read(0x4001), 0x0F);
        assert!(!cpu.flag_z());
    }
}

#[cfg(test)]
mod branch_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn native16() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p &= !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH | FLAG_DECIMAL);
        cpu
    }

    // =========================================================================
    // BCC /  branch on carry clear / setBCS
    // =========================================================================

    #[test]
    fn bcc_taken_when_carry_clear() {
        let mut cpu = native16();
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x90, 0x04]); // BCC +4
        cpu.step();
        assert_eq!(cpu.pc, 0x0006); // 0x0002 (after fetch) + 4
    }

    #[test]
    fn bcc_not_taken_when_carry_set() {
        let mut cpu = native16();
        cpu.set_flag_c(true);
        cpu.bus.load(0x0000, &[0x90, 0x04]); // BCC +4
        cpu.step();
        assert_eq!(cpu.pc, 0x0002); // not taken
    }

    #[test]
    fn bcs_taken_when_carry_set() {
        let mut cpu = native16();
        cpu.set_flag_c(true);
        cpu.bus.load(0x0000, &[0xB0, 0x06]); // BCS +6
        cpu.step();
        assert_eq!(cpu.pc, 0x0008);
    }

    #[test]
    fn bcs_not_taken_when_carry_clear() {
        let mut cpu = native16();
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0xB0, 0x06]);
        cpu.step();
        assert_eq!(cpu.pc, 0x0002);
    }

    // =========================================================================
    // BEQ /  branch on zero set / clearBNE
    // =========================================================================

    #[test]
    fn beq_taken_when_zero_set() {
        let mut cpu = native16();
        cpu.p |= FLAG_ZERO;
        cpu.bus.load(0x0000, &[0xF0, 0x10]); // BEQ +16
        cpu.step();
        assert_eq!(cpu.pc, 0x0012);
    }

    #[test]
    fn beq_not_taken_when_zero_clear() {
        let mut cpu = native16();
        cpu.p &= !FLAG_ZERO;
        cpu.bus.load(0x0000, &[0xF0, 0x10]);
        cpu.step();
        assert_eq!(cpu.pc, 0x0002);
    }

    #[test]
    fn bne_taken_when_zero_clear() {
        let mut cpu = native16();
        cpu.p &= !FLAG_ZERO;
        cpu.bus.load(0x0000, &[0xD0, 0x08]); // BNE +8
        cpu.step();
        assert_eq!(cpu.pc, 0x000A);
    }

    #[test]
    fn bne_not_taken_when_zero_set() {
        let mut cpu = native16();
        cpu.p |= FLAG_ZERO;
        cpu.bus.load(0x0000, &[0xD0, 0x08]);
        cpu.step();
        assert_eq!(cpu.pc, 0x0002);
    }

    // =========================================================================
    // BMI /  branch on negative set / clearBPL
    // =========================================================================

    #[test]
    fn bmi_taken_when_negative_set() {
        let mut cpu = native16();
        cpu.p |= FLAG_NEGATIVE;
        cpu.bus.load(0x0000, &[0x30, 0x02]); // BMI +2
        cpu.step();
        assert_eq!(cpu.pc, 0x0004);
    }

    #[test]
    fn bpl_taken_when_negative_clear() {
        let mut cpu = native16();
        cpu.p &= !FLAG_NEGATIVE;
        cpu.bus.load(0x0000, &[0x10, 0x05]); // BPL +5
        cpu.step();
        assert_eq!(cpu.pc, 0x0007);
    }

    // =========================================================================
    // BVC /  branch on overflow clear / setBVS
    // =========================================================================

    #[test]
    fn bvc_taken_when_overflow_clear() {
        let mut cpu = native16();
        cpu.p &= !FLAG_OVERFLOW;
        cpu.bus.load(0x0000, &[0x50, 0x03]); // BVC +3
        cpu.step();
        assert_eq!(cpu.pc, 0x0005);
    }

    #[test]
    fn bvs_taken_when_overflow_set() {
        let mut cpu = native16();
        cpu.p |= FLAG_OVERFLOW;
        cpu.bus.load(0x0000, &[0x70, 0x01]); // BVS +1
        cpu.step();
        assert_eq!(cpu.pc, 0x0003);
    }

    // =========================================================================
    // Backward branch (negative offset)
    // =========================================================================

    #[test]
    fn bne_backward_branch() {
        // PC starts at 0x0010; BNE $FE = -2 -> 0x0010
        let mut cpu = native16();
        cpu.pc = 0x0010;
        cpu.p &= !FLAG_ZERO;
        cpu.bus.load(0x0010, &[0xD0, 0xFE]); // BNE -2
        cpu.step();
        assert_eq!(cpu.pc, 0x0010); // 0x0012 + (-2) = 0x0010
    }

    #[test]
    fn beq_backward_branch_big_offset() {
        // PC=0x0020, BEQ 0x80 (-128 signed) -> 0x0020+2+(-128) = 0xFFA2
        let mut cpu = native16();
        cpu.pc = 0x0020;
        cpu.p |= FLAG_ZERO;
        cpu.bus.load(0x0020, &[0xF0, 0x80]); // BEQ -128
        cpu.step();
        assert_eq!(cpu.pc, 0xFFA2);
    }

    // =========================================================================
    //  branch long (16-bit signed offset)BRL
    // =========================================================================

    #[test]
    fn brl_forward_branch() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0x82, 0x00, 0x01]); // BRL +256
        cpu.step();
        assert_eq!(cpu.pc, 0x0103); // 0x0003 + 0x0100
    }

    #[test]
    fn brl_backward_branch() {
        let mut cpu = native16();
        cpu.pc = 0x0100;
        cpu.bus.load(0x0100, &[0x82, 0xFB, 0xFF]); // BRL -5 (0xFFFB = -5 signed)
        cpu.step();
        assert_eq!(cpu.pc, 0x00FE); // 0x0103 + (-5) = 0x00FE
    }
}

// =============================================================================
// Iteration 10: JMP / JSR / RTI / RTS / RTL + stack push/pull ops
// =============================================================================

#[cfg(test)]
mod jmp_jsr_rts_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn native16() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p &= !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);
        cpu
    }

    // =========================================================================
    // JMP absolute (0x4C)
    // =========================================================================

    #[test]
    fn jmp_abs_sets_pc_to_operand() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0x4C, 0x34, 0x12]); // JMP $1234
        cpu.step();
        assert_eq!(cpu.pc, 0x1234);
        assert_eq!(cpu.pbr, 0x00); // PBR unchanged
    }

    // =========================================================================
    // JMP (abs) indirect (0x6C)
    // =========================================================================

    #[test]
    fn jmp_abs_ind_follows_pointer() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0x6C, 0x10, 0x20]); // JMP ($2010)
        cpu.bus.load(0x2010, &[0x78, 0x56]); // pointer -> $5678
        cpu.step();
        assert_eq!(cpu.pc, 0x5678);
    }

    // =========================================================================
    // JMP (abs,X) indexed indirect (0x7C)
    // =========================================================================

    #[test]
    fn jmp_abs_x_ind_uses_x_offset() {
        let mut cpu = native16();
        cpu.x = 4;
        cpu.bus.load(0x0000, &[0x7C, 0x10, 0x20]); // JMP ($2010,X)
        cpu.bus.load(0x2014, &[0xCD, 0xAB]); // pointer -> $ABCD
        cpu.step();
        assert_eq!(cpu.pc, 0xABCD);
    }

    #[test]
    fn jmp_abs_x_ind_wraps_pointer_read_within_program_bank() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.pbr = 0x20;
        cpu.x = 0x00C5;
        cpu.bus.load(0x20113C, &[0x7C, 0x3A, 0xFF]);
        cpu.bus.load(0x20FFFF, &[0x83]);
        cpu.bus.load(0x200000, &[0x7E]);
        cpu.write_pc(0x113C);

        cpu.step();

        assert_eq!(cpu.pbr, 0x20);
        assert_eq!(cpu.pc, 0x7E83);
    }

    // =========================================================================
    // JMP abs_long ( changes PBR0x5C)
    // =========================================================================

    #[test]
    fn jmp_abs_long_sets_pbr_and_pc() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0x5C, 0x00, 0x30, 0x02]); // JMP $02:3000
        cpu.step();
        assert_eq!(cpu.pbr, 0x02);
        assert_eq!(cpu.pc, 0x3000);
    }

    // =========================================================================
    // JMP [abs] indirect long (0xDC)
    // =========================================================================

    #[test]
    fn jmp_abs_ind_long_reads_24bit_pointer() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0xDC, 0x50, 0x00]); // JMP [$0050]
        cpu.bus.load(0x0050, &[0x00, 0x40, 0x03]); // pointer -> bank $03, addr $4000
        cpu.step();
        assert_eq!(cpu.pbr, 0x03);
        assert_eq!(cpu.pc, 0x4000);
    }

    // =========================================================================
    // JSR absolute ( push return_addr-1, jump0x20)
    // =========================================================================

    #[test]
    fn jsr_abs_pushes_return_addr_minus_one_and_jumps() {
        let mut cpu = native16();
        cpu.s = 0x01FF;
        cpu.bus.load(0x0000, &[0x20, 0x00, 0x80]); // JSR $8000
        cpu.step();
        assert_eq!(cpu.pc, 0x8000);
        // Return address on stack = 0x0002 (next instruction) - 1 = 0x0002
        // Pushed high byte first: $00 at $01FF, low byte $02 at $01FE
        assert_eq!(cpu.bus.read(0x01FF), 0x00); // high byte of 0x0002
        assert_eq!(cpu.bus.read(0x01FE), 0x02); // low byte of 0x0002
        assert_eq!(cpu.s, 0x01FD);
    }

    // =========================================================================
    // JSR (abs,X) indexed indirect (0xFC)
    // =========================================================================

    #[test]
    fn jsr_abs_x_ind_pushes_and_jumps_indirect() {
        let mut cpu = native16();
        cpu.s = 0x01FF;
        cpu.x = 2;
        cpu.bus.load(0x0000, &[0xFC, 0x10, 0x20]); // JSR ($2010,X)
        cpu.bus.load(0x2012, &[0x00, 0x60]); // pointer -> $6000
        cpu.step();
        assert_eq!(cpu.pc, 0x6000);
        assert_eq!(cpu.s, 0x01FD);
    }

    #[test]
    fn jsr_abs_x_ind_wraps_pointer_read_within_program_bank() {
        let mut cpu = native16();
        cpu.s = 0xD082;
        cpu.pbr = 0x98;
        cpu.x = 0x0013;
        cpu.bus.load(0x983A3D, &[0xFC, 0xEC, 0xFF]);
        cpu.bus.load(0x98FFFF, &[0x72]);
        cpu.bus.load(0x980000, &[0x4B]);
        cpu.write_pc(0x3A3D);

        cpu.step();

        assert_eq!(cpu.pc, 0x4B72);
        assert_eq!(cpu.s, 0xD080);
    }

    #[test]
    fn jsr_abs_x_ind_return_addr_push_crosses_stack_page_in_emulation_mode() {
        // snes-tests cputest test 0277 (jsr ($FFFF,x), E=1, S=$0100),
        // cross-verified against Mesen2: unlike plain `jsr $8000` (opcode
        // $20), the (abs,X) indirect form's 2-byte return-address push uses
        // a natural, unclamped intermediate stack address that may cross
        // out of page 1, and only clamps the final S back into page 1 once
        // both bytes are written.
        let mut cpu = native16();
        cpu.e = true;
        cpu.s = 0x0100;
        cpu.x = 0x0081;
        cpu.pbr = 0x7E;
        cpu.dbr = 0x7F;
        cpu.write_pc(0x7000);
        cpu.bus.load(0x7E7000, &[0xFC, 0xFF, 0xFF]); // JSR ($FFFF,X)
        cpu.bus.load(0x7E0080, &[0x00, 0x80]); // pointer -> $8000
        cpu.step();

        assert_eq!(cpu.pc, 0x8000);
        // Return addr = $7002: high byte at initial S=$0100, low byte at the
        // natural (unclamped) predecessor $00FF, not $01FF.
        assert_eq!(cpu.bus.read(0x0100), 0x70);
        assert_eq!(cpu.bus.read(0x00FF), 0x02);
        assert_eq!(cpu.s, 0x01FE);
    }

    // =========================================================================
    // JSL abs_long ( push PBR + return_addr-1 (3 bytes), jump0x22)
    // =========================================================================

    #[test]
    fn jsl_abs_long_pushes_pbr_and_addr_minus_one() {
        let mut cpu = native16();
        cpu.s = 0x01FF;
        cpu.pbr = 0x01;
        cpu.bus.load(0x010000, &[0x22, 0x00, 0x40, 0x02]); // JSL $02:4000
        cpu.step();
        assert_eq!(cpu.pbr, 0x02);
        assert_eq!(cpu.pc, 0x4000);
        // 4-byte instruction: return-1 = 0x0003; pushed: PBR(0x01), hi(0x00), lo(0x03)
        assert_eq!(cpu.bus.read(0x01FF), 0x01); // PBR
        assert_eq!(cpu.bus.read(0x01FE), 0x00); // high byte of 0x0003
        assert_eq!(cpu.bus.read(0x01FD), 0x03); // low byte of 0x0003
        assert_eq!(cpu.s, 0x01FC);
    }

    #[test]
    fn jsl_emulation_uses_linear_stack_addresses_then_normalizes_s() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.write_s(0xEC00);
        cpu.pbr = 0xDD;
        cpu.bus.load(0xDD1D9F, &[0x22, 0x77, 0xA3, 0xE4]); // JSL $E4:A377
        cpu.write_pc(0x1D9F);

        cpu.step();

        assert_eq!(cpu.bus.read(0x0100), 0xDD);
        assert_eq!(cpu.bus.read(0x00FF), 0x1D);
        assert_eq!(cpu.bus.read(0x00FE), 0xA2);
        assert_eq!(cpu.s, 0x01FD);
        assert_eq!(cpu.pbr, 0xE4);
        assert_eq!(cpu.pc, 0xA377);
    }

    // =========================================================================
    // RTS ( pull 16-bit addr, add 1, jump within PBR0x60)
    // =========================================================================

    #[test]
    fn rts_pulls_return_addr_and_increments() {
        let mut cpu = native16();
        cpu.s = 0x01FD;
        cpu.bus.load(0x01FE, &[0xFF, 0x3F]); // return addr on stack = $3FFF
        cpu.bus.load(0x0000, &[0x60]); // RTS
        cpu.step();
        assert_eq!(cpu.pc, 0x4000); // $3FFF + 1
        assert_eq!(cpu.s, 0x01FF);
    }

    // =========================================================================
    // RTL ( pull 16-bit addr + PBR, add 10x6B)
    // =========================================================================

    #[test]
    fn rtl_pulls_addr_and_pbr_and_increments() {
        let mut cpu = native16();
        cpu.s = 0x01FC;
        // Stack: lo=$00, hi=$3F, bank=$02 (from low to high addr)
        cpu.bus.load(0x01FD, &[0xFF, 0x3F, 0x02]); // return addr = $3FFF, bank $02
        cpu.bus.load(0x0000, &[0x6B]); // RTL
        cpu.step();
        assert_eq!(cpu.pc, 0x4000); // $3FFF + 1
        assert_eq!(cpu.pbr, 0x02);
        assert_eq!(cpu.s, 0x01FF);
    }

    #[test]
    fn rtl_emulation_matches_6b_e_104_vector() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.bus.load(0x91976B, &[0x6B]);
        cpu.bus.load(0x0200, &[0x3A, 0xFE, 0xD0]);

        cpu.load_state_for_processor_test(
            0xC83D, 0x0040, 0x0031, 0x0B57, 0x13, 0x91, 0x12FF, 0x976B, 0xF5, true,
        );

        assert_eq!(cpu.read_s(), 0x01FF);

        cpu.step();

        assert_eq!(cpu.read_pc(), 0xFE3B);
        assert_eq!(cpu.read_pbr(), 0xD0);
        assert_eq!(cpu.read_s(), 0x0102);
    }

    // =========================================================================
    // RTI ( pull P, pull PC (native: also pull PBR)0x40)
    // =========================================================================

    #[test]
    fn rti_native_pulls_p_pc_pbr() {
        let mut cpu = native16();
        cpu.s = 0x01FB;
        // Stack low-to-high: P=$30, PClo=$00, PChi=$50, PBR=$04
        cpu.bus.load(0x01FC, &[0x30, 0x00, 0x50, 0x04]);
        cpu.bus.load(0x0000, &[0x40]); // RTI
        cpu.step();
        assert_eq!(cpu.p, 0x30);
        assert_eq!(cpu.pc, 0x5000);
        assert_eq!(cpu.pbr, 0x04);
        assert_eq!(cpu.s, 0x01FF);
    }

    #[test]
    fn rti_emulation_pulls_p_and_pc_only() {
        let mut cpu = Cpu::new(TestBus::default()); // emulation mode
        cpu.s = 0x01FC;
        // Stack: P=$20 (no M/X set), PClo=$00, PChi=$60
        // In emulation mode, RTI must force M=1 and X=1 after pulling P
        cpu.bus.load(0x01FD, &[0x20, 0x00, 0x60]);
        cpu.bus.load(0x0000, &[0x40]); // RTI
        cpu.step();
        assert_eq!(cpu.p, 0x20 | FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH); // M=1, X=1 forced
        assert_eq!(cpu.pc, 0x6000);
        assert_eq!(cpu.s, 0x01FF);
    }
}

#[cfg(test)]
mod stack_ops_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn native16() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p &= !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);
        cpu
    }

    fn native8() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p |= FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
        cpu
    }

    // =========================================================================
    // PHA / PLA (0x48 / 0x68)
    // =========================================================================

    #[test]
    fn pha_16bit_pushes_accumulator() {
        let mut cpu = native16();
        cpu.a = 0x1234;
        cpu.s = 0x01FF;
        cpu.bus.load(0x0000, &[0x48]);
        cpu.step();
        assert_eq!(cpu.bus.read(0x01FF), 0x12); // high byte
        assert_eq!(cpu.bus.read(0x01FE), 0x34); // low byte
        assert_eq!(cpu.s, 0x01FD);
    }

    #[test]
    fn pha_8bit_pushes_low_byte_only() {
        let mut cpu = native8();
        cpu.a = 0xBB42;
        cpu.s = 0x01FF;
        cpu.bus.load(0x0000, &[0x48]);
        cpu.step();
        assert_eq!(cpu.bus.read(0x01FF), 0x42); // low byte (A)
        assert_eq!(cpu.s, 0x01FE);
    }

    #[test]
    fn pla_16bit_pulls_accumulator_and_sets_nz() {
        let mut cpu = native16();
        cpu.s = 0x01FD;
        cpu.bus.load(0x01FE, &[0x78, 0x56]); // $5678
        cpu.bus.load(0x0000, &[0x68]);
        cpu.step();
        assert_eq!(cpu.a, 0x5678);
        assert_eq!(cpu.s, 0x01FF);
        assert!(!cpu.flag_z());
        assert!(!cpu.flag_n());
    }

    #[test]
    fn pla_8bit_pulls_one_byte_sets_nz() {
        let mut cpu = native8();
        cpu.s = 0x01FE;
        cpu.bus.load(0x01FF, &[0x80]); // $80 -> negative
        cpu.bus.load(0x0000, &[0x68]);
        cpu.step();
        assert_eq!(cpu.a & 0xFF, 0x80);
        assert_eq!(cpu.s, 0x01FF);
        assert!(cpu.flag_n());
        assert!(!cpu.flag_z());
    }

    // =========================================================================
    // PHX / PLX (0xDA / 0xFA)
    // =========================================================================

    #[test]
    fn phx_16bit_pushes_x() {
        let mut cpu = native16();
        cpu.x = 0xABCD;
        cpu.s = 0x01FF;
        cpu.bus.load(0x0000, &[0xDA]);
        cpu.step();
        assert_eq!(cpu.bus.read(0x01FF), 0xAB);
        assert_eq!(cpu.bus.read(0x01FE), 0xCD);
        assert_eq!(cpu.s, 0x01FD);
    }

    #[test]
    fn plx_16bit_pulls_x_and_sets_nz() {
        let mut cpu = native16();
        cpu.s = 0x01FD;
        cpu.bus.load(0x01FE, &[0x00, 0x00]); // $0000 -> zero
        cpu.bus.load(0x0000, &[0xFA]);
        cpu.step();
        assert_eq!(cpu.x, 0x0000);
        assert_eq!(cpu.s, 0x01FF);
        assert!(cpu.flag_z());
    }

    #[test]
    fn phd_emulation_writes_second_byte_to_00ff() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.d = 0xC825;
        cpu.write_s(0xDD00);
        cpu.bus.load(0x0000, &[0x0B]); // PHD

        cpu.step();

        assert_eq!(cpu.bus.read(0x0100), 0xC8);
        assert_eq!(cpu.bus.read(0x00FF), 0x25);
        assert_eq!(cpu.s, 0x01FE);
    }

    // =========================================================================
    // PHY / PLY (0x5A / 0x7A)
    // =========================================================================

    #[test]
    fn phy_8bit_pushes_y_low_byte() {
        let mut cpu = native8();
        cpu.y = 0x0055;
        cpu.s = 0x01FF;
        cpu.bus.load(0x0000, &[0x5A]);
        cpu.step();
        assert_eq!(cpu.bus.read(0x01FF), 0x55);
        assert_eq!(cpu.s, 0x01FE);
    }

    #[test]
    fn ply_8bit_pulls_y_and_sets_nz() {
        let mut cpu = native8();
        cpu.s = 0x01FE;
        cpu.bus.load(0x01FF, &[0x00]); // zero
        cpu.bus.load(0x0000, &[0x7A]);
        cpu.step();
        assert_eq!(cpu.y & 0xFF, 0x00);
        assert_eq!(cpu.s, 0x01FF);
        assert!(cpu.flag_z());
    }

    // =========================================================================
    // PHP / PLP (0x08 / 0x28)
    // =========================================================================

    #[test]
    fn php_pushes_p() {
        let mut cpu = native16();
        cpu.p = 0b0011_0001; // some flags
        cpu.s = 0x01FF;
        cpu.bus.load(0x0000, &[0x08]);
        cpu.step();
        assert_eq!(cpu.bus.read(0x01FF), 0b0011_0001);
        assert_eq!(cpu.s, 0x01FE);
    }

    #[test]
    fn plp_pulls_p() {
        let mut cpu = native16();
        cpu.s = 0x01FE;
        cpu.bus.load(0x01FF, &[0b1100_0000]); // N=1, V=1
        cpu.bus.load(0x0000, &[0x28]);
        cpu.step();
        assert_eq!(cpu.p, 0b1100_0000);
        assert_eq!(cpu.s, 0x01FF);
    }

    // =========================================================================
    // PHB / PLB (0x8B / 0xAB)
    // =========================================================================

    #[test]
    fn phb_pushes_dbr() {
        let mut cpu = native16();
        cpu.dbr = 0x05;
        cpu.s = 0x01FF;
        cpu.bus.load(0x0000, &[0x8B]);
        cpu.step();
        assert_eq!(cpu.bus.read(0x01FF), 0x05);
        assert_eq!(cpu.s, 0x01FE);
    }

    #[test]
    fn plb_pulls_dbr_and_sets_nz() {
        let mut cpu = native16();
        cpu.s = 0x01FE;
        cpu.bus.load(0x01FF, &[0x80]); // negative
        cpu.bus.load(0x0000, &[0xAB]);
        cpu.step();
        assert_eq!(cpu.dbr, 0x80);
        assert_eq!(cpu.s, 0x01FF);
        assert!(cpu.flag_n());
    }

    #[test]
    fn plb_pull_crosses_stack_page_in_emulation_mode() {
        // snes-tests cputest test 0x3D9 (plb, E=1, S=$01FF), cross-verified
        // against Mesen2: unlike the 6502-heritage single-byte pulls
        // (PLA/PLX/PLY/PLP), PLB is 65816-exclusive and reads its byte from
        // the natural (unclamped) incremented address, only clamping the
        // final S afterward.
        let mut cpu = native16();
        cpu.e = true;
        cpu.s = 0x01FF;
        cpu.bus.load(0x0200, &[0x3D]); // would be $0100 if wrongly clamped first
        cpu.bus.load(0x0000, &[0xAB]); // PLB
        cpu.step();
        assert_eq!(cpu.dbr, 0x3D);
        assert_eq!(cpu.s, 0x0100);
    }

    // =========================================================================
    // PHD / PLD (0x0B / 0x2B)
    // =========================================================================

    #[test]
    fn phd_pushes_d_16bit() {
        let mut cpu = native16();
        cpu.d = 0x1234;
        cpu.s = 0x01FF;
        cpu.bus.load(0x0000, &[0x0B]);
        cpu.step();
        assert_eq!(cpu.bus.read(0x01FF), 0x12);
        assert_eq!(cpu.bus.read(0x01FE), 0x34);
        assert_eq!(cpu.s, 0x01FD);
    }

    #[test]
    fn phd_in_emulation_crosses_0100_to_00ff_and_keeps_s_in_page_1() {
        let mut cpu = native16();
        cpu.e = true;
        cpu.p |= FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
        cpu.d = 0xC825;
        cpu.s = 0x0100;
        cpu.bus.load(0x0000, &[0x0B]);
        cpu.step();

        assert_eq!(cpu.bus.read(0x0100), 0xC8);
        assert_eq!(cpu.bus.read(0x00FF), 0x25);
        assert_eq!(cpu.s, 0x01FE);
    }

    #[test]
    fn pld_pulls_d_and_sets_nz() {
        let mut cpu = native16();
        cpu.s = 0x01FD;
        cpu.bus.load(0x01FE, &[0x00, 0x80]); // $8000 -> negative
        cpu.bus.load(0x0000, &[0x2B]);
        cpu.step();
        assert_eq!(cpu.d, 0x8000);
        assert_eq!(cpu.s, 0x01FF);
        assert!(cpu.flag_n());
    }

    // =========================================================================
    // PHK ( push PBR0x4B)
    // =========================================================================

    #[test]
    fn phk_pushes_pbr() {
        let mut cpu = native16();
        cpu.pbr = 0x03;
        cpu.s = 0x01FF;
        cpu.bus.load(0x030000, &[0x4B]);
        cpu.step();
        assert_eq!(cpu.bus.read(0x01FF), 0x03);
        assert_eq!(cpu.s, 0x01FE);
    }

    // =========================================================================
    // PEA ( push absolute (immediate 16-bit value, no indirection)0xF4)
    // =========================================================================

    #[test]
    fn pea_pushes_immediate_word() {
        let mut cpu = native16();
        cpu.s = 0x01FF;
        cpu.bus.load(0x0000, &[0xF4, 0x34, 0x12]); // PEA $1234
        cpu.step();
        assert_eq!(cpu.bus.read(0x01FF), 0x12); // high byte
        assert_eq!(cpu.bus.read(0x01FE), 0x34); // low byte
        assert_eq!(cpu.s, 0x01FD);
    }

    // =========================================================================
    // PEI ( push effective indirect (DP indirect, no add)0xD4)
    // =========================================================================

    #[test]
    fn pei_pushes_dp_indirect_value() {
        let mut cpu = native16();
        cpu.s = 0x01FF;
        cpu.d = 0x0000;
        cpu.bus.load(0x0000, &[0xD4, 0x10]); // PEI ($10)
        cpu.bus.load(0x0010, &[0x78, 0x56]); // value at DP+$10 = $5678
        cpu.step();
        assert_eq!(cpu.bus.read(0x01FF), 0x56); // high byte
        assert_eq!(cpu.bus.read(0x01FE), 0x78); // low byte
        assert_eq!(cpu.s, 0x01FD);
    }

    #[test]
    fn pei_pointer_read_does_not_wrap_within_dp_page_in_emulation_mode() {
        // snes-tests cputest test 0x3C4 (pei ($ff), E=1, D=$0200, D-low
        // byte zero), cross-verified against Mesen2: unlike the
        // 6502-heritage (dp) indirect opcodes (ADC, AND, CMP, ...), which
        // wrap their pointer's high-byte read within D's page in this
        // scenario, PEI's 65816-only addressing reads the pointer with a
        // plain, unwrapped 16-bit add. The stack push itself also crosses
        // page 1 naturally here, matching PHD/PEA/PER's push16 behavior.
        let mut cpu = native16();
        cpu.e = true;
        cpu.s = 0x0100;
        cpu.d = 0x0200;
        cpu.dbr = 0x7F;
        cpu.bus.load(0x0000, &[0xD4, 0xFF]); // PEI ($FF)
        cpu.bus.load(0x02FF, &[0x54]);
        cpu.bus.load(0x0300, &[0x76]); // would be $0200 if wrongly wrapped
        cpu.step();

        assert_eq!(cpu.bus.read(0x0100), 0x76); // high byte
        assert_eq!(cpu.bus.read(0x00FF), 0x54); // low byte
        assert_eq!(cpu.s, 0x01FE);
    }

    // =========================================================================
    // PER ( push effective relative (PC + signed 16-bit offset)0x62)
    // =========================================================================

    #[test]
    fn per_pushes_pc_plus_offset() {
        let mut cpu = native16();
        cpu.s = 0x01FF;
        cpu.bus.load(0x0000, &[0x62, 0x00, 0x01]); // PER +$0100
        // After fetch PC = 0x0003; effective = 0x0003 + 0x0100 = 0x0103
        cpu.step();
        assert_eq!(cpu.bus.read(0x01FF), 0x01);
        assert_eq!(cpu.bus.read(0x01FE), 0x03);
        assert_eq!(cpu.s, 0x01FD);
    }
}

#[cfg(test)]
mod rep_sep_xce_dispatch_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    // =========================================================================
    // REP ( Reset Processor Status Bits0xC2)
    // =========================================================================

    #[test]
    fn rep_opcode_clears_flags_via_immediate() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false; // native mode
        cpu.p = 0xFF;
        cpu.bus.load(0x0000, &[0xC2, 0x30]); // REP #$ clear M and X30 
        cpu.step();
        assert!(!cpu.m_flag());
        assert!(!cpu.x_flag());
        assert_eq!(cpu.pc, 0x0002);
    }

    #[test]
    fn rep_opcode_takes_2_cycles() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p = 0xFF;
        cpu.bus.load(0x0000, &[0xC2, 0x30]);
        let cycles = cpu.step();
        assert_eq!(cycles, 3);
    }

    // =========================================================================
    // SEP ( Set Processor Status Bits0xE2)
    // =========================================================================

    #[test]
    fn sep_opcode_sets_flags_via_immediate() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p = 0x00;
        cpu.bus.load(0x0000, &[0xE2, 0x30]); // SEP #$ set M and X30 
        cpu.step();
        assert!(cpu.m_flag());
        assert!(cpu.x_flag());
        assert_eq!(cpu.pc, 0x0002);
    }

    #[test]
    fn sep_opcode_takes_3_cycles() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p = 0x00;
        cpu.bus.load(0x0000, &[0xE2, 0x30]);
        let cycles = cpu.step();
        assert_eq!(cycles, 3);
    }

    // =========================================================================
    // XCE ( Exchange Carry with Emulation0xFB)
    // =========================================================================

    #[test]
    fn xce_opcode_switches_to_native_mode() {
        let mut cpu = Cpu::new(TestBus::default()); // emulation mode, C=0
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0xFB]); // XCE: E=1,C=0 -> E=0 (native), C=1
        cpu.step();
        assert!(!cpu.emulation_mode()); // now native mode
        assert!(cpu.flag_c()); // old E=1 now in C
        assert_eq!(cpu.pc, 0x0001);
    }

    #[test]
    fn xce_opcode_takes_2_cycles() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.bus.load(0x0000, &[0xFB]);
        let cycles = cpu.step();
        assert_eq!(cycles, 2);
    }
}

#[cfg(test)]
mod flag_set_clear_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn cpu() -> Cpu<TestBus> {
        let mut c = Cpu::new(TestBus::default());
        c.e = false;
        c
    }

    #[test]
    fn clc_clears_carry() {
        let mut cpu = cpu();
        cpu.set_flag_c(true);
        cpu.bus.load(0x0000, &[0x18]); // CLC
        let cycles = cpu.step();
        assert!(!cpu.flag_c());
        assert_eq!(cycles, 2);
    }

    #[test]
    fn sec_sets_carry() {
        let mut cpu = cpu();
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x38]); // SEC
        let cycles = cpu.step();
        assert!(cpu.flag_c());
        assert_eq!(cycles, 2);
    }

    #[test]
    fn cli_clears_interrupt_disable() {
        let mut cpu = cpu();
        cpu.set_flag_i(true);
        cpu.bus.load(0x0000, &[0x58]); // CLI
        let cycles = cpu.step();
        assert!(!cpu.flag_i());
        assert_eq!(cycles, 2);
    }

    #[test]
    fn sei_sets_interrupt_disable() {
        let mut cpu = cpu();
        cpu.set_flag_i(false);
        cpu.bus.load(0x0000, &[0x78]); // SEI
        let cycles = cpu.step();
        assert!(cpu.flag_i());
        assert_eq!(cycles, 2);
    }

    #[test]
    fn clv_clears_overflow() {
        let mut cpu = cpu();
        cpu.set_flag_v(true);
        cpu.bus.load(0x0000, &[0xB8]); // CLV
        let cycles = cpu.step();
        assert!(!cpu.flag_v());
        assert_eq!(cycles, 2);
    }

    #[test]
    fn cld_clears_decimal() {
        let mut cpu = cpu();
        cpu.set_flag_d(true);
        cpu.bus.load(0x0000, &[0xD8]); // CLD
        let cycles = cpu.step();
        assert!(!cpu.flag_d());
        assert_eq!(cycles, 2);
    }

    #[test]
    fn sed_sets_decimal() {
        let mut cpu = cpu();
        cpu.set_flag_d(false);
        cpu.bus.load(0x0000, &[0xF8]); // SED
        let cycles = cpu.step();
        assert!(cpu.flag_d());
        assert_eq!(cycles, 2);
    }
}

#[cfg(test)]
mod plp_rti_x_zeroing_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    // When PLP pulls a P value that sets X=1 (from X=0),
    // the high bytes of X and Y must be forced to 0.
    #[test]
    fn plp_x_transition_zeros_x_high_byte() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p &= !(FLAG_INDEX_WIDTH); // X=0 (16-bit index)
        cpu.x = 0x1234;
        cpu.y = 0x5678;
        cpu.s = 0x01FE;
        // Pull P with X=1 bit set
        cpu.bus.load(0x01FF, &[FLAG_INDEX_WIDTH]);
        cpu.bus.load(0x0000, &[0x28]); // PLP
        cpu.step();
        assert!(cpu.x_flag());
        assert_eq!(cpu.x, 0x0034); // high byte zeroed
        assert_eq!(cpu.y, 0x0078); // high byte zeroed
    }

    #[test]
    fn plp_no_x_transition_preserves_x() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p |= FLAG_INDEX_WIDTH; // X=1 already
        cpu.x = 0x0034;
        cpu.y = 0x0078;
        cpu.s = 0x01FE;
        cpu.bus.load(0x01FF, &[FLAG_INDEX_WIDTH]);
        cpu.bus.load(0x0000, &[0x28]); // PLP
        cpu.step();
        assert_eq!(cpu.x, 0x0034); // unchanged
        assert_eq!(cpu.y, 0x0078);
    }

    // RTI in native mode: same X-flag zeroing must apply
    #[test]
    fn rti_x_transition_zeros_x_high_byte() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p &= !(FLAG_INDEX_WIDTH); // X=0
        cpu.x = 0xABCD;
        cpu.y = 0x1234;
        cpu.s = 0x01FA;
        // Stack: P (X=1), PClo, PChi, PBR
        cpu.bus.load(0x01FB, &[FLAG_INDEX_WIDTH, 0x00, 0x50, 0x00]);
        cpu.bus.load(0x0000, &[0x40]); // RTI
        cpu.step();
        assert!(cpu.x_flag());
        assert_eq!(cpu.x, 0x00CD); // high byte zeroed
        assert_eq!(cpu.y, 0x0034); // high byte zeroed
    }
}

#[cfg(test)]
mod tcs_fix_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    #[test]
    fn tcs_emulation_mode_clamps_stack_high_byte() {
        // In emulation mode TCS must force S high byte to $01
        let mut cpu = Cpu::new(TestBus::default()); // emulation mode
        cpu.a = 0x0234;
        cpu.bus.load(0x0000, &[0x1B]); // TCS
        cpu.step();
        // write_s() in emulation mode clamps to $01xx
        assert_eq!(cpu.read_s(), 0x0134);
    }

    #[test]
    fn tcs_native_mode_transfers_full_16bit() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.a = 0x0234;
        cpu.bus.load(0x0000, &[0x1B]); // TCS
        cpu.step();
        assert_eq!(cpu.read_s(), 0x0234);
    }
}

/// SNES-specific methods on Cpu<SnesSystemBus> for cartridge RAM operations.
impl Cpu<SnesSystemBus> {
    /// Returns whether the cartridge has battery-backed RAM.
    pub fn has_battery(&self) -> bool {
        self.bus.has_battery()
    }

    /// Returns the SRAM size in bytes.
    pub fn sram_size(&self) -> usize {
        self.bus.sram_size()
    }

    /// Restores SRAM from a byte slice.
    pub fn restore_sram(&mut self, data: &[u8]) {
        self.bus.restore_sram(data);
    }

    /// Returns a snapshot of the current SRAM contents.
    pub fn sram_snapshot(&self) -> Vec<u8> {
        self.bus.sram_snapshot()
    }

    /// Snapshot the PPU's visible framebuffer as packed RGB888.
    pub fn screen_snapshot(&self) -> Vec<u8> {
        self.bus.ppu_screen_snapshot()
    }

    /// Active screen dimensions for the current frame.
    pub fn screen_dimensions(&self) -> (u32, u32) {
        self.bus.screen_dimensions()
    }

    /// Returns and clears the count of PPU vblank entries since the last drain.
    pub fn take_completed_frames(&mut self) -> u32 {
        self.bus.take_ppu_completed_frames()
    }

    /// Set a controller button on the given port (0 = port 1, 1 = port 2).
    pub fn set_controller_button(
        &mut self,
        port: u8,
        button: crate::snes::input::SnesButton,
        pressed: bool,
    ) {
        self.bus.set_controller_button(port, button, pressed);
    }

    /// Bulk-set the 8 NES-convention buttons on the given port.
    pub fn set_joypad_button_states(&mut self, port: u8, state: u8) {
        self.bus.set_joypad_button_states(port, state);
    }

    /// Add relative mouse motion for the given SNES controller port.
    pub fn add_mouse_delta(&mut self, port: u8, dx: i16, dy: i16) {
        self.bus.add_mouse_delta(port, dx, dy);
    }

    /// Set SNES mouse left button state for the given port.
    pub fn set_mouse_left_button(&mut self, port: u8, pressed: bool) {
        self.bus.set_mouse_left_button(port, pressed);
    }

    /// Set SNES mouse right button state for the given port.
    pub fn set_mouse_right_button(&mut self, port: u8, pressed: bool) {
        self.bus.set_mouse_right_button(port, pressed);
    }

    pub fn set_superscope_position(&mut self, port: u8, x: i16, y: i16) {
        self.bus.set_superscope_position(port, x, y);
    }

    pub fn set_superscope_trigger(&mut self, port: u8, pressed: bool) {
        self.bus.set_superscope_trigger(port, pressed);
    }

    pub fn set_superscope_cursor(&mut self, port: u8, pressed: bool) {
        self.bus.set_superscope_cursor(port, pressed);
    }

    pub fn set_superscope_turbo(&mut self, port: u8, pressed: bool) {
        self.bus.set_superscope_turbo(port, pressed);
    }

    pub fn set_superscope_pause(&mut self, port: u8, pressed: bool) {
        self.bus.set_superscope_pause(port, pressed);
    }

    pub fn has_superscope(&self) -> bool {
        self.bus.has_superscope()
    }

    pub fn has_superscope_on_port(&self, port: u8) -> bool {
        self.bus.has_superscope_on_port(port)
    }

    pub fn is_multitap_on_port(&self, port: u8) -> bool {
        self.bus.is_multitap_on_port(port)
    }

    /// Returns true if any SNES controller port currently hosts a mouse.
    pub fn has_mouse(&self) -> bool {
        self.bus.has_mouse()
    }

    /// Returns true if the given physical SNES port currently hosts a mouse.
    pub fn has_mouse_on_port(&self, port: u8) -> bool {
        self.bus.has_mouse_on_port(port)
    }

    /// Configure the device plugged into each controller port.
    pub fn configure_controllers(
        &mut self,
        port1: crate::snes::input::SnesControllerType,
        port2: crate::snes::input::SnesControllerType,
    ) {
        self.bus.configure_controllers(port1, port2);
    }

    /// Return the 8 NES-convention button states for the given port.
    pub fn joypad_button_states(&self, port: u8) -> u8 {
        self.bus.joypad_button_states(port)
    }

    pub(crate) fn capture_save_state(&self) -> SnesSaveState {
        SnesSaveState {
            version: crate::snes::console::save_state::SNES_SAVESTATE_VERSION,
            rom_identity: self.bus.rom_identity(),
            cpu: self.capture_state(),
            bus: self.bus.capture_state(),
            ppu: self.bus.ppu_capture_state(),
        }
    }

    pub(crate) fn restore_save_state(
        &mut self,
        state: &SnesSaveState,
    ) -> Result<(), SnesSaveStateError> {
        let current_rom = self.bus.rom_identity();
        if state.rom_identity != current_rom {
            return Err(SnesSaveStateError::RomMismatch {
                expected: state.rom_identity.clone(),
                found: current_rom,
            });
        }

        self.bus
            .restore_state(&state.bus)
            .map_err(SaveStateError::RestoreFailed)?;
        self.bus
            .ppu_restore_state(&state.ppu)
            .map_err(SaveStateError::RestoreFailed)?;
        self.restore_state(&state.cpu);
        Ok(())
    }
}

impl<B: SnesBus> Stateful for Cpu<B> {
    type State = SnesCpuState;

    fn capture_state(&self) -> SnesCpuState {
        self.capture_state_inner()
    }

    fn restore_state(&mut self, state: &SnesCpuState) {
        self.restore_state_inner(state);
    }
}

#[cfg(test)]
mod branch_cycle_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn emu() -> Cpu<TestBus> {
        // emulation mode (for page-crossing +1 rule)
        Cpu::new(TestBus::default())
    }

    #[test]
    fn cpu_implements_stateful() {
        // The CPU snapshot is captured through the `Stateful` trait.
        fn assert_stateful<T: Stateful>() {}
        assert_stateful::<Cpu<TestBus>>();
    }

    fn native() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p &= !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);
        cpu
    }

    #[test]
    fn branch_not_taken_is_2_cycles() {
        let mut cpu = native();
        cpu.set_flag_c(true); // BCC not taken
        cpu.bus.load(0x0000, &[0x90, 0x04]); // BCC +4
        let cycles = cpu.step();
        assert_eq!(cycles, 2);
    }

    #[test]
    fn branch_taken_same_page_is_3_cycles() {
        let mut cpu = native();
        cpu.set_flag_c(false); // BCC taken
        cpu.bus.load(0x0000, &[0x90, 0x04]); // BCC +4, lands at 0x0006 (same page)
        let cycles = cpu.step();
        assert_eq!(cycles, 3);
    }

    #[test]
    fn branch_taken_page_cross_emulation_is_4_cycles() {
        let mut cpu = emu(); // emulation mode triggers +1 for page crossing
        cpu.pc = 0x00F0;
        cpu.set_flag_c(false); // BCC taken
        // BCC +20 => lands at 0x00F2 + 20 = 0x0106 (crosses page boundary)
        cpu.bus.load(0x00F0, &[0x90, 0x14]);
        let cycles = cpu.step();
        assert_eq!(cycles, 4);
    }

    #[test]
    fn branch_taken_page_cross_native_is_3_cycles() {
        // In native mode, no extra cycle for page crossing
        let mut cpu = native();
        cpu.pc = 0x00F0;
        cpu.set_flag_c(false);
        cpu.bus.load(0x00F0, &[0x90, 0x14]); // BCC +20, lands at 0x0106
        let cycles = cpu.step();
        assert_eq!(cycles, 3);
    }
}

#[cfg(test)]
mod brk_cop_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    // Native mode BRK vector: $00FFE6/$00FFE7
    // Emulation mode IRQ/BRK vector: $00FFFE/$00FFFF

    fn native() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p &= !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);
        cpu
    }

    // =========================================================================
    // BRK ( native mode0x00)
    // Pushes PBR, PC+2, P (with B=1 per 65816), then vectors via $FFE6/$FFE7
    // =========================================================================

    #[test]
    fn brk_native_pushes_pbr_pc_p_and_vectors() {
        let mut cpu = native();
        cpu.pbr = 0x7A;
        cpu.pc = 0x0000;
        cpu.s = 0x01FF;
        // BRK native vector at $FFE6/$FFE7 -> $1234
        cpu.bus.load(0x00FFE6, &[0x34, 0x12]);
        cpu.bus.load(0x0000, &[0x00, 0x00]); // BRK (signature byte at +1)
        cpu.step();
        assert_eq!(cpu.pc, 0x1234);
        assert_eq!(cpu.pbr, 0x00);
        assert_eq!(cpu.s, 0x01FB); // 4 pushes: PBR, PChi, PClo, P
        // Stack: PBR=0x7A at 01FF, PChi=0x00 at 01FE, PClo=0x02 at 01FD, P at 01FC
        assert_eq!(cpu.bus.read(0x01FF), 0x7A); // pushed PBR
        assert_eq!(cpu.bus.read(0x01FE), 0x00); // PC+2 high byte
        assert_eq!(cpu.bus.read(0x01FD), 0x02); // PC+2 low byte
        // P on stack should have I=1 (set after push) -- P pushed before I set
        assert!(cpu.flag_i()); // I set after vectoring
        assert!(!cpu.flag_d()); // D cleared in native mode BRK
    }

    #[test]
    fn brk_native_clears_d_and_sets_i() {
        let mut cpu = native();
        cpu.set_flag_d(true);
        cpu.set_flag_i(false);
        cpu.s = 0x01FF;
        cpu.bus.load(0x00FFE6, &[0x00, 0x20]);
        cpu.bus.load(0x0000, &[0x00, 0x00]);
        cpu.step();
        assert!(cpu.flag_i());
        assert!(!cpu.flag_d());
    }

    // =========================================================================
    // BRK ( emulation mode0x00)
    // Pushes PC+2, P (B flag set), vectors via $FFFE/$FFFF
    // =========================================================================

    #[test]
    fn brk_emulation_pushes_pc_p_and_vectors() {
        let mut cpu = Cpu::new(TestBus::default()); // emulation mode
        cpu.pc = 0x0000;
        cpu.s = 0x01FF;
        // Emulation BRK/IRQ vector at $FFFE -> $5678
        cpu.bus.load(0x00FFFE, &[0x78, 0x56]);
        cpu.bus.load(0x0000, &[0x00, 0x00]); // BRK
        cpu.step();
        assert_eq!(cpu.pc, 0x5678);
        assert_eq!(cpu.s, 0x01FC); // pushed PChi, PClo, P (3 bytes, no PBR in emulation)
        assert_eq!(cpu.bus.read(0x01FF), 0x00); // PChi of PC+2
        assert_eq!(cpu.bus.read(0x01FE), 0x02); // PClo of PC+2
        // P on stack should have B flag set
        assert!(cpu.bus.read(0x01FD) & FLAG_INDEX_WIDTH != 0); // B flag is bit 4 in emulation
        assert!(cpu.flag_i());
    }

    // =========================================================================
    // COP ( native mode0x02)
    // Like BRK but vectors via $FFE4/$FFE5, no B flag
    // =========================================================================

    #[test]
    fn cop_native_pushes_pbr_pc_p_and_vectors() {
        let mut cpu = native();
        cpu.s = 0x01FF;
        cpu.bus.load(0x00FFE4, &[0xCD, 0xAB]); // COP native vector -> $ABCD
        cpu.bus.load(0x0000, &[0x02, 0x00]); // COP (signature byte at +1)
        cpu.step();
        assert_eq!(cpu.pc, 0xABCD);
        assert_eq!(cpu.s, 0x01FB); // 4 pushes: PBR, PChi, PClo, P
        assert!(cpu.flag_i());
        assert!(!cpu.flag_d());
    }
}

#[cfg(test)]
mod mvn_mvp_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn native16() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p &= !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);
        cpu
    }

    // MVN: Move Negative (src_bank, dst_bank) - increments X and Y
    // Opcode: 0x54 dst_bank src_bank
    // Copies A+1 bytes from src_bank:X to dst_bank:Y, incrementing X/Y each
    // After: A=$FFFF, X/Y point past last transferred bytes, DBR=dst_bank

    #[test]
    fn mvn_copies_bytes_from_src_to_dst_bank() {
        let mut cpu = native16();
        // Source at bank $01:$0010, destination at bank $02:$0020
        cpu.bus.load(0x01_0010, &[0xAA, 0xBB, 0xCC]); // 3 bytes to move
        cpu.a = 0x0002; // move 3 bytes (A+1)
        cpu.x = 0x0010; // source offset
        cpu.y = 0x0020; // destination offset
        cpu.bus.load(0x0000, &[0x54, 0x02, 0x01]); // MVN dst=$02, src=$01
        // MVN moves one byte per step(), PC stays at 0x0000 until last byte
        cpu.step(); // byte 1
        cpu.step(); // byte 2
        cpu.step(); // byte 3 (last)
        assert_eq!(cpu.bus.read(0x02_0020), 0xAA);
        assert_eq!(cpu.bus.read(0x02_0021), 0xBB);
        assert_eq!(cpu.bus.read(0x02_0022), 0xCC);
        assert_eq!(cpu.a, 0xFFFF);
        assert_eq!(cpu.x, 0x0013); // 0x0010 + 3
        assert_eq!(cpu.y, 0x0023); // 0x0020 + 3
        assert_eq!(cpu.dbr, 0x02); // DBR set to dst bank
        assert_eq!(cpu.pc, 0x0003); // advanced past the instruction
    }

    // MVP: Move Positive (src_bank, dst_bank) - decrements X and Y
    // Opcode: 0x44 dst_bank src_bank

    #[test]
    fn mvp_copies_bytes_decrementing_xy() {
        let mut cpu = native16();
        // Source at bank $01 starting at $0012 (last byte), destination bank $02 at $0022
        cpu.bus.load(0x01_0010, &[0xAA, 0xBB, 0xCC]); // source bytes
        cpu.a = 0x0002; // move 3 bytes (A+1)
        cpu.x = 0x0012; // source start (high end, MVP decrements)
        cpu.y = 0x0022; // destination start (high end)
        cpu.bus.load(0x0000, &[0x44, 0x02, 0x01]); // MVP dst=$02, src=$01
        cpu.step(); // byte 1
        cpu.step(); // byte 2
        cpu.step(); // byte 3 (last)
        assert_eq!(cpu.bus.read(0x02_0022), 0xCC); // last byte first
        assert_eq!(cpu.bus.read(0x02_0021), 0xBB);
        assert_eq!(cpu.bus.read(0x02_0020), 0xAA);
        assert_eq!(cpu.a, 0xFFFF);
        assert_eq!(cpu.x, 0x000F); // 0x0012 - 3
        assert_eq!(cpu.y, 0x001F); // 0x0022 - 3
        assert_eq!(cpu.dbr, 0x02);
        assert_eq!(cpu.pc, 0x0003);
    }
}

#[cfg(test)]
mod wai_stp_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    // WAI (0xCB): halts CPU until interrupt; for vector conformance,
    // model as a 4-cycle instruction (PC advances, no state change beyond cycles).
    #[test]
    fn wai_advances_pc_and_returns_4_cycles() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.bus.load(0x0000, &[0xCB]);
        let cycles = cpu.step();
        assert_eq!(cpu.pc, 0x0001);
        assert_eq!(cycles, 4);
    }

    // WAI halts the CPU: once executed, subsequent steps must idle (PC frozen)
    // until a hardware interrupt is asserted, then execution resumes.
    #[test]
    fn wai_halts_until_interrupt_then_resumes() {
        let mut cpu = Cpu::new(TestBus::default());
        // WAI followed by NOP.
        cpu.bus.load(0x0000, &[0xCB, 0xEA]);

        // Execute WAI: PC advances past the opcode and the CPU enters wait state.
        cpu.step();
        assert_eq!(cpu.pc, 0x0001);
        assert!(cpu.waiting, "CPU should be waiting after WAI");

        // With no interrupt pending, stepping idles without fetching the next
        // instruction (PC stays frozen).
        for _ in 0..4 {
            cpu.step();
            assert_eq!(cpu.pc, 0x0001, "PC must not advance while waiting");
            assert!(cpu.waiting, "CPU must remain in wait state without an IRQ");
        }

        // Assert an IRQ. The reset I flag masks dispatch, so WAI wakes and simply
        // resumes with the following instruction (NOP), advancing PC.
        cpu.irq_pending = true;
        cpu.step();
        assert!(!cpu.waiting, "IRQ must release the WAI wait state");
        assert_eq!(cpu.pc, 0x0002, "execution should resume after WAI");
    }

    // Hardware runs TWO idle cycles between the wait-loop poll that first
    // sees the interrupt and execution resuming (Mesen ProcessHaltedState:
    // the detecting Idle() completes, and `_waiOver` is sampled before the
    // next Idle(), so one more full idle runs before the CPU is Running).
    // Verified against Mesen via the #2914 boot bus trace (WAI wake was 6
    // master clocks early with a single idle).
    #[test]
    fn wai_wake_adds_two_cycles_before_masked_resume() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.bus.load(0x0000, &[0xCB, 0xEA]); // WAI ; NOP
        assert_eq!(cpu.step(), 4, "WAI opcode timing");
        assert!(cpu.waiting);

        // I=1 after reset: IRQ wakes WAI but does not dispatch.
        cpu.irq_pending = true;
        let cycles = cpu.step();
        assert_eq!(cycles, 4, "wake from WAI adds two idle cycles before NOP");
        assert!(!cpu.waiting);
        assert_eq!(cpu.pc, 0x0002);
    }

    #[test]
    fn wai_wake_adds_two_cycles_before_irq_dispatch() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.pc = 0x8000;
        cpu.s = 0x01FF;
        cpu.set_flag_i(false); // allow IRQ dispatch
        cpu.bus.load(0x008000, &[0xCB]); // WAI
        cpu.bus.load(0x00FFFE, &[0x00, 0x91]); // IRQ vector -> $9100

        assert_eq!(cpu.step(), 4, "WAI opcode timing");
        assert!(cpu.waiting);
        cpu.irq_pending = true;

        let cycles = cpu.step();
        assert_eq!(
            cycles, 9,
            "two wake cycles + emulation IRQ dispatch (7) should total 9"
        );
        assert_eq!(cpu.pc, 0x9100);
        assert!(!cpu.waiting);
    }

    #[test]
    fn wai_waiting_state_round_trips_through_cpu_state() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.bus.load(0x0000, &[0xCB, 0xEA]); // WAI ; NOP
        assert_eq!(cpu.step(), 4);
        assert!(cpu.waiting);

        let state = cpu.capture_state_inner();
        let mut restored = Cpu::new(TestBus::default());
        restored.restore_state_inner(&state);

        assert!(
            restored.waiting,
            "restored CPU must remain in WAI wait state"
        );
        let cycles = restored.step();
        assert_eq!(cycles, 1, "without interrupt, WAI wait should idle");
        assert_eq!(restored.pc, 0x0001, "PC must stay frozen while waiting");
    }

    // STP (0xDB): halts CPU until reset; modeled as 4-cycle instruction.
    #[test]
    fn stp_advances_pc_and_returns_4_cycles() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.bus.load(0x0000, &[0xDB]);
        let cycles = cpu.step();
        assert_eq!(cpu.pc, 0x0001);
        assert_eq!(cycles, 4);
    }
}

#[cfg(test)]
mod decimal_mode_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn native8_decimal() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p |= FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH; // 8-bit
        cpu.set_flag_d(true);
        cpu
    }

    fn native16_decimal() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p &= !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH); // 16-bit
        cpu.set_flag_d(true);
        cpu
    }

    // =========================================================================
    // ADC decimal 8-bit
    // =========================================================================

    #[test]
    fn adc_decimal_8bit_basic_add() {
        // $09 + $01 = $10 (BCD: 9 + 1 = 10)
        let mut cpu = native8_decimal();
        cpu.a = 0x09;
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x69, 0x01]); // ADC #$01
        cpu.step();
        assert_eq!(cpu.a & 0xFF, 0x10);
        assert!(!cpu.flag_c()); // no decimal carry
    }

    #[test]
    fn adc_decimal_8bit_carry_out() {
        // $99 + $01 = $00 with carry (BCD: 99 + 01 = 100)
        let mut cpu = native8_decimal();
        cpu.a = 0x99;
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x69, 0x01]); // ADC #$01
        cpu.step();
        assert_eq!(cpu.a & 0xFF, 0x00);
        assert!(cpu.flag_c());
        assert!(cpu.flag_z());
    }

    #[test]
    fn adc_decimal_8bit_mid_digit_carry() {
        // $19 + $01 = $20 (BCD: 19 + 1 = 20)
        let mut cpu = native8_decimal();
        cpu.a = 0x19;
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x69, 0x01]);
        cpu.step();
        assert_eq!(cpu.a & 0xFF, 0x20);
        assert!(!cpu.flag_c());
    }

    // =========================================================================
    // SBC decimal 8-bit
    // =========================================================================

    #[test]
    fn sbc_decimal_8bit_basic_sub() {
        // $10 - $01 = $09 (BCD: 10 - 1 = 9, borrow=0 -> C=1)
        let mut cpu = native8_decimal();
        cpu.a = 0x10;
        cpu.set_flag_c(true); // no borrow
        cpu.bus.load(0x0000, &[0xE9, 0x01]); // SBC #$01
        cpu.step();
        assert_eq!(cpu.a & 0xFF, 0x09);
        assert!(cpu.flag_c()); // no borrow out
    }

    #[test]
    fn sbc_decimal_8bit_borrow() {
        // $00 - $01 = $99 with borrow (BCD: 00 - 1 = -1 wraps to 99)
        let mut cpu = native8_decimal();
        cpu.a = 0x00;
        cpu.set_flag_c(true); // no borrow in
        cpu.bus.load(0x0000, &[0xE9, 0x01]);
        cpu.step();
        assert_eq!(cpu.a & 0xFF, 0x99);
        assert!(!cpu.flag_c()); // borrow out
    }

    // =========================================================================
    // ADC decimal 16-bit
    // =========================================================================

    #[test]
    fn adc_decimal_16bit_basic_add() {
        // $0099 + $0001 = $0100 (BCD: 99 + 1 = 100)
        let mut cpu = native16_decimal();
        cpu.a = 0x0099;
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x69, 0x01, 0x00]); // ADC #$0001
        cpu.step();
        assert_eq!(cpu.a, 0x0100);
        assert!(!cpu.flag_c());
    }

    #[test]
    fn adc_decimal_16bit_carry_out() {
        // $9999 + $0001 = $0000 with carry
        let mut cpu = native16_decimal();
        cpu.a = 0x9999;
        cpu.set_flag_c(false);
        cpu.bus.load(0x0000, &[0x69, 0x01, 0x00]);
        cpu.step();
        assert_eq!(cpu.a, 0x0000);
        assert!(cpu.flag_c());
        assert!(cpu.flag_z());
    }
}

// =========================================================================
// Cycle accuracy tests: DP penalty, M/X width penalty, abs-idx page-cross
// =========================================================================
#[cfg(test)]
mod cycle_accuracy_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn cpu_native8() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::new());
        cpu.e = false;
        cpu.p = FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH; // M=1, X=1 (8-bit modes)
        cpu.d = 0x0000;
        cpu.dbr = 0x00;
        cpu.pc = 0x0000;
        cpu.pbr = 0x00;
        cpu
    }

    fn cpu_native16() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::new());
        cpu.e = false;
        cpu.p = 0x00; // M=0, X=0 (16-bit modes)
        cpu.d = 0x0000;
        cpu.dbr = 0x00;
        cpu.pc = 0x0000;
        cpu.pbr = 0x00;
        cpu
    }

    // DP cycle penalty (+1 when D low byte != 0)

    #[test]
    fn lda_dp_no_penalty_when_d_low_zero() {
        let mut cpu = cpu_native8();
        cpu.d = 0x0100; // low byte = 0
        cpu.bus.load(0x0100, &[0x55]); // value at DP address
        cpu.bus.load(0x0000, &[0xA5, 0x00]); // LDA $00 (dp)
        let cycles = cpu.step();
        assert_eq!(cycles, 3, "LDA dp base is 3 when D low byte == 0");
    }

    #[test]
    fn lda_dp_plus1_cycle_when_d_low_nonzero() {
        let mut cpu = cpu_native8();
        cpu.d = 0x0101; // low byte = 1 -> +1 cycle
        cpu.bus.load(0x0101, &[0x55]); // value at DP+0
        cpu.bus.load(0x0000, &[0xA5, 0x00]); // LDA $00 (dp)
        let cycles = cpu.step();
        assert_eq!(cycles, 4, "LDA dp adds 1 cycle when D low byte != 0");
    }

    #[test]
    fn lda_dp_x_ind_no_dp_penalty() {
        let mut cpu = cpu_native8();
        cpu.d = 0x0200; // low byte = 0
        cpu.x = 0x00;
        // pointer at $0200: points to $0300
        cpu.bus.load(0x0200, &[0x00, 0x03]);
        cpu.bus.load(0x000300, &[0x42]); // value
        cpu.bus.load(0x0000, &[0xA1, 0x00]); // LDA ($00,X)
        let cycles = cpu.step();
        assert_eq!(cycles, 6, "LDA (dp,X) base is 6 when D low byte == 0");
    }

    #[test]
    fn lda_dp_x_ind_plus1_dp_penalty() {
        let mut cpu = cpu_native8();
        cpu.d = 0x0201; // low byte = 1 -> +1 cycle
        cpu.x = 0x00;
        cpu.bus.load(0x0201, &[0x00, 0x03]);
        cpu.bus.load(0x000300, &[0x42]);
        cpu.bus.load(0x0000, &[0xA1, 0x00]); // LDA ($00,X)
        let cycles = cpu.step();
        assert_eq!(cycles, 7, "LDA (dp,X) adds 1 cycle when D low byte != 0");
    }

    #[test]
    fn lda_dp_ind_y_no_dp_penalty() {
        let mut cpu = cpu_native8();
        cpu.d = 0x0200; // low byte = 0
        cpu.y = 0x01;
        cpu.bus.load(0x0200, &[0x00, 0x03]); // ptr -> $0300
        cpu.bus.load(0x000301, &[0x42]);
        cpu.bus.load(0x0000, &[0xB1, 0x00]); // LDA ($00),Y
        let cycles = cpu.step();
        assert_eq!(
            cycles, 5,
            "LDA (dp),Y base is 5 when D low byte == 0, no page cross"
        );
    }

    #[test]
    fn lda_dp_ind_y_plus1_dp_penalty() {
        let mut cpu = cpu_native8();
        cpu.d = 0x0201; // low byte = 1 -> +1 cycle
        cpu.y = 0x01;
        cpu.bus.load(0x0201, &[0x00, 0x03]); // ptr -> $0300
        cpu.bus.load(0x000301, &[0x42]);
        cpu.bus.load(0x0000, &[0xB1, 0x00]); // LDA ($00),Y
        let cycles = cpu.step();
        assert_eq!(cycles, 6, "LDA (dp),Y adds 1 cycle when D low byte != 0");
    }

    // M-width cycle penalty (+1 when M=0)

    #[test]
    fn lda_abs_no_m_penalty_in_8bit_mode() {
        let mut cpu = cpu_native8();
        cpu.bus.load(0x001000, &[0x42]);
        cpu.bus.load(0x0000, &[0xAD, 0x00, 0x10]); // LDA $1000
        let cycles = cpu.step();
        assert_eq!(cycles, 4, "LDA abs is 4 in 8-bit accumulator mode");
    }

    #[test]
    fn lda_abs_plus1_cycle_in_16bit_mode() {
        let mut cpu = cpu_native16();
        cpu.bus.load(0x001000, &[0x42, 0x00]);
        cpu.bus.load(0x0000, &[0xAD, 0x00, 0x10]); // LDA $1000
        let cycles = cpu.step();
        assert_eq!(
            cycles, 5,
            "LDA abs adds 1 cycle when M=0 (16-bit accumulator)"
        );
    }

    #[test]
    fn sta_abs_no_m_penalty_in_8bit_mode() {
        let mut cpu = cpu_native8();
        cpu.a = 0x55;
        cpu.bus.load(0x0000, &[0x8D, 0x00, 0x10]); // STA $1000
        let cycles = cpu.step();
        assert_eq!(cycles, 4, "STA abs is 4 in 8-bit accumulator mode");
    }

    #[test]
    fn sta_abs_plus1_cycle_in_16bit_mode() {
        let mut cpu = cpu_native16();
        cpu.a = 0x1234;
        cpu.bus.load(0x0000, &[0x8D, 0x00, 0x10]); // STA $1000
        let cycles = cpu.step();
        assert_eq!(
            cycles, 5,
            "STA abs adds 1 cycle when M=0 (16-bit accumulator)"
        );
    }

    #[test]
    fn lda_dp_combined_dp_and_m_penalty() {
        // Both D low != 0 AND M=0: +1 +1 on top of base 3 = 5
        let mut cpu = cpu_native16();
        cpu.d = 0x0101; // low byte = 1
        cpu.bus.load(0x0101, &[0x42, 0x00]);
        cpu.bus.load(0x0000, &[0xA5, 0x00]); // LDA $00 (dp)
        let cycles = cpu.step();
        assert_eq!(cycles, 5, "LDA dp with both D low != 0 and M=0 is 5 cycles");
    }

    // X-width cycle penalty (+1 when X=0)

    #[test]
    fn ldx_dp_no_x_penalty_in_8bit_mode() {
        let mut cpu = cpu_native8();
        cpu.bus.load(0x0010, &[0x42]);
        cpu.bus.load(0x0000, &[0xA6, 0x10]); // LDX $10 (dp)
        let cycles = cpu.step();
        assert_eq!(cycles, 3, "LDX dp is 3 in 8-bit index mode");
    }

    #[test]
    fn ldx_dp_plus1_cycle_in_16bit_mode() {
        let mut cpu = cpu_native16();
        cpu.bus.load(0x0010, &[0x42, 0x00]);
        cpu.bus.load(0x0000, &[0xA6, 0x10]); // LDX $10 (dp)
        let cycles = cpu.step();
        assert_eq!(cycles, 4, "LDX dp adds 1 cycle when X=0 (16-bit index)");
    }

    #[test]
    fn ldy_abs_no_x_penalty_in_8bit_mode() {
        let mut cpu = cpu_native8();
        cpu.bus.load(0x001000, &[0x42]);
        cpu.bus.load(0x0000, &[0xAC, 0x00, 0x10]); // LDY $1000
        let cycles = cpu.step();
        assert_eq!(cycles, 4, "LDY abs is 4 in 8-bit index mode");
    }

    #[test]
    fn ldy_abs_plus1_cycle_in_16bit_mode() {
        let mut cpu = cpu_native16();
        cpu.bus.load(0x001000, &[0x42, 0x00]);
        cpu.bus.load(0x0000, &[0xAC, 0x00, 0x10]); // LDY $1000
        let cycles = cpu.step();
        assert_eq!(cycles, 5, "LDY abs adds 1 cycle when X=0 (16-bit index)");
    }

    // Absolute indexed page-crossing penalty (+1 for read when crosses page)

    #[test]
    fn lda_abs_x_no_page_cross() {
        let mut cpu = cpu_native8();
        cpu.x = 0x01;
        // $00FE + 1 = $00FF (same page, no cross)
        cpu.bus.load(0x0000FF, &[0x42]);
        cpu.bus.load(0x0000, &[0xBD, 0xFE, 0x00]); // LDA $00FE,X
        let cycles = cpu.step();
        assert_eq!(cycles, 4, "LDA abs,X no page cross is 4 cycles");
    }

    #[test]
    fn lda_abs_x_page_cross_adds_cycle() {
        let mut cpu = cpu_native8();
        cpu.x = 0x03;
        // $00FE + 3 = $0101 (crosses from page $00 to page $01)
        cpu.bus.load(0x000101, &[0x42]);
        cpu.bus.load(0x0000, &[0xBD, 0xFE, 0x00]); // LDA $00FE,X
        let cycles = cpu.step();
        assert_eq!(cycles, 5, "LDA abs,X page cross adds 1 cycle");
    }

    #[test]
    fn lda_abs_y_no_page_cross() {
        let mut cpu = cpu_native8();
        cpu.y = 0x01;
        cpu.bus.load(0x0000FF, &[0x42]);
        cpu.bus.load(0x0000, &[0xB9, 0xFE, 0x00]); // LDA $00FE,Y
        let cycles = cpu.step();
        assert_eq!(cycles, 4, "LDA abs,Y no page cross is 4 cycles");
    }

    #[test]
    fn lda_abs_y_page_cross_adds_cycle() {
        let mut cpu = cpu_native8();
        cpu.y = 0x03;
        cpu.bus.load(0x000101, &[0x42]);
        cpu.bus.load(0x0000, &[0xB9, 0xFE, 0x00]); // LDA $00FE,Y
        let cycles = cpu.step();
        assert_eq!(cycles, 5, "LDA abs,Y page cross adds 1 cycle");
    }

    #[test]
    fn lda_dp_ind_y_no_page_cross() {
        let mut cpu = cpu_native8();
        cpu.d = 0x0200;
        cpu.y = 0x01;
        // ptr at $0200 = $00FE -> EA = $00FE + 1 = $00FF (no cross)
        cpu.bus.load(0x0200, &[0xFE, 0x00]);
        cpu.bus.load(0x0000FF, &[0x42]);
        cpu.bus.load(0x0000, &[0xB1, 0x00]); // LDA ($00),Y
        let cycles = cpu.step();
        assert_eq!(cycles, 5, "LDA (dp),Y no page cross is 5 cycles");
    }

    #[test]
    fn lda_dp_ind_y_page_cross_adds_cycle() {
        let mut cpu = cpu_native8();
        cpu.d = 0x0200;
        cpu.y = 0x03;
        // ptr at $0200 = $00FE -> EA = $00FE + 3 = $0101 (cross $00->$01)
        cpu.bus.load(0x0200, &[0xFE, 0x00]);
        cpu.bus.load(0x000101, &[0x42]);
        cpu.bus.load(0x0000, &[0xB1, 0x00]); // LDA ($00),Y
        let cycles = cpu.step();
        assert_eq!(cycles, 6, "LDA (dp),Y page cross adds 1 cycle");
    }
}

#[cfg(test)]
mod wdm_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    #[test]
    fn wdm_advances_pc_by_2_and_returns_2_cycles() {
        let mut cpu = Cpu::new(TestBus::new());
        cpu.e = false;
        cpu.pc = 0x0000;
        cpu.pbr = 0x00;
        cpu.bus.load(0x0000, &[0x42, 0xAB]); // WDM $AB
        let cycles = cpu.step();
        assert_eq!(cycles, 2, "WDM is 2 cycles");
        assert_eq!(cpu.pc, 0x0002, "WDM advances PC by 2 (opcode + operand)");
    }

    #[test]
    fn wdm_does_not_alter_flags() {
        let mut cpu = Cpu::new(TestBus::new());
        cpu.e = false;
        cpu.pc = 0x0000;
        cpu.pbr = 0x00;
        let p_before = 0b1010_1010u8;
        cpu.p = p_before;
        cpu.bus.load(0x0000, &[0x42, 0x00]);
        cpu.step();
        assert_eq!(cpu.p, p_before, "WDM must not change any flags");
    }
}

#[cfg(test)]
mod imm_width_cycle_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn native8() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::new());
        cpu.e = false;
        cpu.p = FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
        cpu.pc = 0x0000;
        cpu.pbr = 0x00;
        cpu
    }

    fn native16() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::new());
        cpu.e = false;
        cpu.p = 0x00; // M=0, X=0
        cpu.pc = 0x0000;
        cpu.pbr = 0x00;
        cpu
    }

    // M-width ops: LDA, ADC, SBC, AND, ORA, EOR, BIT imm, CMP imm

    #[test]
    fn lda_imm_8bit_is_2_cycles() {
        let mut cpu = native8();
        cpu.bus.load(0x0000, &[0xA9, 0x42]);
        assert_eq!(cpu.step(), 2);
    }

    #[test]
    fn lda_imm_16bit_is_3_cycles() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0xA9, 0x42, 0x00]);
        assert_eq!(cpu.step(), 3, "LDA # adds 1 cycle when M=0");
    }

    #[test]
    fn adc_imm_16bit_is_3_cycles() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0x69, 0x01, 0x00]);
        assert_eq!(cpu.step(), 3, "ADC # adds 1 cycle when M=0");
    }

    #[test]
    fn sbc_imm_16bit_is_3_cycles() {
        let mut cpu = native16();
        cpu.set_flag_c(true);
        cpu.bus.load(0x0000, &[0xE9, 0x01, 0x00]);
        assert_eq!(cpu.step(), 3, "SBC # adds 1 cycle when M=0");
    }

    #[test]
    fn and_imm_16bit_is_3_cycles() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0x29, 0xFF, 0xFF]);
        assert_eq!(cpu.step(), 3, "AND # adds 1 cycle when M=0");
    }

    #[test]
    fn ora_imm_16bit_is_3_cycles() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0x09, 0x00, 0x00]);
        assert_eq!(cpu.step(), 3, "ORA # adds 1 cycle when M=0");
    }

    #[test]
    fn eor_imm_16bit_is_3_cycles() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0x49, 0xFF, 0xFF]);
        assert_eq!(cpu.step(), 3, "EOR # adds 1 cycle when M=0");
    }

    #[test]
    fn cmp_imm_16bit_is_3_cycles() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0xC9, 0x00, 0x00]);
        assert_eq!(cpu.step(), 3, "CMP # adds 1 cycle when M=0");
    }

    #[test]
    fn bit_imm_16bit_is_3_cycles() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0x89, 0xFF, 0xFF]);
        assert_eq!(cpu.step(), 3, "BIT # adds 1 cycle when M=0");
    }

    // X-width ops: LDX, LDY, CPX, CPY imm

    #[test]
    fn ldx_imm_8bit_is_2_cycles() {
        let mut cpu = native8();
        cpu.bus.load(0x0000, &[0xA2, 0x42]);
        assert_eq!(cpu.step(), 2);
    }

    #[test]
    fn ldx_imm_16bit_is_3_cycles() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0xA2, 0x42, 0x00]);
        assert_eq!(cpu.step(), 3, "LDX # adds 1 cycle when X=0");
    }

    #[test]
    fn ldy_imm_16bit_is_3_cycles() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0xA0, 0x42, 0x00]);
        assert_eq!(cpu.step(), 3, "LDY # adds 1 cycle when X=0");
    }

    #[test]
    fn cpx_imm_16bit_is_3_cycles() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0xE0, 0x00, 0x00]);
        assert_eq!(cpu.step(), 3, "CPX # adds 1 cycle when X=0");
    }

    #[test]
    fn cpy_imm_16bit_is_3_cycles() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0xC0, 0x00, 0x00]);
        assert_eq!(cpu.step(), 3, "CPY # adds 1 cycle when X=0");
    }
}

#[cfg(test)]
mod stack_width_cycle_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn native8() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::new());
        cpu.e = false;
        cpu.p = FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
        cpu.s = 0x01FF;
        cpu.pc = 0x0000;
        cpu.pbr = 0x00;
        cpu
    }

    fn native16() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::new());
        cpu.e = false;
        cpu.p = 0x00;
        cpu.s = 0x01FF;
        cpu.pc = 0x0000;
        cpu.pbr = 0x00;
        cpu
    }

    #[test]
    fn pha_8bit_is_3_cycles() {
        let mut cpu = native8();
        cpu.bus.load(0x0000, &[0x48]); // PHA
        assert_eq!(cpu.step(), 3);
    }

    #[test]
    fn pha_16bit_is_4_cycles() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0x48]); // PHA
        assert_eq!(cpu.step(), 4, "PHA adds 1 cycle when M=0");
    }

    #[test]
    fn pla_8bit_is_4_cycles() {
        let mut cpu = native8();
        cpu.s = 0x01FE; // pre-decremented (one value on stack)
        cpu.bus.load(0x01FF, &[0x42]); // value to pull
        cpu.bus.load(0x0000, &[0x68]); // PLA
        assert_eq!(cpu.step(), 4);
    }

    #[test]
    fn pla_16bit_is_5_cycles() {
        let mut cpu = native16();
        cpu.s = 0x01FD;
        cpu.bus.load(0x01FE, &[0x42, 0x00]);
        cpu.bus.load(0x0000, &[0x68]); // PLA
        assert_eq!(cpu.step(), 5, "PLA adds 1 cycle when M=0");
    }

    #[test]
    fn phx_8bit_is_3_cycles() {
        let mut cpu = native8();
        cpu.bus.load(0x0000, &[0xDA]); // PHX
        assert_eq!(cpu.step(), 3);
    }

    #[test]
    fn phx_16bit_is_4_cycles() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0xDA]); // PHX
        assert_eq!(cpu.step(), 4, "PHX adds 1 cycle when X=0");
    }

    #[test]
    fn plx_8bit_is_4_cycles() {
        let mut cpu = native8();
        cpu.s = 0x01FE;
        cpu.bus.load(0x01FF, &[0x42]);
        cpu.bus.load(0x0000, &[0xFA]); // PLX
        assert_eq!(cpu.step(), 4);
    }

    #[test]
    fn plx_16bit_is_5_cycles() {
        let mut cpu = native16();
        cpu.s = 0x01FD;
        cpu.bus.load(0x01FE, &[0x42, 0x00]);
        cpu.bus.load(0x0000, &[0xFA]); // PLX
        assert_eq!(cpu.step(), 5, "PLX adds 1 cycle when X=0");
    }

    #[test]
    fn phy_8bit_is_3_cycles() {
        let mut cpu = native8();
        cpu.bus.load(0x0000, &[0x5A]); // PHY
        assert_eq!(cpu.step(), 3);
    }

    #[test]
    fn phy_16bit_is_4_cycles() {
        let mut cpu = native16();
        cpu.bus.load(0x0000, &[0x5A]); // PHY
        assert_eq!(cpu.step(), 4, "PHY adds 1 cycle when X=0");
    }

    #[test]
    fn ply_8bit_is_4_cycles() {
        let mut cpu = native8();
        cpu.s = 0x01FE;
        cpu.bus.load(0x01FF, &[0x42]);
        cpu.bus.load(0x0000, &[0x7A]); // PLY
        assert_eq!(cpu.step(), 4);
    }

    #[test]
    fn ply_16bit_is_5_cycles() {
        let mut cpu = native16();
        cpu.s = 0x01FD;
        cpu.bus.load(0x01FE, &[0x42, 0x00]);
        cpu.bus.load(0x0000, &[0x7A]); // PLY
        assert_eq!(cpu.step(), 5, "PLY adds 1 cycle when X=0");
    }
}

#[cfg(test)]
mod abs_idx_x0_cycle_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn native8_x16() -> Cpu<TestBus> {
        // M=1 (8-bit acc), X=0 (16-bit index)
        let mut cpu = Cpu::new(TestBus::new());
        cpu.e = false;
        cpu.p = FLAG_ACCUM_WIDTH; // M=1, X=0
        cpu.dbr = 0x00;
        cpu.pc = 0x0000;
        cpu.pbr = 0x00;
        cpu
    }

    #[test]
    fn lda_abs_x_no_page_cross_but_x16_adds_cycle() {
        // X=0 (16-bit), no page crossing -> still +1 cycle
        let mut cpu = native8_x16();
        cpu.x = 0x0001; // $0010 + $0001 = $0011 (same page)
        cpu.bus.load(0x000011, &[0x42]);
        cpu.bus.load(0x0000, &[0xBD, 0x10, 0x00]); // LDA $0010,X
        assert_eq!(
            cpu.step(),
            5,
            "LDA abs,X: +1 when X=0 even without page cross"
        );
    }

    #[test]
    fn lda_abs_y_no_page_cross_but_x16_adds_cycle() {
        let mut cpu = native8_x16();
        cpu.y = 0x0001;
        cpu.bus.load(0x000011, &[0x42]);
        cpu.bus.load(0x0000, &[0xB9, 0x10, 0x00]); // LDA $0010,Y
        assert_eq!(
            cpu.step(),
            5,
            "LDA abs,Y: +1 when X=0 even without page cross"
        );
    }

    #[test]
    fn lda_abs_x_page_cross_x16_still_only_adds_one_cycle() {
        // X=0, page crosses: should still be exactly +1 (not +2)
        let mut cpu = native8_x16();
        cpu.x = 0x0003;
        cpu.bus.load(0x000101, &[0x42]);
        cpu.bus.load(0x0000, &[0xBD, 0xFE, 0x00]); // LDA $00FE,X -> $0101
        assert_eq!(
            cpu.step(),
            5,
            "LDA abs,X: only +1 cycle when both X=0 and page cross"
        );
    }

    #[test]
    fn lda_dp_ind_y_no_page_cross_but_x16_adds_cycle() {
        let mut cpu = native8_x16();
        cpu.d = 0x0200;
        cpu.y = 0x0001;
        cpu.bus.load(0x0200, &[0x10, 0x00]); // ptr = $0010
        cpu.bus.load(0x000011, &[0x42]);
        cpu.bus.load(0x0000, &[0xB1, 0x00]); // LDA ($00),Y
        assert_eq!(
            cpu.step(),
            6,
            "LDA (dp),Y: +1 when X=0 even without page cross"
        );
    }

    #[test]
    fn adc_abs_x_no_page_cross_8bit_index_no_x0_penalty() {
        // M=1, X=1 (8-bit both): no page cross, no X=0 penalty -> 4 cycles
        let mut cpu = Cpu::new(TestBus::new());
        cpu.e = false;
        cpu.p = FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
        cpu.x = 0x01;
        cpu.bus.load(0x000011, &[0x01]);
        cpu.bus.load(0x0000, &[0x7D, 0x10, 0x00]); // ADC $0010,X
        assert_eq!(
            cpu.step(),
            4,
            "ADC abs,X: no penalty without page cross and X=1"
        );
    }
}

#[cfg(test)]
mod rti_cycle_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    #[test]
    fn rti_emulation_mode_is_6_cycles() {
        let mut cpu = Cpu::new(TestBus::new());
        cpu.e = true;
        cpu.s = 0x01FC; // 3 bytes on stack: P, PCL, PCH
        cpu.bus.load(0x01FD, &[0x00, 0x30, 0x00]); // P=0, PCL=$30, PCH=$00
        cpu.bus.load(0x0000, &[0x40]); // RTI
        assert_eq!(cpu.step(), 6, "RTI emulation mode is 6 cycles");
    }

    #[test]
    fn rti_native_mode_is_7_cycles() {
        let mut cpu = Cpu::new(TestBus::new());
        cpu.e = false;
        cpu.p = FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
        cpu.s = 0x01FB; // 4 bytes on stack: P, PCL, PCH, PBR
        cpu.bus.load(0x01FC, &[0x20, 0x30, 0x00, 0x00]); // P=$20, PCL=$30, PCH=$00, PBR=$00
        cpu.bus.load(0x0000, &[0x40]); // RTI
        assert_eq!(cpu.step(), 7, "RTI native mode is 7 cycles (pulls PBR too)");
    }
}

#[cfg(test)]
mod sbc_decimal_v_flag_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn native8_decimal() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::new());
        cpu.e = false;
        cpu.p = FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
        cpu.set_flag_d(true);
        cpu
    }

    fn native16_decimal() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::new());
        cpu.e = false;
        cpu.p = 0; // M=0, X=0
        cpu.set_flag_d(true);
        cpu
    }

    /// 8-bit SBC decimal: positive minus large positive value yields negative result, V=1
    /// A=$50, M=$80, C=1 (no borrow)
    /// Binary: $50 + ~$80 + 1 = $D0, sign flips, V=1
    #[test]
    fn sbc_decimal_8bit_overflow_sets_v_flag() {
        let mut cpu = native8_decimal();
        cpu.a = 0x50;
        cpu.set_flag_c(true);
        cpu.bus.load(0x0000, &[0xE9, 0x80]); // SBC #$80
        cpu.step();
        assert!(
            cpu.flag_v(),
            "V should be set: $50 - $80 overflows in 8-bit signed BCD"
        );
    }

    /// 8-bit SBC decimal: no overflow when same signs, V=0
    /// A=$50, M=$30, C=1 result keeps sign, V=0
    #[test]
    fn sbc_decimal_8bit_no_overflow_clears_v_flag() {
        let mut cpu = native8_decimal();
        cpu.a = 0x50;
        cpu.set_flag_c(true);
        cpu.bus.load(0x0000, &[0xE9, 0x30]); // SBC #$30
        cpu.step();
        assert!(
            !cpu.flag_v(),
            "V should be clear: $50 - $30, no signed overflow"
        );
    }

    /// 16-bit SBC decimal: positive minus large value overflows, V=1
    /// A=$5000, M=$8000, C=1
    /// Binary: $5000 + ~$8000 + 1 = $D000, sign flips, V=1
    #[test]
    fn sbc_decimal_16bit_overflow_sets_v_flag() {
        let mut cpu = native16_decimal();
        cpu.a = 0x5000;
        cpu.set_flag_c(true);
        cpu.bus.load(0x0000, &[0xE9, 0x00, 0x80]); // SBC #$8000
        cpu.step();
        assert!(
            cpu.flag_v(),
            "V should be set: $5000 - $8000 overflows in 16-bit signed BCD"
        );
    }

    /// 16-bit SBC decimal: no overflow, V=0
    /// A=$5000, M=$3000, C=1
    #[test]
    fn sbc_decimal_16bit_no_overflow_clears_v_flag() {
        let mut cpu = native16_decimal();
        cpu.a = 0x5000;
        cpu.set_flag_c(true);
        cpu.bus.load(0x0000, &[0xE9, 0x00, 0x30]); // SBC #$3000
        cpu.step();
        assert!(
            !cpu.flag_v(),
            "V should be clear: $5000 - $3000, no signed overflow"
        );
    }
}

#[cfg(test)]
mod emulation_dp_wrap_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    // In emulation mode with D=$0000, dp,X and dp,Y addressing wraps
    // within page 0 ($00-$FF). Overflow beyond $FF wraps back to $00,
    // not into page 1 ($100+).

    fn emulation_cpu() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::new());
        cpu.e = true;
        cpu.d = 0x0000;
        cpu.p = FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
        cpu
    }

    fn native_cpu() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::new());
        cpu.e = false;
        cpu.d = 0x0000;
        cpu.p = FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH;
        cpu
    }

    /// In emulation mode D=$0000: LDA $FF,X with X=1 loads from $00, not $100
    #[test]
    fn emulation_dp_x_wraps_in_page_zero() {
        let mut cpu = emulation_cpu();
        cpu.x = 0x01;
        cpu.bus.load(0x0000, &[0x02]); // value at $0000
        cpu.bus.load(0x0100, &[0xFF]); // decoy at $0100 (should NOT be read)
        cpu.bus.load(0x0100, &[0xFF]); // already set
        // write a known value at $0000
        cpu.bus.write(0x0000, 0x42);
        cpu.bus.load(0x8000, &[0xB5, 0xFF]); // LDA $FF,X
        cpu.pc = 0x8000;
        cpu.step();
        assert_eq!(
            cpu.a & 0xFF,
            0x42,
            "emulation dp,X: $FF + X=$01 must wrap to $00 in page 0"
        );
    }

    /// In native mode D=$0000: LDA $FF,X with X=1 loads from $100 (no page-0 wrap)
    #[test]
    fn native_dp_x_does_not_wrap_in_page_zero() {
        let mut cpu = native_cpu();
        cpu.x = 0x01;
        cpu.bus.write(0x0000, 0x11);
        cpu.bus.write(0x0100, 0x42);
        cpu.bus.load(0x8000, &[0xB5, 0xFF]); // LDA $FF,X
        cpu.pc = 0x8000;
        cpu.step();
        assert_eq!(
            cpu.a & 0xFF,
            0x42,
            "native dp,X: $FF + X=$01 must land at $0100 (no page-0 wrap)"
        );
    }

    /// In emulation mode D=$0000: LDA ($FF,X) with X=1 reads pointer from $00, not $100
    #[test]
    fn emulation_dp_x_ind_wraps_in_page_zero() {
        let mut cpu = emulation_cpu();
        cpu.x = 0x01;
        // pointer at $0000: points to $0300
        cpu.bus.write(0x0000, 0x00);
        cpu.bus.write(0x0001, 0x03);
        // data at $0300
        cpu.bus.write(0x0300, 0x77);
        cpu.bus.load(0x8000, &[0xA1, 0xFF]); // LDA ($FF,X)
        cpu.pc = 0x8000;
        cpu.step();
        assert_eq!(
            cpu.a & 0xFF,
            0x77,
            "emulation dp,X indirect: pointer address must wrap to $00"
        );
    }
}

#[cfg(test)]
mod mvn_mvp_per_byte_cycle_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn native16() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::new());
        cpu.e = false;
        cpu.p &= !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);
        cpu
    }

    /// MVN must move exactly one byte per step() call, returning 7 cycles each time.
    /// While the transfer is in progress, PC remains on the second operand byte.
    #[test]
    fn mvn_moves_one_byte_per_step_7_cycles() {
        let mut cpu = native16();
        cpu.bus.load(0x01_0010, &[0xAA, 0xBB]);
        cpu.a = 0x0001; // 2 bytes to transfer
        cpu.x = 0x0010;
        cpu.y = 0x0020;
        cpu.bus.load(0x0000, &[0x54, 0x02, 0x01]); // MVN dst=$02, src=$01

        // First step: moves 1 byte, returns 7 cycles, PC stays on the src-bank operand.
        let c1 = cpu.step();
        assert_eq!(c1, 7, "MVN must return 7 cycles per byte");
        assert_eq!(
            cpu.pc, 0x0002,
            "PC must stay on the MVN src-bank operand while transfer is in progress"
        );
        assert_eq!(cpu.bus.read(0x02_0020), 0xAA, "first byte transferred");
        assert_eq!(cpu.a, 0x0000, "A decremented to 0 after first byte");

        // Second (last) step: moves final byte, returns 7 cycles, PC advances
        let c2 = cpu.step();
        assert_eq!(c2, 7, "MVN last byte also 7 cycles");
        assert_eq!(cpu.pc, 0x0003, "PC advances past MVN after last byte");
        assert_eq!(cpu.bus.read(0x02_0021), 0xBB, "second byte transferred");
        assert_eq!(cpu.a, 0xFFFF, "A=$FFFF after full transfer");
    }

    /// MVP must move exactly one byte per step() call, returning 7 cycles each time.
    #[test]
    fn mvp_moves_one_byte_per_step_7_cycles() {
        let mut cpu = native16();
        cpu.bus.load(0x01_0010, &[0xAA, 0xBB]);
        cpu.a = 0x0001; // 2 bytes to transfer
        cpu.x = 0x0011; // high end (MVP decrements)
        cpu.y = 0x0021;
        cpu.bus.load(0x0000, &[0x44, 0x02, 0x01]); // MVP dst=$02, src=$01

        let c1 = cpu.step();
        assert_eq!(c1, 7, "MVP must return 7 cycles per byte");
        assert_eq!(
            cpu.pc, 0x0002,
            "PC must stay on the MVP src-bank operand while transfer is in progress"
        );
        assert_eq!(cpu.bus.read(0x02_0021), 0xBB, "first byte (from high end)");
        assert_eq!(cpu.a, 0x0000);

        let c2 = cpu.step();
        assert_eq!(c2, 7);
        assert_eq!(cpu.pc, 0x0003, "PC advances past MVP after last byte");
        assert_eq!(cpu.bus.read(0x02_0020), 0xAA);
        assert_eq!(cpu.a, 0xFFFF);
    }
}

// =============================================================================
// Interrupt dispatch tests (issue #2731)
// =============================================================================
#[cfg(test)]
mod interrupt_dispatch_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn native() -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false;
        cpu.p &= !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);
        cpu
    }

    /// Minimal bus that delivers a single NMI edge via `poll_nmi`, to test the `step()` sync.
    ///
    /// `arm_nmi_at_poll` (if set) arms the edge on the Nth call to `poll_nmi`
    /// (0-indexed) rather than on the very first call, so tests can model an
    /// edge that rises mid-instruction -- e.g. on a multi-cycle instruction's
    /// 3rd bus access rather than its opcode fetch -- once `poll_nmi` is
    /// polled once per CPU cycle instead of once per `step()`.
    struct PollNmiBus {
        mem: Vec<u8>,
        nmi_once: bool,
        poll_count: u32,
        arm_nmi_at_poll: Option<u32>,
    }

    impl PollNmiBus {
        fn new() -> Self {
            Self {
                mem: vec![0; 0x100_0000],
                nmi_once: false,
                poll_count: 0,
                arm_nmi_at_poll: None,
            }
        }

        fn load(&mut self, addr: u32, data: &[u8]) {
            let a = (addr & 0xFF_FFFF) as usize;
            self.mem[a..a + data.len()].copy_from_slice(data);
        }
    }

    impl crate::snes::bus::SnesBus for PollNmiBus {
        fn read(&self, addr: u32) -> u8 {
            self.mem[(addr & 0xFF_FFFF) as usize]
        }
        fn write(&mut self, addr: u32, value: u8) {
            self.mem[(addr & 0xFF_FFFF) as usize] = value;
        }
        fn tick(&mut self) {}
        fn poll_nmi(&mut self) -> bool {
            if self.arm_nmi_at_poll == Some(self.poll_count) {
                self.nmi_once = true;
            }
            self.poll_count += 1;
            let n = self.nmi_once;
            self.nmi_once = false;
            n
        }
    }

    struct PollIrqBus {
        mem: Vec<u8>,
        irq_level: bool,
        /// `poll_irq` takes `&self` (level-triggered, non-consuming), so this
        /// needs interior mutability to count calls for tests that need the
        /// level to become live starting from a specific *cycle* rather than
        /// from the very start.
        poll_count: std::cell::Cell<u32>,
        /// If set, `poll_irq` ignores `irq_level` and instead returns true
        /// starting from the Nth call (0-indexed) and for every call after.
        level_from_poll: Option<u32>,
    }

    impl PollIrqBus {
        fn new() -> Self {
            Self {
                mem: vec![0; 0x100_0000],
                irq_level: false,
                poll_count: std::cell::Cell::new(0),
                level_from_poll: None,
            }
        }

        fn load(&mut self, addr: u32, data: &[u8]) {
            let a = (addr & 0xFF_FFFF) as usize;
            self.mem[a..a + data.len()].copy_from_slice(data);
        }
    }

    impl crate::snes::bus::SnesBus for PollIrqBus {
        fn read(&self, addr: u32) -> u8 {
            self.mem[(addr & 0xFF_FFFF) as usize]
        }
        fn write(&mut self, addr: u32, value: u8) {
            self.mem[(addr & 0xFF_FFFF) as usize] = value;
        }
        fn tick(&mut self) {}
        fn poll_irq(&self) -> bool {
            let count = self.poll_count.get();
            self.poll_count.set(count + 1);
            match self.level_from_poll {
                Some(threshold) => count >= threshold,
                None => self.irq_level,
            }
        }
    }

    /// Unlike [`PollIrqBus::level_from_poll`] (keyed to a call *count*, which
    /// can't distinguish resampling before vs. after a cycle's own clock tick
    /// -- both placements make exactly one `poll_irq` call per cycle either
    /// way), this bus keys the line's visibility to *elapsed master clocks*
    /// via `tick`, matching how the real PPU's H/V-IRQ line actually works.
    /// This is what's needed to catch a resample that's one whole cycle
    /// stale (#3049).
    struct ClockedIrqBus {
        mem: Vec<u8>,
        clocks: u64,
        visible_at: u64,
    }

    impl ClockedIrqBus {
        fn new(visible_at: u64) -> Self {
            Self {
                mem: vec![0; 0x100_0000],
                clocks: 0,
                visible_at,
            }
        }

        fn load(&mut self, addr: u32, data: &[u8]) {
            let a = (addr & 0xFF_FFFF) as usize;
            self.mem[a..a + data.len()].copy_from_slice(data);
        }
    }

    impl crate::snes::bus::SnesBus for ClockedIrqBus {
        fn read(&self, addr: u32) -> u8 {
            self.mem[(addr & 0xFF_FFFF) as usize]
        }
        fn write(&mut self, addr: u32, value: u8) {
            self.mem[(addr & 0xFF_FFFF) as usize] = value;
        }
        fn tick(&mut self) {
            self.clocks += 1;
        }
        fn poll_irq(&self) -> bool {
            self.clocks >= self.visible_at
        }
    }

    #[test]
    fn irq_level_during_wai_wakes_as_soon_as_the_preceding_cycle_observes_it() {
        // The shadow WAI's wait loop reads at the top of each step() call
        // must reflect the line's state as of the end of the *immediately
        // preceding* internal cycle -- not one whole cycle before that.
        // Resampling before a cycle's own clock tick (rather than after,
        // mirroring poll_and_arm_nmi_edge's placement) makes the shadow lag
        // by a full internal cycle (6 master clocks), delaying WAI's wake by
        // one extra "still waiting" iteration. Caught via a Mesen2-vs-NESER
        // bus-trace diff on undisbeliever's inidisp_forgot_to_force_blank.sfc
        // (#3049): NESER's WAI woke 6 master clocks late, corrupting a
        // subsequent DMA-timed palette/tile setup and leaving the screen
        // permanently unpainted.
        let mut cpu = Cpu::new(ClockedIrqBus::new(u64::MAX)); // emulation mode
        cpu.pc = 0x8000;
        cpu.set_flag_i(true); // I=1: WAI wakes without dispatching, isolating wake timing
        cpu.bus.load(0x008000, &[0xCB, 0xEA]); // WAI ; NOP

        assert_eq!(cpu.step(), 4, "WAI opcode timing");
        assert!(cpu.waiting);

        // The line becomes visible exactly 2 internal cycles (12 master
        // clocks) after WAI starts waiting.
        let baseline = cpu.bus.clocks;
        cpu.bus.visible_at = baseline + 12;

        // Two "still waiting" iterations must elapse before the line is
        // visible to the wait loop's own check.
        for _ in 0..2 {
            assert_eq!(cpu.step(), 1, "still waiting: one idle internal cycle");
            assert!(cpu.waiting);
        }

        // The third step() call must see the now-visible line and wake --
        // not require a fourth (stale-by-one-cycle) call. It falls through
        // to fetch and run the following NOP in the same call (masked
        // dispatch), so the total is 2 wake cycles + NOP's own 2.
        let cycles = cpu.step();
        assert!(!cpu.waiting, "WAI must wake once the line is 12 clocks old");
        assert_eq!(
            cycles, 4,
            "two wake cycles plus NOP's own 2 (masked resume)"
        );
        assert_eq!(cpu.pc, 0x8002, "execution resumes at the following NOP");
    }

    #[test]
    fn step_polls_the_bus_nmi_edge_and_dispatches_nmi_on_the_next_step() {
        // A bus-polled edge that becomes visible during the *current*
        // instruction's own bus ticks resolves one CPU cycle later than the
        // edge itself on real hardware (Mesen2 `SnesCpu::DetectNmiSignalEdge`
        // / `NmiFlagCounter`: the edge detector samples once per CPU cycle,
        // so `NeedNmi` only becomes true on the cycle *after* the one during
        // which the line rose). Proven against Mesen2 via a bus-trace diff
        // on the KungFuFurby nmi.smc ROM (#2883): at the exact clock where
        // both emulators read $4210 inside a polling loop, NESER dispatched
        // NMI immediately while Mesen2 executed one more loop instruction
        // first. So a *freshly* polled edge must not dispatch on the same
        // `step()` call that discovers it -- only once it was already
        // pending going into a call.
        let mut cpu = Cpu::new(PollNmiBus::new()); // emulation mode
        cpu.pc = 0x8000;
        cpu.s = 0x01FF;
        cpu.bus.load(0x008000, &[0xEA]); // NOP at $8000
        cpu.bus.load(0x00FFFA, &[0x00, 0x90]); // NMI emulation vector -> $9000
        cpu.bus.nmi_once = true;

        let cycles = cpu.step();
        assert_eq!(
            cpu.pc, 0x8001,
            "a freshly polled edge must not interrupt the in-flight instruction"
        );
        assert_eq!(cycles, 2, "the NOP's normal cycle cost, not NMI dispatch");

        cpu.step();
        assert_eq!(
            cpu.pc, 0x9000,
            "the edge, now pending from the previous step(), dispatches NMI"
        );
    }

    #[test]
    fn nmi_edge_mid_instruction_resolves_in_time_to_dispatch_right_after_that_instruction() {
        // Real hardware (and Mesen2's `DetectNmiSignalEdge`, called once per
        // CPU cycle from every Read/Write/Idle) samples the NMI line on
        // EVERY cycle, not just once per instruction. So an edge that rises
        // mid-instruction -- not on the instruction's very first cycle --
        // still resolves in time to dispatch right after THAT SAME
        // instruction; it must not cost an entire extra instruction of
        // delay just because it wasn't visible on the opcode fetch. LDA
        // $1234 (absolute, 4 cycles: opcode fetch + 2 operand bytes + 1
        // data read) is the edge-bearing instruction; the edge arises on
        // its 3rd cycle (poll index 2), partway through, not on the opcode
        // fetch itself (#3049).
        let mut cpu = Cpu::new(PollNmiBus::new()); // emulation mode
        cpu.pc = 0x8000;
        cpu.s = 0x01FF;
        cpu.bus.load(0x008000, &[0xAD, 0x34, 0x12]); // LDA $1234
        cpu.bus.load(0x008003, &[0xEA]); // NOP -- must never execute; NMI takes this slot
        cpu.bus.load(0x00FFFA, &[0x00, 0x90]); // NMI emulation vector -> $9000
        cpu.bus.arm_nmi_at_poll = Some(2);

        let cycles = cpu.step();
        assert_eq!(
            cpu.pc, 0x8003,
            "LDA executes in full; the edge doesn't interrupt it mid-flight"
        );
        assert_eq!(cycles, 4, "LDA absolute's normal cycle cost");

        cpu.step();
        assert_eq!(
            cpu.pc, 0x9000,
            "the edge, resolved during LDA's own cycles, dispatches NMI right \
             after LDA -- not one more instruction later"
        );
    }

    #[test]
    fn nmi_edge_that_wakes_wai_dispatches_in_the_same_step_call() {
        // While parked in WAI, `nmi_pending` is kept live by the per-cycle
        // resolve/poll running inside each waiting step()'s own
        // tick_internal_cycle() call -- so once it's visible at the TOP of a
        // step() call (the same check point WAI's wake condition reads), that
        // call must both clear `waiting` AND dispatch, not merely wake and
        // leave dispatch for a later call (#3049; this is the scenario the
        // pre-#3049 `nmi_dispatch_ready` WAI-wake resync existed for).
        let mut cpu = Cpu::new(PollNmiBus::new()); // emulation mode
        cpu.pc = 0x8000;
        cpu.s = 0x01FF;
        cpu.bus.load(0x008000, &[0xCB]); // WAI
        cpu.bus.load(0x00FFFA, &[0x00, 0x90]); // NMI emulation vector -> $9000

        assert_eq!(cpu.step(), 4, "WAI opcode timing");
        assert!(cpu.waiting);

        cpu.bus.nmi_once = true;
        let mut dispatched = false;
        for _ in 0..5 {
            cpu.step();
            if !cpu.waiting {
                dispatched = true;
                break;
            }
        }
        assert!(dispatched, "WAI never woke");
        assert_eq!(
            cpu.pc, 0x9000,
            "the step() call that wakes WAI must also dispatch NMI, not a later call"
        );
    }

    #[test]
    fn step_polls_bus_irq_level_and_dispatches_irq() {
        let mut cpu = Cpu::new(PollIrqBus::new()); // emulation mode
        cpu.pc = 0x8000;
        cpu.s = 0x01FF;
        cpu.set_flag_i(false);
        cpu.bus.load(0x00FFFE, &[0x00, 0x91]); // IRQ emulation vector -> $9100
        cpu.bus.irq_level = true;
        // The dispatch decision reads `irq_line_shadow` (resampled once per
        // CPU cycle, #3049), not a fresh poll -- pre-seed it to simulate the
        // line already having been asserted for at least one prior cycle.
        // Without this, BRK's opcode ($00, the fetch this test's empty
        // memory would otherwise produce) shares emulation mode's $FFFE
        // vector and 7-cycle cost with IRQ dispatch, so the assertions below
        // would pass even if IRQ never actually dispatched.
        cpu.irq_line_shadow = true;

        let cycles = cpu.step();

        assert_eq!(cycles, 7, "IRQ dispatch cycles in emulation mode");
        assert_eq!(cpu.pc, 0x9100, "step() should dispatch IRQ from bus level");
    }

    #[test]
    fn irq_level_mid_instruction_resolves_in_time_to_dispatch_right_after_that_instruction() {
        // Same principle as the NMI edge test: NESER's H/V-IRQ line is
        // level-triggered and non-consuming (Ppu::poll_irq_dispatch), so it
        // doesn't need an arm/latch counter like NMI's edge -- but the
        // shadow that step()'s dispatch check reads must still be sampled
        // once per CPU cycle (mirroring Mesen2's PrevIrqSource, updated
        // every ProcessCpuCycle), not once per instruction. A level that
        // becomes asserted mid-instruction -- not on the instruction's very
        // first cycle -- must still resolve in time to dispatch right after
        // THAT SAME instruction (#3049).
        let mut cpu = Cpu::new(PollIrqBus::new()); // emulation mode
        cpu.pc = 0x8000;
        cpu.s = 0x01FF;
        cpu.set_flag_i(false);
        cpu.bus.load(0x008000, &[0xAD, 0x34, 0x12]); // LDA $1234
        cpu.bus.load(0x008003, &[0xEA]); // NOP -- must never execute; IRQ takes this slot
        cpu.bus.load(0x00FFFE, &[0x00, 0x91]); // IRQ emulation vector -> $9100
        cpu.bus.level_from_poll = Some(2);

        let cycles = cpu.step();
        assert_eq!(
            cpu.pc, 0x8003,
            "LDA executes in full; the level doesn't interrupt it mid-flight"
        );
        assert_eq!(cycles, 4, "LDA absolute's normal cycle cost");

        cpu.step();
        assert_eq!(
            cpu.pc, 0x9100,
            "the level, visible during LDA's own cycles, dispatches IRQ \
             right after LDA -- not one more instruction later"
        );
    }

    #[test]
    fn bus_irq_deassertion_stops_redispatch() {
        let mut cpu = Cpu::new(PollIrqBus::new()); // emulation mode
        cpu.s = 0x01FF;
        cpu.set_flag_i(false);
        cpu.bus.load(0x00FFFE, &[0x00, 0x91]); // IRQ vector -> $9100
        cpu.bus.irq_level = true;
        cpu.irq_line_shadow = true; // see step_polls_bus_irq_level_and_dispatches_irq
        assert_eq!(cpu.step(), 7, "first IRQ dispatch");

        cpu.set_flag_i(false);
        cpu.bus.irq_level = false;
        // The line has genuinely deasserted by the time the next dispatch
        // decision is made -- the dispatch's own dummy-read/push/vector-read
        // cycles would otherwise resample the (still-true, at that point)
        // line into the shadow, since the test only flips `irq_level` after
        // step() already returned (#3049: the shadow reflects state as of
        // the end of the *previous* cycle, not "whatever the test set right
        // before calling step()").
        cpu.irq_line_shadow = false;
        cpu.bus.load(cpu.pc as u32, &[0xEA]); // NOP
        assert_eq!(
            cpu.step(),
            2,
            "IRQ should not redispatch once line is deasserted"
        );
    }

    #[test]
    fn write_4200_defers_irq_dispatch_for_one_step() {
        let mut cpu = Cpu::new(PollIrqBus::new());
        cpu.pc = 0x8000;
        cpu.s = 0x01FF;
        cpu.set_flag_i(false);
        cpu.write_a(0x0010);
        // STA $4200 ; NOP ; NOP
        cpu.bus.load(0x008000, &[0x8D, 0x00, 0x42, 0xEA, 0xEA]);
        cpu.bus.load(0x00FFFE, &[0x00, 0x91]); // IRQ vector -> $9100
        cpu.bus.irq_level = false;

        cpu.step(); // STA $4200 (arms irq lock)
        cpu.bus.irq_level = true;
        cpu.step(); // lock window: should execute NOP
        assert_eq!(cpu.pc, 0x8004, "defer window should run next opcode first");

        let cycles = cpu.step();
        assert_eq!(cycles, 7, "IRQ dispatch should occur on following step");
        assert_eq!(cpu.pc, 0x9100);
    }

    #[test]
    fn irq_after_cli_executes_one_more_instruction_before_dispatch() {
        // 65816 IRQ recognition samples the I flag on the cycle BEFORE it takes
        // effect: CLI's clear only becomes dispatch-visible after the FOLLOWING
        // instruction (Mesen2 DetectNmiSignalEdge PrevIrqSource; classic 6502
        // CLI shadow).
        let mut cpu = Cpu::new(PollIrqBus::new()); // emulation mode
        cpu.pc = 0x8000;
        cpu.s = 0x01FF;
        cpu.set_flag_i(true);
        cpu.bus.load(0x008000, &[0x58, 0xEA, 0xEA]); // CLI ; NOP ; NOP
        cpu.bus.load(0x00FFFE, &[0x00, 0x91]); // IRQ vector -> $9100
        cpu.bus.irq_level = true;

        cpu.step(); // CLI
        assert_eq!(cpu.pc, 0x8001, "CLI executes without dispatch");
        cpu.step();
        assert_eq!(
            cpu.pc, 0x8002,
            "the instruction after CLI executes before the IRQ dispatches"
        );
        cpu.step();
        assert_eq!(cpu.pc, 0x9100, "the IRQ dispatches one instruction later");
    }

    #[test]
    fn cli_rti_dispatches_after_the_full_rti_unwind() {
        // The CLI;RTI epilogue idiom (absindx SA1RamProtectionTest wrapper): a
        // pending IRQ must NOT dispatch between the CLI and the RTI -- the RTI
        // completes first and the IRQ then interrupts the ORIGINAL context.
        let mut cpu = Cpu::new(PollIrqBus::new()); // emulation mode
        cpu.pc = 0x8000;
        cpu.s = 0x01FC;
        cpu.set_flag_i(true);
        // Fake interrupted frame: P (I clear), PC = $8100.
        cpu.bus.load(0x0001FD, &[0x00, 0x00, 0x81]);
        cpu.bus.load(0x008000, &[0x58, 0x40]); // CLI ; RTI
        cpu.bus.load(0x008100, &[0xEA, 0xEA]); // original context
        cpu.bus.load(0x00FFFE, &[0x00, 0x91]); // IRQ vector -> $9100
        cpu.bus.irq_level = true;

        cpu.step(); // CLI
        assert_eq!(cpu.pc, 0x8001, "CLI executes without dispatch");
        cpu.step();
        assert_eq!(
            cpu.pc, 0x8100,
            "the RTI completes before the pending IRQ dispatches"
        );
        cpu.step();
        assert_eq!(
            cpu.pc, 0x9100,
            "the IRQ then interrupts the original context"
        );
        // The stacked return address is the original context, not the epilogue.
        assert_eq!(
            cpu.bus.read(0x0001FF),
            0x81,
            "pushed PCH = original context"
        );
        assert_eq!(
            cpu.bus.read(0x0001FE),
            0x00,
            "pushed PCL = original context"
        );
    }

    #[test]
    fn plp_clearing_i_is_delayed_like_cli() {
        let mut cpu = Cpu::new(PollIrqBus::new()); // emulation mode
        cpu.pc = 0x8000;
        cpu.s = 0x01FE;
        cpu.set_flag_i(true);
        cpu.bus.load(0x0001FF, &[0x00]); // pulled P: I clear
        cpu.bus.load(0x008000, &[0x28, 0xEA, 0xEA]); // PLP ; NOP ; NOP
        cpu.bus.load(0x00FFFE, &[0x00, 0x91]);
        cpu.bus.irq_level = true;

        cpu.step(); // PLP (pulls I=0; SetPS lands on its final cycle -> delayed)
        assert_eq!(cpu.pc, 0x8001, "PLP executes without dispatch");
        cpu.step();
        assert_eq!(
            cpu.pc, 0x8002,
            "the instruction after PLP executes before the IRQ dispatches"
        );
        cpu.step();
        assert_eq!(cpu.pc, 0x9100, "the IRQ dispatches one instruction later");
    }

    #[test]
    fn rti_restoring_i_clear_dispatches_the_pending_irq_immediately() {
        // RTI pulls P mid-instruction, so its restored I=0 IS visible to the
        // recognition poll on its remaining cycles: the IRQ dispatches right
        // after the RTI (no extra instruction), unlike CLI/PLP.
        let mut cpu = Cpu::new(PollIrqBus::new()); // emulation mode
        cpu.pc = 0x8000;
        cpu.s = 0x01FC;
        cpu.set_flag_i(true);
        cpu.bus.load(0x0001FD, &[0x00, 0x00, 0x81]); // frame: P (I clear), PC $8100
        cpu.bus.load(0x008000, &[0x40]); // RTI
        cpu.bus.load(0x008100, &[0xEA]);
        cpu.bus.load(0x00FFFE, &[0x00, 0x91]);
        cpu.bus.irq_level = true;

        cpu.step(); // RTI
        assert_eq!(cpu.pc, 0x8100, "RTI returns to the frame");
        cpu.step();
        assert_eq!(
            cpu.pc, 0x9100,
            "the pending IRQ dispatches immediately after RTI"
        );
    }

    #[test]
    fn write_420b_defers_irq_dispatch_for_one_step() {
        let mut cpu = Cpu::new(PollIrqBus::new());
        cpu.pc = 0x8000;
        cpu.s = 0x01FF;
        cpu.set_flag_i(false);
        cpu.write_a(0x0001);
        // STA $420B ; NOP ; NOP
        cpu.bus.load(0x008000, &[0x8D, 0x0B, 0x42, 0xEA, 0xEA]);
        cpu.bus.load(0x00FFFE, &[0x00, 0x91]); // IRQ vector -> $9100
        cpu.bus.irq_level = false;

        cpu.step(); // STA $420B (DMA start path, arms irq lock)
        cpu.bus.irq_level = true;
        cpu.step(); // lock window: should execute NOP
        assert_eq!(cpu.pc, 0x8004, "defer window should run next opcode first");

        let cycles = cpu.step();
        assert_eq!(cycles, 7);
        assert_eq!(cpu.pc, 0x9100);
    }

    // =========================================================================
    // NMI — native mode
    // Vectors via $FFEA/$FFEB; pushes PBR, PCH, PCL, P; sets I=1, D=0; 8 cycles
    // =========================================================================

    #[test]
    fn nmi_native_pushes_pbr_pc_p_sets_i_clears_d_vectors() {
        let mut cpu = native();
        cpu.pbr = 0x02;
        cpu.pc = 0x1234;
        cpu.s = 0x01FF;
        cpu.set_flag_d(true);
        cpu.set_flag_i(false);
        cpu.bus.load(0x00FFEA, &[0xAB, 0xCD]); // NMI native vector -> $CDAB
        cpu.set_nmi(true);
        let cycles = cpu.step();
        assert_eq!(cycles, 8, "NMI native: 8 cycles");
        assert_eq!(cpu.pc, 0xCDAB, "PC loaded from NMI native vector");
        assert_eq!(cpu.pbr, 0x00, "PBR cleared to 0 on interrupt entry");
        assert_eq!(cpu.s, 0x01FB, "4 bytes pushed: PBR, PCH, PCL, P");
        assert_eq!(cpu.bus.read(0x01FF), 0x02, "PBR on stack");
        assert_eq!(cpu.bus.read(0x01FE), 0x12, "PCH on stack");
        assert_eq!(cpu.bus.read(0x01FD), 0x34, "PCL on stack");
        assert!(cpu.flag_i(), "I set on interrupt entry");
        assert!(!cpu.flag_d(), "D cleared on interrupt entry (native)");
    }

    #[test]
    fn nmi_native_8_cycles() {
        let mut cpu = native();
        cpu.s = 0x01FF;
        cpu.set_nmi(true);
        let cycles = cpu.step();
        assert_eq!(cycles, 8);
    }

    #[test]
    fn nmi_not_masked_by_i_flag() {
        let mut cpu = native();
        cpu.s = 0x01FF;
        cpu.set_flag_i(true); // I=1 normally masks IRQ, but not NMI
        cpu.bus.load(0x00FFEA, &[0x00, 0x30]); // vector -> $3000
        cpu.set_nmi(true);
        let cycles = cpu.step();
        assert_eq!(cycles, 8, "NMI fires even with I=1");
        assert_eq!(cpu.pc, 0x3000);
    }

    #[test]
    fn nmi_pending_cleared_after_dispatch() {
        let mut cpu = native();
        cpu.s = 0x01FF;
        cpu.set_nmi(true);
        cpu.step(); // dispatch NMI
        // set a NOP at the NMI vector so next step runs it
        let vector_pc = cpu.pc;
        cpu.bus.load(vector_pc as u32, &[0xEA]); // NOP
        let cycles = cpu.step(); // should execute NOP, not another NMI
        assert_eq!(cycles, 2, "NMI not re-dispatched after first dispatch");
    }

    #[test]
    fn nmi_arm_counter_round_trips_through_cpu_state_before_it_resolves() {
        // Save/restore mid-arm-window (armed but not yet resolved) must not
        // silently drop an in-flight NMI edge (#3049).
        let mut cpu = Cpu::new(PollNmiBus::new()); // emulation mode
        cpu.pc = 0x8000;
        cpu.s = 0x01FF;
        cpu.bus.load(0x008000, &[0xAD, 0x34, 0x12]); // LDA $1234 (4 cycles)
        cpu.bus.load(0x00FFFA, &[0x00, 0x90]); // NMI emulation vector -> $9000
        cpu.bus.arm_nmi_at_poll = Some(3); // arm during LDA's last cycle

        cpu.step(); // LDA executes in full; edge arms but doesn't resolve yet
        assert!(
            !cpu.nmi_pending,
            "edge should be armed but not yet resolved"
        );
        assert_eq!(
            cpu.nmi_arm_counter, 1,
            "counter should be armed, one cycle from resolving"
        );

        let state = cpu.capture_state_inner();
        let mut restored = Cpu::new(PollNmiBus::new());
        restored.restore_state_inner(&state);
        // Bus/memory contents aren't part of CpuState; reload what's needed.
        restored.bus.load(0x008003, &[0xEA]); // NOP
        restored.bus.load(0x00FFFA, &[0x00, 0x90]);

        restored.step(); // counter resolves mid-NOP; doesn't interrupt it
        restored.step(); // NMI dispatches now that nmi_pending is set
        assert_eq!(
            restored.pc, 0x9000,
            "an in-flight (armed but unresolved) NMI edge must survive save/restore"
        );
    }

    #[test]
    fn irq_line_shadow_round_trips_through_cpu_state() {
        // A live IRQ level cached in the shadow must survive save/restore,
        // same as nmi_arm_counter's in-flight edge above (#3049). Uses native
        // mode so IRQ's vector ($00FFEE) doesn't collide with BRK's
        // ($00FFE6, emulation mode shares $00FFFE with IRQ) -- a silently
        // dropped shadow would otherwise still land at the same PC via BRK
        // fetched from the zeroed test bus, making the dispatch assertion
        // pass vacuously (see step_polls_bus_irq_level_and_dispatches_irq).
        let mut cpu = Cpu::new(PollIrqBus::new());
        cpu.e = false; // native mode
        cpu.s = 0x01FF;
        cpu.set_flag_i(false);
        cpu.irq_line_shadow = true;

        let state = cpu.capture_state_inner();
        let mut restored = Cpu::new(PollIrqBus::new());
        restored.restore_state_inner(&state);
        // Bus/memory contents aren't part of CpuState; reload what's needed.
        restored.bus.load(0x00FFEE, &[0x00, 0x91]); // IRQ native vector -> $9100

        assert!(
            restored.irq_line_shadow,
            "the live IRQ level must survive save/restore"
        );
        restored.step();
        assert_eq!(
            restored.pc, 0x9100,
            "a live IRQ level cached in the shadow must survive save/restore"
        );
    }

    // =========================================================================
    // NMI — emulation mode
    // Vectors via $FFFA/$FFFB; pushes PCH, PCL, P (B=0); sets I=1, D=0; 7 cycles
    // =========================================================================

    #[test]
    fn nmi_emulation_pushes_pc_p_sets_i_clears_d_vectors() {
        let mut cpu = Cpu::new(TestBus::default()); // emulation mode
        cpu.pbr = 0x05; // non-zero to verify it gets cleared
        cpu.pc = 0x5678;
        cpu.s = 0x01FF;
        cpu.set_flag_d(true);
        cpu.set_flag_i(false);
        cpu.bus.load(0x00FFFA, &[0xEF, 0xBE]); // NMI emulation vector -> $BEEF
        cpu.set_nmi(true);
        let cycles = cpu.step();
        assert_eq!(cycles, 7, "NMI emulation: 7 cycles");
        assert_eq!(cpu.pc, 0xBEEF, "PC loaded from NMI emulation vector");
        assert_eq!(
            cpu.pbr, 0x00,
            "PBR forced to 0 on emulation-mode interrupt entry"
        );
        assert_eq!(cpu.s, 0x01FC, "3 bytes pushed: PCH, PCL, P");
        assert_eq!(cpu.bus.read(0x01FF), 0x56, "PCH on stack");
        assert_eq!(cpu.bus.read(0x01FE), 0x78, "PCL on stack");
        // B flag (bit 4) must be 0 for hardware interrupts in emulation mode
        assert_eq!(
            cpu.bus.read(0x01FD) & FLAG_INDEX_WIDTH,
            0,
            "B=0 on stack for hardware NMI"
        );
        assert!(cpu.flag_i(), "I set");
        assert!(!cpu.flag_d(), "D cleared (emulation mode too on 65C816)");
    }

    #[test]
    fn nmi_emulation_7_cycles() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.s = 0x01FF;
        cpu.set_nmi(true);
        let cycles = cpu.step();
        assert_eq!(cycles, 7);
    }

    // =========================================================================
    // IRQ — native mode
    // Vectors via $FFEE/$FFEF; same push order as NMI; 8 cycles; masked by I=1
    // =========================================================================

    #[test]
    fn irq_native_pushes_pbr_pc_p_and_vectors() {
        let mut cpu = native();
        cpu.pbr = 0x03;
        cpu.pc = 0xABCD;
        cpu.s = 0x01FF;
        cpu.set_flag_i(false);
        cpu.set_flag_d(true);
        cpu.bus.load(0x00FFEE, &[0x00, 0x40]); // IRQ native vector -> $4000
        cpu.set_irq(true);
        let cycles = cpu.step();
        assert_eq!(cycles, 8, "IRQ native: 8 cycles");
        assert_eq!(cpu.pc, 0x4000);
        assert_eq!(cpu.pbr, 0x00, "PBR cleared to 0");
        assert_eq!(cpu.s, 0x01FB, "4 bytes pushed");
        assert_eq!(cpu.bus.read(0x01FF), 0x03, "PBR on stack");
        assert_eq!(cpu.bus.read(0x01FE), 0xAB, "PCH on stack");
        assert_eq!(cpu.bus.read(0x01FD), 0xCD, "PCL on stack");
        assert!(cpu.flag_i(), "I set");
        assert!(!cpu.flag_d(), "D cleared");
    }

    #[test]
    fn irq_masked_when_i_flag_set() {
        let mut cpu = native();
        cpu.s = 0x01FF;
        cpu.set_flag_i(true);
        cpu.set_irq(true);
        // Place a NOP at PC so step() executes it instead
        cpu.bus.load(cpu.pc as u32, &[0xEA]);
        let cycles = cpu.step();
        assert_eq!(cycles, 2, "IRQ masked: NOP executes instead");
    }

    #[test]
    fn irq_fires_when_i_flag_clear() {
        let mut cpu = native();
        cpu.s = 0x01FF;
        cpu.set_flag_i(false);
        cpu.bus.load(0x00FFEE, &[0x00, 0x50]);
        cpu.set_irq(true);
        let cycles = cpu.step();
        assert_eq!(cycles, 8, "IRQ fires with I=0");
        assert_eq!(cpu.pc, 0x5000);
    }

    #[test]
    fn irq_level_triggered_redispatches_while_held() {
        // Verify that irq_pending is NOT cleared on dispatch (level-triggered):
        // hold the IRQ line, clear I after the first dispatch, and the next step
        // should dispatch again rather than execute an opcode.
        let mut cpu = native();
        cpu.s = 0x01FF;
        cpu.set_flag_i(false);
        cpu.bus.load(0x00FFEE, &[0x00, 0x50]); // IRQ native vector -> $5000
        cpu.set_irq(true);

        // First dispatch: IRQ fires, I becomes 1
        let cycles1 = cpu.step();
        assert_eq!(cycles1, 8, "first IRQ dispatch: 8 cycles");
        assert_eq!(cpu.pc, 0x5000);

        // Manually clear I to allow the held IRQ to fire again
        cpu.set_flag_i(false);
        // IRQ is still asserted (level-triggered — not cleared by dispatch)
        let cycles2 = cpu.step();
        assert_eq!(cycles2, 8, "IRQ re-dispatches while line held");
    }

    // =========================================================================
    // IRQ — emulation mode
    // Vectors via $FFFE/$FFFF; pushes PCH, PCL, P (B=0); 7 cycles
    // =========================================================================

    #[test]
    fn irq_emulation_pushes_pc_p_b0_and_vectors() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.pc = 0x1000;
        cpu.s = 0x01FF;
        cpu.set_flag_i(false);
        cpu.set_flag_d(true);
        cpu.bus.load(0x00FFFE, &[0x00, 0x60]); // IRQ emulation vector -> $6000
        cpu.set_irq(true);
        let cycles = cpu.step();
        assert_eq!(cycles, 7, "IRQ emulation: 7 cycles");
        assert_eq!(cpu.pc, 0x6000);
        assert_eq!(cpu.s, 0x01FC, "3 bytes pushed");
        assert_eq!(cpu.bus.read(0x01FF), 0x10, "PCH");
        assert_eq!(cpu.bus.read(0x01FE), 0x00, "PCL");
        assert_eq!(cpu.bus.read(0x01FD) & FLAG_INDEX_WIDTH, 0, "B=0 on stack");
        assert!(cpu.flag_i());
        assert!(!cpu.flag_d(), "D cleared in emulation mode too");
    }

    // =========================================================================
    // ABORT — native mode
    // Vectors via $FFE8/$FFE9; same push order as NMI; 8 cycles
    // =========================================================================

    #[test]
    fn abort_native_pushes_pbr_pc_p_and_vectors() {
        let mut cpu = native();
        cpu.pbr = 0x01;
        cpu.pc = 0x2000;
        cpu.s = 0x01FF;
        cpu.set_flag_d(true);
        cpu.set_flag_i(false);
        cpu.bus.load(0x00FFE8, &[0x00, 0x70]); // ABORT native vector -> $7000
        cpu.set_abort(true);
        let cycles = cpu.step();
        assert_eq!(cycles, 8, "ABORT native: 8 cycles");
        assert_eq!(cpu.pc, 0x7000);
        assert_eq!(cpu.pbr, 0x00);
        assert_eq!(cpu.s, 0x01FB, "4 bytes pushed");
        assert_eq!(cpu.bus.read(0x01FF), 0x01, "PBR on stack");
        assert_eq!(cpu.bus.read(0x01FE), 0x20, "PCH on stack");
        assert_eq!(cpu.bus.read(0x01FD), 0x00, "PCL on stack");
        assert!(cpu.flag_i());
        assert!(!cpu.flag_d());
    }

    #[test]
    fn abort_pending_cleared_after_dispatch() {
        let mut cpu = native();
        cpu.s = 0x01FF;
        cpu.set_abort(true);
        cpu.step();
        let vector_pc = cpu.pc;
        cpu.bus.load(vector_pc as u32, &[0xEA]);
        let cycles = cpu.step(); // should execute NOP
        assert_eq!(cycles, 2, "ABORT not re-dispatched");
    }

    // =========================================================================
    // ABORT — emulation mode
    // Vectors via $FFF8/$FFF9; pushes PCH, PCL, P (B=0); 7 cycles
    // =========================================================================

    #[test]
    fn abort_emulation_pushes_pc_p_and_vectors() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.pc = 0x3000;
        cpu.s = 0x01FF;
        cpu.set_flag_d(true);
        cpu.set_flag_i(false);
        cpu.bus.load(0x00FFF8, &[0x00, 0x80]); // ABORT emulation vector -> $8000
        cpu.set_abort(true);
        let cycles = cpu.step();
        assert_eq!(cycles, 7, "ABORT emulation: 7 cycles");
        assert_eq!(cpu.pc, 0x8000);
        assert_eq!(cpu.s, 0x01FC, "3 bytes pushed");
        assert_eq!(cpu.bus.read(0x01FF), 0x30, "PCH");
        assert_eq!(cpu.bus.read(0x01FE), 0x00, "PCL");
        assert_eq!(cpu.bus.read(0x01FD) & FLAG_INDEX_WIDTH, 0, "B=0 on stack");
        assert!(cpu.flag_i());
        assert!(!cpu.flag_d());
    }

    // =========================================================================
    // RESET
    // No stack push; vectors via $FFFC/$FFFD (same in both modes); enters emulation mode
    // =========================================================================

    #[test]
    fn reset_loads_vector_and_enters_emulation_mode() {
        let mut cpu = native(); // start in native mode
        cpu.pbr = 0x05;
        cpu.dbr = 0x05;
        cpu.s = 0x0123;
        cpu.bus.load(0x00FFFC, &[0x00, 0x90]); // RESET vector -> $9000
        cpu.do_reset();
        assert_eq!(cpu.pc, 0x9000, "PC from RESET vector");
        assert_eq!(cpu.pbr, 0x00, "PBR cleared");
        assert!(cpu.emulation_mode(), "CPU enters emulation mode on RESET");
        assert!(cpu.flag_i(), "I=1 after RESET");
        assert_eq!(
            cpu.s & 0xFF00,
            0x0100,
            "stack high byte forced to $01 in emulation"
        );
    }

    #[test]
    fn reset_does_not_push_to_stack() {
        let mut cpu = Cpu::new(TestBus::default()); // emulation mode
        cpu.s = 0x01FF;
        // Fill stack with sentinel values
        for i in 0..8u32 {
            cpu.bus.load(0x01F8 + i, &[0x42]);
        }
        cpu.do_reset();
        // Stack pointer should not have moved down (no pushes)
        // The RESET vector is at $FFFC/$FFFD (zeroed TestBus → PC=$0000)
        // All sentinel bytes should be unchanged
        for i in 0..8u32 {
            assert_eq!(
                cpu.bus.read(0x01F8 + i),
                0x42,
                "stack not modified at offset {i}"
            );
        }
    }

    // =========================================================================
    // Interrupt priority: ABORT > NMI > IRQ
    // =========================================================================

    #[test]
    fn abort_takes_priority_over_nmi() {
        let mut cpu = native();
        cpu.s = 0x01FF;
        cpu.set_flag_i(false);
        cpu.bus.load(0x00FFE8, &[0x00, 0xA0]); // ABORT native -> $A000
        cpu.bus.load(0x00FFEA, &[0x00, 0xB0]); // NMI native -> $B000
        cpu.set_abort(true);
        cpu.set_nmi(true);
        cpu.step(); // should dispatch ABORT
        assert_eq!(cpu.pc, 0xA000, "ABORT takes priority over NMI");
    }

    #[test]
    fn nmi_takes_priority_over_irq() {
        let mut cpu = native();
        cpu.s = 0x01FF;
        cpu.set_flag_i(false);
        cpu.bus.load(0x00FFEA, &[0x00, 0xC0]); // NMI native -> $C000
        cpu.bus.load(0x00FFEE, &[0x00, 0xD0]); // IRQ native -> $D000
        cpu.set_nmi(true);
        cpu.set_irq(true);
        cpu.step(); // should dispatch NMI
        assert_eq!(cpu.pc, 0xC000, "NMI takes priority over IRQ");
    }

    // =========================================================================
    // BRK/COP emulation mode D-flag fix (regression)
    // Per 65C816 spec, D is cleared in ALL interrupt entries including emulation mode
    // =========================================================================

    #[test]
    fn brk_emulation_clears_d_flag() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.set_flag_d(true);
        cpu.bus.load(0x0000, &[0x00, 0x00]); // BRK
        cpu.step();
        assert!(!cpu.flag_d(), "D cleared by BRK in emulation mode");
    }

    #[test]
    fn cop_emulation_clears_d_flag() {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.set_flag_d(true);
        cpu.bus.load(0x0000, &[0x02, 0x00]); // COP
        cpu.step();
        assert!(!cpu.flag_d(), "D cleared by COP in emulation mode");
    }
}

/// Master-clock tick count integration tests.
///
/// These verify that `SnesBus::tick()` is called the correct number of master
/// clock cycles per instruction, based on the memory access speed of each
/// address region.
///
/// All tests load instructions at `$00:$0000` (WRAM mirror → 8 master clocks
/// per access) unless they are specifically testing a different region.
#[cfg(test)]
mod master_clock_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn cpu_at(load_addr: u32, code: &[u8]) -> Cpu<TestBus> {
        let mut cpu = Cpu::new(TestBus::default());
        cpu.e = false; // native mode
        cpu.p = 0b0011_0000; // M=1, X=1 (8-bit)
        cpu.write_pbr((load_addr >> 16) as u8);
        cpu.write_pc(load_addr as u16);
        cpu.bus.load(load_addr, code);
        cpu
    }

    // -------------------------------------------------------------------------
    // NOP — 2 bus cycles (opcode fetch + internal cycle)
    // At $00:$0000 (WRAM mirror, 8 clocks fetch + 6 clocks internal) → 14 ticks
    // -------------------------------------------------------------------------

    #[test]
    fn nop_at_wram_region_produces_14_master_clocks() {
        let mut cpu = cpu_at(0x00_0000, &[0xEA]); // NOP
        cpu.step();
        assert_eq!(cpu.bus.tick_count(), 14);
    }

    // -------------------------------------------------------------------------
    // NOP at $80:$8000 with MEMSEL=0 (WS2 ROM, slow) → 8 (fetch) + 6 (internal) = 14
    // -------------------------------------------------------------------------

    #[test]
    fn nop_at_ws2_rom_slow_produces_14_master_clocks() {
        let mut cpu = cpu_at(0x80_8000, &[0xEA]); // NOP
        // fast_rom defaults to false
        cpu.step();
        assert_eq!(cpu.bus.tick_count(), 14);
    }

    // -------------------------------------------------------------------------
    // NOP at $80:$8000 with MEMSEL=1 (WS2 ROM, fast) → 2 × 6 = 12
    // -------------------------------------------------------------------------

    #[test]
    fn nop_at_ws2_rom_fast_produces_12_master_clocks() {
        let mut cpu = cpu_at(0x80_8000, &[0xEA]); // NOP
        cpu.fast_rom = true;
        cpu.step();
        assert_eq!(cpu.bus.tick_count(), 12);
    }

    // -------------------------------------------------------------------------
    // NOP at $C0:$0000 with MEMSEL=0 (WS2 HiROM, slow) → 8 (fetch) + 6 (internal) = 14
    // -------------------------------------------------------------------------

    #[test]
    fn nop_at_ws2_hirom_slow_produces_14_master_clocks() {
        let mut cpu = cpu_at(0xC0_0000, &[0xEA]); // NOP
        cpu.step();
        assert_eq!(cpu.bus.tick_count(), 14);
    }

    // -------------------------------------------------------------------------
    // NOP at $C0:$0000 with MEMSEL=1 (WS2 HiROM, fast) → 2 × 6 = 12
    // -------------------------------------------------------------------------

    #[test]
    fn nop_at_ws2_hirom_fast_produces_12_master_clocks() {
        let mut cpu = cpu_at(0xC0_0000, &[0xEA]); // NOP
        cpu.fast_rom = true;
        cpu.step();
        assert_eq!(cpu.bus.tick_count(), 12);
    }

    // -------------------------------------------------------------------------
    // LDA #imm (8-bit M=1): 2 bus accesses → 2 × 8 = 16 at $00:$0000
    // -------------------------------------------------------------------------

    #[test]
    fn lda_imm_8bit_at_wram_produces_16_master_clocks() {
        let mut cpu = cpu_at(0x00_0000, &[0xA9, 0x42]); // LDA #$42
        cpu.step();
        assert_eq!(cpu.bus.tick_count(), 16);
    }

    // -------------------------------------------------------------------------
    // MEMSEL $420D write toggles fast_rom and affects subsequent tick counts
    // -------------------------------------------------------------------------

    #[test]
    fn write_to_memsel_420d_enables_fast_rom() {
        // STA abs → store A to $420D (opcode $8D, addr lo $0D, addr hi $42)
        // This writes A to MEMSEL and sets fast_rom.
        // Instruction at $00:$0000 (WRAM, 8 clocks each)
        // STA abs = 4 bus accesses (opcode + lo + hi + write) → 4 × 8 = 32 ticks
        // But the write is to $420D which is CPU I/O (6 clocks), so: 3×8 + 6 = 30 ticks
        let mut cpu = cpu_at(0x00_0000, &[0x8D, 0x0D, 0x42]); // STA $420D
        cpu.write_a(0x01); // set bit 0 = enable fast_rom
        cpu.step();
        assert!(
            cpu.fast_rom,
            "fast_rom must be set after writing 1 to $420D"
        );
        // tick count: opcode(8) + lo addr(8) + hi addr(8) + write to $420D(6) = 30
        assert_eq!(cpu.bus.tick_count(), 30);
    }

    #[test]
    fn write_to_memsel_420d_disables_fast_rom() {
        let mut cpu = cpu_at(0x00_0000, &[0x8D, 0x0D, 0x42]); // STA $420D
        cpu.fast_rom = true;
        cpu.write_a(0x00); // bit 0 = 0 → disable fast_rom
        cpu.step();
        assert!(
            !cpu.fast_rom,
            "fast_rom must be cleared after writing 0 to $420D"
        );
    }

    // -------------------------------------------------------------------------
    // XSlow region: LDA abs accessing $4016 (joypad, 12 clocks)
    // LDA $4016: opcode(8) + lo(8) + hi(8) + read $4016(12) = 36 ticks
    // -------------------------------------------------------------------------

    #[test]
    fn lda_from_joypad_region_uses_xslow_access() {
        // LDA $4016 (abs) = 0xAD 0x16 0x40, code at $00:$0000
        let mut cpu = cpu_at(0x00_0000, &[0xAD, 0x16, 0x40]); // LDA $4016
        cpu.step();
        // opcode at $0000 (WRAM, 8) + lo at $0001 (8) + hi at $0002 (8) + read $4016 (XSlow, 12) = 36
        assert_eq!(cpu.bus.tick_count(), 36);
    }

    // -------------------------------------------------------------------------
    // Fast I/O region: LDA $2140 (APU port, 6 clocks for the read)
    // opcode(8) + lo(8) + hi(8) + read $2140(6) = 30 ticks
    // -------------------------------------------------------------------------

    #[test]
    fn lda_from_apu_port_uses_fast_access() {
        // LDA $2140 (abs) = 0xAD 0x40 0x21
        let mut cpu = cpu_at(0x00_0000, &[0xAD, 0x40, 0x21]); // LDA $2140
        cpu.step();
        // opcode(8) + lo(8) + hi(8) + read at $2140 (B-Bus I/O, 6) = 30
        assert_eq!(cpu.bus.tick_count(), 30);
    }

    // -------------------------------------------------------------------------
    // Hardware interrupt dispatch must tick the bus for the full 8/7-cycle
    // sequence, including the two wasted cycles before the pushes — not only
    // the memory accesses. The 65816 native IRQ/NMI sequence is 8 cycles (a
    // dummy re-read of the interrupted PC + 1 internal + 4 stack pushes + 2
    // vector reads); the emulation-mode sequence is 7 cycles (dummy read + 1
    // internal + 3 stack pushes + 2 vector reads). The dummy read genuinely
    // re-reads the interrupted PC's own memory region (Mesen2
    // `ProcessInterrupt`'s `ReadCode(_state.PC)`, #3049), so its cost is
    // region-dependent like any other read, not a flat internal-cycle tick.
    // -------------------------------------------------------------------------

    #[test]
    fn native_irq_dispatch_ticks_full_eight_cycles() {
        // Native mode, I clear so the IRQ dispatches. No opcode is fetched.
        let mut cpu = cpu_at(0x00_0000, &[]);
        cpu.write_pc(0x0000);
        // Native IRQ/BRK vector at $00FFEE -> handler.
        cpu.bus.load(0x00_FFEE, &[0x00, 0x90]);
        cpu.set_flag_i(false); // sync the recognition shadow past cpu_at's direct p poke
        cpu.set_irq(true);
        cpu.step();
        // Dummy read at the interrupted PC ($0000, WRAM, 8) + 4 pushes (WRAM
        // stack $01xx, 8 each = 32) + 2 vector reads ($00FFEE/EF, WS1 ROM
        // slow 8 each = 16) + 1 internal (6) = 62.
        assert_eq!(cpu.bus.tick_count(), 62);
    }

    #[test]
    fn emulation_irq_dispatch_ticks_full_seven_cycles() {
        let mut cpu = cpu_at(0x00_0000, &[]);
        cpu.e = true; // emulation mode
        cpu.write_pc(0x0000);
        // Emulation IRQ vector at $00FFFE -> handler.
        cpu.bus.load(0x00_FFFE, &[0x00, 0x90]);
        cpu.set_flag_i(false); // sync the recognition shadow past cpu_at's direct p poke
        cpu.set_irq(true);
        cpu.step();
        // Dummy read at the interrupted PC ($0000, WRAM, 8) + 3 pushes (8
        // each = 24) + 2 vector reads (8 each = 16) + 1 internal (6) = 54.
        assert_eq!(cpu.bus.tick_count(), 54);
    }

    /// Minimal bus that records the cumulative tick count at the moment of
    /// the *first* write, to distinguish "internal cycle before the push"
    /// from "internal cycle after the push" -- both orderings tick the same
    /// TOTAL count by the time step() returns, so only an intermediate
    /// checkpoint like this can tell them apart.
    struct FirstWriteTickBus {
        mem: Vec<u8>,
        tick_count: u64,
        first_write_tick: Option<u64>,
    }

    impl FirstWriteTickBus {
        fn new() -> Self {
            Self {
                mem: vec![0; 0x100_0000],
                tick_count: 0,
                first_write_tick: None,
            }
        }

        fn load(&mut self, addr: u32, data: &[u8]) {
            let a = (addr & 0xFF_FFFF) as usize;
            self.mem[a..a + data.len()].copy_from_slice(data);
        }
    }

    impl crate::snes::bus::SnesBus for FirstWriteTickBus {
        fn read(&self, addr: u32) -> u8 {
            self.mem[(addr & 0xFF_FFFF) as usize]
        }
        fn write(&mut self, addr: u32, value: u8) {
            if self.first_write_tick.is_none() {
                self.first_write_tick = Some(self.tick_count);
            }
            self.mem[(addr & 0xFF_FFFF) as usize] = value;
        }
        fn tick(&mut self) {
            self.tick_count += 1;
        }
    }

    // -------------------------------------------------------------------------
    // PHA/PHX/PHY's internal cycle happens BEFORE the push on real hardware
    // (Mesen2: `PHA`/`PHX`/`PHY` all call `Idle()` before `PushRegister`), not
    // after. NESER's generic "tick the opcode's leftover internal cycles after
    // the opcode function returns" model got this backwards, ticking the
    // internal cycle after the push instead -- harmless to the TOTAL cycle
    // count (both orderings tick the same number of master clocks overall),
    // but it desyncs the bus-visible timestamp of the push itself from
    // Mesen2's, breaking bit-exact bus-trace diffs for any code that pushes
    // A/X/Y right after an interrupt dispatch (as KungFuFurby's nmi.smc/
    // test_nmi.smc do, #3049).
    // -------------------------------------------------------------------------

    #[test]
    fn pha_ticks_its_internal_cycle_before_the_push() {
        // PHA at $00:0000 (WRAM, 8 clocks/access), native 8-bit A.
        let mut cpu = Cpu::new(FirstWriteTickBus::new());
        cpu.e = false;
        cpu.p = 0b0011_0000; // M=1, X=1 (8-bit)
        cpu.write_pbr(0x00);
        cpu.write_pc(0x0000);
        cpu.bus.load(0x00_0000, &[0x48]); // PHA
        cpu.step();
        // opcode fetch (8) + internal (6) = 14 ticked before the push's own
        // clock-advance begins; the write happens once that access completes
        // (writes hold the bus until the very end), i.e. at 14 + 8 = 22.
        assert_eq!(
            cpu.bus.first_write_tick,
            Some(22),
            "PHA's internal cycle must tick before the push, not after"
        );
    }

    #[test]
    fn phx_ticks_its_internal_cycle_before_the_push() {
        let mut cpu = Cpu::new(FirstWriteTickBus::new());
        cpu.e = false;
        cpu.p = 0b0011_0000;
        cpu.write_pbr(0x00);
        cpu.write_pc(0x0000);
        cpu.bus.load(0x00_0000, &[0xDA]); // PHX
        cpu.step();
        assert_eq!(
            cpu.bus.first_write_tick,
            Some(22),
            "PHX's internal cycle must tick before the push, not after"
        );
    }

    #[test]
    fn phy_ticks_its_internal_cycle_before_the_push() {
        let mut cpu = Cpu::new(FirstWriteTickBus::new());
        cpu.e = false;
        cpu.p = 0b0011_0000;
        cpu.write_pbr(0x00);
        cpu.write_pc(0x0000);
        cpu.bus.load(0x00_0000, &[0x5A]); // PHY
        cpu.step();
        assert_eq!(
            cpu.bus.first_write_tick,
            Some(22),
            "PHY's internal cycle must tick before the push, not after"
        );
    }
}

#[cfg(test)]
mod processor_vector_regressions {
    use super::*;
    use crate::snes::bus::TestBus;

    #[test]
    fn and_dp_ind_long_emulation_matches_vector_27_e_3341() {
        let mut cpu = Cpu::new(TestBus::default());

        cpu.bus.load(0xF19E72, &[0x27, 0xFE]);
        cpu.bus.write(0x002FFE, 0x9C);
        cpu.bus.write(0x002FFF, 0x6E);
        cpu.bus.write(0x003000, 0x5E);
        cpu.bus.write(0x5E6E9C, 0x2B);

        cpu.load_state_for_processor_test(
            0xBF88, 0x00D9, 0x0073, 0x2F00, 0xEE, 0xF1, 0x88BA, 0x9E72, 0xFC, true,
        );

        assert!(cpu.m_flag());
        assert_eq!(cpu.read_a(), 0xBF88);

        cpu.step();

        assert_eq!(cpu.read_a(), 0xBF08);
        assert_eq!(cpu.read_p() & (FLAG_NEGATIVE | FLAG_ZERO), 0);
    }
}
