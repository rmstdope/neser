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
    /// Flag to track if we're on the exact cycle when VBlank starts (for race condition)
    vblank_start_cycle: bool,
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
            vblank_start_cycle: false,
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
        self.vblank_start_cycle = false;
    }

    /// Enter VBlank period
    pub fn enter_vblank(&mut self, nmi_on_vblank: bool) {
        // println!("PPU Status: Entering VBlank");
        self.vblank_flag = true;
        self.frame_complete = true;
        self.vblank_start_cycle = true;
        if nmi_on_vblank {
            self.nmi_enabled = true;
        }
    }

    /// Exit VBlank period (clear all flags)
    pub fn exit_vblank(&mut self) {
        // println!("PPU Status: Exiting VBlank");
        self.vblank_flag = false;
        self.nmi_enabled = false;
        self.sprite_0_hit = false;
        self.pending_sprite_0_hit = false;
        self.sprite_overflow = false;
    }

    /// Trigger NMI edge (used when NMI is enabled mid-VBlank)
    pub fn trigger_nmi(&mut self) {
        self.nmi_enabled = true;
    }

    /// Clear the VBlank start cycle flag
    pub fn clear_vblank_start_cycle(&mut self) {
        self.vblank_start_cycle = false;
    }

    /// Check if we're on the VBlank start cycle
    pub fn is_vblank_start_cycle(&self) -> bool {
        self.vblank_start_cycle
    }

    /// Read the status register (clears VBlank flag and write toggle)
    /// Returns the status byte
    pub fn read_status(&mut self) -> u8 {
        let mut status = 0u8;

        if self.vblank_flag {
            status |= 0b1000_0000; // Bit 7: VBlank
            // println!("PPU Status: VBlank flag set");
        }
        if self.sprite_0_hit {
            println!("PPU Status: Sprite 0 Hit flag set");
            status |= 0b0100_0000; // Bit 6: Sprite 0 hit
        }
        if self.sprite_overflow {
            status |= 0b0010_0000; // Bit 5: Sprite overflow
        }

        // Reading status clears VBlank flag (but not during vblank_start_cycle for race condition)
        if !self.vblank_start_cycle {
            self.vblank_flag = false;
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

    /// Set pending sprite 0 hit (will be applied next cycle)
    pub fn set_pending_sprite_0_hit(&mut self) {
        self.pending_sprite_0_hit = true;
    }

    /// Apply pending sprite 0 hit flag (call at start of cycle)
    pub fn apply_pending_sprite_0_hit(&mut self) {
        if self.pending_sprite_0_hit {
            // println!("PPU Status: Applying pending Sprite 0 Hit flag");
            self.sprite_0_hit = true;
            self.pending_sprite_0_hit = false;
        }
    }

    /// Set sprite overflow flag
    pub fn set_sprite_overflow(&mut self) {
        self.sprite_overflow = true;
    }

    /// Check if sprite 0 hit flag is set
    pub fn is_sprite_0_hit(&self) -> bool {
        self.sprite_0_hit
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
        status.enter_vblank(true);
        status.reset();
        assert!(!status.is_in_vblank());
    }

    #[test]
    fn test_enter_vblank() {
        let mut status = Status::new();
        status.enter_vblank(true);
        assert!(status.is_in_vblank());
        assert!(status.is_vblank_start_cycle());
    }

    #[test]
    fn test_exit_vblank() {
        let mut status = Status::new();
        status.enter_vblank(true);
        status.exit_vblank();
        assert!(!status.is_in_vblank());
    }

    #[test]
    fn test_read_status_clears_vblank() {
        let mut status = Status::new();
        status.enter_vblank(false);
        status.clear_vblank_start_cycle();

        let status_byte = status.read_status();
        assert_eq!(status_byte & 0b1000_0000, 0b1000_0000);
        assert!(!status.is_in_vblank());
    }

    #[test]
    fn test_read_status_during_vblank_start() {
        let mut status = Status::new();
        status.enter_vblank(false);

        // Reading during vblank_start_cycle should not clear flag
        let status_byte = status.read_status();
        assert_eq!(status_byte & 0b1000_0000, 0b1000_0000);
        assert!(status.is_in_vblank());
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
        status.enter_vblank(true);

        assert!(status.poll_nmi());
        assert!(!status.poll_nmi()); // Should be cleared after first poll
    }

    #[test]
    fn test_poll_frame_complete() {
        let mut status = Status::new();
        status.enter_vblank(false);

        assert!(status.poll_frame_complete());
        assert!(!status.poll_frame_complete()); // Should be cleared after first poll
    }
}
