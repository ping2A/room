//! Duke Nukem 3D Atomic `game#.sav` parsing and Room save identification.
//!
//! The prefix follows Chocolate Duke's `loadpheader` and `loadplayer` layout.
//! Array reads consume Ken LZW blocks and reconstruct element-wise byte
//! deltas as `kdfread` does. `SavHeader` preserves the native world, sprite
//! links, cyclers, animation state, player prefix, and CON actor-temp data.
//! `room-duke` applies the supported subset to a native session. The modeled
//! Atomic save can be encoded again, while Room-native sessions still use
//! `.fsave` for complete in-engine state.

use crate::dmo::{kdfread_array, kdfread_blob, kdfread_one, kdfwrite_array_raw};
use crate::error::BuildError;

const SAV_MAXSPRITES: usize = 4096;
/// Atomic `MAXPLAYERS` fixed record count in `ps[]` / `po[]` save arrays.
pub const SAV_MAXPLAYERS: usize = 16;
const SAV_MAXWALLS: usize = 8192;
const SAV_MAXSECTORS: usize = 1024;
const SAV_MAXTILES: usize = 9216;
const SAV_MAXSCRIPTSIZE: usize = 20460;
/// Atomic 1.5's save version in Chocolate Duke's `global.c`.
pub const SAV_ATOMIC_VERSION: i32 = 119;

/// C-ABI witness for the portion of Chocolate's `player_struct` around its
/// raw palette pointer. The preceding members occupy 836 bytes, followed by
/// the byte fields through `refresh_inventory` (62 bytes).
#[repr(C)]
#[allow(dead_code)]
struct AtomicPlayerAbi {
    _prefix: [u8; 836],
    _before_palette: [u8; 62],
    _palette: usize,
    toggle_key_flag: u8,
    knuckle_incs: u8,
    walking_snd_toggle: u8,
    palookup: u8,
    hard_landing: u8,
    max_secret_rooms: u8,
    secret_rooms: u8,
    pals: [u8; 3],
    max_actors_killed: u8,
    actors_killed: u8,
    return_to_center: u8,
    auto_aim: i32,
    weapon_autoswitch: i32,
    fakeplayer: u8,
}

const _: () = assert!(
    core::mem::size_of::<AtomicPlayerAbi>()
        == if cfg!(target_pointer_width = "64") {
            944
        } else {
            932
        }
);
const _: () = assert!(
    core::mem::offset_of!(AtomicPlayerAbi, toggle_key_flag)
        == if cfg!(target_pointer_width = "64") {
            912
        } else {
            904
        }
);

/// Native C `player_struct` size for the current target's pointer width.
/// Chocolate writes the complete `ps[MAXPLAYERS]` array with `sizeof(ps)`.
#[must_use]
pub const fn atomic_player_record_size() -> usize {
    core::mem::size_of::<AtomicPlayerAbi>()
}

/// ABI-dependent fields after Chocolate's raw `palette` pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavPlayerTailOffsets {
    /// `toggle_key_flag` byte offset.
    pub toggle_key_flag: usize,
    /// `knuckle_incs` byte offset.
    pub knuckle_incs: usize,
    /// `walking_snd_toggle` byte offset.
    pub walking_snd_toggle: usize,
    /// `palookup` byte offset.
    pub palookup: usize,
    /// `hard_landing` byte offset.
    pub hard_landing: usize,
    /// `max_secret_rooms` byte offset.
    pub max_secret_rooms: usize,
    /// `secret_rooms` byte offset.
    pub secret_rooms: usize,
    /// `pals[3]` byte-array offset.
    pub pals: usize,
    /// `max_actors_killed` byte offset.
    pub max_actors_killed: usize,
    /// `actors_killed` byte offset.
    pub actors_killed: usize,
    /// `return_to_center` byte offset.
    pub return_to_center: usize,
    /// `auto_aim` 32-bit integer offset.
    pub auto_aim: usize,
}

/// Resolve the raw `player_struct` tail offsets from its serialized ABI stride.
///
/// The source struct places `palette` after byte offset 897. C alignment puts
/// the pointer at 900 on 32-bit and 904 on 64-bit, shifting the remaining
/// members. The 936-byte branch reads Room's earlier v119 draft layout for
/// backwards compatibility. Unknown strides are rejected rather than
/// silently patching them.
#[must_use]
pub const fn atomic_player_tail_offsets(record_size: usize) -> Option<SavPlayerTailOffsets> {
    let (tail, auto_aim) = match record_size {
        932 => (
            if cfg!(target_pointer_width = "32") {
                core::mem::offset_of!(AtomicPlayerAbi, toggle_key_flag)
            } else {
                904
            },
            if cfg!(target_pointer_width = "32") {
                core::mem::offset_of!(AtomicPlayerAbi, auto_aim)
            } else {
                920
            },
        ),
        944 => (
            if cfg!(target_pointer_width = "64") {
                core::mem::offset_of!(AtomicPlayerAbi, toggle_key_flag)
            } else {
                912
            },
            if cfg!(target_pointer_width = "64") {
                core::mem::offset_of!(AtomicPlayerAbi, auto_aim)
            } else {
                928
            },
        ),
        936 => (902, 916),
        _ => return None,
    };
    if record_size == 936 {
        return Some(SavPlayerTailOffsets {
            toggle_key_flag: tail,
            knuckle_incs: tail + 1,
            walking_snd_toggle: tail + 2,
            palookup: tail + 3,
            hard_landing: tail + 4,
            max_secret_rooms: tail + 5,
            secret_rooms: tail + 6,
            pals: tail + 7,
            max_actors_killed: tail + 10,
            actors_killed: tail + 11,
            return_to_center: tail + 12,
            auto_aim,
        });
    }
    Some(SavPlayerTailOffsets {
        toggle_key_flag: tail,
        knuckle_incs: tail + 1,
        walking_snd_toggle: tail + 2,
        palookup: tail + 3,
        hard_landing: tail + 4,
        max_secret_rooms: tail + 5,
        secret_rooms: tail + 6,
        pals: tail + 7,
        max_actors_killed: tail + 10,
        actors_killed: tail + 11,
        return_to_center: tail + 12,
        auto_aim,
    })
}

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
    /// Ceiling X panning.
    pub ceilingxpanning: u8,
    /// Ceiling Y panning.
    pub ceilingypanning: u8,
    /// Floor texture tile.
    pub floorpicnum: i16,
    /// Floor slope heinum.
    pub floorheinum: i16,
    /// Floor shade.
    pub floorshade: i8,
    /// Floor palette.
    pub floorpal: u8,
    /// Floor X panning.
    pub floorxpanning: u8,
    /// Floor Y panning.
    pub floorypanning: u8,
    /// Visibility.
    pub visibility: u8,
    /// Native `sectortype` padding byte retained verbatim.
    pub filler: u8,
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
    /// Native `spritetype` padding byte retained verbatim.
    pub filler: u8,
    /// X repeat.
    pub xrepeat: u8,
    /// Y repeat.
    pub yrepeat: u8,
    /// X offset.
    pub xoffset: i8,
    /// Y offset.
    pub yoffset: i8,
    /// Sector index.
    pub sectnum: i16,
    /// Status list.
    pub statnum: i16,
    /// Angle (Build).
    pub ang: i16,
    /// Owner sprite.
    pub owner: i16,
    /// X velocity.
    pub xvel: i16,
    /// Y velocity.
    pub yvel: i16,
    /// Z velocity.
    pub zvel: i16,
    /// Lotag.
    pub lotag: i16,
    /// Hitag.
    pub hitag: i16,
    /// Extra.
    pub extra: i16,
}

/// Intrusive sprite-list arrays dumped by Atomic `loadplayer`.
///
/// These preserve exact slot order and free-list order; reconstructing the
/// lists from `sectnum` / `statnum` would change the allocator's next sprite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavSpriteLists {
    /// Sector head table (`MAXSECTORS + 1`).
    pub headspritesect: Vec<i16>,
    /// Previous sprite in each sector list (`MAXSPRITES`).
    pub prevspritesect: Vec<i16>,
    /// Next sprite in each sector list (`MAXSPRITES`).
    pub nextspritesect: Vec<i16>,
    /// Status head table (`MAXSTATUS + 1`).
    pub headspritestat: Vec<i16>,
    /// Previous sprite in each status list (`MAXSPRITES`).
    pub prevspritestat: Vec<i16>,
    /// Next sprite in each status list (`MAXSPRITES`).
    pub nextspritestat: Vec<i16>,
}

/// Fixed-size Atomic shade-cycler table as stored by `saveplayer`.
///
/// Each of the `MAXCYCLERS` records occupies 12 bytes. The parser retains the
/// wire bytes because the table is only meaningful together with the loaded
/// Build sectors and must not be normalized before a native restore.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavCyclers {
    /// `MAXCYCLERS` twelve-byte records in original file order.
    pub records: Vec<u8>,
}

