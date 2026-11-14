//! Wake sleeping actors — Chocolate `movefta` (STAT 2 → STAT 1).
//!
//! Finds the nearest player, accumulates `timetosleep`, and on `cansee`
//! promotes the sprite to [`STAT_ACTOR`]. Atomic barrels, cans, fire vases,
//! wooden horses and tripbombs instead enter [`STAT_STANDABLE`] (stat 6).

use room_build_engine::geom::{SectorZ, WallXY, cansee, updatesector};
use room_build_engine::numeric::krand_step;
use room_build_engine::types::{SectorT, SpriteT, WallT};
use room_build_engine::world::{MAXSPRITES, SpriteLists};

use crate::badguy::badguy_pic;
use crate::con::ActorConTemps;
use crate::player::Player;
use crate::stat::{STAT_ACTOR, STAT_DUMMY, STAT_STANDABLE};

/// Chocolate's `SLEEPTIME` (`24 * 64`) assigned by successful actor wakeups.
pub(crate) const SLEEPTIME: i16 = 24 * 64;

// Atomic tile IDs from Chocolate Duke's `movefta` classification switch.
const FTA_STANDABLE_PICS: &[i16] = &[
    904,  // WOODENHORSE
    1026, // HORSEONSIDE
    1062, // RUBBERCAN
    1227, // NUKEBARREL
    1228, // NUKEBARRELDENTED
    1229, // NUKEBARRELLEAKED
    1232, // CANWITHSOMETHING
    1238, // EXPLODINGBARREL
    1240, // FIREBARREL
    1390, // FIREVASE
    2566, // TRIPBOMB
    4580, // CANWITHSOMETHING2
    4581, // CANWITHSOMETHING3
    4582, // CANWITHSOMETHING4
];

/// Queue Chocolate `check_fta_sounds` cues when a regular enemy wakes.
fn queue_wake_sounds(pic: i16, palette: u8, health: i16, temps: &mut ActorConTemps) {
    if health <= 0 {
        return;
    }
    let actor_sound = match pic {
        1680 | 1681 | 1715 | 1725 | 1741 | 1742 | 1744 => Some(111), // PRED_RECOG
        2120 | 2150 | 2160 | 2165 => Some(117),                      // CAPT_RECOG
        2000 | 2045 => Some(121),                                    // PIG_RECOG
        1880 => Some(126),                                           // RECO_RECOG
        1960 => Some(131),                                           // DRON_RECOG
        1920 | 1921 => Some(136),                                    // COMM_RECOG
        2420 => Some(146),                                           // TURR_RECOG
        1820 | 1821 => Some(141),                                    // OCTA_RECOG
        2370 => Some(26),                                            // SLIM_RECOG
        _ => None,
    };
    if let Some(sound) = actor_sound {
        temps.pending_actor_sounds.push(sound);
        return;
    }
    match pic {
        2630 => temps.pending_global_sounds.push(97), // BOSS1_RECOG
        2710 => temps
            .pending_global_sounds
            .push(if palette == 1 { 102 } else { 294 }),
        2760 => temps
            .pending_global_sounds
            .push(if palette == 1 { 151 } else { 284 }),
        4740 | 4741 => {
            if palette == 1 {
                temps.pending_global_sounds.push(342); // BOS4_RECOG
            }
            temps.pending_global_sounds.push(371); // BOSS4_FIRSTSEE
        }
        _ => {}
    }
}

fn klabs(x: i32) -> i32 {
    if x == i32::MIN { i32::MIN } else { x.abs() }
}

/// Chocolate `findplayer` distance (SP): manhattan on `opos` + Z/16.
pub fn findplayer_dist(s: &SpriteT, players: &[Player]) -> (usize, i32) {
    let mut best_p = 0usize;
    let mut best = i32::MAX;
    for (pi, p) in players.iter().enumerate() {
        if p.sprite_i < 0 {
            continue;
        }
        let d =
            klabs(p.oposx - s.x) + klabs(p.oposy - s.y) + (klabs(p.oposz - s.z + (28 << 8)) >> 4);
        if d < best {
            best = d;
            best_p = pi;
        }
    }
    (best_p, best)
}

