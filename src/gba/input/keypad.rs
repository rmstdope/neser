//! GBA keypad — `KEYINPUT` (P1, `0x0400_0130`) and `KEYCNT` (`0x0400_0132`).
//!
//! Models the 10-button GBA pad with active-low [`KEYINPUT`][1] semantics
//! (a register bit reads `0` while the corresponding button is held
//! pressed) and key-interrupt generation per [`KEYCNT`][1] (`IRQ3` /
//! [`bits::KEYPAD`]).
//!
//! KEYINPUT bit layout (GBATek):
//!
//! | Bit | Button |
//! |-----|--------|
//! | 0   | A      |
//! | 1   | B      |
//! | 2   | Select |
//! | 3   | Start  |
//! | 4   | Right  |
//! | 5   | Left   |
//! | 6   | Up     |
//! | 7   | Down   |
//! | 8   | R      |
//! | 9   | L      |
//!
//! KEYCNT shares bits 0–9 with KEYINPUT (selecting which buttons may
//! trigger the IRQ). Bit 14 is the IRQ enable; bit 15 is the condition:
//! `0` = OR (any selected button pressed), `1` = AND (all selected
//! buttons pressed).
//!
//! Button IDs use the same NES-convention as the rest of the platform
//! API:
//!
//! | ID  | Button |
//! |-----|--------|
//! | 0   | A      |
//! | 1   | B      |
//! | 2   | Select |
//! | 3   | Start  |
//! | 4   | Up     |
//! | 5   | Down   |
//! | 6   | Left   |
//! | 7   | Right  |
//! | 8   | L      |
//! | 9   | R      |
//!
//! [1]: <https://problemkaputt.de/gbatek.htm#gbakeypadinput>

use crate::gba::bus::interrupt::{InterruptController, bits};
use serde::{Deserialize, Serialize};

/// Address of `KEYINPUT` (read-only).
pub const REG_KEYINPUT: u32 = 0x0400_0130;
/// Address of `KEYCNT` (read/write).
pub const REG_KEYCNT: u32 = 0x0400_0132;

/// Mask covering the 10 button bits used by `KEYINPUT` / `KEYCNT[0..9]`.
pub const KEYS_MASK: u16 = 0x03FF;
/// `KEYCNT` IRQ-enable bit.
pub const KEYCNT_IRQ_ENABLE: u16 = 1 << 14;
/// `KEYCNT` condition bit. `1` = logical AND (all buttons), `0` = OR (any).
pub const KEYCNT_COND_AND: u16 = 1 << 15;

/// GBA keypad state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Keypad {
    /// Pressed-state of each button (`1` = pressed). Bit layout matches
    /// `KEYINPUT` (which is the inverse of this — see [`Self::read_keyinput`]).
    pressed: u16,
    /// `KEYCNT` register backing.
    keycnt: u16,
    /// Last evaluated value of the `KEYCNT` IRQ condition. Used to
    /// raise [`bits::KEYPAD`] only on `false→true` transitions so that
    /// holding a configured button (or repeated bulk `set_states`
    /// updates with unchanged input) cannot keep re-asserting `IF`
    /// after software clears it.
    irq_active: bool,
}

impl Keypad {
    /// Create a new keypad with no buttons pressed and `KEYCNT` cleared.
    pub fn new() -> Self {
        Self::default()
    }

    /// Read the `KEYINPUT` register.
    ///
    /// Returns active-low button state in bits 0–9 — a bit reads `0`
    /// while the button is held. Bits 10–15 are unused and read as `0`
    /// per GBATek.
    pub fn read_keyinput(&self) -> u16 {
        (!self.pressed) & KEYS_MASK
    }

    /// Read the `KEYCNT` register.
    pub fn read_keycnt(&self) -> u16 {
        self.keycnt
    }

    /// Write the `KEYCNT` register and re-evaluate the keypad IRQ
    /// condition. Raises [`bits::KEYPAD`] in `ic` if the new
    /// configuration is currently satisfied.
    pub fn write_keycnt(&mut self, value: u16, ic: &mut InterruptController) {
        self.keycnt = value;
        self.update_irq(ic);
    }

