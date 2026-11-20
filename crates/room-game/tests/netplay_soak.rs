//! Four-seat lockstep soak (M12): four independent `Headless` games, each
//! with its own [`LockstepSession`], over a simulated network that delays,
//! reorders and drops packets. Every peer reports `World::checksum` every 35
//! tics alongside its commands (as the native app does) and checks the
//! others'; at the end all four worlds must hash the same.
//!
//! Inputs are scripted random play (run, strafe, turn, fire, use, switch
//! weapons), so players open doors, fight, die and respawn.
//!
//! Default length is two minutes of game time so it fits the normal suite;
//! `ROOM_SOAK_TICS=63000` runs the full 30 minutes. `ROOM_SOAK_DROP`
//! (permille, default 50) sets packet loss. Skips without Doom / Freedoom.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_decorate::parse_decorate;
use room_game::{GameOptions, GameState, Headless, Skill};
use room_map::load_map;
use room_net::{LockstepSession, NetError, NetTicCmd, PeerId, TimedCmd, Transport};
use room_nodebuild::ensure_nodes;
use room_sim::{Buttons, ClassRegistry, StateTable, TicCmd};
use room_vfs::VfsBuilder;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

const SEATS: u8 = 4;
const DELAY: u32 = 2;
const CHECKSUM_INTERVAL: u32 = 35;

fn soak_wad_and_map() -> Option<(PathBuf, &'static str)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata");
    let doom1 = root.join("doom/doom1.wad");
    if doom1.is_file() {
        return Some((doom1, "E1M1"));
    }
    let doom2 = root.join("doom/doom2.wad");
    if doom2.is_file() {
        return Some((doom2, "MAP01"));
    }
    let freedoom = root.join("freedoom/freedoom2.wad");
    freedoom.is_file().then_some((freedoom, "MAP01"))
}

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Deterministic LCG.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0 >> 8
    }

    fn below(&mut self, n: u32) -> u32 {
        self.next() % n.max(1)
    }
}

/// Simulated network: per-packet latency of 0..=`jitter` steps (so packets
/// reorder) and `drop_permille` loss.
struct Wire {
    now: u64,
    jitter: u32,
    drop_permille: u32,
    rng: Rng,
    /// `(deliver_at, seq) → (to, from, bytes)`.
    queue: BTreeMap<(u64, u64), (PeerId, PeerId, Vec<u8>)>,
    seq: u64,
    sent: u64,
    dropped: u64,
}

#[derive(Clone)]
struct Endpoint {
    id: PeerId,
    peers: Vec<PeerId>,
    wire: Rc<RefCell<Wire>>,
}

impl Transport for Endpoint {
    fn send(&mut self, peer: PeerId, bytes: &[u8]) {
        if peer == self.id {
            return;
        }
        let mut w = self.wire.borrow_mut();
        w.sent += 1;
        let drop_permille = w.drop_permille;
        if w.rng.below(1000) < drop_permille {
            w.dropped += 1;
            return;
        }
        let jitter = w.jitter;
        let at = w.now + u64::from(w.rng.below(jitter + 1));
        w.seq += 1;
        let seq = w.seq;
        w.queue.insert((at, seq), (peer, self.id, bytes.to_vec()));
    }

    fn poll(&mut self, out: &mut Vec<(PeerId, Vec<u8>)>) {
        let mut w = self.wire.borrow_mut();
        let now = w.now;
        let ready: Vec<(u64, u64)> = w
            .queue
            .iter()
            .filter(|((at, _), (to, _, _))| *at <= now && *to == self.id)
            .map(|(k, _)| *k)
            .collect();
        for k in ready {
            if let Some((_, from, bytes)) = w.queue.remove(&k) {
                out.push((from, bytes));
            }
        }
    }

    fn peers(&self) -> &[PeerId] {
        &self.peers
    }
}

