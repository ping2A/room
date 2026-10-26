//! Wake sleeping actors — Chocolate `movefta` (STAT 2 → STAT 1).
//!
//! Finds the nearest player, accumulates `timetosleep`, and on `cansee`
//! promotes the sprite to [`STAT_ACTOR`]. Barrel-style picnums go to
//! [`STAT_STANDABLE`] (stat 6); those picnums are stubbed empty for now.

use room_build_engine::geom::{SectorZ, WallXY, cansee, updatesector};
use room_build_engine::numeric::krand_step;
use room_build_engine::types::{SectorT, SpriteT, WallT};
use room_build_engine::world::{MAXSPRITES, SpriteLists};

use crate::badguy::badguy_pic;
use crate::con::ActorConTemps;
use crate::player::Player;
use crate::stat::{STAT_ACTOR, STAT_DUMMY};


fn klabs(x: i32) -> i32 {
    if x == i32::MIN {
        i32::MIN
    } else {
        x.abs()
    }
}

/// Chocolate `findplayer` distance (SP): manhattan on `opos` + Z/16.
pub fn findplayer_dist(s: &SpriteT, players: &[Player]) -> (usize, i32) {
    let mut best_p = 0usize;
    let mut best = i32::MAX;
    for (pi, p) in players.iter().enumerate() {
        if p.sprite_i < 0 {
            continue;
        }
        let d = klabs(p.oposx - s.x)
            + klabs(p.oposy - s.y)
            + (klabs(p.oposz - s.z + (28 << 8)) >> 4);
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
        let (_pidx, x) = findplayer_dist(&s, players);
        let p = match players.first() {
            Some(p) if p.sprite_i >= 0 => p,
            _ => {
                i = next;
                continue;
            }
        };

        // Living player: health lives on sprite.extra later; for now always awake-capable.
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
                    let sz = s.z - (krand_step(randomseed) as u32 % (52 << 8)) as i32;
                    let pz = p.oposz - (krand_step(randomseed) as u32 % (32 << 8)) as i32;
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
                    let sz = s.z - ((krand_step(randomseed) & 31) << 8);
                    let pz = p.oposz - ((krand_step(randomseed) & 31) << 8);
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
                    let _ = lists.changespritestat(i, STAT_ACTOR);
                } else {
                    temps[iu].timetosleep = 0;
                }
            }
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
