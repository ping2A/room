//! Duke Nukem 3D `game#.sav` header (`loadpheader` in `MENUES.C`).
//!
//! Each field is its own Ken `kdfread` blob. The rest of the file is a
//! version-specific memory dump (walls, sectors, `ps[]`). The dump's
//! `numwalls` / `wall[]` / `numsectors` / `sector[]` / `numsprites` / `sprite[]` / `numcyclers` / `ps[]` / `numanimwalls` / `spriteqamount` / `numclouds` / `randomseed` / `lockclock` / `pskybits` / `pskyoff` / `animatecnt` / `animategoal` / `animatevel` / `animateptr` / `earthquaketime` / `from_bonus` / `secretlevel` / `respawn_monsters` / `respawn_items` / `respawn_inventory` / `god` / `auto_run` / `crosshair` / `monsters_off` / `last_level` / `eog` / `coop` / `marker` / `ffire` / `camsprite` / `connecthead` / `connectpoint2` / `numplayersprites` / `frags` / `global_random` / `parallaxyscale` prefix is read; `randomseed`, `lockclock`, `earthquaketime`, `respawn_monsters`, `respawn_items`, `respawn_inventory`, `monsters_off`, `from_bonus`, `secretlevel`, `eog`, `global_random`, `coop`, `ffire`, `marker`, `last_level`, `camsprite`, `pskybits`, `pskyoff`, `parallaxyscale`, `connecthead`, `connectpoint2`, `numplayersprites`, `frags`, `animatecnt` / `animategoal` / `animatevel` / `animateptr`, `numanimwalls`, `spriteqamount`, `numclouds`, `numcyclers`, `god`, `auto_run`, `crosshair`, and the first `ps[]` pose (including `horiz+horizoff` pitch, `ohoriz`/`ohorizoff` previous look, `look_ang` yaw plus `weapon_ang` look-aside, `last_extra` health plus `extra_extra8 / 256` overheal, `dead_flag` corpse, `frag_ps` attacker, `inven_icon` selected inventory, `toggle_key_flag` inventory-key hold, `refresh_inventory` inventory-name redraw, `invdisptime` HUD inventory-name timer, `on_ground` stance, `transporter_hold` lock, `hbomb_on` remote, `weapon_pos` HUD Y, `got_access` cards (1 blue / 2 red / 4 yellow), `holoduke_on` decoy, `jetpack_on` flight, `scuba_on` radsuit, `airleft` remaining air, `jumping_counter` mid-jump, `heat_on` night vision, `falling_counter` fall tics, `hard_landing` view dip, `pycount` bob phase, `newowner` camera, `hurt_delay` pain flash, `hbomb_hold_delay` pipebomb wind-up, `rotscrnang` screen tilt, `jumping_toggle` jump hold, `knuckle_incs` knuckle-crack, `return_to_center` look recenter, `tipincs` tip pose, `show_empty_weapon` empty click, `rapid_fire_hold` held fire, `pals` / `pals_time` HUD flash, `palookup` player pal, `walking_snd_toggle` footstep phase, `footprintshade` footprint shade, `footprintcount` remaining footprints, `footprintpal` footprint pal, `holster_weapon` holster, `quick_kick` kick, `knee_incs` knee Fire, `access_incs` Use, `access_wallnum` wall Use, `access_spritenum` sprite Use, `fta`/`ftq` quote, `kickback_pic` Fire, `ammo_amount` clip (pistol / ripper) / shell / rocket / cell / pipebomb / devastator / freeze / trip / grow, `curr_weapon`, `last_weapon`, `last_full_weapon`, `wantweaponfire`, `gotweapon`, `shield_amount` armor, `firstaid_amount`, `steroids_amount`, `jetpack_amount`, `scuba_amount`, `holoduke_amount`, `boot_amount`, and `heat_amount` night vision) plus dump `sector[]` floor / ceiling Z, dump `sector[]` floor / ceiling tiles, dump `sector[]` light from `floorshade` / `visibility`, dump `sector[]` fog density (`visibility × 4`), dump `sprite[]` extra > 0 XY onto living `COUNTKILL`s, dump `sprite[]` extra as actor health, dump `sprite[]` `cstat` (blocking / translucent / invisible), dump `sprite[]` `xrepeat` / `yrepeat` / `clipdist` as scale and radius, dump `sprite[]` `pal` as `DukePalN`, dump `sprite[]` `ang` as converter yaw, dump `sprite[]` Z (`−z/256`), and dump `wall[]` extra / blocking / mid tiles onto linedefs are applied. `gotpic` (any set bit reveals the automap) and `playerswhenstarted` after that prefix are applied. Anything further is not restored — Room still uses
//! `.fsave` for that.

use crate::dmo::{kdfread_blob, kdfread_one, kdfskip_one};
use crate::error::BuildError;

/// One Duke `walltype` (32 bytes) from the dump `wall[]` blob.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavWall {
    /// Build X.
    pub x: i32,
    /// Build Y.
    pub y: i32,
    /// Next point on the loop.
    pub point2: i16,
    /// Adjacent wall, or −1.
    pub nextwall: i16,
    /// Adjacent sector, or −1.
    pub nextsector: i16,
    /// Wall `cstat`.
    pub cstat: i16,
    /// Main texture tile.
    pub picnum: i16,
    /// Masked/over texture tile.
    pub overpicnum: i16,
    /// Shade.
    pub shade: i8,
    /// Palette.
    pub pal: u8,
    /// X repeat.
    pub xrepeat: u8,
    /// Y repeat.
    pub yrepeat: u8,
    /// X panning.
    pub xpanning: u8,
    /// Y panning.
    pub ypanning: u8,
    /// Lotag.
    pub lotag: i16,
    /// Hitag.
    pub hitag: i16,
    /// Extra.
    pub extra: i16,
}

impl SavWall {
    /// Room map XY (`x/16`, `−y/16`).
    #[must_use]
    pub fn room_xy(&self) -> (i32, i32) {
        (self.x / 16, -self.y / 16)
    }

    /// Converted mid texture (`overpicnum`, else `picnum`, else `-`).
    #[must_use]
    pub fn room_mid_pic(&self) -> String {
        let pic = if self.overpicnum != 0 {
            self.overpicnum
        } else {
            self.picnum
        };
        if pic == 0 {
            "-".into()
        } else {
            crate::convert::tile_name(i32::from(pic))
        }
    }
}

/// One Duke `sectortype` (40 bytes) from the dump `sector[]` blob.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavSector {
    /// First wall index.
    pub wallptr: i16,
    /// Wall count.
    pub wallnum: i16,
    /// Build ceiling Z.
    pub ceilingz: i32,
    /// Build floor Z.
    pub floorz: i32,
    /// Ceiling `cstat`.
    pub ceilingstat: i16,
    /// Floor `cstat`.
    pub floorstat: i16,
    /// Ceiling texture tile.
    pub ceilingpicnum: i16,
    /// Ceiling slope heinum.
    pub ceilingheinum: i16,
    /// Ceiling shade.
    pub ceilingshade: i8,
    /// Ceiling palette.
    pub ceilingpal: u8,
    /// Floor texture tile.
    pub floorpicnum: i16,
    /// Floor slope heinum.
    pub floorheinum: i16,
    /// Floor shade.
    pub floorshade: i8,
    /// Floor palette.
    pub floorpal: u8,
    /// Visibility.
    pub visibility: u8,
    /// Lotag.
    pub lotag: i16,
    /// Hitag.
    pub hitag: i16,
    /// Extra.
    pub extra: i16,
}

impl SavSector {
    /// Room ceiling Z (`−z/256`).
    #[must_use]
    pub fn room_ceiling_z(&self) -> i32 {
        -self.ceilingz / 256
    }

    /// Room floor Z (`−z/256`).
    #[must_use]
    pub fn room_floor_z(&self) -> i32 {
        -self.floorz / 256
    }

    /// Converted floor texture (`BT####`).
    #[must_use]
    pub fn room_floor_pic(&self) -> String {
        crate::convert::tile_name(i32::from(self.floorpicnum))
    }

    /// Converted ceiling texture (`BT####`).
    #[must_use]
    pub fn room_ceiling_pic(&self) -> String {
        crate::convert::tile_name(i32::from(self.ceilingpicnum))
    }

    /// Room sector light from `floorshade` and `visibility`.
    #[must_use]
    pub fn room_light(&self) -> i16 {
        let mut light = crate::convert::shade_light(self.floorshade);
        if self.visibility != 0 {
            light = (light * 128 / (128 + i32::from(self.visibility))).clamp(32, 255);
        }
        i16::try_from(light).unwrap_or(255)
    }

    /// Room fog density (`visibility × 4`).
    #[must_use]
    pub fn room_fog_density(&self) -> u8 {
        u8::try_from(i32::from(self.visibility).saturating_mul(4).clamp(0, 255)).unwrap_or(255)
    }
}

/// One Duke `spritetype` (44 bytes) from the dump `sprite[]` blob.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavSprite {
    /// Build X.
    pub x: i32,
    /// Build Y.
    pub y: i32,
    /// Build Z.
    pub z: i32,
    /// Sprite `cstat`.
    pub cstat: i16,
    /// Tile.
    pub picnum: i16,
    /// Shade.
    pub shade: i8,
    /// Palette.
    pub pal: u8,
    /// Clip distance.
    pub clipdist: u8,
    /// X repeat.
    pub xrepeat: u8,
    /// Y repeat.
    pub yrepeat: u8,
    /// Sector index.
    pub sectnum: i16,
    /// Status list.
    pub statnum: i16,
    /// Angle (Build).
    pub ang: i16,
    /// Owner sprite.
    pub owner: i16,
    /// Lotag.
    pub lotag: i16,
    /// Hitag.
    pub hitag: i16,
    /// Extra.
    pub extra: i16,
}

impl SavSprite {
    /// Room map XY (`x/16`, `−y/16`).
    #[must_use]
    pub fn room_xy(&self) -> (i32, i32) {
        (self.x / 16, -self.y / 16)
    }

    /// Room Z (`−z/256`).
    #[must_use]
    pub fn room_z(&self) -> i32 {
        -self.z / 256
    }

    /// Room scale (`xrepeat/64`, `yrepeat/64`).
    #[must_use]
    pub fn room_scale(&self) -> (f64, f64) {
        (
            f64::from(self.xrepeat) / 64.0,
            f64::from(self.yrepeat) / 64.0,
        )
    }

    /// Room clip radius (`clipdist/16`) when set.
    #[must_use]
    pub fn room_clipdist(&self) -> Option<f64> {
        (self.clipdist != 0).then(|| (f64::from(self.clipdist) / 16.0).clamp(1.0, 64.0))
    }

    /// Room palette translation (`DukePalN`, empty when 0).
    #[must_use]
    pub fn room_translation(&self) -> String {
        if self.pal == 0 {
            String::new()
        } else {
            format!("DukePal{}", self.pal)
        }
    }

    /// Room yaw degrees (converter: `−(ang & 2047) * 360 / 2048`).
    #[must_use]
    pub fn room_yaw(&self) -> i32 {
        (-(i32::from(self.ang) & 2047) * 360 / 2048).rem_euclid(360)
    }
}

/// First `player_struct` pose from dump `ps[]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavPlayer {
    /// Build X.
    pub posx: i32,
    /// Build Y.
    pub posy: i32,
    /// Build Z.
    pub posz: i32,
    /// Horizon (100 = level).
    pub horiz: i32,
    /// Previous horizon (`ohoriz`) when the blob is long enough.
    pub ohoriz: Option<i32>,
    /// Previous look offset (`ohorizoff`) when the blob is long enough.
    pub ohorizoff: Option<i32>,
    /// Build angle (0..=2047).
    pub ang: i16,
    /// Copy of sprite extra (health) when the blob is long enough.
    pub last_extra: Option<i16>,
    /// Overheal leftover (`extra_extra8`) when the blob is long enough.
    pub extra_extra8: Option<i16>,
    /// Look offset (`horizoff`) added to `horiz` when the blob is long enough.
    pub horizoff: Option<i16>,
    /// Look-aside (`look_ang`) added to `ang` when the blob is long enough.
    pub look_ang: Option<i16>,
    /// Screen tilt (`rotscrnang`, Build ang) when the blob is long enough.
    pub rotscrnang: Option<i16>,
    /// `ammo_amount[MAX_WEAPONS]` when the blob is long enough.
    pub ammo_amount: Option<[i16; 12]>,
    /// Ready weapon slot when the blob is long enough.
    pub curr_weapon: Option<i16>,
    /// Previous weapon slot (`last_weapon`) when the blob is long enough.
    pub last_weapon: Option<i16>,
    /// Pending fire slot (`wantweaponfire`; `-1` = none) when long enough.
    pub wantweaponfire: Option<i16>,
    /// Recenter look (`return_to_center`, 30 Hz) when the blob is long enough.
    pub return_to_center: Option<i16>,
    /// Mid tip pose (`tipincs`, 30 Hz) when the blob is long enough.
    pub tipincs: Option<i16>,
    /// `gotweapon[MAX_WEAPONS]` when the blob is long enough.
    pub gotweapon: Option<[u8; 12]>,
    /// `shield_amount` (armor) when the blob is long enough.
    pub shield_amount: Option<i16>,
    /// `firstaid_amount` (medkit) when the blob is long enough.
    pub firstaid_amount: Option<i16>,
    /// `steroids_amount` when the blob is long enough.
    pub steroids_amount: Option<i16>,
    /// `jetpack_amount` when the blob is long enough.
    pub jetpack_amount: Option<i16>,
    /// `scuba_amount` when the blob is long enough.
    pub scuba_amount: Option<i16>,
    /// `holoduke_amount` when the blob is long enough.
    pub holoduke_amount: Option<i16>,
    /// Camera sprite (`newowner`; `< 0` = own eyes) when the blob is long enough.
    pub newowner: Option<i16>,
    /// Pain-flash cooldown (`hurt_delay`, 30 Hz) when the blob is long enough.
    pub hurt_delay: Option<i16>,
    /// Pipebomb pin-hold (`hbomb_hold_delay`) when the blob is long enough.
    pub hbomb_hold_delay: Option<i16>,
    /// `boot_amount` when the blob is long enough.
    pub boot_amount: Option<i16>,
    /// `heat_amount` (night vision) when the blob is long enough.
    pub heat_amount: Option<i16>,
    /// Weapon fire frame (`kickback_pic`) when the blob is long enough.
    pub kickback_pic: Option<i16>,
    /// Weapon look-aside (`weapon_ang`) when the blob is long enough.
    pub weapon_ang: Option<i16>,
    /// Access-card bits (`got_access`: 1 blue, 2 red, 4 yellow) when long enough.
    pub got_access: Option<i16>,
    /// Holoduke decoy sprite (`holoduke_on`; `-1` = none) when long enough.
    pub holoduke_on: Option<i16>,
    /// Jetpack engaged (`jetpack_on`) when the blob is long enough.
    pub jetpack_on: Option<u8>,
    /// Scuba engaged (`scuba_on`) when the blob is long enough.
    pub scuba_on: Option<u8>,
    /// Night vision engaged (`heat_on`) when the blob is long enough.
    pub heat_on: Option<u8>,
    /// Mid-air fall tics (`falling_counter`, 30 Hz) when the blob is long enough.
    pub falling_counter: Option<u8>,
    /// Ready gun put away (`holster_weapon`) when the blob is long enough.
    pub holster_weapon: Option<u8>,
    /// Remaining breath (`airleft`, 30 Hz) when the blob is long enough.
    pub airleft: Option<i16>,
    /// Mid-jump (`jumping_counter`; `0` = grounded) when the blob is long enough.
    pub jumping_counter: Option<i16>,
    /// Death tilt (`dead_flag`; `0` = alive) when the blob is long enough.
    pub dead_flag: Option<i16>,
    /// Selected inventory (`inven_icon`: 1 kit … 7 boots) when long enough.
    pub inven_icon: Option<u8>,
    /// Inventory-key still held (`toggle_key_flag`) when the blob is long enough.
    pub toggle_key_flag: Option<u8>,
    /// Force inventory-name redraw (`refresh_inventory`) when the blob is long enough.
    pub refresh_inventory: Option<u8>,
    /// HUD inventory-name tics (`invdisptime`, 30 Hz) when the blob is long enough.
    pub invdisptime: Option<i32>,
    /// Standing on a floor (`on_ground`) when the blob is long enough.
    pub on_ground: Option<u8>,
    /// Empty-gun click (`show_empty_weapon`) when the blob is long enough.
    pub show_empty_weapon: Option<u8>,
    /// Held rapid fire (`rapid_fire_hold`) when the blob is long enough.
    pub rapid_fire_hold: Option<u8>,
    /// HUD palette flash RGB (`pals[3]`) when the blob is long enough.
    pub pals: Option<[u8; 3]>,
    /// HUD palette flash tics (`pals_time`, 30 Hz) when the blob is long enough.
    pub pals_time: Option<i16>,
    /// Post-teleport lock (`transporter_hold`, 30 Hz) when long enough.
    pub transporter_hold: Option<i16>,
    /// Pipebomb remote out (`hbomb_on`) when the blob is long enough.
    pub hbomb_on: Option<u8>,
    /// Jump button still held (`jumping_toggle`) when the blob is long enough.
    pub jumping_toggle: Option<u8>,
    /// HUD gun raise/lower (`weapon_pos`) when the blob is long enough.
    pub weapon_pos: Option<i16>,
    /// Last non-empty gun (`last_full_weapon`) when the blob is long enough.
    pub last_full_weapon: Option<i16>,
    /// Footprint shade (`footprintshade`) when the blob is long enough.
    pub footprintshade: Option<i16>,
    /// Remaining footprints (`footprintcount`) when the blob is long enough.
    pub footprintcount: Option<u8>,
    /// Footprint palette (`footprintpal`) when the blob is long enough.
    pub footprintpal: Option<u8>,
    /// Mid `/` kick (`quick_kick`; `0` = idle) when the blob is long enough.
    pub quick_kick: Option<i16>,
    /// Last killer player slot (`frag_ps`) when the blob is long enough.
    pub frag_ps: Option<i16>,
    /// Mid knee attack (`knee_incs`; `0` = idle) when the blob is long enough.
    pub knee_incs: Option<i16>,
    /// Mid access-card use (`access_incs`; `0` = idle) when long enough.
    pub access_incs: Option<i16>,
    /// Quote display tics (`fta`, 30 Hz) when the blob is long enough.
    pub fta: Option<i16>,
    /// Quote index (`ftq`) when the blob is long enough.
    pub ftq: Option<i16>,
    /// Wall being unlocked (`access_wallnum`; `< 0` = none) when long enough.
    pub access_wallnum: Option<i16>,
    /// Sprite being unlocked (`access_spritenum`; `< 0` = none) when long enough.
    pub access_spritenum: Option<i16>,
    /// Hard-land view dip (`hard_landing`) when the blob is long enough.
    pub hard_landing: Option<u8>,
    /// Mid knuckle-crack (`knuckle_incs`; `0` = idle) when the blob is long enough.
    pub knuckle_incs: Option<u8>,
    /// Player palette (`palookup`) when the blob is long enough.
    pub palookup: Option<u8>,
    /// Footstep phase (`walking_snd_toggle`) when the blob is long enough.
    pub walking_snd_toggle: Option<u8>,
    /// View-bob phase (`pycount`, 0..=2047) when the blob is long enough.
    pub pycount: Option<i16>,
}

