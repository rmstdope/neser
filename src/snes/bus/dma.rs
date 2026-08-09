use crate::snes::console::save_state::SnesDmaState;

const DMA_REG_BYTES: usize = 0x80;

/// B-bus address of WMDATA (`$2180`), the WRAM data port.
const WMDATA_B_BUS_ADDR: u8 = 0x80;

/// Byte the A-bus write deposits when a `$2180` -> WRAM transfer is refused.
///
/// The references disagree and neither value is hardware-verified: Mesen2
/// `SnesDmaController::CopyDmaByte` writes `0xFF` ("the value written is
/// invalid"), ares `CPU::Channel::readB` yields `0x00`. fullsnes says only that
/// the transfer "isn't possible", and byuu's own `test_dmavalid` ROM asserts
/// merely that the destination changed to something other than its seed --
/// while noting the value is *not* MDR ("which would be #$ea"), which is why
/// the DMA open bus is deliberately left untouched here.
///
/// We follow Mesen2, which the rest of this controller's byte-slot model is
/// already built on.
const INVALID_WRAM_TRANSFER_BYTE: u8 = 0xFF;

pub trait DmaABus {
    fn dma_read_a_bus(&mut self, addr: u32, open_bus: u8) -> u8;
    fn dma_write_a_bus(&mut self, addr: u32, value: u8);
    fn dma_write_b_bus(&mut self, addr: u8, value: u8);
    /// Read the live B-bus register at `$2100 + addr`, honoring its real read
    /// side effects (VRAM prefetch reload/increment, OAM/CGRAM auto-increment,
    /// OPHCT/OPVCT flip-flops, WMDATA/WMADD auto-increment).
    ///
    /// `open_bus` is the byte the DMA controller last drove on the shared data
    /// bus; a port with no read driver returns it unchanged. Both references
    /// model it that way: ares `CPU::Channel::readB` reads
    /// `bus.read(0x2100 | address, cpu.r.mdr)`, and Mesen2's `ReadDma` ->
    /// `RegisterHandlerB::Read` -> `SnesPpu::Read` returns `GetOpenBus()`.
    /// Which ports those are is the bus implementation's business -- on
    /// hardware it is every write-only PPU register plus the unused
    /// `$2184-$21FF` window.
    fn dma_read_b_bus(&mut self, addr: u8, open_bus: u8) -> u8;
    /// True when `addr` decodes to WRAM on the A-bus: banks `$7E`/`$7F` in full,
    /// plus the `$0000-$1FFF` mirror in banks `$00-$3F`/`$80-$BF`.
    ///
    /// Used only to detect the WRAM-to-WRAM transfer hardware refuses (see
    /// [`DmaController::copy_dma_byte`]). Must be a pure address decode: it is
    /// evaluated before the slot's first `dma_tick`, so it must not advance the
    /// clock, read memory, or mutate any state.
    fn dma_a_bus_is_wram(&self, addr: u32) -> bool;
    /// Advance the rest of the system (PPU/APU/input) by `master_clocks` while the DMA
    /// controller owns the bus. Real hardware keeps rendering during a general-purpose DMA
    /// (Mesen2 `SnesDmaController` advances the master clock per transferred byte), so B-bus
    /// writes must land at the scan position they really occur at. Default no-op for test
    /// doubles that don't model time.
    fn dma_tick(&mut self, _master_clocks: u64) {}

    /// Claim an HDMA that fell due while a general-purpose transfer is running, so it can be
    /// run NESTED inside that transfer (Mesen2 re-enters `ProcessPendingTransfers` from
    /// inside `RunDma`; ares reaches the same shape through `dmaEdge`).
    ///
    /// Returns the pending kind ([`HDMA_KIND_INIT`] or [`HDMA_KIND_LINE`]) together with the
    /// **live** HDMAEN, which both references re-read at run time rather than latching it at
    /// the trigger. Implementations must burn the one-CPU-cycle start delay themselves
    /// (Mesen2 `_dmaStartDelay`, which swallows exactly one poll) and must clear the pending
    /// slot as they hand it over, so the bus's own cycle hook cannot run it a second time.
    ///
    /// Defaults to `None` for test doubles and for any caller that does not model HDMA.
    fn take_due_hdma(&mut self) -> Option<(u8, u8)> {
        None
    }
}

/// [`DmaABus::take_due_hdma`] kind: the once-per-frame HDMA reload (Mesen2 `InitHdmaChannels`).
pub const HDMA_KIND_INIT: u8 = 0;
/// [`DmaABus::take_due_hdma`] kind: the per-scanline HDMA transfer (Mesen2 `ProcessHdmaChannels`).
pub const HDMA_KIND_LINE: u8 = 1;

/// The two running totals a transfer accumulates. They are **not** the same quantity, and
/// conflating them is what #3067/#3074/#3127 each ran aground on.
///
/// `ticks` is real time: every master clock the controller hands to `dma_tick`. `align` is
/// Mesen2's `_dmaClockCounter` -- the bookkeeping value `SyncEndDma` divides by the CPU speed
/// -- and it deliberately differs from `ticks` in two ways, both faithful to the reference:
///
/// - a general-purpose channel charges `align` only `8 * (bytes mod 256)`, because Mesen2
///   counts its byte loop in a `uint8_t` (see `run_channel`);
/// - DRAM-refresh clocks stolen mid-transfer advance `ticks` (via the bus) but never `align`,
///   which is the reason Mesen2 keeps a separate counter at all rather than subtracting two
///   master-clock readings (upstream commit `542d7302`, "Fixed freeze in intro/menus in
///   Battle Grand Prix").
struct DmaClocks {
    ticks: u64,
    align: u64,
}

#[derive(Clone)]
pub struct DmaController {
    regs: [u8; DMA_REG_BYTES],
    hdma_active_mask: u8,
    hdma_do_transfer: [bool; 8],
    /// Which general-purpose channels are still armed, i.e. Mesen2's per-channel `DmaActive`.
    ///
    /// Live only for the duration of a `start_dma` call, which seeds it from MDMAEN. An HDMA
    /// running nested inside that transfer clears the bits of every HDMA-enabled channel
    /// (Mesen2 `ProcessHdmaChannels`/`InitHdmaChannels` do `ch.DmaActive = false`), which
    /// aborts a general-purpose transfer in flight on the same channel. Not serialised: no
    /// save state can be taken mid-transfer.
    dma_active_mask: u8,
}

impl DmaController {
    pub fn new() -> Self {
        Self {
            regs: [0; DMA_REG_BYTES],
            hdma_active_mask: 0,
            hdma_do_transfer: [false; 8],
            dma_active_mask: 0,
        }
    }

    pub fn read_register(&self, offset: u16) -> Option<u8> {
        if !(0x4300..=0x437F).contains(&offset) {
            return None;
        }
        Some(self.regs[(offset - 0x4300) as usize])
    }

    pub fn write_register(&mut self, offset: u16, value: u8) -> bool {
        if !(0x4300..=0x437F).contains(&offset) {
            return false;
        }
        self.regs[(offset - 0x4300) as usize] = value;
        true
    }

    pub(crate) fn capture_state(&self) -> SnesDmaState {
        SnesDmaState {
            regs: self.regs.to_vec(),
            hdma_active_mask: self.hdma_active_mask,
            hdma_do_transfer: self.hdma_do_transfer.to_vec(),
        }
    }

    pub(crate) fn restore_state(&mut self, state: &SnesDmaState) -> Result<(), String> {
        if state.regs.len() != DMA_REG_BYTES {
            return Err(format!(
                "DMA register state size mismatch (expected {DMA_REG_BYTES}, found {})",
                state.regs.len()
            ));
        }
        if state.hdma_do_transfer.len() != 8 {
            return Err("DMA HDMA state size mismatch".to_string());
        }

        self.regs.copy_from_slice(&state.regs);
        self.hdma_active_mask = state.hdma_active_mask;
        self.hdma_do_transfer
            .copy_from_slice(&state.hdma_do_transfer);
        Ok(())
    }

    /// Mesen2 `SnesDmaController::SyncEndDma`: after the transfer, "wait 2-8 master cycles
    /// to reach a whole number of CPU clock cycles since the pause".
    ///
    /// Callers pass the cycle length to round to. For the two HDMA envelopes that is the
    /// speed of the CPU access the transfer is standing in front of -- `SnesCpu::Read`/`Write`
    /// call `SetCpuSpeed` for the upcoming access *before* `ProcessCpuCycle`, which is where a
    /// pending transfer runs (`Idle` sets 6). Rounding an HDMA to a fixed 8 makes one that
    /// lands in front of a 6-clock register access two clocks too long; in StarWars.sfc that
    /// pushed the `$4210` poll read past the 4-clock RDNMI hold window and cost one Mode 7
    /// zoom step per frame (#3050).
    ///
    /// General-purpose DMA deliberately passes a literal 8 instead, on trace evidence -- see
    /// `start_dma` (#3067). Do not "fix" that from this doc alone.
    ///
    /// The pad is never zero: an already-aligned total still pays a full cycle.
    fn sync_end_pad(charged: u64, cpu_speed: u8) -> u64 {
        let speed = u64::from(cpu_speed);
        speed - (charged % speed)
    }

