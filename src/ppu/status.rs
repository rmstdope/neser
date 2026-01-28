/// Manages PPU status flags including VBlank, sprite 0 hit, and NMI
pub struct Status {
    /// VBlank flag (bit 7 of status register)
    vblank_flag: bool,
    /// Sprite 0 Hit flag (bit 6 of status register)
    sprite_0_hit: bool,
    /// Pending sprite 0 hit (becomes readable next cycle)
    pending_sprite_0_hit: bool,
    /// Sprite Overflow flag (bit 5 of status register)
    sprite_overflow: bool,
    /// NMI enabled flag
    nmi_enabled: bool,
    /// Frame complete flag - set when VBlank starts, regardless of NMI generation
    frame_complete: bool,
}

impl Default for Status {
    fn default() -> Self {
        Self::new()
    }
}

impl Status {
    /// Create a new Status instance
    pub fn new() -> Self {
        Self {
            vblank_flag: false,
            sprite_0_hit: false,
            pending_sprite_0_hit: false,
            sprite_overflow: false,
            nmi_enabled: false,
            frame_complete: false,
        }
    }

    /// Reset status to initial state
    pub fn reset(&mut self) {
        self.vblank_flag = false;
        self.sprite_0_hit = false;
        self.pending_sprite_0_hit = false;
        self.sprite_overflow = false;
        self.nmi_enabled = false;
        self.frame_complete = false;
    }

    /// Enter VBlank period
    pub fn enter_vblank(&mut self) {
        // println!("PPU Status: Entering VBlank");
        self.vblank_flag = true;
        self.frame_complete = true;
    }

    /// Exit VBlank period (clear VBL flag, but NOT sprite flags)
    pub fn exit_vblank(&mut self) {
        // println!("PPU Status: Exiting VBlank");
        self.vblank_flag = false;
        // Note: Sprite 0 hit and sprite overflow are cleared separately at scanline 261, pixel 0
    }

    /// Clear sprite 0 hit and sprite overflow flags (happens at scanline 261, pixel 0)
    pub fn clear_sprite_flags(&mut self) {
        self.sprite_0_hit = false;
        self.pending_sprite_0_hit = false;
        self.sprite_overflow = false;
    }

    /// Trigger NMI edge (used when NMI is enabled mid-VBlank)
    pub fn trigger_nmi(&mut self) {
        self.nmi_enabled = true;
    }

    /// Clear any latched/pending NMI edge.
    pub fn clear_nmi(&mut self) {
        self.nmi_enabled = false;
    }

    /// Read the status register (clears VBlank flag and write toggle)
    /// Returns the status byte
    pub fn read_status(&mut self) -> u8 {
        let status = self.peek_status();

        // Reading status clears VBlank flag.
        self.vblank_flag = false;

        status
    }

    /// Read the status register without side effects.
    /// Returns the status byte without clearing flags.
    pub fn peek_status(&self) -> u8 {
        let mut status = 0u8;

        if self.vblank_flag {
            status |= 0b1000_0000; // Bit 7: VBlank
        }
        if self.sprite_0_hit {
            status |= 0b0100_0000; // Bit 6: Sprite 0 hit
        }
        if self.sprite_overflow {
            status |= 0b0010_0000; // Bit 5: Sprite overflow
        }

        status
    }

    /// Poll NMI status and clear it
    pub fn poll_nmi(&mut self) -> bool {
        let result = self.nmi_enabled;
        self.nmi_enabled = false;
        result
    }

    /// Poll frame complete status and clear it
    pub fn poll_frame_complete(&mut self) -> bool {
        let result = self.frame_complete;
        self.frame_complete = false;
        result
    }

    /// Check if we're in VBlank period
    pub fn is_in_vblank(&self) -> bool {
        self.vblank_flag
    }

    /// Set sprite 0 hit flag immediately
    pub fn set_sprite_0_hit(&mut self) {
        // println!("PPU Status: Setting Sprite 0 Hit flag");
        self.sprite_0_hit = true;
    }

    /// Set sprite overflow flag
    pub fn set_sprite_overflow(&mut self) {
        // println!("PPU Status: Setting Sprite Overflow flag");
        self.sprite_overflow = true;
    }

    /// Check if sprite 0 hit flag is set
    pub fn is_sprite_0_hit(&self) -> bool {
        self.sprite_0_hit
    }

    /// Get sprite overflow flag for save-state.
    pub fn is_sprite_overflow(&self) -> bool {
        self.sprite_overflow
    }

    /// Get NMI enabled flag for save-state.
    pub fn is_nmi_enabled(&self) -> bool {
        self.nmi_enabled
    }

    pub fn pending_sprite_0_hit(&self) -> bool {
        self.pending_sprite_0_hit
    }

