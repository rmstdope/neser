//! Game Boy joypad input register ($FF00 / P1).
//!
//! Implements the P1 register per Pan Docs:
//! <https://gbdev.io/pandocs/Joypad_Input.html>
//!
//! The register uses two selection groups:
//! - **P14** (bit4=0): direction buttons — Right, Left, Up, Down
//! - **P15** (bit5=0): action buttons   — A, B, Select, Start
//!
//! Button IDs follow the NES convention used throughout the platform API:
//! A=0, B=1, Select=2, Start=3, Up=4, Down=5, Left=6, Right=7.

use serde::{Deserialize, Serialize};

/// Game Boy joypad ($FF00 / P1 register).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Joypad {
    /// Bits 4-5 written by the game (`val & 0x30`); bit=0 means group is selected.
    select_bits: u8,
    /// Button state for P14 (direction): 1=pressed.
    /// bit0=Right, bit1=Left, bit2=Up, bit3=Down.
    p14_state: u8,
    /// Button state for P15 (action): 1=pressed.
    /// bit0=A, bit1=B, bit2=Select, bit3=Start.
    p15_state: u8,
    /// Last computed effective lower nibble; used for joypad IRQ edge detection.
    prev_nibble: u8,
}

impl Joypad {
    pub fn new() -> Self {
        Self {
            select_bits: 0x00, // both groups selected (P14+P15 active low)
            p14_state: 0,
            p15_state: 0,
            prev_nibble: 0xF,
        }
    }

    /// Read the P1 register.
    ///
    /// Returns `0xC0 | select_bits | effective_nibble()`.
    /// Bits 7-6 are always 1 (open-bus); bits 5-4 reflect the select lines;
    /// bits 3-0 are the inverted active button states (0 = pressed).
    pub fn read(&self) -> u8 {
        0xC0 | self.select_bits | self.effective_nibble()
    }

    /// Write the P1 register.
    ///
    /// Only bits 4-5 are writable (select lines). Resets the IRQ edge baseline
    /// so that a group change does not spuriously re-fire the interrupt.
    pub fn write(&mut self, val: u8) {
        self.select_bits = val & 0x30;
        self.prev_nibble = self.effective_nibble();
    }

    /// Update a button state. Returns `true` if a joypad interrupt should fire.
    ///
    /// An interrupt fires on any **1→0 falling edge** of a selected output
    /// line, i.e. whenever a button in the active group transitions from
    /// released to pressed. Multiple simultaneous presses each generate a
    /// separate falling edge.
    ///
    /// `id` uses the NES platform convention:
    /// A=0, B=1, Select=2, Start=3, Up=4, Down=5, Left=6, Right=7.
    /// Unknown IDs are silently ignored and never trigger an interrupt.
    pub fn set_button(&mut self, id: u8, pressed: bool) -> bool {
        let (state, mask) = match id {
            0 => (&mut self.p15_state, 0x01u8), // A      → P15 bit0
            1 => (&mut self.p15_state, 0x02),   // B      → P15 bit1
            2 => (&mut self.p15_state, 0x04),   // Select → P15 bit2
            3 => (&mut self.p15_state, 0x08),   // Start  → P15 bit3
            4 => (&mut self.p14_state, 0x04),   // Up     → P14 bit2
            5 => (&mut self.p14_state, 0x08),   // Down   → P14 bit3
            6 => (&mut self.p14_state, 0x02),   // Left   → P14 bit1
            7 => (&mut self.p14_state, 0x01),   // Right  → P14 bit0
            _ => return false,
        };
        if pressed {
            *state |= mask;
        } else {
            *state &= !mask;
        }
        let new_nibble = self.effective_nibble();
        let irq = (self.prev_nibble & !new_nibble) != 0;
        self.prev_nibble = new_nibble;
        irq
    }

