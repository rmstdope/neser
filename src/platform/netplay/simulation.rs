//! A test bench for lockstep: a simulated channel with latency, jitter and loss, and a
//! driver that plays two [`Lockstep`] sessions against each other over it.
//!
//! Everything is driven by seeded pseudo-random numbers and simulated time, so every run
//! is reproducible.

use super::lockstep::{Desync, FrameInputs, Lockstep, LockstepConfig, Player};
use crate::platform::crc32::crc32;
use crate::platform::determinism::{loaded_console, play_frame_with_pads, read_rom};
use crate::platform::emulator::{Console, SystemType};

/// Simulated milliseconds per tick: the two sides run at 60 ticks a second.
const MS_PER_TICK: f64 = 1000.0 / 60.0;

/// One direction of a simulated link.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SimLink {
    /// The least time a packet takes, in milliseconds.
    pub latency_ms: f64,
    /// Up to this much more, uniformly; packets sent close together can overtake each other.
    pub jitter_ms: f64,
    /// The share of packets lost, 0.0 to 1.0.
    pub loss: f64,
    pub seed: u64,
}

/// splitmix64: small, fast and good enough to shuffle a simulated network.
struct SplitMix(u64);

impl SplitMix {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut x = self.0;
        x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        x ^ (x >> 31)
    }

    /// Uniform in `[0, 1)`.
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Packets in flight in one direction.
pub(crate) struct SimChannel {
    link: SimLink,
    rng: SplitMix,
    /// `(deliver_at_ms, sequence, bytes)`; the sequence keeps equal times in send order.
    in_flight: Vec<(f64, u64, Vec<u8>)>,
    sent: u64,
    drop_next: bool,
}

impl SimChannel {
    pub(crate) fn new(link: SimLink) -> Self {
        SimChannel {
            link,
            rng: SplitMix(link.seed),
            in_flight: Vec::new(),
            sent: 0,
            drop_next: false,
        }
    }

    /// Put `bytes` on the link at `now_ms`; it may be lost.
    pub(crate) fn send(&mut self, now_ms: f64, bytes: Vec<u8>) {
        // Draw both numbers for every packet, so a forced drop does not shift the
        // randomness of the packets after it.
        let lost = self.rng.next_f64() < self.link.loss;
        let delay = self.link.latency_ms + self.rng.next_f64() * self.link.jitter_ms;
        if lost || std::mem::take(&mut self.drop_next) {
            return;
        }
        self.sent += 1;
        self.in_flight.push((now_ms + delay, self.sent, bytes));
    }

    /// The next packet sent is lost, whatever the link's loss rate.
    pub(crate) fn force_drop_next(&mut self) {
        self.drop_next = true;
    }

    /// Every packet due by `now_ms`, in arrival order.
    pub(crate) fn deliver(&mut self, now_ms: f64) -> Vec<Vec<u8>> {
        let (mut due, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.in_flight)
            .into_iter()
            .partition(|&(at, _, _)| at <= now_ms);
        self.in_flight = waiting;
        due.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        due.into_iter().map(|(_, _, bytes)| bytes).collect()
    }
}

/// A game the pair driver can run: one frame on both pads, returning its state hash.
pub(crate) trait SimGame {
    fn run_frame(&mut self, pads: [u16; 2]) -> u32;
}

/// A cheap game whose state folds in every pad it was given, so any difference in the
/// inputs either side ran shows up in its hash.
#[derive(Default)]
pub(crate) struct FoldGame {
    state: u64,
    frames: u32,
    /// After running this frame, the state is corrupted: an injected desync.
    pub corrupt_after: Option<u32>,
}

impl SimGame for FoldGame {
    fn run_frame(&mut self, pads: [u16; 2]) -> u32 {
        self.state = self
            .state
            .wrapping_mul(0x100_0000_01B3)
            .wrapping_add(u64::from(pads[0]) << 16 | u64::from(pads[1]));
        if self.corrupt_after == Some(self.frames) {
            self.state ^= 1;
        }
        self.frames += 1;
        (self.state ^ (self.state >> 32)) as u32
    }
}

