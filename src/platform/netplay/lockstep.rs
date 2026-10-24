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

/// How many frames back each side's state hashes are kept for comparison: 10 seconds, far
/// longer than any link lockstep can play over, whatever the hash interval.
const HASH_SPAN_FRAMES: u32 = 600;

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
    /// Each side's `(frame, hash)` pairs of the last [`HASH_SPAN_FRAMES`], oldest first.
    local_hashes: VecDeque<(u32, u32)>,
    remote_hashes: VecDeque<(u32, u32)>,
    desync: Option<Desync>,
    /// Hashes of frames before this one are from before the last resync, and are ignored.
    hash_floor: u32,
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
            hash_floor: 0,
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
        let sent_end = self.local_base + self.local_inputs.len() as u32;
        if packet.ack > sent_end {
            return Err(LockstepError::AckAhead { ack: packet.ack });
        }
        let window_end = u64::from(self.next_frame) + 2 * u64::from(self.config.input_delay);
        if u64::from(packet.first_frame) + packet.inputs.len() as u64 > window_end {
            return Err(LockstepError::OutOfWindow {
                first_frame: packet.first_frame,
                count: packet.inputs.len(),
            });
        }
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
        if let Some((frame, hash)) = packet.state_hash.filter(|&(f, _)| self.is_hash_frame(f)) {
            remember(&mut self.remote_hashes, (frame, hash));
            self.compare_hashes();
        }
        Ok(())
    }

    /// Record the hash of the game state after `frame` ran. Only frames divisible by
    /// `hash_interval` are kept and compared.
    pub fn record_state_hash(&mut self, frame: u32, hash: u32) {
        if self.is_hash_frame(frame) {
            remember(&mut self.local_hashes, (frame, hash));
            self.compare_hashes();
        }
    }

    /// The two games were made the same again before frame `next_frame`: the desync and every
    /// hash so far are forgotten, and hashes of frames already run, still on their way from the
    /// peer, are ignored. Pads and the frame count are untouched.
    pub fn resynced(&mut self) {
        self.local_hashes.clear();
        self.remote_hashes.clear();
        self.desync = None;
        self.hash_floor = self.next_frame;
    }

    /// The first state mismatch found, kept until [`Lockstep::resynced`].
    pub fn desync(&self) -> Option<Desync> {
        self.desync
    }

    /// How many remote pads from `next_frame` on are known, without a gap. More than
    /// `input_delay` means the peer has run frames this side has not: this side is behind.
    pub fn remote_frames_known(&self) -> u32 {
        self.remote_inputs
            .iter()
            .position(Option::is_none)
            .unwrap_or(self.remote_inputs.len()) as u32
    }

    /// The input delay both sides play with.
    pub fn input_delay(&self) -> u32 {
        self.config.input_delay
    }

    /// The first remote frame not yet received: what the peer must still send.
    fn ack(&self) -> u32 {
        self.next_frame + self.remote_frames_known()
    }

    fn is_hash_frame(&self, frame: u32) -> bool {
        self.config.hash_interval != 0
            && frame >= self.hash_floor
            && frame.is_multiple_of(self.config.hash_interval)
    }

    /// Record the first frame both sides hashed differently, if there is one yet.
    fn compare_hashes(&mut self) {
        if self.desync.is_some() {
            return;
        }
        self.desync = self.local_hashes.iter().find_map(|&(frame, local_hash)| {
            self.remote_hashes
                .iter()
                .find(|&&(f, h)| f == frame && h != local_hash)
                .map(|&(_, remote_hash)| Desync {
                    frame,
                    local_hash,
                    remote_hash,
                })
        });
    }

    /// Forget local pads both the peer (acknowledged) and this side (already run) are done with.
    fn drop_unneeded_local_inputs(&mut self) {
        while self.local_base < self.remote_ack.min(self.next_frame) {
            self.local_inputs.pop_front();
            self.local_base += 1;
        }
    }
}