    /// Return all eight button states as a NES-convention bitmask.
    ///
    /// Each bit N is 1 when the button with ID N is pressed.
    pub fn get_states(&self) -> u8 {
        // bits 0-3: P15 action buttons (A, B, Select, Start)
        let actions = self.p15_state & 0x0F;
        // bit4 (Up)    = p14 bit2
        let up = (self.p14_state & 0x04) << 2;
        // bit5 (Down)  = p14 bit3
        let down = (self.p14_state & 0x08) << 2;
        // bit6 (Left)  = p14 bit1
        let left = (self.p14_state & 0x02) << 5;
        // bit7 (Right) = p14 bit0
        let right = (self.p14_state & 0x01) << 7;
        actions | up | down | left | right
    }

    /// Set all button states from a NES-convention bitmask (bulk update).
    ///
    /// Updates the IRQ edge baseline (`prev_nibble`) to the new effective
    /// nibble without firing an interrupt, so subsequent `set_button()` calls
    /// compare against the current bulk-updated state.
    pub fn set_states(&mut self, state: u8) {
        self.p15_state = state & 0x0F;
        self.p14_state = ((state & 0x10) >> 2)  // Up   (bit4) → p14 bit2
            | ((state & 0x20) >> 2)              // Down (bit5) → p14 bit3
            | ((state & 0x40) >> 5)              // Left (bit6) → p14 bit1
            | ((state & 0x80) >> 7); // Right(bit7) → p14 bit0
        self.prev_nibble = self.effective_nibble();
    }

    /// Clear all button states (called after loading a save state).
    ///
    /// Physical key/button states from save time are meaningless after restore
    /// — no keys are physically held — so we reset them to avoid stuck inputs.
    /// `select_bits` is left intact (hardware register state). `prev_nibble` is
    /// rebased to the new effective nibble so subsequent presses compute IRQ
    /// edges correctly without spuriously re-firing.
    pub fn clear_buttons(&mut self) {
        self.p14_state = 0;
        self.p15_state = 0;
        self.prev_nibble = self.effective_nibble();
    }

    /// Compute the effective lower nibble for the currently selected group(s).
    ///
    /// Returns `0xF` when no group is selected. Otherwise ANDs together the
    /// inverted states of every selected group, so that a pressed button (1)
    /// pulls its corresponding output bit low (0), as the hardware does.
    fn effective_nibble(&self) -> u8 {
        let mut nibble = 0x0Fu8;
        if self.select_bits & 0x10 == 0 {
            // P14 selected (direction buttons)
            nibble &= !self.p14_state & 0x0F;
        }
        if self.select_bits & 0x20 == 0 {
            // P15 selected (action buttons)
            nibble &= !self.p15_state & 0x0F;
        }
        nibble
    }
}

impl Default for Joypad {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── helpers ────────────────────────────────────────────────────────────

    /// Fresh joypad with both groups selected (write 0x00).
    fn make_both_selected() -> Joypad {
        let mut j = Joypad::new();
        j.write(0x00);
        j
    }

    /// Fresh joypad with only P15 (action buttons) selected (write 0x10).
    fn make_p15_selected() -> Joypad {
        let mut j = Joypad::new();
        j.write(0x10);
        j
    }

    /// Fresh joypad with only P14 (direction buttons) selected (write 0x20).
    fn make_p14_selected() -> Joypad {
        let mut j = Joypad::new();
        j.write(0x20);
        j
    }

    // ── read/write ─────────────────────────────────────────────────────────

    /// After construction, both groups selected (P14+P15 active low),
    /// no buttons pressed → reads $CF (hardware power-on default).
    #[test]
    fn test_default_read_after_new() {
        let j = Joypad::new();
        assert_eq!(j.read(), 0xCF, "expected 0xCF, got {:#04x}", j.read());
    }

    /// After construction and selecting both groups, no buttons pressed →
    /// lower nibble must be 0xF (all output lines high = released).
    #[test]
    fn test_read_with_both_groups_selected_no_buttons_pressed() {
        // Given: both groups selected, no buttons pressed
        let j = make_both_selected();
        // When: read P1
        let val = j.read();
        // Then: lower nibble is 0xF (0xCF)
        assert_eq!(val, 0xCF, "expected 0xCF, got {val:#04x}");
    }

    /// Bits 7-6 are always 1 regardless of state.
    #[test]
    fn test_upper_bits_always_set_in_read() {
        let j = Joypad::new();
        assert_eq!(j.read() & 0xC0, 0xC0);
    }

