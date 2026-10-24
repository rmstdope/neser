//! Lockstep with input delay: a frame runs only when both players' pads for it are known.
//!
//! Each side owns one [`Lockstep`]. Every frame the caller offers the local pad to
//! [`Lockstep::advance`]; the pad is scheduled `input_delay` frames ahead, and the call
//! returns both pads for the next frame only once the peer's pad for it has arrived. The
//! first `input_delay` frames run on released pads, which is what gives the peer's inputs
//! time to cross the link.
//!
//! [`Lockstep::packet`] repeats every local input the peer has not yet acknowledged, so a
//! lost packet costs nothing once a later one arrives; duplicated, reordered and stale
//! packets are harmless. Every `hash_interval` frames each side records a hash of its game
//! state, and the two are compared to detect a desync.
//!
//! The state machine has no clock and no I/O: the caller decides when to advance, when to
//! send and what arrived.

use std::collections::VecDeque;

use super::wire::{MAX_INPUTS_PER_PACKET, Packet, WireError};

/// How many of each side's state hashes are kept for comparison.
const HASH_HISTORY: usize = 16;

/// Which pad a side plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Player {
    /// Player 1, `pads[0]`.
    One,
    /// Player 2, `pads[1]`.
    Two,
}

impl Player {
    fn index(self) -> usize {
        match self {
            Player::One => 0,
            Player::Two => 1,
        }
    }
}

/// What both sides of a session must agree on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockstepConfig {
    /// Frames between a press and the frame it is applied to; at least 1.
    pub input_delay: u32,
    /// State hashes are compared on frames divisible by this; 0 compares none.
    pub hash_interval: u32,
}

/// Both pads for one frame, one bit per button id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameInputs {
    /// The frame these pads are for.
    pub frame: u32,
    /// `pads[0]` is player 1, `pads[1]` player 2.
    pub pads: [u16; 2],
}

/// The two sides' games differ: their state hashes for `frame` disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Desync {
    /// The frame whose hashes differ.
    pub frame: u32,
    /// This side's hash.
    pub local_hash: u32,
    /// The peer's hash.
    pub remote_hash: u32,
}

/// Why a received packet was refused. A refused packet changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockstepError {
    /// The bytes are not a packet.
    Wire(WireError),
    /// The packet carries inputs further ahead than a peer in lockstep can be.
    OutOfWindow {
        /// The packet's first frame.
        first_frame: u32,
        /// How many inputs it carries.
        count: usize,
    },
    /// The packet acknowledges local inputs that were never sent.
    AckAhead {
        /// The acknowledged frame.
        ack: u32,
    },
}

/// One side of a lockstep session.
#[derive(Debug, Clone)]
pub struct Lockstep {
    local: Player,
    config: LockstepConfig,
    /// The frame the game runs next.
    next_frame: u32,
    /// Local pads from frame `local_base` on: still needed by the peer or by this side.
    local_inputs: VecDeque<u16>,
    local_base: u32,
    /// Remote pads from frame `next_frame` on, `None` where not yet received.
    remote_inputs: VecDeque<Option<u16>>,
    /// The first local frame the peer still needs.
    remote_ack: u32,
    /// The newest `(frame, hash)` pairs of each side, oldest first.
    local_hashes: VecDeque<(u32, u32)>,
    remote_hashes: VecDeque<(u32, u32)>,
    desync: Option<Desync>,
}

impl Lockstep {
    /// A session that has run no frame yet, playing `local`.
    ///
    /// # Panics
    ///
    /// If `config.input_delay` is 0: neither side could then ever send a pad before the
    /// other needs it.
    pub fn new(local: Player, config: LockstepConfig) -> Self {
        assert!(config.input_delay >= 1, "input_delay must be at least 1");
        let delay = config.input_delay as usize;
        Lockstep {
            local,
            config,
            next_frame: 0,
            local_inputs: VecDeque::from(vec![0; delay]),
            local_base: 0,
            remote_inputs: VecDeque::from(vec![Some(0); delay]),
            remote_ack: 0,
            local_hashes: VecDeque::new(),
            remote_hashes: VecDeque::new(),
            desync: None,
        }
    }

    /// The frame the game runs next.
    pub fn next_frame(&self) -> u32 {
        self.next_frame
    }

    /// Both pads for the next frame, if both are known; the caller then runs that frame.
    /// `local_pad` is recorded for frame `next_frame + input_delay` only when the frame
    /// advances, so a held frame records nothing. `None` means the game holds this frame.
    pub fn advance(&mut self, local_pad: u16) -> Option<FrameInputs> {
        let remote = (*self.remote_inputs.front()?)?;
        let local = self.local_inputs[(self.next_frame - self.local_base) as usize];
        let mut pads = [0; 2];
        pads[self.local.index()] = local;
        pads[1 - self.local.index()] = remote;
        let inputs = FrameInputs {
            frame: self.next_frame,
            pads,
        };
        self.local_inputs.push_back(local_pad);
        self.remote_inputs.pop_front();
        self.next_frame += 1;
        self.drop_unneeded_local_inputs();
        Some(inputs)
    }