struct Peer {
    game: Headless,
    session: LockstepSession,
    net: Endpoint,
    rng: Rng,
    /// Checksum sent with the command now waiting for its frame.
    waiting: Option<Option<(u32, u64)>>,
    stalled_steps: u32,
    /// Current scripted intent, held for a few tics like a human would.
    intent: NetTicCmd,
    intent_tics: u32,
    /// Rollback mode: checksum of the confirmed world to send with the next
    /// command.
    pending_checksum: Option<(u32, u64)>,
    /// Rollback mode: released frames by tic (recent ones).
    released: BTreeMap<u32, Vec<TicCmd>>,
    /// Rollback mode: speculations to check once confirmed —
    /// `(first tic, frames guessed, checksum of the copy)`.
    guesses: Vec<(u32, Vec<Vec<TicCmd>>, u64)>,
    stats: RollbackStats,
}

/// What rollback play did during a soak.
#[derive(Clone, Copy, Debug, Default)]
struct RollbackStats {
    /// Speculations whose guesses all came true: the copy matched the real
    /// game exactly.
    exact: u32,
    /// Speculations with a wrong guess (thrown away, as a rollback would).
    mispredicted: u32,
    /// Local tics that had to wait for the window.
    waits: u32,
    /// Most tics the local commands ran ahead of the confirmed game.
    max_lead: u32,
}

/// Rollback window: local commands run at most this far ahead.
const ROLLBACK_WINDOW: u32 = 8;

fn net_to_tic(net: NetTicCmd) -> TicCmd {
    TicCmd {
        buttons: Buttons(net.buttons),
        pitch: net.pitch,
        yaw: net.yaw,
        roll: net.roll,
        forward_move: net.forward_move,
        side_move: net.side_move,
        up_move: net.up_move,
        weapon_select: net.weapon_select,
    }
}

fn frame_cmds(frame: &BTreeMap<PeerId, NetTicCmd>) -> Vec<TicCmd> {
    let mut cmds = vec![TicCmd::default(); usize::from(SEATS)];
    for (peer, &net) in frame {
        cmds[usize::from(peer.0)] = net_to_tic(net);
    }
    cmds
}

fn random_intent(rng: &mut Rng) -> NetTicCmd {
    let mut buttons = 0u32;
    if rng.below(3) == 0 {
        buttons |= Buttons::ATTACK;
    }
    if rng.below(6) == 0 {
        buttons |= Buttons::USE;
    }
    let weapon_select = if rng.below(10) == 0 {
        u8::try_from(1 + rng.below(7)).unwrap_or(1)
    } else {
        0
    };
    let speed = if rng.below(2) == 0 { 50 } else { 25 };
    let sign = |r: &mut Rng| if r.below(2) == 0 { 1 } else { -1 };
    NetTicCmd {
        buttons,
        pitch: 0,
        yaw: i16::try_from(rng.below(1200)).unwrap_or(0) - 600,
        roll: 0,
        forward_move: if rng.below(5) == 0 { 0 } else { speed },
        side_move: if rng.below(3) == 0 {
            speed * sign(rng)
        } else {
            0
        },
        up_move: 0,
        weapon_select,
    }
}

impl Peer {
    fn next_input(&mut self) -> NetTicCmd {
        if self.intent_tics == 0 {
            self.intent = random_intent(&mut self.rng);
            self.intent_tics = 4 + self.rng.below(30);
        }
        self.intent_tics -= 1;
        let mut cmd = self.intent;
        // Weapon switches and use are one-shot presses.
        self.intent.weapon_select = 0;
        if self.rng.below(2) == 0 {
            self.intent.buttons &= !Buttons::USE;
        }
        if self.game.state == GameState::Intermission {
            // Everyone votes ready (attack/use) so the next map starts.
            cmd.buttons |= Buttons::USE;
        }
        cmd
    }