    /// Write 0x10 (P15 selected) — bits 5-4 of read must reflect that.
    #[test]
    fn test_select_bits_reflected_in_read() {
        let j = make_p15_selected();
        // bit4=1 (P14 not selected), bit5=0 (P15 selected) → 0x10 in bits 5-4
        assert_eq!(j.read() & 0x30, 0x10);
    }

    // ── P15 (action buttons) ───────────────────────────────────────────────

    /// P15 selected; pressing A (id=0) pulls bit0 of read low.
    #[test]
    fn test_p15_select_a_button_pressed() {
        // Given: P15 selected only
        let mut j = make_p15_selected();
        // When: press A
        j.set_button(0, true);
        // Then: read = 0xC0 | 0x10 | 0x0E = 0xDE (bit0 clear)
        assert_eq!(j.read(), 0xDE, "expected 0xDE, got {:#04x}", j.read());
    }

    /// P15 selected; pressing all four action buttons clears the lower nibble.
    #[test]
    fn test_p15_all_action_buttons_pressed() {
        let mut j = make_p15_selected();
        for id in 0..=3u8 {
            j.set_button(id, true);
        }
        // All four P15 bits pressed → lower nibble = 0x0
        assert_eq!(j.read() & 0x0F, 0x00);
    }

    // ── P14 (direction buttons) ────────────────────────────────────────────

    /// P14 selected; pressing Right (id=7) pulls bit0 of read low.
    #[test]
    fn test_p14_select_right_pressed() {
        // Given: P14 selected only
        let mut j = make_p14_selected();
        // When: press Right
        j.set_button(7, true);
        // Then: read = 0xC0 | 0x20 | 0x0E = 0xEE (bit0 clear)
        assert_eq!(j.read(), 0xEE, "expected 0xEE, got {:#04x}", j.read());
    }

    /// P14 selected; Up (id=4) maps to p14 bit2; pressing it clears bit2.
    #[test]
    fn test_p14_select_up_pressed() {
        let mut j = make_p14_selected();
        j.set_button(4, true); // Up → p14 bit2
        // effective_nibble = !0x04 & 0x0F = 0x0B
        assert_eq!(j.read() & 0x0F, 0x0B);
    }

    // ── neither / both groups ──────────────────────────────────────────────

    /// Neither group selected: pressing a button does not
    /// change the lower nibble (all output lines float high).
    #[test]
    fn test_neither_group_selected_lower_nibble_stays_f() {
        // Given: neither group selected (write 0x30 to select neither)
        let mut j = Joypad::new();
        j.write(0x30);
        // When: press A
        j.set_button(0, true);
        // Then: lower nibble is still 0xF
        assert_eq!(j.read() & 0x0F, 0x0F);
    }

    /// Both groups selected; A (P15 bit0) and Right (P14 bit0) both map to
    /// output bit0 — pressing either clears that bit.
    #[test]
    fn test_both_groups_selected_combines_bit0() {
        let mut j = make_both_selected();
        j.set_button(0, true); // A → P15 bit0
        j.set_button(7, true); // Right → P14 bit0
        // Both drive output bit0 low
        assert_eq!(j.read() & 0x01, 0x00);
    }

    // ── joypad interrupt ───────────────────────────────────────────────────

    /// First button press in selected group fires an interrupt (0xF → non-0xF).
    #[test]
    fn test_interrupt_fires_on_first_press_in_selected_group() {
        // Given: P15 selected
        let mut j = make_p15_selected();
        // When: press A (first button, nibble transitions from 0xF)
        let irq = j.set_button(0, true);
        // Then: interrupt fires
        assert!(irq, "expected joypad IRQ on first press");
    }

    /// Pressing a button whose group is *not* selected does not fire interrupt.
    #[test]
    fn test_interrupt_no_fire_when_group_not_selected() {
        // Given: P14 selected only
        let mut j = make_p14_selected();
        // When: press A (action button, P15 not selected)
        let irq = j.set_button(0, true);
        // Then: no interrupt (effective nibble unchanged — P15 not active)
        assert!(!irq, "expected no IRQ when button's group not selected");
    }