    /// The packet to send now: every local pad the peer has not acknowledged (oldest first,
    /// at most [`MAX_INPUTS_PER_PACKET`]), this side's acknowledgement, and its newest hash.
    pub fn packet(&self) -> Packet {
        Packet {
            ack: self.ack(),
            first_frame: self.remote_ack,
            inputs: self
                .local_inputs
                .iter()
                .skip((self.remote_ack - self.local_base) as usize)
                .take(MAX_INPUTS_PER_PACKET)
                .copied()
                .collect(),
            state_hash: self.local_hashes.back().copied(),
        }
    }

    /// Take in bytes the peer sent. A refused packet changes nothing.
    pub fn receive(&mut self, bytes: &[u8]) -> Result<(), LockstepError> {
        let packet = Packet::decode(bytes).map_err(LockstepError::Wire)?;
        self.remote_ack = self.remote_ack.max(packet.ack);
        for (frame, pad) in (u64::from(packet.first_frame)..).zip(packet.inputs) {
            let Some(index) = frame
                .checked_sub(u64::from(self.next_frame))
                .map(|i| i as usize)
            else {
                continue;
            };
            if self.remote_inputs.len() <= index {
                self.remote_inputs.resize(index + 1, None);
            }
            self.remote_inputs[index].get_or_insert(pad);
        }
        self.drop_unneeded_local_inputs();
        Ok(())
    }

    /// Record the hash of the game state after `frame` ran. Only frames divisible by
    /// `hash_interval` are kept and compared.
    pub fn record_state_hash(&mut self, frame: u32, hash: u32) {
        let _ = (frame, hash, HASH_HISTORY);
        todo!()
    }

    /// The first state mismatch found, kept for the rest of the session.
    pub fn desync(&self) -> Option<Desync> {
        self.desync
    }

    /// The first remote frame not yet received: what the peer must still send.
    fn ack(&self) -> u32 {
        let known = self
            .remote_inputs
            .iter()
            .position(Option::is_none)
            .unwrap_or(self.remote_inputs.len());
        self.next_frame + known as u32
    }

    /// Forget local pads both the peer (acknowledged) and this side (already run) are done with.
    fn drop_unneeded_local_inputs(&mut self) {
        while self.local_base < self.remote_ack.min(self.next_frame) {
            self.local_inputs.pop_front();
            self.local_base += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(local: Player, input_delay: u32) -> Lockstep {
        Lockstep::new(
            local,
            LockstepConfig {
                input_delay,
                hash_interval: 30,
            },
        )
    }

    /// Deliver each side's current packet to the other.
    fn exchange(a: &mut Lockstep, b: &mut Lockstep) {
        let to_b = a.packet().encode();
        let to_a = b.packet().encode();
        b.receive(&to_b).expect("a's packet");
        a.receive(&to_a).expect("b's packet");
    }

    #[test]
    fn the_first_input_delay_frames_run_without_the_peer() {
        let mut a = session(Player::One, 3);
        for frame in 0..3 {
            assert_eq!(
                a.advance(0x0F),
                Some(FrameInputs {
                    frame,
                    pads: [0, 0]
                })
            );
        }
        assert_eq!(a.advance(0x0F), None);
    }

    #[test]
    fn a_frame_never_runs_without_the_remote_input() {
        let mut a = session(Player::One, 2);
        a.advance(1);
        a.advance(2);
        for _ in 0..100 {
            assert_eq!(a.advance(3), None);
        }
        assert_eq!(a.next_frame(), 2);
    }

    #[test]
    fn received_inputs_unblock_the_held_frame() {
        let mut a = session(Player::One, 2);
        let mut b = session(Player::Two, 2);
        for pad in [0x11, 0x12] {
            a.advance(pad);
        }
        for pad in [0x21, 0x22] {
            b.advance(pad);
        }
        assert_eq!(a.advance(0x13), None);
        exchange(&mut a, &mut b);
        assert_eq!(
            a.advance(0x13),
            Some(FrameInputs {
                frame: 2,
                pads: [0x11, 0x21]
            })
        );
        assert_eq!(
            b.advance(0x23),
            Some(FrameInputs {
                frame: 2,
                pads: [0x11, 0x21]
            })
        );
    }

    #[test]
    fn local_input_lands_input_delay_frames_ahead() {
        let mut a = session(Player::One, 2);
        a.advance(0x11);
        a.advance(0x12);
        let packet = a.packet();
        assert_eq!(packet.first_frame, 0);
        assert_eq!(packet.inputs, [0, 0, 0x11, 0x12]);
    }

    #[test]
    fn a_held_frame_records_no_local_input() {
        let mut a = session(Player::One, 1);
        a.advance(0x11);
        a.advance(0x99);
        a.advance(0x99);
        assert_eq!(a.packet().inputs, [0, 0x11]);
    }

    #[test]
    #[should_panic(expected = "input_delay")]
    fn an_input_delay_of_zero_is_refused() {
        session(Player::One, 0);
    }
}
