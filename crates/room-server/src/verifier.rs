//! Optional headless verifier (docs/22 phase 3, feature `verifier`).
//!
//! Runs `room-game::Headless` on completed TIC_FRAMEs and returns authoritative
//! world checksums for desync detection.

use bytes::Bytes;
use room_decorate::parse_decorate;
use room_game::{GameOptions, Headless, Skill};
use room_map::load_map;
use room_nodebuild::ensure_nodes;
use room_proto::{DukeSyncFrameMsg, NetTicCmd, TicFrameMsg};
use room_sim::{Buttons, ClassRegistry, MAX_PLAYERS, StateTable, TicCmd};
use room_vfs::VfsBuilder;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn};

/// Convert wire cmd → sim cmd.
#[must_use]
pub fn net_to_tic(n: NetTicCmd) -> TicCmd {
    TicCmd {
        buttons: Buttons(n.buttons),
        pitch: n.pitch,
        yaw: n.yaw,
        roll: n.roll,
        forward_move: n.forward_move,
        side_move: n.side_move,
        up_move: n.up_move,
        weapon_select: n.weapon_select,
    }
}

/// Authoritative checksum from the verifier for one tic.
#[derive(Clone, Debug)]
pub struct VerifierChecksum {
    /// Tic.
    pub tic: u32,
    /// World checksum.
    pub checksum: u64,
}

/// Spawn a verifier worker; returns frame sink + checksum source. `game_mod`
/// (`name`, PWAD bytes) is mounted after the IWAD, like every client does.
///
/// # Errors
///
/// IWAD / map / mod load failure.
pub fn spawn_verifier_with_output(
    iwad: PathBuf,
    game_mod: Option<(String, Bytes)>,
    map: &str,
    skill: u8,
    rng_seed: u32,
    max_players: u8,
    time_limit_tics: u32,
) -> Result<(mpsc::Sender<TicFrameMsg>, mpsc::Receiver<VerifierChecksum>), String> {
    let (tx, mut rx) = mpsc::channel::<TicFrameMsg>(256);
    let (out_tx, out_rx) = mpsc::channel::<VerifierChecksum>(64);
    let map = map.to_owned();
    let mut game = load_headless(
        &iwad,
        game_mod,
        &map,
        skill,
        rng_seed,
        max_players,
        time_limit_tics,
    )?;
    info!(map = %map, "verifier Headless started");
    tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Handle::current();
        while let Some(frame) = rt.block_on(rx.recv()) {
            let cmds = frame_to_cmds(&frame, max_players);
            game.tick(&cmds);
            if frame.tic > 0 && frame.tic % 35 == 0 {
                if let Some(checksum) = game.checksum() {
                    if out_tx
                        .try_send(VerifierChecksum {
                            tic: frame.tic,
                            checksum,
                        })
                        .is_err()
                    {
                        warn!("verifier checksum channel full");
                    }
                }
            }
        }
    });
    Ok((tx, out_rx))
}

/// Spawn the native Duke verifier from an Atomic GRP path.
pub fn spawn_duke_verifier_with_output(
    grp_path: PathBuf,
    map: &str,
    skill: u8,
    max_players: u8,
    duke_game_mode: u8,
    duke_friendly_fire: bool,
) -> Result<
    (
        mpsc::Sender<DukeSyncFrameMsg>,
        mpsc::Receiver<VerifierChecksum>,
    ),
    String,
> {
    let grp = std::fs::read(&grp_path).map_err(|e| format!("read Duke GRP: {e}"))?;
    let mut session = room_duke::session_from_grp_map(&grp, map, skill.saturating_sub(1).min(4))?;
    session.configure_multiplayer_mode(
        usize::from(max_players.clamp(2, 8)),
        duke_game_mode,
        duke_friendly_fire,
    )?;
    let (tx, mut rx) = mpsc::channel::<DukeSyncFrameMsg>(256);
    let (out_tx, out_rx) = mpsc::channel::<VerifierChecksum>(64);
    info!(map, grp = %grp_path.display(), "native Duke verifier started");
    tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Handle::current();
        while let Some(frame) = rt.block_on(rx.recv()) {
            let inputs = frame
                .cmds
                .into_iter()
                .map(|cmd| room_duke::DukeInput {
                    avel: cmd.avel,
                    horz: cmd.horz,
                    fvel: cmd.fvel,
                    svel: cmd.svel,
                    bits: cmd.bits,
                })
                .collect::<Vec<_>>();
            session.tick_inputs(&inputs);
            if let Some(transition) = session.take_campaign_exit_request() {
                match transition {
                    room_duke::DukeCampaignTransition::NextLevel { volume, level } => {
                        let next_map = format!("E{}L{}", volume + 1, level + 1);
                        match room_duke::session_from_grp_map(
                            &grp,
                            &next_map,
                            skill.saturating_sub(1).min(4),
                        ) {
                            Ok(mut next) => match next.inherit_campaign_roster(&session) {
                                Ok(()) => session = next,
                                Err(error) => {
                                    warn!(map = %next_map, %error, "Duke verifier could not carry campaign roster");
                                    break;
                                }
                            },
                            Err(error) => {
                                warn!(map = %next_map, %error, "Duke verifier could not load campaign map");
                                break;
                            }
                        }
                    }
                    room_duke::DukeCampaignTransition::EndGame => {
                        info!(tic = frame.tic, "Duke verifier reached campaign ending");
                    }
                }
            }
            if frame.tic > 0 && frame.tic % 30 == 0 {
                if let Some(checksum) = session.checksum() {
                    if out_tx
                        .try_send(VerifierChecksum {
                            tic: frame.tic,
                            checksum,
                        })
                        .is_err()
                    {
                        warn!(tic = frame.tic, "Duke verifier checksum channel full");
                    }
                }
            }
        }
    });
    Ok((tx, out_rx))
}

