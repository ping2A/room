//! Enter-level sprite pass — partial Chocolate `premap.c` / `prelevel` (D5).
//!
//! Deletes marker sprites (`GPSPEED`, `CYCLER`) by walking `headspritestat[0]`
//! (same order as Chocolate, so the free-list head after deletes matches for
//! the later player `EGS`). Map `APLAYER` bodies are soft-culled in spawn;
//! the live player is inserted by [`crate::spawn::spawn_player_body`].

use room_build_engine::types::SpriteT;
use room_build_engine::world::{MAXSTATUS, SpriteLists};

use crate::stat::STAT_DEFAULT;

/// `names.h` — sector glide speed marker (consumed into `sector.extra`).
pub const GPSPEED: i16 = 10;
/// `names.h` — shade cycler marker.
pub const CYCLER: i16 = 7;

/// Run the early `prelevel` sprite walk (GPSPEED / CYCLER deletes).
pub fn prelevel_sprites(lists: &mut SpriteLists, sprites: &mut [SpriteT], skill: u8) {
    let _ = skill;
    // Chocolate: `i = headspritestat[0]; while (i >= 0) { nexti = …; switch GPSPEED/CYCLER }`.
    let mut i = lists.headspritestat[STAT_DEFAULT as usize];
    while i >= 0 {
        let next = lists.nextspritestat[i as usize];
        let iu = i as usize;
        if iu < sprites.len() {
            match sprites[iu].picnum {
                GPSPEED | CYCLER => {
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
            GPSPEED | CYCLER => {
                let _ = lists.deletesprite(i as i16);
            }
            _ => {}
        }
    }
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