    /// Update a single button by its NES-convention id (see module docs).
    /// IDs outside `0..=9` are silently ignored.
    pub fn set_button(&mut self, id: u8, pressed: bool, ic: &mut InterruptController) {
        if let Some(bit) = button_id_to_keyinput_bit(id) {
            let mask = 1u16 << bit;
            if pressed {
                self.pressed |= mask;
            } else {
                self.pressed &= !mask;
            }
            self.update_irq(ic);
        }
    }

    /// Return all 8 NES-convention button states as a bitmask.
    ///
    /// Bit layout: A=0, B=1, Select=2, Start=3, Up=4, Down=5, Left=6,
    /// Right=7. The 8-bit return value cannot represent the GBA-only
    /// `L` and `R` shoulder buttons; those remain accessible via
    /// [`Self::set_button`].
    pub fn get_states(&self) -> u8 {
        let actions = (self.pressed & 0x000F) as u8; // A,B,Sel,Start
        let right = ((self.pressed >> 4) & 1) as u8;
        let left = ((self.pressed >> 5) & 1) as u8;
        let up = ((self.pressed >> 6) & 1) as u8;
        let down = ((self.pressed >> 7) & 1) as u8;
        actions | (up << 4) | (down << 5) | (left << 6) | (right << 7)
    }

    /// Set the 8 NES-convention buttons in bulk. Preserves the `L`/`R`
    /// shoulder buttons and re-evaluates the keypad IRQ.
    pub fn set_states(&mut self, state: u8, ic: &mut InterruptController) {
        let actions = (state & 0x0F) as u16;
        let up = ((state >> 4) & 1) as u16;
        let down = ((state >> 5) & 1) as u16;
        let left = ((state >> 6) & 1) as u16;
        let right = ((state >> 7) & 1) as u16;
        // Preserve L (bit 9) / R (bit 8).
        let lr = self.pressed & 0x0300;
        self.pressed = actions | (right << 4) | (left << 5) | (up << 6) | (down << 7) | lr;
        self.update_irq(ic);
    }

    /// Re-evaluate the keypad IRQ. Raises [`bits::KEYPAD`] only on a
    /// `false→true` transition of the `KEYCNT`-configured condition so
    /// that holding a configured button — or repeated bulk
    /// `set_states` updates with unchanged input — does not keep
    /// re-asserting `IF` after software acknowledges the previous one.
    fn update_irq(&mut self, ic: &mut InterruptController) {
        let now = self.irq_condition_met();
        if now && !self.irq_active {
            ic.raise(bits::KEYPAD);
        }
        self.irq_active = now;
    }

    /// Whether the `KEYCNT`-configured key-interrupt condition is met
    /// against the current pressed state.
    fn irq_condition_met(&self) -> bool {
        if self.keycnt & KEYCNT_IRQ_ENABLE == 0 {
            return false;
        }
        let select = self.keycnt & KEYS_MASK;
        if select == 0 {
            return false;
        }
        let pressed_selected = self.pressed & select;
        if self.keycnt & KEYCNT_COND_AND != 0 {
            pressed_selected == select
        } else {
            pressed_selected != 0
        }
    }
}

