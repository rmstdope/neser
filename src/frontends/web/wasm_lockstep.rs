//! The page's handle on the lockstep rules (nr-ayk5.5): [`crate::platform::netplay::lockstep`]
//! behind `wasm_bindgen`, so the browser runs a frame only once both players' pads for it are
//! known. The page owns the clock and the link; this owns no more than the rules do.
//!
//! State hashes are not compared here (`hash_interval` 0): noticing a drifted game is nr-ayk5.6.

use wasm_bindgen::prelude::*;

use crate::platform::netplay::lockstep::{Lockstep, LockstepConfig, Player};

#[wasm_bindgen]
pub struct WasmLockstep {
    inner: Lockstep,
}

#[wasm_bindgen]
impl WasmLockstep {
    /// One side of a session: `player` 1 (the host) or 2 (the joiner), with an input delay of
    /// at least one frame, the same on both sides.
    #[wasm_bindgen(constructor)]
    pub fn new(player: u8, input_delay: u32) -> Result<WasmLockstep, JsValue> {
        let local = match player {
            1 => Player::One,
            2 => Player::Two,
            _ => return Err(JsValue::from_str("player must be 1 or 2")),
        };
        if input_delay == 0 {
            return Err(JsValue::from_str("input delay must be at least 1"));
        }
        let config = LockstepConfig {
            input_delay,
            hash_interval: 0,
        };
        Ok(WasmLockstep {
            inner: Lockstep::new(local, config),
        })
    }

    /// `[frame, pad1, pad2]` for the frame to run now, or empty while it holds.
    pub fn advance(&mut self, local_pad: u16) -> Vec<u32> {
        self.inner
            .advance(local_pad)
            .map(|inputs| {
                vec![
                    inputs.frame,
                    u32::from(inputs.pads[0]),
                    u32::from(inputs.pads[1]),
                ]
            })
            .unwrap_or_default()
    }

    /// The bytes to send the other side now.
    pub fn packet(&self) -> Vec<u8> {
        self.inner.packet().encode()
    }

    /// Takes in bytes the other side sent; false when they were refused, which changes nothing.
    pub fn receive(&mut self, bytes: &[u8]) -> bool {
        self.inner.receive(bytes).is_ok()
    }

    /// The frame the game runs next.
    pub fn next_frame(&self) -> u32 {
        self.inner.next_frame()
    }

    /// The other side's pads known from the next frame on; more than the input delay means
    /// the other side has run ahead.
    pub fn remote_frames_known(&self) -> u32 {
        self.inner.remote_frames_known()
    }

    pub fn input_delay(&self) -> u32 {
        self.inner.input_delay()
    }
}