    /// One scheduler slice: submit a command when none is pending, then try
    /// to release a frame. Returns the desync tic if one was found.
    fn step(&mut self) -> Result<bool, u32> {
        if self.waiting.is_none() {
            let tic = self.session.next_tic();
            let checksum = (tic % CHECKSUM_INTERVAL == 0)
                .then(|| self.game.checksum())
                .flatten()
                .map(|value| (tic, value));
            let cmd = self.next_input();
            let msg = self.session.submit_local(cmd);
            if let Some((tic, value)) = checksum {
                self.session
                    .report_checksum(tic, self.session.local(), value);
            }
            self.session
                .broadcast_with_checksum(&mut self.net, &msg, checksum);
            self.waiting = Some(checksum);
            self.stalled_steps = 0;
        }
        self.session.recv(&mut self.net);
        let Some(frame) = self.session.try_advance() else {
            self.stalled_steps += 1;
            if self.stalled_steps % 4 == 0 {
                let checksum = self.waiting.flatten();
                self.session.rebroadcast_recent(&mut self.net, checksum);
            }
            assert!(
                self.stalled_steps < 20_000,
                "seat {} stalled at tic {}",
                self.session.local().0,
                self.session.next_tic()
            );
            return Ok(false);
        };
        match self.session.check_complete_desyncs() {
            Err(NetError::Desync { tic }) => return Err(tic),
            Err(e) => panic!("{e}"),
            Ok(()) => {}
        }
        let mut cmds = vec![TicCmd::default(); usize::from(SEATS)];
        for (peer, net) in frame {
            cmds[usize::from(peer.0)] = TicCmd {
                buttons: Buttons(net.buttons),
                pitch: net.pitch,
                yaw: net.yaw,
                roll: net.roll,
                forward_move: net.forward_move,
                side_move: net.side_move,
                up_move: net.up_move,
                weapon_select: net.weapon_select,
            };
        }
        self.game.tick(&cmds);
        self.waiting = None;
        Ok(true)
    }
}

impl Peer {
    /// Rollback mode, one local tic: release whatever frames are complete
    /// (never waiting for them), send this tic's command unless the window
    /// is full, and now and then run a speculative copy ahead with guessed
    /// commands, to check it against the real game later.
    fn step_rollback(&mut self, step: u64, limit: u32) -> Result<(), u32> {
        self.session.recv(&mut self.net);
        // Stop at `limit` so every peer ends on the same tic.
        while self.session.next_tic() < limit
            && let Some(frame) = self.session.try_advance()
        {
            match self.session.check_complete_desyncs() {
                Err(NetError::Desync { tic }) => return Err(tic),
                Err(e) => panic!("{e}"),
                Ok(()) => {}
            }
            let tic = self.session.next_tic() - 1;
            let cmds = frame_cmds(&frame);
            self.game.tick(&cmds);
            self.released.insert(tic, cmds);
            while self.released.len() > 256 {
                self.released.pop_first();
            }
            let next = self.session.next_tic();
            if next % CHECKSUM_INTERVAL == 0
                && let Some(value) = self.game.checksum()
            {
                self.pending_checksum = Some((next, value));
            }
            self.settle_guesses();
        }
        let lead = self.session.local_lead();
        self.stats.max_lead = self.stats.max_lead.max(lead);
        if lead < ROLLBACK_WINDOW {
            let cmd = self.next_input();
            let msg = self.session.submit_local(cmd);
            let checksum = self.pending_checksum.take();
            if let Some((tic, value)) = checksum {
                self.session
                    .report_checksum(tic, self.session.local(), value);
            }
            self.session
                .broadcast_with_checksum(&mut self.net, &msg, checksum);
            self.stalled_steps = 0;
        } else {
            // Window full: wait (and resend, in case ours were lost).
            self.stats.waits += 1;
            self.stalled_steps += 1;
            if self.stalled_steps % 4 == 0 {
                self.session.rebroadcast_recent(&mut self.net, None);
            }
            assert!(
                self.stalled_steps < 20_000,
                "seat {} stalled at tic {}",
                self.session.local().0,
                self.session.next_tic()
            );
        }
        if step % 3 == 0 {
            let frames: Vec<Vec<TicCmd>> = self
                .session
                .speculative_frames()
                .iter()
                .map(|(frame, _)| frame_cmds(frame))
                .collect();
            if !frames.is_empty()
                && let Some(copy) = self.game.speculate(&frames)
            {
                let sum = copy.checksum();
                self.guesses.push((self.session.next_tic(), frames, sum));
            }
        }
        Ok(())
    }

    /// Compare finished speculations with what really happened.
    fn settle_guesses(&mut self) {
        let next = self.session.next_tic();
        let game_sum = self.game.checksum();
        let released = &self.released;
        let stats = &mut self.stats;
        self.guesses.retain(|(start, frames, sum)| {
            let end = start + u32::try_from(frames.len()).unwrap_or(0);
            if end != next {
                return end > next;
            }
            let right = frames
                .iter()
                .enumerate()
                .all(|(k, f)| released.get(&(start + k as u32)) == Some(f));
            if right {
                assert_eq!(
                    Some(*sum),
                    game_sum,
                    "a speculation with every guess right must match the game (tics {start}..{end})"
                );
                stats.exact += 1;
            } else {
                stats.mispredicted += 1;
            }
            false
        });
    }
}

