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
}

/// Multitap device with four independent standard controllers.
#[derive(Debug, Clone, Default)]
pub struct Multitap {
    players: [StandardController; 4],
    select_high: bool,
}

impl Multitap {
    pub fn new() -> Self {
        Self {
            players: std::array::from_fn(|_| StandardController::new()),
            select_high: true,
        }
    }
}

impl SnesController for Multitap {
    fn write_strobe(&mut self, high: bool) {
        for player in &mut self.players {
            player.write_strobe(high);
        }
    }

    fn write_select(&mut self, high: bool) {
        self.select_high = high;
    }

    fn read(&mut self) -> (bool, bool) {
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
        })
    }

    fn restore_multitap_state(&mut self, state: &MultitapState) {
        self.select_high = state.select_high;
        for (player, saved) in self.players.iter_mut().zip(state.players.iter()) {
            player.restore_state(saved);
        }
    }
}