    pub fn frame_complete(&self) -> bool {
        self.frame_complete
    }

    /// Restore status state from a save-state.
    pub fn restore_state(
        &mut self,
        vblank: bool,
        sprite_0_hit: bool,
        sprite_overflow: bool,
        nmi_enabled: bool,
    ) {
        self.vblank_flag = vblank;
        self.sprite_0_hit = sprite_0_hit;
        self.pending_sprite_0_hit = false;
        self.sprite_overflow = sprite_overflow;
        self.nmi_enabled = nmi_enabled;
        // Note: frame_complete is derived state, will be set during next vblank
        self.frame_complete = false;
    }

    pub fn set_pending_sprite_0_hit(&mut self, pending: bool) {
        self.pending_sprite_0_hit = pending;
    }

    pub fn set_frame_complete(&mut self, complete: bool) {
        self.frame_complete = complete;
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusDebugState {
    pub vblank_flag: bool,
    pub sprite_0_hit: bool,
    pub pending_sprite_0_hit: bool,
    pub sprite_overflow: bool,
    pub nmi_enabled: bool,
    pub frame_complete: bool,
}

#[cfg(test)]
impl Status {
    pub fn debug_state(&self) -> StatusDebugState {
        StatusDebugState {
            vblank_flag: self.vblank_flag,
            sprite_0_hit: self.sprite_0_hit,
            pending_sprite_0_hit: self.pending_sprite_0_hit,
            sprite_overflow: self.sprite_overflow,
            nmi_enabled: self.nmi_enabled,
            frame_complete: self.frame_complete,
        }
    }

    pub fn set_debug_state(&mut self, state: StatusDebugState) {
        self.vblank_flag = state.vblank_flag;
        self.sprite_0_hit = state.sprite_0_hit;
        self.pending_sprite_0_hit = state.pending_sprite_0_hit;
        self.sprite_overflow = state.sprite_overflow;
        self.nmi_enabled = state.nmi_enabled;
        self.frame_complete = state.frame_complete;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_new() {
        let status = Status::new();
        assert!(!status.is_in_vblank());
        assert!(!status.is_sprite_0_hit());
    }

    #[test]
    fn test_status_reset() {
        let mut status = Status::new();
        status.enter_vblank();
        status.reset();
        assert!(!status.is_in_vblank());
    }

    #[test]
    fn test_enter_vblank() {
        let mut status = Status::new();
        status.enter_vblank();
        assert!(status.is_in_vblank());
    }

    #[test]
    fn test_exit_vblank() {
        let mut status = Status::new();
        status.enter_vblank();
        status.exit_vblank();
        assert!(!status.is_in_vblank());
    }

    #[test]
    fn test_read_status_clears_vblank() {
        let mut status = Status::new();
        status.enter_vblank();

        let status_byte = status.read_status();
        assert_eq!(status_byte & 0b1000_0000, 0b1000_0000);
        assert!(!status.is_in_vblank());
    }

    #[test]
    fn test_peek_status_does_not_clear_vblank() {
        let mut status = Status::new();
        status.enter_vblank();

        let status_byte = status.peek_status();
        assert_eq!(status_byte & 0b1000_0000, 0b1000_0000);
        assert!(status.is_in_vblank());
    }

    #[test]
    fn test_read_status_clears_vblank_even_on_vblank_start() {
        let mut status = Status::new();
        status.enter_vblank();

        // Reading at the VBlank start dot should still clear the flag.
        let status_byte = status.read_status();
        assert_eq!(status_byte & 0b1000_0000, 0b1000_0000);
        assert!(!status.is_in_vblank());
    }

    #[test]
    fn test_sprite_0_hit() {
        let mut status = Status::new();
        status.set_sprite_0_hit();
        assert!(status.is_sprite_0_hit());

        let status_byte = status.read_status();
        assert_eq!(status_byte & 0b0100_0000, 0b0100_0000);
    }

    #[test]
    fn test_sprite_overflow() {
        let mut status = Status::new();
        status.set_sprite_overflow();

        let status_byte = status.read_status();
        assert_eq!(status_byte & 0b0010_0000, 0b0010_0000);
    }

    #[test]
    fn test_poll_nmi() {
        let mut status = Status::new();
        status.enter_vblank();

        // NMI edge must be explicitly latched.
        status.trigger_nmi();

        assert!(status.poll_nmi());
        assert!(!status.poll_nmi()); // Should be cleared after first poll
    }

    #[test]
    fn test_poll_frame_complete() {
        let mut status = Status::new();
        status.enter_vblank();

        assert!(status.poll_frame_complete());
        assert!(!status.poll_frame_complete()); // Should be cleared after first poll
    }
}
