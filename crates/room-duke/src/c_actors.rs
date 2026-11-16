//! Chocolate `actors.c::moveactors` C-coded actors that have no CON body:
//! the RECON patrol car, rats, live green slime, ooze and swivelling
//! security cameras. The CON actor loop hands these sprites here instead of
//! skipping them (they would otherwise never move or attack).

use room_build_engine::geom::{
    SectorWalls, SectorZ, WallXY, cansee, clipmove_xy_at_z_sprites_with_lists, getzrange,
};
use room_build_engine::helpers::{getangle, sincos_q14};
use room_build_engine::numeric::krand_step;
use room_build_engine::types::{SectorT, SpriteT, WallT};
use room_build_engine::world::SpriteLists;

use crate::con::{ActorConTemps, ScriptImage};
use crate::fta::findplayer_dist;
use crate::player::{Player, TileZMeta};
use crate::shoot::ldist;

const TICSPERFRAME: i32 = 4;
const DUKE_GC: i32 = 176;

const RECON: i16 = 1960;
const RAT: i16 = 1267;
const OOZ: i16 = 2300;
const OOZ2: i16 = 2309;
const CAMERA1: i16 = 621;
const GREENSLIME: i16 = 2370;
const PIGCOP: i16 = 2000;
const LIZTROOP: i16 = 1680;
const LIZMAN: i16 = 2120;
const NEWBEAST: i16 = 4610;
const EXPLOSION2: i16 = 1890;
const FIRELASER: i16 = 1625;
const BLOODPOOL: i16 = 1226;
const SCRAP3: i16 = 2408;
const SCRAP6: i16 = 2390;

const RECO_PAIN: i16 = 128;
const RECO_ATTACK: i16 = 127;
const RECO_ROAM: i16 = 125;
const LASERTRIP_EXPLODE: i16 = 17;
const RATTY: i16 = 247;
const SLIM_ROAM: i16 = 163;
const SLIM_ATTACK: i16 = 34;
const SLIM_DYING: i16 = 149;
const SQUISHED: i16 = 69;

const HANDBOMB_WEAPON: i16 = 5;
const TRIPBOMB_WEAPON: i16 = 8;
const HANDREMOTE_WEAPON: i16 = 10;
const SLEEPTIME: i16 = 24 * 64;

/// Whether `pic` is one of the C-only actors handled by [`move_c_actor`].
#[must_use]
pub(crate) fn is_c_actor(pic: i16) -> bool {
    matches!(pic, RECON | RAT | OOZ | OOZ2)
        || (GREENSLIME..=GREENSLIME + 7).contains(&pic)
        || pic == CAMERA1
}

/// Shared world references for one C actor tick.
pub(crate) struct CActorCtx<'a> {
    pub lists: &'a mut SpriteLists,
    pub sprites: &'a mut [SpriteT],
    pub sectors: &'a mut [SectorT],
    pub walls: &'a mut [WallT],
    pub picnum: &'a mut Vec<i16>,
    pub temps: &'a mut Vec<ActorConTemps>,
    pub tile_meta: &'a [TileZMeta],
    pub script: &'a ScriptImage,
    pub players: &'a mut [Player],
    pub sync_bits: u32,
    pub randomseed: &'a mut i32,
    pub global_random: i32,
    pub single_player: bool,
    pub actor_toggle: u8,
}

