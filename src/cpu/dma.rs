/// DMA Controller for cycle-accurate OAM DMA and DMC DMA handling.
///
/// This module implements the Mesen-style DMA state machines that allow:
/// - OAM DMA to be executed cycle-by-cycle
/// - DMC DMA to interrupt OAM DMA mid-transfer
/// - Proper priority handling (DMC has higher priority than OAM)

/// OAM DMA state machine states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OamDmaState {
    /// No OAM DMA in progress
    Idle,
    /// Waiting for CPU read cycle to start
    Pending { page: u8 },
    /// Alignment cycle (dummy read, odd-cycle alignment)
    Aligning { page: u8 },
    /// Reading byte from source page
    Reading { page: u8, offset: u8 },
    /// Writing byte to $2004
    Writing { page: u8, offset: u8, value: u8 },
}

/// DMC DMA state machine states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmcDmaState {
    /// No DMC DMA in progress
    Idle,
    /// Halt cycle - waiting on CPU read, halted_addr is the address CPU was trying to read
    Halt { halted_addr: u16 },
    /// Dummy cycle - repeat the halted read
    Dummy { halted_addr: u16 },
    /// Alignment cycle (if needed)
    Aligning { halted_addr: u16 },
    /// Get cycle - read the sample byte
    Reading,
}

/// DMA Controller manages both OAM DMA and DMC DMA state
#[derive(Debug, Clone)]
pub struct DmaController {
    /// OAM DMA state machine
    pub oam_state: OamDmaState,
    /// DMC DMA state machine
    pub dmc_state: DmcDmaState,
    /// Total cycles consumed by current DMA operation(s)
    pub cycles_consumed: u16,
    /// Whether an OAM DMA alignment cycle is needed
    oam_needs_align: bool,
    /// Tracks if we're on a "get" cycle (even total cycles) for DMC DMA alignment
    is_get_cycle: bool,
}

impl Default for DmaController {
    fn default() -> Self {
        Self::new()
    }
}

impl DmaController {
    pub fn new() -> Self {
        Self {
            oam_state: OamDmaState::Idle,
            dmc_state: DmcDmaState::Idle,
            cycles_consumed: 0,
            oam_needs_align: false,
            is_get_cycle: true,
        }
    }

    /// Check if any DMA is currently active (OAM or DMC)
    pub fn is_active(&self) -> bool {
        self.oam_state != OamDmaState::Idle || self.dmc_state != DmcDmaState::Idle
    }

    /// Check if OAM DMA is active (pending or in progress)
    pub fn oam_dma_active(&self) -> bool {
        self.oam_state != OamDmaState::Idle
    }

    /// Check if DMC DMA is active
    pub fn dmc_dma_active(&self) -> bool {
        self.dmc_state != DmcDmaState::Idle
    }

    /// Start an OAM DMA transfer from the given page
    /// Called when CPU writes to $4014
    pub fn start_oam_dma(&mut self, page: u8, is_odd_cycle: bool) {
        self.oam_state = OamDmaState::Pending { page };
        self.oam_needs_align = is_odd_cycle;
        self.cycles_consumed = 0;
    }

    /// Start a DMC DMA request
    /// Called when DMC needs a sample byte
    pub fn start_dmc_dma(&mut self, halted_addr: u16) {
        if self.dmc_state == DmcDmaState::Idle {
            self.dmc_state = DmcDmaState::Halt { halted_addr };
        }
    }

    /// Check if we need to halt the CPU for DMC DMA
    /// Returns true if DMC DMA should start on the current read cycle
    pub fn should_start_dmc_dma(&self) -> bool {
        matches!(self.dmc_state, DmcDmaState::Halt { .. })
    }

    /// Step the DMA state machine by one CPU cycle
    /// Returns a DmaAction describing what memory access to perform this cycle
    pub fn step(&mut self, total_cpu_cycles: u64) -> DmaAction {
        self.is_get_cycle = total_cpu_cycles % 2 == 0;

        // DMC DMA has priority over OAM DMA
        if self.dmc_state != DmcDmaState::Idle {
            return self.step_dmc_dma();
        }

        // OAM DMA
        if self.oam_state != OamDmaState::Idle {
            return self.step_oam_dma();
        }

        DmaAction::None
    }

    fn step_dmc_dma(&mut self) -> DmaAction {
        self.cycles_consumed += 1;

        match self.dmc_state {
            DmcDmaState::Idle => DmaAction::None,

            DmcDmaState::Halt { halted_addr } => {
                // Halt cycle: repeat the CPU's halted read
                self.dmc_state = DmcDmaState::Dummy { halted_addr };
                DmaAction::DummyRead(halted_addr)
            }

            DmcDmaState::Dummy { halted_addr } => {
                // Dummy cycle complete, check if we need alignment
                if !self.is_get_cycle {
                    self.dmc_state = DmcDmaState::Aligning { halted_addr };
                    DmaAction::DummyRead(halted_addr)
                } else {
                    self.dmc_state = DmcDmaState::Reading;
                    DmaAction::DmcRead
                }
            }

            DmcDmaState::Aligning { .. } => {
                // Alignment cycle complete, do the actual read
                self.dmc_state = DmcDmaState::Reading;
                DmaAction::DmcRead
            }

            DmcDmaState::Reading => {
                // DMC read complete
                self.dmc_state = DmcDmaState::Idle;
                DmaAction::None
            }
        }
    }