/// Atomic CON/runtime state serialized between cloud arrays and `lockclock`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavScriptState {
    /// `MAXSCRIPTSIZE` pointer-presence flags for the CON script array.
    pub script_pointer_flags: Vec<u8>,
    /// `MAXSCRIPTSIZE` signed script values (saved pointers are offsets).
    pub script_words: Vec<i32>,
    /// `MAXTILES` saved actor-script pointer offsets.
    pub actor_script_offsets: Vec<i32>,
    /// `MAXSPRITES` bitfield indicating actor-temp script pointers.
    pub sprite_script_flags: Vec<u8>,
    /// Raw 72-byte Atomic `weaponhit` records for all `MAXSPRITES` slots.
    pub weapon_hits: Vec<u8>,
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavPlayer {
    /// Full original 32-bit Atomic `player_struct` bytes. Keeping this opaque
    /// tail allows future writers to preserve fields Room does not model yet.
    pub raw_record: Vec<u8>,
    /// `offsetof(player_struct, posx)`; this prefix is pointer-width invariant.
    /// Build X.
    pub posx: i32,
    /// Build Y.
    pub posy: i32,
    /// Build Z.
    pub posz: i32,
    /// Remaining lizard-spit overlays (`numloogs`) from the raw player prefix.
    pub num_loogs: Option<i32>,
    /// Lizard-spit countdown (`loogcnt`) from the raw player prefix.
    pub loogie_tics: Option<i32>,
    /// First six saved loogie X positions from `loogiex[64]`.
    pub loogie_x: Option<[i32; 6]>,
    /// First six saved loogie Y positions from `loogiey[64]`.
    pub loogie_y: Option<[i32; 6]>,
    /// XY momentum.
    pub posxv: Option<i32>,
    /// Y momentum.
    pub posyv: Option<i32>,
    /// Vertical momentum.
    pub poszv: Option<i32>,
    /// Chocolate `player_struct::truefz` at player-record offset POSX+72.
    pub true_floor_z: Option<i32>,
    /// Chocolate `player_struct::truecz` at player-record offset POSX+76.
    pub true_ceiling_z: Option<i32>,
    /// Chocolate `player_struct::bobcounter` at player-record offset POSX+88.
    pub bob_counter: Option<i32>,
    /// Current Build sector (`cursectnum`).
    pub cursectnum: Option<i16>,
    /// Player body sprite (`i`).
    pub sprite_i: Option<i16>,
    /// Atomic third-person camera toggle (`over_shoulder_on`).
    pub over_shoulder_on: Option<bool>,
    /// Previous-tic pose.
    pub oposx: Option<i32>,
    /// Previous Y.
    pub oposy: Option<i32>,
    /// Previous Z.
    pub oposz: Option<i32>,
    /// Previous pose used for walk bob.
    pub bobposx: Option<i32>,
    /// Previous Y used for walk bob.
    pub bobposy: Option<i32>,
    /// View bob offset.
    pub pyoff: Option<i32>,
    /// Horizon (100 = level).
    pub horiz: i32,
    /// Previous horizon (`ohoriz`) when the blob is long enough.
    pub ohoriz: Option<i32>,
    /// Previous look offset (`ohorizoff`) when the blob is long enough.
    pub ohorizoff: Option<i32>,
    /// Build angle (0..=2047).
    pub ang: i16,
    /// Chocolate's signed `player_struct::frag` score counter.
    pub frag: Option<i16>,
    /// Chocolate's signed `player_struct::fraggedself` counter.
    pub fraggedself: Option<i16>,
    /// Chocolate `player_struct::aim_mode` (32-bit Atomic field).
    pub aim_mode: Option<i32>,
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
    /// Atomic exit-fist animation (`fist_incs`) when the blob is long enough.
    pub fist_incs: Option<i16>,
    /// Palette of the last activated nuke button (`buttonpalette`).
    pub button_palette: Option<u8>,
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
    /// Weapon reservation entries and active length.
    pub weapon_reservations: Option<[i16; 16]>,
    /// Number of valid reserved sprite entries.
    pub weapon_reservation_count: Option<i16>,
    /// Countdown written by the CON `endofgame` action.
    pub time_before_exit: Option<i16>,
    /// Number of secret rooms visited.
    pub secret_rooms: Option<u8>,
    /// Map's total secret-room count (`max_secret_rooms`).
    pub max_secret_rooms: Option<u8>,
    /// Number of actors killed.
    pub actors_killed: Option<u8>,
    /// Map's total counted enemies (`max_actors_killed`).
    pub max_actors_killed: Option<u8>,
    /// Chocolate player's auto-aim mode.
    pub auto_aim: Option<i32>,
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
    /// The 160×100 save-screen tile payload written after the header. Retained
    /// because Chocolate's loader consumes it before reading the Build world.
    pub save_thumbnail: Option<Vec<u8>>,
    /// Dump prefix: `numwalls` (`short`) when the board blob is present.
    pub numwalls: Option<i32>,
    /// Dump prefix: first `numwalls` `walltype` records when the blob is long enough.
    pub walls: Option<Vec<SavWall>>,
    /// Dump prefix: `numsectors` (`short`) after the walls blob.
    pub numsectors: Option<i32>,
    /// Dump prefix: first `numsectors` `sectortype` records when the blob is long enough.
    pub sectors: Option<Vec<SavSector>>,
    /// Effective sprite-array length. Atomic saves always store all
    /// `SAV_MAXSPRITES`; the explicit short is accepted only in legacy compact
    /// fixtures predating the native fixed-size save layout.
    pub numsprites: Option<i32>,
    /// Dump prefix: first `numsprites` `spritetype` records when the blob is long enough.
    pub sprites: Option<Vec<SavSprite>>,
    /// Dump arrays preserving sector/status and free-list links for every sprite slot.
    pub sprite_lists: Option<SavSpriteLists>,
    /// Dump prefix: `numcyclers` after the sprite link lists.
    pub numcyclers: Option<i32>,
    /// Fixed 256 × 12-byte Atomic shade-cycler table after `numcyclers`.
    pub cyclers: Option<SavCyclers>,
    /// Dump prefix: first `ps[]` pose when the blob is long enough.
    pub player: Option<SavPlayer>,
    /// Opaque Atomic `po[MAXPLAYERS]` original-player pose array.
    pub player_orig: Option<Vec<u8>>,
    /// Dump prefix: `numanimwalls` after the cyclers / `ps[]` / `po[]` blobs.
    pub numanimwalls: Option<i32>,
    /// Fixed 512 × 6-byte `animwalltype` records following `numanimwalls`.
    pub animwalls: Option<Vec<u8>>,
    /// Fixed 2048-entry wall X animation target table.
    pub msx: Option<Vec<i32>>,
    /// Fixed 2048-entry wall Y animation target table.
    pub msy: Option<Vec<i32>>,
    /// `spriteqloc` circular-list cursor.
    pub spriteqloc: Option<i32>,
    /// Dump prefix: `spriteqamount` after animwalls / mirrors / `spriteqloc`.
    pub spriteqamount: Option<i32>,
    /// Active sprite deletion queue entries, in allocation order.
    pub spriteq: Option<Vec<i16>>,
    /// Number of active mirror records.
    pub mirror_count: Option<i32>,
    /// Fixed 64-entry mirror wall table.
    pub mirror_walls: Option<Vec<i16>>,
    /// Fixed 64-entry mirror sector table.
    pub mirror_sectors: Option<Vec<i16>>,
    /// Atomic 2D automap sector bitset (`MAXSECTORS / 8`).
    pub show2dsector: Option<Vec<u8>>,
    /// Fixed `actortype[MAXTILES]` table used by CON actor dispatch.
    pub actor_types: Option<Vec<u8>>,
    /// Fixed 128-byte saved board name, including its original terminator.
    pub boardfilename: Option<Vec<u8>>,
    /// Dump prefix: `numclouds` after `spriteqamount`.
    pub numclouds: Option<i32>,
    /// Fixed 128-entry cloud sprite index table.
    pub clouds: Option<Vec<i16>>,
    /// Fixed 128-entry cloud X offset table.
    pub cloudx: Option<Vec<i16>>,
    /// Fixed 128-entry cloud Y offset table.
    pub cloudy: Option<Vec<i16>>,
    /// Script and actor-temp payload saved before `lockclock`.
    pub script_state: Option<SavScriptState>,
    /// `randomseed` after the player/frag arrays.
    pub randomseed: Option<i32>,
    /// `lockclock` (Duke 30 Hz tics) after script and actor-temp arrays.
    pub lockclock: Option<i32>,
    /// Dump prefix: `pskybits` (`short`) after `lockclock`.
    pub pskybits: Option<i32>,
    /// Dump prefix: eight `pskyoff` slots (`short[8]`) after `pskybits`.
    pub pskyoff: Option<[i16; 8]>,
    /// Dump prefix: `animatecnt` (`long`) after `pskyoff`.
    pub animatecnt: Option<i32>,
    /// Fixed 64-entry `animatesect` table, with active entries first.
    pub animatesect: Option<Vec<i16>>,
    /// Dump prefix: first `animatecnt` `animategoal` slots (`long[64]` blob).
    pub animategoal: Option<Vec<i32>>,
    /// Dump prefix: first `animatecnt` `animatevel` slots (`long[64]` blob).
    pub animatevel: Option<Vec<i32>>,
    /// Dump prefix: first `animatecnt` `animateptr` slots (`long[64]` blob).
    pub animateptr: Option<Vec<i32>>,
    /// Dump prefix: `earthquaketime` (`char`) after `animateptr`.
    pub earthquaketime: Option<i32>,
    /// Dump prefix: `ud.from_bonus` (`short`) after `earthquaketime`.
    pub from_bonus: Option<i32>,
    /// Dump prefix: `ud.secretlevel` (`short`) after `from_bonus`.
    pub secretlevel: Option<i32>,
    /// Dump prefix: `ud.respawn_monsters` (`long`) after `secretlevel`.
    pub respawn_monsters: Option<i32>,
    /// Dump prefix: `ud.respawn_items` (`long`) after `respawn_monsters`.
    pub respawn_items: Option<i32>,
    /// Dump prefix: `ud.respawn_inventory` (`long`) after `respawn_items`.
    pub respawn_inventory: Option<i32>,
    /// Dump prefix: `ud.god` (`char`) after `respawn_inventory`.
    pub god: Option<i32>,
    /// Dump prefix: `ud.auto_run` (`long`) after `god`.
    pub auto_run: Option<i32>,
    /// Dump prefix: `ud.crosshair` (`long`) after `auto_run`.
    pub crosshair: Option<i32>,
    /// Dump prefix: `ud.monsters_off` (`long`) after `crosshair`.
    pub monsters_off: Option<i32>,
    /// Dump prefix: `ud.last_level` (`short`) after `monsters_off`.
    pub last_level: Option<i32>,
    /// Dump prefix: `ud.eog` (`char`) after `last_level`.
    pub eog: Option<i32>,
    /// Dump prefix: `ud.coop` (`long`) after `eog`.
    pub coop: Option<i32>,
    /// Dump prefix: `ud.marker` (`long`) after `coop`.
    pub marker: Option<i32>,
    /// Dump prefix: `ud.ffire` (`long`) after `marker`.
    pub ffire: Option<i32>,
    /// Dump prefix: `camsprite` (`short`) after `ffire`.
    pub camsprite: Option<i32>,
    /// Dump prefix: `connecthead` (`short`) after `camsprite`.
    pub connecthead: Option<i32>,
    /// Dump prefix: sixteen `connectpoint2` slots (`short[16]`) after `connecthead`.
    pub connectpoint2: Option<[i16; 16]>,
    /// Dump prefix: `numplayersprites` (`byte`) after `connectpoint2`.
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
    /// Create a complete zero-initialized Atomic 1.5 save model for a live
    /// Room Build world. The session writer must replace geometry, player,
    /// script and runtime arrays before encoding.
    pub fn blank_atomic(
        volume: i32,
        level: i32,
        skill: i32,
        players: usize,
        numwalls: usize,
        numsectors: usize,
        boardfilename: &[u8],
    ) -> Result<Self, BuildError> {
        if !(0..=3).contains(&volume)
            || !(0..=15).contains(&level)
            || !(0..=4).contains(&skill)
            || !(1..=8).contains(&players)
            || numwalls > SAV_MAXWALLS
            || numsectors > SAV_MAXSECTORS
            || boardfilename.len() >= 128
        {
            return Err(BuildError::Invalid("Atomic SAV initial dimensions"));
        }
        let mut board = vec![0; 128];
        board[..boardfilename.len()].copy_from_slice(boardfilename);
        let player_stride = atomic_player_record_size();
        let player_records = vec![0; player_stride * SAV_MAXPLAYERS];
        let mut player = parse_sav_player(&player_records[..player_stride])
            .ok_or(BuildError::Invalid("Atomic player record ABI"))?;
        player.raw_record = player_records;

        Ok(Self {
            version: SAV_ATOMIC_VERSION,
            players: i32::try_from(players).map_err(|_| BuildError::Invalid("SAV players"))?,
            name: "ROOM SAVE".to_owned(),
            volume,
            level,
            skill,
            save_thumbnail: Some(vec![0; 160 * 100]),
            numwalls: Some(i32::try_from(numwalls).map_err(|_| BuildError::Invalid("SAV walls"))?),
            walls: Some(vec![
                SavWall {
                    x: 0,
                    y: 0,
                    point2: -1,
                    nextwall: -1,
                    nextsector: -1,
                    cstat: 0,
                    picnum: 0,
                    overpicnum: 0,
                    shade: 0,
                    pal: 0,
                    xrepeat: 0,
                    yrepeat: 0,
                    xpanning: 0,
                    ypanning: 0,
                    lotag: 0,
                    hitag: 0,
                    extra: -1
                };
                numwalls
            ]),
            numsectors: Some(
                i32::try_from(numsectors).map_err(|_| BuildError::Invalid("SAV sectors"))?,
            ),
            sectors: Some(vec![
                SavSector {
                    wallptr: 0,
                    wallnum: 0,
                    ceilingz: 0,
                    floorz: 0,
                    ceilingstat: 0,
                    floorstat: 0,
                    ceilingpicnum: 0,
                    ceilingheinum: 0,
                    ceilingshade: 0,
                    ceilingpal: 0,
                    ceilingxpanning: 0,
                    ceilingypanning: 0,
                    floorpicnum: 0,
                    floorheinum: 0,
                    floorshade: 0,
                    floorpal: 0,
                    floorxpanning: 0,
                    floorypanning: 0,
                    visibility: 0,
                    filler: 0,
                    lotag: 0,
                    hitag: 0,
                    extra: -1
                };
                numsectors
            ]),
            numsprites: Some(SAV_MAXSPRITES as i32),
            sprites: Some(vec![
                SavSprite {
                    x: 0,
                    y: 0,
                    z: 0,
                    cstat: 0,
                    picnum: 0,
                    shade: 0,
                    pal: 0,
                    clipdist: 0,
                    filler: 0,
                    xrepeat: 0,
                    yrepeat: 0,
                    xoffset: 0,
                    yoffset: 0,
                    sectnum: -1,
                    statnum: -1,
                    ang: 0,
                    owner: -1,
                    xvel: 0,
                    yvel: 0,
                    zvel: 0,
                    lotag: 0,
                    hitag: 0,
                    extra: -1
                };
                SAV_MAXSPRITES
            ]),
            sprite_lists: Some(SavSpriteLists {
                headspritesect: vec![-1; SAV_MAXSECTORS + 1],
                prevspritesect: vec![-1; SAV_MAXSPRITES],
                nextspritesect: vec![-1; SAV_MAXSPRITES],
                headspritestat: vec![-1; 1025],
                prevspritestat: vec![-1; SAV_MAXSPRITES],
                nextspritestat: vec![-1; SAV_MAXSPRITES],
            }),
            numcyclers: Some(0),
            cyclers: Some(SavCyclers {
                records: vec![0; 256 * 12],
            }),
            player: Some(player),
            player_orig: Some(vec![0; SAV_MAXPLAYERS * 16]),
            numanimwalls: Some(0),
            animwalls: Some(vec![0; 512 * 6]),
            msx: Some(vec![0; 2048]),
            msy: Some(vec![0; 2048]),
            spriteqloc: Some(0),
            spriteqamount: Some(0),
            spriteq: Some(Vec::new()),
            mirror_count: Some(0),
            mirror_walls: Some(vec![-1; 64]),
            mirror_sectors: Some(vec![-1; 64]),
            show2dsector: Some(vec![0; 128]),
            actor_types: Some(vec![0; SAV_MAXTILES]),
            boardfilename: Some(board),
            numclouds: Some(0),
            clouds: Some(vec![0; 128]),
            cloudx: Some(vec![0; 128]),
            cloudy: Some(vec![0; 128]),
            script_state: Some(SavScriptState {
                script_pointer_flags: vec![0; SAV_MAXSCRIPTSIZE],
                script_words: vec![0; SAV_MAXSCRIPTSIZE],
                actor_script_offsets: vec![0; SAV_MAXTILES],
                sprite_script_flags: vec![0; SAV_MAXSPRITES],
                weapon_hits: vec![0; SAV_MAXSPRITES * 72],
            }),
            randomseed: Some(0),
            lockclock: Some(0),
            pskybits: Some(0),
            pskyoff: Some([0; 8]),
            animatecnt: Some(0),
            animatesect: Some(vec![0; 64]),
            animategoal: Some(vec![0; 64]),
            animatevel: Some(vec![0; 64]),
            animateptr: Some(vec![0; 64]),
            earthquaketime: Some(0),
            from_bonus: Some(0),
            secretlevel: Some(0),
            respawn_monsters: Some(0),
            respawn_items: Some(0),
            respawn_inventory: Some(0),
            god: Some(0),
            auto_run: Some(0),
            crosshair: Some(1),
            monsters_off: Some(0),
            last_level: Some(0),
            eog: Some(0),
            coop: Some(0),
            marker: Some(0),
            ffire: Some(0),
            camsprite: Some(-1),
            connecthead: Some(0),
            connectpoint2: Some([-1; SAV_MAXPLAYERS]),
            numplayersprites: Some(0),
            frags: Some([[0; SAV_MAXPLAYERS]; SAV_MAXPLAYERS]),
            global_random: Some(0),
            parallaxyscale: Some(0),
            gotpic: Some(vec![0; SAV_MAXTILES.div_ceil(8)]),
            playerswhenstarted: Some(
                i32::try_from(players).map_err(|_| BuildError::Invalid("SAV players"))?,
            ),
        })
    }

    /// Parse one fixed Atomic `ps[MAXPLAYERS]` record from the saved player
    /// array. Returns `None` for compact diagnostic dumps or malformed arrays.
    #[must_use]
    pub fn player_record(&self, seat: usize) -> Option<SavPlayer> {
        if seat >= SAV_MAXPLAYERS {
            return None;
        }
        let all_records = &self.player.as_ref()?.raw_record;
        if all_records.len() % SAV_MAXPLAYERS != 0 {
            return None;
        }
        let stride = all_records.len() / SAV_MAXPLAYERS;
        let start = seat.checked_mul(stride)?;
        let end = start.checked_add(stride)?;
        parse_sav_player(all_records.get(start..end)?)
    }

    /// Encode the Atomic save prefix through its 160×100 thumbnail.
    ///
    /// This is the portion written by Chocolate's `saveplayer` before the
    /// Build world arrays. Each field uses `dfwrite`'s element-wise delta and
    /// raw Ken block encoding. It deliberately does not claim to serialize a
    /// complete save until the native world and script-state tail are emitted.
    pub fn encode_atomic_prefix(&self) -> Result<Vec<u8>, BuildError> {
        fn append_array(
            out: &mut Vec<u8>,
            data: &[u8],
            element_size: usize,
            count: usize,
        ) -> Result<(), BuildError> {
            let encoded = kdfwrite_array_raw(data, element_size, count)
                .ok_or(BuildError::Invalid("SAV prefix field"))?;
            out.extend_from_slice(&encoded);
            Ok(())
        }

        let mut out = Vec::new();
        append_array(&mut out, &self.version.to_le_bytes(), 4, 1)?;
        append_array(&mut out, &self.players.to_le_bytes(), 4, 1)?;
        let mut name = [0_u8; 19];
        let name_bytes = self.name.as_bytes();
        let name_len = name_bytes.len().min(name.len());
        name[..name_len].copy_from_slice(&name_bytes[..name_len]);
        append_array(&mut out, &name, name.len(), 1)?;
        append_array(&mut out, &self.volume.to_le_bytes(), 4, 1)?;
        append_array(&mut out, &self.level.to_le_bytes(), 4, 1)?;
        append_array(&mut out, &self.skill.to_le_bytes(), 4, 1)?;
        let blank_thumbnail = vec![0_u8; 160 * 100];
        let thumbnail = self.save_thumbnail.as_deref().unwrap_or(&blank_thumbnail);
        if thumbnail.len() != 160 * 100 {
            return Err(BuildError::Invalid("SAV thumbnail size"));
        }
        append_array(&mut out, thumbnail, 160, 100)?;
        Ok(out)
    }

    /// Encode the Atomic header, thumbnail, and fixed Build geometry arrays.
    ///
    /// The sprite array is emitted directly after the sector array, with no
    /// intervening count, exactly as `saveplayer` does. Link lists and the
    /// remaining gameplay/script state still need to be appended for a full
    /// `GAME#.SAV` writer.
    pub fn encode_atomic_geometry_prefix(&self) -> Result<Vec<u8>, BuildError> {
        fn put_i16(out: &mut Vec<u8>, value: i32) -> Result<(), BuildError> {
            let value = i16::try_from(value).map_err(|_| BuildError::Invalid("SAV count"))?;
            let encoded = kdfwrite_array_raw(&value.to_le_bytes(), 2, 1)
                .ok_or(BuildError::Invalid("SAV count"))?;
            out.extend_from_slice(&encoded);
            Ok(())
        }

        fn append_array(
            out: &mut Vec<u8>,
            data: &[u8],
            element_size: usize,
            count: usize,
        ) -> Result<(), BuildError> {
            let encoded = kdfwrite_array_raw(data, element_size, count)
                .ok_or(BuildError::Invalid("SAV geometry array"))?;
            out.extend_from_slice(&encoded);
            Ok(())
        }

        let numwalls = self.numwalls.ok_or(BuildError::Invalid("SAV numwalls"))?;
        let numsectors = self
            .numsectors
            .ok_or(BuildError::Invalid("SAV numsectors"))?;
        let walls = self
            .walls
            .as_deref()
            .ok_or(BuildError::Invalid("SAV walls"))?;
        let sectors = self
            .sectors
            .as_deref()
            .ok_or(BuildError::Invalid("SAV sectors"))?;
        let sprites = self
            .sprites
            .as_deref()
            .ok_or(BuildError::Invalid("SAV sprites"))?;
        let numwalls_usize =
            usize::try_from(numwalls).map_err(|_| BuildError::Invalid("SAV numwalls"))?;
        let numsectors_usize =
            usize::try_from(numsectors).map_err(|_| BuildError::Invalid("SAV numsectors"))?;
        if numwalls_usize > SAV_MAXWALLS
            || numsectors_usize > SAV_MAXSECTORS
            || walls.len() < numwalls_usize
            || sectors.len() < numsectors_usize
            || sprites.len() > SAV_MAXSPRITES
        {
            return Err(BuildError::Invalid("SAV geometry dimensions"));
        }

        let mut out = self.encode_atomic_prefix()?;
        put_i16(&mut out, numwalls)?;
        let mut wall_bytes = vec![0_u8; SAV_MAXWALLS * 32];
        for (i, wall) in walls.iter().take(numwalls_usize).enumerate() {
            let record = &mut wall_bytes[i * 32..(i + 1) * 32];
            record[0..4].copy_from_slice(&wall.x.to_le_bytes());
            record[4..8].copy_from_slice(&wall.y.to_le_bytes());
            for (field, value) in [
                (8, wall.point2),
                (10, wall.nextwall),
                (12, wall.nextsector),
                (14, wall.cstat),
                (16, wall.picnum),
                (18, wall.overpicnum),
            ] {
                record[field..field + 2].copy_from_slice(&value.to_le_bytes());
            }
            record[20] = wall.shade as u8;
            record[21] = wall.pal;
            record[22] = wall.xrepeat;
            record[23] = wall.yrepeat;
            record[24] = wall.xpanning;
            record[25] = wall.ypanning;
            for (field, value) in [(26, wall.lotag), (28, wall.hitag), (30, wall.extra)] {
                record[field..field + 2].copy_from_slice(&value.to_le_bytes());
            }
        }
        append_array(&mut out, &wall_bytes, 32, SAV_MAXWALLS)?;

        put_i16(&mut out, numsectors)?;
        let mut sector_bytes = vec![0_u8; SAV_MAXSECTORS * 40];
        for (i, sector) in sectors.iter().take(numsectors_usize).enumerate() {
            let record = &mut sector_bytes[i * 40..(i + 1) * 40];
            for (field, value) in [(0, sector.wallptr), (2, sector.wallnum)] {
                record[field..field + 2].copy_from_slice(&value.to_le_bytes());
            }
            record[4..8].copy_from_slice(&sector.ceilingz.to_le_bytes());
            record[8..12].copy_from_slice(&sector.floorz.to_le_bytes());
            for (field, value) in [
                (12, sector.ceilingstat),
                (14, sector.floorstat),
                (16, sector.ceilingpicnum),
                (18, sector.ceilingheinum),
            ] {
                record[field..field + 2].copy_from_slice(&value.to_le_bytes());
            }
            record[20] = sector.ceilingshade as u8;
            record[21] = sector.ceilingpal;
            record[22] = sector.ceilingxpanning;
            record[23] = sector.ceilingypanning;
            for (field, value) in [(24, sector.floorpicnum), (26, sector.floorheinum)] {
                record[field..field + 2].copy_from_slice(&value.to_le_bytes());
            }
            record[28] = sector.floorshade as u8;
            record[29] = sector.floorpal;
            record[30] = sector.floorxpanning;
            record[31] = sector.floorypanning;
            record[32] = sector.visibility;
            record[33] = sector.filler;
            for (field, value) in [(34, sector.lotag), (36, sector.hitag), (38, sector.extra)] {
                record[field..field + 2].copy_from_slice(&value.to_le_bytes());
            }
        }
        append_array(&mut out, &sector_bytes, 40, SAV_MAXSECTORS)?;

        let mut sprite_bytes = vec![0_u8; SAV_MAXSPRITES * 44];
        for (i, sprite) in sprites.iter().enumerate() {
            let record = &mut sprite_bytes[i * 44..(i + 1) * 44];
            record[0..4].copy_from_slice(&sprite.x.to_le_bytes());
            record[4..8].copy_from_slice(&sprite.y.to_le_bytes());
            record[8..12].copy_from_slice(&sprite.z.to_le_bytes());
            for (field, value) in [(12, sprite.cstat), (14, sprite.picnum)] {
                record[field..field + 2].copy_from_slice(&value.to_le_bytes());
            }
            record[16] = sprite.shade as u8;
            record[17] = sprite.pal;
            record[18] = sprite.clipdist;
            record[19] = sprite.filler;
            record[20] = sprite.xrepeat;
            record[21] = sprite.yrepeat;
            record[22] = sprite.xoffset as u8;
            record[23] = sprite.yoffset as u8;
            for (field, value) in [
                (24, sprite.sectnum),
                (26, sprite.statnum),
                (28, sprite.ang),
                (30, sprite.owner),
                (32, sprite.xvel),
                (34, sprite.yvel),
                (36, sprite.zvel),
                (38, sprite.lotag),
                (40, sprite.hitag),
                (42, sprite.extra),
            ] {
                record[field..field + 2].copy_from_slice(&value.to_le_bytes());
            }
        }
        append_array(&mut out, &sprite_bytes, 44, SAV_MAXSPRITES)?;
        Ok(out)
    }

    /// Continue the native save through sprite links and sector cyclers.
    pub fn encode_atomic_lists_prefix(&self) -> Result<Vec<u8>, BuildError> {
        fn append_shorts(
            out: &mut Vec<u8>,
            values: &[i16],
            expected: usize,
        ) -> Result<(), BuildError> {
            if values.len() != expected {
                return Err(BuildError::Invalid("SAV sprite-list length"));
            }
            let mut bytes = Vec::with_capacity(expected * 2);
            for value in values {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            let encoded = kdfwrite_array_raw(&bytes, 2, expected)
                .ok_or(BuildError::Invalid("SAV sprite-list data"))?;
            out.extend_from_slice(&encoded);
            Ok(())
        }

        let lists = self
            .sprite_lists
            .as_ref()
            .ok_or(BuildError::Invalid("SAV sprite lists"))?;
        let mut out = self.encode_atomic_geometry_prefix()?;
        append_shorts(&mut out, &lists.headspritesect, SAV_MAXSECTORS + 1)?;
        append_shorts(&mut out, &lists.prevspritesect, SAV_MAXSPRITES)?;
        append_shorts(&mut out, &lists.nextspritesect, SAV_MAXSPRITES)?;
        append_shorts(&mut out, &lists.headspritestat, 1025)?;
        append_shorts(&mut out, &lists.prevspritestat, SAV_MAXSPRITES)?;
        append_shorts(&mut out, &lists.nextspritestat, SAV_MAXSPRITES)?;

        let numcyclers = self
            .numcyclers
            .ok_or(BuildError::Invalid("SAV numcyclers"))?;
        if !(0..=256).contains(&numcyclers) {
            return Err(BuildError::Invalid("SAV numcyclers"));
        }
        let numcyclers =
            i16::try_from(numcyclers).map_err(|_| BuildError::Invalid("SAV numcyclers"))?;
        let encoded_count = kdfwrite_array_raw(&numcyclers.to_le_bytes(), 2, 1)
            .ok_or(BuildError::Invalid("SAV numcyclers"))?;
        out.extend_from_slice(&encoded_count);
        let cyclers = self
            .cyclers
            .as_ref()
            .ok_or(BuildError::Invalid("SAV cyclers"))?;
        if cyclers.records.len() != 256 * 12 {
            return Err(BuildError::Invalid("SAV cycler table"));
        }
        let encoded_cyclers = kdfwrite_array_raw(&cyclers.records, 12, 256)
            .ok_or(BuildError::Invalid("SAV cycler table"))?;
        out.extend_from_slice(&encoded_cyclers);
        Ok(out)
    }

    /// Continue the native save through player poses and animated walls.
    pub fn encode_atomic_player_prefix(&self) -> Result<Vec<u8>, BuildError> {
        fn append_array(
            out: &mut Vec<u8>,
            data: &[u8],
            element_size: usize,
            count: usize,
            label: &'static str,
        ) -> Result<(), BuildError> {
            let encoded =
                kdfwrite_array_raw(data, element_size, count).ok_or(BuildError::Invalid(label))?;
            out.extend_from_slice(&encoded);
            Ok(())
        }

        fn append_short(out: &mut Vec<u8>, value: i32) -> Result<(), BuildError> {
            let value = i16::try_from(value).map_err(|_| BuildError::Invalid("SAV short"))?;
            append_array(out, &value.to_le_bytes(), 2, 1, "SAV short")
        }

        fn append_i32_array(
            out: &mut Vec<u8>,
            values: &[i32],
            count: usize,
        ) -> Result<(), BuildError> {
            if values.len() != count {
                return Err(BuildError::Invalid("SAV animation table length"));
            }
            let mut bytes = Vec::with_capacity(count * 4);
            for value in values {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            append_array(out, &bytes, 4, count, "SAV animation table")
        }

        let player = self
            .player
            .as_ref()
            .ok_or(BuildError::Invalid("SAV player pose"))?;
        let player_orig = self
            .player_orig
            .as_deref()
            .ok_or(BuildError::Invalid("SAV previous player poses"))?;
        let numanimwalls = self
            .numanimwalls
            .ok_or(BuildError::Invalid("SAV numanimwalls"))?;
        if !(0..=512).contains(&numanimwalls) {
            return Err(BuildError::Invalid("SAV numanimwalls"));
        }
        let animwalls = self
            .animwalls
            .as_deref()
            .filter(|bytes| bytes.len() == 512 * 6)
            .ok_or(BuildError::Invalid("SAV animation walls"))?;
        let msx = self
            .msx
            .as_deref()
            .ok_or(BuildError::Invalid("SAV animation X table"))?;
        let msy = self
            .msy
            .as_deref()
            .ok_or(BuildError::Invalid("SAV animation Y table"))?;

        let mut out = self.encode_atomic_lists_prefix()?;
        append_array(
            &mut out,
            &player.raw_record,
            player.raw_record.len(),
            1,
            "SAV player pose",
        )?;
        append_array(
            &mut out,
            player_orig,
            player_orig.len(),
            1,
            "SAV previous player poses",
        )?;
        append_short(&mut out, numanimwalls)?;
        append_array(
            &mut out,
            animwalls,
            animwalls.len(),
            1,
            "SAV animation walls",
        )?;
        append_i32_array(&mut out, msx, 2048)?;
        append_i32_array(&mut out, msy, 2048)?;
        Ok(out)
    }

    /// Continue the native save through maps, clouds and CON/actor state.
    pub fn encode_atomic_script_prefix(&self) -> Result<Vec<u8>, BuildError> {
        fn append_array(
            out: &mut Vec<u8>,
            data: &[u8],
            element_size: usize,
            count: usize,
            label: &'static str,
        ) -> Result<(), BuildError> {
            let encoded =
                kdfwrite_array_raw(data, element_size, count).ok_or(BuildError::Invalid(label))?;
            out.extend_from_slice(&encoded);
            Ok(())
        }

        fn append_short(out: &mut Vec<u8>, value: i32) -> Result<(), BuildError> {
            let value = i16::try_from(value).map_err(|_| BuildError::Invalid("SAV short"))?;
            append_array(out, &value.to_le_bytes(), 2, 1, "SAV short")
        }

        fn append_shorts(
            out: &mut Vec<u8>,
            values: &[i16],
            count: usize,
            label: &'static str,
        ) -> Result<(), BuildError> {
            if values.len() != count {
                return Err(BuildError::Invalid(label));
            }
            let mut bytes = Vec::with_capacity(count * 2);
            for value in values {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            append_array(out, &bytes, 2, count, label)
        }

        let mut out = self.encode_atomic_player_prefix()?;
        append_short(
            &mut out,
            self.spriteqloc
                .ok_or(BuildError::Invalid("SAV spriteqloc"))?,
        )?;
        let spriteqamount = self
            .spriteqamount
            .ok_or(BuildError::Invalid("SAV spriteqamount"))?;
        if !(0..=1024).contains(&spriteqamount) {
            return Err(BuildError::Invalid("SAV spriteqamount"));
        }
        append_short(&mut out, spriteqamount)?;
        if spriteqamount > 0 {
            append_shorts(
                &mut out,
                self.spriteq
                    .as_deref()
                    .ok_or(BuildError::Invalid("SAV spriteq"))?,
                spriteqamount as usize,
                "SAV spriteq",
            )?;
        }

        let mirror_count = self
            .mirror_count
            .ok_or(BuildError::Invalid("SAV mirror count"))?;
        if !(0..=64).contains(&mirror_count) {
            return Err(BuildError::Invalid("SAV mirror count"));
        }
        append_short(&mut out, mirror_count)?;
        append_shorts(
            &mut out,
            self.mirror_walls
                .as_deref()
                .ok_or(BuildError::Invalid("SAV mirror walls"))?,
            64,
            "SAV mirror walls",
        )?;
        append_shorts(
            &mut out,
            self.mirror_sectors
                .as_deref()
                .ok_or(BuildError::Invalid("SAV mirror sectors"))?,
            64,
            "SAV mirror sectors",
        )?;

        let show2dsector = self
            .show2dsector
            .as_deref()
            .filter(|bytes| bytes.len() == 128)
            .ok_or(BuildError::Invalid("SAV automap bits"))?;
        append_array(&mut out, show2dsector, 1, 128, "SAV automap bits")?;
        let actor_types = self
            .actor_types
            .as_deref()
            .filter(|bytes| bytes.len() == SAV_MAXTILES)
            .ok_or(BuildError::Invalid("SAV actor types"))?;
        append_array(&mut out, actor_types, 1, SAV_MAXTILES, "SAV actor types")?;
        let boardfilename = self
            .boardfilename
            .as_deref()
            .filter(|bytes| bytes.len() == 128)
            .ok_or(BuildError::Invalid("SAV board filename"))?;
        append_array(&mut out, boardfilename, 128, 1, "SAV board filename")?;

        let numclouds = self.numclouds.ok_or(BuildError::Invalid("SAV numclouds"))?;
        if !(0..=128).contains(&numclouds) {
            return Err(BuildError::Invalid("SAV numclouds"));
        }
        append_short(&mut out, numclouds)?;
        for table in [&self.clouds, &self.cloudx, &self.cloudy] {
            let values = table
                .as_deref()
                .ok_or(BuildError::Invalid("SAV cloud table"))?;
            append_shorts(&mut out, values, 128, "SAV cloud table")?;
        }

        let script = self
            .script_state
            .as_ref()
            .ok_or(BuildError::Invalid("SAV script state"))?;
        if script.script_pointer_flags.len() != SAV_MAXSCRIPTSIZE
            || script.script_words.len() != SAV_MAXSCRIPTSIZE
            || script.actor_script_offsets.len() != SAV_MAXTILES
            || script.sprite_script_flags.len() != SAV_MAXSPRITES
            || script.weapon_hits.len() != SAV_MAXSPRITES * 72
        {
            return Err(BuildError::Invalid("SAV script-state dimensions"));
        }
        append_array(
            &mut out,
            &script.script_pointer_flags,
            1,
            SAV_MAXSCRIPTSIZE,
            "SAV script pointer flags",
        )?;
        let mut script_words = Vec::with_capacity(SAV_MAXSCRIPTSIZE * 4);
        for word in &script.script_words {
            script_words.extend_from_slice(&word.to_le_bytes());
        }
        append_array(
            &mut out,
            &script_words,
            4,
            SAV_MAXSCRIPTSIZE,
            "SAV script words",
        )?;
        let mut actor_script_offsets = Vec::with_capacity(SAV_MAXTILES * 4);
        for word in &script.actor_script_offsets {
            actor_script_offsets.extend_from_slice(&word.to_le_bytes());
        }
        append_array(
            &mut out,
            &actor_script_offsets,
            4,
            SAV_MAXTILES,
            "SAV actor script offsets",
        )?;
        append_array(
            &mut out,
            &script.sprite_script_flags,
            1,
            SAV_MAXSPRITES,
            "SAV actor temp flags",
        )?;
        append_array(
            &mut out,
            &script.weapon_hits,
            72,
            SAV_MAXSPRITES,
            "SAV weapon hits",
        )?;
        Ok(out)
    }

    /// Encode a complete Atomic `GAME#.SAV` from the parsed native state.
    ///
    /// This serializes the data model as loaded from an Atomic save. Runtime
    /// session integration must update that model before calling this method
    /// when saving a live game.
    pub fn encode_atomic(&self) -> Result<Vec<u8>, BuildError> {
        fn append_record(
            out: &mut Vec<u8>,
            data: &[u8],
            element_size: usize,
            count: usize,
            label: &'static str,
        ) -> Result<(), BuildError> {
            let encoded =
                kdfwrite_array_raw(data, element_size, count).ok_or(BuildError::Invalid(label))?;
            out.extend_from_slice(&encoded);
            Ok(())
        }

        fn append_i32(
            out: &mut Vec<u8>,
            value: Option<i32>,
            label: &'static str,
        ) -> Result<(), BuildError> {
            let value = value.ok_or(BuildError::Invalid(label))?;
            append_record(out, &value.to_le_bytes(), 4, 1, label)
        }

        fn append_short(
            out: &mut Vec<u8>,
            value: Option<i32>,
            label: &'static str,
        ) -> Result<(), BuildError> {
            let value = i16::try_from(value.ok_or(BuildError::Invalid(label))?)
                .map_err(|_| BuildError::Invalid(label))?;
            append_record(out, &value.to_le_bytes(), 2, 1, label)
        }

        fn append_byte(
            out: &mut Vec<u8>,
            value: Option<i32>,
            label: &'static str,
        ) -> Result<(), BuildError> {
            let value = u8::try_from(value.ok_or(BuildError::Invalid(label))?)
                .map_err(|_| BuildError::Invalid(label))?;
            append_record(out, &[value], 1, 1, label)
        }

        fn append_shorts(
            out: &mut Vec<u8>,
            values: &[i16],
            count: usize,
            label: &'static str,
        ) -> Result<(), BuildError> {
            if values.len() != count {
                return Err(BuildError::Invalid(label));
            }
            let mut bytes = Vec::with_capacity(count * 2);
            for value in values {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            append_record(out, &bytes, 2, count, label)
        }

        fn append_i32s(
            out: &mut Vec<u8>,
            values: &[i32],
            count: usize,
            label: &'static str,
        ) -> Result<(), BuildError> {
            if values.len() != count {
                return Err(BuildError::Invalid(label));
            }
            let mut bytes = Vec::with_capacity(count * 4);
            for value in values {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            append_record(out, &bytes, 4, count, label)
        }

        let mut out = self.encode_atomic_script_prefix()?;
        append_i32(&mut out, self.lockclock, "SAV lockclock")?;
        append_short(&mut out, self.pskybits, "SAV pskybits")?;
        let pskyoff = self.pskyoff.ok_or(BuildError::Invalid("SAV pskyoff"))?;
        append_shorts(&mut out, &pskyoff, 8, "SAV pskyoff")?;
        let animatecnt = self
            .animatecnt
            .filter(|&count| (0..=64).contains(&count))
            .ok_or(BuildError::Invalid("SAV animatecnt"))?;
        append_i32(&mut out, Some(animatecnt), "SAV animatecnt")?;
        append_shorts(
            &mut out,
            self.animatesect
                .as_deref()
                .ok_or(BuildError::Invalid("SAV animatesect"))?,
            64,
            "SAV animatesect",
        )?;
        append_i32s(
            &mut out,
            self.animateptr
                .as_deref()
                .ok_or(BuildError::Invalid("SAV animateptr"))?,
            64,
            "SAV animateptr",
        )?;
        append_i32s(
            &mut out,
            self.animategoal
                .as_deref()
                .ok_or(BuildError::Invalid("SAV animategoal"))?,
            64,
            "SAV animategoal",
        )?;
        append_i32s(
            &mut out,
            self.animatevel
                .as_deref()
                .ok_or(BuildError::Invalid("SAV animatevel"))?,
            64,
            "SAV animatevel",
        )?;
        append_byte(&mut out, self.earthquaketime, "SAV earthquaketime")?;
        append_short(&mut out, self.from_bonus, "SAV from_bonus")?;
        append_short(&mut out, self.secretlevel, "SAV secretlevel")?;
        append_i32(&mut out, self.respawn_monsters, "SAV respawn_monsters")?;
        append_i32(&mut out, self.respawn_items, "SAV respawn_items")?;
        append_i32(&mut out, self.respawn_inventory, "SAV respawn_inventory")?;
        append_byte(&mut out, self.god, "SAV god")?;
        append_i32(&mut out, self.auto_run, "SAV auto_run")?;
        append_i32(&mut out, self.crosshair, "SAV crosshair")?;
        append_i32(&mut out, self.monsters_off, "SAV monsters_off")?;
        append_short(&mut out, self.last_level, "SAV last_level")?;
        append_byte(&mut out, self.eog, "SAV eog")?;
        append_i32(&mut out, self.coop, "SAV coop")?;
        append_i32(&mut out, self.marker, "SAV marker")?;
        append_i32(&mut out, self.ffire, "SAV ffire")?;
        append_short(&mut out, self.camsprite, "SAV camsprite")?;
        append_short(&mut out, self.connecthead, "SAV connecthead")?;
        append_shorts(
            &mut out,
            &self
                .connectpoint2
                .ok_or(BuildError::Invalid("SAV connectpoint2"))?,
            16,
            "SAV connectpoint2",
        )?;
        let numplayersprites = self
            .numplayersprites
            .filter(|&count| (0..=16).contains(&count));
        append_byte(&mut out, numplayersprites, "SAV numplayersprites")?;
        let frags = self.frags.ok_or(BuildError::Invalid("SAV frags"))?;
        let frag_bytes: Vec<u8> = frags
            .iter()
            .flat_map(|row| row.iter())
            .flat_map(|frag| frag.to_le_bytes())
            .collect();
        append_record(&mut out, &frag_bytes, 2, 16 * 16, "SAV frags")?;
        append_i32(&mut out, self.randomseed, "SAV randomseed")?;
        append_short(&mut out, self.global_random, "SAV global_random")?;
        append_i32(&mut out, self.parallaxyscale, "SAV parallaxyscale")?;
        // Chocolate Atomic writes the Build `gotpic[MAXTILES / 8]` bitset and
        // `playerswhenstarted` immediately after `parallaxyscale`.
        let gotpic = self
            .gotpic
            .as_deref()
            .filter(|bytes| bytes.len() == SAV_MAXTILES.div_ceil(8))
            .ok_or(BuildError::Invalid("SAV gotpic"))?;
        append_record(&mut out, gotpic, gotpic.len(), 1, "SAV gotpic")?;
        append_byte(&mut out, self.playerswhenstarted, "SAV playerswhenstarted")?;
        Ok(out)
    }

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
        // `saveplayer` writes tile[MAXTILES-1].data (160×100) here, and
        // `loadplayer` consumes it into MAXTILES-3 as a fake read. Older
        // compact Room fixtures omitted that image, so only consume a valid
        // full payload and rewind cleanly when the following bytes are the
        // legacy numwalls field instead.
        let save_thumbnail = read_sav_thumbnail(bytes, &mut pos);
        let numwalls = kdfread_one(bytes, &mut pos, 2).and_then(|b| {
            let n = i16::from_le_bytes([b[0], b[1]]);
            (0..=SAV_MAXWALLS as i16)
                .contains(&n)
                .then_some(i32::from(n))
        });
        let wall_blob = if numwalls.is_some() {
            read_sav_array_compat(bytes, &mut pos, 32, 8192)
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
                (0..=SAV_MAXSECTORS as i16)
                    .contains(&n)
                    .then_some(i32::from(n))
            })
        } else {
            None
        };
        let sector_blob = if numsectors.is_some() {
            read_sav_array_compat(bytes, &mut pos, 40, 1024)
        } else {
            None
        };
        let sectors = match (numsectors, sector_blob.as_deref()) {
            (Some(n), Some(b)) => parse_sav_sectors(n, b),
            _ => None,
        };
        let (numsprites, sprite_blob) = if sector_blob.is_some() {
            let sprite_start = pos;
            // Chocolate's saveplayer writes sprite[MAXSPRITES] directly after
            // sector[MAXSECTORS], with no count field. Recognize old compact
            // Room fixtures by a standalone two-byte dfwrite record, then
            // follow the native fixed-array layout otherwise.
            let mut probe = pos;
            let leading_blob = kdfread_blob(bytes, &mut probe);
            let legacy_count = leading_blob.as_deref().and_then(|b| {
                if b.len() != 2 {
                    return None;
                }
                let n = i16::from_le_bytes([b[0], b[1]]);
                (0..=16_384).contains(&n).then_some(i32::from(n))
            });
            if let Some(count) = legacy_count {
                pos = probe;
                (
                    Some(count),
                    read_sav_array_compat(bytes, &mut pos, 44, SAV_MAXSPRITES),
                )
            } else {
                pos = sprite_start;
                (
                    Some(SAV_MAXSPRITES as i32),
                    read_sav_array_compat(bytes, &mut pos, 44, SAV_MAXSPRITES),
                )
            }
        } else {
            (None, None)
        };
        let sprites = match (numsprites, sprite_blob.as_deref()) {
            (Some(n), Some(b)) => parse_sav_sprites(n, b),
            _ => None,
        };
        // Chocolate `MENUES.C::loadplayer` reads these fixed-size arrays in
        // this order: sector heads, sector prev/next, status heads, status
        // prev/next. Keeping them preserves the allocator and thinker order.
        let sprite_list_blobs = if sprite_blob.is_some() {
            (|| {
                Some([
                    read_sav_array_compat(bytes, &mut pos, 2, 1025)?,
                    read_sav_array_compat(bytes, &mut pos, 2, 4096)?,
                    read_sav_array_compat(bytes, &mut pos, 2, 4096)?,
                    read_sav_array_compat(bytes, &mut pos, 2, 1025)?,
                    read_sav_array_compat(bytes, &mut pos, 2, 4096)?,
                    read_sav_array_compat(bytes, &mut pos, 2, 4096)?,
                ])
            })()
        } else {
            None
        };
        let sprite_lists = sprite_list_blobs.as_ref().and_then(
            |[
                headspritesect_blob,
                prevspritesect_blob,
                nextspritesect_blob,
                headspritestat_blob,
                prevspritestat_blob,
                nextspritestat_blob,
            ]| {
                Some(SavSpriteLists {
                    headspritesect: parse_short_payload(headspritesect_blob, 1025)?,
                    prevspritesect: parse_short_payload(prevspritesect_blob, 4096)?,
                    nextspritesect: parse_short_payload(nextspritesect_blob, 4096)?,
                    headspritestat: parse_short_payload(headspritestat_blob, 1025)?,
                    prevspritestat: parse_short_payload(prevspritestat_blob, 4096)?,
                    nextspritestat: parse_short_payload(nextspritestat_blob, 4096)?,
                })
            },
        );
        let numcyclers = if sprite_list_blobs.is_some() {
            kdfread_one(bytes, &mut pos, 2).and_then(|b| {
                let n = i16::from_le_bytes([b[0], b[1]]);
                (0..=256).contains(&n).then_some(i32::from(n))
            })
        } else {
            None
        };
        // Chocolate `saveplayer` writes `cyclers[0][0]` as 12 bytes for every
        // MAXCYCLERS slot, not merely `numcyclers` active slots.
        let cycler_blob = if numcyclers.is_some() {
            read_sav_array_compat(bytes, &mut pos, 12, 256)
        } else {
            None
        };
        let cyclers = cycler_blob.as_deref().and_then(|blob| {
            (blob.len() >= 256 * 12).then(|| SavCyclers {
                records: blob[..256 * 12].to_vec(),
            })
        });
        // Then ps[] / po[], followed by numanimwalls.
        let cyclers_ok = cycler_blob.is_some();
        let ps_blob = if cyclers_ok {
            kdfread_blob(bytes, &mut pos)
        } else {
            None
        };
        let player = ps_blob.as_deref().and_then(parse_sav_player);
        let player_orig = if ps_blob.is_some() {
            kdfread_blob(bytes, &mut pos)
        } else {
            None
        };
        let numanimwalls = if player_orig.is_some() {
            kdfread_one(bytes, &mut pos, 2).and_then(|b| {
                let n = i16::from_le_bytes([b[0], b[1]]);
                (0..=512).contains(&n).then_some(i32::from(n))
            })
        } else {
            None
        };
        // Chocolate stores the full fixed animation arrays, then the active
        // deletion queue. Keep each table independently so truncated saves
        // still expose the valid prefix without pretending to restore it.
        let animwalls_blob = if numanimwalls.is_some() {
            kdfread_blob(bytes, &mut pos)
        } else {
            None
        };
        let animwalls = animwalls_blob
            .as_deref()
            .and_then(|b| (b.len() >= 512 * 6).then(|| b[..512 * 6].to_vec()));
        let msx_blob = if animwalls_blob.is_some() {
            read_sav_array_compat(bytes, &mut pos, 4, 2048)
        } else {
            None
        };
        let msx = msx_blob.as_deref().and_then(|b| parse_i32_payload(b, 2048));
        let msy_blob = if msx_blob.is_some() {
            read_sav_array_compat(bytes, &mut pos, 4, 2048)
        } else {
            None
        };
        let msy = msy_blob.as_deref().and_then(|b| parse_i32_payload(b, 2048));
        let spriteqloc = if msy_blob.is_some() {
            kdfread_one(bytes, &mut pos, 2)
                .and_then(|b| Some(i32::from(i16::from_le_bytes([b[0], b[1]]))))
        } else {
            None
        };
        let spriteqamount = if spriteqloc.is_some() {
            kdfread_one(bytes, &mut pos, 2).and_then(|b| {
                let n = i16::from_le_bytes([b[0], b[1]]);
                (0..=1024).contains(&n).then_some(i32::from(n))
            })
        } else {
            None
        };
        let spriteq_blob = match spriteqamount {
            Some(0) => Some(Vec::new()),
            Some(n) => read_sav_array_compat(bytes, &mut pos, 2, n as usize),
            None => None,
        };
        let spriteq = match (spriteqamount, spriteq_blob.as_deref()) {
            (Some(n), Some(b)) => parse_short_payload(b, n as usize),
            _ => None,
        };
        let mirror_count = if spriteq_blob.is_some() {
            kdfread_one(bytes, &mut pos, 2).and_then(|b| {
                let n = i16::from_le_bytes([b[0], b[1]]);
                (0..=64).contains(&n).then_some(i32::from(n))
            })
        } else {
            None
        };
        let mirror_walls_blob = if mirror_count.is_some() {
            read_sav_array_compat(bytes, &mut pos, 2, 64)
        } else {
            None
        };
        let mirror_walls = mirror_walls_blob
            .as_deref()
            .and_then(|b| parse_short_payload(b, 64));
        let mirror_sectors_blob = if mirror_walls_blob.is_some() {
            read_sav_array_compat(bytes, &mut pos, 2, 64)
        } else {
            None
        };
        let mirror_sectors = mirror_sectors_blob
            .as_deref()
            .and_then(|b| parse_short_payload(b, 64));
        let show2dsector = if mirror_sectors_blob.is_some() {
            read_sav_array_compat(bytes, &mut pos, 1, 128)
        } else {
            None
        };
        // The fixed `actortype[MAXTILES]` table follows the automap bitset.
        let actor_types_blob = if show2dsector.is_some() {
            read_sav_array_compat(bytes, &mut pos, 1, SAV_MAXTILES)
        } else {
            None
        };
        let boardfilename_blob = if actor_types_blob.is_some() {
            kdfread_blob(bytes, &mut pos)
        } else {
            None
        };
        let boardfilename = boardfilename_blob
            .as_deref()
            .and_then(|b| (b.len() >= 128).then(|| b[..128].to_vec()));
        let numclouds = if boardfilename_blob.is_some() {
            kdfread_one(bytes, &mut pos, 2).and_then(|b| {
                let n = i16::from_le_bytes([b[0], b[1]]);
                (0..=128).contains(&n).then_some(i32::from(n))
            })
        } else {
            None
        };
        // The three cloud tables are fixed-size regardless of active count.
        let clouds_blob = if numclouds.is_some() {
            kdfread_blob(bytes, &mut pos)
        } else {
            None
        };
        let clouds = clouds_blob
            .as_deref()
            .and_then(|b| parse_short_payload(b, 128));
        let cloudx_blob = if clouds_blob.is_some() {
            kdfread_blob(bytes, &mut pos)
        } else {
            None
        };
        let cloudx = cloudx_blob
            .as_deref()
            .and_then(|b| parse_short_payload(b, 128));
        let cloudy_blob = if cloudx_blob.is_some() {
            kdfread_blob(bytes, &mut pos)
        } else {
            None
        };
        let cloudy = cloudy_blob
            .as_deref()
            .and_then(|b| parse_short_payload(b, 128));
        // Atomic loadplayer follows these with script pointers/code, actor
        // script pointers, per-sprite script flags, and all weaponhit temps.
        let script_pointer_flags = if cloudy_blob.is_some() {
            read_sav_array_compat(bytes, &mut pos, 1, SAV_MAXSCRIPTSIZE)
        } else {
            None
        };
        let script_words = if script_pointer_flags.is_some() {
            read_sav_array_compat(bytes, &mut pos, 4, SAV_MAXSCRIPTSIZE)
        } else {
            None
        };
        let actor_script_offsets = if script_words.is_some() {
            read_sav_array_compat(bytes, &mut pos, 4, SAV_MAXTILES)
        } else {
            None
        };
        let sprite_script_flags = if actor_script_offsets.is_some() {
            read_sav_array_compat(bytes, &mut pos, 1, SAV_MAXSPRITES)
        } else {
            None
        };
        let weapon_hits = if sprite_script_flags.is_some() {
            read_sav_array_compat(bytes, &mut pos, 72, SAV_MAXSPRITES)
        } else {
            None
        };
        let has_weapon_hits = weapon_hits.is_some();
        let script_state = match (
            script_pointer_flags,
            script_words,
            actor_script_offsets,
            sprite_script_flags,
            weapon_hits,
        ) {
            (Some(flags), Some(words), Some(actor_offsets), Some(sprite_flags), Some(hits))
                if flags.len() >= SAV_MAXSCRIPTSIZE
                    && words.len() >= SAV_MAXSCRIPTSIZE * 4
                    && actor_offsets.len() >= SAV_MAXTILES * 4
                    && sprite_flags.len() >= SAV_MAXSPRITES
                    && hits.len() >= SAV_MAXSPRITES * 72 =>
            {
                parse_i32_payload(&words, SAV_MAXSCRIPTSIZE).and_then(|script_words| {
                    parse_i32_payload(&actor_offsets, SAV_MAXTILES).map(|actor_script_offsets| {
                        SavScriptState {
                            script_pointer_flags: flags[..SAV_MAXSCRIPTSIZE].to_vec(),
                            script_words,
                            actor_script_offsets,
                            sprite_script_flags: sprite_flags[..SAV_MAXSPRITES].to_vec(),
                            weapon_hits: hits[..SAV_MAXSPRITES * 72].to_vec(),
                        }
                    })
                })
            }
            _ => None,
        };
        let lockclock = if has_weapon_hits {
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
            read_sav_array_compat(bytes, &mut pos, 2, 8).and_then(|b| {
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
            kdfread_one(bytes, &mut pos, 4).and_then(|b| {
                let n = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                (0..=64).contains(&n).then_some(n)
            })
        } else {
            None
        };
        let animatesect = if animatecnt.is_some() {
            read_sav_array_compat(bytes, &mut pos, 2, 64).and_then(|b| parse_short_payload(&b, 64))
        } else {
            None
        };
        let animateptr = if animatesect.is_some() {
            read_sav_array_compat(bytes, &mut pos, 4, 64).and_then(|b| parse_i32_payload(&b, 64))
        } else {
            None
        };
        let animategoal = if animateptr.is_some() {
            read_sav_array_compat(bytes, &mut pos, 4, 64).and_then(|b| parse_i32_payload(&b, 64))
        } else {
            None
        };
        let animatevel = if animategoal.is_some() {
            read_sav_array_compat(bytes, &mut pos, 4, 64).and_then(|b| parse_i32_payload(&b, 64))
        } else {
            None
        };
        let earthquaketime = if animatevel.is_some() {
            kdfread_one(bytes, &mut pos, 1).and_then(|b| b.first().copied().map(i32::from))
        } else {
            None
        };
        let from_bonus = if earthquaketime.is_some() {
            read_sav_scalar(bytes, &mut pos, 2)
        } else {
            None
        };
        let secretlevel = if from_bonus.is_some() {
            read_sav_scalar(bytes, &mut pos, 2)
        } else {
            None
        };
        let respawn_monsters = if secretlevel.is_some() {
            read_sav_scalar(bytes, &mut pos, 4)
        } else {
            None
        };
        let respawn_items = if respawn_monsters.is_some() {
            read_sav_scalar(bytes, &mut pos, 4)
        } else {
            None
        };
        let respawn_inventory = if respawn_items.is_some() {
            read_sav_scalar(bytes, &mut pos, 4)
        } else {
            None
        };
        let god = if respawn_inventory.is_some() {
            read_sav_scalar(bytes, &mut pos, 1)
        } else {
            None
        };
        let auto_run = if god.is_some() {
            read_sav_scalar(bytes, &mut pos, 4)
        } else {
            None
        };
        let crosshair = if auto_run.is_some() {
            read_sav_scalar(bytes, &mut pos, 4)
        } else {
            None
        };
        let monsters_off = if crosshair.is_some() {
            read_sav_scalar(bytes, &mut pos, 4)
        } else {
            None
        };
        let last_level = if monsters_off.is_some() {
            read_sav_scalar(bytes, &mut pos, 2)
        } else {
            None
        };
        let eog = if last_level.is_some() {
            read_sav_scalar(bytes, &mut pos, 1)
        } else {
            None
        };
        let coop = if eog.is_some() {
            read_sav_scalar(bytes, &mut pos, 4)
        } else {
            None
        };
        let marker = if coop.is_some() {
            read_sav_scalar(bytes, &mut pos, 4)
        } else {
            None
        };
        let ffire = if marker.is_some() {
            read_sav_scalar(bytes, &mut pos, 4)
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
            read_sav_array_compat(bytes, &mut pos, 2, 16).and_then(|b| {
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
            read_sav_scalar(bytes, &mut pos, 1).filter(|n| (0..=16).contains(n))
        } else {
            None
        };
        let frags = if numplayersprites.is_some() {
            read_sav_array_compat(bytes, &mut pos, 2, 256).and_then(|b| {
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
        let randomseed = if frags.is_some() {
            kdfread_one(bytes, &mut pos, 4).and_then(|b| {
                let a: [u8; 4] = b.try_into().ok()?;
                Some(i32::from_le_bytes(a))
            })
        } else {
            None
        };
        let global_random = if randomseed.is_some() {
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
            save_thumbnail,
            numwalls,
            walls,
            numsectors,
            sectors,
            numsprites,
            sprites,
            sprite_lists,
            numcyclers,
            cyclers,
            player,
            player_orig,
            numanimwalls,
            animwalls,
            msx,
            msy,
            spriteqloc,
            spriteqamount,
            spriteq,
            mirror_count,
            mirror_walls,
            mirror_sectors,
            show2dsector,
            actor_types: actor_types_blob
                .as_deref()
                .and_then(|b| (b.len() >= SAV_MAXTILES).then(|| b[..SAV_MAXTILES].to_vec())),
            boardfilename,
            numclouds,
            clouds,
            cloudx,
            cloudy,
            script_state,
            randomseed,
            lockclock,
            pskybits,
            pskyoff,
            animatecnt,
            animatesect,
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
    /// Duke `game#.sav` parsed native state and `saveplayer` arrays.
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

fn parse_short_payload(blob: &[u8], count: usize) -> Option<Vec<i16>> {
    let byte_count = count.checked_mul(2)?;
    if blob.len() < byte_count {
        return None;
    }
    Some(
        blob[..byte_count]
            .chunks_exact(2)
            .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
            .collect(),
    )
}

fn parse_i32_payload(blob: &[u8], count: usize) -> Option<Vec<i32>> {
    let required = count.checked_mul(4)?;
    if blob.len() < required {
        return None;
    }
    Some(
        blob[..required]
            .chunks_exact(4)
            .map(|chunk| i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect(),
    )
}

/// Read a complete Chocolate `kdfread` array. Tiny compact fixtures predating
/// the full fixed Build arrays are still accepted as a single-blob prefix.
fn read_sav_array_compat(
    src: &[u8],
    pos: &mut usize,
    element_size: usize,
    count: usize,
) -> Option<Vec<u8>> {
    let start = *pos;
    if let Some(array) = kdfread_array(src, pos, element_size, count) {
        return Some(array);
    }
    *pos = start;
    kdfread_blob(src, pos)
}

/// Optional Chocolate save-screen image: it is exactly one sub-`LZWSIZE`
/// array block, unlike the fixed Build tables which can span several blocks.
/// Requiring that exact decompressed size avoids mistaking a compact fixture's
/// following world records for an omitted thumbnail.
fn read_sav_thumbnail(src: &[u8], pos: &mut usize) -> Option<Vec<u8>> {
    let start = *pos;
    let result = (|| {
        let header_end = start.checked_add(2)?;
        let len = usize::from(u16::from_le_bytes(
            src.get(start..header_end)?.try_into().ok()?,
        ));
        if len < 4 {
            return None;
        }
        let end = header_end.checked_add(len)?;
        let pixels = crate::dmo::ken_uncompress(src.get(header_end..end)?)?;
        (pixels.len() == 160 * 100).then_some((end, pixels))
    })();
    if let Some((end, pixels)) = result {
        *pos = end;
        Some(pixels)
    } else {
        *pos = start;
        None
    }
}

fn read_sav_scalar(src: &[u8], pos: &mut usize, width: usize) -> Option<i32> {
    let bytes = kdfread_one(src, pos, width)?;
    match width {
        1 => Some(i32::from(*bytes.first()?)),
        2 => Some(i32::from(i16::from_le_bytes([
            *bytes.first()?,
            *bytes.get(1)?,
        ]))),
        4 => Some(i32::from_le_bytes([
            *bytes.first()?,
            *bytes.get(1)?,
            *bytes.get(2)?,
            *bytes.get(3)?,
        ])),
        _ => None,
    }
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
            ceilingxpanning: blob[o + 22],
            ceilingypanning: blob[o + 23],
            floorpicnum: i16::from_le_bytes([blob[o + 24], blob[o + 25]]),
            floorheinum: i16::from_le_bytes([blob[o + 26], blob[o + 27]]),
            floorshade: blob[o + 28] as i8,
            floorpal: blob[o + 29],
            floorxpanning: blob[o + 30],
            floorypanning: blob[o + 31],
            visibility: blob[o + 32],
            filler: blob[o + 33],
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
            filler: blob[o + 19],
            xrepeat: blob[o + 20],
            yrepeat: blob[o + 21],
            xoffset: blob[o + 22] as i8,
            yoffset: blob[o + 23] as i8,
            sectnum: i16::from_le_bytes([blob[o + 24], blob[o + 25]]),
            statnum: i16::from_le_bytes([blob[o + 26], blob[o + 27]]),
            ang: i16::from_le_bytes([blob[o + 28], blob[o + 29]]),
            owner: i16::from_le_bytes([blob[o + 30], blob[o + 31]]),
            xvel: i16::from_le_bytes([blob[o + 32], blob[o + 33]]),
            yvel: i16::from_le_bytes([blob[o + 34], blob[o + 35]]),
            zvel: i16::from_le_bytes([blob[o + 36], blob[o + 37]]),
            lotag: i16::from_le_bytes([blob[o + 38], blob[o + 39]]),
            hitag: i16::from_le_bytes([blob[o + 40], blob[o + 41]]),
            extra: i16::from_le_bytes([blob[o + 42], blob[o + 43]]),
        });
    }
    Some(out)
}

fn parse_sav_player(blob: &[u8]) -> Option<SavPlayer> {
    // Chocolate Atomic saves write the raw 32-bit `player_struct` array.
    // Its initial zoom/loogie fields place `posx` at byte 532; parsing from
    // byte zero as if the structure began with the pose silently shifted every
    // field and produced a plausible but unusable player state.
    const POSX: usize = 532;
    const ANG: usize = 644;
    const AMMO: usize = 658;
    const U8: usize = 836;
    let tail = atomic_player_tail_offsets(blob.len())?;
    let i16_at = |o: usize| -> Option<i16> {
        Some(i16::from_le_bytes(blob.get(o..o + 2)?.try_into().ok()?))
    };
    let i32_at = |o: usize| -> Option<i32> {
        Some(i32::from_le_bytes(blob.get(o..o + 4)?.try_into().ok()?))
    };
    let byte = |o: usize| -> Option<u8> { blob.get(o).copied() };
    let (posx, posy, posz, horiz) = (
        i32_at(POSX)?,
        i32_at(POSX + 4)?,
        i32_at(POSX + 8)?,
        i32_at(POSX + 12)?,
    );
    let ammo_amount = blob
        .get(AMMO..AMMO + 24)
        .map(|raw| std::array::from_fn(|i| i16::from_le_bytes([raw[i * 2], raw[i * 2 + 1]])));
    let weapon_reservations = blob
        .get(ANG + 118..ANG + 150)
        .map(|raw| std::array::from_fn(|i| i16::from_le_bytes([raw[i * 2], raw[i * 2 + 1]])));
    let gotweapon = blob.get(U8 + 49..U8 + 61).map(|raw| {
        let mut got = [0u8; 12];
        got.copy_from_slice(raw);
        got
    });
    let pals = blob
        .get(tail.pals..tail.pals + 3)
        .map(|raw| [raw[0], raw[1], raw[2]]);
    let pals_time = i32_at(POSX + 96).map(|n| n.clamp(0, i32::from(i16::MAX)) as i16);
    let loogie_x = blob.get(12..12 + 6 * 4).map(|raw| {
        std::array::from_fn(|i| {
            i32::from_le_bytes(raw[i * 4..i * 4 + 4].try_into().unwrap_or([0; 4]))
        })
    });
    let loogie_y = blob.get(268..268 + 6 * 4).map(|raw| {
        std::array::from_fn(|i| {
            i32::from_le_bytes(raw[i * 4..i * 4 + 4].try_into().unwrap_or([0; 4]))
        })
    });
    Some(SavPlayer {
        raw_record: blob.to_vec(),
        posx,
        posy,
        posz,
        num_loogs: i32_at(524),
        loogie_tics: i32_at(528),
        loogie_x,
        loogie_y,
        horiz,
        posxv: i32_at(POSX + 56),
        posyv: i32_at(POSX + 60),
        poszv: i32_at(POSX + 64),
        true_floor_z: i32_at(POSX + 72),
        true_ceiling_z: i32_at(POSX + 76),
        bob_counter: i32_at(POSX + 88),
        cursectnum: i16_at(ANG + 6),
        sprite_i: i16_at(ANG + 90),
        oposx: i32_at(POSX + 36),
        oposy: i32_at(POSX + 40),
        oposz: i32_at(POSX + 44),
        bobposx: i32_at(POSX + 28),
        bobposy: i32_at(POSX + 32),
        pyoff: i32_at(POSX + 48),
        ohoriz: i32_at(POSX + 16),
        ohorizoff: i32_at(POSX + 20),
        invdisptime: i32_at(POSX + 24),
        ang: i16_at(ANG)?,
        frag: i16_at(ANG + 40),
        fraggedself: i16_at(ANG + 42),
        aim_mode: i32_at(POSX + 108),
        over_shoulder_on: i16_at(ANG + 92).map(|value| value != 0),
        last_extra: i16_at(ANG + 10),
        extra_extra8: i16_at(ANG + 106),
        horizoff: i16_at(ANG + 50),
        look_ang: i16_at(ANG + 8),
        rotscrnang: i16_at(ANG + 156),
        ammo_amount,
        curr_weapon: i16_at(ANG + 44),
        last_weapon: i16_at(ANG + 46),
        wantweaponfire: i16_at(ANG + 52),
        return_to_center: byte(tail.return_to_center).map(i16::from),
        tipincs: i16_at(ANG + 48),
        gotweapon,
        shield_amount: i16_at(ANG + 168),
        firstaid_amount: i16_at(ANG + 84),
        steroids_amount: i16_at(ANG + 166),
        jetpack_amount: i16_at(ANG + 164),
        scuba_amount: i16_at(ANG + 162),
        holoduke_amount: i16_at(ANG + 54),
        newowner: i16_at(ANG + 56),
        hurt_delay: i16_at(ANG + 58),
        hbomb_hold_delay: i16_at(ANG + 60),
        boot_amount: i16_at(ANG + 184),
        heat_amount: i16_at(ANG + 110),
        kickback_pic: i16_at(ANG + 78),
        fist_incs: i16_at(ANG + 98),
        button_palette: byte(U8 + 40),
        weapon_ang: i16_at(ANG + 82),
        got_access: i16_at(ANG + 80),
        holoduke_on: i16_at(ANG + 170),
        jetpack_on: byte(U8 + 41),
        scuba_on: byte(U8 + 44),
        heat_on: byte(U8 + 46),
        falling_counter: byte(U8 + 48),
        holster_weapon: byte(U8 + 47),
        airleft: i16_at(ANG + 64),
        jumping_counter: i16_at(ANG + 62),
        dead_flag: i16_at(ANG + 158),
        inven_icon: byte(U8 + 39),
        toggle_key_flag: byte(tail.toggle_key_flag),
        refresh_inventory: byte(U8 + 61),
        on_ground: byte(U8 + 6),
        show_empty_weapon: i16_at(ANG + 160).map(|n| u8::from(n != 0)),
        rapid_fire_hold: byte(U8 + 5),
        pals,
        pals_time,
        transporter_hold: i16_at(ANG + 178),
        hbomb_on: byte(U8 + 3),
        jumping_toggle: byte(U8 + 4),
        weapon_pos: i16_at(ANG + 174),
        last_full_weapon: i16_at(ANG + 180),
        footprintshade: i16_at(ANG + 182),
        footprintcount: byte(U8 + 2),
        footprintpal: byte(U8 + 45),
        quick_kick: i16_at(ANG + 108),
        frag_ps: i16_at(ANG + 176),
        knee_incs: i16_at(ANG + 66),
        access_incs: i16_at(ANG + 68),
        fta: i16_at(ANG + 70),
        ftq: i16_at(ANG + 72),
        access_wallnum: i16_at(ANG + 74),
        access_spritenum: i16_at(ANG + 76),
        hard_landing: byte(tail.hard_landing),
        knuckle_incs: byte(tail.knuckle_incs),
        palookup: byte(tail.palookup),
        walking_snd_toggle: byte(tail.walking_snd_toggle),
        pycount: i16_at(ANG + 172),
        weapon_reservations,
        weapon_reservation_count: i16_at(ANG + 150),
        time_before_exit: i16_at(ANG + 114),
        secret_rooms: byte(tail.secret_rooms),
        max_secret_rooms: byte(tail.max_secret_rooms),
        actors_killed: byte(tail.actors_killed),
        max_actors_killed: byte(tail.max_actors_killed),
        auto_aim: i32_at(tail.auto_aim),
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
        for count in [1025, 4096, 4096, 1025, 4096, 4096] {
            sav.extend(kdfwrite_raw(&vec![0u8; count * 2]));
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
        sav.extend(kdfwrite_raw(&[0u8; 256 * 12])); // fixed cycler table
        let h = SavHeader::parse(&sav).unwrap();
        assert_eq!(h.numsprites, Some(12));
        assert_eq!(h.sprite_lists.as_ref().unwrap().headspritesect.len(), 1025);
        assert_eq!(h.sprite_lists.as_ref().unwrap().nextspritesect.len(), 4096);
        assert_eq!(h.numcyclers, Some(2));
        assert_eq!(h.cyclers.as_ref().unwrap().records.len(), 256 * 12);
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
        let compact = ps;
        let mut ps = vec![0u8; 928];
        for (src, dst, len) in [
            (0, 532, 16),
            (16, 548, 12),
            (28, 644, 2),
            (30, 750, 2),
            (32, 694, 2),
            (34, 652, 2),
            (36, 800, 2),
            (38, 654, 2),
            (42, 658, 24),
            (72, 688, 8),
            (82, 698, 32),
            (136, 752, 4),
            (184, 802, 2),
            (188, 806, 24),
            (219, 839, 1),
            (220, 840, 1),
            (222, 842, 1),
            (224, 841, 1),
            (225, 909, 3),
            (255, 875, 1),
            (256, 902, 1),
            (257, 877, 1),
            (258, 897, 1),
            (259, 838, 1),
            (260, 880, 1),
            (261, 881, 1),
            (262, 882, 1),
            (263, 883, 1),
            (264, 884, 1),
            (265, 885, 12),
            (283, 903, 3),
            (286, 906, 1),
        ] {
            ps[dst..dst + len].copy_from_slice(&compact[src..src + len]);
        }
        ps[804..806].copy_from_slice(&u16::from(compact[223]).to_le_bytes());
        ps[628..632].copy_from_slice(
            &i32::from(i16::from_le_bytes([compact[228], compact[229]])).to_le_bytes(),
        );
        ps[914] = compact[80];
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
                raw_record: Vec::new(),
                posx: 0,
                posy: 0,
                posz: 0,
                num_loogs: None,
                loogie_tics: None,
                loogie_x: None,
                loogie_y: None,
                horiz: 140,
                posxv: None,
                posyv: None,
                poszv: None,
                true_floor_z: None,
                true_ceiling_z: None,
                bob_counter: None,
                cursectnum: None,
                sprite_i: None,
                over_shoulder_on: None,
                oposx: None,
                oposy: None,
                oposz: None,
                bobposx: None,
                bobposy: None,
                pyoff: None,
                ohoriz: None,
                ohorizoff: None,
                ang: 0,
                frag: None,
                fraggedself: None,
                aim_mode: None,
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
                fist_incs: None,
                button_palette: None,
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
                weapon_reservations: None,
                weapon_reservation_count: None,
                time_before_exit: None,
                secret_rooms: None,
                max_secret_rooms: None,
                actors_killed: None,
                max_actors_killed: None,
                auto_aim: None,
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