/// A real console as a [`SimGame`]: its hash is the CRC-32 of its save state.
pub(crate) struct ConsoleGame {
    console: Console,
    system: SystemType,
    name: String,
    frames: u32,
    /// The state hash after every frame run, in order.
    pub hashes: Vec<u32>,
    /// On this frame player 2's first button is flipped on this console alone, bypassing
    /// lockstep: an injected desync.
    pub extra_press_on: Option<u32>,
}

impl ConsoleGame {
    pub(crate) fn new(system: SystemType, rom_path: &str) -> Self {
        let rom = read_rom(rom_path);
        ConsoleGame {
            console: loaded_console(system, &rom, rom_path),
            system,
            name: rom_path.to_owned(),
            frames: 0,
            hashes: Vec::new(),
            extra_press_on: None,
        }
    }
}

impl SimGame for ConsoleGame {
    fn run_frame(&mut self, mut pads: [u16; 2]) -> u32 {
        if self.extra_press_on == Some(self.frames) {
            pads[1] ^= 1;
        }
        let label = format!("{}, frame {}", self.name, self.frames);
        let output = play_frame_with_pads(&mut self.console, self.system, pads, &label);
        self.frames += 1;
        let hash = crc32(&[&output.state]);
        self.hashes.push(hash);
        hash
    }
}

/// How the console runs are played: the lossy link, a 4-frame delay, hashes every 30 frames.
fn console_pair() -> PairSim {
    PairSim {
        link: SimLink {
            latency_ms: 60.0,
            jitter_ms: 40.0,
            loss: 0.2,
            seed: 0xC0_501E,
        },
        config: LockstepConfig {
            input_delay: 4,
            hash_interval: 30,
        },
        ticks: 180,
        drop_one_each_way_at: None,
    }
}

/// Two consoles of `system` on `rom_path`, in lockstep over a lossy link, run the same
/// frames on the same pads, agree on their whole state after every frame, and report no
/// desync.
pub(crate) fn assert_consoles_stay_in_step(system: SystemType, rom_path: &str) {
    let mut games = [
        ConsoleGame::new(system, rom_path),
        ConsoleGame::new(system, rom_path),
    ];
    let [a_game, b_game] = &mut games;
    let [a, b] = run_pair(&console_pair(), [a_game, b_game]);
    let common = a.ran.len().min(b.ran.len());
    assert!(common >= 60, "{rom_path}: only {common} frames ran");
    assert_eq!(a.ran[..common], b.ran[..common], "{rom_path}: pads differ");
    assert_eq!(
        games[0].hashes[..common],
        games[1].hashes[..common],
        "{rom_path}: states differ"
    );
    assert_eq!((a.desync, b.desync), (None, None), "{rom_path}");
}

/// The same pair, with one extra press on the second console alone at frame 60: both
/// sides report a desync at the first hashed frame from 60 on.
pub(crate) fn assert_injected_console_mismatch_is_detected(system: SystemType, rom_path: &str) {
    let mut a_game = ConsoleGame::new(system, rom_path);
    let mut b_game = ConsoleGame {
        extra_press_on: Some(60),
        ..ConsoleGame::new(system, rom_path)
    };
    let [a, b] = run_pair(&console_pair(), [&mut a_game, &mut b_game]);
    for log in [&a, &b] {
        assert_eq!(
            log.desync.map(|d| d.frame),
            Some(60),
            "{rom_path}: the injected press went unnoticed"
        );
    }
}

/// The pad `player` holds for `frame`: a fixed pseudo-random pattern that changes every
/// 8 frames, different for each player, over 12 buttons.
pub(crate) fn scripted_pad(player: Player, frame: u32) -> u16 {
    let salt = match player {
        Player::One => 0x1111,
        Player::Two => 0x2222,
    };
    (SplitMix(u64::from(frame / 8) << 16 | salt).next_u64() & 0x0FFF) as u16
}

/// How a pair is played.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PairSim {
    /// Both directions use this link, each with its own seed derived from it.
    pub link: SimLink,
    pub config: LockstepConfig,
    pub ticks: u32,
    /// At this tick, one packet each way is lost whatever the link.
    pub drop_one_each_way_at: Option<u32>,
}

/// What one side did during a pair run.
#[derive(Debug, Default)]
pub(crate) struct PlayerLog {
    /// Every frame run, in the order run, with the pads it ran on.
    pub ran: Vec<FrameInputs>,
    /// Ticks on which the frame was held for the peer's pad.
    pub held: u32,
    pub desync: Option<Desync>,
}