fn defs() -> (Arc<ClassRegistry>, Arc<StateTable>) {
    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(src).expect("decorate");
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).expect("classes"));
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    (classes, states)
}

struct SoakReport {
    rollback: RollbackStats,
    tics: u32,
    verified: u32,
    sent: u64,
    dropped: u64,
    deaths: u32,
}

fn run_soak(
    deathmatch: bool,
    bots: bool,
    tics: u32,
    drop_permille: u32,
    sabotage_at: Option<u32>,
    rollback: bool,
) -> Option<SoakReport> {
    let (wad, map_name) = soak_wad_and_map()?;
    let (classes, states) = defs();
    let peers: Vec<PeerId> = (0..SEATS).map(PeerId).collect();
    let wire = Rc::new(RefCell::new(Wire {
        now: 0,
        jitter: 6,
        drop_permille,
        rng: Rng(0xC0FF_EE11),
        queue: BTreeMap::new(),
        seq: 0,
        sent: 0,
        dropped: 0,
    }));
    let seed = 0x5EED_0042;
    let mut all: Vec<Peer> = peers
        .iter()
        .map(|&id| {
            let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
            let mut map = load_map(&vfs, map_name).unwrap();
            ensure_nodes(&mut map);
            let mut game = Headless::new(GameOptions {
                skill: Skill::new(3),
                rng_seed: seed,
                coop_players: SEATS,
                deathmatch,
                // Seats 2 and 3 are bots: their commands come from the sim,
                // so this also checks bot brains stay in lockstep.
                fill_bots: bots,
                bot_first_seat: if bots { 2 } else { 0 },
                ..Default::default()
            });
            game.set_defs(Arc::clone(&classes), Arc::clone(&states));
            game.start_custom_map(map, Skill::new(3)).unwrap();
            let mut session = LockstepSession::new(id, peers.clone(), DELAY);
            if rollback {
                session.set_rollback_window(ROLLBACK_WINDOW);
            }
            for tic in 0..DELAY {
                for &p in &peers {
                    session.push_remote(TimedCmd {
                        tic,
                        peer: p,
                        cmd: NetTicCmd::default(),
                    });
                }
            }
            Peer {
                game,
                session,
                net: Endpoint {
                    id,
                    peers: peers.clone(),
                    wire: Rc::clone(&wire),
                },
                rng: Rng(0xA11CE ^ (u32::from(id.0) * 7919)),
                waiting: None,
                stalled_steps: 0,
                intent: NetTicCmd::default(),
                intent_tics: 0,
                pending_checksum: None,
                released: BTreeMap::new(),
                guesses: Vec::new(),
                stats: RollbackStats::default(),
            }
        })
        .collect();

    let mut order = Rng(0xBEEF);
    let mut deaths = 0u32;
    let mut was_dead = [false; SEATS as usize];
    let mut sabotaged = false;
    let mut step_no = 0u64;
    while all.iter().any(|p| p.session.next_tic() < tics) {
        step_no += 1;
        // Peers run at uneven speeds: each slice skips some of them.
        for i in 0..all.len() {
            if all[i].session.next_tic() >= tics || order.below(5) == 0 {
                continue;
            }
            let result = if rollback {
                all[i].step_rollback(step_no, tics)
            } else {
                all[i].step().map(|_| ())
            };
            if let Err(tic) = result {
                panic!(
                    "desync at tic {tic} (seat {} noticed; deathmatch={deathmatch} bots={bots})",
                    i
                );
            }
        }
        wire.borrow_mut().now += 1;
        // Desync drill: quietly change one peer's world.
        if sabotage_at.is_some_and(|t| all[3].session.next_tic() == t)
            && let Some(w) = all[3].game.world_mut()
            && let Some(mo) = w.players[0].as_ref().and_then(|p| p.mo)
            && let Some(a) = w.actors.get_mut(mo)
            && !sabotaged
        {
            a.health += 1;
            sabotaged = true;
        }
        // Count deaths on seat 0's world (all worlds agree).
        if let Some(w) = all[0].game.world() {
            for (seat, dead) in was_dead.iter_mut().enumerate() {
                let now_dead = w.players[seat]
                    .as_ref()
                    .is_some_and(|p| p.player_state == room_sim::PlayerState::Dead);
                if now_dead && !*dead {
                    deaths += 1;
                }
                *dead = now_dead;
            }
        }
    }

    let sums: Vec<Option<u64>> = all.iter().map(|p| p.game.checksum()).collect();
    assert!(
        sums.windows(2).all(|w| w[0] == w[1]),
        "final worlds differ: {sums:x?}"
    );
    let verified = all
        .iter()
        .map(|p| p.session.verified_checksums())
        .min()
        .unwrap_or(0);
    let mut stats = RollbackStats::default();
    for p in &all {
        stats.exact += p.stats.exact;
        stats.mispredicted += p.stats.mispredicted;
        stats.waits += p.stats.waits;
        stats.max_lead = stats.max_lead.max(p.stats.max_lead);
    }
    let w = wire.borrow();
    Some(SoakReport {
        rollback: stats,
        tics,
        verified,
        sent: w.sent,
        dropped: w.dropped,
        deaths,
    })
}