impl SavPlayer {
    /// Room map XY (`x/16`, `−y/16`).
    #[must_use]
    pub fn room_xy(&self) -> (i32, i32) {
        (self.posx / 16, -self.posy / 16)
    }

    /// Room Z (`−z/256`).
    #[must_use]
    pub fn room_z(&self) -> i32 {
        -self.posz / 256
    }

    /// Room yaw degrees (converter: `−((ang+look_ang) & 2047) * 360 / 2048`).
    #[must_use]
    pub fn room_yaw(&self) -> i32 {
        let ang = i32::from(self.ang).wrapping_add(i32::from(self.look_ang.unwrap_or(0)));
        (-(ang & 2047) * 360 / 2048).rem_euclid(360)
    }

    /// Room roll degrees from dump `rotscrnang` when non-zero.
    #[must_use]
    pub fn room_rotscrnang(&self) -> Option<i16> {
        self.rotscrnang.filter(|&n| n != 0)
    }

    /// Room pitch degrees (Duke `horiz` 100 = level, plus `horizoff`).
    #[must_use]
    pub fn room_pitch(&self) -> i32 {
        self.horiz
            .saturating_add(i32::from(self.horizoff.unwrap_or(0)))
            .saturating_sub(100)
            .clamp(-90, 90)
    }

    /// Previous Room pitch from dump `ohoriz` plus `ohorizoff`.
    #[must_use]
    pub fn room_prev_pitch(&self) -> Option<i32> {
        self.ohoriz.map(|h| {
            h.saturating_add(self.ohorizoff.unwrap_or(0))
                .saturating_sub(100)
                .clamp(-90, 90)
        })
    }

    /// Pawn health from dump `last_extra` plus `extra_extra8 / 256` overheal.
    #[must_use]
    pub fn room_health(&self) -> Option<i32> {
        self.last_extra.map(|h| {
            let extra = self.extra_extra8.unwrap_or(0).max(0);
            i32::from(h) + i32::from(extra) / 256
        })
    }

    /// Room `Clip` from pistol / ripper dump slots (the larger count).
    #[must_use]
    pub fn room_clip_ammo(&self) -> Option<i32> {
        self.ammo_amount.map(|a| i32::from(a[1].max(a[3]).max(0)))
    }

    /// Room `Shell` amount from dump `ammo_amount[SHOTGUN_WEAPON]`.
    #[must_use]
    pub fn room_shell_ammo(&self) -> Option<i32> {
        self.ammo_amount.map(|a| i32::from(a[2].max(0)))
    }

    /// Room `RocketAmmo` amount from dump `ammo_amount[RPG_WEAPON]`.
    #[must_use]
    pub fn room_rocket_ammo(&self) -> Option<i32> {
        self.ammo_amount.map(|a| i32::from(a[4].max(0)))
    }

    /// Room `Cell` amount from dump `ammo_amount[SHRINKER_WEAPON]`.
    #[must_use]
    pub fn room_cell_ammo(&self) -> Option<i32> {
        self.ammo_amount.map(|a| i32::from(a[6].max(0)))
    }

    /// Room `DukePipebombAmmo` from dump `ammo_amount[HANDBOMB_WEAPON]`.
    #[must_use]
    pub fn room_pipebomb_ammo(&self) -> Option<i32> {
        self.ammo_amount.map(|a| i32::from(a[5].max(0)))
    }

    /// Room `DukeDevastatorAmmo` from dump `ammo_amount[DEVISTATOR_WEAPON]`.
    #[must_use]
    pub fn room_devastator_ammo(&self) -> Option<i32> {
        self.ammo_amount.map(|a| i32::from(a[7].max(0)))
    }

    /// Room `DukeFreezeAmmo` from dump `ammo_amount[FREEZE_WEAPON]`.
    #[must_use]
    pub fn room_freeze_ammo(&self) -> Option<i32> {
        self.ammo_amount.map(|a| i32::from(a[9].max(0)))
    }

    /// Room `DukeTripbombAmmo` from dump `ammo_amount[TRIPBOMB_WEAPON]`.
    #[must_use]
    pub fn room_tripbomb_ammo(&self) -> Option<i32> {
        self.ammo_amount.map(|a| i32::from(a[8].max(0)))
    }

    /// Room `DukeGrowAmmo` from dump `ammo_amount[GROW_WEAPON]`.
    #[must_use]
    pub fn room_grow_ammo(&self) -> Option<i32> {
        self.ammo_amount.map(|a| i32::from(a[11].max(0)))
    }

    /// Duke `curr_weapon` slot (`0` knee … `11` grow).
    #[must_use]
    pub fn room_weapon_slot(&self) -> Option<i16> {
        self.curr_weapon
    }

    /// Duke `last_weapon` slot (`0` knee … `11` grow).
    #[must_use]
    pub fn room_last_weapon_slot(&self) -> Option<i16> {
        self.last_weapon
    }

    /// Duke `wantweaponfire` slot when the player is mid-trigger (`>= 0`).
    #[must_use]
    pub fn room_wantweapon_fire(&self) -> Option<i16> {
        self.wantweaponfire.filter(|&n| n >= 0)
    }

    /// Remaining recenter tics from dump `return_to_center` (30 Hz → 35 Hz).
    #[must_use]
    pub fn room_return_to_center(&self) -> Option<u32> {
        self.return_to_center
            .filter(|&n| n > 0)
            .map(|n| u32::try_from(i32::from(n)).unwrap_or(0) * 35 / 30)
    }

    /// Remaining tip tics from dump `tipincs` (30 Hz → 35 Hz).
    #[must_use]
    pub fn room_tip_incs(&self) -> Option<u32> {
        self.tipincs
            .filter(|&n| n > 0)
            .map(|n| u32::try_from(i32::from(n)).unwrap_or(0) * 35 / 30)
    }

    /// Owned-weapon flags from dump `gotweapon`.
    #[must_use]
    pub fn room_gotweapon(&self) -> Option<[u8; 12]> {
        self.gotweapon
    }

    /// Room armor from dump `shield_amount`.
    #[must_use]
    pub fn room_armor(&self) -> Option<i32> {
        self.shield_amount.map(|n| i32::from(n.max(0)))
    }

    /// Room `DukeFirstAid` amount from dump `firstaid_amount`.
    #[must_use]
    pub fn room_firstaid(&self) -> Option<i32> {
        self.firstaid_amount.map(|n| i32::from(n.max(0)))
    }

    /// Room `DukeSteroids` amount from dump `steroids_amount`.
    #[must_use]
    pub fn room_steroids(&self) -> Option<i32> {
        self.steroids_amount.map(|n| i32::from(n.max(0)))
    }

    /// Room `DukeJetpack` amount from dump `jetpack_amount`.
    #[must_use]
    pub fn room_jetpack(&self) -> Option<i32> {
        self.jetpack_amount.map(|n| i32::from(n.max(0)))
    }

    /// Room `DukeScubaGear` amount from dump `scuba_amount`.
    #[must_use]
    pub fn room_scuba(&self) -> Option<i32> {
        self.scuba_amount.map(|n| i32::from(n.max(0)))
    }

    /// Room `DukeHoloduke` amount from dump `holoduke_amount`.
    #[must_use]
    pub fn room_holoduke(&self) -> Option<i32> {
        self.holoduke_amount.map(|n| i32::from(n.max(0)))
    }

    /// Dump sprite looked through (`newowner` when `>= 0`).
    #[must_use]
    pub fn room_newowner(&self) -> Option<usize> {
        self.newowner
            .filter(|&n| n >= 0)
            .and_then(|n| usize::try_from(n).ok())
    }

    /// Remaining pain-flash tics from dump `hurt_delay` (30 Hz → 35 Hz).
    #[must_use]
    pub fn room_hurt_delay_tics(&self) -> Option<u32> {
        self.hurt_delay
            .filter(|&n| n > 0)
            .map(|n| u32::try_from(i32::from(n)).unwrap_or(0) * 35 / 30)
    }

    /// Pipebomb wind-up from dump `hbomb_hold_delay` when `> 0` (clamped to 6).
    #[must_use]
    pub fn room_hbomb_hold(&self) -> Option<i32> {
        self.hbomb_hold_delay
            .filter(|&n| n > 0)
            .map(|n| i32::from(n).min(6))
    }

    /// Room `DukeBoots` amount from dump `boot_amount`.
    #[must_use]
    pub fn room_boots(&self) -> Option<i32> {
        self.boot_amount.map(|n| i32::from(n.max(0)))
    }

    /// Room `DukeNightVision` amount from dump `heat_amount`.
    #[must_use]
    pub fn room_heat(&self) -> Option<i32> {
        self.heat_amount.map(|n| i32::from(n.max(0)))
    }

    /// Room yaw degrees from dump `weapon_ang` (converter, 0 if unset).
    #[must_use]
    pub fn room_weapon_ang_deg(&self) -> Option<i32> {
        self.weapon_ang
            .map(|n| (-(i32::from(n) & 2047) * 360 / 2048).rem_euclid(360))
    }

    /// Dump `kickback_pic` (0 = idle, >0 = mid-shot).
    #[must_use]
    pub fn room_kickback_pic(&self) -> Option<i16> {
        self.kickback_pic
    }

    /// Whether the ready gun is mid-shot (`kickback_pic > 0`).
    #[must_use]
    pub fn room_kickback_firing(&self) -> Option<bool> {
        self.kickback_pic.map(|n| n > 0)
    }

    /// Dump `got_access` bits (1 blue, 2 red, 4 yellow).
    #[must_use]
    pub fn room_got_access(&self) -> Option<i16> {
        self.got_access
    }

    /// Blue access card from dump `got_access` bit 0.
    #[must_use]
    pub fn room_blue_card(&self) -> Option<bool> {
        self.got_access.map(|n| n & 1 != 0)
    }

    /// Red access card from dump `got_access` bit 1.
    #[must_use]
    pub fn room_red_card(&self) -> Option<bool> {
        self.got_access.map(|n| n & 2 != 0)
    }

    /// Yellow access card from dump `got_access` bit 2.
    #[must_use]
    pub fn room_yellow_card(&self) -> Option<bool> {
        self.got_access.map(|n| n & 4 != 0)
    }

    /// Whether a holoduke decoy is out (`holoduke_on >= 0`).
    #[must_use]
    pub fn room_holoduke_on(&self) -> Option<bool> {
        self.holoduke_on.map(|n| n >= 0)
    }

    /// Whether the jetpack is engaged (`jetpack_on != 0`).
    #[must_use]
    pub fn room_jetpack_on(&self) -> Option<bool> {
        self.jetpack_on.map(|n| n != 0)
    }

    /// Whether scuba is engaged (`scuba_on != 0`).
    #[must_use]
    pub fn room_scuba_on(&self) -> Option<bool> {
        self.scuba_on.map(|n| n != 0)
    }

    /// Whether night vision is engaged (`heat_on != 0`).
    #[must_use]
    pub fn room_heat_on(&self) -> Option<bool> {
        self.heat_on.map(|n| n != 0)
    }

    /// Room falling tics from dump `falling_counter` (30 Hz → 35 Hz).
    #[must_use]
    pub fn room_falling_tics(&self) -> Option<u32> {
        self.falling_counter.map(|n| u32::from(n) * 35 / 30)
    }

    /// Whether the ready gun is holstered (`holster_weapon != 0`).
    #[must_use]
    pub fn room_holster_weapon(&self) -> Option<bool> {
        self.holster_weapon.map(|n| n != 0)
    }

    /// Room air tics from dump `airleft` (30 Hz → 35 Hz).
    #[must_use]
    pub fn room_air_tics(&self) -> Option<u32> {
        self.airleft
            .map(|n| u32::try_from(i32::from(n).max(0)).unwrap_or(0) * 35 / 30)
    }

    /// Whether the pawn is mid-jump (`jumping_counter` in `1..=1279`).
    #[must_use]
    pub fn room_jumping(&self) -> Option<bool> {
        self.jumping_counter.map(|n| n > 0 && n < 1280)
    }

    /// Whether the pawn is dead (`dead_flag != 0`).
    #[must_use]
    pub fn room_dead(&self) -> Option<bool> {
        self.dead_flag.map(|n| n != 0)
    }

    /// Duke selected inventory (`inven_icon` when `1..=7`).
    #[must_use]
    pub fn room_inven_icon(&self) -> Option<u8> {
        self.inven_icon.filter(|&n| (1..=7).contains(&n))
    }

    /// Whether the pawn is standing (`on_ground != 0`).
    #[must_use]
    pub fn room_on_ground(&self) -> Option<bool> {
        self.on_ground.map(|n| n != 0)
    }

    /// Whether the ready gun is mid empty-click (`show_empty_weapon != 0`).
    #[must_use]
    pub fn room_show_empty_weapon(&self) -> Option<bool> {
        self.show_empty_weapon.map(|n| n != 0)
    }

    /// Whether rapid fire is held (`rapid_fire_hold != 0`).
    #[must_use]
    pub fn room_rapid_fire_hold(&self) -> Option<bool> {
        self.rapid_fire_hold.map(|n| n != 0)
    }

    /// Remaining HUD flash from dump `pals_time` / `pals` (30 Hz → 35 Hz).
    #[must_use]
    pub fn room_pals_flash(&self) -> Option<i32> {
        if let Some(n) = self.pals_time.filter(|&n| n > 0) {
            return Some((i32::from(n) * 35 / 30).clamp(1, 100));
        }
        self.pals
            .and_then(|p| p.into_iter().max())
            .filter(|&n| n > 0)
            .map(|n| i32::from(n).clamp(1, 100))
    }

    /// Room lock tics from dump `transporter_hold` (30 Hz → 35 Hz).
    #[must_use]
    pub fn room_transporter_hold_tics(&self) -> Option<u32> {
        self.transporter_hold
            .map(|n| u32::try_from(i32::from(n).max(0)).unwrap_or(0) * 35 / 30)
    }