impl CActorCtx<'_> {
    fn rand(&mut self) -> i32 {
        krand_step(self.randomseed)
    }

    fn sound(&mut self, i: usize, id: i16) {
        self.temps[i].pending_actor_sounds.push(id);
    }

    fn kill(&mut self, i: usize) {
        let _ = self.lists.deletesprite(i as i16);
        self.sprites[i].statnum = room_build_engine::world::MAXSTATUS as i16;
    }

    fn geometry(&self) -> (Vec<SectorWalls>, Vec<SectorZ>, Vec<WallXY>) {
        (
            self.sectors.iter().map(SectorT::walls).collect(),
            self.sectors.iter().map(SectorT::z).collect(),
            self.walls.iter().map(WallT::xy).collect(),
        )
    }

    /// Chocolate `getglobalz` (sector clip only; `zr = 127`).
    fn getglobalz(&mut self, i: usize) {
        let s = self.sprites[i];
        let (_, sec_z, wxy) = self.geometry();
        let range = getzrange(
            s.x,
            s.y,
            s.z - 256,
            s.sectnum,
            127,
            (1 << 16) | 1,
            &sec_z,
            &wxy,
        );
        if range.ceilz != i32::MIN {
            self.temps[i].hit_ceilingz = range.ceilz;
            self.temps[i].hit_floorz = range.florz;
        }
    }

    fn makeitfall(&mut self, i: usize) {
        let peers = self.sprites.to_vec();
        let mut s = self.sprites[i];
        crate::spawn::makeitfall(
            &mut s,
            self.sectors,
            self.walls,
            &peers,
            self.tile_meta,
            DUKE_GC,
            false,
        );
        self.sprites[i] = s;
    }

    /// Chocolate `IFHIT` / `ifhitbyweapon`: apply queued damage, return the
    /// weapon tile or -1.
    fn ifhit(&mut self, i: usize) -> i16 {
        let t = &mut self.temps[i];
        if t.hit_extra < 0 {
            return -1;
        }
        if self.sprites[i].extra < 0 {
            t.hit_extra = -1;
            return -1;
        }
        self.sprites[i].extra = self.sprites[i].extra.saturating_sub(t.hit_extra);
        t.hit_extra = -1;
        t.hit_picnum
    }

    fn spawn(&mut self, parent: usize, pn: i16) -> i16 {
        crate::spawn_rt::spawn_from(
            self.lists,
            self.sprites,
            self.sectors,
            self.walls,
            self.picnum,
            self.temps,
            self.tile_meta,
            self.script,
            self.randomseed,
            parent as i16,
            pn,
        )
    }

    /// Chocolate `RANDOMSCRAP` (EGS arguments evaluated right to left).
    fn random_scrap(&mut self, i: usize) {
        let src = self.sprites[i];
        let zvel = (-512 - (self.rand() & 2047)) as i16;
        let xvel = (64 + (self.rand() & 63)) as i16;
        let ang = (self.rand() & 2047) as i16;
        let pn = SCRAP6 + (self.rand() & 15) as i16;
        let z = src.z - (8 << 8) - (self.rand() & 8191);
        let y = src.y + (self.rand() & 255) - 128;
        let x = src.x + (self.rand() & 255) - 128;
        self.egs(
            src.sectnum,
            x,
            y,
            z,
            pn,
            -8,
            48,
            48,
            ang,
            xvel,
            zvel,
            i as i16,
            crate::stat::STAT_MISC,
        );
    }

    /// Green slime gore: `EGS(SCRAP3+(TRAND&3), …)` with pal 6.
    fn slime_scrap(&mut self, i: usize) {
        let src = self.sprites[i];
        let zvel = (-(self.rand() & 4095) - (i32::from(src.zvel) >> 2)) as i16;
        let xvel = ((self.rand() & 63) + 64) as i16;
        let ang = (self.rand() & 2047) as i16;
        let pn = SCRAP3 + (self.rand() & 3) as i16;
        let j = self.egs(
            src.sectnum,
            src.x,
            src.y,
            src.z - (8 << 8),
            pn,
            -8,
            48,
            48,
            ang,
            xvel,
            zvel,
            i as i16,
            5,
        );
        if j >= 0 {
            self.sprites[j as usize].pal = 6;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn egs(
        &mut self,
        sect: i16,
        x: i32,
        y: i32,
        z: i32,
        pn: i16,
        shade: i8,
        xr: u8,
        yr: u8,
        ang: i16,
        xvel: i16,
        zvel: i16,
        owner: i16,
        stat: i16,
    ) -> i16 {
        crate::shoot::egs(
            self.lists,
            self.sprites,
            self.picnum,
            self.temps,
            Some(self.script),
            sect,
            x,
            y,
            z,
            pn,
            shade,
            xr,
            yr,
            ang,
            xvel,
            zvel,
            owner,
            stat,
        )
    }

    /// Chocolate `movesprite` for badguys (`ssp`): clipmove, stay-put gate,
    /// wall bounce and the cached floor/ceiling Z window. Returns the hit.
    fn movesprite(&mut self, i: usize, xchange: i32, ychange: i32, zchange: i32) -> i32 {
        let (sec, sec_z, wxy) = self.geometry();
        let mut s = self.sprites[i];
        let cd = if s.xrepeat > 60 {
            1024
        } else if self
            .script
            .actortype
            .get(s.picnum.max(0) as usize)
            .copied()
            .unwrap_or(0)
            & 3
            != 0
        {
            i32::from(s.clipdist) << 2
        } else {
            192
        };
        let height = self
            .tile_meta
            .get(s.picnum.max(0) as usize)
            .map_or(64, |t| i32::from(t.height));
        let daz = s.z - ((height * i32::from(s.yrepeat)) << 1);
        let (oldx, oldy, oldsect) = (s.x, s.y, s.sectnum);
        let mut sect = s.sectnum;
        let retval = clipmove_xy_at_z_sprites_with_lists(
            &mut s.x,
            &mut s.y,
            daz,
            &mut sect,
            (xchange * TICSPERFRAME) >> 3,
            (ychange * TICSPERFRAME) >> 3,
            cd,
            4 << 8,
            4 << 8,
            &sec,
            &wxy,
            Some(&sec_z),
            &[],
            1,
            Some(self.lists),
        );
        let stayput = self.temps[i].actorstayput;
        if sect < 0 || (stayput >= 0 && stayput != sect) {
            s.x = oldx;
            s.y = oldy;
            self.sprites[i] = s;
            return 16384 + i32::from(oldsect.max(0));
        }
        if (retval & 49152) >= 32768 && self.temps[i].cgg == 0 {
            s.ang = ((i32::from(s.ang) + 768) & 2047) as i16;
        }
        self.sprites[i] = s;
        if sect != oldsect {
            let _ = self.lists.changespritesect(i as i16, sect);
            self.sprites[i].sectnum = sect;
        }
        let next_z = self.sprites[i].z + ((zchange * TICSPERFRAME) >> 3);
        if next_z > self.temps[i].hit_ceilingz && next_z <= self.temps[i].hit_floorz {
            self.sprites[i].z = next_z;
        } else if retval == 0 {
            return 16384 + i32::from(sect);
        }
        retval
    }

    /// Chocolate `ssp(i, CLIPMASK0)`: move along `ang` at `xvel`/`zvel`;
    /// true when nothing was hit.
    fn ssp(&mut self, i: usize) -> bool {
        let s = self.sprites[i];
        let (sin, cos) = sincos_q14(i32::from(s.ang));
        let xvel = i32::from(s.xvel);
        self.movesprite(i, (xvel * cos) >> 14, (xvel * sin) >> 14, i32::from(s.zvel)) == 0
    }

    fn player_sees(&self, i: usize, p: usize) -> bool {
        let s = self.sprites[i];
        let player = &self.players[p];
        let sec_z: Vec<SectorZ> = self.sectors.iter().map(SectorT::z).collect();
        let wxy: Vec<WallXY> = self.walls.iter().map(WallT::xy).collect();
        cansee(
            s.x,
            s.y,
            s.z - (16 << 8),
            s.sectnum,
            player.posx,
            player.posy,
            player.posz,
            player.cursectnum,
            &sec_z,
            &wxy,
        )
    }

    /// Chocolate `LocateTheLocator(n, -1)`.
    fn locate_the_locator(&self, n: i32) -> i16 {
        let mut j = self.lists.headspritestat[crate::stat::STAT_LOCATOR as usize];
        while j >= 0 {
            if i32::from(self.sprites[j as usize].lotag) == n {
                return j;
            }
            j = self.lists.nextspritestat[j as usize];
        }
        -1
    }

    fn shoot_firelaser(&mut self, i: usize) {
        let _ = crate::shoot::shoot_enemy(
            self.lists,
            self.sprites,
            self.sectors,
            self.walls,
            self.picnum,
            self.temps,
            self.tile_meta,
            self.script,
            self.players,
            self.randomseed,
            i as i16,
            FIRELASER,
        );
    }
}

thread_local! {
    static EVERYOTHERTIME: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// Chocolate `everyothertime`, mirrored from the session before `moveactors`.
pub(crate) fn set_everyothertime(value: u8) {
    EVERYOTHERTIME.with(|cell| cell.set(value));
}

fn sintable(a: i32) -> i32 {
    sincos_q14(a & 2047).0
}

/// Chocolate `rnd(X)`: `(TRAND >> 8) >= 255 - X`.
fn rnd(ctx: &mut CActorCtx<'_>, x: i32) -> bool {
    (ctx.rand() >> 8) >= 255 - x
}

fn getincangle(a: i32, na: i32) -> i32 {
    let a = a & 2047;
    let na = na & 2047;
    if (a - na).abs() < 1024 {
        return na - a;
    }
    let na = if na > 1024 { na - 2048 } else { na };
    let a = if a > 1024 { a - 2048 } else { a };
    na - a
}

fn sgn(v: i32) -> i32 {
    v.signum()
}

/// Run one C actor tick. Returns `true` when the actor was handled (the CON
/// body, if any, must not run this tic).
pub(crate) fn move_c_actor(ctx: &mut CActorCtx<'_>, i: usize) -> bool {
    if ctx.temps.len() <= i {
        ctx.temps.resize_with(i + 1, ActorConTemps::default);
    }
    match ctx.sprites[i].picnum {
        RECON => move_recon(ctx, i),
        RAT => move_rat(ctx, i),
        OOZ | OOZ2 => {
            ctx.getglobalz(i);
            let j = ((ctx.temps[i].hit_floorz - ctx.temps[i].hit_ceilingz) >> 9).clamp(0, 255);
            let x = (25 - (j >> 1)).clamp(8, 48);
            ctx.sprites[i].yrepeat = j as u8;
            ctx.sprites[i].xrepeat = x as u8;
            ctx.sprites[i].z = ctx.temps[i].hit_floorz;
        }
        CAMERA1 => move_camera(ctx, i),
        pic if (GREENSLIME..=GREENSLIME + 7).contains(&pic) => move_greenslime(ctx, i),
        _ => return false,
    }
    true
}

fn move_camera(ctx: &mut CActorCtx<'_>, i: usize) {
    // Chocolate: shootable cameras only exist with `camerashitable`, which is
    // off in the shipped CON; only the sweep runs.
    if ctx.temps[i].g_t[0] != 0 {
        return;
    }
    ctx.temps[i].g_t[1] += 8;
    let hitag = i32::from(ctx.sprites[i].hitag);
    if hitag <= 0 {
        return;
    }
    let t1 = ctx.temps[i].g_t[1];
    let delta = if t1 < hitag || (t1 >= hitag * 3 && t1 < hitag << 2) {
        8
    } else if t1 < hitag * 3 {
        -8
    } else {
        ctx.temps[i].g_t[1] = 8;
        16
    };
    ctx.sprites[i].ang = ((i32::from(ctx.sprites[i].ang) + delta) & 2047) as i16;
}

fn move_rat(ctx: &mut CActorCtx<'_>, i: usize) {
    ctx.getglobalz(i);
    ctx.makeitfall(i);
    // `IFMOVING` = `ssp(i, CLIPMASK0)` returned "no hit".
    if ctx.ssp(i) {
        if ctx.rand() & 255 < 3 {
            ctx.sound(i, RATTY);
        }
        let wobble = (ctx.rand() & 31) - 15 + (sintable((ctx.temps[i].g_t[0] << 8) & 2047) >> 11);
        ctx.sprites[i].ang = ((i32::from(ctx.sprites[i].ang) + wobble) & 2047) as i16;
    } else {
        ctx.temps[i].g_t[0] += 1;
        if ctx.temps[i].g_t[0] > 1 {
            ctx.kill(i);
            return;
        }
        ctx.sprites[i].ang = (ctx.rand() & 2047) as i16;
    }
    if ctx.sprites[i].xvel < 128 {
        ctx.sprites[i].xvel += 2;
    }
    let turn = (ctx.rand() & 3) - 6;
    ctx.sprites[i].ang = ((i32::from(ctx.sprites[i].ang) + turn) & 2047) as i16;
}

#[allow(clippy::too_many_lines)]
fn move_recon(ctx: &mut CActorCtx<'_>, i: usize) {
    ctx.getglobalz(i);
    let sect = ctx.sprites[i].sectnum;
    let Some(sector) = usize::try_from(sect)
        .ok()
        .and_then(|s| ctx.sectors.get(s))
        .copied()
    else {
        return;
    };
    {
        let s = &mut ctx.sprites[i];
        let target = if sector.ceilingstat & 1 != 0 {
            sector.ceilingshade
        } else {
            sector.floorshade
        };
        s.shade = (i32::from(s.shade) + ((i32::from(target) - i32::from(s.shade)) >> 1)) as i8;
        if s.z < sector.ceilingz + (32 << 8) {
            s.z = sector.ceilingz + (32 << 8);
        }
    }
    if ctx.single_player {
        if ctx.actor_toggle == 1 {
            ctx.sprites[i].cstat = 32768;
            return;
        } else if ctx.actor_toggle == 2 {
            ctx.sprites[i].cstat = 257;
        }
    }
    if ctx.ifhit(i) >= 0 {
        if ctx.sprites[i].extra < 0 && ctx.temps[i].g_t[0] != -1 {
            ctx.temps[i].g_t[0] = -1;
            ctx.sprites[i].extra = 0;
        }
        ctx.sound(i, RECO_PAIN);
        ctx.random_scrap(i);
    }

    if ctx.temps[i].g_t[0] == -1 {
        // Shot down: spiral into the ground, then crash into a pig cop.
        ctx.sprites[i].z += 1024;
        ctx.temps[i].g_t[2] += 1;
        if ctx.temps[i].g_t[2] & 3 == 0 {
            ctx.spawn(i, EXPLOSION2);
        }
        ctx.getglobalz(i);
        ctx.sprites[i].ang = ((i32::from(ctx.sprites[i].ang) + 96) & 2047) as i16;
        ctx.sprites[i].xvel = 128;
        let moved = ctx.ssp(i);
        if !moved || ctx.sprites[i].z > ctx.temps[i].hit_floorz {
            for _ in 0..16 {
                ctx.random_scrap(i);
            }
            ctx.sound(i, LASERTRIP_EXPLODE);
            ctx.spawn(i, PIGCOP);
            if let Some(player) = ctx.players.first_mut() {
                player.actors_killed = player.actors_killed.wrapping_add(1);
            }
            ctx.kill(i);
        }
        return;
    }
    if ctx.sprites[i].z > ctx.temps[i].hit_floorz - (48 << 8) {
        ctx.sprites[i].z = ctx.temps[i].hit_floorz - (48 << 8);
    }

    let (p, x) = findplayer_dist(&ctx.sprites[i], ctx.players);
    let Some(player) = ctx.players.get(p) else {
        return;
    };
    let (ppx, ppy, ppz) = (player.posx, player.posy, player.posz);
    let mut j = ctx.sprites[i].owner;

    // t[0]: 0/1 patrol to a locator, 2 = match player height, 3 = hover
    // and shoot, 4/5 = strafe-run shooting while following locators.
    let t0 = ctx.temps[i].g_t[0];
    if t0 >= 4 {
        ctx.temps[i].g_t[2] += 1;
        if ctx.temps[i].g_t[2] & 15 == 0 {
            let a = ctx.sprites[i].ang;
            ctx.sprites[i].ang = ctx.temps[i].tempang;
            ctx.sound(i, RECO_ATTACK);
            ctx.shoot_firelaser(i);
            ctx.sprites[i].ang = a;
        }
        if ctx.temps[i].g_t[2] > 26 * 3 || !ctx.player_sees(i, p) {
            ctx.temps[i].g_t[0] = 0;
            ctx.temps[i].g_t[2] = 0;
        } else {
            let s = ctx.sprites[i];
            let goal = getangle(ppx - s.x, ppy - s.y);
            let ta = i32::from(ctx.temps[i].tempang);
            ctx.temps[i].tempang = ((ta + getincangle(ta, goal) / 3) & 2047) as i16;
        }
    } else if t0 == 2 || t0 == 3 {
        ctx.temps[i].g_t[3] = 0;
        let s = &mut ctx.sprites[i];
        s.xvel = if s.xvel > 0 { s.xvel - 16 } else { 0 };
        if t0 == 2 {
            let l = ppz - ctx.sprites[i].z;
            if l.abs() < (48 << 8) {
                ctx.temps[i].g_t[0] = 3;
            } else {
                ctx.sprites[i].z += sgn(l) << 10;
            }
        } else {
            ctx.temps[i].g_t[2] += 1;
            if ctx.temps[i].g_t[2] > 26 * 3 || !ctx.player_sees(i, p) {
                ctx.temps[i].g_t[0] = 1;
                ctx.temps[i].g_t[2] = 0;
            } else if ctx.temps[i].g_t[2] & 15 == 0 {
                ctx.sound(i, RECO_ATTACK);
                ctx.shoot_firelaser(i);
            }
        }
        let s = ctx.sprites[i];
        let goal = getangle(ppx - s.x, ppy - s.y);
        ctx.sprites[i].ang =
            ((i32::from(s.ang) + (getincangle(i32::from(s.ang), goal) >> 2)) & 2047) as i16;
    }

    let t0 = ctx.temps[i].g_t[0];
    if t0 != 2 && t0 != 3 {
        let owner = usize::try_from(j)
            .ok()
            .and_then(|o| ctx.sprites.get(o))
            .copied();
        let s = ctx.sprites[i];
        let a = match owner {
            Some(target) if ldist(&target, &s) > 1524 => getangle(target.x - s.x, target.y - s.y),
            Some(_) => {
                ctx.sprites[i].xvel >>= 1;
                i32::from(s.ang)
            }
            None => i32::from(s.ang),
        };
        if t0 == 1 || t0 == 4 {
            let far = owner.is_some_and(|target| {
                let dx = i64::from(target.x - s.x);
                let dy = i64::from(target.y - s.y);
                let dz = i64::from((target.z - s.z) >> 4);
                ((dx * dx + dy * dy + dz * dz) as f64).sqrt() > 1524.0
            });
            if !far {
                ctx.temps[i].g_t[0] = if t0 == 1 { 0 } else { 5 };
            } else if ctx.sprites[i].xvel < 256 {
                ctx.sprites[i].xvel += 32;
            }
            if ctx.temps[i].g_t[0] < 2 {
                ctx.temps[i].g_t[2] += 1;
            }
            if x < 6144 && ctx.temps[i].g_t[0] < 2 && ctx.temps[i].g_t[2] > 26 * 4 {
                ctx.temps[i].g_t[0] = 2 + (ctx.rand() & 2);
                ctx.temps[i].g_t[2] = 0;
                ctx.temps[i].tempang = ctx.sprites[i].ang;
            }
        }
        let t0 = ctx.temps[i].g_t[0];
        if t0 == 0 || t0 == 5 {
            ctx.temps[i].g_t[0] = if t0 == 0 { 1 } else { 4 };
            j = ctx.locate_the_locator(i32::from(ctx.sprites[i].hitag));
            ctx.sprites[i].owner = j;
            if j == -1 {
                let first = ctx.temps[i].g_t[5];
                ctx.sprites[i].hitag = first as i16;
                j = ctx.locate_the_locator(first);
                ctx.sprites[i].owner = j;
                if j == -1 {
                    ctx.kill(i);
                    return;
                }
            } else {
                ctx.sprites[i].hitag += 1;
            }
        }
        let s = ctx.sprites[i];
        let turn = getincangle(i32::from(s.ang), a);
        ctx.temps[i].g_t[3] = turn;
        ctx.sprites[i].ang = ((i32::from(s.ang) + (turn >> 3)) & 2047) as i16;
        if let Some(target) = usize::try_from(j)
            .ok()
            .and_then(|o| ctx.sprites.get(o))
            .copied()
        {
            if ctx.sprites[i].z < target.z {
                ctx.sprites[i].z += 1024;
            } else {
                ctx.sprites[i].z -= 1024;
            }
        }
    }
    // `if (Sound[RECO_ROAM].num == 0) spritesound(RECO_ROAM, i)`.
    ctx.temps[i].pending_actor_sounds_once.push(RECO_ROAM);
    let _ = ctx.ssp(i);
}

#[allow(clippy::too_many_lines)]
fn move_greenslime(ctx: &mut CActorCtx<'_>, i: usize) {
    // `tick_frozen_slime` already ran the `t[1] += 128` / floor-sky prelude
    // and the frozen state; this is the live slime.
    if ctx.single_player {
        if ctx.actor_toggle == 1 {
            ctx.sprites[i].cstat = 32768;
            return;
        } else if ctx.actor_toggle == 2 {
            ctx.sprites[i].cstat = 257;
        }
    }
    let (p, x) = findplayer_dist(&ctx.sprites[i], ctx.players);
    if p >= ctx.players.len() {
        return;
    }
    if x > 20480 {
        ctx.temps[i].timetosleep += 1;
        if ctx.temps[i].timetosleep > SLEEPTIME {
            ctx.temps[i].timetosleep = 0;
            let _ = ctx
                .lists
                .changespritestat(i as i16, crate::stat::STAT_DUMMY);
            return;
        }
    }
    ctx.sprites[i].cstat = if x < 1596 { 0 } else { 257 };
    let sect = ctx.sprites[i].sectnum;

    if ctx.temps[i].g_t[0] == -4 {
        // Clinging to the player's face.
        if ctx.players[p].extra < 1 {
            ctx.temps[i].g_t[0] = 0;
            return;
        }
        ctx.sprites[i].ang = ctx.players[p].ang;
        let player = &ctx.players[p];
        let firing = ctx.sync_bits & 4 != 0 || player.quick_kick > 0;
        let weapon = player.curr_weapon;
        let can_swat = player.quick_kick > 0
            || (weapon != HANDREMOTE_WEAPON
                && weapon != HANDBOMB_WEAPON
                && weapon != TRIPBOMB_WEAPON
                && player
                    .ammo
                    .get(weapon.max(0) as usize)
                    .is_some_and(|&a| a >= 0));
        if firing && player.extra > 0 && can_swat {
            for _ in 0..8 {
                ctx.slime_scrap(i);
            }
            ctx.sound(i, SLIM_DYING);
            ctx.sound(i, SQUISHED);
            if ctx.rand() & 255 < 32 {
                let j = ctx.spawn(i, BLOODPOOL);
                if j >= 0 {
                    ctx.sprites[j as usize].pal = 0;
                }
            }
            let player = &mut ctx.players[p];
            player.actors_killed = player.actors_killed.wrapping_add(1);
            if player.something_on_player == Some(i as i16) {
                player.something_on_player = None;
            }
            ctx.temps[i].g_t[0] = -3;
            ctx.kill(i);
            return;
        }
        let player = &ctx.players[p];
        let t2 = ctx.temps[i].g_t[2];
        ctx.sprites[i].z = player.posz + i32::from(player.pyoff) - t2
            + (8 << 8)
            + ((100 - i32::from(player.horiz)) << 4);
        if t2 > 512 {
            ctx.temps[i].g_t[2] -= 128;
        }
        if ctx.temps[i].g_t[2] < 348 {
            ctx.temps[i].g_t[2] += 128;
        }
        let t3 = ctx.temps[i].g_t[3];
        if t3 > 0 {
            const FRAMES: [i16; 8] = [5, 5, 6, 6, 7, 7, 6, 5];
            ctx.sprites[i].picnum = GREENSLIME + FRAMES[(t3 as usize).min(7)];
            if t3 == 5 {
                let bite = 5 + (ctx.rand() & 3) as i16;
                let player = &mut ctx.players[p];
                player.extra -= bite;
                ctx.sound(i, SLIM_ATTACK);
            }
            ctx.temps[i].g_t[3] = if t3 < 7 { t3 + 1 } else { 0 };
        } else {
            ctx.sprites[i].picnum = GREENSLIME + 5;
            if rnd(ctx, 32) {
                ctx.temps[i].g_t[3] = 1;
            }
        }
        let t1 = ctx.temps[i].g_t[1];
        let wobble = sintable(t1 & 2047) >> 13;
        ctx.sprites[i].xrepeat = (20 + wobble) as u8;
        ctx.sprites[i].yrepeat = (15 + wobble) as u8;
        let player = &ctx.players[p];
        let (sin, cos) = sincos_q14(i32::from(player.ang));
        ctx.sprites[i].x = player.posx + (cos >> 7);
        ctx.sprites[i].y = player.posy + (sin >> 7);
        let (px, py, psect) = (ctx.sprites[i].x, ctx.sprites[i].y, player.cursectnum);
        if psect >= 0 && psect != ctx.sprites[i].sectnum {
            let _ = ctx.lists.changespritesect(i as i16, psect);
            ctx.sprites[i].sectnum = psect;
        }
        let _ = (px, py);
        ctx.picnum[i] = ctx.sprites[i].picnum;
        return;
    } else if ctx.sprites[i].xvel < 64 && x < 768 && ctx.players[p].something_on_player.is_none() {
        ctx.players[p].something_on_player = Some(i as i16);
        let t0 = ctx.temps[i].g_t[0];
        ctx.temps[i].g_t[2] = if t0 == 3 || t0 == 2 {
            12 << 8
        } else {
            -(13 << 8)
        };
        ctx.temps[i].g_t[0] = -4;
    }

    let hit = ctx.ifhit(i);
    if hit >= 0 {
        ctx.sound(i, SLIM_DYING);
        let player = &mut ctx.players[p];
        player.actors_killed = player.actors_killed.wrapping_add(1);
        if player.something_on_player == Some(i as i16) {
            player.something_on_player = None;
        }
        if ctx.rand() & 255 < 32 {
            let j = ctx.spawn(i, BLOODPOOL);
            if j >= 0 {
                ctx.sprites[j as usize].pal = 0;
            }
        }
        for _ in 0..8 {
            ctx.slime_scrap(i);
        }
        ctx.temps[i].g_t[0] = -3;
        ctx.kill(i);
        return;
    }

    let t0 = ctx.temps[i].g_t[0];
    if t0 == -1 {
        // Shrinking back after eating an enemy.
        ctx.makeitfall(i);
        ctx.sprites[i].cstat &= !8;
        ctx.sprites[i].picnum = GREENSLIME + 4;
        if ctx.sprites[i].xrepeat > 32 {
            let d = (ctx.rand() & 7) as u8;
            ctx.sprites[i].xrepeat = ctx.sprites[i].xrepeat.saturating_sub(d);
        }
        if ctx.sprites[i].yrepeat > 16 {
            let d = (ctx.rand() & 7) as u8;
            ctx.sprites[i].yrepeat = ctx.sprites[i].yrepeat.saturating_sub(d);
        } else {
            ctx.sprites[i].xrepeat = 40;
            ctx.sprites[i].yrepeat = 16;
            ctx.temps[i].g_t[5] = -1;
            ctx.temps[i].g_t[0] = 0;
        }
        ctx.picnum[i] = ctx.sprites[i].picnum;
        return;
    } else if t0 != -2 {
        ctx.getglobalz(i);
    }

    if t0 == -2 {
        // Engulfing another enemy.
        ctx.makeitfall(i);
        let victim = usize::try_from(ctx.temps[i].g_t[5])
            .ok()
            .filter(|&v| v < ctx.sprites.len());
        let Some(v) = victim else {
            ctx.temps[i].g_t[0] = 0;
            return;
        };
        ctx.sprites[v].xvel = 0;
        let target = ctx.sprites[v];
        let (sin, cos) = sincos_q14(i32::from(target.ang));
        ctx.sprites[i].z = target.z;
        ctx.sprites[i].x = target.x + (cos >> 11);
        ctx.sprites[i].y = target.y + (sin >> 11);
        ctx.sprites[i].picnum = GREENSLIME + 2 + (ctx.global_random & 1) as i16;
        if ctx.sprites[i].yrepeat < 64 {
            ctx.sprites[i].yrepeat += 2;
        } else if ctx.sprites[i].xrepeat < 32 {
            ctx.sprites[i].xrepeat += 4;
        } else {
            ctx.temps[i].g_t[0] = -1;
            if ldist(&ctx.sprites[i], &target) < 768 {
                ctx.sprites[v].xrepeat = 0;
            }
        }
        ctx.picnum[i] = ctx.sprites[i].picnum;
        return;
    }

    // Randomly look for an enemy to gulp.
    if rnd(ctx, 32) {
        let mut j = usize::try_from(sect)
            .ok()
            .and_then(|s| ctx.lists.headspritesect.get(s).copied())
            .unwrap_or(-1);
        while j >= 0 {
            let other = ctx.sprites[j as usize];
            if matches!(other.picnum, LIZTROOP | LIZMAN | PIGCOP | NEWBEAST)
                && ldist(&ctx.sprites[i], &other) < 768
                && (ctx.sprites[i].z - other.z).abs() < 8192
            {
                ctx.temps[i].g_t[5] = i32::from(j);
                ctx.temps[i].g_t[0] = -2;
                ctx.temps[i].g_t[1] = 0;
                return;
            }
            j = ctx.lists.nextspritesect[j as usize];
        }
    }

    let sector = usize::try_from(sect)
        .ok()
        .and_then(|s| ctx.sectors.get(s))
        .copied();
    let Some(sector) = sector else { return };
    let t0 = ctx.temps[i].g_t[0];
    if t0 == 0 || t0 == 2 {
        // Crawling on the floor (0) or the ceiling (2).
        ctx.sprites[i].picnum = GREENSLIME;
        if ctx.rand() & 511 == 0 {
            ctx.sound(i, SLIM_ROAM);
        }
        if t0 == 2 {
            ctx.sprites[i].zvel = 0;
            ctx.sprites[i].cstat &= !8;
            if sector.ceilingstat & 1 != 0 || ctx.temps[i].hit_ceilingz + 6144 < ctx.sprites[i].z {
                ctx.sprites[i].z += 2048;
                ctx.temps[i].g_t[0] = 3;
                ctx.picnum[i] = ctx.sprites[i].picnum;
                return;
            }
        } else {
            ctx.sprites[i].cstat |= 8;
            ctx.makeitfall(i);
        }
        if EVERYOTHERTIME.with(std::cell::Cell::get) & 1 != 0 {
            let _ = ctx.ssp(i);
        }
        if ctx.sprites[i].xvel > 96 {
            ctx.sprites[i].xvel -= 2;
            ctx.picnum[i] = ctx.sprites[i].picnum;
            return;
        }
        let t1 = ctx.temps[i].g_t[1];
        ctx.sprites[i].xvel = (64 - (sintable((t1 + 512) & 2047)) >> 9) as i16;
        let player = &ctx.players[p];
        let s = ctx.sprites[i];
        let goal = getangle(player.posx - s.x, player.posy - s.y);
        ctx.sprites[i].ang =
            ((i32::from(s.ang) + (getincangle(i32::from(s.ang), goal) >> 3)) & 2047) as i16;
        ctx.sprites[i].xrepeat = (36 + (sintable((t1 + 512) & 2047)) >> 11) as u8;
        ctx.sprites[i].yrepeat = (16 + (sintable(t1 & 2047)) >> 13) as u8;
        if rnd(ctx, 4)
            && sector.ceilingstat & 1 == 0
            && (ctx.temps[i].hit_floorz - ctx.temps[i].hit_ceilingz).abs() < (192 << 8)
        {
            ctx.sprites[i].zvel = 0;
            ctx.temps[i].g_t[0] += 1;
        }
    }
    let t0 = ctx.temps[i].g_t[0];
    if t0 == 1 {
        // Jumping up to the ceiling.
        ctx.sprites[i].picnum = GREENSLIME;
        let s = &mut ctx.sprites[i];
        if s.yrepeat < 40 {
            s.yrepeat += 8;
        }
        if s.xrepeat > 8 {
            s.xrepeat -= 4;
        }
        if s.zvel > -(2048 + 1024) {
            s.zvel -= 348;
        }
        s.z += i32::from(s.zvel);
        if s.z < ctx.temps[i].hit_ceilingz + 4096 {
            s.z = ctx.temps[i].hit_ceilingz + 4096;
            s.xvel = 0;
            ctx.temps[i].g_t[0] = 2;
        }
    }
    if ctx.temps[i].g_t[0] == 3 {
        // Dropping from the ceiling.
        ctx.sprites[i].picnum = GREENSLIME + 1;
        ctx.makeitfall(i);
        let floor = ctx.temps[i].hit_floorz;
        let s = &mut ctx.sprites[i];
        if s.z > floor - (8 << 8) {
            s.yrepeat = s.yrepeat.saturating_sub(4);
            s.xrepeat = s.xrepeat.saturating_add(2);
        } else {
            if s.yrepeat < 40 - 4 {
                s.yrepeat += 8;
            }
            if s.xrepeat > 8 {
                s.xrepeat -= 4;
            }
        }
        if s.z > floor - 2048 {
            s.z = floor - 2048;
            ctx.temps[i].g_t[0] = 0;
            s.xvel = 0;
        }
    }
    ctx.picnum[i] = ctx.sprites[i].picnum;
}
