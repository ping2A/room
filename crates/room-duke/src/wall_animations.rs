//! Atomic animated wall state (`premap` / `animatewalls`, D7).

use room_build_engine::{
    numeric::{krand_log, krand_step},
    types::WallT,
};

const SCREENBREAKS: &[i16] = &[
    263, 264, 265, 266, 267, 268, 269, 270, 271, 272, 273, 274, 275, 4120, 4123, 4125, 4127, 4128,
    4129,
];
const SCREENBREAK6: i16 = 268;
const SCREENBREAK8: i16 = 270;
const W_FORCEFIELD: i16 = 663;
const W_FORCEFIELD2: i16 = 664;
const W_FORCEFIELD3: i16 = 665;
const BIGFORCE: i16 = 230;
const FEMPIC1: i16 = 1280;
const FEMPIC2: i16 = 1289;
const FEMPIC3: i16 = 1298;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct AnimWall {
    wall: usize,
    tag: i32,
}

impl AnimWall {
    pub(crate) const fn atomic_record(self) -> (usize, i32) {
        (self.wall, self.tag)
    }
}

/// Rebuild Chocolate's saved `animwalltype[512]` prefix without rerunning
/// `premap`, which would reset animation phases and mutate saved walls.
pub(crate) fn restore_anim_walls(
    records: &[u8],
    count: usize,
    numwalls: usize,
) -> Option<Vec<AnimWall>> {
    if records.len() != 512 * 6 || count > 512 {
        return None;
    }
    let mut restored = Vec::with_capacity(count);
    for i in 0..count {
        let record = &records[i * 6..(i + 1) * 6];
        let wall = usize::try_from(i16::from_le_bytes([record[0], record[1]])).ok()?;
        if wall >= numwalls {
            return None;
        }
        let tag = i32::from_le_bytes([record[2], record[3], record[4], record[5]]);
        restored.push(AnimWall { wall, tag });
    }
    Some(restored)
}

/// Chocolate `operateforcefields`: toggle tagged animated force-field walls
/// and reset their animation phase before the next `animatewalls` update.
pub(crate) fn operate_forcefields(walls: &mut [WallT], animwalls: &mut [AnimWall], lotag: i16) {
    for anim in animwalls {
        let Some(wall) = walls.get_mut(anim.wall) else {
            continue;
        };
        if lotag != -1 && wall.lotag != lotag {
            continue;
        }
        if !matches!(
            wall.overpicnum,
            W_FORCEFIELD | W_FORCEFIELD2 | W_FORCEFIELD3 | BIGFORCE
        ) {
            continue;
        }
        anim.tag = 0;
        wall.cstat = if wall.cstat != 0 { 0 } else { 85 };
    }
}

/// Create the ordered wall list and initial tags built by Chocolate `premap`.
pub(crate) fn init_animated_walls(walls: &mut [WallT], mirror_walls: &[usize]) -> Vec<AnimWall> {
    let mut linked_lotags = Vec::new();
    for (wall_index, wall) in walls.iter_mut().enumerate() {
        if mirror_walls.contains(&wall_index) {
            continue;
        }
        if matches!(
            wall.overpicnum,
            W_FORCEFIELD | W_FORCEFIELD2 | W_FORCEFIELD3
        ) {
            if wall.shade > 31 {
                wall.cstat = 0;
            } else {
                wall.cstat |= 85 + 256;
            }
            if wall.lotag != 0 && wall.nextwall >= 0 {
                linked_lotags.push((wall.nextwall as usize, wall.lotag));
            }
        } else if wall.overpicnum != BIGFORCE {
            wall.extra = -1;
            if matches!(wall.picnum, FEMPIC1 | FEMPIC2 | FEMPIC3) {
                wall.extra = wall.picnum;
            }
        }
    }
    for (wall, lotag) in linked_lotags {
        if let Some(peer) = walls.get_mut(wall) {
            peer.lotag = lotag;
        }
    }
    walls
        .iter()
        .enumerate()
        .filter_map(|(i, wall)| {
            if matches!(
                wall.overpicnum,
                W_FORCEFIELD | W_FORCEFIELD2 | W_FORCEFIELD3 | BIGFORCE
            ) {
                return Some(AnimWall { wall: i, tag: 0 });
            }
            if SCREENBREAKS.contains(&wall.picnum) {
                let tag = if (SCREENBREAK6..=SCREENBREAK8).contains(&wall.picnum) {
                    -1
                } else {
                    i32::from(wall.picnum)
                };
                return Some(AnimWall { wall: i, tag });
            }
            if matches!(wall.picnum, FEMPIC1 | FEMPIC2 | FEMPIC3) {
                return Some(AnimWall {
                    wall: i,
                    tag: i32::from(wall.picnum),
                });
            }
            // The Atomic source also puts fans, tech walls and female screens
            // in this list. Their current animation path consumes no RNG.
            let other = matches!(wall.overpicnum, 407 | 412)
                || matches!(wall.picnum, 293 | 297 | 301 | 305);
            other.then_some(AnimWall { wall: i, tag: 0 })
        })
        .collect()
}

