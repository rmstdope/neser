//! OAM DMA (`$FF46`), written once for the DMG and CGB buses.
//!
//! A transfer takes 162 M-cycles: one warm-up (nothing copied), 160 copies
//! (OAM bytes 0-159 from `source << 8`), and one teardown. OAM is blocked from
//! the first copy until the teardown; a transfer restarted while one is
//! running keeps OAM blocked through its warm-up. Where each source byte is
//! read from, and which CPU accesses a running transfer displaces, differ by
//! model and stay with each bus.

/// Last M-cycle position of a transfer: the teardown.
const TEARDOWN: u8 = 161;

/// The state of one OAM DMA transfer.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct OamDma {
    active: bool,
    /// High byte of the source address (the value written to `$FF46`).
    source: u8,
    /// 0 = warm-up, 1-160 = copying byte `position - 1`, 161 = teardown.
    position: u8,
    /// Separate from `active`: OAM stays open through a fresh warm-up.
    oam_blocked: bool,
}

impl OamDma {
    /// Rebuild a transfer from its save-state fields.
    pub fn from_parts(active: bool, source: u8, position: u8, oam_blocked: bool) -> Self {
        Self {
            active,
            source,
            position,
            oam_blocked,
        }
    }

    /// The save-state fields: `(active, source, position, oam_blocked)`.
    pub fn parts(&self) -> (bool, u8, u8, bool) {
        (self.active, self.source, self.position, self.oam_blocked)
    }

    /// Start a transfer from `val << 8` (a `$FF46` write).
    pub fn start(&mut self, val: u8) {
        self.oam_blocked = self.active && self.oam_blocked;
        self.active = true;
        self.source = val;
        self.position = 0;
    }

    /// Advance one M-cycle. On a copy cycle, returns the OAM index to fill
    /// and the source address to read it from.
    pub fn step(&mut self) -> Option<(usize, u16)> {
        if !self.active {
            return None;
        }
        match self.position {
            0 => {
                self.position = 1;
                None
            }
            TEARDOWN => {
                self.active = false;
                self.oam_blocked = false;
                None
            }
            position => {
                self.oam_blocked = true;
                self.position += 1;
                let index = u16::from(position - 1);
                Some((usize::from(index), u16::from(self.source) << 8 | index))
            }
        }
    }

    /// High byte of the source address, as `$FF46` reads back.
    pub fn source(&self) -> u8 {
        self.source
    }

    /// Whether the CPU is locked out of OAM.
    pub fn blocks_oam(&self) -> bool {
        self.oam_blocked
    }

    /// Whether a transfer is copying, so CPU accesses may conflict with it.
    pub fn holds_bus(&self) -> bool {
        self.active && self.oam_blocked
    }

    /// The source address whose byte a displaced CPU read sees: the byte the
    /// transfer copied last.
    pub fn conflict_address(&self) -> u16 {
        (u16::from(self.source) << 8) + u16::from(self.position.saturating_sub(2))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn started_from(source: u8) -> OamDma {
        let mut dma = OamDma::default();
        dma.start(source);
        dma
    }

    #[test]
    fn a_fresh_transfer_leaves_oam_open_through_the_warm_up() {
        let mut dma = started_from(0xC0);
        assert!(!dma.blocks_oam());
        assert_eq!(dma.step(), None);
        assert!(!dma.blocks_oam());
        assert!(!dma.holds_bus());
    }

    #[test]
    fn copy_cycles_return_each_oam_index_with_its_source_address() {
        let mut dma = started_from(0xC0);
        dma.step();
        for index in 0..160usize {
            assert_eq!(dma.step(), Some((index, 0xC000 + index as u16)));
            assert!(dma.blocks_oam());
            assert!(dma.holds_bus());
        }
    }

    #[test]
    fn oam_opens_again_after_the_162nd_cycle() {
        let mut dma = started_from(0xC0);
        for _ in 0..161 {
            dma.step();
        }
        assert!(dma.blocks_oam());
        assert_eq!(dma.step(), None);
        assert!(!dma.blocks_oam());
        assert!(!dma.holds_bus());
    }

    #[test]
    fn a_restart_keeps_oam_blocked_through_its_warm_up() {
        let mut dma = started_from(0xC0);
        dma.step();
        dma.step();
        dma.start(0xD0);
        assert!(dma.blocks_oam());
        assert_eq!(dma.source(), 0xD0);
        assert_eq!(dma.step(), None);
        assert!(dma.blocks_oam());
        assert_eq!(dma.step(), Some((0, 0xD000)));
    }

    #[test]
    fn a_start_while_idle_does_not_block_even_after_a_finished_transfer() {
        let mut dma = started_from(0xC0);
        for _ in 0..162 {
            dma.step();
        }
        dma.start(0xC1);
        assert!(!dma.blocks_oam());
    }

    #[test]
    fn the_conflict_address_trails_the_copy_by_one_byte() {
        let mut dma = started_from(0xC0);
        dma.step();
        dma.step(); // copied byte 0
        assert_eq!(dma.conflict_address(), 0xC000);
        dma.step(); // copied byte 1
        assert_eq!(dma.conflict_address(), 0xC001);
    }

    #[test]
    fn idle_steps_copy_nothing() {
        let mut dma = OamDma::default();
        assert_eq!(dma.step(), None);
        assert!(!dma.blocks_oam());
    }

    #[test]
    fn parts_round_trip() {
        let dma = OamDma::from_parts(true, 0x9A, 17, true);
        assert_eq!(dma.parts(), (true, 0x9A, 17, true));
    }
}
