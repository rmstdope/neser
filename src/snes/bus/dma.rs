use crate::snes::console::save_state::SnesDmaState;

const DMA_REG_BYTES: usize = 0x80;
const B_BUS_PORT_BYTES: usize = 0x100;

pub trait DmaABus {
    fn dma_read_a_bus(&mut self, addr: u32, open_bus: u8) -> u8;
    fn dma_write_a_bus(&mut self, addr: u32, value: u8);
    fn dma_write_b_bus(&mut self, addr: u8, value: u8);
    /// Advance the rest of the system (PPU/APU/input) by `master_clocks` while the DMA
    /// controller owns the bus. Real hardware keeps rendering during a general-purpose DMA
    /// (Mesen2 `SnesDmaController` advances the master clock per transferred byte), so B-bus
    /// writes must land at the scan position they really occur at. Default no-op for test
    /// doubles that don't model time.
    fn dma_tick(&mut self, _master_clocks: u64) {}
}

#[derive(Clone)]
pub struct DmaController {
    regs: [u8; DMA_REG_BYTES],
    bbus_ports: [u8; B_BUS_PORT_BYTES],
    hdma_active_mask: u8,
    hdma_do_transfer: [bool; 8],
    hdma_repeat_mode: [bool; 8],
    hdma_lines_left: [u16; 8],
}

impl DmaController {
    pub fn new() -> Self {
        Self {
            regs: [0; DMA_REG_BYTES],
            bbus_ports: [0; B_BUS_PORT_BYTES],
            hdma_active_mask: 0,
            hdma_do_transfer: [false; 8],
            hdma_repeat_mode: [false; 8],
            hdma_lines_left: [0; 8],
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
            bbus_ports: self.bbus_ports.to_vec(),
            hdma_active_mask: self.hdma_active_mask,
            hdma_do_transfer: self.hdma_do_transfer.to_vec(),
            hdma_repeat_mode: self.hdma_repeat_mode.to_vec(),
            hdma_lines_left: self.hdma_lines_left.to_vec(),
        }
    }

    pub(crate) fn restore_state(&mut self, state: &SnesDmaState) -> Result<(), String> {
        if state.regs.len() != DMA_REG_BYTES {
            return Err(format!(
                "DMA register state size mismatch (expected {DMA_REG_BYTES}, found {})",
                state.regs.len()
            ));
        }
        if state.bbus_ports.len() != B_BUS_PORT_BYTES {
            return Err(format!(
                "DMA B-bus state size mismatch (expected {B_BUS_PORT_BYTES}, found {})",
                state.bbus_ports.len()
            ));
        }
        if state.hdma_do_transfer.len() != 8
            || state.hdma_repeat_mode.len() != 8
            || state.hdma_lines_left.len() != 8
        {
            return Err("DMA HDMA state size mismatch".to_string());
        }

        self.regs.copy_from_slice(&state.regs);
        self.bbus_ports.copy_from_slice(&state.bbus_ports);
        self.hdma_active_mask = state.hdma_active_mask;
        self.hdma_do_transfer
            .copy_from_slice(&state.hdma_do_transfer);
        self.hdma_repeat_mode
            .copy_from_slice(&state.hdma_repeat_mode);
        self.hdma_lines_left.copy_from_slice(&state.hdma_lines_left);
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
        // Deliberately unused: general-purpose DMA rounds to a fixed 8 (see the SyncEndDma
        // comment below). Kept so all three envelopes share a signature.
        _cpu_speed: u8,
    ) -> (u64, u8) {
        if mdmaen == 0 {
            return (0, seed_open_bus);
        }

        // Hardware start envelope (Mesen2 SyncStartDma): the caller invokes this
        // one full CPU cycle after the $420B write (the start-delay cycle runs
        // normally); the transfer then pauses 1-8 clocks to reach a whole
        // multiple of 8 master clocks since reset, then 8 clocks of setup
        // overhead.
        let pad_start = 8 - (base_clock & 7);
        abus.dma_tick(pad_start);
        let mut counter = pad_start;
        counter += 8;
        abus.dma_tick(8);
        let mut open_bus = seed_open_bus;

        for channel in 0u8..8 {
            if (mdmaen & (1 << channel)) == 0 {
                continue;
            }

            counter += 8;
            abus.dma_tick(8);
            counter += self.run_channel(channel, abus, &mut open_bus);
        }

        // SyncEndDma: general-purpose DMA rounds to a FIXED 8-clock cycle, unlike the two
        // HDMA envelopes and unlike Mesen2's single speed-aware `SyncEndDma`.
        //
        // Kept deliberately (#3067, re-verified after #3074). Flipping this to `cpu_speed`
        // makes `mosaic_mode5_sized` diverge from its Mesen2-approved golden, with every other
        // vector unchanged; the fixed 8 passes all of them. Reproduce by changing the literal
        // below and running:
        //
        //     cargo test --no-default-features --lib mosaic_mode5_sized
        //
        // The witness has already moved once -- before #3070 it was
        // `inidisp_forgot_to_force_blank` -- and #3074 changed which CPU cycle a transfer runs
        // in, so it can move again. Re-derive which vector objects rather than trusting this
        // paragraph; only the conclusion (keep the 8) has survived every re-measurement.
        //
        // Why the rule that is right for HDMA is not obviously right here: Mesen2 re-enters
        // `ProcessPendingTransfers` from inside `RunDma`, so an HDMA firing during a
        // general-purpose transfer runs NESTED -- it pays no sync pads of its own
        // (`needSync == false` while any channel is `DmaActive`) and folds its clocks into the
        // same `_dmaClockCounter` that the outer `SyncEndDma` rounds. NESER cannot nest
        // (`self.dma` is `mem::take`n for the duration), so `counter` is not necessarily the
        // quantity Mesen2 is rounding. Revisit together with that re-entrancy.
        //
        // The two HDMA envelopes DO use `cpu_speed`, which is what made StarWars and
        // hdmaen_latch_test pixel-exact in #3050.
        let pad_end = Self::sync_end_pad(counter, 8);
        abus.dma_tick(pad_end);
        counter += pad_end;

        (counter, open_bus)
    }