    pub fn start_dma<B: DmaABus>(
        &mut self,
        mdmaen: u8,
        abus: &mut B,
        seed_open_bus: u8,
        base_clock: u64,
        cpu_speed: u8,
    ) -> (u64, u8) {
        if mdmaen == 0 {
            return (0, seed_open_bus);
        }

        // Hardware start envelope (Mesen2 SyncStartDma): the caller invokes this
        // one full CPU cycle after the $420B write (the start-delay cycle runs
        // normally); the transfer then pauses 1-8 clocks to reach a whole
        // multiple of 8 master clocks since reset, then 8 clocks of setup
        // overhead. `SyncStartDma` *assigns* the alignment counter the pad it just
        // paid rather than zeroing it, so the pad counts toward the end rounding.
        let pad_start = 8 - (base_clock & 7);
        abus.dma_tick(pad_start);
        let mut clocks = DmaClocks {
            ticks: pad_start,
            align: pad_start,
        };
        clocks.ticks += 8;
        clocks.align += 8;
        abus.dma_tick(8);
        let mut open_bus = seed_open_bus;

        // Mesen2 arms `DmaActive` on the `$420B` write itself; a nested HDMA can disarm a
        // channel before or during its transfer.
        self.dma_active_mask = mdmaen;

        // Mesen2 `ProcessPendingTransfers` re-enters itself here, before any channel runs.
        self.run_nested_hdma(abus, &mut open_bus, &mut clocks, cpu_speed);

        for channel in 0u8..8 {
            if (self.dma_active_mask & (1 << channel)) == 0 {
                continue;
            }

            clocks.ticks += 8;
            clocks.align += 8;
            abus.dma_tick(8);
            // Mesen2 `RunDma`, after the per-channel overhead.
            self.run_nested_hdma(abus, &mut open_bus, &mut clocks, cpu_speed);
            if (self.dma_active_mask & (1 << channel)) != 0 {
                self.run_channel(channel, abus, &mut open_bus, &mut clocks, cpu_speed);
            }
            // `RunDma` disarms the channel on the way out, so a later nested HDMA cannot
            // resurrect it.
            self.dma_active_mask &= !(1 << channel);
        }

        // SyncEndDma: one pad for the whole envelope -- including any HDMA that ran nested
        // inside it -- rounding the ALIGNMENT counter (not the elapsed clocks) up to a whole
        // CPU cycle at the speed of the access the transfer is standing in front of.
        //
        // #3127 replaced a fixed 8 here. That literal survived #3067 and #3074 because it
        // reproduced Mesen2 on every vector then measured, and the reason it did is now
        // understood: Mesen2 counts a channel's bytes in a `uint8_t` and charges
        // `_dmaClockCounter += 8 * i` afterwards, so a >= 256-byte transfer contributes a
        // multiple of 2048 less than its real cost. `8 * n` is a multiple of 8 for every `n`,
        // so that mismatch is INVISIBLE while the pad rounds to 8 and only appears once it
        // rounds to 6 or 12. Every vector #3067 measured runs a big transfer; the fixed 8 was
        // compensating for the counter, not modelling the pad.
        //
        // `run_channel` now reproduces the wrap, so the two quantities agree again and the
        // divisor can be what both references say it is. See `DmaClocks` and the sibling
        // tests `general_purpose_dma_end_pad_rounds_to_the_upcoming_cpu_access_speed` and
        // `general_purpose_dma_align_counter_wraps_the_byte_count_at_256`.
        //
        // The re-entrancy gap this comment used to name as the live hypothesis was falsified
        // in #3127 and then closed anyway: `test_dmatiming/demo.smc` never writes `$420C`, so
        // no HDMA could ever have nested inside the transfer it measures. Nesting is modelled
        // now regardless (`run_nested_hdma`), which is what `test_hdmatiming` rows 9-12 check.
        let pad_end = Self::sync_end_pad(clocks.align, cpu_speed);
        abus.dma_tick(pad_end);
        clocks.ticks += pad_end;

        (clocks.ticks, open_bus)
    }

    /// Run an HDMA that fell due while this general-purpose transfer holds the bus.
    ///
    /// Mesen2 re-enters `ProcessPendingTransfers` from inside `RunDma` (after the start
    /// overhead, after each channel's overhead, and after every byte), and both HDMA entry
    /// points then see `needSync = !HasActiveDmaChannel()` -- false, because `$420B` armed the
    /// channels. So the nested run pays neither sync pad and its clocks fold into the same
    /// counter the outer `SyncEndDma` rounds. ares is identical in shape: `CPU::dmaEdge`
    /// guards both `step()` pads with `if(!dmaEnable())`.
    fn run_nested_hdma<B: DmaABus>(
        &mut self,
        abus: &mut B,
        open_bus: &mut u8,
        clocks: &mut DmaClocks,
        cpu_speed: u8,
    ) {
        let Some((kind, hdmaen)) = abus.take_due_hdma() else {
            return;
        };
        // `base_clock` only feeds the SyncStartDma pad, which `need_sync = false` skips, so
        // there is no meaningful clock to pass. `cpu_speed` is likewise unused here, but is
        // passed honestly rather than faked so the call keeps working if that ever changes.
        const NO_BASE_CLOCK: u64 = 0;
        let (charged, new_open_bus) = if kind == HDMA_KIND_INIT {
            self.hdma_init(hdmaen, abus, *open_bus, NO_BASE_CLOCK, cpu_speed, false)
        } else {
            self.hdma_do_line(hdmaen, abus, *open_bus, NO_BASE_CLOCK, cpu_speed, false)
        };
        // With both pads skipped, everything the nested run charged is bookkeeping clocks, so
        // the one figure it returns belongs in both totals.
        clocks.ticks += charged;
        clocks.align += charged;
        *open_bus = new_open_bus;
    }

    pub fn hdma_init<B: DmaABus>(
        &mut self,
        hdmaen: u8,
        abus: &mut B,
        seed_open_bus: u8,
        base_clock: u64,
        cpu_speed: u8,
        need_sync: bool,
    ) -> (u64, u8) {
        // Initialize active_mask to 0xFF (all channels potentially active).
        // Channels will be cleared from active_mask when they terminate (descriptor=$00).
        // This allows mid-scanline HDMAEN writes to work - HDMAEN gates which channels
        // process, while active_mask only tracks termination (#2943).
        self.hdma_active_mask = 0xFF;
        self.hdma_do_transfer = [false; 8];

        if hdmaen == 0 {
            return (0, seed_open_bus);
        }

        // Hardware envelope (Mesen2 InitHdmaChannels): SyncStartDma pad + 8
        // clocks of setup overhead; SyncEndDma pad at the end. Both pads are skipped when
        // this runs nested inside a general-purpose transfer, where Mesen2 evaluates
        // `needSync = !HasActiveDmaChannel()` as false and the outer envelope rounds once for
        // both (see `run_nested_hdma`).
        let mut ticks = 0;
        if need_sync {
            let pad_start = 8 - (base_clock & 7);
            abus.dma_tick(pad_start);
            ticks = pad_start;
        }
        ticks += 8;
        abus.dma_tick(8);
        let mut open_bus = seed_open_bus;

        for channel in 0u8..8 {
            if (hdmaen & (1 << channel)) == 0 {
                continue;
            }

            let dmap = self.get_reg(channel, 0x0);
            let indirect = (dmap & 0x40) != 0;

            let a1t_low = self.get_reg(channel, 0x2);
            let a1t_high = self.get_reg(channel, 0x3);
            self.set_reg(channel, 0x8, a1t_low);
            self.set_reg(channel, 0x9, a1t_high);
            // Mesen2 `InitHdmaChannels`: `ch.DmaActive = false`. Aborts any general-purpose
            // transfer this channel is running (only reachable when nested).
            self.dma_active_mask &= !(1 << channel);

            // Line-counter load: one 8-clock slot per enabled channel.
            abus.dma_tick(4);
            let descriptor = self.read_hdma_table_byte_raw(channel, abus, &mut open_bus);
            abus.dma_tick(4);
            ticks += 8;
            self.set_reg(channel, 0xA, descriptor);
            let finished = descriptor == 0;
            if finished {
                // Terminator: clear this channel's bit so it's not treated as active
                self.hdma_active_mask &= !(1 << channel);
            }

            if indirect {
                // A terminated channel still pays for (and performs) the LSB
                // read, which lands in the pointer's HIGH byte with a zero low
                // byte (Mesen2 InitHdmaChannels' one-byte case).
                abus.dma_tick(4);
                let indirect_low = self.read_hdma_table_byte_raw(channel, abus, &mut open_bus);
                abus.dma_tick(4);
                ticks += 8;
                if finished {
                    self.set_reg(channel, 0x5, 0x00);
                    self.set_reg(channel, 0x6, indirect_low);
                } else {
                    abus.dma_tick(4);
                    let indirect_high = self.read_hdma_table_byte_raw(channel, abus, &mut open_bus);
                    abus.dma_tick(4);
                    ticks += 8;
                    self.set_reg(channel, 0x5, indirect_low);
                    self.set_reg(channel, 0x6, indirect_high);
                }
            }

            if !finished {
                // active_mask already 0xFF; do_transfer enables this channel
                self.hdma_do_transfer[channel as usize] = true;
            }
        }

        if need_sync {
            // SyncEndDma: round the charged total up to a whole CPU cycle (see `sync_end_pad`).
            let pad_end = Self::sync_end_pad(ticks, cpu_speed);
            abus.dma_tick(pad_end);
            ticks += pad_end;
        }

        (ticks, open_bus)
    }