/// Play two sessions, player 1 on `games[0]` and player 2 on `games[1]`, over the
/// simulated link. Each tick each side takes in what arrived, offers its scripted pad,
/// runs the frame if both pads are known, records its hash and sends a packet.
pub(crate) fn run_pair(sim: &PairSim, mut games: [&mut dyn SimGame; 2]) -> [PlayerLog; 2] {
    let players = [Player::One, Player::Two];
    let mut sessions = players.map(|p| Lockstep::new(p, sim.config));
    // channels[i] carries packets to player i.
    let mut channels = [0x5151u64, 0xA2A2].map(|salt| {
        SimChannel::new(SimLink {
            seed: sim.link.seed ^ salt,
            ..sim.link
        })
    });
    let mut logs = [PlayerLog::default(), PlayerLog::default()];
    for tick in 0..sim.ticks {
        let now = f64::from(tick) * MS_PER_TICK;
        for (i, game) in games.iter_mut().enumerate() {
            let session = &mut sessions[i];
            for bytes in channels[i].deliver(now) {
                session
                    .receive(&bytes)
                    .unwrap_or_else(|e| panic!("player {} refused a packet: {e:?}", i + 1));
            }
            let pad = scripted_pad(players[i], session.next_frame() + sim.config.input_delay);
            match session.advance(pad) {
                Some(inputs) => {
                    let hash = game.run_frame(inputs.pads);
                    session.record_state_hash(inputs.frame, hash);
                    logs[i].ran.push(inputs);
                }
                None => logs[i].held += 1,
            }
            let to = &mut channels[1 - i];
            if sim.drop_one_each_way_at == Some(tick) {
                to.force_drop_next();
            }
            to.send(now, session.packet().encode());
        }
    }
    for (log, session) in logs.iter_mut().zip(&sessions) {
        log.desync = session.desync();
    }
    logs
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOSSY: SimLink = SimLink {
        latency_ms: 60.0,
        jitter_ms: 40.0,
        loss: 0.2,
        seed: 1,
    };

    #[test]
    fn sim_channel_delivers_within_latency_plus_jitter() {
        let mut channel = SimChannel::new(SimLink { loss: 0.0, ..LOSSY });
        channel.send(0.0, vec![1]);
        assert!(channel.deliver(59.9).is_empty());
        assert_eq!(channel.deliver(100.0), [vec![1]]);
        assert!(channel.deliver(1000.0).is_empty(), "delivered once");
    }

    #[test]
    fn sim_channel_jitter_reorders_packets() {
        let mut channel = SimChannel::new(SimLink { loss: 0.0, ..LOSSY });
        for i in 0..100u8 {
            channel.send(f64::from(i), vec![i]);
        }
        let arrived: Vec<u8> = channel.deliver(1000.0).into_iter().map(|p| p[0]).collect();
        assert_eq!(arrived.len(), 100);
        assert!(arrived.windows(2).any(|w| w[0] > w[1]), "some overtook");
    }

    #[test]
    fn sim_channel_loses_about_the_configured_share() {
        let mut channel = SimChannel::new(LOSSY);
        for _ in 0..10_000 {
            channel.send(0.0, vec![0]);
        }
        let lost = 10_000 - channel.deliver(1000.0).len();
        assert!((1_700..2_300).contains(&lost), "{lost} lost");
    }

    #[test]
    fn sim_channel_force_drop_loses_exactly_the_next_packet() {
        let mut channel = SimChannel::new(SimLink {
            loss: 0.0,
            jitter_ms: 0.0,
            ..LOSSY
        });
        channel.send(0.0, vec![1]);
        channel.force_drop_next();
        channel.send(0.0, vec![2]);
        channel.send(0.0, vec![3]);
        assert_eq!(channel.deliver(60.0), [vec![1], vec![3]]);
    }

    #[test]
    fn sim_channel_is_reproducible_from_its_seed() {
        let run = || {
            let mut channel = SimChannel::new(LOSSY);
            let mut arrivals = Vec::new();
            for i in 0..200u8 {
                channel.send(f64::from(i) * 16.0, vec![i]);
                arrivals.push(channel.deliver(f64::from(i) * 16.0));
            }
            arrivals
        };
        assert_eq!(run(), run());
    }

    const CONFIG: LockstepConfig = LockstepConfig {
        input_delay: 4,
        hash_interval: 30,
    };

    fn lossy_pair(seed: u64, ticks: u32) -> PairSim {
        PairSim {
            link: SimLink { seed, ..LOSSY },
            config: CONFIG,
            ticks,
            drop_one_each_way_at: None,
        }
    }

    /// The pads each frame must run on: what each player's script pressed for it, released
    /// for the opening input-delay frames.
    fn expected_pads(frame: u32) -> [u16; 2] {
        if frame < CONFIG.input_delay {
            [0, 0]
        } else {
            [Player::One, Player::Two].map(|p| scripted_pad(p, frame))
        }
    }

    #[test]
    fn two_players_run_the_same_frames_with_the_same_inputs() {
        for seed in 1..=8 {
            let ticks = 1200;
            let [a, b] = run_pair(
                &lossy_pair(seed, ticks),
                [&mut FoldGame::default(), &mut FoldGame::default()],
            );
            for log in [&a, &b] {
                for (i, inputs) in log.ran.iter().enumerate() {
                    assert_eq!(inputs.frame, i as u32, "seed {seed}: frames run in order");
                    assert_eq!(
                        inputs.pads,
                        expected_pads(inputs.frame),
                        "seed {seed}: frame {} ran on a pad nobody pressed",
                        inputs.frame
                    );
                }
                assert!(
                    log.ran.len() as u32 > ticks / 2,
                    "seed {seed}: only {} of {ticks} frames ran",
                    log.ran.len()
                );
                assert_eq!(log.desync, None, "seed {seed}");
            }
            let common = a.ran.len().min(b.ran.len());
            assert_eq!(a.ran[..common], b.ran[..common], "seed {seed}");
            assert!(
                a.ran.len().abs_diff(b.ran.len()) <= CONFIG.input_delay as usize,
                "seed {seed}: one side ran ahead of lockstep"
            );
        }
    }

    #[test]
    fn an_input_delay_covering_the_latency_keeps_play_at_full_speed() {
        // 8 frames is 133 ms, more than the link's worst 100 ms plus a tick of waiting to
        // be read, so a lost packet's inputs then ride the next one, a tick later, in time.
        let config = LockstepConfig {
            input_delay: 8,
            ..CONFIG
        };
        for seed in 1..=8 {
            let [a, b] = run_pair(
                &PairSim {
                    config,
                    ..lossy_pair(seed, 1200)
                },
                [&mut FoldGame::default(), &mut FoldGame::default()],
            );
            for log in [&a, &b] {
                // Only start-up and runs of losses back to back may still hold a frame.
                assert!(
                    log.held <= 1200 / 50,
                    "seed {seed}: held {} ticks",
                    log.held
                );
            }
            assert_eq!(
                a.ran[..b.ran.len().min(a.ran.len())],
                b.ran[..b.ran.len().min(a.ran.len())]
            );
        }
    }

    #[test]
    fn one_lost_packet_does_not_stall_play() {
        let sim = PairSim {
            link: SimLink {
                latency_ms: MS_PER_TICK,
                jitter_ms: 0.0,
                loss: 0.0,
                seed: 7,
            },
            config: LockstepConfig {
                input_delay: 3,
                hash_interval: 30,
            },
            ticks: 300,
            drop_one_each_way_at: Some(150),
        };
        let [a, b] = run_pair(&sim, [&mut FoldGame::default(), &mut FoldGame::default()]);
        assert_eq!((a.held, b.held), (0, 0));
        assert_eq!(a.ran, b.ran);
    }

    #[test]
    fn an_injected_state_mismatch_is_detected_on_both_sides() {
        let [a, b] = run_pair(
            &lossy_pair(3, 1200),
            [
                &mut FoldGame::default(),
                &mut FoldGame {
                    corrupt_after: Some(300),
                    ..FoldGame::default()
                },
            ],
        );
        assert_eq!(a.desync.map(|d| d.frame), Some(300));
        assert_eq!(b.desync.map(|d| d.frame), Some(300));
    }

    #[test]
    fn without_an_injection_no_mismatch_is_reported() {
        let [a, b] = run_pair(
            &lossy_pair(3, 1200),
            [&mut FoldGame::default(), &mut FoldGame::default()],
        );
        assert_eq!((a.desync, b.desync), (None, None));
    }
}
