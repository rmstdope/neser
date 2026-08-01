//! SNES Super Multitap / Multiplayer 5 adaptor.
//!
//! The adaptor presents four controller slots on one physical port and uses
//! WRIO (`$4201`) bit 7 to choose between the first pair (players 2/3) and the
//! second pair (players 4/5).

use serde::{Deserialize, Serialize};

use super::standard_controller::StandardController;
use super::{SnesButton, SnesController, SnesControllerState, pressed_mask_to_joypad_state};

/// Persisted state for a multitap.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct MultitapState {
    #[serde(default)]
    pub players: [SnesControllerState; 4],
    #[serde(default)]
    pub select_high: bool,
    /// OUT0 strobe line: while high the multitap reports its detection
    /// signature instead of controller data. Defaults to false for states
    /// saved before it existed.
    #[serde(default)]
    pub strobe_high: bool,
}

/// Multitap device with four independent standard controllers.
#[derive(Debug, Clone, Default)]
pub struct Multitap {
    players: [StandardController; 4],
    select_high: bool,
    strobe_high: bool,
}

impl Multitap {
    pub fn new() -> Self {
        Self {
            players: std::array::from_fn(|_| StandardController::new()),
            select_high: true,
            strobe_high: false,
        }
    }
}

impl SnesController for Multitap {
    fn write_strobe(&mut self, high: bool) {
        self.strobe_high = high;
        for player in &mut self.players {
            player.write_strobe(high);
        }
    }

    fn write_select(&mut self, high: bool) {
        self.select_high = high;
    }

    fn read(&mut self) -> (bool, bool) {
        // Multitap detection: while the strobe is held high the adaptor drives
        // data1 = 0, data2 = 1 (the port reads 0x02) regardless of buttons, so
        // games can tell a multitap from a lone controller (Mesen2
        // `Multitap::ReadRam`).
        if self.strobe_high {
            return (false, true);
        }

        let pair = if self.select_high {
            &mut self.players[..2]
        } else {
            &mut self.players[2..]
        };
        let (left, right) = pair.split_at_mut(1);
        let (data1, _) = left[0].read();
        let (data2, _) = right[0].read();
        (data1, data2)
    }

    fn set_button(&mut self, button: SnesButton, pressed: bool) -> bool {
        self.set_player_button(0, button, pressed)
    }

    fn set_player_button(&mut self, player: u8, button: SnesButton, pressed: bool) -> bool {
        let Some(slot) = self.players.get_mut(player as usize) else {
            return false;
        };
        slot.set_button(button, pressed)
    }

    fn button_states(&self) -> u16 {
        self.players[0].button_states()
    }

    fn player_joypad_button_states(&self, player: u8) -> u8 {
        let Some(slot) = self.players.get(player as usize) else {
            return 0;
        };
        pressed_mask_to_joypad_state(slot.button_states())
    }

    fn capture_state(&self) -> SnesControllerState {
        self.players[0].capture_state()
    }

    fn restore_state(&mut self, state: &SnesControllerState) {
        self.players[0].restore_state(state);
    }

    fn capture_multitap_state(&self) -> Option<MultitapState> {
        Some(MultitapState {
            players: [
                self.players[0].capture_state(),
                self.players[1].capture_state(),
                self.players[2].capture_state(),
                self.players[3].capture_state(),
            ],
            select_high: self.select_high,
            strobe_high: self.strobe_high,
        })
    }

    fn restore_multitap_state(&mut self, state: &MultitapState) {
        self.select_high = state.select_high;
        self.strobe_high = state.strobe_high;
        for (player, saved) in self.players.iter_mut().zip(state.players.iter()) {
            player.restore_state(saved);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// While the strobe is held high the multitap reports its detection
    /// signature -- data1 = 0, data2 = 1 (the port reads 0x02) -- regardless of
    /// the buttons held or which pair is selected. Mirrors Mesen2
    /// `Multitap::ReadRam` (`return 0x02` while `_strobe`), which is how games
    /// detect a multitap (a standard controller returns its live B on data1).
    #[test]
    fn strobe_high_reports_the_multitap_detection_signature() {
        let mut tap = Multitap::new();
        tap.set_player_button(0, SnesButton::B, true);
        tap.set_player_button(1, SnesButton::B, true);
        tap.write_select(true);
        tap.write_strobe(true);

        assert_eq!(tap.read(), (false, true));
        assert_eq!(
            tap.read(),
            (false, true),
            "held high keeps reporting the signature"
        );
    }

    /// Dropping the strobe begins a normal serial read of the selected pair;
    /// the detection read must not corrupt or advance the stream.
    #[test]
    fn strobe_low_reads_the_selected_pair_after_detection() {
        let mut tap = Multitap::new();
        tap.set_player_button(0, SnesButton::B, true); // slot 0 -> data1
        tap.set_player_button(1, SnesButton::B, true); // slot 1 -> data2
        tap.write_select(true);

        tap.write_strobe(true);
        let _ = tap.read(); // detection signature
        tap.write_strobe(false);

        // First serial bit out of each controller is its B button.
        assert_eq!(tap.read(), (true, true));
    }
}