    /// Whether the pipebomb remote is out (`hbomb_on != 0`).
    #[must_use]
    pub fn room_hbomb_on(&self) -> Option<bool> {
        self.hbomb_on.map(|n| n != 0)
    }

    /// Whether jump is still held (`jumping_toggle != 0`).
    #[must_use]
    pub fn room_jumping_toggle(&self) -> Option<bool> {
        self.jumping_toggle.map(|n| n != 0)
    }

    /// Room `pspr_y` from dump `weapon_pos` (`0` = ready at 32, `|n|·8` down).
    #[must_use]
    pub fn room_pspr_y(&self) -> Option<f64> {
        self.weapon_pos.map(|n| {
            let lowered = f64::from(i32::from(n).unsigned_abs().min(12));
            (32.0 + lowered * 8.0).clamp(32.0, 128.0)
        })
    }

    /// Duke `last_full_weapon` slot (`0` knee … `11` grow) when `>= 0`.
    #[must_use]
    pub fn room_last_full_weapon_slot(&self) -> Option<i16> {
        self.last_full_weapon.filter(|&n| n >= 0)
    }

    /// Whether a `/` kick is in progress (`quick_kick > 0`).
    #[must_use]
    pub fn room_quick_kick(&self) -> Option<bool> {
        self.quick_kick.map(|n| n > 0)
    }

    /// Killer player slot from dump `frag_ps` (`0..=7`).
    #[must_use]
    pub fn room_frag_ps(&self) -> Option<u8> {
        self.frag_ps
            .filter(|&n| (0..8).contains(&n))
            .and_then(|n| u8::try_from(n).ok())
    }

    /// Knee-fire frame from dump `knee_incs` when mid-attack (`> 0`).
    #[must_use]
    pub fn room_knee_incs(&self) -> Option<i16> {
        self.knee_incs.filter(|&n| n > 0)
    }

    /// Whether an access-card use is in progress (`access_incs > 0`).
    #[must_use]
    pub fn room_access_incs(&self) -> Option<bool> {
        self.access_incs.map(|n| n > 0)
    }

    /// Room quote tics from dump `fta` (30 Hz → 35 Hz) when a quote is up.
    #[must_use]
    pub fn room_fta_tics(&self) -> Option<u32> {
        self.fta
            .filter(|&n| n > 0)
            .map(|n| u32::try_from(i32::from(n)).unwrap_or(0) * 35 / 30)
    }

    /// Duke quote index from dump `ftq` when `fta` is showing it.
    #[must_use]
    pub fn room_ftq(&self) -> Option<i16> {
        match (self.room_fta_tics(), self.ftq) {
            (Some(_), Some(q)) if q >= 0 => Some(q),
            _ => None,
        }
    }

    /// Dump wall index being unlocked (`access_wallnum` when `>= 0`).
    #[must_use]
    pub fn room_access_wallnum(&self) -> Option<usize> {
        self.access_wallnum
            .filter(|&n| n >= 0)
            .and_then(|n| usize::try_from(n).ok())
    }

    /// Dump sprite index being unlocked (`access_spritenum` when `>= 0`).
    #[must_use]
    pub fn room_access_spritenum(&self) -> Option<usize> {
        self.access_spritenum
            .filter(|&n| n >= 0)
            .and_then(|n| usize::try_from(n).ok())
    }

    /// Hard-land dip from dump `hard_landing` when `> 0`.
    #[must_use]
    pub fn room_hard_landing(&self) -> Option<u8> {
        self.hard_landing.filter(|&n| n > 0)
    }

    /// Whether a knuckle-crack is mid-pose (`knuckle_incs > 0`).
    #[must_use]
    pub fn room_knuckle_incs(&self) -> Option<bool> {
        self.knuckle_incs.map(|n| n > 0)
    }

    /// Player palette index from dump `palookup`.
    #[must_use]
    pub fn room_palookup(&self) -> Option<u8> {
        self.palookup
    }

    /// Footstep phase from dump `walking_snd_toggle` (`!= 0` = other foot).
    #[must_use]
    pub fn room_walking_snd_toggle(&self) -> Option<bool> {
        self.walking_snd_toggle.map(|n| n != 0)
    }

    /// Inventory-key hold from dump `toggle_key_flag` (`!= 0` = still held).
    #[must_use]
    pub fn room_toggle_key_flag(&self) -> Option<bool> {
        self.toggle_key_flag.map(|n| n != 0)
    }

    /// Whether dump `refresh_inventory` should show the inventory name.
    #[must_use]
    pub fn room_refresh_inventory(&self) -> Option<bool> {
        self.refresh_inventory.map(|n| n != 0)
    }

    /// HUD inventory-name tics from dump `invdisptime` (30 Hz → 35 Hz) when `> 0`.
    #[must_use]
    pub fn room_invdisp_tics(&self) -> Option<u32> {
        self.invdisptime
            .filter(|&n| n > 0)
            .map(|n| u32::try_from(n).unwrap_or(0) * 35 / 30)
    }

    /// View-bob phase from dump `pycount` (wrapped to 0..=2047).
    #[must_use]
    pub fn room_pycount(&self) -> Option<i16> {
        self.pycount.map(|n| n & 2047)
    }

    /// Footprint shade from dump `footprintshade`.
    #[must_use]
    pub fn room_footprint_shade(&self) -> Option<i16> {
        self.footprintshade
    }

    /// Remaining footprints from dump `footprintcount`.
    #[must_use]
    pub fn room_footprint_count(&self) -> Option<u8> {
        self.footprintcount
    }

    /// Footprint palette from dump `footprintpal`.
    #[must_use]
    pub fn room_footprint_pal(&self) -> Option<u8> {
        self.footprintpal
    }
}

/// Parsed `game0.sav` … header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavHeader {
    /// `BYTEVERSION` from the file.
    pub version: i32,
    /// Player count written with the save.
    pub players: i32,
    /// 19-byte slot name (trimmed).
    pub name: String,
    /// Volume, 0-based.
    pub volume: i32,
    /// Level, 0-based.
    pub level: i32,
    /// Skill 0..=4.
    pub skill: i32,
    /// Dump prefix: `numwalls` (`short`) when the board blob is present.
    pub numwalls: Option<i32>,
    /// Dump prefix: first `numwalls` `walltype` records when the blob is long enough.
    pub walls: Option<Vec<SavWall>>,
    /// Dump prefix: `numsectors` (`short`) after the walls blob.
    pub numsectors: Option<i32>,
    /// Dump prefix: first `numsectors` `sectortype` records when the blob is long enough.
    pub sectors: Option<Vec<SavSector>>,
    /// Dump prefix: `numsprites` (`short`) after the sectors blob.
    pub numsprites: Option<i32>,
    /// Dump prefix: first `numsprites` `spritetype` records when the blob is long enough.
    pub sprites: Option<Vec<SavSprite>>,
    /// Dump prefix: `numcyclers` after the sprite link lists.
    pub numcyclers: Option<i32>,
    /// Dump prefix: first `ps[]` pose when the blob is long enough.
    pub player: Option<SavPlayer>,
    /// Dump prefix: `numanimwalls` after the cyclers / `ps[]` / `po[]` blobs.
    pub numanimwalls: Option<i32>,
    /// Dump prefix: `spriteqamount` after animwalls / mirrors / `spriteqloc`.
    pub spriteqamount: Option<i32>,
    /// Dump prefix: `numclouds` after `spriteqamount`.
    pub numclouds: Option<i32>,
    /// Dump prefix: `randomseed` after the cloud arrays.
    pub randomseed: Option<i32>,
    /// Dump prefix: `lockclock` (Duke 30 Hz tics) after `randomseed`.
    pub lockclock: Option<i32>,
    /// Dump prefix: `pskybits` (`short`) after `lockclock`.
    pub pskybits: Option<i32>,
    /// Dump prefix: eight `pskyoff` slots (`short[8]`) after `pskybits`.
    pub pskyoff: Option<[i16; 8]>,
    /// Dump prefix: `animatecnt` (`short`) after `pskyoff`.
    pub animatecnt: Option<i32>,
    /// Dump prefix: first `animatecnt` `animategoal` slots (`long[64]` blob).
    pub animategoal: Option<Vec<i32>>,
    /// Dump prefix: first `animatecnt` `animatevel` slots (`long[64]` blob).
    pub animatevel: Option<Vec<i32>>,
    /// Dump prefix: first `animatecnt` `animateptr` slots (`long[64]` blob).
    pub animateptr: Option<Vec<i32>>,
    /// Dump prefix: `earthquaketime` (`char`) after `animateptr`.
    pub earthquaketime: Option<i32>,
    /// Dump prefix: `ud.from_bonus` (`char`) after `earthquaketime`.
    pub from_bonus: Option<i32>,
    /// Dump prefix: `ud.secretlevel` (`char`) after `from_bonus`.
    pub secretlevel: Option<i32>,
    /// Dump prefix: `ud.respawn_monsters` (`char`) after `secretlevel`.
    pub respawn_monsters: Option<i32>,
    /// Dump prefix: `ud.respawn_items` (`char`) after `respawn_monsters`.
    pub respawn_items: Option<i32>,
    /// Dump prefix: `ud.respawn_inventory` (`char`) after `respawn_items`.
    pub respawn_inventory: Option<i32>,
    /// Dump prefix: `ud.god` (`char`) after `respawn_inventory`.
    pub god: Option<i32>,
    /// Dump prefix: `ud.auto_run` (`char`) after `god`.
    pub auto_run: Option<i32>,
    /// Dump prefix: `ud.crosshair` (`char`) after `auto_run`.
    pub crosshair: Option<i32>,
    /// Dump prefix: `ud.monsters_off` (`char`) after `crosshair`.
    pub monsters_off: Option<i32>,
    /// Dump prefix: `ud.last_level` (`char`) after `monsters_off`.
    pub last_level: Option<i32>,
    /// Dump prefix: `ud.eog` (`char`) after `last_level`.
    pub eog: Option<i32>,
    /// Dump prefix: `ud.coop` (`char`) after `eog`.
    pub coop: Option<i32>,
    /// Dump prefix: `ud.marker` (`char`) after `coop`.
    pub marker: Option<i32>,
    /// Dump prefix: `ud.ffire` (`char`) after `marker`.
    pub ffire: Option<i32>,
    /// Dump prefix: `camsprite` (`short`) after `ffire`.
    pub camsprite: Option<i32>,
    /// Dump prefix: `connecthead` (`short`) after `camsprite`.
    pub connecthead: Option<i32>,
    /// Dump prefix: sixteen `connectpoint2` slots (`short[16]`) after `connecthead`.
    pub connectpoint2: Option<[i16; 16]>,
    /// Dump prefix: `numplayersprites` (`short`) after `connectpoint2`.
    pub numplayersprites: Option<i32>,
    /// Dump prefix: `frags[16][16]` (`short`) after `numplayersprites`.
    pub frags: Option<[[i16; 16]; 16]>,
    /// Dump prefix: `global_random` (`short`) after `frags`.
    pub global_random: Option<i32>,
    /// Dump prefix: `parallaxyscale` (`long`) after `global_random`.
    pub parallaxyscale: Option<i32>,
    /// Dump after prefix: `gotpic` tile-seen bits.
    pub gotpic: Option<Vec<u8>>,
    /// Dump after prefix: `playerswhenstarted`.
    pub playerswhenstarted: Option<i32>,
}

impl SavHeader {
    /// Room map lump (`E1M1` …).
    #[must_use]
    pub fn map_name(&self) -> String {
        format!(
            "E{}M{}",
            self.volume.saturating_add(1),
            self.level.saturating_add(1)
        )
    }

    /// Room skill 1..=5 (Duke 0..=4 plus one).
    #[must_use]
    pub fn room_skill(&self) -> u8 {
        u8::try_from(self.skill.saturating_add(1))
            .unwrap_or(1)
            .clamp(1, 5)
    }

    /// Room `rng_seed` from the dump `randomseed` (`long` bits).
    #[must_use]
    pub fn room_rng_seed(&self) -> Option<u32> {
        self.randomseed.map(|n| u32::from_le_bytes(n.to_le_bytes()))
    }

    /// Room map tics from dump `lockclock` (Duke 30 Hz → Room 35 Hz).
    #[must_use]
    pub fn room_lock_tics(&self) -> Option<u32> {
        let n = self.lockclock?;
        if n < 0 {
            return None;
        }
        let room = u64::from(u32::from_le_bytes(n.to_le_bytes())) * 35 / 30;
        u32::try_from(room).ok()
    }

    /// Remaining quake tics from dump `earthquaketime` (Duke 30 Hz → Room 35 Hz).
    #[must_use]
    pub fn room_quake_tics(&self) -> Option<u32> {
        let n = self.earthquaketime?;
        if n <= 0 {
            return None;
        }
        let room = u64::from(u32::from_le_bytes(n.to_le_bytes())) * 35 / 30;
        u32::try_from(room).ok()
    }

    /// Room `respawn_monsters` from dump `ud.respawn_monsters`.
    #[must_use]
    pub fn room_respawn_monsters(&self) -> Option<bool> {
        self.respawn_monsters.map(|n| n != 0)
    }

    /// Room item-respawn flag from dump `ud.respawn_items`.
    #[must_use]
    pub fn room_respawn_items(&self) -> Option<bool> {
        self.respawn_items.map(|n| n != 0)
    }

    /// Room inventory-respawn flag from dump `ud.respawn_inventory`.
    #[must_use]
    pub fn room_respawn_inventory(&self) -> Option<bool> {
        self.respawn_inventory.map(|n| n != 0)
    }

    /// Room player god flag from dump `ud.god`.
    #[must_use]
    pub fn room_god(&self) -> Option<bool> {
        self.god.map(|n| n != 0)
    }

    /// Room `cl_run` from dump `ud.auto_run`.
    #[must_use]
    pub fn room_auto_run(&self) -> Option<bool> {
        self.auto_run.map(|n| n != 0)
    }

    /// Came-from-secret flag from dump `ud.from_bonus`.
    #[must_use]
    pub fn room_from_bonus(&self) -> Option<bool> {
        self.from_bonus.map(|n| n != 0)
    }

    /// Pending secret level from dump `ud.secretlevel`.
    #[must_use]
    pub fn room_secretlevel(&self) -> Option<i32> {
        self.secretlevel
    }

    /// Crosshair-on flag from dump `ud.crosshair`.
    #[must_use]
    pub fn room_crosshair(&self) -> Option<bool> {
        self.crosshair.map(|n| n != 0)
    }

    /// No-monsters flag from dump `ud.monsters_off`.
    #[must_use]
    pub fn room_monsters_off(&self) -> Option<bool> {
        self.monsters_off.map(|n| n != 0)
    }

    /// End-of-game flag from dump `ud.eog`.
    #[must_use]
    pub fn room_eog(&self) -> Option<bool> {
        self.eog.map(|n| n != 0)
    }

    /// Cooperative-play flag from dump `ud.coop`.
    #[must_use]
    pub fn room_coop(&self) -> Option<bool> {
        self.coop.map(|n| n != 0)
    }

    /// Respawn-marker flag from dump `ud.marker`.
    #[must_use]
    pub fn room_marker(&self) -> Option<bool> {
        self.marker.map(|n| n != 0)
    }

    /// Friendly-fire flag from dump `ud.ffire`.
    #[must_use]
    pub fn room_ffire(&self) -> Option<bool> {
        self.ffire.map(|n| n != 0)
    }

    /// Active camera sprite from dump `camsprite` (`-1` = none).
    #[must_use]
    pub fn room_camsprite(&self) -> Option<i32> {
        self.camsprite
    }

    /// Multiplayer connect-list head from dump `connecthead`.
    #[must_use]
    pub fn room_connecthead(&self) -> Option<i32> {
        self.connecthead
    }

    /// Multiplayer connect-list next links from dump `connectpoint2`.
    #[must_use]
    pub fn room_connectpoint2(&self) -> Option<[i16; 16]> {
        self.connectpoint2
    }

    /// Player-sprite count from dump `numplayersprites`.
    #[must_use]
    pub fn room_numplayersprites(&self) -> Option<i32> {
        self.numplayersprites
    }

    /// DukeMatch frag table from dump `frags`.
    #[must_use]
    pub fn room_frags(&self) -> Option<[[i16; 16]; 16]> {
        self.frags
    }

    /// Script RNG from dump `global_random`.
    #[must_use]
    pub fn room_global_random(&self) -> Option<i32> {
        self.global_random
    }

    /// Sky parallax scale from dump `parallaxyscale`.
    #[must_use]
    pub fn room_parallaxyscale(&self) -> Option<i32> {
        self.parallaxyscale
    }

    /// Previous level index from dump `ud.last_level`.
    #[must_use]
    pub fn room_last_level(&self) -> Option<i32> {
        self.last_level
    }