    /// Perform the once-per-scanline HDMA processing: check each enabled channel,
    /// transfer data if due, decrement line counters, and reload descriptors as needed.
    pub fn hdma_do_line<B: DmaABus>(
        &mut self,
        hdmaen: u8,
        abus: &mut B,
        seed_open_bus: u8,
        base_clock: u64,
        cpu_speed: u8,
        need_sync: bool,
    ) -> (u64, u8) {
        if hdmaen == 0 {
            return (0, seed_open_bus);
        }

        // Hardware envelope (Mesen2 ProcessHdmaChannels): SyncStartDma pads
        // 1-8 clocks to a multiple of 8 master clocks since reset, then 8
        // clocks of setup overhead. Both pads are skipped when this runs nested inside a
        // general-purpose transfer (see `run_nested_hdma`).
        let mut counter = 0;
        if need_sync {
            let pad_start = 8 - (base_clock & 7);
            abus.dma_tick(pad_start);
            counter = pad_start;
        }
        counter += 8;
        abus.dma_tick(8);
        let mut open_bus = seed_open_bus;

        // Phase A: run every active channel's transfer for this line first.
        for channel in 0u8..8 {
            if (hdmaen & (1 << channel)) == 0 {
                continue;
            }
            // Mesen2 `ProcessHdmaChannels`: `ch.DmaActive = false` for every HDMA-enabled
            // channel, before the finished check. Aborts a general-purpose transfer running
            // on that channel (only reachable when nested).
            self.dma_active_mask &= !(1 << channel);
            if (self.hdma_active_mask & (1 << channel)) == 0 {
                continue;
            }
            if self.hdma_do_transfer[channel as usize] {
                counter += self.run_hdma_transfer_unit(channel, abus, &mut open_bus);
            }
        }

        // Phase B: per-channel bookkeeping. The next table byte is read EVERY
        // line (8 clocks, Mesen2 "Read the next byte from Address into $43xA");
        // its value is only consumed when the line counter expires.
        for channel in 0u8..8 {
            if (hdmaen & (1 << channel)) == 0 || (self.hdma_active_mask & (1 << channel)) == 0 {
                continue;
            }
            let idx = channel as usize;
            // `$43xA` IS the line counter and repeat flag -- there is no separate
            // internal copy to decrement (Mesen2 `ch.HdmaLineCounterAndRepeat--`).
            // Keeping one meant a CPU write to `$43xA` was ignored until the next
            // frame's init, so a ROM that arms HDMA mid-frame decremented a stale
            // zero to `$FF` instead of the value it had just written (#3062).
            // Use wrapping_sub to match hardware (0 - 1 = 0xFF, not 0).
            let line_counter = self.get_reg(channel, 0xA).wrapping_sub(1);
            self.set_reg(channel, 0xA, line_counter);

            // Set do_transfer based on repeat bit (bit 7) of line counter register.
            // This also allows mid-scanline activation to work (#2943).
            self.hdma_do_transfer[idx] = (line_counter & 0x80) != 0;

            // Speculative descriptor read (pointer NOT advanced unless consumed).
            abus.dma_tick(4);
            let descriptor = abus.dma_read_a_bus(self.hdma_table_address(channel), open_bus);
            open_bus = descriptor;
            abus.dma_tick(4);
            counter += 8;

            // Expiry is tested on the low 7 bits only, so a repeat-mode counter
            // expires at `$80` (Mesen2 `(ch.HdmaLineCounterAndRepeat & 0x7F) == 0`).
            if (line_counter & 0x7F) == 0 {
                // Consume the descriptor: advance the table pointer.
                let next =
                    u16::from_le_bytes([self.get_reg(channel, 0x8), self.get_reg(channel, 0x9)])
                        .wrapping_add(1);
                let [next_low, next_high] = next.to_le_bytes();
                self.set_reg(channel, 0x8, next_low);
                self.set_reg(channel, 0x9, next_high);
                self.set_reg(channel, 0xA, descriptor);

                let indirect = (self.get_reg(channel, 0x0) & 0x40) != 0;
                if indirect {
                    // Indirect pointer load happens BEFORE the termination
                    // check; a $00 descriptor on the last active channel loads
                    // only the high byte (Mesen2's one-byte oddity).
                    if descriptor == 0 && self.is_last_active_hdma_channel(hdmaen, channel) {
                        abus.dma_tick(4);
                        let msb = self.read_hdma_table_byte_raw(channel, abus, &mut open_bus);
                        abus.dma_tick(4);
                        counter += 8;
                        self.set_reg(channel, 0x5, 0x00);
                        self.set_reg(channel, 0x6, msb);
                    } else {
                        abus.dma_tick(4);
                        let lsb = self.read_hdma_table_byte_raw(channel, abus, &mut open_bus);
                        abus.dma_tick(4);
                        abus.dma_tick(4);
                        let msb = self.read_hdma_table_byte_raw(channel, abus, &mut open_bus);
                        abus.dma_tick(4);
                        counter += 16;
                        self.set_reg(channel, 0x5, lsb);
                        self.set_reg(channel, 0x6, msb);
                    }
                }

                if descriptor == 0 {
                    self.hdma_active_mask &= !(1 << channel);
                    self.hdma_do_transfer[idx] = false;
                } else {
                    self.hdma_do_transfer[idx] = true;
                }
            }
        }

        if need_sync {
            // SyncEndDma: pad clocks rounding the charged total to a whole CPU cycle
            // (see `sync_end_pad`).
            let pad_end = Self::sync_end_pad(counter, cpu_speed);
            abus.dma_tick(pad_end);
            counter += pad_end;
        }

        (counter, open_bus)
    }

    /// True when `channel` is the highest-numbered still-active enabled HDMA
    /// channel this line (Mesen2 `IsLastActiveHdmaChannel`).
    fn is_last_active_hdma_channel(&self, hdmaen: u8, channel: u8) -> bool {
        ((channel + 1)..8)
            .all(|c| (hdmaen & (1 << c)) == 0 || (self.hdma_active_mask & (1 << c)) == 0)
    }

    /// Read one HDMA table byte and advance the channel's table pointer,
    /// WITHOUT charging clocks (the caller places the 4/4 slot ticks).
    fn read_hdma_table_byte_raw<B: DmaABus>(
        &mut self,
        channel: u8,
        abus: &mut B,
        open_bus: &mut u8,
    ) -> u8 {
        let value = abus.dma_read_a_bus(self.hdma_table_address(channel), *open_bus);
        *open_bus = value;
        let next = u16::from_le_bytes([self.get_reg(channel, 0x8), self.get_reg(channel, 0x9)])
            .wrapping_add(1);
        let [next_low, next_high] = next.to_le_bytes();
        self.set_reg(channel, 0x8, next_low);
        self.set_reg(channel, 0x9, next_high);
        value
    }

    fn run_channel<B: DmaABus>(
        &mut self,
        channel: u8,
        abus: &mut B,
        open_bus: &mut u8,
        clocks: &mut DmaClocks,
        cpu_speed: u8,
    ) {
        let dmap = self.get_reg(channel, 0x0);
        let bbad = self.get_reg(channel, 0x1);
        let mut a_low = self.get_reg(channel, 0x2);
        let mut a_high = self.get_reg(channel, 0x3);
        let bank = self.get_reg(channel, 0x4);
        let das_low = self.get_reg(channel, 0x5);
        let das_high = self.get_reg(channel, 0x6);

        let mode = Self::canonical_mode(dmap & 0x07);
        let pattern = Self::transfer_offsets(mode);
        let direction_b_to_a = (dmap & 0x80) != 0;
        let step = (dmap >> 3) & 0x03;

        let mut count = u16::from_le_bytes([das_low, das_high]);
        let transfer_bytes: usize = if count == 0 { 0x1_0000 } else { count as usize };

        // Mesen2's byte counter is a `uint8_t` (`SnesDmaController.cpp:93`), and the whole
        // channel's alignment charge is `8 * i` applied AFTER the loop -- so this deliberately
        // wraps at 256 while the real clocks below do not. See `DmaClocks`.
        let mut performed: u8 = 0;

        for i in 0..transfer_bytes {
            let a_addr = ((bank as u32) << 16) | ((a_high as u32) << 8) | (a_low as u32);
            let b_addr = bbad.wrapping_add(pattern[i % pattern.len()]);

            Self::copy_dma_byte(abus, open_bus, a_addr, b_addr, direction_b_to_a);

            clocks.ticks += 8;
            performed = performed.wrapping_add(1);
            count = count.wrapping_sub(1);
            (a_low, a_high) = Self::advance_a_address(a_low, a_high, step);

            // Mesen2 re-enters `ProcessPendingTransfers` after every byte, then re-tests
            // `channel.DmaActive` in the loop condition -- so an HDMA that disarms this
            // channel stops the transfer here, with `$43x5` still non-zero.
            self.run_nested_hdma(abus, open_bus, clocks, cpu_speed);
            if (self.dma_active_mask & (1 << channel)) == 0 {
                break;
            }
        }
        clocks.align += 8 * u64::from(performed);

        self.set_reg(channel, 0x2, a_low);
        self.set_reg(channel, 0x3, a_high);
        self.set_reg(channel, 0x5, (count & 0x00FF) as u8);
        self.set_reg(channel, 0x6, (count >> 8) as u8);
    }

    /// Moves one byte through its 8-master-clock bus slot, in either direction
    /// (Mesen2 `SnesDmaController::CopyDmaByte`, shared by its GPDMA and HDMA
    /// drivers for the same reason it is shared here: the WRAM rule below must
    /// not drift between the two).
    ///
    /// The read fills the first 4 clocks and the destination write lands at the
    /// END of the slot (Mesen2: `ReadDma` advances 4, then `WriteDma` advances 4
    /// more BEFORE calling the register handler).
    ///
    /// fullsnes "DMA Notes": *"WRAM-to-WRAM DMA isn't possible (neither in
    /// A-Bus to B-Bus direction, nor vice-versa). Externally, the separate
    /// address lines are there, but the WRAM chip is unable to process both at
    /// once."* So a slot whose B-bus address is WMDATA and whose A-bus address
    /// is WRAM is refused:
    ///
    /// - A->B: neither access happens. In particular `$2180` is never written,
    ///   so the WMADD counter does not advance.
    /// - B->A: `$2180` is never read (again no WMADD advance), but the A-bus
    ///   write still occurs, depositing [`INVALID_WRAM_TRANSFER_BYTE`].
    ///
    /// Either way the slot still costs its full 8 master clocks and the DMA open
    /// bus is left unchanged. The channel's address/count bookkeeping is the
    /// caller's job and runs regardless -- byuu's `test_dmavalid` checks that
    /// `$43x2` still advanced and `$43x5` still reached zero across 1024
    /// refused slots, and that the transfer still consumed its ~6 scanlines.
    ///
    /// The predicate is tested against the slot's own offset `b_addr`, not the
    /// channel's raw BBAD: a mode-1 channel with BBAD `$7F` alternates
    /// `$7F`/`$80`, so only every other slot is refused.
    fn copy_dma_byte<B: DmaABus>(
        abus: &mut B,
        open_bus: &mut u8,
        a_addr: u32,
        b_addr: u8,
        direction_b_to_a: bool,
    ) {
        let valid = b_addr != WMDATA_B_BUS_ADDR || !abus.dma_a_bus_is_wram(a_addr);
        abus.dma_tick(4);
        match (direction_b_to_a, valid) {
            (false, true) => {
                let value = abus.dma_read_a_bus(a_addr, *open_bus);
                *open_bus = value;
                abus.dma_tick(4);
                abus.dma_write_b_bus(b_addr, value);
            }
            (false, false) => abus.dma_tick(4),
            (true, true) => {
                let value = abus.dma_read_b_bus(b_addr, *open_bus);
                *open_bus = value;
                abus.dma_tick(4);
                abus.dma_write_a_bus(a_addr, value);
            }
            (true, false) => {
                abus.dma_tick(4);
                abus.dma_write_a_bus(a_addr, INVALID_WRAM_TRANSFER_BYTE);
            }
        }
    }