/// Map a NES-convention button id to a `KEYINPUT` bit position.
fn button_id_to_keyinput_bit(id: u8) -> Option<u8> {
    Some(match id {
        0 => 0, // A
        1 => 1, // B
        2 => 2, // Select
        3 => 3, // Start
        4 => 6, // Up
        5 => 7, // Down
        6 => 5, // Left
        7 => 4, // Right
        8 => 9, // L
        9 => 8, // R
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ic() -> InterruptController {
        InterruptController::new()
    }

    #[test]
    fn keyinput_default_reads_all_released() {
        let kp = Keypad::new();
        assert_eq!(kp.read_keyinput(), KEYS_MASK);
    }

    #[test]
    fn keyinput_active_low_for_a_button() {
        let mut kp = Keypad::new();
        let mut ic = ic();
        kp.set_button(0, true, &mut ic); // A
        assert_eq!(kp.read_keyinput(), 0x03FE);
        kp.set_button(0, false, &mut ic);
        assert_eq!(kp.read_keyinput(), 0x03FF);
    }

    #[test]
    fn keyinput_bit_mapping_for_all_ten_buttons() {
        // (id, expected KEYINPUT bit cleared when pressed)
        let cases: [(u8, u16); 10] = [
            (0, 0), // A      → bit 0
            (1, 1), // B      → bit 1
            (2, 2), // Select → bit 2
            (3, 3), // Start  → bit 3
            (4, 6), // Up     → bit 6
            (5, 7), // Down   → bit 7
            (6, 5), // Left   → bit 5
            (7, 4), // Right  → bit 4
            (8, 9), // L      → bit 9
            (9, 8), // R      → bit 8
        ];
        for (id, bit) in cases {
            let mut kp = Keypad::new();
            let mut ic = ic();
            kp.set_button(id, true, &mut ic);
            assert_eq!(
                kp.read_keyinput(),
                KEYS_MASK & !(1u16 << bit),
                "button id {id} should clear KEYINPUT bit {bit}"
            );
        }
    }

    #[test]
    fn keyinput_a_plus_b_pressed() {
        let mut kp = Keypad::new();
        let mut ic = ic();
        kp.set_button(0, true, &mut ic);
        kp.set_button(1, true, &mut ic);
        assert_eq!(kp.read_keyinput(), 0x03FC);
    }

    #[test]
    fn keyinput_all_ten_pressed_reads_zero() {
        let mut kp = Keypad::new();
        let mut ic = ic();
        for id in 0..10u8 {
            kp.set_button(id, true, &mut ic);
        }
        assert_eq!(kp.read_keyinput(), 0x0000);
    }

    #[test]
    fn unknown_button_id_is_silently_ignored() {
        let mut kp = Keypad::new();
        let mut ic = ic();
        kp.set_button(42, true, &mut ic);
        assert_eq!(kp.read_keyinput(), KEYS_MASK);
    }

    // ── KEYCNT / IRQ ─────────────────────────────────────────────────

    #[test]
    fn keypad_irq_not_raised_when_keycnt_disabled() {
        let mut kp = Keypad::new();
        let mut ic = ic();
        // KEYCNT selects A but IRQ-enable bit is clear.
        kp.write_keycnt(0x0001, &mut ic);
        kp.set_button(0, true, &mut ic);
        assert_eq!(ic.if_flags & bits::KEYPAD, 0);
    }

    #[test]
    fn keypad_irq_raised_or_mode_when_any_selected_pressed() {
        let mut kp = Keypad::new();
        let mut ic = ic();
        // Select A, IRQ-enable, OR mode.
        kp.write_keycnt(KEYCNT_IRQ_ENABLE | 0x0001, &mut ic);
        assert_eq!(ic.if_flags & bits::KEYPAD, 0);
        kp.set_button(0, true, &mut ic);
        assert_ne!(ic.if_flags & bits::KEYPAD, 0);
    }

    #[test]
    fn keypad_irq_or_mode_ignores_unselected_buttons() {
        let mut kp = Keypad::new();
        let mut ic = ic();
        // Select only A.
        kp.write_keycnt(KEYCNT_IRQ_ENABLE | 0x0001, &mut ic);
        // Press B (id=1), not selected.
        kp.set_button(1, true, &mut ic);
        assert_eq!(ic.if_flags & bits::KEYPAD, 0);
    }

    #[test]
    fn keypad_irq_and_mode_requires_all_selected() {
        let mut kp = Keypad::new();
        let mut ic = ic();
        // Select A+B, AND mode.
        kp.write_keycnt(KEYCNT_IRQ_ENABLE | KEYCNT_COND_AND | 0b11, &mut ic);
        kp.set_button(0, true, &mut ic);
        assert_eq!(
            ic.if_flags & bits::KEYPAD,
            0,
            "AND mode must require ALL selected buttons"
        );
        kp.set_button(1, true, &mut ic);
        assert_ne!(ic.if_flags & bits::KEYPAD, 0);
    }

    #[test]
    fn write_keycnt_re_evaluates_irq_when_button_already_pressed() {
        let mut kp = Keypad::new();
        let mut ic = ic();
        kp.set_button(0, true, &mut ic); // A pressed first
        assert_eq!(ic.if_flags & bits::KEYPAD, 0);
        kp.write_keycnt(KEYCNT_IRQ_ENABLE | 0x0001, &mut ic);
        assert_ne!(ic.if_flags & bits::KEYPAD, 0);
    }

    #[test]
    fn keypad_irq_disable_after_enable_does_not_re_raise() {
        let mut kp = Keypad::new();
        let mut ic = ic();
        kp.write_keycnt(KEYCNT_IRQ_ENABLE | 0x0001, &mut ic);
        kp.set_button(0, true, &mut ic);
        ic.acknowledge(bits::KEYPAD); // software clears IF
        // Now disable IRQ via KEYCNT and toggle the button — IRQ must stay clear.
        kp.write_keycnt(0x0001, &mut ic);
        kp.set_button(0, false, &mut ic);
        kp.set_button(0, true, &mut ic);
        assert_eq!(ic.if_flags & bits::KEYPAD, 0);
    }

    // ── set/get bulk states ──────────────────────────────────────────

    #[test]
    fn set_states_and_get_states_round_trip() {
        let mut kp = Keypad::new();
        let mut ic = ic();
        kp.set_states(0b1010_0101, &mut ic);
        assert_eq!(kp.get_states(), 0b1010_0101);
    }

    #[test]
    fn set_states_preserves_l_and_r_buttons() {
        let mut kp = Keypad::new();
        let mut ic = ic();
        kp.set_button(8, true, &mut ic); // L
        kp.set_button(9, true, &mut ic); // R
        // Bulk-update only the 8 NES-convention buttons.
        kp.set_states(0b0000_0001, &mut ic); // A pressed only
        // L / R remain pressed.
        assert_eq!(
            kp.read_keyinput(),
            KEYS_MASK & !((1 << 0) | (1 << 8) | (1 << 9))
        );
    }

    // ── IRQ edge-detection ───────────────────────────────────────────

    /// Holding a configured button across repeated `set_states` calls
    /// (e.g. a frontend pumping the same input every frame) must not
    /// re-raise `IF.KEYPAD` after software has acknowledged the
    /// previous interrupt — the IRQ is edge-triggered.
    #[test]
    fn held_button_does_not_re_raise_irq_via_set_states() {
        let mut kp = Keypad::new();
        let mut ic = ic();
        kp.write_keycnt(KEYCNT_IRQ_ENABLE | 0x0001, &mut ic); // select A
        kp.set_states(0b0000_0001, &mut ic); // press A
        assert_ne!(ic.if_flags & bits::KEYPAD, 0);
        ic.acknowledge(bits::KEYPAD);
        // Repeatedly re-apply the same state — should NOT re-raise.
        for _ in 0..8 {
            kp.set_states(0b0000_0001, &mut ic);
        }
        assert_eq!(
            ic.if_flags & bits::KEYPAD,
            0,
            "held key must not storm the IRQ flag"
        );
    }

    /// `set_button(true)` for an already-pressed button must not
    /// re-raise the IRQ either.
    #[test]
    fn redundant_press_does_not_re_raise_irq() {
        let mut kp = Keypad::new();
        let mut ic = ic();
        kp.write_keycnt(KEYCNT_IRQ_ENABLE | 0x0001, &mut ic);
        kp.set_button(0, true, &mut ic);
        ic.acknowledge(bits::KEYPAD);
        kp.set_button(0, true, &mut ic);
        assert_eq!(ic.if_flags & bits::KEYPAD, 0);
    }

    /// After releasing the configured button (condition false) and
    /// re-pressing it (condition true again), the IRQ must fire again
    /// — the edge re-armed.
    #[test]
    fn release_then_press_re_raises_irq() {
        let mut kp = Keypad::new();
        let mut ic = ic();
        kp.write_keycnt(KEYCNT_IRQ_ENABLE | 0x0001, &mut ic);
        kp.set_button(0, true, &mut ic);
        ic.acknowledge(bits::KEYPAD);
        kp.set_button(0, false, &mut ic);
        kp.set_button(0, true, &mut ic);
        assert_ne!(ic.if_flags & bits::KEYPAD, 0);
    }

    /// Repeated `KEYCNT` writes that leave the condition continuously
    /// true must not re-raise the IRQ.
    #[test]
    fn redundant_keycnt_write_does_not_re_raise_irq() {
        let mut kp = Keypad::new();
        let mut ic = ic();
        kp.set_button(0, true, &mut ic);
        kp.write_keycnt(KEYCNT_IRQ_ENABLE | 0x0001, &mut ic);
        ic.acknowledge(bits::KEYPAD);
        kp.write_keycnt(KEYCNT_IRQ_ENABLE | 0x0001, &mut ic);
        assert_eq!(ic.if_flags & bits::KEYPAD, 0);
    }
}