    fn step_oam_dma(&mut self) -> DmaAction {
        self.cycles_consumed += 1;

        match self.oam_state {
            OamDmaState::Idle => DmaAction::None,

            OamDmaState::Pending { page } => {
                // First cycle: halt
                if self.oam_needs_align {
                    self.oam_state = OamDmaState::Aligning { page };
                    DmaAction::DummyRead(0) // Dummy read for halt
                } else {
                    self.oam_state = OamDmaState::Reading { page, offset: 0 };
                    DmaAction::DummyRead(0) // Dummy read for halt
                }
            }

            OamDmaState::Aligning { page } => {
                // Alignment cycle complete, start reading
                self.oam_state = OamDmaState::Reading { page, offset: 0 };
                DmaAction::DummyRead(0) // Dummy cycle
            }

            OamDmaState::Reading { page, offset } => {
                // Read from source
                let addr = ((page as u16) << 8) | (offset as u16);
                self.oam_state = OamDmaState::Writing {
                    page,
                    offset,
                    value: 0, // Will be filled after read
                };
                DmaAction::OamRead(addr)
            }

            OamDmaState::Writing {
                page,
                offset,
                value,
            } => {
                // Write to $2004
                let next_offset = offset.wrapping_add(1);
                if next_offset == 0 {
                    // Transfer complete
                    self.oam_state = OamDmaState::Idle;
                } else {
                    self.oam_state = OamDmaState::Reading {
                        page,
                        offset: next_offset,
                    };
                }
                DmaAction::OamWrite(value)
            }
        }
    }

    /// Update OAM DMA state with the value read from memory
    pub fn set_oam_read_value(&mut self, value: u8) {
        if let OamDmaState::Writing { page, offset, .. } = self.oam_state {
            self.oam_state = OamDmaState::Writing {
                page,
                offset,
                value,
            };
        }
    }

    /// Get the cycles consumed and reset the counter
    pub fn take_cycles_consumed(&mut self) -> u16 {
        let cycles = self.cycles_consumed;
        self.cycles_consumed = 0;
        cycles
    }

    /// Reset all DMA state
    pub fn reset(&mut self) {
        self.oam_state = OamDmaState::Idle;
        self.dmc_state = DmcDmaState::Idle;
        self.cycles_consumed = 0;
        self.oam_needs_align = false;
        self.is_get_cycle = true;
    }
}

/// Actions that can be returned by the DMA controller
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaAction {
    /// No DMA action this cycle
    None,
    /// Dummy read from address (discard value)
    DummyRead(u16),
    /// Read for OAM DMA from the given address
    OamRead(u16),
    /// Write to $2004 (OAM data) with the given value
    OamWrite(u8),
    /// DMC sample byte read (address comes from APU)
    DmcRead,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_controller_is_idle() {
        let dma = DmaController::new();
        assert!(!dma.is_active());
        assert_eq!(dma.oam_state, OamDmaState::Idle);
        assert_eq!(dma.dmc_state, DmcDmaState::Idle);
    }

    #[test]
    fn test_oam_dma_start_on_even_cycle() {
        let mut dma = DmaController::new();
        dma.start_oam_dma(0x02, false); // Not odd cycle

        assert!(dma.is_active());
        assert!(dma.oam_dma_active());
        assert!(!dma.oam_needs_align);
    }

    #[test]
    fn test_oam_dma_start_on_odd_cycle() {
        let mut dma = DmaController::new();
        dma.start_oam_dma(0x02, true); // Odd cycle

        assert!(dma.is_active());
        assert!(dma.oam_dma_active());
        assert!(dma.oam_needs_align);
    }

    #[test]
    fn test_dmc_dma_start() {
        let mut dma = DmaController::new();
        dma.start_dmc_dma(0x8000);

        assert!(dma.is_active());
        assert!(dma.dmc_dma_active());
        assert!(dma.should_start_dmc_dma());
    }

    #[test]
    fn test_dmc_has_priority_over_oam() {
        let mut dma = DmaController::new();

        // Start OAM DMA first
        dma.start_oam_dma(0x02, false);

        // Then DMC DMA request comes in
        dma.start_dmc_dma(0x8000);

        // Step should process DMC first
        let action = dma.step(0);
        assert!(
            matches!(action, DmaAction::DummyRead(_)),
            "DMC should take priority"
        );
        assert!(dma.dmc_dma_active());
    }

    #[test]
    fn test_set_oam_read_value_updates_writing_state() {
        let mut dma = DmaController::new();

        dma.start_oam_dma(0x02, false);
        let action = dma.step(0);
        assert!(matches!(action, DmaAction::DummyRead(_)));

        let action = dma.step(1);
        assert!(matches!(action, DmaAction::OamRead(0x0200)));

        dma.set_oam_read_value(0xAB);
        match dma.oam_state {
            OamDmaState::Writing { value, .. } => assert_eq!(value, 0xAB),
            _ => panic!("expected OAM DMA to be in Writing state"),
        }
    }

    #[test]
    fn test_take_cycles_consumed_returns_and_resets_counter() {
        let mut dma = DmaController::new();

        dma.start_oam_dma(0x02, false);
        let _ = dma.step(0);
        let _ = dma.step(1);

        let cycles = dma.take_cycles_consumed();
        assert!(cycles >= 2);
        assert_eq!(dma.take_cycles_consumed(), 0);
    }

    #[test]
    fn test_reset_clears_state_and_counters() {
        let mut dma = DmaController::new();
        dma.start_oam_dma(0x02, true);
        dma.start_dmc_dma(0x8000);
        let _ = dma.step(0);

        dma.reset();
        assert_eq!(dma.oam_state, OamDmaState::Idle);
        assert_eq!(dma.dmc_state, DmcDmaState::Idle);
        assert_eq!(dma.cycles_consumed, 0);
        assert!(!dma.is_active());
    }
}