    pub fn hdma_init<B: DmaABus>(
        &mut self,
        hdmaen: u8,
        abus: &mut B,
        seed_open_bus: u8,
        base_clock: u64,
        cpu_speed: u8,
    ) -> (u64, u8) {
        // Initialize active_mask to 0xFF (all channels potentially active).
        // Channels will be cleared from active_mask when they terminate (descriptor=$00).
        // This allows mid-scanline HDMAEN writes to work - HDMAEN gates which channels
        // process, while active_mask only tracks termination (#2943).
        self.hdma_active_mask = 0xFF;
        self.hdma_do_transfer = [false; 8];
        self.hdma_repeat_mode = [false; 8];
        self.hdma_lines_left = [0; 8];

        if hdmaen == 0 {
            return (0, seed_open_bus);
        }

        // Hardware envelope (Mesen2 InitHdmaChannels): SyncStartDma pad + 8
        // clocks of setup overhead; SyncEndDma pad at the end.
        let pad_start = 8 - (base_clock & 7);
        abus.dma_tick(pad_start);
        let mut ticks = pad_start + 8;
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
            } else {
                let (repeat_mode, lines_left) = Self::decode_hdma_descriptor(descriptor);
                self.hdma_repeat_mode[channel as usize] = repeat_mode;
                self.hdma_lines_left[channel as usize] = lines_left;
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

        // SyncEndDma: round the charged total up to a whole CPU cycle (see `sync_end_pad`).
        let pad_end = Self::sync_end_pad(ticks, cpu_speed);
        abus.dma_tick(pad_end);
        ticks += pad_end;

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
    ) -> (u64, u8) {
        if hdmaen == 0 {
            return (0, seed_open_bus);
        }

        // Hardware envelope (Mesen2 ProcessHdmaChannels): SyncStartDma pads
        // 1-8 clocks to a multiple of 8 master clocks since reset, then 8
        // clocks of setup overhead.
        let pad_start = 8 - (base_clock & 7);
        abus.dma_tick(pad_start);
        let mut counter = pad_start + 8;
        abus.dma_tick(8);
        let mut open_bus = seed_open_bus;

        // Phase A: run every active channel's transfer for this line first.
        for channel in 0u8..8 {
            if (hdmaen & (1 << channel)) == 0 || (self.hdma_active_mask & (1 << channel)) == 0 {
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
            // Use wrapping_sub to match hardware (0 - 1 = 0xFF, not 0)
            self.hdma_lines_left[idx] = self.hdma_lines_left[idx].wrapping_sub(1);

            let line_counter =
                Self::encode_hdma_counter(self.hdma_repeat_mode[idx], self.hdma_lines_left[idx]);
            self.set_reg(channel, 0xA, line_counter);

            // Set do_transfer based on repeat bit (bit 7) of line counter register,
            // not internal repeat_mode. This allows mid-scanline activation to work (#2943).
            self.hdma_do_transfer[idx] = (line_counter & 0x80) != 0;

            // Speculative descriptor read (pointer NOT advanced unless consumed).
            abus.dma_tick(4);
            let descriptor = abus.dma_read_a_bus(self.hdma_table_address(channel), open_bus);
            open_bus = descriptor;
            abus.dma_tick(4);
            counter += 8;

            if self.hdma_lines_left[idx] == 0 {
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
                    self.hdma_repeat_mode[idx] = false;
                    self.hdma_lines_left[idx] = 0;
                } else {
                    let (repeat_mode, lines_left) = Self::decode_hdma_descriptor(descriptor);
                    self.hdma_repeat_mode[idx] = repeat_mode;
                    self.hdma_lines_left[idx] = lines_left;
                    self.hdma_do_transfer[idx] = true;
                }
            }
        }

        // SyncEndDma: pad clocks rounding the charged total to a whole CPU cycle
        // (see `sync_end_pad`).
        let pad_end = Self::sync_end_pad(counter, cpu_speed);
        abus.dma_tick(pad_end);
        counter += pad_end;

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

    fn run_channel<B: DmaABus>(&mut self, channel: u8, abus: &mut B, open_bus: &mut u8) -> u64 {
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
        let mut ticks = 0u64;

        for i in 0..transfer_bytes {
            let a_addr = ((bank as u32) << 16) | ((a_high as u32) << 8) | (a_low as u32);
            let b_addr = bbad.wrapping_add(pattern[i % pattern.len()]);

            // Each byte occupies an 8-master-clock bus slot: the read fills the
            // first 4 clocks, and the destination write lands at the END of the
            // slot (Mesen2 `CopyDmaByte`: ReadDma advances 4, then WriteDma
            // advances 4 more BEFORE calling the register handler).
            abus.dma_tick(4);
            if direction_b_to_a {
                let value = self.read_b_bus(b_addr);
                *open_bus = value;
                abus.dma_tick(4);
                abus.dma_write_a_bus(a_addr, value);
            } else {
                let value = abus.dma_read_a_bus(a_addr, *open_bus);
                *open_bus = value;
                self.write_b_bus(b_addr, value);
                abus.dma_tick(4);
                abus.dma_write_b_bus(b_addr, value);
            }

            ticks += 8;
            count = count.wrapping_sub(1);
            (a_low, a_high) = Self::advance_a_address(a_low, a_high, step);
        }

        self.set_reg(channel, 0x2, a_low);
        self.set_reg(channel, 0x3, a_high);
        self.set_reg(channel, 0x5, (count & 0x00FF) as u8);
        self.set_reg(channel, 0x6, (count >> 8) as u8);
        ticks
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

    fn read_b_bus(&self, addr: u8) -> u8 {
        self.bbus_ports[addr as usize]
    }

    fn write_b_bus(&mut self, addr: u8, value: u8) {
        self.bbus_ports[addr as usize] = value;
    }

    fn get_reg(&self, channel: u8, reg: usize) -> u8 {
        self.regs[(channel as usize) * 0x10 + reg]
    }

    fn set_reg(&mut self, channel: u8, reg: usize, value: u8) {
        self.regs[(channel as usize) * 0x10 + reg] = value;
    }

    fn decode_hdma_descriptor(descriptor: u8) -> (bool, u16) {
        if descriptor == 0 {
            return (false, 0);
        }
        if descriptor <= 0x80 {
            if descriptor == 0x80 {
                (false, 128)
            } else {
                (false, descriptor as u16)
            }
        } else {
            (true, (descriptor - 0x80) as u16)
        }
    }

    fn encode_hdma_counter(repeat_mode: bool, lines_left: u16) -> u8 {
        if lines_left == 0 {
            0
        } else if repeat_mode {
            0x80 | (lines_left as u8)
        } else if lines_left == 128 {
            0x80
        } else {
            lines_left as u8
        }
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
            // 8-clock byte slot with the destination write at the slot's end
            // (Mesen2 CopyDmaByte), landing at its true bus clock.
            abus.dma_tick(4);
            if direction_b_to_a {
                let value = self.read_b_bus(b_addr);
                *open_bus = value;
                abus.dma_tick(4);
                abus.dma_write_a_bus(a_addr, value);
            } else {
                let value = abus.dma_read_a_bus(a_addr, *open_bus);
                *open_bus = value;
                self.write_b_bus(b_addr, value);
                abus.dma_tick(4);
                abus.dma_write_b_bus(b_addr, value);
            }
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
    struct RecordingBus {
        clock: u64,
        a_bus: Vec<u8>,
        b_bus_writes: Vec<(u64, u8, u8)>,
    }

    impl RecordingBus {
        fn new(clock: u64) -> Self {
            Self {
                clock,
                a_bus: vec![0; 0x1_0000],
                b_bus_writes: Vec::new(),
            }
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

        fn dma_tick(&mut self, master_clocks: u64) {
            self.clock += master_clocks;
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
        dma.hdma_init(0x03, &mut bus, 0, init_clock, 8);
        let base_clock = bus.clock;
        assert_eq!(base_clock % 8, 0, "init ends on a CPU cycle boundary");

        let start = bus.clock;
        let (counter, _) = dma.hdma_do_line(0x03, &mut bus, 0, base_clock, 8);

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
            dma.hdma_init(0x01, &mut bus, 0, init_clock, 8);
            let base_clock = bus.clock;
            let (counter, _) = dma.hdma_do_line(0x01, &mut bus, 0, base_clock, cpu_speed);
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
            dma.hdma_init(0x01, &mut bus, 0, 0, cpu_speed).0
        };
        assert_eq!(charged(6) % 6, 0, "init ends on a whole 6-clock CPU cycle");
        assert_eq!(charged(8) % 8, 0, "init ends on a whole 8-clock CPU cycle");
        assert_ne!(charged(6), charged(8), "the speed must change the end pad");
    }

    /// General-purpose DMA keeps a FIXED 8-clock end pad while the two HDMA envelopes round
    /// to the upcoming access's speed. #3067 settled that on measurement -- see the
    /// `SyncEndDma` comment in `start_dma` for the evidence and the re-entrancy argument. This test exists so
    /// the asymmetry is a recorded decision, and so that a future attempt to "fix" it has to
    /// confront the evidence rather than just Mesen2's source.
    #[test]
    fn general_purpose_dma_end_pad_stays_on_a_fixed_eight_clock_cycle() {
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
        // pad_start 8 + overhead 8 + channel 8 + 4 byte slots (32) = 56 charged before the pad,
        // so a fixed-8 pad gives 64 whatever the caller passes. A speed-aware pad would give
        // 60 for both 6 and 12, which is what the two equalities below rule out.
        assert_eq!(charged(8), 64, "ends on a whole 8-clock CPU cycle");
        assert_eq!(
            charged(6),
            64,
            "the CPU speed does NOT change the GPDMA pad"
        );
        assert_eq!(charged(12), 64);
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

        dma.hdma_init(0x01, &mut bus, 0, 0, 8);
        let table_before = u16::from_le_bytes([dma.get_reg(0, 0x8), dma.get_reg(0, 0x9)]);

        let base = bus.clock;
        dma.hdma_do_line(0x01, &mut bus, 0, base, 8);

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

        dma.hdma_init(0x01, &mut bus, 0, 0, 8);
        let base = bus.clock;
        let (counter, _) = dma.hdma_do_line(0x01, &mut bus, 0, base, 8);

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

        dma.hdma_init(0x01, &mut bus, 0, 0, 8);
        let base = bus.clock;
        let (counter, _) = dma.hdma_do_line(0x01, &mut bus, 0, base, 8);

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
