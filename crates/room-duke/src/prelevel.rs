//! Enter-level sprite pass — partial Chocolate `premap.c` / `prelevel` (D5).
//!
//! Deletes marker sprites (`GPSPEED`, `CYCLER`) by walking `headspritestat[0]`
//! (same order as Chocolate, so the free-list head after deletes matches for
//! the later player `EGS`). Map `APLAYER` bodies are soft-culled in spawn;
//! the live player is inserted by [`crate::spawn::spawn_player_body`].

use room_build_engine::types::{SectorT, SpriteT};
use room_build_engine::world::{BuildWorld, MAXSTATUS, SpriteLists};

use crate::stat::STAT_DEFAULT;

/// `names.h` — sector glide speed marker (consumed into `sector.extra`).
pub const GPSPEED: i16 = 10;
/// `names.h` — shade cycler marker.
pub const CYCLER: i16 = 7;
/// Atomic `names.h` mirror tile, overridden by the active CON `define`.
pub(crate) const MIRROR: i16 = 560;

/// Initialize Chocolate's `premap` mirror table and mark the back side.
///
/// Returns the source mirror wall indices in map order. Their wall records are
/// excluded from `animwall`; the destination sectors have MIRROR flat/ceiling
/// tiles and every destination wall has MIRROR on both texture planes.
pub(crate) fn init_mirrors(world: &mut BuildWorld, mirror_tile: i16) -> Vec<usize> {
    let numwalls = usize::try_from(world.numwalls)
        .unwrap_or(0)
        .min(world.walls.len());
    let numsectors = usize::try_from(world.numsectors)
        .unwrap_or(0)
        .min(world.sectors.len());
    let mut mirror_walls = Vec::new();
    let mut mirror_sectors = Vec::new();
    for wall_index in 0..numwalls {
        let wall = world.walls[wall_index];
        if wall.overpicnum != mirror_tile || wall.cstat & 32 == 0 {
            continue;
        }
        let Ok(sector_index) = usize::try_from(wall.nextsector) else {
            continue;
        };
        if sector_index >= numsectors || world.sectors[sector_index].ceilingpicnum == mirror_tile {
            continue;
        }
        world.sectors[sector_index].ceilingpicnum = mirror_tile;
        world.sectors[sector_index].floorpicnum = mirror_tile;
        mirror_walls.push(wall_index);
        mirror_sectors.push(sector_index);
    }
    for sector_index in mirror_sectors {
        let sector = world.sectors[sector_index];
        let Ok(start) = usize::try_from(sector.wallptr) else {
            continue;
        };
        if start >= numwalls {
            continue;
        }
        let wall_count = usize::try_from(sector.wallnum).unwrap_or(0);
        let end = start.saturating_add(wall_count).min(numwalls);
        if let Some(walls) = world.walls.get_mut(start..end) {
            for wall in walls {
                wall.picnum = mirror_tile;
                wall.overpicnum = mirror_tile;
            }
        }
    }
    mirror_walls
}

/// Live Atomic shade cycler initialized from a map `CYCLER` sprite.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ShadeCycler {
    pub(crate) sector: i16,
    pub(crate) phase: i16,
    pub(crate) min_shade: i8,
    pub(crate) max_shade: i8,
    pub(crate) hitag: i16,
    pub(crate) active: bool,
}

/// Run the early `prelevel` sprite walk (GPSPEED / CYCLER deletes).
pub fn prelevel_sprites(lists: &mut SpriteLists, sprites: &mut [SpriteT], skill: u8) {
    let mut sectors: [SectorT; 0] = [];
    let _ = prelevel_sprites_with_state(lists, sprites, &mut sectors, skill);
}