    fn canonical_mode(mode: u8) -> u8 {
        match mode {
            0x5 => 0x1,
            0x6 => 0x2,
            0x7 => 0x3,
            _ => mode,
        }
    }

    fn transfer_offsets(mode: u8) -> &'static [u8] {
        match mode {
            0x0 => &[0],
            0x1 => &[0, 1],
            0x2 => &[0, 0],
            0x3 => &[0, 0, 1, 1],
            0x4 => &[0, 1, 2, 3],
            _ => &[0],
        }
    }

    /// The B-bus offset pattern HDMA writes/reads once per active scanline for
    /// a given raw DMAP transfer-mode field (0-7).
    ///
    /// Unlike GPDMA (`run_channel`/`transfer_offsets`+`canonical_mode`), which
    /// cycles its pattern over an arbitrary total byte count -- so modes 5/6/7
    /// are byte-identical to modes 1/2/3 once cycled and can be safely
    /// canonicalized down -- HDMA performs exactly *one* full pattern per line
    /// with no cycling. Mode 5 ("2 registers, written twice each": B, B+1, B,
    /// B+1) therefore transfers 4 bytes per line, not the 2 bytes of mode 1's
    /// pattern; collapsing it to mode 1 silently drops half the table bytes
    /// each line and desyncs the per-channel table pointer, corrupting every
    /// entry read afterward. See #2952.
    fn hdma_transfer_offsets(mode: u8) -> &'static [u8] {
        match mode {
            0x0 => &[0],
            0x1 => &[0, 1],
            0x2 | 0x6 => &[0, 0],
            0x3 | 0x7 => &[0, 0, 1, 1],
            0x4 => &[0, 1, 2, 3],
            0x5 => &[0, 1, 0, 1],
            _ => &[0],
        }
    }

    fn advance_a_address(low: u8, high: u8, step: u8) -> (u8, u8) {
        let mut addr = u16::from_le_bytes([low, high]);
        match step {
            0x0 => addr = addr.wrapping_add(1),
            0x2 => addr = addr.wrapping_sub(1),
            _ => {}
        }
        let [next_low, next_high] = addr.to_le_bytes();
        (next_low, next_high)
    }

    fn get_reg(&self, channel: u8, reg: usize) -> u8 {
        self.regs[(channel as usize) * 0x10 + reg]
    }

    fn set_reg(&mut self, channel: u8, reg: usize, value: u8) {
        self.regs[(channel as usize) * 0x10 + reg] = value;
    }

    fn hdma_table_address(&self, channel: u8) -> u32 {
        let bank = self.get_reg(channel, 0x4);
        let low = self.get_reg(channel, 0x8);
        let high = self.get_reg(channel, 0x9);
        ((bank as u32) << 16) | u16::from_le_bytes([low, high]) as u32
    }

    fn run_hdma_transfer_unit<B: DmaABus>(
        &mut self,
        channel: u8,
        abus: &mut B,
        open_bus: &mut u8,
    ) -> u64 {
        let dmap = self.get_reg(channel, 0x0);
        let bbad = self.get_reg(channel, 0x1);
        let pattern = Self::hdma_transfer_offsets(dmap & 0x07);
        let direction_b_to_a = (dmap & 0x80) != 0;
        let indirect = (dmap & 0x40) != 0;

        let bank = if indirect {
            self.get_reg(channel, 0x7)
        } else {
            self.get_reg(channel, 0x4)
        };
        let mut addr = if indirect {
            u16::from_le_bytes([self.get_reg(channel, 0x5), self.get_reg(channel, 0x6)])
        } else {
            u16::from_le_bytes([self.get_reg(channel, 0x8), self.get_reg(channel, 0x9)])
        };

        for offset in pattern {
            let a_addr = ((bank as u32) << 16) | addr as u32;
            let b_addr = bbad.wrapping_add(*offset);
            Self::copy_dma_byte(abus, open_bus, a_addr, b_addr, direction_b_to_a);
            addr = addr.wrapping_add(1);
        }

        let [low, high] = addr.to_le_bytes();
        if indirect {
            self.set_reg(channel, 0x5, low);
            self.set_reg(channel, 0x6, high);
        } else {
            self.set_reg(channel, 0x8, low);
            self.set_reg(channel, 0x9, high);
        }

        (pattern.len() as u64) * 8
    }
}