fn check(name: &str, deathmatch: bool, bots: bool) {
    check_mode(name, deathmatch, bots, false);
}

fn check_mode(name: &str, deathmatch: bool, bots: bool, rollback: bool) {
    let tics = env_u32("ROOM_SOAK_TICS", 35 * 120);
    let drop = env_u32("ROOM_SOAK_DROP", 50);
    let Some(r) = run_soak(deathmatch, bots, tics, drop, None, rollback) else {
        eprintln!("skip {name}: no Doom / Freedoom WAD");
        return;
    };
    eprintln!(
        "{name}: {} tics ({:.1} min), {} checksums verified per seat, {} / {} packets dropped, {} deaths",
        r.tics,
        f64::from(r.tics) / 35.0 / 60.0,
        r.verified,
        r.dropped,
        r.sent,
        r.deaths
    );
    if rollback {
        let s = r.rollback;
        eprintln!(
            "{name}: rollback: {} speculations matched the game exactly, {} had a wrong guess, {} local tics waited for the window, lead up to {} tics",
            s.exact, s.mispredicted, s.waits, s.max_lead
        );
        assert!(s.exact > 100, "speculation was checked: {s:?}");
        assert!(s.max_lead <= ROLLBACK_WINDOW);
    }
    // Every interval except the last (still in flight at the end) checks.
    assert!(
        r.verified + 2 >= r.tics / CHECKSUM_INTERVAL,
        "only {} of {} checksum intervals verified",
        r.verified,
        r.tics / CHECKSUM_INTERVAL
    );
    if drop > 0 {
        assert!(r.dropped > 0, "the lossy wire dropped nothing");
    }
}

#[test]
fn four_seat_coop_soak_over_a_lossy_network() {
    check("coop", false, false);
}

#[test]
fn four_seat_deathmatch_soak_with_two_bots() {
    check("deathmatch+bots", true, true);
}

#[test]
fn a_peer_whose_world_drifts_is_caught_within_a_second() {
    let Some(_) = soak_wad_and_map() else {
        eprintln!("skip: no Doom / Doom II / Freedoom WAD");
        return;
    };
    let caught = std::panic::catch_unwind(|| run_soak(false, false, 700, 50, Some(300), false));
    let message = caught
        .err()
        .and_then(|e| e.downcast_ref::<String>().cloned())
        .expect("the drift must be reported");
    let tic: u32 = message
        .strip_prefix("desync at tic ")
        .and_then(|rest| rest.split(' ').next())
        .and_then(|t| t.parse().ok())
        .unwrap_or_else(|| panic!("unexpected failure: {message}"));
    assert!(
        (300..=300 + 2 * CHECKSUM_INTERVAL).contains(&tic),
        "{message}"
    );
}

#[test]
fn four_seat_deathmatch_rollback_soak_with_two_bots() {
    check_mode("rollback deathmatch+bots", true, true, true);
}

#[test]
fn four_seat_coop_rollback_soak() {
    check_mode("rollback coop", false, false, true);
}