    /// Duke `pskybits` from the dump.
    #[must_use]
    pub fn room_pskybits(&self) -> Option<i32> {
        self.pskybits
    }

    /// Duke `pskyoff[8]` from the dump.
    #[must_use]
    pub fn room_pskyoff(&self) -> Option<[i16; 8]> {
        self.pskyoff
    }

    /// Duke `animatecnt` from the dump.
    #[must_use]
    pub fn room_animatecnt(&self) -> Option<i32> {
        self.animatecnt
    }

    /// Duke `numanimwalls` from the dump.
    #[must_use]
    pub fn room_numanimwalls(&self) -> Option<i32> {
        self.numanimwalls
    }

    /// Duke `spriteqamount` from the dump.
    #[must_use]
    pub fn room_spriteqamount(&self) -> Option<i32> {
        self.spriteqamount
    }

    /// Duke `numclouds` from the dump.
    #[must_use]
    pub fn room_numclouds(&self) -> Option<i32> {
        self.numclouds
    }

    /// Duke `numcyclers` from the dump.
    #[must_use]
    pub fn room_numcyclers(&self) -> Option<i32> {
        self.numcyclers
    }

    /// Tile-seen bits from dump `gotpic`.
    #[must_use]
    pub fn room_gotpic(&self) -> Option<&[u8]> {
        self.gotpic.as_deref()
    }

    /// Seat count from dump `playerswhenstarted`.
    #[must_use]
    pub fn room_playerswhenstarted(&self) -> Option<i32> {
        self.playerswhenstarted
    }

    /// Parse the `loadpheader` prefix of a Duke `.SAV`.
    pub fn parse(bytes: &[u8]) -> Result<Self, BuildError> {
        let mut pos = 0usize;
        let version = i32_le(kdfread_one(bytes, &mut pos, 4).ok_or(BuildError::Truncated("SAV"))?)?;
        let players = i32_le(kdfread_one(bytes, &mut pos, 4).ok_or(BuildError::Truncated("SAV"))?)?;
        let name_raw = kdfread_one(bytes, &mut pos, 19).ok_or(BuildError::Truncated("SAV"))?;
        let volume = i32_le(kdfread_one(bytes, &mut pos, 4).ok_or(BuildError::Truncated("SAV"))?)?;
        let level = i32_le(kdfread_one(bytes, &mut pos, 4).ok_or(BuildError::Truncated("SAV"))?)?;
        let skill = i32_le(kdfread_one(bytes, &mut pos, 4).ok_or(BuildError::Truncated("SAV"))?)?;
        if !(0..=3).contains(&volume) || !(0..=15).contains(&level) {
            return Err(BuildError::Invalid("SAV map"));
        }
        if !(1..=8).contains(&players) {
            return Err(BuildError::Invalid("SAV players"));
        }
        let name = name_raw
            .split(|&b| b == 0)
            .next()
            .unwrap_or(&[])
            .iter()
            .copied()
            .filter(|&b| (32..=126).contains(&b))
            .map(char::from)
            .collect();
        let numwalls = kdfread_one(bytes, &mut pos, 2).and_then(|b| {
            let n = i16::from_le_bytes([b[0], b[1]]);
            (0..=16_384).contains(&n).then_some(i32::from(n))
        });
        let wall_blob = if numwalls.is_some() {
            kdfread_blob(bytes, &mut pos)
        } else {
            None
        };
        let walls = match (numwalls, wall_blob.as_deref()) {
            (Some(n), Some(b)) => parse_sav_walls(n, b),
            _ => None,
        };
        let numsectors = if wall_blob.is_some() {
            kdfread_one(bytes, &mut pos, 2).and_then(|b| {
                let n = i16::from_le_bytes([b[0], b[1]]);
                (0..=4096).contains(&n).then_some(i32::from(n))
            })
        } else {
            None
        };
        let sector_blob = if numsectors.is_some() {
            kdfread_blob(bytes, &mut pos)
        } else {
            None
        };
        let sectors = match (numsectors, sector_blob.as_deref()) {
            (Some(n), Some(b)) => parse_sav_sectors(n, b),
            _ => None,
        };
        let numsprites = if sector_blob.is_some() {
            kdfread_one(bytes, &mut pos, 2).and_then(|b| {
                let n = i16::from_le_bytes([b[0], b[1]]);
                (0..=16_384).contains(&n).then_some(i32::from(n))
            })
        } else {
            None
        };
        let sprite_blob = if numsprites.is_some() {
            kdfread_blob(bytes, &mut pos)
        } else {
            None
        };
        let sprites = match (numsprites, sprite_blob.as_deref()) {
            (Some(n), Some(b)) => parse_sav_sprites(n, b),
            _ => None,
        };
        // head/prev/next spritesect and spritestat (6 lists).
        let numcyclers = if sprite_blob.is_some() && (0..6).all(|_| kdfskip_one(bytes, &mut pos)) {
            kdfread_one(bytes, &mut pos, 2).and_then(|b| {
                let n = i16::from_le_bytes([b[0], b[1]]);
                (0..=256).contains(&n).then_some(i32::from(n))
            })
        } else {
            None
        };
        // cyclers[][], then ps[] / po[], then numanimwalls.
        let cyclers_ok = numcyclers.is_some() && kdfskip_one(bytes, &mut pos);
        let ps_blob = if cyclers_ok {
            kdfread_blob(bytes, &mut pos)
        } else {
            None
        };
        let player = ps_blob.as_deref().and_then(parse_sav_player);
        let numanimwalls = if ps_blob.is_some() && kdfskip_one(bytes, &mut pos) {
            kdfread_one(bytes, &mut pos, 2).and_then(|b| {
                let n = i16::from_le_bytes([b[0], b[1]]);
                (0..=1024).contains(&n).then_some(i32::from(n))
            })
        } else {
            None
        };
        // animwall[], msx[], msy[], spriteqloc[], then spriteqamount.
        let spriteqamount =
            if numanimwalls.is_some() && (0..4).all(|_| kdfskip_one(bytes, &mut pos)) {
                kdfread_one(bytes, &mut pos, 2).and_then(|b| {
                    let n = i16::from_le_bytes([b[0], b[1]]);
                    (0..=1024).contains(&n).then_some(i32::from(n))
                })
            } else {
                None
            };
        let numclouds = if spriteqamount.is_some() {
            kdfread_one(bytes, &mut pos, 2).and_then(|b| {
                let n = i16::from_le_bytes([b[0], b[1]]);
                (0..=128).contains(&n).then_some(i32::from(n))
            })
        } else {
            None
        };
        // clouds[], cloudx[], cloudy[], then randomseed (long).
        let randomseed = if numclouds.is_some() && (0..3).all(|_| kdfskip_one(bytes, &mut pos)) {
            kdfread_one(bytes, &mut pos, 4).and_then(|b| {
                let a: [u8; 4] = b.try_into().ok()?;
                Some(i32::from_le_bytes(a))
            })
        } else {
            None
        };
        let lockclock = if randomseed.is_some() {
            kdfread_one(bytes, &mut pos, 4).and_then(|b| {
                let a: [u8; 4] = b.try_into().ok()?;
                Some(i32::from_le_bytes(a))
            })
        } else {
            None
        };
        let pskybits = if lockclock.is_some() {
            kdfread_one(bytes, &mut pos, 2).and_then(|b| {
                let n = i16::from_le_bytes([b[0], b[1]]);
                (0..=16).contains(&n).then_some(i32::from(n))
            })
        } else {
            None
        };
        let pskyoff = if pskybits.is_some() {
            kdfread_one(bytes, &mut pos, 16).and_then(|b| {
                if b.len() < 16 {
                    return None;
                }
                let mut off = [0i16; 8];
                for (i, slot) in off.iter_mut().enumerate() {
                    let n = i * 2;
                    *slot = i16::from_le_bytes([b[n], b[n + 1]]);
                }
                Some(off)
            })
        } else {
            None
        };
        let animatecnt = if pskyoff.is_some() {
            kdfread_one(bytes, &mut pos, 2).and_then(|b| {
                let n = i16::from_le_bytes([b[0], b[1]]);
                (0..=64).contains(&n).then_some(i32::from(n))
            })
        } else {
            None
        };
        let animategoal = if let Some(n) = animatecnt {
            kdfread_one(bytes, &mut pos, 256).and_then(|b| {
                let take = usize::try_from(n).ok()?;
                if b.len() < take.saturating_mul(4) {
                    return None;
                }
                let mut out = Vec::with_capacity(take);
                for i in 0..take {
                    let o = i * 4;
                    out.push(i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]));
                }
                Some(out)
            })
        } else {
            None
        };
        let animatevel = if animategoal.is_some() {
            if let Some(n) = animatecnt {
                kdfread_one(bytes, &mut pos, 256).and_then(|b| {
                    let take = usize::try_from(n).ok()?;
                    if b.len() < take.saturating_mul(4) {
                        return None;
                    }
                    let mut out = Vec::with_capacity(take);
                    for i in 0..take {
                        let o = i * 4;
                        out.push(i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]));
                    }
                    Some(out)
                })
            } else {
                None
            }
        } else {
            None
        };
        let animateptr = if animatevel.is_some() {
            if let Some(n) = animatecnt {
                kdfread_one(bytes, &mut pos, 256).and_then(|b| {
                    let take = usize::try_from(n).ok()?;
                    if b.len() < take.saturating_mul(4) {
                        return None;
                    }
                    let mut out = Vec::with_capacity(take);
                    for i in 0..take {
                        let o = i * 4;
                        out.push(i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]));
                    }
                    Some(out)
                })
            } else {
                None
            }
        } else {
            None
        };
        let earthquaketime = if animateptr.is_some() {
            kdfread_one(bytes, &mut pos, 1).and_then(|b| b.first().copied().map(i32::from))
        } else {
            None
        };
        let from_bonus = if earthquaketime.is_some() {
            kdfread_one(bytes, &mut pos, 1).and_then(|b| b.first().copied().map(i32::from))
        } else {
            None
        };
        let secretlevel = if from_bonus.is_some() {
            kdfread_one(bytes, &mut pos, 1).and_then(|b| b.first().copied().map(i32::from))
        } else {
            None
        };
        let respawn_monsters = if secretlevel.is_some() {
            kdfread_one(bytes, &mut pos, 1).and_then(|b| b.first().copied().map(i32::from))
        } else {
            None
        };
        let respawn_items = if respawn_monsters.is_some() {
            kdfread_one(bytes, &mut pos, 1).and_then(|b| b.first().copied().map(i32::from))
        } else {
            None
        };
        let respawn_inventory = if respawn_items.is_some() {
            kdfread_one(bytes, &mut pos, 1).and_then(|b| b.first().copied().map(i32::from))
        } else {
            None
        };
        let god = if respawn_inventory.is_some() {
            kdfread_one(bytes, &mut pos, 1).and_then(|b| b.first().copied().map(i32::from))
        } else {
            None
        };
        let auto_run = if god.is_some() {
            kdfread_one(bytes, &mut pos, 1).and_then(|b| b.first().copied().map(i32::from))
        } else {
            None
        };
        let crosshair = if auto_run.is_some() {
            kdfread_one(bytes, &mut pos, 1).and_then(|b| b.first().copied().map(i32::from))
        } else {
            None
        };
        let monsters_off = if crosshair.is_some() {
            kdfread_one(bytes, &mut pos, 1).and_then(|b| b.first().copied().map(i32::from))
        } else {
            None
        };
        let last_level = if monsters_off.is_some() {
            kdfread_one(bytes, &mut pos, 1).and_then(|b| b.first().copied().map(i32::from))
        } else {
            None
        };
        let eog = if last_level.is_some() {
            kdfread_one(bytes, &mut pos, 1).and_then(|b| b.first().copied().map(i32::from))
        } else {
            None
        };
        let coop = if eog.is_some() {
            kdfread_one(bytes, &mut pos, 1).and_then(|b| b.first().copied().map(i32::from))
        } else {
            None
        };
        let marker = if coop.is_some() {
            kdfread_one(bytes, &mut pos, 1).and_then(|b| b.first().copied().map(i32::from))
        } else {
            None
        };
        let ffire = if marker.is_some() {
            kdfread_one(bytes, &mut pos, 1).and_then(|b| b.first().copied().map(i32::from))
        } else {
            None
        };
        let camsprite = if ffire.is_some() {
            kdfread_one(bytes, &mut pos, 2).and_then(|b| {
                let n = i16::from_le_bytes([b[0], b[1]]);
                Some(i32::from(n))
            })
        } else {
            None
        };
        let connecthead = if camsprite.is_some() {
            kdfread_one(bytes, &mut pos, 2).and_then(|b| {
                let n = i16::from_le_bytes([b[0], b[1]]);
                Some(i32::from(n))
            })
        } else {
            None
        };
        let connectpoint2 = if connecthead.is_some() {
            kdfread_one(bytes, &mut pos, 32).and_then(|b| {
                if b.len() < 32 {
                    return None;
                }
                let mut next = [-1i16; 16];
                for (i, slot) in next.iter_mut().enumerate() {
                    let n = i * 2;
                    *slot = i16::from_le_bytes([b[n], b[n + 1]]);
                }
                Some(next)
            })
        } else {
            None
        };
        let numplayersprites = if connectpoint2.is_some() {
            kdfread_one(bytes, &mut pos, 2).and_then(|b| {
                let n = i16::from_le_bytes([b[0], b[1]]);
                (0..=16).contains(&n).then_some(i32::from(n))
            })
        } else {
            None
        };
        let frags = if numplayersprites.is_some() {
            kdfread_one(bytes, &mut pos, 512).and_then(|b| {
                if b.len() < 512 {
                    return None;
                }
                let mut table = [[0i16; 16]; 16];
                for (row, slots) in table.iter_mut().enumerate() {
                    for (col, slot) in slots.iter_mut().enumerate() {
                        let n = (row * 16 + col) * 2;
                        *slot = i16::from_le_bytes([b[n], b[n + 1]]);
                    }
                }
                Some(table)
            })
        } else {
            None
        };
        let global_random = if frags.is_some() {
            kdfread_one(bytes, &mut pos, 2).and_then(|b| {
                let n = i16::from_le_bytes([b[0], b[1]]);
                Some(i32::from(n))
            })
        } else {
            None
        };
        let parallaxyscale = if global_random.is_some() {
            kdfread_one(bytes, &mut pos, 4).and_then(|b| {
                let a: [u8; 4] = b.try_into().ok()?;
                Some(i32::from_le_bytes(a))
            })
        } else {
            None
        };
        let after = if parallaxyscale.is_some() {
            kdfread_blob(bytes, &mut pos)
        } else {
            None
        };
        let (gotpic, playerswhenstarted) = match after {
            Some(b) if b.len() >= 32 => {
                let pws =
                    kdfread_one(bytes, &mut pos, 1).and_then(|p| p.first().copied().map(i32::from));
                (Some(b), pws)
            }
            Some(b) if (1..=4).contains(&b.len()) => {
                let n = if b.len() == 1 {
                    i32::from(b[0])
                } else if b.len() == 2 {
                    i32::from(i16::from_le_bytes([b[0], b[1]]))
                } else if let Ok(a) = <[u8; 4]>::try_from(b.as_slice()) {
                    i32::from_le_bytes(a)
                } else {
                    0
                };
                (None, Some(n))
            }
            _ => (None, None),
        };
        Ok(Self {
            version,
            players,
            name,
            volume,
            level,
            skill,
            numwalls,
            walls,
            numsectors,
            sectors,
            numsprites,
            sprites,
            numcyclers,
            player,
            numanimwalls,
            spriteqamount,
            numclouds,
            randomseed,
            lockclock,
            pskybits,
            pskyoff,
            animatecnt,
            animategoal,
            animatevel,
            animateptr,
            earthquaketime,
            from_bonus,
            secretlevel,
            respawn_monsters,
            respawn_items,
            respawn_inventory,
            god,
            auto_run,
            crosshair,
            monsters_off,
            last_level,
            eog,
            coop,
            marker,
            ffire,
            camsprite,
            connecthead,
            connectpoint2,
            numplayersprites,
            frags,
            global_random,
            parallaxyscale,
            gotpic,
            playerswhenstarted,
        })
    }
}

/// What [`identify_save`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadedSave {
    /// Room `.fsave` ZIP.
    Room,
    /// Duke `game#.sav` header (map + optional dump counts; dump is not restored).
    Duke(SavHeader),
}

/// ZIP `.fsave` or a Duke `loadpheader`.
pub fn identify_save(bytes: &[u8]) -> Result<LoadedSave, BuildError> {
    if bytes.len() >= 2 && bytes[0] == b'P' && bytes[1] == b'K' {
        return Ok(LoadedSave::Room);
    }
    SavHeader::parse(bytes).map(LoadedSave::Duke)
}