fn load_headless(
    iwad: &Path,
    game_mod: Option<(String, Bytes)>,
    map: &str,
    skill: u8,
    rng_seed: u32,
    max_players: u8,
    time_limit_tics: u32,
) -> Result<Headless, String> {
    let mut builder = room_build::mount::mount_path(VfsBuilder::new(), iwad)
        .map_err(|e| format!("mount iwad: {e}"))?;
    if let Some((name, bytes)) = game_mod {
        builder = builder
            .mount_memory(format!("{name}.wad"), bytes)
            .map_err(|e| format!("mount mod {name}: {e}"))?;
    }
    let mut vfs = builder.build().map_err(|e| format!("vfs: {e}"))?;
    let mut geometry = load_map(&vfs, map).map_err(|e| format!("load_map: {e}"))?;
    ensure_nodes(&mut geometry);
    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(src).map_err(|e| format!("decorate: {e}"))?;
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).map_err(|e| format!("classes: {e}"))?);
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let skill = Skill::new(skill);
    let mut game = Headless::new(GameOptions {
        skill,
        rng_seed,
        record_demo: false,
        coop_players: max_players.clamp(1, MAX_PLAYERS as u8),
        time_limit_tics,
        ..Default::default()
    });
    game.set_defs(classes, states);
    let (mods, mod_errors) = room_game::discover_mods(&vfs);
    if let Some(e) = mod_errors.first() {
        return Err(format!("mod: {e}"));
    }
    if !mods.is_empty() {
        let names = game
            .set_script_mods_with_vfs(mods, Some(&mut vfs))
            .map_err(|e| format!("mods: {e}"))?;
        info!(mods = %names.join(", "), "verifier mods loaded");
    }
    game.start_custom_map(geometry, skill)
        .map_err(|e| format!("start map: {e}"))?;
    Ok(game)
}

fn frame_to_cmds(frame: &TicFrameMsg, max_players: u8) -> Vec<TicCmd> {
    let n = usize::from(max_players).clamp(1, MAX_PLAYERS);
    let mut cmds = vec![TicCmd::default(); n];
    let mut idx = 0usize;
    for seat in 0..8u8 {
        if frame.seat_mask & (1 << seat) == 0 {
            continue;
        }
        if let Some(net) = frame.cmds.get(idx) {
            if let Some(slot) = cmds.get_mut(usize::from(seat)) {
                *slot = net_to_tic(*net);
            }
        }
        idx += 1;
    }
    cmds
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use room_proto::TicFrameMsg;

    #[test]
    fn net_to_tic_maps_fields() {
        let n = NetTicCmd {
            buttons: 1,
            forward_move: 50,
            weapon_select: 2,
            ..NetTicCmd::default()
        };
        let t = net_to_tic(n);
        assert_eq!(t.buttons.0, 1);
        assert_eq!(t.forward_move, 50);
        assert_eq!(t.weapon_select, 2);
    }

    #[test]
    fn frame_to_cmds_seat_order() {
        let frame = TicFrameMsg {
            tic: 0,
            seat_mask: 0b11,
            cmds: vec![
                NetTicCmd {
                    forward_move: 1,
                    ..NetTicCmd::default()
                },
                NetTicCmd {
                    forward_move: 2,
                    ..NetTicCmd::default()
                },
            ],
        };
        let cmds = frame_to_cmds(&frame, 2);
        assert_eq!(cmds[0].forward_move, 1);
        assert_eq!(cmds[1].forward_move, 2);
    }
}