/// Advance the vanilla `animatewalls` state once (Chocolate calls this every
/// other game frame). Mutates the Build wall copy and the deterministic RNG.
pub(crate) fn animate_walls(walls: &mut [WallT], animwalls: &mut [AnimWall], seed: &mut i32) {
    for anim in animwalls {
        let Some(wall) = walls.get_mut(anim.wall) else {
            continue;
        };
        if SCREENBREAKS.contains(&wall.picnum) {
            if wall.picnum == SCREENBREAK6 || wall.picnum == 269 || wall.picnum == SCREENBREAK8 {
                if anim.tag >= 0 && wall.extra != FEMPIC2 && wall.extra != FEMPIC3 {
                    wall.picnum = anim.tag as i16;
                } else {
                    wall.picnum += 1;
                    if wall.picnum == SCREENBREAK6 + 3 {
                        wall.picnum = SCREENBREAK6;
                    }
                }
            } else {
                let recording = krand_log::is_recording();
                if recording {
                    krand_log::set_context(Some(format!(
                        "animatewalls screen wall={} pic={} tag={}",
                        anim.wall, wall.picnum, anim.tag
                    )));
                }
                let breaks = krand_step(seed) & 255 < 16;
                if recording {
                    krand_log::set_context(None);
                }
                if breaks {
                    anim.tag = i32::from(wall.picnum);
                    wall.picnum = SCREENBREAK6;
                }
            }
            continue;
        }

        if !matches!(
            wall.overpicnum,
            W_FORCEFIELD | W_FORCEFIELD2 | W_FORCEFIELD3
        ) || wall.cstat & 254 == 0
        {
            continue;
        }

        let t = anim.tag;
        wall.xpanning = wall.xpanning.wrapping_sub((t >> 10) as u8);
        wall.ypanning = wall.ypanning.wrapping_sub((t >> 10) as u8);
        if wall.extra == 1 {
            wall.extra = 0;
            anim.tag = 0;
        } else {
            anim.tag += 128;
        }
        if anim.tag < (128 << 4) {
            wall.overpicnum = if anim.tag & 128 != 0 {
                W_FORCEFIELD
            } else {
                W_FORCEFIELD2
            };
        } else {
            let recording = krand_log::is_recording();
            if recording {
                krand_log::set_context(Some(format!(
                    "animatewalls forcefield wall={} pic={} over={} tag_prev={} tag_next={} cstat={} extra={}",
                    anim.wall, wall.picnum, wall.overpicnum, t, anim.tag, wall.cstat, wall.extra
                )));
            }
            let rephase = krand_step(seed) & 255 < 32;
            if recording {
                krand_log::set_context(None);
            }
            if rephase {
                if recording {
                    krand_log::set_context(Some(format!(
                        "animatewalls forcefield-phase wall={} pic={} over={} tag_prev={} tag_next={} cstat={} extra={}",
                        anim.wall,
                        wall.picnum,
                        wall.overpicnum,
                        t,
                        anim.tag,
                        wall.cstat,
                        wall.extra
                    )));
                }
                let phase = krand_step(seed) & 3;
                if recording {
                    krand_log::set_context(None);
                }
                anim.tag = 128 << phase;
            } else {
                wall.overpicnum = W_FORCEFIELD2;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AnimWall, animate_walls, init_animated_walls};
    use room_build_engine::types::WallT;

    #[test]
    fn forcefields_advance_and_roll_at_the_sixteenth_half_rate_update() {
        let mut walls = [
            WallT {
                overpicnum: 663,
                cstat: 81,
                extra: -1,
                ..Default::default()
            },
            WallT {
                overpicnum: 663,
                cstat: 24,
                extra: -1,
                ..Default::default()
            },
            WallT {
                overpicnum: 663,
                cstat: 89,
                extra: -1,
                ..Default::default()
            },
            WallT {
                overpicnum: 663,
                cstat: 81,
                extra: -1,
                ..Default::default()
            },
        ];
        let mut anim = init_animated_walls(&mut walls, &[]);
        assert_eq!(
            anim,
            (0..4)
                .map(|wall| AnimWall { wall, tag: 0 })
                .collect::<Vec<_>>()
        );
        let mut seed = -1_781_826_469;
        for _ in 0..15 {
            animate_walls(&mut walls, &mut anim, &mut seed);
        }
        assert_eq!(seed, -1_781_826_469);
        animate_walls(&mut walls, &mut anim, &mut seed);
        assert_eq!(seed, -2_125_186_996);
        assert_eq!(
            anim.iter().map(|wall| wall.tag).collect::<Vec<_>>(),
            [2048, 2048, 2048, 128]
        );
        assert_eq!(
            walls.iter().map(|wall| wall.overpicnum).collect::<Vec<_>>(),
            [664, 664, 664, 663]
        );
    }

    #[test]
    fn screens_do_not_roll_until_their_animation_step() {
        let mut walls = [WallT {
            picnum: 263,
            ..Default::default()
        }];
        let mut anim = init_animated_walls(&mut walls, &[]);
        let mut seed = 17;
        // With this seed the first test is false, so the screen stays intact.
        animate_walls(&mut walls, &mut anim, &mut seed);
        assert_eq!(walls[0].picnum, 263);
        assert_ne!(seed, 17);
    }
}