fn parse_sav_walls(numwalls: i32, blob: &[u8]) -> Option<Vec<SavWall>> {
    let n = usize::try_from(numwalls).ok()?;
    let need = n.saturating_mul(32);
    if blob.len() < need {
        return None;
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let o = i * 32;
        let x = i32::from_le_bytes(blob[o..o + 4].try_into().ok()?);
        let y = i32::from_le_bytes(blob[o + 4..o + 8].try_into().ok()?);
        out.push(SavWall {
            x,
            y,
            point2: i16::from_le_bytes([blob[o + 8], blob[o + 9]]),
            nextwall: i16::from_le_bytes([blob[o + 10], blob[o + 11]]),
            nextsector: i16::from_le_bytes([blob[o + 12], blob[o + 13]]),
            cstat: i16::from_le_bytes([blob[o + 14], blob[o + 15]]),
            picnum: i16::from_le_bytes([blob[o + 16], blob[o + 17]]),
            overpicnum: i16::from_le_bytes([blob[o + 18], blob[o + 19]]),
            shade: blob[o + 20] as i8,
            pal: blob[o + 21],
            xrepeat: blob[o + 22],
            yrepeat: blob[o + 23],
            xpanning: blob[o + 24],
            ypanning: blob[o + 25],
            lotag: i16::from_le_bytes([blob[o + 26], blob[o + 27]]),
            hitag: i16::from_le_bytes([blob[o + 28], blob[o + 29]]),
            extra: i16::from_le_bytes([blob[o + 30], blob[o + 31]]),
        });
    }
    Some(out)
}

fn parse_sav_sectors(numsectors: i32, blob: &[u8]) -> Option<Vec<SavSector>> {
    let n = usize::try_from(numsectors).ok()?;
    let need = n.saturating_mul(40);
    if blob.len() < need {
        return None;
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let o = i * 40;
        let ceilingz = i32::from_le_bytes(blob[o + 4..o + 8].try_into().ok()?);
        let floorz = i32::from_le_bytes(blob[o + 8..o + 12].try_into().ok()?);
        out.push(SavSector {
            wallptr: i16::from_le_bytes([blob[o], blob[o + 1]]),
            wallnum: i16::from_le_bytes([blob[o + 2], blob[o + 3]]),
            ceilingz,
            floorz,
            ceilingstat: i16::from_le_bytes([blob[o + 12], blob[o + 13]]),
            floorstat: i16::from_le_bytes([blob[o + 14], blob[o + 15]]),
            ceilingpicnum: i16::from_le_bytes([blob[o + 16], blob[o + 17]]),
            ceilingheinum: i16::from_le_bytes([blob[o + 18], blob[o + 19]]),
            ceilingshade: blob[o + 20] as i8,
            ceilingpal: blob[o + 21],
            floorpicnum: i16::from_le_bytes([blob[o + 24], blob[o + 25]]),
            floorheinum: i16::from_le_bytes([blob[o + 26], blob[o + 27]]),
            floorshade: blob[o + 28] as i8,
            floorpal: blob[o + 29],
            visibility: blob[o + 32],
            lotag: i16::from_le_bytes([blob[o + 34], blob[o + 35]]),
            hitag: i16::from_le_bytes([blob[o + 36], blob[o + 37]]),
            extra: i16::from_le_bytes([blob[o + 38], blob[o + 39]]),
        });
    }
    Some(out)
}

fn parse_sav_sprites(numsprites: i32, blob: &[u8]) -> Option<Vec<SavSprite>> {
    let n = usize::try_from(numsprites).ok()?;
    let need = n.saturating_mul(44);
    if blob.len() < need {
        return None;
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let o = i * 44;
        let x = i32::from_le_bytes(blob[o..o + 4].try_into().ok()?);
        let y = i32::from_le_bytes(blob[o + 4..o + 8].try_into().ok()?);
        let z = i32::from_le_bytes(blob[o + 8..o + 12].try_into().ok()?);
        out.push(SavSprite {
            x,
            y,
            z,
            cstat: i16::from_le_bytes([blob[o + 12], blob[o + 13]]),
            picnum: i16::from_le_bytes([blob[o + 14], blob[o + 15]]),
            shade: blob[o + 16] as i8,
            pal: blob[o + 17],
            clipdist: blob[o + 18],
            xrepeat: blob[o + 20],
            yrepeat: blob[o + 21],
            sectnum: i16::from_le_bytes([blob[o + 24], blob[o + 25]]),
            statnum: i16::from_le_bytes([blob[o + 26], blob[o + 27]]),
            ang: i16::from_le_bytes([blob[o + 28], blob[o + 29]]),
            owner: i16::from_le_bytes([blob[o + 30], blob[o + 31]]),
            lotag: i16::from_le_bytes([blob[o + 38], blob[o + 39]]),
            hitag: i16::from_le_bytes([blob[o + 40], blob[o + 41]]),
            extra: i16::from_le_bytes([blob[o + 42], blob[o + 43]]),
        });
    }
    Some(out)
}

fn parse_sav_player(blob: &[u8]) -> Option<SavPlayer> {
    if blob.len() < 30 {
        return None;
    }
    let ohoriz = if blob.len() >= 20 {
        Some(i32::from_le_bytes(blob[16..20].try_into().ok()?))
    } else {
        None
    };
    let ohorizoff = if blob.len() >= 24 {
        Some(i32::from_le_bytes(blob[20..24].try_into().ok()?))
    } else {
        None
    };
    let invdisptime = if blob.len() >= 28 {
        Some(i32::from_le_bytes(blob[24..28].try_into().ok()?))
    } else {
        None
    };
    let extra_extra8 = (blob.len() >= 32).then(|| i16::from_le_bytes([blob[30], blob[31]]));
    let horizoff = (blob.len() >= 34).then(|| i16::from_le_bytes([blob[32], blob[33]]));
    let look_ang = (blob.len() >= 36).then(|| i16::from_le_bytes([blob[34], blob[35]]));
    let rotscrnang = (blob.len() >= 38).then(|| i16::from_le_bytes([blob[36], blob[37]]));
    let last_extra = (blob.len() >= 40).then(|| i16::from_le_bytes([blob[38], blob[39]]));
    let ammo_amount = (blob.len() >= 66).then(|| {
        let mut ammo = [0i16; 12];
        for (i, slot) in ammo.iter_mut().enumerate() {
            let o = 42 + i * 2;
            *slot = i16::from_le_bytes([blob[o], blob[o + 1]]);
        }
        ammo
    });
    let curr_weapon = (blob.len() >= 74).then(|| i16::from_le_bytes([blob[72], blob[73]]));
    let last_weapon = (blob.len() >= 76).then(|| i16::from_le_bytes([blob[74], blob[75]]));
    let wantweaponfire = (blob.len() >= 78).then(|| i16::from_le_bytes([blob[76], blob[77]]));
    let tipincs = (blob.len() >= 80).then(|| i16::from_le_bytes([blob[78], blob[79]]));
    let return_to_center = (blob.len() >= 82).then(|| i16::from_le_bytes([blob[80], blob[81]]));
    let holoduke_amount = (blob.len() >= 84).then(|| i16::from_le_bytes([blob[82], blob[83]]));
    let newowner = (blob.len() >= 86).then(|| i16::from_le_bytes([blob[84], blob[85]]));
    let hurt_delay = (blob.len() >= 88).then(|| i16::from_le_bytes([blob[86], blob[87]]));
    let hbomb_hold_delay = (blob.len() >= 90).then(|| i16::from_le_bytes([blob[88], blob[89]]));
    let jumping_counter = (blob.len() >= 92).then(|| i16::from_le_bytes([blob[90], blob[91]]));
    let airleft = (blob.len() >= 94).then(|| i16::from_le_bytes([blob[92], blob[93]]));
    let knee_incs = (blob.len() >= 96).then(|| i16::from_le_bytes([blob[94], blob[95]]));
    let access_incs = (blob.len() >= 98).then(|| i16::from_le_bytes([blob[96], blob[97]]));
    let fta = (blob.len() >= 100).then(|| i16::from_le_bytes([blob[98], blob[99]]));
    let ftq = (blob.len() >= 102).then(|| i16::from_le_bytes([blob[100], blob[101]]));
    let access_wallnum = (blob.len() >= 104).then(|| i16::from_le_bytes([blob[102], blob[103]]));
    let access_spritenum = (blob.len() >= 106).then(|| i16::from_le_bytes([blob[104], blob[105]]));
    let kickback_pic = (blob.len() >= 108).then(|| i16::from_le_bytes([blob[106], blob[107]]));
    let got_access = (blob.len() >= 110).then(|| i16::from_le_bytes([blob[108], blob[109]]));
    let weapon_ang = (blob.len() >= 112).then(|| i16::from_le_bytes([blob[110], blob[111]]));
    let firstaid_amount = (blob.len() >= 114).then(|| i16::from_le_bytes([blob[112], blob[113]]));
    let quick_kick = (blob.len() >= 138).then(|| i16::from_le_bytes([blob[136], blob[137]]));
    let heat_amount = (blob.len() >= 140).then(|| i16::from_le_bytes([blob[138], blob[139]]));
    let dead_flag = (blob.len() >= 186).then(|| i16::from_le_bytes([blob[184], blob[185]]));
    let scuba_amount = (blob.len() >= 190).then(|| i16::from_le_bytes([blob[188], blob[189]]));
    let jetpack_amount = (blob.len() >= 192).then(|| i16::from_le_bytes([blob[190], blob[191]]));
    let steroids_amount = (blob.len() >= 194).then(|| i16::from_le_bytes([blob[192], blob[193]]));
    let shield_amount = (blob.len() >= 196).then(|| i16::from_le_bytes([blob[194], blob[195]]));
    let holoduke_on = (blob.len() >= 198).then(|| i16::from_le_bytes([blob[196], blob[197]]));
    let pycount = (blob.len() >= 200).then(|| i16::from_le_bytes([blob[198], blob[199]]));
    let weapon_pos = (blob.len() >= 202).then(|| i16::from_le_bytes([blob[200], blob[201]]));
    let frag_ps = (blob.len() >= 204).then(|| i16::from_le_bytes([blob[202], blob[203]]));
    let transporter_hold = (blob.len() >= 206).then(|| i16::from_le_bytes([blob[204], blob[205]]));
    let last_full_weapon = (blob.len() >= 208).then(|| i16::from_le_bytes([blob[206], blob[207]]));
    let footprintshade = (blob.len() >= 210).then(|| i16::from_le_bytes([blob[208], blob[209]]));
    let boot_amount = (blob.len() >= 212).then(|| i16::from_le_bytes([blob[210], blob[211]]));
    let hbomb_on = (blob.len() >= 220).then(|| blob[219]);
    let jumping_toggle = (blob.len() >= 221).then(|| blob[220]);
    let on_ground = (blob.len() >= 223).then(|| blob[222]);
    let show_empty_weapon = (blob.len() >= 224).then(|| blob[223]);
    let rapid_fire_hold = (blob.len() >= 225).then(|| blob[224]);
    let pals = (blob.len() >= 228).then(|| [blob[225], blob[226], blob[227]]);
    let pals_time = (blob.len() >= 230).then(|| i16::from_le_bytes([blob[228], blob[229]]));
    let inven_icon = (blob.len() >= 256).then(|| blob[255]);
    let toggle_key_flag = (blob.len() >= 257).then(|| blob[256]);
    let jetpack_on = (blob.len() >= 258).then(|| blob[257]);
    let refresh_inventory = (blob.len() >= 259).then(|| blob[258]);
    let footprintcount = (blob.len() >= 260).then(|| blob[259]);
    let scuba_on = (blob.len() >= 261).then(|| blob[260]);
    let footprintpal = (blob.len() >= 262).then(|| blob[261]);
    let heat_on = (blob.len() >= 263).then(|| blob[262]);
    let holster_weapon = (blob.len() >= 264).then(|| blob[263]);
    let falling_counter = (blob.len() >= 265).then(|| blob[264]);
    let gotweapon = (blob.len() >= 277).then(|| {
        let mut got = [0u8; 12];
        got.copy_from_slice(&blob[265..277]);
        got
    });
    let hard_landing = (blob.len() >= 287).then(|| blob[286]);
    let knuckle_incs = (blob.len() >= 284).then(|| blob[283]);
    let walking_snd_toggle = (blob.len() >= 285).then(|| blob[284]);
    let palookup = (blob.len() >= 286).then(|| blob[285]);
    Some(SavPlayer {
        posx: i32::from_le_bytes(blob[0..4].try_into().ok()?),
        posy: i32::from_le_bytes(blob[4..8].try_into().ok()?),
        posz: i32::from_le_bytes(blob[8..12].try_into().ok()?),
        horiz: i32::from_le_bytes(blob[12..16].try_into().ok()?),
        ohoriz,
        ohorizoff,
        ang: i16::from_le_bytes([blob[28], blob[29]]),
        last_extra,
        extra_extra8,
        horizoff,
        look_ang,
        rotscrnang,
        ammo_amount,
        curr_weapon,
        last_weapon,
        wantweaponfire,
        tipincs,
        return_to_center,
        gotweapon,
        shield_amount,
        firstaid_amount,
        steroids_amount,
        jetpack_amount,
        scuba_amount,
        holoduke_amount,
        newowner,
        hurt_delay,
        hbomb_hold_delay,
        boot_amount,
        heat_amount,
        kickback_pic,
        weapon_ang,
        got_access,
        holoduke_on,
        jetpack_on,
        scuba_on,
        heat_on,
        falling_counter,
        holster_weapon,
        airleft,
        jumping_counter,
        dead_flag,
        inven_icon,
        toggle_key_flag,
        refresh_inventory,
        invdisptime,
        on_ground,
        show_empty_weapon,
        rapid_fire_hold,
        pals,
        pals_time,
        transporter_hold,
        hbomb_on,
        jumping_toggle,
        weapon_pos,
        last_full_weapon,
        footprintshade,
        footprintcount,
        footprintpal,
        quick_kick,
        frag_ps,
        knee_incs,
        access_incs,
        fta,
        ftq,
        access_wallnum,
        access_spritenum,
        hard_landing,
        knuckle_incs,
        palookup,
        walking_snd_toggle,
        pycount,
    })
}