    /// Re-pressing an already-held button does not fire a second IRQ (no new falling edge).
    #[test]
    fn test_interrupt_no_fire_on_repeated_press_of_same_button() {
        let mut j = make_p15_selected();
        j.set_button(0, true); // press A → IRQ fires
        let irq = j.set_button(0, true); // press A again while held → no new falling edge
        assert!(
            !irq,
            "expected no IRQ on repeated press of already-held button"
        );
    }

    /// Each new button press creates a new falling edge and fires IRQ independently.
    ///
    /// Per Pan Docs: the joypad IRQ fires on every 1→0 transition of a selected
    /// output line, not just the first press from the all-released state.
    #[test]
    fn test_interrupt_fires_for_each_new_button_press() {
        let mut j = make_p15_selected();
        j.set_button(0, true); // press A → IRQ fires
        let irq = j.set_button(1, true); // press B while A held → new falling edge → IRQ
        assert!(
            irq,
            "expected IRQ for B press while A held (new falling edge)"
        );
    }

    /// After fully releasing all buttons in the group, the next press fires
    /// the interrupt again.
    #[test]
    fn test_interrupt_refires_after_full_release() {
        let mut j = make_p15_selected();
        j.set_button(0, true); // press A → IRQ fires
        j.set_button(0, false); // release A → nibble back to 0xF
        let irq = j.set_button(0, true); // press A again → IRQ should fire
        assert!(irq, "expected IRQ to re-fire after full release");
    }

    /// After a group switch and switch back, re-pressing an already-held button
    /// does not fire a spurious IRQ (write() resets prev_nibble to current state).
    #[test]
    fn test_no_spurious_irq_after_group_switch_with_held_button() {
        let mut j = Joypad::new();
        // Press A while P15 is selected
        j.write(0x10);
        j.set_button(0, true); // IRQ fires; prev_nibble = 0xE
        // Switch group away — A still held; prev_nibble reset to current P14 nibble (0xF)
        j.write(0x20); // P14 selected
        // Switch back to P15 — A still held; prev_nibble reset to 0xE
        j.write(0x10);
        // Re-pressing A (already held): bit0 already low, no falling edge → no IRQ
        let irq = j.set_button(0, true);
        assert!(
            !irq,
            "re-pressing held button after group switch must not fire IRQ"
        );
    }

    // ── get_states / set_states ────────────────────────────────────────────

    /// Each button id (0..=7): press it, verify its bit is set in get_states().
    #[test]
    fn test_get_states_roundtrip_each_button() {
        for id in 0u8..=7 {
            let mut j = Joypad::new();
            j.set_button(id, true);
            let states = j.get_states();
            assert_ne!(
                states & (1 << id),
                0,
                "button id={id} not set in get_states()={states:#010b}"
            );
        }
    }

    /// set_states + get_states round-trip with a mixed bitmask.
    #[test]
    fn test_set_states_get_states_roundtrip() {
        let mut j = Joypad::new();
        let mask: u8 = 0b1011_0101; // A, Sel, Up, Down, Right pressed
        j.set_states(mask);
        assert_eq!(
            j.get_states(),
            mask,
            "round-trip failed: got {:#010b}",
            j.get_states()
        );
    }

    /// Releasing a button clears its bit in get_states.
    #[test]
    fn test_release_button_clears_state() {
        let mut j = Joypad::new();
        j.set_button(0, true);
        j.set_button(0, false);
        assert_eq!(j.get_states() & 0x01, 0x00);
    }

    /// set_states updates prev_nibble so that a subsequent set_button does not
    /// fire a spurious IRQ for a button that was already set by set_states.
    #[test]
    fn test_set_states_updates_prev_nibble() {
        let mut j = make_p15_selected();
        // Bulk-set A as pressed via set_states
        j.set_states(0x01); // A pressed (bit0)
        // A is already "pressed"; pressing it again is not a new falling edge
        let irq = j.set_button(0, true);
        assert!(!irq, "expected no IRQ: A was already in the bulk-set state");
    }
}