/// Chocolate `movefta`: wake STAT_DUMMY actors that can see a living player.
pub fn movefta(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    players: &[Player],
    randomseed: &mut i32,
    temps: &mut Vec<ActorConTemps>,
    actortype: &[u8],
) {
    let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let sec_walls: Vec<room_build_engine::geom::SectorWalls> =
        sec_z.iter().map(|s| s.walls()).collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();

    let mut i = lists.headspritestat[STAT_DUMMY as usize];
    while i >= 0 {
        let next = lists.nextspritestat[i as usize];
        let iu = i as usize;
        if iu >= MAXSPRITES || iu >= sprites.len() {
            i = next;
            continue;
        }
        let s = sprites[iu];
        let (pidx, x) = findplayer_dist(&s, players);
        let p = match players.get(pidx) {
            Some(p)
                if p.sprite_i >= 0
                    && sprites
                        .get(p.sprite_i as usize)
                        .is_some_and(|body| body.extra > 0) =>
            {
                p
            }
            _ => {
                i = next;
                continue;
            }
        };

        // Chocolate checks the selected player's live body sprite, not a stale
        // cached player health field, before doing wake timers or shade updates.
        if x < 30_000 {
            if temps.len() <= iu {
                temps.resize(iu + 1, ActorConTemps::default());
            }
            temps[iu].timetosleep = temps[iu].timetosleep.saturating_add(1);
            if i32::from(temps[iu].timetosleep) >= (x >> 8) {
                let seen = if badguy_pic(s.picnum, actortype) {
                    // Chocolate: 4×(TRAND&127) XY probes + 2× Z mods.
                    let px = p.oposx + 64 - (krand_step(randomseed) & 127);
                    let py = p.oposy + 64 - (krand_step(randomseed) & 127);
                    let mut psect = p.cursectnum;
                    updatesector(px, py, &mut psect, &sec_walls, &wxy);
                    if psect < 0 {
                        i = next;
                        continue;
                    }
                    let sx = s.x + 64 - (krand_step(randomseed) & 127);
                    let sy = s.y + 64 - (krand_step(randomseed) & 127);
                    // Chocolate passes player XY into the sprite-sector probe (vanilla quirk).
                    let mut ssect = s.sectnum;
                    updatesector(px, py, &mut ssect, &sec_walls, &wxy);
                    if ssect < 0 {
                        i = next;
                        continue;
                    }
                    // `cansee(…, s->z-(TRAND…), …, ps[p].oposz-(TRAND…), …)`: 32-bit gcc
                    // evaluates the arguments right to left, so the player z draws first.
                    let pz = p.oposz - (krand_step(randomseed) as u32 % (32 << 8)) as i32;
                    let sz = s.z - (krand_step(randomseed) as u32 % (52 << 8)) as i32;
                    cansee(
                        sx,
                        sy,
                        sz,
                        s.sectnum,
                        px,
                        py,
                        pz,
                        p.cursectnum,
                        &sec_z,
                        &wxy,
                    )
                } else {
                    // `cansee(…, s->z-(TRAND…), …, ps[p].oposz-(TRAND…), …)`: 32-bit gcc
                    // evaluates the arguments right to left, so the player z draws first.
                    let pz = p.oposz - ((krand_step(randomseed) & 31) << 8);
                    let sz = s.z - ((krand_step(randomseed) & 31) << 8);
                    cansee(
                        s.x,
                        s.y,
                        sz,
                        s.sectnum,
                        p.oposx,
                        p.oposy,
                        pz,
                        p.cursectnum,
                        &sec_z,
                        &wxy,
                    )
                };
                if seen {
                    temps[iu].timetosleep = 0;
                    if FTA_STANDABLE_PICS.contains(&s.picnum) {
                        if let Some(sector) = sectors.get(s.sectnum.max(0) as usize) {
                            sprites[iu].shade = if sector.ceilingstat & 1 != 0 {
                                sector.ceilingshade
                            } else {
                                sector.floorshade
                            };
                        }
                        let _ = lists.changespritestat(i, STAT_STANDABLE);
                    } else {
                        queue_wake_sounds(s.picnum, s.pal, s.extra, &mut temps[iu]);
                        let _ = lists.changespritestat(i, STAT_ACTOR);
                    }
                } else {
                    temps[iu].timetosleep = 0;
                }
            }
        }
        if badguy_pic(s.picnum, actortype)
            && let Some(sector) = sectors.get(s.sectnum.max(0) as usize)
        {
            sprites[iu].shade = if sector.ceilingstat & 1 != 0 {
                sector.ceilingshade
            } else {
                sector.floorshade
            };
        }
        i = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use room_build_engine::types::{SectorT, WallT};
    use room_build_engine::world::SpriteLists;

    fn open_square() -> (Vec<SectorT>, Vec<WallT>) {
        let walls = vec![
            WallT {
                x: 0,
                y: 0,
                point2: 1,
                nextsector: -1,
                ..Default::default()
            },
            WallT {
                x: 4096,
                y: 0,
                point2: 2,
                nextsector: -1,
                ..Default::default()
            },
            WallT {
                x: 4096,
                y: 4096,
                point2: 3,
                nextsector: -1,
                ..Default::default()
            },
            WallT {
                x: 0,
                y: 4096,
                point2: 0,
                nextsector: -1,
                ..Default::default()
            },
        ];
        let sectors = vec![SectorT {
            wallptr: 0,
            wallnum: 4,
            floorz: 0,
            ceilingz: -(64 << 8),
            ..Default::default()
        }];
        (sectors, walls)
    }

    #[test]
    fn wakes_dummy_when_player_near_and_visible() {
        let (sectors, walls) = open_square();
        let mut lists = SpriteLists::new();
        assert!(lists.place_sprite_at(0, 0, STAT_DUMMY));
        let mut sprites = vec![SpriteT::default(); 8];
        sprites[0] = SpriteT {
            x: 1000,
            y: 1000,
            z: -(32 << 8),
            sectnum: 0,
            picnum: 2000,
            ..Default::default()
        };
        sprites[1].extra = 100;
        let players = [Player {
            posx: 1100,
            posy: 1100,
            posz: -(32 << 8),
            oposx: 1100,
            oposy: 1100,
            oposz: -(32 << 8),
            cursectnum: 0,
            sprite_i: 1,
            ..Default::default()
        }];
        let mut seed = 1;
        let mut temps = vec![ActorConTemps {
            timetosleep: 200,
            ..Default::default()
        }];
        // Non-badguy path (empty actortype, pic not in table → use 910 tree? use 0)
        sprites[0].picnum = 0;
        movefta(
            &mut lists,
            &mut sprites,
            &sectors,
            &walls,
            &players,
            &mut seed,
            &mut temps,
            &[],
        );
        assert_eq!(lists.sprite_stat[0], STAT_ACTOR);
    }
}