fn i32_le(b: Vec<u8>) -> Result<i32, BuildError> {
    let a: [u8; 4] = b.try_into().map_err(|_| BuildError::Truncated("SAV"))?;
    Ok(i32::from_le_bytes(a))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::dmo::kdfwrite_raw;

    fn block_i32(v: i32) -> Vec<u8> {
        kdfwrite_raw(&v.to_le_bytes())
    }

    fn dump_through_secretlevel() -> Vec<u8> {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&8i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&12i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // sprite[]
        for _ in 0..6 {
            sav.extend(kdfwrite_raw(&[0u8; 4]));
        }
        sav.extend(kdfwrite_raw(&2i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cyclers
        sav.extend(kdfwrite_raw(&[0u8; 4])); // ps[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // po[]
        sav.extend(kdfwrite_raw(&5i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // animwall[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msy[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // spriteqloc[]
        sav.extend(kdfwrite_raw(&7i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // clouds[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudy[]
        sav.extend(block_i32(0x1357_9bdf_u32 as i32));
        sav.extend(block_i32(90));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        let mut off = [0u8; 16];
        for (i, slot) in [0i16, 2, 3, 0, 2, 0, 1, 0].into_iter().enumerate() {
            off[i * 2..i * 2 + 2].copy_from_slice(&slot.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&off));
        sav.extend(kdfwrite_raw(&4i16.to_le_bytes()));
        let mut goals = [0u8; 256];
        for (i, g) in [100i32, 200, 300, 400].into_iter().enumerate() {
            goals[i * 4..i * 4 + 4].copy_from_slice(&g.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&goals));
        let mut vels = [0u8; 256];
        for (i, v) in [8i32, 16, 24, 32].into_iter().enumerate() {
            vels[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&vels));
        let mut ptrs = [0u8; 256];
        for (i, p) in [0x1000i32, 0x2000, 0x3000, 0x4000].into_iter().enumerate() {
            ptrs[i * 4..i * 4 + 4].copy_from_slice(&p.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&ptrs));
        sav.extend(kdfwrite_raw(&[30u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[7u8]));
        sav
    }

    #[test]
    fn header_names_e1m3() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L3");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(2));
        sav.extend(block_i32(3));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.map_name(), "E1M3");
        assert_eq!(h.skill, 3);
        assert_eq!(h.name, "E1L3");
        assert_eq!(h.version, 117);
        assert_eq!(h.players, 1);
        assert_eq!(h.room_skill(), 4);
        assert_eq!(h.numwalls, None);
        assert!(matches!(
            identify_save(&sav),
            Ok(LoadedSave::Duke(ref d)) if d.map_name() == "E1M3"
        ));
        assert_eq!(identify_save(b"PK\x03\x04rest").unwrap(), LoadedSave::Room);
    }

    #[test]
    fn dump_reads_numwalls() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&8i16.to_le_bytes()));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.map_name(), "E1M1");
        assert_eq!(h.numwalls, Some(8));
        assert_eq!(h.walls, None);
        assert_eq!(h.numsectors, None);
    }

    #[test]
    fn dump_reads_numsectors_after_walls() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&8i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.numwalls, Some(8));
        assert_eq!(h.walls, None);
        assert_eq!(h.numsectors, Some(3));
        assert_eq!(h.sectors, None);
        assert_eq!(h.numsprites, None);
    }

    #[test]
    fn dump_reads_walls_after_numwalls() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&1i16.to_le_bytes()));
        let mut wall = [0u8; 32];
        wall[0..4].copy_from_slice(&1024i32.to_le_bytes());
        wall[4..8].copy_from_slice(&(-2048i32).to_le_bytes());
        wall[8..10].copy_from_slice(&1i16.to_le_bytes());
        wall[10..12].copy_from_slice(&(-1i16).to_le_bytes());
        wall[12..14].copy_from_slice(&(-1i16).to_le_bytes());
        wall[16..18].copy_from_slice(&5i16.to_le_bytes());
        wall[18..20].copy_from_slice(&7i16.to_le_bytes());
        sav.extend(kdfwrite_raw(&wall));
        let h = SavHeader::parse(&sav).unwrap();
        let w = &h.walls.as_ref().unwrap()[0];
        assert_eq!(w.x, 1024);
        assert_eq!(w.y, -2048);
        assert_eq!(w.point2, 1);
        assert_eq!(w.nextwall, -1);
        assert_eq!(w.nextsector, -1);
        assert_eq!(w.picnum, 5);
        assert_eq!(w.overpicnum, 7);
        assert_eq!(w.room_mid_pic(), "BT0007");
        assert_eq!(w.room_xy(), (64, 128));
        assert_eq!(h.numsectors, None);
    }

    #[test]
    fn dump_reads_numsprites_after_sectors() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&8i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&12i16.to_le_bytes()));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.numsectors, Some(3));
        assert_eq!(h.sectors, None);
        assert_eq!(h.numsprites, Some(12));
        assert_eq!(h.sprites, None);
        assert_eq!(h.numcyclers, None);
    }

    #[test]
    fn dump_reads_sprites_after_numsprites() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&0i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&0i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&1i16.to_le_bytes()));
        let mut sp = [0u8; 44];
        sp[0..4].copy_from_slice(&1600i32.to_le_bytes());
        sp[4..8].copy_from_slice(&(-3200i32).to_le_bytes());
        sp[8..12].copy_from_slice(&(-8192i32).to_le_bytes());
        sp[12..14].copy_from_slice(&(0x8002u16 as i16).to_le_bytes());
        sp[14..16].copy_from_slice(&1405i16.to_le_bytes());
        sp[17] = 21;
        sp[18] = 32;
        sp[20] = 48;
        sp[21] = 16;
        sp[28..30].copy_from_slice(&512i16.to_le_bytes());
        sav.extend(kdfwrite_raw(&sp));
        let h = SavHeader::parse(&sav).unwrap();
        let s = &h.sprites.as_ref().unwrap()[0];
        assert_eq!(s.x, 1600);
        assert_eq!(s.y, -3200);
        assert_eq!(s.z, -8192);
        assert_eq!(s.cstat, 0x8002u16 as i16);
        assert_eq!(s.picnum, 1405);
        assert_eq!(s.pal, 21);
        assert_eq!(s.room_translation(), "DukePal21");
        assert_eq!(s.clipdist, 32);
        assert_eq!(s.xrepeat, 48);
        assert_eq!(s.yrepeat, 16);
        assert_eq!(s.room_xy(), (100, 200));
        assert_eq!(s.room_z(), 32);
        assert!((s.room_scale().0 - 0.75).abs() < 1e-9);
        assert!((s.room_scale().1 - 0.25).abs() < 1e-9);
        assert!((s.room_clipdist().unwrap() - 2.0).abs() < 1e-9);
        assert_eq!(s.ang, 512);
        assert_eq!(s.room_yaw(), 270);
        assert_eq!(h.numcyclers, None);
    }

    #[test]
    fn dump_reads_sectors_after_numsectors() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&0i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&1i16.to_le_bytes()));
        let mut sec = [0u8; 40];
        sec[0..2].copy_from_slice(&0i16.to_le_bytes());
        sec[2..4].copy_from_slice(&4i16.to_le_bytes());
        sec[4..8].copy_from_slice(&(-16384i32).to_le_bytes());
        sec[8..12].copy_from_slice(&0i32.to_le_bytes());
        sec[16..18].copy_from_slice(&2i16.to_le_bytes());
        sec[24..26].copy_from_slice(&3i16.to_le_bytes());
        sec[28] = 20u8;
        sec[32] = 8u8;
        sav.extend(kdfwrite_raw(&sec));
        let h = SavHeader::parse(&sav).unwrap();
        let s = &h.sectors.as_ref().unwrap()[0];
        assert_eq!(s.wallptr, 0);
        assert_eq!(s.wallnum, 4);
        assert_eq!(s.ceilingz, -16384);
        assert_eq!(s.floorz, 0);
        assert_eq!(s.room_ceiling_z(), 64);
        assert_eq!(s.room_floor_z(), 0);
        assert_eq!(s.ceilingpicnum, 2);
        assert_eq!(s.floorpicnum, 3);
        assert_eq!(s.room_ceiling_pic(), "BT0002");
        assert_eq!(s.room_floor_pic(), "BT0003");
        assert_eq!(s.floorshade, 20);
        assert_eq!(s.visibility, 8);
        assert_eq!(s.room_light(), 127);
        assert_eq!(s.room_fog_density(), 32);
        assert_eq!(h.numsprites, None);
    }

    #[test]
    fn dump_reads_numcyclers_after_sprite_lists() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&8i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&12i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // sprite[]
        for _ in 0..6 {
            sav.extend(kdfwrite_raw(&[0u8; 4]));
        }
        sav.extend(kdfwrite_raw(&2i16.to_le_bytes()));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.numsprites, Some(12));
        assert_eq!(h.numcyclers, Some(2));
        assert_eq!(h.numanimwalls, None);
    }

    #[test]
    fn dump_reads_numanimwalls_after_players() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&8i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&12i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // sprite[]
        for _ in 0..6 {
            sav.extend(kdfwrite_raw(&[0u8; 4]));
        }
        sav.extend(kdfwrite_raw(&2i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cyclers
        sav.extend(kdfwrite_raw(&[0u8; 4])); // ps[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // po[]
        sav.extend(kdfwrite_raw(&5i16.to_le_bytes()));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.numcyclers, Some(2));
        assert_eq!(h.player, None);
        assert_eq!(h.numanimwalls, Some(5));
        assert_eq!(h.spriteqamount, None);
    }

    #[test]
    fn dump_reads_player_after_cyclers() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&0i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&0i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&0i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        for _ in 0..6 {
            sav.extend(kdfwrite_raw(&[0u8; 4]));
        }
        sav.extend(kdfwrite_raw(&0i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cyclers
        let mut ps = [0u8; 287];
        ps[0..4].copy_from_slice(&1600i32.to_le_bytes());
        ps[4..8].copy_from_slice(&(-3200i32).to_le_bytes());
        ps[8..12].copy_from_slice(&(-8192i32).to_le_bytes());
        ps[12..16].copy_from_slice(&100i32.to_le_bytes());
        ps[16..20].copy_from_slice(&110i32.to_le_bytes());
        ps[20..24].copy_from_slice(&10i32.to_le_bytes());
        ps[24..28].copy_from_slice(&30i32.to_le_bytes());
        ps[28..30].copy_from_slice(&512i16.to_le_bytes());
        ps[30..32].copy_from_slice(&256i16.to_le_bytes());
        ps[32..34].copy_from_slice(&15i16.to_le_bytes());
        ps[34..36].copy_from_slice(&256i16.to_le_bytes());
        ps[36..38].copy_from_slice(&256i16.to_le_bytes());
        ps[38..40].copy_from_slice(&75i16.to_le_bytes());
        ps[44..46].copy_from_slice(&48i16.to_le_bytes());
        ps[46..48].copy_from_slice(&10i16.to_le_bytes());
        ps[48..50].copy_from_slice(&200i16.to_le_bytes());
        ps[50..52].copy_from_slice(&5i16.to_le_bytes());
        ps[52..54].copy_from_slice(&9i16.to_le_bytes());
        ps[54..56].copy_from_slice(&20i16.to_le_bytes());
        ps[56..58].copy_from_slice(&15i16.to_le_bytes());
        ps[58..60].copy_from_slice(&6i16.to_le_bytes());
        ps[60..62].copy_from_slice(&25i16.to_le_bytes());
        ps[64..66].copy_from_slice(&12i16.to_le_bytes());
        ps[72..74].copy_from_slice(&2i16.to_le_bytes());
        ps[74..76].copy_from_slice(&1i16.to_le_bytes());
        ps[76..78].copy_from_slice(&3i16.to_le_bytes());
        ps[78..80].copy_from_slice(&30i16.to_le_bytes());
        ps[80..82].copy_from_slice(&30i16.to_le_bytes());
        ps[82..84].copy_from_slice(&2400i16.to_le_bytes());
        ps[84..86].copy_from_slice(&1i16.to_le_bytes());
        ps[86..88].copy_from_slice(&30i16.to_le_bytes());
        ps[88..90].copy_from_slice(&4i16.to_le_bytes());
        ps[90..92].copy_from_slice(&180i16.to_le_bytes());
        ps[92..94].copy_from_slice(&30i16.to_le_bytes());
        ps[94..96].copy_from_slice(&8i16.to_le_bytes());
        ps[96..98].copy_from_slice(&12i16.to_le_bytes());
        ps[98..100].copy_from_slice(&30i16.to_le_bytes());
        ps[100..102].copy_from_slice(&61i16.to_le_bytes());
        ps[102..104].copy_from_slice(&0i16.to_le_bytes());
        ps[104..106].copy_from_slice(&1i16.to_le_bytes());
        ps[106..108].copy_from_slice(&15i16.to_le_bytes());
        ps[108..110].copy_from_slice(&5i16.to_le_bytes());
        ps[110..112].copy_from_slice(&256i16.to_le_bytes());
        ps[112..114].copy_from_slice(&80i16.to_le_bytes());
        ps[136..138].copy_from_slice(&14i16.to_le_bytes());
        ps[138..140].copy_from_slice(&1200i16.to_le_bytes());
        ps[184..186].copy_from_slice(&15i16.to_le_bytes());
        ps[188..190].copy_from_slice(&640i16.to_le_bytes());
        ps[190..192].copy_from_slice(&600i16.to_le_bytes());
        ps[192..194].copy_from_slice(&400i16.to_le_bytes());
        ps[194..196].copy_from_slice(&50i16.to_le_bytes());
        ps[196..198].copy_from_slice(&3i16.to_le_bytes());
        ps[198..200].copy_from_slice(&512i16.to_le_bytes());
        ps[200..202].copy_from_slice(&6i16.to_le_bytes());
        ps[202..204].copy_from_slice(&3i16.to_le_bytes());
        ps[204..206].copy_from_slice(&30i16.to_le_bytes());
        ps[206..208].copy_from_slice(&2i16.to_le_bytes());
        ps[208..210].copy_from_slice(&12i16.to_le_bytes());
        ps[210..212].copy_from_slice(&200i16.to_le_bytes());
        ps[219] = 1;
        ps[220] = 1;
        ps[222] = 1;
        ps[223] = 1;
        ps[224] = 1;
        ps[225] = 48;
        ps[228..230].copy_from_slice(&30i16.to_le_bytes());
        ps[255] = 2;
        ps[256] = 1;
        ps[257] = 1;
        ps[258] = 1;
        ps[259] = 7;
        ps[260] = 1;
        ps[261] = 21;
        ps[262] = 1;
        ps[263] = 1;
        ps[264] = 30;
        ps[267] = 1;
        ps[283] = 8;
        ps[284] = 1;
        ps[285] = 21;
        ps[286] = 8;
        sav.extend(kdfwrite_raw(&ps));
        let h = SavHeader::parse(&sav).unwrap();
        let p = h.player.unwrap();
        assert_eq!(p.posx, 1600);
        assert_eq!(p.posy, -3200);
        assert_eq!(p.posz, -8192);
        assert_eq!(p.horiz, 100);
        assert_eq!(p.ohoriz, Some(110));
        assert_eq!(p.ohorizoff, Some(10));
        assert_eq!(p.room_prev_pitch(), Some(20));
        assert_eq!(p.ang, 512);
        assert_eq!(p.last_extra, Some(75));
        assert_eq!(p.extra_extra8, Some(256));
        assert_eq!(p.horizoff, Some(15));
        assert_eq!(p.look_ang, Some(256));
        assert_eq!(p.rotscrnang, Some(256));
        assert_eq!(p.room_rotscrnang(), Some(256));
        assert_eq!(p.ammo_amount.map(|a| a[1]), Some(48));
        assert_eq!(p.ammo_amount.map(|a| a[2]), Some(10));
        assert_eq!(p.ammo_amount.map(|a| a[3]), Some(200));
        assert_eq!(p.ammo_amount.map(|a| a[4]), Some(5));
        assert_eq!(p.ammo_amount.map(|a| a[6]), Some(20));
        assert_eq!(p.ammo_amount.map(|a| a[5]), Some(9));
        assert_eq!(p.ammo_amount.map(|a| a[7]), Some(15));
        assert_eq!(p.ammo_amount.map(|a| a[8]), Some(6));
        assert_eq!(p.ammo_amount.map(|a| a[9]), Some(25));
        assert_eq!(p.ammo_amount.map(|a| a[11]), Some(12));
        assert_eq!(p.room_xy(), (100, 200));
        assert_eq!(p.room_z(), 32);
        assert_eq!(p.room_yaw(), 225);
        assert_eq!(p.room_pitch(), 15);
        assert_eq!(p.room_health(), Some(76));
        assert_eq!(p.room_clip_ammo(), Some(200));
        assert_eq!(p.room_shell_ammo(), Some(10));
        assert_eq!(p.room_rocket_ammo(), Some(5));
        assert_eq!(p.room_cell_ammo(), Some(20));
        assert_eq!(p.room_pipebomb_ammo(), Some(9));
        assert_eq!(p.room_devastator_ammo(), Some(15));
        assert_eq!(p.room_freeze_ammo(), Some(25));
        assert_eq!(p.room_tripbomb_ammo(), Some(6));
        assert_eq!(p.room_grow_ammo(), Some(12));
        assert_eq!(p.curr_weapon, Some(2));
        assert_eq!(p.room_weapon_slot(), Some(2));
        assert_eq!(p.last_weapon, Some(1));
        assert_eq!(p.room_last_weapon_slot(), Some(1));
        assert_eq!(p.wantweaponfire, Some(3));
        assert_eq!(p.tipincs, Some(30));
        assert_eq!(p.room_tip_incs(), Some(35));
        assert_eq!(p.return_to_center, Some(30));
        assert_eq!(p.room_return_to_center(), Some(35));
        assert_eq!(p.room_wantweapon_fire(), Some(3));
        assert_eq!(p.gotweapon.map(|g| g[2]), Some(1));
        assert_eq!(p.room_gotweapon().map(|g| g[2]), Some(1));
        assert_eq!(p.shield_amount, Some(50));
        assert_eq!(p.room_armor(), Some(50));
        assert_eq!(p.firstaid_amount, Some(80));
        assert_eq!(p.room_firstaid(), Some(80));
        assert_eq!(p.steroids_amount, Some(400));
        assert_eq!(p.room_steroids(), Some(400));
        assert_eq!(p.jetpack_amount, Some(600));
        assert_eq!(p.room_jetpack(), Some(600));
        assert_eq!(p.scuba_amount, Some(640));
        assert_eq!(p.room_scuba(), Some(640));
        assert_eq!(p.holoduke_amount, Some(2400));
        assert_eq!(p.newowner, Some(1));
        assert_eq!(p.room_newowner(), Some(1));
        assert_eq!(p.hurt_delay, Some(30));
        assert_eq!(p.room_hurt_delay_tics(), Some(35));
        assert_eq!(p.hbomb_hold_delay, Some(4));
        assert_eq!(p.room_hbomb_hold(), Some(4));
        assert_eq!(p.room_holoduke(), Some(2400));
        assert_eq!(p.boot_amount, Some(200));
        assert_eq!(p.room_boots(), Some(200));
        assert_eq!(p.heat_amount, Some(1200));
        assert_eq!(p.room_heat(), Some(1200));
        assert_eq!(p.kickback_pic, Some(15));
        assert_eq!(p.room_kickback_pic(), Some(15));
        assert_eq!(p.room_kickback_firing(), Some(true));
        assert_eq!(p.weapon_ang, Some(256));
        assert_eq!(p.room_weapon_ang_deg(), Some(315));
        assert_eq!(p.got_access, Some(5));
        assert_eq!(p.room_got_access(), Some(5));
        assert_eq!(p.room_blue_card(), Some(true));
        assert_eq!(p.room_red_card(), Some(false));
        assert_eq!(p.room_yellow_card(), Some(true));
        assert_eq!(p.holoduke_on, Some(3));
        assert_eq!(p.room_holoduke_on(), Some(true));
        assert_eq!(p.jetpack_on, Some(1));
        assert_eq!(p.room_jetpack_on(), Some(true));
        assert_eq!(p.scuba_on, Some(1));
        assert_eq!(p.room_scuba_on(), Some(true));
        assert_eq!(p.heat_on, Some(1));
        assert_eq!(p.room_heat_on(), Some(true));
        assert_eq!(p.falling_counter, Some(30));
        assert_eq!(p.room_falling_tics(), Some(35));
        assert_eq!(p.holster_weapon, Some(1));
        assert_eq!(p.room_holster_weapon(), Some(true));
        assert_eq!(p.airleft, Some(30));
        assert_eq!(p.room_air_tics(), Some(35));
        assert_eq!(p.jumping_counter, Some(180));
        assert_eq!(p.room_jumping(), Some(true));
        assert_eq!(p.dead_flag, Some(15));
        assert_eq!(p.room_dead(), Some(true));
        assert_eq!(p.inven_icon, Some(2));
        assert_eq!(p.room_inven_icon(), Some(2));
        assert_eq!(p.toggle_key_flag, Some(1));
        assert_eq!(p.room_toggle_key_flag(), Some(true));
        assert_eq!(p.refresh_inventory, Some(1));
        assert_eq!(p.room_refresh_inventory(), Some(true));
        assert_eq!(p.invdisptime, Some(30));
        assert_eq!(p.room_invdisp_tics(), Some(35));
        assert_eq!(p.on_ground, Some(1));
        assert_eq!(p.room_on_ground(), Some(true));
        assert_eq!(p.show_empty_weapon, Some(1));
        assert_eq!(p.room_show_empty_weapon(), Some(true));
        assert_eq!(p.rapid_fire_hold, Some(1));
        assert_eq!(p.room_rapid_fire_hold(), Some(true));
        assert_eq!(p.pals, Some([48, 0, 0]));
        assert_eq!(p.pals_time, Some(30));
        assert_eq!(p.room_pals_flash(), Some(35));
        assert_eq!(p.transporter_hold, Some(30));
        assert_eq!(p.room_transporter_hold_tics(), Some(35));
        assert_eq!(p.hbomb_on, Some(1));
        assert_eq!(p.room_hbomb_on(), Some(true));
        assert_eq!(p.jumping_toggle, Some(1));
        assert_eq!(p.room_jumping_toggle(), Some(true));
        assert_eq!(p.weapon_pos, Some(6));
        assert_eq!(p.room_pspr_y(), Some(80.0));
        assert_eq!(p.last_full_weapon, Some(2));
        assert_eq!(p.footprintshade, Some(12));
        assert_eq!(p.room_footprint_shade(), Some(12));
        assert_eq!(p.footprintcount, Some(7));
        assert_eq!(p.room_footprint_count(), Some(7));
        assert_eq!(p.footprintpal, Some(21));
        assert_eq!(p.room_footprint_pal(), Some(21));
        assert_eq!(p.room_last_full_weapon_slot(), Some(2));
        assert_eq!(p.quick_kick, Some(14));
        assert_eq!(p.room_quick_kick(), Some(true));
        assert_eq!(p.frag_ps, Some(3));
        assert_eq!(p.room_frag_ps(), Some(3));
        assert_eq!(p.knee_incs, Some(8));
        assert_eq!(p.room_knee_incs(), Some(8));
        assert_eq!(p.access_incs, Some(12));
        assert_eq!(p.room_access_incs(), Some(true));
        assert_eq!(p.fta, Some(30));
        assert_eq!(p.ftq, Some(61));
        assert_eq!(p.room_fta_tics(), Some(35));
        assert_eq!(p.room_ftq(), Some(61));
        assert_eq!(p.access_wallnum, Some(0));
        assert_eq!(p.room_access_wallnum(), Some(0));
        assert_eq!(p.access_spritenum, Some(1));
        assert_eq!(p.room_access_spritenum(), Some(1));
        assert_eq!(p.hard_landing, Some(8));
        assert_eq!(p.room_hard_landing(), Some(8));
        assert_eq!(p.knuckle_incs, Some(8));
        assert_eq!(p.room_knuckle_incs(), Some(true));
        assert_eq!(p.palookup, Some(21));
        assert_eq!(p.room_palookup(), Some(21));
        assert_eq!(p.walking_snd_toggle, Some(1));
        assert_eq!(p.room_walking_snd_toggle(), Some(true));
        assert_eq!(p.pycount, Some(512));
        assert_eq!(p.room_pycount(), Some(512));
        assert_eq!(
            SavPlayer {
                posx: 0,
                posy: 0,
                posz: 0,
                horiz: 140,
                ohoriz: None,
                ohorizoff: None,
                ang: 0,
                last_extra: None,
                extra_extra8: None,
                horizoff: None,
                look_ang: None,
                rotscrnang: None,
                ammo_amount: None,
                curr_weapon: None,
                last_weapon: None,
                wantweaponfire: None,
                tipincs: None,
                return_to_center: None,
                gotweapon: None,
                shield_amount: None,
                firstaid_amount: None,
                steroids_amount: None,
                jetpack_amount: None,
                scuba_amount: None,
                holoduke_amount: None,
                newowner: None,
                hurt_delay: None,
                hbomb_hold_delay: None,
                boot_amount: None,
                heat_amount: None,
                kickback_pic: None,
                weapon_ang: None,
                got_access: None,
                holoduke_on: None,
                jetpack_on: None,
                scuba_on: None,
                heat_on: None,
                falling_counter: None,
                holster_weapon: None,
                airleft: None,
                jumping_counter: None,
                dead_flag: None,
                inven_icon: None,
                toggle_key_flag: None,
                refresh_inventory: None,
                invdisptime: None,
                on_ground: None,
                show_empty_weapon: None,
                rapid_fire_hold: None,
                pals: None,
                pals_time: None,
                transporter_hold: None,
                hbomb_on: None,
                jumping_toggle: None,
                weapon_pos: None,
                last_full_weapon: None,
                footprintshade: None,
                footprintcount: None,
                footprintpal: None,
                quick_kick: None,
                frag_ps: None,
                knee_incs: None,
                access_incs: None,
                fta: None,
                ftq: None,
                access_wallnum: None,
                access_spritenum: None,
                hard_landing: None,
                knuckle_incs: None,
                palookup: None,
                walking_snd_toggle: None,
                pycount: None,
            }
            .room_pitch(),
            40
        );
        assert_eq!(h.numanimwalls, None);
    }

    #[test]
    fn dump_reads_spriteqamount_after_mirrors() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&8i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&12i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // sprite[]
        for _ in 0..6 {
            sav.extend(kdfwrite_raw(&[0u8; 4]));
        }
        sav.extend(kdfwrite_raw(&2i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cyclers
        sav.extend(kdfwrite_raw(&[0u8; 4])); // ps[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // po[]
        sav.extend(kdfwrite_raw(&5i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // animwall[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msy[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // spriteqloc[]
        sav.extend(kdfwrite_raw(&7i16.to_le_bytes()));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.numanimwalls, Some(5));
        assert_eq!(h.spriteqamount, Some(7));
        assert_eq!(h.numclouds, None);
    }

    #[test]
    fn dump_reads_numclouds_after_spriteq() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&8i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&12i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // sprite[]
        for _ in 0..6 {
            sav.extend(kdfwrite_raw(&[0u8; 4]));
        }
        sav.extend(kdfwrite_raw(&2i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cyclers
        sav.extend(kdfwrite_raw(&[0u8; 4])); // ps[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // po[]
        sav.extend(kdfwrite_raw(&5i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // animwall[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msy[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // spriteqloc[]
        sav.extend(kdfwrite_raw(&7i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.spriteqamount, Some(7));
        assert_eq!(h.numclouds, Some(3));
        assert_eq!(h.randomseed, None);
    }

    #[test]
    fn dump_reads_randomseed_after_clouds() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&8i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&12i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // sprite[]
        for _ in 0..6 {
            sav.extend(kdfwrite_raw(&[0u8; 4]));
        }
        sav.extend(kdfwrite_raw(&2i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cyclers
        sav.extend(kdfwrite_raw(&[0u8; 4])); // ps[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // po[]
        sav.extend(kdfwrite_raw(&5i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // animwall[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msy[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // spriteqloc[]
        sav.extend(kdfwrite_raw(&7i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // clouds[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudy[]
        sav.extend(block_i32(0x1357_9bdf_u32 as i32));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.numclouds, Some(3));
        assert_eq!(h.randomseed, Some(0x1357_9bdf_u32 as i32));
        assert_eq!(h.room_rng_seed(), Some(0x1357_9bdf));
        assert_eq!(h.lockclock, None);
    }

    #[test]
    fn dump_reads_lockclock_after_seed() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&8i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&12i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // sprite[]
        for _ in 0..6 {
            sav.extend(kdfwrite_raw(&[0u8; 4]));
        }
        sav.extend(kdfwrite_raw(&2i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cyclers
        sav.extend(kdfwrite_raw(&[0u8; 4])); // ps[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // po[]
        sav.extend(kdfwrite_raw(&5i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // animwall[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msy[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // spriteqloc[]
        sav.extend(kdfwrite_raw(&7i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // clouds[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudy[]
        sav.extend(block_i32(0x1357_9bdf_u32 as i32));
        sav.extend(block_i32(90));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.lockclock, Some(90));
        assert_eq!(h.room_lock_tics(), Some(105));
        assert_eq!(h.pskybits, None);
    }

    #[test]
    fn dump_reads_pskybits_after_lockclock() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&8i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&12i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // sprite[]
        for _ in 0..6 {
            sav.extend(kdfwrite_raw(&[0u8; 4]));
        }
        sav.extend(kdfwrite_raw(&2i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cyclers
        sav.extend(kdfwrite_raw(&[0u8; 4])); // ps[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // po[]
        sav.extend(kdfwrite_raw(&5i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // animwall[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msy[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // spriteqloc[]
        sav.extend(kdfwrite_raw(&7i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // clouds[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudy[]
        sav.extend(block_i32(0x1357_9bdf_u32 as i32));
        sav.extend(block_i32(90));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.pskybits, Some(3));
        assert_eq!(h.pskyoff, None);
    }

    #[test]
    fn dump_reads_pskyoff_after_pskybits() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&8i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&12i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // sprite[]
        for _ in 0..6 {
            sav.extend(kdfwrite_raw(&[0u8; 4]));
        }
        sav.extend(kdfwrite_raw(&2i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cyclers
        sav.extend(kdfwrite_raw(&[0u8; 4])); // ps[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // po[]
        sav.extend(kdfwrite_raw(&5i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // animwall[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msy[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // spriteqloc[]
        sav.extend(kdfwrite_raw(&7i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // clouds[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudy[]
        sav.extend(block_i32(0x1357_9bdf_u32 as i32));
        sav.extend(block_i32(90));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        let mut off = [0u8; 16];
        for (i, slot) in [0i16, 2, 3, 0, 2, 0, 1, 0].into_iter().enumerate() {
            off[i * 2..i * 2 + 2].copy_from_slice(&slot.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&off));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.pskyoff, Some([0, 2, 3, 0, 2, 0, 1, 0]));
        assert_eq!(h.room_pskyoff(), Some([0, 2, 3, 0, 2, 0, 1, 0]));
        assert_eq!(h.room_pskybits(), Some(3));
        assert_eq!(h.animatecnt, None);
    }

    #[test]
    fn dump_reads_animatecnt_after_pskyoff() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&8i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&12i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // sprite[]
        for _ in 0..6 {
            sav.extend(kdfwrite_raw(&[0u8; 4]));
        }
        sav.extend(kdfwrite_raw(&2i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cyclers
        sav.extend(kdfwrite_raw(&[0u8; 4])); // ps[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // po[]
        sav.extend(kdfwrite_raw(&5i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // animwall[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msy[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // spriteqloc[]
        sav.extend(kdfwrite_raw(&7i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // clouds[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudy[]
        sav.extend(block_i32(0x1357_9bdf_u32 as i32));
        sav.extend(block_i32(90));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        let mut off = [0u8; 16];
        for (i, slot) in [0i16, 2, 3, 0, 2, 0, 1, 0].into_iter().enumerate() {
            off[i * 2..i * 2 + 2].copy_from_slice(&slot.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&off));
        sav.extend(kdfwrite_raw(&4i16.to_le_bytes()));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.animatecnt, Some(4));
        assert_eq!(h.room_animatecnt(), Some(4));
        assert_eq!(h.animategoal, None);
    }

    #[test]
    fn dump_reads_animategoal_after_animatecnt() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&8i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&12i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // sprite[]
        for _ in 0..6 {
            sav.extend(kdfwrite_raw(&[0u8; 4]));
        }
        sav.extend(kdfwrite_raw(&2i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cyclers
        sav.extend(kdfwrite_raw(&[0u8; 4])); // ps[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // po[]
        sav.extend(kdfwrite_raw(&5i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // animwall[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msy[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // spriteqloc[]
        sav.extend(kdfwrite_raw(&7i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // clouds[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudy[]
        sav.extend(block_i32(0x1357_9bdf_u32 as i32));
        sav.extend(block_i32(90));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        let mut off = [0u8; 16];
        for (i, slot) in [0i16, 2, 3, 0, 2, 0, 1, 0].into_iter().enumerate() {
            off[i * 2..i * 2 + 2].copy_from_slice(&slot.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&off));
        sav.extend(kdfwrite_raw(&4i16.to_le_bytes()));
        let mut goals = [0u8; 256];
        for (i, g) in [100i32, 200, 300, 400].into_iter().enumerate() {
            goals[i * 4..i * 4 + 4].copy_from_slice(&g.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&goals));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.animategoal, Some(vec![100, 200, 300, 400]));
        assert_eq!(h.animatevel, None);
    }

    #[test]
    fn dump_reads_animatevel_after_animategoal() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&8i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&12i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // sprite[]
        for _ in 0..6 {
            sav.extend(kdfwrite_raw(&[0u8; 4]));
        }
        sav.extend(kdfwrite_raw(&2i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cyclers
        sav.extend(kdfwrite_raw(&[0u8; 4])); // ps[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // po[]
        sav.extend(kdfwrite_raw(&5i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // animwall[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msy[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // spriteqloc[]
        sav.extend(kdfwrite_raw(&7i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // clouds[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudy[]
        sav.extend(block_i32(0x1357_9bdf_u32 as i32));
        sav.extend(block_i32(90));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        let mut off = [0u8; 16];
        for (i, slot) in [0i16, 2, 3, 0, 2, 0, 1, 0].into_iter().enumerate() {
            off[i * 2..i * 2 + 2].copy_from_slice(&slot.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&off));
        sav.extend(kdfwrite_raw(&4i16.to_le_bytes()));
        let mut goals = [0u8; 256];
        for (i, g) in [100i32, 200, 300, 400].into_iter().enumerate() {
            goals[i * 4..i * 4 + 4].copy_from_slice(&g.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&goals));
        let mut vels = [0u8; 256];
        for (i, v) in [8i32, 16, 24, 32].into_iter().enumerate() {
            vels[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&vels));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.animatevel, Some(vec![8, 16, 24, 32]));
        assert_eq!(h.animateptr, None);
    }

    #[test]
    fn dump_reads_animateptr_after_animatevel() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&8i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&12i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // sprite[]
        for _ in 0..6 {
            sav.extend(kdfwrite_raw(&[0u8; 4]));
        }
        sav.extend(kdfwrite_raw(&2i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cyclers
        sav.extend(kdfwrite_raw(&[0u8; 4])); // ps[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // po[]
        sav.extend(kdfwrite_raw(&5i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // animwall[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msy[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // spriteqloc[]
        sav.extend(kdfwrite_raw(&7i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // clouds[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudy[]
        sav.extend(block_i32(0x1357_9bdf_u32 as i32));
        sav.extend(block_i32(90));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        let mut off = [0u8; 16];
        for (i, slot) in [0i16, 2, 3, 0, 2, 0, 1, 0].into_iter().enumerate() {
            off[i * 2..i * 2 + 2].copy_from_slice(&slot.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&off));
        sav.extend(kdfwrite_raw(&4i16.to_le_bytes()));
        let mut goals = [0u8; 256];
        for (i, g) in [100i32, 200, 300, 400].into_iter().enumerate() {
            goals[i * 4..i * 4 + 4].copy_from_slice(&g.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&goals));
        let mut vels = [0u8; 256];
        for (i, v) in [8i32, 16, 24, 32].into_iter().enumerate() {
            vels[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&vels));
        let mut ptrs = [0u8; 256];
        for (i, p) in [0x1000i32, 0x2000, 0x3000, 0x4000].into_iter().enumerate() {
            ptrs[i * 4..i * 4 + 4].copy_from_slice(&p.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&ptrs));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.animateptr, Some(vec![0x1000, 0x2000, 0x3000, 0x4000]));
        assert_eq!(h.earthquaketime, None);
    }

    #[test]
    fn dump_reads_earthquaketime_after_animateptr() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&8i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&12i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // sprite[]
        for _ in 0..6 {
            sav.extend(kdfwrite_raw(&[0u8; 4]));
        }
        sav.extend(kdfwrite_raw(&2i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cyclers
        sav.extend(kdfwrite_raw(&[0u8; 4])); // ps[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // po[]
        sav.extend(kdfwrite_raw(&5i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // animwall[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msy[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // spriteqloc[]
        sav.extend(kdfwrite_raw(&7i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // clouds[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudy[]
        sav.extend(block_i32(0x1357_9bdf_u32 as i32));
        sav.extend(block_i32(90));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        let mut off = [0u8; 16];
        for (i, slot) in [0i16, 2, 3, 0, 2, 0, 1, 0].into_iter().enumerate() {
            off[i * 2..i * 2 + 2].copy_from_slice(&slot.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&off));
        sav.extend(kdfwrite_raw(&4i16.to_le_bytes()));
        let mut goals = [0u8; 256];
        for (i, g) in [100i32, 200, 300, 400].into_iter().enumerate() {
            goals[i * 4..i * 4 + 4].copy_from_slice(&g.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&goals));
        let mut vels = [0u8; 256];
        for (i, v) in [8i32, 16, 24, 32].into_iter().enumerate() {
            vels[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&vels));
        let mut ptrs = [0u8; 256];
        for (i, p) in [0x1000i32, 0x2000, 0x3000, 0x4000].into_iter().enumerate() {
            ptrs[i * 4..i * 4 + 4].copy_from_slice(&p.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&ptrs));
        sav.extend(kdfwrite_raw(&[30u8]));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.earthquaketime, Some(30));
        assert_eq!(h.room_quake_tics(), Some(35));
        assert_eq!(h.from_bonus, None);
    }

    #[test]
    fn dump_reads_from_bonus_after_earthquaketime() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&8i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&12i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // sprite[]
        for _ in 0..6 {
            sav.extend(kdfwrite_raw(&[0u8; 4]));
        }
        sav.extend(kdfwrite_raw(&2i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cyclers
        sav.extend(kdfwrite_raw(&[0u8; 4])); // ps[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // po[]
        sav.extend(kdfwrite_raw(&5i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // animwall[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msy[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // spriteqloc[]
        sav.extend(kdfwrite_raw(&7i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // clouds[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudy[]
        sav.extend(block_i32(0x1357_9bdf_u32 as i32));
        sav.extend(block_i32(90));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        let mut off = [0u8; 16];
        for (i, slot) in [0i16, 2, 3, 0, 2, 0, 1, 0].into_iter().enumerate() {
            off[i * 2..i * 2 + 2].copy_from_slice(&slot.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&off));
        sav.extend(kdfwrite_raw(&4i16.to_le_bytes()));
        let mut goals = [0u8; 256];
        for (i, g) in [100i32, 200, 300, 400].into_iter().enumerate() {
            goals[i * 4..i * 4 + 4].copy_from_slice(&g.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&goals));
        let mut vels = [0u8; 256];
        for (i, v) in [8i32, 16, 24, 32].into_iter().enumerate() {
            vels[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&vels));
        let mut ptrs = [0u8; 256];
        for (i, p) in [0x1000i32, 0x2000, 0x3000, 0x4000].into_iter().enumerate() {
            ptrs[i * 4..i * 4 + 4].copy_from_slice(&p.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&ptrs));
        sav.extend(kdfwrite_raw(&[30u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.from_bonus, Some(1));
        assert_eq!(h.room_from_bonus(), Some(true));
        assert_eq!(h.secretlevel, None);
    }

    #[test]
    fn dump_reads_secretlevel_after_from_bonus() {
        let mut name = [0u8; 19];
        name[..4].copy_from_slice(b"E1L1");
        let mut sav = Vec::new();
        sav.extend(block_i32(117));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&name));
        sav.extend(block_i32(0));
        sav.extend(block_i32(0));
        sav.extend(block_i32(1));
        sav.extend(kdfwrite_raw(&8i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4]));
        sav.extend(kdfwrite_raw(&12i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // sprite[]
        for _ in 0..6 {
            sav.extend(kdfwrite_raw(&[0u8; 4]));
        }
        sav.extend(kdfwrite_raw(&2i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cyclers
        sav.extend(kdfwrite_raw(&[0u8; 4])); // ps[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // po[]
        sav.extend(kdfwrite_raw(&5i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // animwall[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // msy[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // spriteqloc[]
        sav.extend(kdfwrite_raw(&7i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 4])); // clouds[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudx[]
        sav.extend(kdfwrite_raw(&[0u8; 4])); // cloudy[]
        sav.extend(block_i32(0x1357_9bdf_u32 as i32));
        sav.extend(block_i32(90));
        sav.extend(kdfwrite_raw(&3i16.to_le_bytes()));
        let mut off = [0u8; 16];
        for (i, slot) in [0i16, 2, 3, 0, 2, 0, 1, 0].into_iter().enumerate() {
            off[i * 2..i * 2 + 2].copy_from_slice(&slot.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&off));
        sav.extend(kdfwrite_raw(&4i16.to_le_bytes()));
        let mut goals = [0u8; 256];
        for (i, g) in [100i32, 200, 300, 400].into_iter().enumerate() {
            goals[i * 4..i * 4 + 4].copy_from_slice(&g.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&goals));
        let mut vels = [0u8; 256];
        for (i, v) in [8i32, 16, 24, 32].into_iter().enumerate() {
            vels[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&vels));
        let mut ptrs = [0u8; 256];
        for (i, p) in [0x1000i32, 0x2000, 0x3000, 0x4000].into_iter().enumerate() {
            ptrs[i * 4..i * 4 + 4].copy_from_slice(&p.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&ptrs));
        sav.extend(kdfwrite_raw(&[30u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[7u8]));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.secretlevel, Some(7));
        assert_eq!(h.room_secretlevel(), Some(7));
        assert_eq!(h.respawn_monsters, None);
    }

    #[test]
    fn dump_reads_respawn_monsters_after_secretlevel() {
        let mut sav = dump_through_secretlevel();
        sav.extend(kdfwrite_raw(&[1u8]));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.respawn_monsters, Some(1));
        assert_eq!(h.room_respawn_monsters(), Some(true));
        assert_eq!(h.respawn_items, None);
    }

    #[test]
    fn dump_reads_respawn_items_after_respawn_monsters() {
        let mut sav = dump_through_secretlevel();
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.respawn_items, Some(1));
        assert_eq!(h.room_respawn_items(), Some(true));
        assert_eq!(h.respawn_inventory, None);
    }

    #[test]
    fn dump_reads_respawn_inventory_after_respawn_items() {
        let mut sav = dump_through_secretlevel();
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.respawn_inventory, Some(1));
        assert_eq!(h.room_respawn_inventory(), Some(true));
        assert_eq!(h.god, None);
    }

    #[test]
    fn dump_reads_god_after_respawn_inventory() {
        let mut sav = dump_through_secretlevel();
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.god, Some(1));
        assert_eq!(h.room_god(), Some(true));
        assert_eq!(h.auto_run, None);
    }

    #[test]
    fn dump_reads_auto_run_after_god() {
        let mut sav = dump_through_secretlevel();
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.auto_run, Some(1));
        assert_eq!(h.room_auto_run(), Some(true));
        assert_eq!(h.crosshair, None);
    }

    #[test]
    fn dump_reads_crosshair_after_auto_run() {
        let mut sav = dump_through_secretlevel();
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.crosshair, Some(1));
        assert_eq!(h.room_crosshair(), Some(true));
        assert_eq!(h.monsters_off, None);
    }

    #[test]
    fn dump_reads_monsters_off_after_crosshair() {
        let mut sav = dump_through_secretlevel();
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.monsters_off, Some(1));
        assert_eq!(h.room_monsters_off(), Some(true));
        assert_eq!(h.last_level, None);
    }

    #[test]
    fn dump_reads_last_level_after_monsters_off() {
        let mut sav = dump_through_secretlevel();
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[2u8]));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.last_level, Some(2));
        assert_eq!(h.room_last_level(), Some(2));
        assert_eq!(h.eog, None);
    }

    #[test]
    fn dump_reads_eog_after_last_level() {
        let mut sav = dump_through_secretlevel();
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[2u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.eog, Some(1));
        assert_eq!(h.room_eog(), Some(true));
        assert_eq!(h.coop, None);
    }

    #[test]
    fn dump_reads_coop_after_eog() {
        let mut sav = dump_through_secretlevel();
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[2u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.coop, Some(1));
        assert_eq!(h.room_coop(), Some(true));
        assert_eq!(h.marker, None);
    }

    #[test]
    fn dump_reads_marker_after_coop() {
        let mut sav = dump_through_secretlevel();
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[2u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.marker, Some(1));
        assert_eq!(h.room_marker(), Some(true));
        assert_eq!(h.ffire, None);
    }

    #[test]
    fn dump_reads_ffire_after_marker() {
        let mut sav = dump_through_secretlevel();
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[2u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.ffire, Some(1));
        assert_eq!(h.room_ffire(), Some(true));
        assert_eq!(h.camsprite, None);
    }

    #[test]
    fn dump_reads_camsprite_after_ffire() {
        let mut sav = dump_through_secretlevel();
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[2u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&42i16.to_le_bytes()));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.camsprite, Some(42));
        assert_eq!(h.room_camsprite(), Some(42));
        assert_eq!(h.connecthead, None);
    }

    #[test]
    fn dump_reads_connecthead_after_camsprite() {
        let mut sav = dump_through_secretlevel();
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[2u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&42i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&0i16.to_le_bytes()));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.connecthead, Some(0));
        assert_eq!(h.room_connecthead(), Some(0));
        assert_eq!(h.connectpoint2, None);
    }

    #[test]
    fn dump_reads_connectpoint2_after_connecthead() {
        let mut sav = dump_through_secretlevel();
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[2u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&42i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&0i16.to_le_bytes()));
        let mut links = [-1i16; 16];
        links[0] = -1;
        let mut raw = [0u8; 32];
        for (i, n) in links.iter().enumerate() {
            raw[i * 2..i * 2 + 2].copy_from_slice(&n.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&raw));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.connectpoint2, Some(links));
        assert_eq!(h.room_connectpoint2(), Some(links));
        assert_eq!(h.numplayersprites, None);
    }

    #[test]
    fn dump_reads_numplayersprites_after_connectpoint2() {
        let mut sav = dump_through_secretlevel();
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[2u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&42i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&0i16.to_le_bytes()));
        let mut links = [-1i16; 16];
        links[0] = -1;
        let mut raw = [0u8; 32];
        for (i, n) in links.iter().enumerate() {
            raw[i * 2..i * 2 + 2].copy_from_slice(&n.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&raw));
        sav.extend(kdfwrite_raw(&1i16.to_le_bytes()));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.numplayersprites, Some(1));
        assert_eq!(h.room_numplayersprites(), Some(1));
        assert_eq!(h.frags, None);
    }

    #[test]
    fn dump_reads_frags_after_numplayersprites() {
        let mut sav = dump_through_secretlevel();
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[2u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&42i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&0i16.to_le_bytes()));
        let mut links = [-1i16; 16];
        links[0] = -1;
        let mut raw = [0u8; 32];
        for (i, n) in links.iter().enumerate() {
            raw[i * 2..i * 2 + 2].copy_from_slice(&n.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&raw));
        sav.extend(kdfwrite_raw(&1i16.to_le_bytes()));
        let mut table = [[0i16; 16]; 16];
        table[0][1] = 3;
        let mut frag_raw = [0u8; 512];
        for (row, slots) in table.iter().enumerate() {
            for (col, n) in slots.iter().enumerate() {
                let o = (row * 16 + col) * 2;
                frag_raw[o..o + 2].copy_from_slice(&n.to_le_bytes());
            }
        }
        sav.extend(kdfwrite_raw(&frag_raw));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.frags, Some(table));
        assert_eq!(h.room_frags(), Some(table));
        assert_eq!(h.room_frags().unwrap()[0][1], 3);
        assert_eq!(h.global_random, None);
    }

    #[test]
    fn dump_reads_global_random_after_frags() {
        let mut sav = dump_through_secretlevel();
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[2u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&42i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&0i16.to_le_bytes()));
        let mut links = [-1i16; 16];
        links[0] = -1;
        let mut raw = [0u8; 32];
        for (i, n) in links.iter().enumerate() {
            raw[i * 2..i * 2 + 2].copy_from_slice(&n.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&raw));
        sav.extend(kdfwrite_raw(&1i16.to_le_bytes()));
        let mut table = [[0i16; 16]; 16];
        table[0][1] = 3;
        let mut frag_raw = [0u8; 512];
        for (row, slots) in table.iter().enumerate() {
            for (col, n) in slots.iter().enumerate() {
                let o = (row * 16 + col) * 2;
                frag_raw[o..o + 2].copy_from_slice(&n.to_le_bytes());
            }
        }
        sav.extend(kdfwrite_raw(&frag_raw));
        sav.extend(kdfwrite_raw(&1234i16.to_le_bytes()));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.global_random, Some(1234));
        assert_eq!(h.room_global_random(), Some(1234));
        assert_eq!(h.parallaxyscale, None);
    }

    #[test]
    fn dump_reads_parallaxyscale_after_global_random() {
        let mut sav = dump_through_secretlevel();
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[2u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&42i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&0i16.to_le_bytes()));
        let mut links = [-1i16; 16];
        links[0] = -1;
        let mut raw = [0u8; 32];
        for (i, n) in links.iter().enumerate() {
            raw[i * 2..i * 2 + 2].copy_from_slice(&n.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&raw));
        sav.extend(kdfwrite_raw(&1i16.to_le_bytes()));
        let mut table = [[0i16; 16]; 16];
        table[0][1] = 3;
        let mut frag_raw = [0u8; 512];
        for (row, slots) in table.iter().enumerate() {
            for (col, n) in slots.iter().enumerate() {
                let o = (row * 16 + col) * 2;
                frag_raw[o..o + 2].copy_from_slice(&n.to_le_bytes());
            }
        }
        sav.extend(kdfwrite_raw(&frag_raw));
        sav.extend(kdfwrite_raw(&1234i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&65536i32.to_le_bytes()));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.parallaxyscale, Some(65536));
        assert_eq!(h.room_parallaxyscale(), Some(65536));
        assert_eq!(h.gotpic, None);
    }

    #[test]
    fn dump_reads_gotpic_and_playerswhenstarted() {
        let mut sav = dump_through_secretlevel();
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[2u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&[1u8]));
        sav.extend(kdfwrite_raw(&42i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&0i16.to_le_bytes()));
        let mut links = [-1i16; 16];
        links[0] = -1;
        let mut raw = [0u8; 32];
        for (i, n) in links.iter().enumerate() {
            raw[i * 2..i * 2 + 2].copy_from_slice(&n.to_le_bytes());
        }
        sav.extend(kdfwrite_raw(&raw));
        sav.extend(kdfwrite_raw(&1i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&[0u8; 512]));
        sav.extend(kdfwrite_raw(&1234i16.to_le_bytes()));
        sav.extend(kdfwrite_raw(&65536i32.to_le_bytes()));
        let mut pic = vec![0u8; 512];
        pic[0] = 1;
        sav.extend(kdfwrite_raw(&pic));
        sav.extend(kdfwrite_raw(&[2u8]));
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.room_gotpic().map(|b| b[0]), Some(1));
        assert_eq!(h.room_playerswhenstarted(), Some(2));
    }
}