/// Keep `entry`, once per frame, and forget hashes more than [`HASH_SPAN_FRAMES`] older
/// than the newest kept.
fn remember(hashes: &mut VecDeque<(u32, u32)>, entry: (u32, u32)) {
    if hashes.iter().any(|&(frame, _)| frame == entry.0) {
        return;
    }
    let at = hashes.partition_point(|&(frame, _)| frame < entry.0);
    hashes.insert(at, entry);
    let newest = hashes.back().map_or(0, |&(frame, _)| frame);
    while hashes
        .front()
        .is_some_and(|&(frame, _)| frame.saturating_add(HASH_SPAN_FRAMES) < newest)
    {
        hashes.pop_front();
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
    fn a_side_whose_peer_ran_ahead_knows_more_than_its_delay() {
        let mut a = session(Player::One, 2);
        let mut b = session(Player::Two, 2);
        assert_eq!(a.remote_frames_known(), 2, "the released frames");
        b.advance(0x21);
        b.advance(0x22);
        a.receive(&b.packet().encode()).unwrap();
        assert_eq!(a.remote_frames_known(), 4, "b ran two frames a has not");
        a.advance(0x11);
        a.advance(0x12);
        assert_eq!(a.remote_frames_known(), 2, "a has caught up");
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

    #[test]
    fn packet_repeats_inputs_until_acknowledged() {
        let mut a = session(Player::One, 2);
        a.advance(0x11);
        let lost = a.packet();
        a.advance(0x12);
        let next = a.packet();
        assert_eq!(lost.inputs, [0, 0, 0x11]);
        assert_eq!(next.first_frame, 0);
        assert_eq!(
            next.inputs,
            [0, 0, 0x11, 0x12],
            "the lost inputs ride again"
        );
        let mut b = session(Player::Two, 2);
        b.receive(&next.encode()).unwrap();
        assert_eq!(b.packet().ack, 4, "b now needs a's frame 4");
    }

    #[test]
    fn acknowledged_inputs_are_not_resent() {
        let mut a = session(Player::One, 2);
        let mut b = session(Player::Two, 2);
        for pad in [0x11, 0x12] {
            a.advance(pad);
            b.advance(pad << 4);
        }
        exchange(&mut a, &mut b);
        exchange(&mut a, &mut b);
        let packet = a.packet();
        assert_eq!(packet.first_frame, 4);
        assert!(packet.inputs.is_empty());
        a.advance(0x13);
        exchange(&mut a, &mut b);
        assert!(a.advance(0x14).is_some());
        assert_eq!(a.packet().first_frame, 4);
        assert_eq!(a.packet().inputs, [0x13, 0x14]);
    }

    #[test]
    fn duplicate_reordered_and_stale_packets_are_harmless() {
        let mut a = session(Player::One, 2);
        a.advance(0x11);
        let first = a.packet().encode();
        a.advance(0x12);
        let second = a.packet().encode();
        let mut in_order = session(Player::Two, 2);
        in_order.receive(&second).unwrap();
        let mut shuffled = session(Player::Two, 2);
        for bytes in [&second, &first, &second, &first] {
            shuffled.receive(bytes).unwrap();
        }
        assert_eq!(shuffled.packet(), in_order.packet());
        for _ in 0..4 {
            assert_eq!(shuffled.advance(0), in_order.advance(0));
        }
        // Long after those frames ran, the old packet still changes nothing.
        let before = shuffled.packet();
        shuffled.receive(&first).unwrap();
        assert_eq!(shuffled.packet(), before);
    }

    #[test]
    fn a_later_packet_never_overwrites_a_known_input() {
        let mut b = session(Player::Two, 2);
        let packet = |pad| {
            Packet {
                ack: 0,
                first_frame: 2,
                inputs: vec![pad],
                state_hash: None,
            }
            .encode()
        };
        b.receive(&packet(0x11)).unwrap();
        b.receive(&packet(0x77)).unwrap();
        b.advance(0);
        b.advance(0);
        assert_eq!(b.advance(0).unwrap().pads[0], 0x11);
    }

    #[test]
    fn inputs_beyond_the_window_are_rejected() {
        let packet = |first_frame, count| {
            Packet {
                ack: 0,
                first_frame,
                inputs: vec![1; count],
                state_hash: None,
            }
            .encode()
        };
        let mut b = session(Player::Two, 2);
        // A peer in lockstep is at most input_delay frames ahead, so its pads reach at
        // most frame next_frame + 2 * input_delay - 1.
        assert_eq!(b.receive(&packet(0, 4)), Ok(()));
        let before = b.packet();
        assert_eq!(
            b.receive(&packet(0, 5)),
            Err(LockstepError::OutOfWindow {
                first_frame: 0,
                count: 5
            })
        );
        assert_eq!(
            b.receive(&packet(u32::MAX, 1)),
            Err(LockstepError::OutOfWindow {
                first_frame: u32::MAX,
                count: 1
            })
        );
        assert_eq!(b.packet(), before);
    }

    #[test]
    fn an_ack_for_inputs_never_sent_is_rejected() {
        let mut a = session(Player::One, 2);
        let ack = |ack| {
            Packet {
                ack,
                first_frame: 0,
                inputs: vec![],
                state_hash: None,
            }
            .encode()
        };
        assert_eq!(a.receive(&ack(2)), Ok(()), "frames 0 and 1 were sent");
        assert_eq!(a.receive(&ack(3)), Err(LockstepError::AckAhead { ack: 3 }));
        assert_eq!(a.packet().first_frame, 2);
    }

    #[test]
    fn a_malformed_packet_is_an_error_and_changes_nothing() {
        let mut b = session(Player::Two, 2);
        let before = b.packet();
        assert_eq!(
            b.receive(&[0xFF, 1, 2]),
            Err(LockstepError::Wire(WireError::UnknownVersion(0xFF)))
        );
        assert_eq!(
            b.receive(&[]),
            Err(LockstepError::Wire(WireError::Truncated))
        );
        assert_eq!(b.packet(), before);
    }

    #[test]
    fn matching_hashes_report_no_desync() {
        let mut a = session(Player::One, 2);
        let mut b = session(Player::Two, 2);
        for frame in [0, 30, 60] {
            a.record_state_hash(frame, frame * 7);
            b.record_state_hash(frame, frame * 7);
            exchange(&mut a, &mut b);
        }
        assert_eq!(a.desync(), None);
        assert_eq!(b.desync(), None);
    }

    #[test]
    fn a_differing_hash_is_reported_with_its_frame() {
        let mut a = session(Player::One, 2);
        let mut b = session(Player::Two, 2);
        a.record_state_hash(0, 5);
        b.record_state_hash(0, 5);
        exchange(&mut a, &mut b);
        a.record_state_hash(30, 0xAAAA);
        b.record_state_hash(30, 0xBBBB);
        exchange(&mut a, &mut b);
        assert_eq!(
            a.desync(),
            Some(Desync {
                frame: 30,
                local_hash: 0xAAAA,
                remote_hash: 0xBBBB
            })
        );
        assert_eq!(
            b.desync(),
            Some(Desync {
                frame: 30,
                local_hash: 0xBBBB,
                remote_hash: 0xAAAA
            })
        );
    }

    #[test]
    fn a_remote_hash_arriving_first_is_compared_once_ours_exists() {
        let mut a = session(Player::One, 2);
        let mut b = session(Player::Two, 2);
        b.record_state_hash(30, 0xBBBB);
        exchange(&mut a, &mut b);
        assert_eq!(a.desync(), None);
        a.record_state_hash(30, 0xAAAA);
        assert_eq!(a.desync().map(|d| d.frame), Some(30));
    }

    #[test]
    fn the_first_desync_is_kept() {
        let mut a = session(Player::One, 2);
        let mut b = session(Player::Two, 2);
        for frame in [30, 60] {
            a.record_state_hash(frame, 1);
            b.record_state_hash(frame, 2);
            exchange(&mut a, &mut b);
        }
        assert_eq!(a.desync().map(|d| d.frame), Some(30));
    }

    #[test]
    fn only_hash_interval_frames_are_kept() {
        let mut a = session(Player::One, 2);
        a.record_state_hash(31, 9);
        assert_eq!(a.packet().state_hash, None);
        a.record_state_hash(60, 9);
        assert_eq!(a.packet().state_hash, Some((60, 9)));
        let mut never = Lockstep::new(
            Player::One,
            LockstepConfig {
                input_delay: 2,
                hash_interval: 0,
            },
        );
        never.record_state_hash(0, 9);
        assert_eq!(never.packet().state_hash, None);
    }

    #[test]
    fn a_remote_hash_off_the_interval_is_ignored() {
        let mut a = session(Player::One, 2);
        a.record_state_hash(30, 1);
        let off_interval = Packet {
            ack: 0,
            first_frame: 0,
            inputs: vec![],
            state_hash: Some((30 + 1, 2)),
        };
        a.receive(&off_interval.encode()).unwrap();
        assert_eq!(a.desync(), None);
    }

    #[test]
    fn a_remote_hash_arriving_long_after_ours_is_still_compared() {
        // Hashing every frame over a slow link: the peer's hash for frame 0 arrives after
        // this side has hashed 100 more frames, and must still meet ours.
        let config = LockstepConfig {
            input_delay: 2,
            hash_interval: 1,
        };
        let mut a = Lockstep::new(Player::One, config);
        let mut b = Lockstep::new(Player::Two, config);
        b.record_state_hash(0, 0xBBBB);
        let late = b.packet().encode();
        for frame in 0..=100 {
            a.record_state_hash(frame, 0xAAAA);
        }
        a.receive(&late).unwrap();
        assert_eq!(a.desync().map(|d| d.frame), Some(0));
    }

    #[test]
    fn a_hash_for_the_last_frame_there_is_does_not_panic() {
        let mut a = session(Player::One, 2);
        a.record_state_hash(0, 1);
        let hostile = Packet {
            ack: 0,
            first_frame: 0,
            inputs: vec![],
            state_hash: Some((u32::MAX / 30 * 30, 2)),
        };
        a.receive(&hostile.encode()).unwrap();
        a.record_state_hash(30, 1);
        assert_eq!(a.desync(), None);
    }

    /// Both sides run `frames` frames, swapping packets after each.
    fn run_both(a: &mut Lockstep, b: &mut Lockstep, frames: u32) {
        for _ in 0..frames {
            a.advance(0).expect("a runs");
            b.advance(0).expect("b runs");
            exchange(a, b);
        }
    }

    #[test]
    fn resynced_forgets_the_desync_and_compares_again_from_the_next_frame() {
        let mut a = session(Player::One, 2);
        let mut b = session(Player::Two, 2);
        run_both(&mut a, &mut b, 31);
        a.record_state_hash(30, 0xAAAA);
        b.record_state_hash(30, 0xBBBB);
        exchange(&mut a, &mut b);
        assert!(a.desync().is_some() && b.desync().is_some());
        a.resynced();
        b.resynced();
        assert_eq!(a.desync(), None);
        assert_eq!(
            a.packet().state_hash,
            None,
            "no hash from before the resync is sent"
        );
        run_both(&mut a, &mut b, 30);
        a.record_state_hash(60, 0x6060);
        b.record_state_hash(60, 0x6060);
        exchange(&mut a, &mut b);
        assert_eq!(a.desync(), None, "the games match again");
        run_both(&mut a, &mut b, 30);
        a.record_state_hash(90, 1);
        b.record_state_hash(90, 2);
        exchange(&mut a, &mut b);
        assert_eq!(
            a.desync().map(|d| d.frame),
            Some(90),
            "a later drift is noticed again"
        );
    }

    #[test]
    fn a_stale_hash_from_before_the_resync_is_ignored() {
        let mut a = session(Player::One, 2);
        let mut b = session(Player::Two, 2);
        run_both(&mut a, &mut b, 31);
        b.record_state_hash(30, 0xBBBB);
        let in_flight = b.packet().encode();
        a.record_state_hash(30, 0xAAAA);
        a.resynced();
        a.receive(&in_flight).unwrap();
        assert_eq!(a.desync(), None);
        a.record_state_hash(30, 0xAAAA);
        assert_eq!(
            a.packet().state_hash,
            None,
            "a frame run before the resync is not hashed again"
        );
    }
}