/// Run the prelevel marker pass while retaining GPSPEED and CYCLER runtime data.
pub(crate) fn prelevel_sprites_with_state(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    skill: u8,
) -> Vec<ShadeCycler> {
    let _ = skill;
    // Chocolate `prelevel` sector pass: every sector's `extra` (the SE
    // `yvel` speed source, e.g. SE11 swing-door rate `SP>>3`) defaults to
    // 256 before GPSPEED overrides it; already-open lotag 20/22 doors get
    // their toggle bit so the first use closes them.
    for sector in sectors.iter_mut() {
        sector.extra = 256;
        if matches!(sector.lotag, 20 | 22) && sector.floorz > sector.ceilingz {
            sector.lotag |= i16::MIN;
        }
    }
    let mut cyclers = Vec::new();
    // Chocolate: `i = headspritestat[0]; while (i >= 0) { nexti = …; switch GPSPEED/CYCLER }`.
    let mut i = lists.headspritestat[STAT_DEFAULT as usize];
    while i >= 0 {
        let next = lists.nextspritestat[i as usize];
        let iu = i as usize;
        if iu < sprites.len() {
            match sprites[iu].picnum {
                GPSPEED => {
                    if let Some(sector_idx) = usize::try_from(sprites[iu].sectnum).ok()
                        && let Some(sector) = sectors.get_mut(sector_idx)
                    {
                        sector.extra = sprites[iu].lotag;
                    }
                    let _ = lists.deletesprite(i);
                }
                CYCLER => {
                    if let Some(sector_idx) = usize::try_from(sprites[iu].sectnum).ok()
                        && let Some(sector) = sectors.get(sector_idx)
                    {
                        cyclers.push(ShadeCycler {
                            sector: sprites[iu].sectnum,
                            phase: sprites[iu].lotag,
                            min_shade: sprites[iu].shade,
                            max_shade: sector.floorshade,
                            hitag: sprites[iu].hitag,
                            active: sprites[iu].ang == 1536,
                        });
                    }
                    let _ = lists.deletesprite(i);
                }
                _ => {}
            }
        }
        i = next;
    }
    // Safety: any GPSPEED/CYCLER left on other stats (unusual maps).
    for i in 0..sprites.len() {
        if lists.sprite_stat[i] == MAXSTATUS as i16 {
            continue;
        }
        if lists.sprite_stat[i] == STAT_DEFAULT {
            continue; // already walked
        }
        match sprites[i].picnum {
            GPSPEED => {
                if let Some(sector_idx) = usize::try_from(sprites[i].sectnum).ok()
                    && let Some(sector) = sectors.get_mut(sector_idx)
                {
                    sector.extra = sprites[i].lotag;
                }
                let _ = lists.deletesprite(i as i16);
            }
            CYCLER => {
                if let Some(sector_idx) = usize::try_from(sprites[i].sectnum).ok()
                    && let Some(sector) = sectors.get(sector_idx)
                {
                    cyclers.push(ShadeCycler {
                        sector: sprites[i].sectnum,
                        phase: sprites[i].lotag,
                        min_shade: sprites[i].shade,
                        max_shade: sector.floorshade,
                        hitag: sprites[i].hitag,
                        active: sprites[i].ang == 1536,
                    });
                }
                let _ = lists.deletesprite(i as i16);
            }
            _ => {}
        }
    }
    cyclers
}

#[cfg(test)]
mod tests {
    use super::*;
    use room_build_engine::world::SpriteLists;

    #[test]
    fn deletes_gpspeed_on_stat0() {
        let mut lists = SpriteLists::new();
        assert!(lists.place_sprite_at(0, 0, STAT_DEFAULT));
        assert!(lists.place_sprite_at(1, 0, STAT_DEFAULT));
        let mut sprites = vec![
            SpriteT {
                picnum: GPSPEED,
                sectnum: 0,
                ..Default::default()
            },
            SpriteT {
                picnum: 100,
                sectnum: 0,
                ..Default::default()
            },
        ];
        prelevel_sprites(&mut lists, &mut sprites, 3);
        assert_eq!(lists.sprite_stat[0], MAXSTATUS as i16);
        assert_ne!(lists.sprite_stat[1], MAXSTATUS as i16);
    }
}