impl Default for DmaController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A-bus stub with its own master-clock counter: `dma_tick` advances it and
    /// every B-bus write is recorded with the clock it lands at, so tests can
    /// pin the hardware envelope's per-byte write clocks exactly.
    ///
    /// `b_bus_ports` is a flat, always-readable port file. That is *not* how the
    /// real B-bus behaves (see #3061), but this is a fake bus: its job is to let
    /// the controller's own tests pin transfer patterns and slot clocks without
    /// dragging in PPU register physics. The production path reads live
    /// registers via `SnesSystemBus::dma_read_b_bus`.
    ///
    /// `hdma_due_at_poll` scripts the pending-HDMA slot the real bus keeps: the payload is
    /// handed over on that 1-based [`DmaABus::take_due_hdma`] call and never again, which
    /// lets a test place a nested HDMA at an exact point inside a general-purpose transfer.
    struct RecordingBus {
        clock: u64,
        a_bus: Vec<u8>,
        b_bus_writes: Vec<(u64, u8, u8)>,
        b_bus_ports: Vec<u8>,
        b_bus_reads: Vec<(u64, u8)>,
        hdma_polls: u32,
        hdma_due_at_poll: Option<u32>,
        hdma_payload: (u8, u8),
    }

    impl RecordingBus {
        fn new(clock: u64) -> Self {
            Self {
                clock,
                a_bus: vec![0; 0x1_0000],
                b_bus_writes: Vec::new(),
                b_bus_ports: vec![0; 0x100],
                b_bus_reads: Vec::new(),
                hdma_polls: 0,
                hdma_due_at_poll: None,
                hdma_payload: (HDMA_KIND_LINE, 0),
            }
        }

        /// Make `take_due_hdma` yield `(kind, hdmaen)` on the `poll`-th call (1-based).
        fn schedule_hdma(&mut self, poll: u32, kind: u8, hdmaen: u8) {
            self.hdma_due_at_poll = Some(poll);
            self.hdma_payload = (kind, hdmaen);
        }
    }

    impl DmaABus for RecordingBus {
        fn dma_read_a_bus(&mut self, addr: u32, _open_bus: u8) -> u8 {
            self.a_bus[(addr & 0xFFFF) as usize]
        }

        fn dma_write_a_bus(&mut self, addr: u32, value: u8) {
            self.a_bus[(addr & 0xFFFF) as usize] = value;
        }

        fn dma_write_b_bus(&mut self, addr: u8, value: u8) {
            self.b_bus_writes.push((self.clock, addr, value));
        }

        fn dma_read_b_bus(&mut self, addr: u8, _open_bus: u8) -> u8 {
            self.b_bus_reads.push((self.clock, addr));
            self.b_bus_ports[addr as usize]
        }

        /// Deliberately restates the WRAM map in ares' mask form
        /// (`CPU::Channel::transfer`) rather than delegating to the production
        /// decoder, so these tests check the rule against an independent
        /// statement of the spec instead of against the implementation.
        fn dma_a_bus_is_wram(&self, addr: u32) -> bool {
            (addr & 0xFE_0000) == 0x7E_0000 || (addr & 0x40_E000) == 0x00_0000
        }

        fn dma_tick(&mut self, master_clocks: u64) {
            self.clock += master_clocks;
        }

        fn take_due_hdma(&mut self) -> Option<(u8, u8)> {
            self.hdma_polls += 1;
            if self.hdma_due_at_poll == Some(self.hdma_polls) {
                self.hdma_due_at_poll = None;
                return Some(self.hdma_payload);
            }
            None
        }
    }

    fn write_hdma_channel(dma: &mut DmaController, channel: u8, dmap: u8, bbad: u8, a_addr: u16) {
        let base = 0x4300 + u16::from(channel) * 0x10;
        dma.write_register(base, dmap);
        dma.write_register(base + 0x1, bbad);
        dma.write_register(base + 0x2, (a_addr & 0xFF) as u8);
        dma.write_register(base + 0x3, (a_addr >> 8) as u8);
        dma.write_register(base + 0x4, 0x00);
    }

    #[test]
    fn hdma_do_line_write_clocks_follow_the_hardware_envelope() {
        // Two mode-2 channels (2 bytes to one register). Run at an 8-aligned
        // clock (1112): SyncStartDma pad 8 -> 1120, overhead 8 -> 1128, then
        // Phase A byte slots of 8 clocks each with the B-bus write at the
        // slot's END (Mesen2 CopyDmaByte: 4 clocks to the A-bus read, 4 more
        // before the B-bus write): ch0 at 1136/1144, ch1 -- with NO
        // per-channel overhead between Phase A transfers -- at 1152/1160.
        let mut dma = DmaController::new();
        let mut bus = RecordingBus::new(1112);
        write_hdma_channel(&mut dma, 0, 0x02, 0x22, 0x3000);
        bus.a_bus[0x3000] = 0xFF; // repeat, 127 lines
        bus.a_bus[0x3001] = 0xA1;
        bus.a_bus[0x3002] = 0xA2;
        write_hdma_channel(&mut dma, 1, 0x02, 0x24, 0x3100);
        bus.a_bus[0x3100] = 0xFF;
        bus.a_bus[0x3101] = 0xB1;
        bus.a_bus[0x3102] = 0xB2;

        let init_clock = bus.clock;
        dma.hdma_init(0x03, &mut bus, 0, init_clock, 8, true);
        let base_clock = bus.clock;
        assert_eq!(base_clock % 8, 0, "init ends on a CPU cycle boundary");

        let start = bus.clock;
        let (counter, _) = dma.hdma_do_line(0x03, &mut bus, 0, base_clock, 8, true);

        assert_eq!(
            bus.b_bus_writes,
            vec![
                (start + 24, 0x22, 0xA1),
                (start + 32, 0x22, 0xA2),
                (start + 40, 0x24, 0xB1),
                (start + 48, 0x24, 0xB2),
            ],
            "per-byte write clocks follow the envelope"
        );
        // pad 8 + overhead 8 + 4 byte slots (32) + 2 speculative descriptor
        // reads (16) + pad_end 8.
        assert_eq!(counter, 72, "charged total");
        assert_eq!(bus.clock - start, 72, "bus advanced in lockstep");
    }

    /// Mesen2 `SyncEndDma` rounds the charged transfer up to a whole *current* CPU cycle:
    /// `cpuSpeed - (_dmaClockCounter % cpuSpeed)`, where `_cpuSpeed` was set by
    /// `SnesCpu::Read`/`Write` for the access that is about to run (`SetCpuSpeed` precedes
    /// `ProcessCpuCycle`, which is where the pending transfer executes). A 6-clock access
    /// therefore ends the DMA two clocks earlier than an 8-clock one.
    ///
    /// #3050: StarWars' Mode 7 zoom counter is advanced from a `$4210` poll loop, and
    /// `$4210` is a 6-clock access. Padding to a fixed 8 delayed that poll read past the
    /// 4-clock RDNMI hold window, costing one zoom step per frame.
    #[test]
    fn sync_end_pad_rounds_up_to_a_whole_cpu_cycle_and_is_never_zero() {
        // An already-aligned total still pays a full cycle ("wait 2-8 master cycles").
        assert_eq!(DmaController::sync_end_pad(40, 8), 8);
        assert_eq!(DmaController::sync_end_pad(40, 6), 2);
        assert_eq!(DmaController::sync_end_pad(40, 12), 8);
        assert_eq!(DmaController::sync_end_pad(42, 6), 6);
        for speed in [6u8, 8, 12] {
            for charged in 0..64u64 {
                let pad = DmaController::sync_end_pad(charged, speed);
                assert!((1..=u64::from(speed)).contains(&pad), "pad in 1..=speed");
                assert_eq!(
                    (charged + pad) % u64::from(speed),
                    0,
                    "ends on a whole cycle"
                );
            }
        }
    }

    #[test]
    fn hdma_do_line_end_pad_rounds_to_the_upcoming_cpu_access_speed() {
        // One mode-2 channel from an 8-aligned base: pad_start 8 + overhead 8 + 2 byte
        // slots (16) + speculative descriptor read (8) = 40 charged before the end pad.
        // `hdma_init` always runs at speed 8 here so only the line transfer's end pad varies.
        let charged = |cpu_speed: u8| {
            let mut dma = DmaController::new();
            let mut bus = RecordingBus::new(1112);
            write_hdma_channel(&mut dma, 0, 0x02, 0x22, 0x3000);
            bus.a_bus[0x3000] = 0xFF;
            bus.a_bus[0x3001] = 0xA1;
            bus.a_bus[0x3002] = 0xA2;
            let init_clock = bus.clock;
            dma.hdma_init(0x01, &mut bus, 0, init_clock, 8, true);
            let base_clock = bus.clock;
            let (counter, _) = dma.hdma_do_line(0x01, &mut bus, 0, base_clock, cpu_speed, true);
            assert_eq!(bus.clock - base_clock, counter, "bus advanced in lockstep");
            counter
        };

        assert_eq!(
            charged(8),
            48,
            "8-clock access pads to a whole 8-clock cycle"
        );
        assert_eq!(
            charged(6),
            42,
            "6-clock access pads to a whole 6-clock cycle"
        );
        assert_eq!(
            charged(12),
            48,
            "12-clock access pads to a whole 12-clock cycle"
        );
    }

    /// Same rule for the once-per-frame HDMA init and for general-purpose DMA -- all three
    /// envelopes share Mesen2's single `SyncEndDma`.
    #[test]
    fn hdma_init_end_pad_rounds_to_the_upcoming_cpu_access_speed() {
        let charged = |cpu_speed: u8| {
            let mut dma = DmaController::new();
            let mut bus = RecordingBus::new(0);
            write_hdma_channel(&mut dma, 0, 0x00, 0x22, 0x3000);
            bus.a_bus[0x3000] = 0x83;
            bus.a_bus[0x3001] = 0x11;
            dma.hdma_init(0x01, &mut bus, 0, 0, cpu_speed, true).0
        };
        assert_eq!(charged(6) % 6, 0, "init ends on a whole 6-clock CPU cycle");
        assert_eq!(charged(8) % 8, 0, "init ends on a whole 8-clock CPU cycle");
        assert_ne!(charged(6), charged(8), "the speed must change the end pad");
    }

    /// Same rule again for general-purpose DMA: all three envelopes share Mesen2's single
    /// `SyncEndDma`, and ares reaches the identical formula from the other direction
    /// (`ares/sfc/cpu/timing.cpp:129`, `status.clockCount - counter.dma % status.clockCount`).
    ///
    /// This test replaces `general_purpose_dma_end_pad_stays_on_a_fixed_eight_clock_cycle`,
    /// which asserted `charged(6) == charged(8) == charged(12) == 64` and so **encoded the
    /// #3127 defect**: it passed only because `start_dma` ignored its `cpu_speed` argument.
    /// The fixed 8 was never trace-verified as a rule -- it happened to reproduce Mesen2 on
    /// the vectors #3067 measured, all of which run a >= 256-byte transfer where Mesen2's own
    /// byte counter wraps (see the sibling wrap test below).
    #[test]
    fn general_purpose_dma_end_pad_rounds_to_the_upcoming_cpu_access_speed() {
        let charged = |cpu_speed: u8| {
            let mut dma = DmaController::new();
            let mut bus = RecordingBus::new(0);
            // One channel, 4 bytes, mode 0, A-bus $3000 -> B-bus $2118.
            dma.write_register(0x4300, 0x00);
            dma.write_register(0x4301, 0x18);
            dma.write_register(0x4302, 0x00);
            dma.write_register(0x4303, 0x30);
            dma.write_register(0x4304, 0x00);
            dma.write_register(0x4305, 0x04);
            dma.write_register(0x4306, 0x00);
            let (counter, _) = dma.start_dma(0x01, &mut bus, 0, 0, cpu_speed);
            assert_eq!(bus.clock, counter, "bus advanced in lockstep");
            counter
        };
        // pad_start 8 + overhead 8 + channel 8 + 4 byte slots (32) = 56 charged before the pad.
        // 56 is a whole 8-clock cycle already, so the 8 case still pays a full extra 8; 6 and
        // 12 both need 4. The three results must not collapse to one value.
        assert_eq!(charged(8), 64, "ends on a whole 8-clock CPU cycle");
        assert_eq!(charged(6), 60, "ends on a whole 6-clock CPU cycle");
        assert_eq!(charged(12), 60, "ends on a whole 12-clock CPU cycle");
    }

    /// Mesen2 counts a general-purpose channel's bytes in a `uint8_t` and charges the
    /// alignment counter in bulk afterwards (`SnesDmaController.cpp:93-99`):
    ///
    /// ```text
    /// uint8_t i = 0;
    /// do { CopyDmaByte(...); channel.TransferSize--; i++; ... } while(...);
    /// _dmaClockCounter += 8 * i;
    /// ```
    ///
    /// so a 256-byte channel contributes **zero** clocks to the end-pad alignment while still
    /// costing its full 2048 master clocks of real time. ares does not do this
    /// (`counter.dma` is a `u32` fed by every byte through `CPU::Channel::step`), so this is a
    /// Mesen2 artifact rather than hardware behaviour -- deliberately reproduced because every
    /// committed SNES screen golden is a Mesen2 capture that encodes it. #3127.
    ///
    /// This is the mechanism that hid the wrong divisor for two issues: `8 * n` is a multiple
    /// of 8 for every `n`, so the wrap is invisible while the pad rounds to 8 and only appears
    /// once it rounds to 6 or 12.
    ///
    /// Measured, not argued. Making `performed` an exact `u64` count -- the ares rule, and
    /// precisely the change #3067 tried -- turns this test red **and** takes
    /// `peterlemon_ppu_advanced_tests::mosaic_mode5_sized` from 0 px to failing, while
    /// `test_dmatiming_latches_hv_after_gpdma` stays green. That is #3067's recorded outcome
    /// reproduced exactly, and it is the evidence that the wrap -- not the divisor -- was the
    /// missing piece.
    #[test]
    fn general_purpose_dma_align_counter_wraps_the_byte_count_at_256() {
        // One channel, `bytes` bytes, mode 0, A-bus $3000 -> B-bus $2118, at cpu_speed 6.
        let charged = |bytes: u16| {
            let mut dma = DmaController::new();
            let mut bus = RecordingBus::new(0);
            dma.write_register(0x4300, 0x00);
            dma.write_register(0x4301, 0x18);
            dma.write_register(0x4302, 0x00);
            dma.write_register(0x4303, 0x30);
            dma.write_register(0x4304, 0x00);
            dma.write_register(0x4305, (bytes & 0xFF) as u8);
            dma.write_register(0x4306, (bytes >> 8) as u8);
            let (counter, _) = dma.start_dma(0x01, &mut bus, 0, 0, 6);
            assert_eq!(bus.clock, counter, "bus advanced in lockstep");
            counter
        };

        // Real time is charged for every byte: 256 bytes cost 2048 clocks more than 0... but
        // the alignment counter sees `8 * (256 as u8)` = 0 for the 256-byte channel, exactly
        // as it sees `8 * 4` = 32 for a 4-byte one. So both end pads are computed from
        // `pad_start 8 + overhead 8 + channel 8 + 8 * (n mod 256)`.
        //   4 bytes  -> align 56, pad 4  -> 60 total
        //   256 bytes-> align 24, pad 6  -> 8 + 8 + 8 + 2048 + 6 = 2078
        //   260 bytes-> align 56, pad 4  -> 8 + 8 + 8 + 2080 + 4 = 2108
        assert_eq!(charged(4), 60, "small transfer: no wrap");
        assert_eq!(charged(256), 2078, "256 bytes charge the counter nothing");
        assert_eq!(charged(260), 2108, "260 bytes charge the counter as 4 do");

        // And this is precisely why the wrong divisor survived two issues: at cpu_speed 8 the
        // alignment counter is a multiple of 8 whether or not the byte count wrapped, so the
        // pad is a flat 8 in every case and the mismatch is unobservable. Only 6 and 12 expose
        // it -- which is the speed both #3127 witnesses actually run at.
        let charged_at_8 = |bytes: u16| {
            let mut dma = DmaController::new();
            let mut bus = RecordingBus::new(0);
            dma.write_register(0x4300, 0x00);
            dma.write_register(0x4301, 0x18);
            dma.write_register(0x4302, 0x00);
            dma.write_register(0x4303, 0x30);
            dma.write_register(0x4304, 0x00);
            dma.write_register(0x4305, (bytes & 0xFF) as u8);
            dma.write_register(0x4306, (bytes >> 8) as u8);
            dma.start_dma(0x01, &mut bus, 0, 0, 8).0
        };
        for bytes in [4u16, 256, 260] {
            assert_eq!(
                charged_at_8(bytes),
                32 + 8 * u64::from(bytes),
                "at cpu_speed 8 the pad is a flat 8 regardless of the wrap ({bytes} bytes)"
            );
        }
    }

    /// An HDMA falling due while a general-purpose transfer holds the bus runs NESTED inside
    /// it: Mesen2 re-enters `ProcessPendingTransfers` from `RunDma` after every byte
    /// (`SnesDmaController.cpp:96`), and both HDMA entry points then take
    /// `needSync = !HasActiveDmaChannel()` -- false here -- so the nested run pays **no**
    /// `SyncStartDma` pad and **no** `SyncEndDma` pad, and its clocks accumulate into the same
    /// `_dmaClockCounter` the outer `SyncEndDma` finally rounds. ares has the identical shape
    /// (`CPU::dmaEdge` guards both `step()` pads with `if(!dmaEnable())`).
    ///
    /// The absolute write clocks below pin both halves: an unpaid start pad would push the
    /// nested B-bus write from 48 to 56, and an unpaid end pad would push the outer
    /// transfer's second byte from 64 to 72. #3127.
    #[test]
    fn hdma_falling_due_inside_a_general_purpose_transfer_runs_nested_without_sync_pads() {
        let mut dma = DmaController::new();
        let mut bus = RecordingBus::new(0);

        // HDMA channel 1: mode 0 (one byte), direct, table at $003100.
        write_hdma_channel(&mut dma, 1, 0x00, 0x24, 0x3100);
        bus.a_bus[0x3100] = 0x83; // repeat, 3 lines
        bus.a_bus[0x3101] = 0xB1; // the byte this line transfers
        bus.a_bus[0x3102] = 0x82; // speculatively re-read every line
        let init_clock = bus.clock;
        dma.hdma_init(0x02, &mut bus, 0, init_clock, 8, true);

        // General-purpose channel 0: mode 0, 4 bytes, $003000 -> $2118.
        dma.write_register(0x4300, 0x00);
        dma.write_register(0x4301, 0x18);
        dma.write_register(0x4302, 0x00);
        dma.write_register(0x4303, 0x30);
        dma.write_register(0x4304, 0x00);
        dma.write_register(0x4305, 0x04);
        dma.write_register(0x4306, 0x00);
        for (i, byte) in [0xA0u8, 0xA1, 0xA2, 0xA3].into_iter().enumerate() {
            bus.a_bus[0x3000 + i] = byte;
        }

        // Poll 3 is the one taken right after the first general-purpose byte (poll 1 follows
        // the 8-clock start overhead, poll 2 the channel's own 8-clock overhead).
        bus.schedule_hdma(3, HDMA_KIND_LINE, 0x02);
        bus.b_bus_writes.clear();
        let start = bus.clock;
        let (ticks, _) = dma.start_dma(0x01, &mut bus, 0, start, 6);

        assert_eq!(
            bus.b_bus_writes,
            vec![
                (start + 32, 0x18, 0xA0),
                (start + 48, 0x24, 0xB1),
                (start + 64, 0x18, 0xA1),
                (start + 72, 0x18, 0xA2),
                (start + 80, 0x18, 0xA3),
            ],
            "the nested HDMA burst is interleaved between two general-purpose byte slots"
        );
        // align = pad_start 8 + overhead 8 + channel 8 + 4 bytes (32) + nested 24 = 80,
        // so the single end pad is 6 - (80 % 6) = 4.
        assert_eq!(ticks, 84, "one envelope, one end pad, rounded on the total");
        assert_eq!(bus.clock - start, 84, "bus advanced in lockstep");
    }

    /// Mesen2's `ProcessHdmaChannels` clears `DmaActive` on **every** HDMA-enabled channel
    /// (`SnesDmaController.cpp:255`), so an HDMA firing mid-transfer aborts a general-purpose
    /// transfer running on that same channel: `RunDma`'s `while(TransferSize > 0 &&
    /// channel.DmaActive)` terminates immediately, leaving `$43x5` non-zero and `$43x2`
    /// wherever it had reached. #3127.
    ///
    /// Honest boundary: deleting the `dma_active_mask` clear in `hdma_do_line` turns **only
    /// this test** red -- no ROM in the suite arms a channel for HDMA and general-purpose DMA
    /// at once. So this pins the reference's rule, not an observed ROM behaviour, and nobody
    /// should read a green suite as evidence that hardware was consulted here.
    #[test]
    fn an_hdma_enabled_channel_aborts_its_own_running_general_purpose_transfer() {
        let mut dma = DmaController::new();
        let mut bus = RecordingBus::new(0);

        // Channel 0 is both the HDMA channel and the general-purpose channel. Point A1T at
        // the HDMA table for the init (which copies it into $4308/9), then repoint it at the
        // general-purpose source.
        write_hdma_channel(&mut dma, 0, 0x00, 0x18, 0x3100);
        bus.a_bus[0x3100] = 0x83;
        bus.a_bus[0x3101] = 0xB1;
        bus.a_bus[0x3102] = 0x82;
        let init_clock = bus.clock;
        dma.hdma_init(0x01, &mut bus, 0, init_clock, 8, true);

        dma.write_register(0x4302, 0x00);
        dma.write_register(0x4303, 0x30);
        dma.write_register(0x4305, 0x04);
        dma.write_register(0x4306, 0x00);
        for (i, byte) in [0xA0u8, 0xA1, 0xA2, 0xA3].into_iter().enumerate() {
            bus.a_bus[0x3000 + i] = byte;
        }

        bus.schedule_hdma(3, HDMA_KIND_LINE, 0x01);
        bus.b_bus_writes.clear();
        let start = bus.clock;
        let (ticks, _) = dma.start_dma(0x01, &mut bus, 0, start, 6);

        assert_eq!(
            bus.b_bus_writes,
            vec![(start + 32, 0x18, 0xA0), (start + 48, 0x18, 0xB1)],
            "only the byte already in flight transfers; the rest of the channel is dropped"
        );
        assert_eq!(
            (
                dma.read_register(0x4305).unwrap(),
                dma.read_register(0x4306).unwrap()
            ),
            (0x03, 0x00),
            "$43x5 is left non-zero by the abort"
        );
        assert_eq!(
            (
                dma.read_register(0x4302).unwrap(),
                dma.read_register(0x4303).unwrap()
            ),
            (0x01, 0x30),
            "$43x2 is left where the aborted transfer had reached"
        );
        // align = pad_start 8 + overhead 8 + channel 8 + 1 byte (8) + nested 24 = 56,
        // pad = 6 - (56 % 6) = 4.
        assert_eq!(ticks, 60, "the aborted bytes cost no time");
        assert_eq!(bus.clock - start, 60, "bus advanced in lockstep");
    }

    /// #3061: a B->A transfer must read the B-bus itself, at the same point in
    /// the 8-clock byte slot an A->B transfer reads the A-bus (Mesen2
    /// `CopyDmaByte`: `ReadDma` after 4 clocks, `WriteDma` 4 clocks later;
    /// ares `Channel::readB` is `step(4); read; step(4)`).
    ///
    /// Nothing is ever written A->B here, so a controller serving B->A reads
    /// from an internal write-through shadow of past A->B bytes copies zeros.
    #[test]
    fn gpdma_b_to_a_reads_the_live_b_bus_at_the_slot_read_clock() {
        let mut dma = DmaController::new();
        let mut bus = RecordingBus::new(0);
        bus.b_bus_ports[0x39] = 0x11;
        bus.b_bus_ports[0x3A] = 0x22;
        // ch0: DMAP $81 = B->A, increment, mode 1 (two registers). 4 bytes from
        // B-bus $39/$3A into A-bus $0600.
        dma.write_register(0x4300, 0x81);
        dma.write_register(0x4301, 0x39);
        dma.write_register(0x4302, 0x00);
        dma.write_register(0x4303, 0x06);
        dma.write_register(0x4304, 0x00);
        dma.write_register(0x4305, 0x04);
        dma.write_register(0x4306, 0x00);

        dma.start_dma(0x01, &mut bus, 0, 0, 8);

        assert_eq!(
            &bus.a_bus[0x0600..0x0604],
            &[0x11, 0x22, 0x11, 0x22],
            "the live B-bus ports drive the transfer"
        );
        assert_eq!(
            bus.b_bus_reads,
            // pad_start 8 + overhead 8 + channel 8 = 24 before the first slot;
            // each 8-clock slot reads 4 clocks in.
            vec![(28, 0x39), (36, 0x3A), (44, 0x39), (52, 0x3A)],
            "mode 1 alternates the two ports, each read 4 clocks into its slot"
        );
        assert!(
            bus.b_bus_writes.is_empty(),
            "a B->A transfer drives no B-bus writes"
        );
    }

    // --- WRAM <-> $2180 is not a transfer hardware performs (#3111) ----------
    // fullsnes "DMA Notes": "WRAM-to-WRAM DMA isn't possible (neither in A-Bus
    // to B-Bus direction, nor vice-versa)." byuu's test_dmavalid ROM pins the
    // observable consequences: no data moves, WMADD does not advance, but the
    // channel's own bookkeeping and the transfer's cost are unaffected.

    /// Arms ch0 for a GPDMA and returns the channel's charged clocks.
    fn run_gpdma(
        bus: &mut RecordingBus,
        dmap: u8,
        bbad: u8,
        a_addr: u32,
        count: u16,
    ) -> (DmaController, u64) {
        let mut dma = DmaController::new();
        dma.write_register(0x4300, dmap);
        dma.write_register(0x4301, bbad);
        dma.write_register(0x4302, (a_addr & 0xFF) as u8);
        dma.write_register(0x4303, ((a_addr >> 8) & 0xFF) as u8);
        dma.write_register(0x4304, ((a_addr >> 16) & 0xFF) as u8);
        dma.write_register(0x4305, (count & 0xFF) as u8);
        dma.write_register(0x4306, (count >> 8) as u8);
        let (charged, _) = dma.start_dma(0x01, bus, 0, 0, 8);
        (dma, charged)
    }

    #[test]
    fn gpdma_wram_to_wmdata_transfers_nothing_but_pays_the_full_slot() {
        let mut bus = RecordingBus::new(0);
        // DMAP $00: A->B, increment, mode 0. A-bus $7E1000 -> B-bus $80.
        let (dma, charged) = run_gpdma(&mut bus, 0x00, 0x80, 0x7E_1000, 4);

        assert!(
            bus.b_bus_writes.is_empty(),
            "WRAM -> $2180 must not write the B-bus at all"
        );
        // pad_start 8 + overhead 8 + channel 8 + 4 slots x 8 = 56, then the
        // fixed 8-clock end pad -> 64. Identical to a valid transfer.
        assert_eq!(charged, 64, "the refused slots still cost their 8 clocks");
        assert_eq!(bus.clock, 64, "and the bus really advanced by them");
        assert_eq!(
            (dma.read_register(0x4302), dma.read_register(0x4303)),
            (Some(0x04), Some(0x10)),
            "$43x2 still advances over the refused slots"
        );
        assert_eq!(
            (dma.read_register(0x4305), dma.read_register(0x4306)),
            (Some(0x00), Some(0x00)),
            "$43x5 still decrements to zero"
        );
    }

    #[test]
    fn gpdma_wmdata_to_wram_writes_the_invalid_byte_without_reading_the_b_bus() {
        let mut bus = RecordingBus::new(0);
        bus.b_bus_ports[0x80] = 0x5A;
        bus.a_bus[0x1000] = 0x11;
        bus.a_bus[0x1001] = 0x11;
        // DMAP $80: B->A, increment, mode 0. B-bus $80 -> A-bus $7E1000.
        let (_dma, charged) = run_gpdma(&mut bus, 0x80, 0x80, 0x7E_1000, 2);

        assert!(
            bus.b_bus_reads.is_empty(),
            "$2180 must not be read, so WMADD cannot advance"
        );
        assert_eq!(
            &bus.a_bus[0x1000..0x1002],
            &[INVALID_WRAM_TRANSFER_BYTE, INVALID_WRAM_TRANSFER_BYTE],
            "the A-bus write still happens, with the invalid byte -- not the \
             port's value (0x5A) and not the untouched seed (0x11)"
        );
        assert_eq!(charged, 48, "clocks match a valid 2-byte transfer");
    }

    #[test]
    fn a_non_wram_a_bus_address_still_reaches_wmdata() {
        // $00:3000 is above the $0000-$1FFF WRAM mirror (it is the SA-1 I-RAM
        // mirror on an SA-1 cart), and $40:0000 is outside the mirrored banks.
        // Neither is WRAM, so both must transfer normally. Guards against an
        // over-broad predicate such as "any low bank" or "offset < $8000".
        for a_addr in [0x00_3000u32, 0x40_0000] {
            let mut bus = RecordingBus::new(0);
            bus.a_bus[(a_addr & 0xFFFF) as usize] = 0x77;
            run_gpdma(&mut bus, 0x00, 0x80, a_addr, 1);
            assert_eq!(
                bus.b_bus_writes,
                vec![(32, 0x80, 0x77)],
                "{a_addr:#08X} is not WRAM, so $2180 must still be written"
            );
        }
    }

    #[test]
    fn a_wram_a_bus_address_is_legal_for_every_other_b_bus_port() {
        let mut bus = RecordingBus::new(0);
        bus.a_bus[0x1000] = 0xAA;
        bus.a_bus[0x1001] = 0xBB;
        // Same WRAM source, but B-bus $18 (VMDATAL) instead of $80.
        run_gpdma(&mut bus, 0x00, 0x18, 0x7E_1000, 2);
        assert_eq!(
            bus.b_bus_writes,
            vec![(32, 0x18, 0xAA), (40, 0x18, 0xBB)],
            "the restriction is specific to WMDATA; WRAM -> VRAM is the most \
             common DMA there is"
        );
    }

    #[test]
    fn the_wram_restriction_applies_per_slot_not_per_channel_bbad() {
        let mut bus = RecordingBus::new(0);
        for i in 0..4 {
            bus.a_bus[0x1000 + i] = 0xC0 + i as u8;
        }
        // Mode 1 with BBAD $7F alternates B-bus $7F and $80: only the $80 slots
        // are refused. Guarding on the channel's raw BBAD would drop all four.
        run_gpdma(&mut bus, 0x01, 0x7F, 0x7E_1000, 4);
        assert_eq!(
            bus.b_bus_writes,
            vec![(32, 0x7F, 0xC0), (48, 0x7F, 0xC2)],
            "the two $7F slots transfer, the two $80 slots are refused"
        );
        assert_eq!(bus.clock, 64, "all four slots still cost 8 clocks each");
    }

    #[test]
    fn a_refused_slot_leaves_the_dma_open_bus_unchanged() {
        let mut dma = DmaController::new();
        let mut bus = RecordingBus::new(0);
        bus.a_bus[0x3000] = 0x5A;
        bus.b_bus_ports[0x80] = 0x37;
        // ch0: valid A->B, $003000 -> B-bus $22, one byte (drives open bus 0x5A).
        dma.write_register(0x4300, 0x00);
        dma.write_register(0x4301, 0x22);
        dma.write_register(0x4302, 0x00);
        dma.write_register(0x4303, 0x30);
        dma.write_register(0x4304, 0x00);
        dma.write_register(0x4305, 0x01);
        // ch1: refused WRAM -> $2180.
        dma.write_register(0x4310, 0x00);
        dma.write_register(0x4311, 0x80);
        dma.write_register(0x4312, 0x00);
        dma.write_register(0x4313, 0x10);
        dma.write_register(0x4314, 0x7E);
        dma.write_register(0x4315, 0x01);

        let (_charged, open_bus) = dma.start_dma(0x03, &mut bus, 0, 0, 8);

        // byuu's test_dmavalid notes the byte a refused B->A slot deposits is
        // NOT MDR; a refused slot performs no read, so it drives nothing.
        assert_eq!(
            open_bus, 0x5A,
            "the refused channel must not publish 0xFF (or anything else) as MDR"
        );
    }

    #[test]
    fn hdma_wram_table_to_wmdata_transfers_nothing_but_still_walks_the_table() {
        let mut dma = DmaController::new();
        let mut bus = RecordingBus::new(0);
        // Mode 2 (two bytes to one register), direct, table in WRAM at $7E3000.
        write_hdma_channel(&mut dma, 0, 0x02, 0x80, 0x3000);
        dma.write_register(0x4304, 0x7E);
        bus.a_bus[0x3000] = 0x01; // one line
        bus.a_bus[0x3001] = 0xA1;
        bus.a_bus[0x3002] = 0xA2;
        dma.hdma_init(0x01, &mut bus, 0, 0, 8, true);
        bus.b_bus_writes.clear();

        dma.hdma_do_line(0x01, &mut bus, 0, 0, 8, true);

        assert!(
            bus.b_bus_writes.is_empty(),
            "an HDMA slot into $2180 from a WRAM table is refused too"
        );
        // The table fetches themselves are controller-internal A-bus reads, NOT
        // transfer bytes, so they must stay unguarded: the line counter loaded
        // and the table pointer advanced past the two data bytes.
        assert_eq!(
            dma.read_register(0x430A),
            Some(0x00),
            "the line counter loaded from the WRAM table, expired, and took the \
             next descriptor ($3003 = 0x00, the terminator)"
        );
        assert_eq!(
            (dma.read_register(0x4308), dma.read_register(0x4309)),
            (Some(0x04), Some(0x30)),
            "the table pointer walked past the descriptor, both data bytes and \
             the expiry's descriptor fetch -- so the table reads were NOT \
             refused along with the transfer"
        );
    }

    #[test]
    fn hdma_b_to_a_through_wmdata_writes_the_invalid_byte_into_the_wram_table() {
        let mut dma = DmaController::new();
        let mut bus = RecordingBus::new(0);
        // DMAP $82: B->A, mode 2, direct table in WRAM at $7E3000.
        write_hdma_channel(&mut dma, 0, 0x82, 0x80, 0x3000);
        dma.write_register(0x4304, 0x7E);
        bus.a_bus[0x3000] = 0x01; // one line
        bus.b_bus_ports[0x80] = 0x5A;
        dma.hdma_init(0x01, &mut bus, 0, 0, 8, true);

        dma.hdma_do_line(0x01, &mut bus, 0, 0, 8, true);

        assert!(bus.b_bus_reads.is_empty(), "$2180 is never read");
        assert_eq!(
            &bus.a_bus[0x3001..0x3003],
            &[INVALID_WRAM_TRANSFER_BYTE, INVALID_WRAM_TRANSFER_BYTE],
            "the A-bus write still lands, with the invalid byte rather than 0x5A"
        );
    }

    #[test]
    fn hdma_do_line_speculative_descriptor_read_does_not_advance_the_table() {
        // The next table byte is read EVERY line (8 clocks) but the pointer
        // only advances when the line counter expires (Mesen2 "read the next
        // byte from Address into $43xA ... value discarded if the line counter
        // isn't 0").
        let mut dma = DmaController::new();
        let mut bus = RecordingBus::new(0);
        write_hdma_channel(&mut dma, 0, 0x00, 0x22, 0x3000);
        bus.a_bus[0x3000] = 0x83; // repeat, 3 lines
        bus.a_bus[0x3001] = 0x11;

        dma.hdma_init(0x01, &mut bus, 0, 0, 8, true);
        let table_before = u16::from_le_bytes([dma.get_reg(0, 0x8), dma.get_reg(0, 0x9)]);

        let base = bus.clock;
        dma.hdma_do_line(0x01, &mut bus, 0, base, 8, true);

        let table_after = u16::from_le_bytes([dma.get_reg(0, 0x8), dma.get_reg(0, 0x9)]);
        assert_eq!(
            table_after,
            table_before + 1,
            "only the transferred byte advanced the pointer"
        );
        assert_eq!(
            dma.get_reg(0, 0xA),
            0x82,
            "counter decremented, not reloaded"
        );
    }

    // $43xA IS the line counter -- there is no separate internal copy. A ROM
    // that arms HDMA mid-frame writes the counter and the table pointer by hand
    // and enables $420C after the frame's HDMA init has already run, so the
    // per-line decrement must start from what the CPU wrote (Mesen2 keeps only
    // `HdmaLineCounterAndRepeat`, decrements it in place, and `$430A` writes
    // land straight on it).
    //
    // This is byuu `test_hdmatiming.smc`'s sub-test 1 as a unit test: counter
    // 2, no transfer, no line load -- the ROM reads back $430A == $01 (#3062).
    #[test]
    fn hdma_line_counter_decrements_the_value_the_cpu_wrote_to_43xa() {
        let mut dma = DmaController::new();
        let mut bus = RecordingBus::new(0);
        write_hdma_channel(&mut dma, 0, 0x00, 0x22, 0x3000);
        bus.a_bus[0x3000] = 0xAA;

        // Frame init runs with HDMA disabled, exactly as the ROM arranges.
        dma.hdma_init(0x00, &mut bus, 0, 0, 8, true);

        // The CPU then writes the table pointer and counter by hand.
        dma.write_register(0x4308, 0x00);
        dma.write_register(0x4309, 0x30);
        dma.write_register(0x430A, 0x02);

        let base = bus.clock;
        dma.hdma_do_line(0x01, &mut bus, 0, base, 8, true);

        assert_eq!(
            dma.get_reg(0, 0xA),
            0x01,
            "counter must decrement the CPU-written $430A, not a stale internal copy"
        );
    }

    // byuu `test_hdmatiming.smc` sub-test 2: a CPU-written counter of 1 expires
    // on the first line, so the descriptor is consumed into $43xA and the table
    // pointer advances.
    #[test]
    fn hdma_cpu_written_counter_of_one_consumes_the_descriptor() {
        let mut dma = DmaController::new();
        let mut bus = RecordingBus::new(0);
        write_hdma_channel(&mut dma, 0, 0x00, 0x22, 0x3000);
        bus.a_bus[0x3000] = 0xAA;

        dma.hdma_init(0x00, &mut bus, 0, 0, 8, true);

        dma.write_register(0x4308, 0x00);
        dma.write_register(0x4309, 0x30);
        dma.write_register(0x430A, 0x01);

        let base = bus.clock;
        dma.hdma_do_line(0x01, &mut bus, 0, base, 8, true);

        assert_eq!(
            dma.get_reg(0, 0xA),
            0xAA,
            "an expiring CPU-written counter must load the next descriptor"
        );
        assert_eq!(
            u16::from_le_bytes([dma.get_reg(0, 0x8), dma.get_reg(0, 0x9)]),
            0x3001,
            "consuming the descriptor advances the table pointer"
        );
    }

    // The expiry test is on the low 7 bits, not the whole byte: a repeat-mode
    // counter reaches $80 (repeat set, zero lines left) and must consume there
    // (Mesen2 `if((ch.HdmaLineCounterAndRepeat & 0x7F) == 0)`).
    #[test]
    fn hdma_repeat_counter_expires_on_the_low_seven_bits() {
        let mut dma = DmaController::new();
        let mut bus = RecordingBus::new(0);
        write_hdma_channel(&mut dma, 0, 0x00, 0x22, 0x3000);
        bus.a_bus[0x3000] = 0x05;

        dma.hdma_init(0x00, &mut bus, 0, 0, 8, true);

        dma.write_register(0x4308, 0x00);
        dma.write_register(0x4309, 0x30);
        dma.write_register(0x430A, 0x81); // repeat, one line left

        let base = bus.clock;
        dma.hdma_do_line(0x01, &mut bus, 0, base, 8, true);

        assert_eq!(
            dma.get_reg(0, 0xA),
            0x05,
            "$81 decrements to $80, whose low 7 bits are zero, so it expires"
        );
    }

    // byuu `test_hdma.smc` sub-test 4: "if $43xa is 0 when an HDMA transfer
    // begins (before the decrement), it will wrap to 0xff and begin a
    // continuous transfer". The ROM arms a mode-3 channel mid-frame with
    // $430A = 0 and its table pointer one byte past the leading terminator,
    // then lets three lines run and checks that CGRAM ends up holding the
    // SECOND data pair -- i.e. the first line transfers nothing, and the two
    // after it each move one 4-byte group (#3062).
    #[test]
    fn hdma_zero_counter_wraps_into_a_continuous_transfer() {
        let mut dma = DmaController::new();
        let mut bus = RecordingBus::new(0);
        // Mode 3 (p, p, p+1, p+1) to $2121/$2122, direct addressing.
        write_hdma_channel(&mut dma, 0, 0x03, 0x21, 0x3000);
        // Table: a leading $00 the ROM deliberately skips, then two 4-byte groups.
        bus.a_bus[0x3000] = 0x00;
        for (offset, byte) in [0x00, 0x00, 0x78, 0x56, 0x00, 0x00, 0xBC, 0x9A]
            .into_iter()
            .enumerate()
        {
            bus.a_bus[0x3001 + offset] = byte;
        }

        // Frame init runs with HDMA disabled; the CPU then points the table one
        // byte in and zeroes the counter before enabling $420C.
        dma.hdma_init(0x00, &mut bus, 0, 0, 8, true);
        dma.write_register(0x4308, 0x01);
        dma.write_register(0x4309, 0x30);
        dma.write_register(0x430A, 0x00);

        for _ in 0..3 {
            let base = bus.clock;
            dma.hdma_do_line(0x01, &mut bus, 0, base, 8, true);
        }

        let written: Vec<(u8, u8)> = bus
            .b_bus_writes
            .iter()
            .map(|&(_, port, value)| (port, value))
            .collect();
        assert_eq!(
            written,
            vec![
                // Line 1 transfers nothing: DoTransfer is only set by the
                // decrement that wraps $00 to $FF.
                (0x21, 0x00),
                (0x21, 0x00),
                (0x22, 0x78),
                (0x22, 0x56),
                (0x21, 0x00),
                (0x21, 0x00),
                (0x22, 0xBC),
                (0x22, 0x9A),
            ],
            "two continuous-mode lines move one 4-byte group each"
        );
    }

    #[test]
    fn hdma_do_line_expiry_charges_indirect_pointer_load() {
        // A one-line indirect entry followed by a real next entry: on expiry
        // the burst charges the 8-clock descriptor consume plus 16 clocks for
        // the two indirect pointer bytes (Mesen2 "if a new indirect address is
        // required, 16 master cycles are taken to load it").
        let mut dma = DmaController::new();
        let mut bus = RecordingBus::new(0);
        write_hdma_channel(&mut dma, 0, 0x40, 0x22, 0x3000);
        dma.write_register(0x4307, 0x00); // indirect bank
        bus.a_bus[0x3000] = 0x01; // 1 line, no repeat
        bus.a_bus[0x3001] = 0x00; // indirect ptr -> $4000
        bus.a_bus[0x3002] = 0x40;
        bus.a_bus[0x4000] = 0x77;
        bus.a_bus[0x3003] = 0x02; // next entry: 2 lines
        bus.a_bus[0x3004] = 0x10; // its indirect ptr -> $4010
        bus.a_bus[0x3005] = 0x40;

        dma.hdma_init(0x01, &mut bus, 0, 0, 8, true);
        let base = bus.clock;
        let (counter, _) = dma.hdma_do_line(0x01, &mut bus, 0, base, 8, true);

        // pad 8 + overhead 8 + 1 byte slot 8 + descriptor consume 8 +
        // indirect pointer load 16 + pad_end 8.
        assert_eq!(counter, 56, "expiry charges the indirect pointer load");
        assert_eq!(
            u16::from_le_bytes([dma.get_reg(0, 0x5), dma.get_reg(0, 0x6)]),
            0x4010,
            "the new indirect pointer is loaded"
        );
    }

    #[test]
    fn hdma_do_line_terminator_on_last_indirect_channel_loads_single_msb_byte() {
        // Mesen2's one-byte oddity: a $00 descriptor on the LAST active
        // indirect channel loads only ONE pointer byte (8 clocks, not 16) and
        // uses it as the HIGH byte with a zero low byte.
        let mut dma = DmaController::new();
        let mut bus = RecordingBus::new(0);
        write_hdma_channel(&mut dma, 0, 0x40, 0x22, 0x3000);
        dma.write_register(0x4307, 0x00);
        bus.a_bus[0x3000] = 0x01; // 1 line
        bus.a_bus[0x3001] = 0x00; // indirect ptr -> $4000
        bus.a_bus[0x3002] = 0x40;
        bus.a_bus[0x4000] = 0x77;
        bus.a_bus[0x3003] = 0x00; // terminator
        bus.a_bus[0x3004] = 0x5D; // the single MSB byte

        dma.hdma_init(0x01, &mut bus, 0, 0, 8, true);
        let base = bus.clock;
        let (counter, _) = dma.hdma_do_line(0x01, &mut bus, 0, base, 8, true);

        // pad 8 + overhead 8 + 1 byte slot 8 + descriptor consume 8 +
        // one-byte pointer load 8 + pad_end 8.
        assert_eq!(counter, 48, "the terminator load is a single 8-clock slot");
        assert_eq!(dma.get_reg(0, 0x5), 0x00, "low byte forced to zero");
        assert_eq!(
            dma.get_reg(0, 0x6),
            0x5D,
            "loaded byte lands in the high byte"
        );
    }
}
