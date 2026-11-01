//! Duke Nukem 3D gameplay content for the converted WAD: DECORATE classes for
//! map sprites (enemies, items, keys, decorations, ambient sounds), the player
//! and the weapons, `KEYCONF` slots, `SNDINFO` for the VOC sounds and the
//! sprite frames they use.
//!
//! Numbers come from the CON scripts ([`ConData`]): tile names, `action`
//! frame tables, strengths, pickup amounts, sounds and quotes. Behaviour is
//! Room's Doom action set (`A_Chase`, `A_CustomMissile`, …) rather than the CON
//! code, which is not executed — see `docs/23-build-engine.md` §2.

use crate::art::TileSet;
use crate::condata::{Action, ConData};
use crate::convert::{MapThing, MoverSound, SpriteMapper, WallBreak};
use crate::grp::Grp;
use crate::map::Sprite;
use crate::sprites::SpriteSet;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// Editor number of the class for Duke tile `t` is `CLASS_BASE + t`.
pub const CLASS_BASE: i32 = 20000;
/// Decorations and ambient sounds get numbers from here up.
pub const DECOR_FIRST: i32 = 27000;
const DECOR_LAST: i32 = 32767;

/// Doom lock numbers for the converted keycards.
pub const LOCK_RED: i32 = 1;
/// Blue card lock.
pub const LOCK_BLUE: i32 = 2;
/// Yellow card lock.
pub const LOCK_YELLOW: i32 = 3;

/// Doom lock for a keycard / access switch palette (`0` blue, 21 red, 23 yellow).
#[must_use]
pub fn lock_for_pal(pal: u8) -> i32 {
    match pal {
        21 => LOCK_RED,
        23 => LOCK_YELLOW,
        _ => LOCK_BLUE,
    }
}

#[derive(Clone, Copy)]
enum Attack {
    None,
    Missile(&'static str),
    Bullets(i32, i32),
    Charge,
}

struct Enemy {
    tile: &'static str,
    class: &'static str,
    variants: &'static [&'static str],
    strength: &'static str,
    /// stand, walk, shoot, pain, dying, dead actions (`""` = none).
    actions: [&'static str; 6],
    attack: Attack,
    melee: i32,
    sfx: &'static str,
    attack_sfx: &'static str,
    size: (i32, i32),
    float: bool,
}

const ENEMIES: &[Enemy] = &[
    Enemy {
        tile: "LIZTROOP",
        class: "DukeTrooper",
        variants: &[
            "LIZTROOPRUNNING",
            "LIZTROOPSTAYPUT",
            "LIZTROOPSHOOT",
            "LIZTROOPJETPACK",
            "LIZTROOPONTOILET",
            "LIZTROOPJUSTSIT",
            "LIZTROOPDUCKING",
        ],
        strength: "TROOPSTRENGTH",
        actions: [
            "ATROOPSTAND",
            "ATROOPWALKING",
            "ATROOPSHOOT",
            "ATROOPFLINTCH",
            "ATROOPDYING",
            "ATROOPDEAD",
        ],
        attack: Attack::Missile("DukeLaser"),
        melee: 0,
        sfx: "PRED",
        attack_sfx: "ATTACK",
        size: (16, 48),
        float: false,
    },
    Enemy {
        tile: "PIGCOP",
        class: "DukePigCop",
        variants: &["PIGCOPSTAYPUT", "PIGCOPDIVE"],
        strength: "PIGCOPSTRENGTH",
        actions: [
            "APIGSTAND",
            "APIGWALK",
            "APIGSHOOT",
            "APIGHIT",
            "APIGDYING",
            "APIGDEAD",
        ],
        attack: Attack::Bullets(5, 3),
        melee: 0,
        sfx: "PIG",
        attack_sfx: "ATTACK",
        size: (18, 52),
        float: false,
    },
    Enemy {
        tile: "LIZMAN",
        class: "DukeEnforcer",
        variants: &[
            "LIZMANSTAYPUT",
            "LIZMANSPITTING",
            "LIZMANFEEDING",
            "LIZMANJUMP",
        ],
        strength: "LIZSTRENGTH",
        actions: [
            "ALIZSTAND",
            "ALIZWALKING",
            "ALIZSHOOTING",
            "ALIZSCREAM",
            "ALIZDYING",
            "ALIZLYINGDEAD",
        ],
        attack: Attack::Bullets(2, 4),
        melee: 0,
        sfx: "CAPT",
        attack_sfx: "ATTACK",
        size: (18, 56),
        float: false,
    },
    Enemy {
        tile: "OCTABRAIN",
        class: "DukeOctabrain",
        variants: &["OCTABRAINSTAYPUT"],
        strength: "OCTASTRENGTH",
        actions: [
            "AOCTASTAND",
            "AOCTAWALK",
            "AOCTASHOOT",
            "AOCTAHIT",
            "AOCTADYING",
            "AOCTADEAD",
        ],
        attack: Attack::Missile("DukeOctaBall"),
        melee: 15,
        sfx: "OCTA",
        attack_sfx: "ATTACK1",
        size: (20, 40),
        float: true,
    },
    Enemy {
        tile: "COMMANDER",
        class: "DukeCommander",
        variants: &["COMMANDERSTAYPUT"],
        strength: "COMMANDERSTRENGTH",
        actions: [
            "ACOMMBREETH",
            "ACOMMGET",
            "ACOMMSHOOT",
            "",
            "ACOMMDYING",
            "ACOMMDEAD",
        ],
        attack: Attack::Missile("DukeRocket"),
        melee: 0,
        sfx: "COMM",
        attack_sfx: "ATTACK",
        size: (24, 56),
        float: true,
    },
    Enemy {
        tile: "DRONE",
        class: "DukeDrone",
        variants: &[],
        strength: "DRONESTRENGTH",
        actions: ["DRONEFRAMES", "DRONEFRAMES", "DRONEFRAMES", "", "", ""],
        attack: Attack::Charge,
        melee: 0,
        sfx: "DRON",
        attack_sfx: "ATTACK1",
        size: (16, 24),
        float: true,
    },
    Enemy {
        tile: "RECON",
        class: "DukeRecon",
        variants: &[],
        strength: "RECONSTRENGTH",
        actions: ["", "", "", "", "", ""],
        attack: Attack::Missile("DukeLaser"),
        melee: 0,
        sfx: "RECO",
        attack_sfx: "ATTACK",
        size: (20, 28),
        float: true,
    },
    Enemy {
        tile: "ROTATEGUN",
        class: "DukeTurret",
        variants: &[],
        strength: "ROTTURRETSTRENGTH",
        actions: ["", "", "", "", "", ""],
        attack: Attack::Bullets(1, 3),
        melee: 0,
        sfx: "TURR",
        attack_sfx: "ATTACK",
        size: (12, 24),
        float: true,
    },
    Enemy {
        tile: "GREENSLIME",
        class: "DukeSlimer",
        variants: &[],
        strength: "GREENSLIMESTRENGTH",
        actions: ["", "", "", "", "", ""],
        attack: Attack::None,
        melee: 3,
        sfx: "SLIM",
        attack_sfx: "ATTACK",
        size: (8, 12),
        float: false,
    },
    Enemy {
        tile: "SHARK",
        class: "DukeShark",
        variants: &[],
        strength: "SHARKSTRENGTH",
        actions: ["ASHARKCRUZING", "ASHARKCRUZING", "ASHARKATACK", "", "", ""],
        attack: Attack::None,
        melee: 8,
        sfx: "",
        attack_sfx: "",
        size: (16, 24),
        float: true,
    },
    Enemy {
        tile: "TANK",
        class: "DukeTank",
        variants: &[],
        strength: "TANKSTRENGTH",
        actions: [
            "ATANKWAIT",
            "ATANKSPIN",
            "ATANKSHOOTING",
            "",
            "",
            "ATANKDEAD",
        ],
        attack: Attack::Bullets(2, 4),
        melee: 0,
        sfx: "TANK",
        attack_sfx: "",
        size: (40, 40),
        float: false,
    },
    Enemy {
        tile: "BOSS1",
        class: "DukeBattlelord",
        variants: &["BOSS1STAYPUT"],
        strength: "BOSS1STRENGTH",
        actions: [
            "ABOSS1WALK",
            "ABOSS1WALK",
            "ABOSS1SHOOT",
            "",
            "ABOSS1DYING",
            "ABOSS1DEAD",
        ],
        attack: Attack::Bullets(3, 4),
        melee: 0,
        sfx: "BOS1",
        attack_sfx: "ATTACK1",
        size: (40, 110),
        float: false,
    },
    Enemy {
        tile: "BOSS2",
        class: "DukeOverlord",
        variants: &[],
        strength: "BOSS2STRENGTH",
        actions: [
            "ABOSS2WALK",
            "ABOSS2WALK",
            "ABOSS2SHOOT",
            "",
            "ABOSS2DYING",
            "ABOSS2DEAD",
        ],
        attack: Attack::Missile("DukeRocket"),
        melee: 0,
        sfx: "BOS2",
        attack_sfx: "ATTACK",
        size: (40, 110),
        float: false,
    },
    Enemy {
        tile: "BOSS3",
        class: "DukeCycloid",
        variants: &[],
        strength: "BOSS3STRENGTH",
        actions: [
            "ABOSS3WALK",
            "ABOSS3WALK",
            "ABOSS3LOBBING",
            "",
            "ABOSS3DYING",
            "ABOSS3DEAD",
        ],
        attack: Attack::Missile("DukeRocket"),
        melee: 0,
        sfx: "BOS3",
        attack_sfx: "ATTACK1",
        size: (40, 110),
        float: false,
    },
    Enemy {
        tile: "BOSS4",
        class: "DukeAlienQueen",
        variants: &[],
        strength: "BOSS4STRENGTH",
        actions: [
            "ABOSS4WALK",
            "ABOSS4WALK",
            "ABOSS4SHOOT",
            "",
            "ABOSS4DYING",
            "ABOSS4DEAD",
        ],
        attack: Attack::Missile("DukeSpit"),
        melee: 0,
        sfx: "BOS4",
        attack_sfx: "ATTACK",
        size: (40, 90),
        float: false,
    },
    Enemy {
        tile: "NEWBEAST",
        class: "DukeProtector",
        variants: &["NEWBEASTSTAYPUT", "NEWBEASTJUMP", "NEWBEASTHANG"],
        strength: "NEWBEASTSTRENGTH",
        actions: [
            "ANEWBEASTSTAND",
            "ANEWBEASTWALKING",
            "ANEWBEASTSCRATCHING",
            "ANEWBEASTFLINTCH",
            "ANEWBEASTDYING",
            "ANEWBEASTLYINGDEAD",
        ],
        attack: Attack::Missile("DukeSpit"),
        melee: 10,
        sfx: "NEWBEAST",
        attack_sfx: "ATTACK",
        size: (24, 64),
        float: false,
    },
];

/// Map pickups: tile, class, Doom parent (pickup behaviour), amount, max.
const ITEMS: &[(&str, &str, &str, i32, i32)] = &[
    ("COLA", "DukeCola", "Stimpack", 10, 100),
    ("SIXPAK", "DukeSixpak", "Stimpack", 30, 100),
    ("FIRSTAID", "DukeFirstAid", "Actor", 100, 100),
    ("ATOMICHEALTH", "DukeAtomicHealth", "Soulsphere", 50, 200),
    ("SHIELD", "DukeArmor", "GreenArmor", 100, 100),
    ("STEROIDS", "DukeSteroids", "Actor", 1, 1),
    ("AIRTANK", "DukeScubaGear", "Actor", 1, 1),
    ("BOOTS", "DukeBoots", "Actor", 1, 1),
    ("HEATSENSOR", "DukeNightVision", "Actor", 1, 1),
    ("HOLODUKE", "DukeHoloduke", "Actor", 1, 1),
    ("JETPACK", "DukeJetpack", "ArtiFly", 1, 1),
    ("AMMO", "DukePistolAmmo", "Clip", 12, 200),
    ("SHOTGUNAMMO", "DukeShells", "Shell", 10, 50),
    ("BATTERYAMMO", "DukeChaingunAmmo", "Actor", 50, 200),
    ("RPGAMMO", "DukeRockets", "RocketAmmo", 5, 50),
    ("CRYSTALAMMO", "DukeCrystals", "Actor", 5, 50),
    ("DEVISTATORAMMO", "DukeDevastatorAmmo", "Actor", 15, 99),
    ("FREEZEAMMO", "DukeFreezeAmmo", "Actor", 25, 99),
];

/// Weapon overlay frame: tile define, offset, Duke screen centre.
type HudTile = (&'static str, i32, i32, i32);

/// TNT1 (nothing drawn) in a weapon state.
const HIDDEN: usize = usize::MAX;

/// One line of a weapon's `Fire` sequence.
enum Step {
    /// Frame index (or [`HIDDEN`]), tics, action.
    F(usize, i32, &'static str),
    /// Start a state label.
    Label(&'static str),
}

use Step::{F, Label};

struct Gun {
    class: &'static str,
    parent: &'static str,
    slot: u8,
    pickup: &'static str,
    ammo: &'static str,
    give: i32,
    frames: &'static [HudTile],
    ready: usize,
    /// `Fire:` (ending in `Goto Ready`), plus any labels it jumps to.
    fire: &'static [Step],
    /// Muzzle-flash overlay frames and tics (`Flash:`); `HIDDEN` only lights.
    flash: &'static [(usize, i32)],
}

const PISTOL_SHELL: &str = "A_SpawnItemEx(\"DukeShell\", 8, -8, 30, 0, -3, 3)";
const CHAINGUN_SHELL: &str = "A_SpawnItemEx(\"DukeShell\", 8, -10, 26, 0, -4, 3)";

// Positions are Duke's `displayweapon` centres with the weapon fully raised;
// tics are Duke's `kickback_pic` counts at 35 Hz.
const GUNS: &[Gun] = &[
    Gun {
        class: "DukeKick",
        parent: "Fist",
        slot: 1,
        pickup: "",
        ammo: "",
        give: 0,
        frames: &[("KNEE", 0, 160, 186), ("KNEE", 1, 140, 150)],
        ready: HIDDEN,
        fire: &[
            F(0, 3, ""),
            F(1, 5, "A_Punch"),
            F(0, 3, ""),
            F(HIDDEN, 4, "A_ReFire"),
        ],
        flash: &[],
    },
    Gun {
        class: "DukePistol",
        parent: "Pistol",
        slot: 2,
        pickup: "FIRSTGUNSPRITE",
        ammo: "Clip",
        give: 12,
        frames: &[
            ("FIRSTGUN", 0, 195, 164),
            ("FIRSTGUN", 1, 195, 164),
            ("FIRSTGUN", 2, 192, 164),
            ("FIRSTGUN", 4, 194, 150),
            ("FIRSTGUN", 5, 224, 140),
            ("FIRSTGUN", 5, 224, 130),
        ],
        ready: 0,
        // Duke reloads after every 12 shots.
        fire: &[
            F(1, 2, "A_FirePistol"),
            F(2, 2, PISTOL_SHELL),
            F(0, 0, "A_GiveInventory(\"DukePistolShots\", 1)"),
            F(
                0,
                0,
                "A_JumpIfInventory(\"DukePistolShots\", 12, \"Reload\")",
            ),
            F(0, 2, ""),
            F(0, 0, "A_ReFire"),
            F(0, 0, "Goto Ready"),
            Label("Reload"),
            F(0, 0, "A_TakeInventory(\"DukePistolShots\", 12)"),
            F(3, 4, "A_PlaySound(\"duke/eject_clip\")"),
            F(4, 4, "A_PlaySound(\"duke/insert_clip\")"),
            F(5, 8, ""),
            F(4, 6, ""),
            F(3, 4, ""),
        ],
        flash: &[(HIDDEN, 3)],
    },
    Gun {
        class: "DukeShotgun",
        parent: "Shotgun",
        slot: 3,
        pickup: "SHOTGUNSPRITE",
        ammo: "Shell",
        give: 10,
        frames: &[
            ("SHOTGUN", 0, 150, 122),
            ("SHOTGUN", 3, 162, 140),
            ("SHOTGUN", 3, 182, 180),
            ("SHOTGUN", 4, 202, 130),
            ("SHOTGUN", 5, 238, 116),
            ("SHOTGUN", 6, 244, 116),
            ("SHOTGUN", 4, 192, 126),
            ("SHOTGUN", 2, 172, 121),
            ("SHOTGUN", 2, 202, 154),
        ],
        ready: 0,
        // Kick back, settle, then pump: cock at kickback 15, shell at 24.
        fire: &[
            F(0, 2, "A_FireShotgun"),
            F(2, 2, ""),
            F(1, 1, ""),
            F(0, 4, ""),
            F(1, 5, ""),
            F(3, 2, ""),
            F(3, 1, "A_PlaySound(\"duke/shotgun_cock\")"),
            F(4, 5, ""),
            F(5, 5, ""),
            F(
                4,
                0,
                "A_SpawnItemEx(\"DukeShotgunShell\", 8, -10, 28, -1, -3, 3)",
            ),
            F(4, 5, ""),
            F(6, 4, ""),
            F(0, 0, "A_ReFire"),
        ],
        flash: &[(7, 2), (8, 2)],
    },
    Gun {
        class: "DukeRipper",
        parent: "Chaingun",
        slot: 4,
        pickup: "CHAINGUNSPRITE",
        ammo: "DukeChaingunAmmo",
        give: 50,
        frames: &[("CHAINGUN", 0, 180, 180), ("CHAINGUN", 0, 180, 183)],
        ready: 0,
        // A shot and a shell every 3 Duke tics.
        fire: &[
            F(1, 2, "A_FireCGun"),
            F(1, 0, CHAINGUN_SHELL),
            F(0, 2, ""),
            F(1, 2, "A_FireCGun"),
            F(1, 0, CHAINGUN_SHELL),
            F(0, 1, ""),
            F(0, 0, "A_ReFire"),
        ],
        flash: &[(HIDDEN, 2)],
    },
    Gun {
        class: "DukeRPG",
        parent: "RocketLauncher",
        slot: 5,
        pickup: "RPGSPRITE",
        ammo: "RocketAmmo",
        give: 5,
        frames: &[("RPGGUN", 0, 176, 96), ("RPGGUN", 1, 176, 96)],
        ready: 0,
        fire: &[
            F(0, 4, "A_GunFlash"),
            F(0, 12, "A_FireCustomMissile(\"DukeRocket\")"),
            F(0, 0, "A_ReFire"),
        ],
        flash: &[(1, 3)],
    },
    Gun {
        class: "DukeShrinker",
        parent: "PlasmaRifle",
        slot: 7,
        pickup: "SHRINKERSPRITE",
        ammo: "DukeCrystals",
        give: 10,
        frames: &[("SHRINKER", 0, 196, 160), ("SHRINKER", 1, 196, 160)],
        ready: 0,
        fire: &[
            F(1, 4, "A_FireCustomMissile(\"DukeShrinkSpark\")"),
            F(0, 12, ""),
            F(0, 0, "A_ReFire"),
        ],
        flash: &[],
    },
    Gun {
        class: "DukeExpander",
        parent: "PlasmaRifle",
        slot: 7,
        pickup: "GROWAMMO",
        ammo: "DukeGrowAmmo",
        give: 10,
        frames: &[("EXPANDER", 0, 196, 160), ("SHRINKER", 0, 196, 160)],
        ready: 0,
        fire: &[
            F(1, 4, "A_FireCustomMissile(\"DukeGrowSpark\")"),
            F(0, 12, ""),
            F(0, 0, "A_ReFire"),
        ],
        flash: &[],
    },
    Gun {
        class: "DukeDevastator",
        parent: "PlasmaRifle",
        slot: 8,
        pickup: "DEVISTATORSPRITE",
        ammo: "DukeDevastatorAmmo",
        give: 15,
        frames: &[("DEVISTATOR", 0, 245, 158), ("DEVISTATOR", 1, 245, 158)],
        ready: 0,
        fire: &[
            F(1, 3, "A_FireCustomMissile(\"DukeDevRocket\")"),
            F(0, 3, ""),
            F(0, 0, "A_ReFire"),
        ],
        flash: &[],
    },
    Gun {
        class: "DukeFreezer",
        parent: "PlasmaRifle",
        slot: 0,
        pickup: "FREEZESPRITE",
        ammo: "DukeFreezeAmmo",
        give: 25,
        frames: &[("FREEZE", 0, 222, 181), ("FREEZE", 0, 222, 184)],
        ready: 0,
        fire: &[
            F(1, 3, "A_FireCustomMissile(\"DukeFreezeBlast\")"),
            F(0, 4, ""),
            F(0, 0, "A_ReFire"),
        ],
        flash: &[],
    },
];

/// Projectiles: class, tile, speed, damage, launch sound, explodes.
const MISSILES: &[(&str, &str, i32, i32, &str, bool)] = &[
    ("DukeLaser", "FIRELASER", 30, 2, "", false),
    ("DukeSpit", "SPIT", 20, 2, "LIZARD_SPIT", false),
    ("DukeOctaBall", "COOLEXPLOSION1", 15, 4, "", false),
    ("DukeRocket", "RPG", 25, 20, "", true),
    ("DukeDevRocket", "RPG", 30, 8, "RPG_SHOOT", false),
    ("DukeShrinkSpark", "SHRINKSPARK", 25, 8, "", false),
    ("DukeGrowSpark", "GROWSPARK", 25, 8, "", false),
    ("DukeFreezeBlast", "FREEZEBLAST", 25, 6, "CAT_FIRE", false),
];

/// Doom logical sounds re-pointed at Duke sounds (`SNDINFO` overrides).
const DOOM_SOUNDS: &[(&str, &str)] = &[
    ("weapons/pistol", "PISTOL_FIRE"),
    ("weapons/shotgf", "SHOTGUN_FIRE"),
    ("weapons/chngun", "CHAINGUN_FIRE"),
    ("weapons/rocklf", "RPG_SHOOT"),
    ("weapons/rocklx", "RPG_EXPLODE"),
    ("weapons/plasmaf", "SHRINKER_FIRE"),
    ("switches/normbutn", "SWITCH_ON"),
    ("switches/exitbutn", "SWITCH_ON"),
    ("doors/open", "DOOR_OPERATE1"),
    ("doors/close", "DOOR_OPERATE1"),
    ("platforms/start", "ELEVATOR_ON"),
    ("platforms/stop", "ELEVATOR_OFF"),
    ("floors/move", "ELEVATOR_ON"),
    ("misc/teleport", "TELEPORTER"),
    ("misc/i_pkup", "DUKE_GET"),
    ("misc/p_pkup", "DUKE_GET"),
    ("misc/k_pkup", "DUKE_GET"),
    ("misc/w_pkup", "DUKE_GETWEAPON2"),
    ("misc/secret", "SECRETLEVELSND"),
    ("world/barrelx", "RPG_EXPLODE"),
    ("pl_pain", "DUKE_LONGTERM_PAIN"),
    ("player/male/pain", "DUKE_LONGTERM_PAIN"),
    ("pl_death", "DUKE_DEAD"),
    ("player/male/death", "DUKE_DEAD"),
    ("pl_xdeath", "DUKE_DEAD"),
    ("player/male/xdeath", "DUKE_DEAD"),
    ("player/male/land", "DUKE_LAND"),
    ("player/male/grunt", "DUKE_GRUNT"),
    ("*noway", "DUKE_GRUNT"),
    ("player/male/fist", "KICK_HIT"),
];

/// Marker sprites (effectors, activators, …) that never become things.
const MARKERS: &[&str] = &[
    "SECTOREFFECTOR",
    "ACTIVATOR",
    "TOUCHPLATE",
    "ACTIVATORLOCKED",
    "MUSICANDSFX",
    "LOCATORS",
    "CYCLER",
    "MASTERSWITCH",
    "GPSPEED",
    "APLAYER",
];

/// Exploding props this thin (`xrepeat`) are hidden charges in Duke.
pub(crate) const HIDDEN_CHARGE_XREPEAT: u8 = 8;
/// Doom tics the opening-scene car flies before blowing up (208 Duke tics).
const CAR_LIFE: i32 = 243;

/// Pipebomb throw (Raze `operateweapon`): held fire winds it up for up to
/// 6 tics, each adding 32 Build units/tic to the 140 base; looking up or
/// down tilts it (`zvel = -512 - horiz * 20`). In map units/tic and degrees.
const THROW_HOLD: i32 = 6;
const THROW_BASE: &str = "7.5";
const THROW_STEP: &str = "1.7";
const THROW_LIFT: &str = "0.86";
const THROW_PITCH: &str = "0.075";
/// Crouched throw (Raze `on_ground && SB_CROUCH`): vel = 15/16 + hold×2.
const THROW_CROUCH: &str = "0.94";
const THROW_CROUCH_STEP: &str = "2";

/// Length of an armed tripbomb laser (map units).
const TRIPBOMB_LASER: i32 = 1024;
/// Editor number of the projectile shooter spawned for `SE 36`.
const SHOOTER_EDNUM: i32 = 26990;
/// Editor number of the `SE 33` quake-debris spawner.
const QUAKE_DEBRIS_EDNUM: i32 = 26991;
const CAMERA_EDNUM: i32 = 26992;
const VIEWSCREEN_EDNUM: i32 = 26993;
const RESPAWN_EDNUM: i32 = 26994;
const BUBBLE_MAKER_EDNUM: i32 = 26995;
const LIGHTNING_EDNUM: i32 = 26996;
const FORCE_SPHERE_EDNUM: i32 = 26997;
const FRAME_EFFECT_EDNUM: i32 = 26998;

/// Enemy tiles that are not the base actor: jetpack and swimming variants fly,
/// and the ones that sit or stand guard do not walk.
const VARIANT_FLAGS: &[(&str, &str)] = &[
    ("LIZTROOPJETPACK", "+FLOAT\n  +NOGRAVITY"),
    ("LIZTROOPSTAYPUT", "Speed 0"),
    ("LIZTROOPONTOILET", "Speed 0"),
    ("LIZTROOPJUSTSIT", "Speed 0"),
    ("LIZTROOPDUCKING", "Speed 0"),
    ("PIGCOPSTAYPUT", "Speed 0"),
    ("PIGCOPDIVE", "+FLOAT\n  +NOGRAVITY"),
    ("LIZMANSTAYPUT", "Speed 0"),
    ("NEWBEASTSTAYPUT", "Speed 0"),
    ("NEWBEASTHANG", "+NOGRAVITY"),
];

/// Duke actor tics (CON `delay` counts 120 Hz clock units in steps of 4 per
/// 30 Hz tic) → Doom 35 Hz tics.
fn duke_tics(delay: i32) -> i32 {
    ((delay.max(0) / 4 + 1) * 35 + 15) / 30
}

/// Converted Duke content, ready for [`crate::wad`] to write.
#[derive(Debug, Default)]
pub struct DukeContent {
    /// `DECORATE` text.
    pub decorate: String,
    /// `KEYCONF` text.
    pub keyconf: String,
    /// `SNDINFO` text.
    pub sndinfo: String,
    /// `SNDSEQ` text (door / lift sounds).
    pub sndseq: String,
    /// Compiled CON events (`CONEVT`).
    pub conevt: String,
    /// Sound lumps: (lump name, GRP file).
    pub sounds: BTreeMap<String, String>,
    /// Sprite lumps.
    pub sprites: SpriteSet,
}

/// Builds [`DukeContent`] and places map sprites (see [`SpriteMapper`]).
pub struct Duke<'a> {
    con: &'a ConData,
    tiles: &'a TileSet<'a>,
    grp: &'a Grp<'a>,
    out: DukeContent,
    sfx: BTreeMap<String, String>,
    doom_sfx: Vec<(&'static str, String)>,
    things: BTreeMap<i32, i32>,
    /// Enemy editor number → (radius, height).
    enemies: BTreeMap<i32, (i32, i32)>,
    decor: BTreeMap<(i32, u8, u16), i32>,
    ambient: BTreeMap<i32, i32>,
    /// Mover sequences already written, by `MUSICANDSFX` (lotag, hitag).
    mover_seqs: BTreeMap<(i16, i16), Option<String>>,
    /// Sound numbers with a written `:DukeSnd<n>` sequence.
    seq_sounds: BTreeSet<i16>,
    /// `DukeHiddenCharge` editor number, once written.
    hidden_charge: Option<i32>,
    /// Wall-break debris spots written, by kind.
    wall_breaks: BTreeMap<WallKind, i32>,
    next_decor: i32,
    /// Palette-21 trooper tiles and the captain editor number.
    captain_tiles: BTreeSet<i32>,
    captain_ednum: Option<i32>,
    /// Palette-21 non-trooper tiles → tougher mini-boss editor number.
    miniboss: BTreeMap<i32, i32>,
}

impl<'a> Duke<'a> {
    /// Generate the fixed classes (player, weapons, items, enemies).
    #[must_use]
    pub fn new(con: &'a ConData, tiles: &'a TileSet<'a>, grp: &'a Grp<'a>) -> Self {
        let mut d = Self {
            con,
            tiles,
            grp,
            out: DukeContent::default(),
            sfx: BTreeMap::new(),
            doom_sfx: Vec::new(),
            things: BTreeMap::new(),
            enemies: BTreeMap::new(),
            decor: BTreeMap::new(),
            ambient: BTreeMap::new(),
            mover_seqs: BTreeMap::new(),
            seq_sounds: BTreeSet::new(),
            hidden_charge: None,
            wall_breaks: BTreeMap::new(),
            next_decor: DECOR_FIRST,
            captain_tiles: BTreeSet::new(),
            captain_ednum: None,
            miniboss: BTreeMap::new(),
        };
        d.doom_sounds();
        d.effects();
        d.fire();
        d.debris();
        d.missiles();
        d.weapons();
        d.player();
        d.items();
        d.keys();
        d.out.decorate.push_str("actor DukeMonster\n{\n}\n\n");
        for e in ENEMIES {
            d.enemy(e);
        }
        d.shooter();
        d.quake_debris();
        d.cameras();
        d.respawn_marker();
        d.water_bubbles();
        d.natural_lightning();
        d.force_sphere();
        d.frame_effect();
        d
    }

    /// Finish and hand over the content.
    #[must_use]
    pub fn finish(mut self) -> DukeContent {
        for (logical, lump) in &self.sfx {
            let _ = writeln!(self.out.sndinfo, "{logical} {lump}");
        }
        for (doom, lump) in &self.doom_sfx {
            let _ = writeln!(self.out.sndinfo, "{doom} {lump}");
        }
        self.out.conevt = self.con.conevt();
        if self.out.conevt.is_empty() {
            self.out.conevt = "event ENTERLEVEL\nend\n".into();
        }
        self.out
    }

    fn tile(&self, name: &str) -> Option<i32> {
        self.con
            .value(name)
            .filter(|&t| self.tiles.get(t).is_some())
    }

    /// Logical sound name for CON sound `define`, registering its lump.
    fn sound(&mut self, define: &str) -> Option<String> {
        let con = self.con;
        let def = con.sound(define)?;
        let stem = def
            .file
            .rsplit_once('.')
            .map_or(def.file.as_str(), |(s, _)| s);
        if stem.is_empty() || stem.len() > 8 || self.grp.get(&def.file).is_none() {
            return None;
        }
        let logical = format!("duke/{}", def.name.to_ascii_lowercase());
        self.out
            .sounds
            .entry(stem.to_owned())
            .or_insert_with(|| def.file.clone());
        self.sfx.insert(logical.clone(), stem.to_owned());
        Some(logical)
    }

    fn doom_sounds(&mut self) {
        for &(doom, duke) in DOOM_SOUNDS {
            if let Some(lump) = self.sound(duke).and_then(|l| self.sfx.get(&l).cloned()) {
                self.doom_sfx.push((doom, lump));
            }
        }
    }

    /// Frames of `action` for an actor whose art starts at `base`.
    fn action_frames(&mut self, group: &str, base: i32, action: &str) -> Vec<(String, char, i32)> {
        let Some(act) = self.con.actions.get(action).copied() else {
            return Vec::new();
        };
        self.frames_of(group, base, act)
    }

    fn frames_of(&mut self, group: &str, base: i32, act: Action) -> Vec<(String, char, i32)> {
        let views = if matches!(act.views, 5 | 7 | 8) {
            act.views
        } else {
            1
        };
        let tics = duke_tics(act.delay);
        let mut out = Vec::new();
        for f in 0..act.frames.clamp(1, 26) {
            let t = act.tile(base, f);
            if self.tiles.get(t).is_none() {
                continue;
            }
            // Some actions only ship the front views: show those from every side.
            let views = if (t..t + views).all(|k| self.tiles.get(k).is_some()) {
                views
            } else {
                1
            };
            let (s, l) = self.out.sprites.frame(group, t, views, 0);
            out.push((s, l, tics));
        }
        out
    }

    /// Duke's hard-coded explosion: `EXPLOSION2` and the tiles after it.
    fn explosion(&mut self) -> Vec<(String, char, i32)> {
        let Some(base) = self.tile("EXPLOSION2") else {
            return Vec::new();
        };
        (base..base + 21)
            .take_while(|&t| self.tiles.get(t).is_some())
            .collect::<Vec<_>>()
            .into_iter()
            .map(|t| {
                let (n, l) = self.out.sprites.frame("DukeExplosion", t, 1, 0);
                (n, l, 2)
            })
            .collect()
    }

    /// Trash fires and burning actors are additive, so the flame reads as light.
    fn flame_actor(&self, tile: i32) -> bool {
        let Some(actor) = self.con.actors.get(&tile) else {
            return false;
        };
        let action = actor.action.as_deref().or_else(|| {
            actor
                .state
                .as_ref()
                .and_then(|name| self.con.state_action.get(name))
                .map(String::as_str)
        });
        matches!(action, Some("FIRE_FRAMES" | "BURNING_FLAME"))
    }

    /// CON action for this tile, else its picanm loop.
    ///
    /// Trash fires are `useractor`s whose `state` starts `FIRE_FRAMES`. The
    /// tile's `picanm` has no frames, so a picanm-only decoration stays frozen.
    fn actor_frames(&mut self, group: &str, tile: i32, pal: u8) -> Vec<(String, char, i32)> {
        let name = self.con.actors.get(&tile).and_then(|a| {
            a.action.clone().or_else(|| {
                a.state
                    .as_ref()
                    .and_then(|s| self.con.state_action.get(s))
                    .cloned()
            })
        });
        if let Some(name) = name {
            let frames = self.action_frames(group, tile, &name);
            if frames.len() > 1 {
                return frames;
            }
        }
        self.anim_frames(group, tile, pal)
    }

    /// Picanm animation of `tile` as frames (one frame when not animated).
    fn anim_frames(&mut self, group: &str, tile: i32, pal: u8) -> Vec<(String, char, i32)> {
        if self.tiles.get(tile).is_none() {
            return Vec::new();
        }
        let mut frames = self.tiles.animation(tile);
        if frames.is_empty() {
            frames.push((tile, 1));
        }
        frames
            .into_iter()
            .take(64)
            .map(|(t, tics)| {
                let (n, l) = self.out.sprites.frame(group, t, 1, pal);
                (n, l, tics)
            })
            .collect()
    }

    /// Duke hitscan sparks and blood in place of Doom's `PUFF` / `BLUD` sprites.
    fn effects(&mut self) {
        for (class, base, tile, tics) in [
            ("DukePuff", "BulletPuff", "SHOTSPARK1", 3),
            ("DukeBlood", "Blood", "BLOOD", 4),
            ("DukeTeleportFog", "TeleportFog", "TRANSPORTERSTAR", 4),
        ] {
            let Some(t) = self.tile(tile) else { continue };
            let mut s = format!(
                "actor {class} : {base} replaces {base}\n{{\n  RenderStyle Normal\n  \
                 Alpha 1\n  States\n  {{\n  Spawn:\n  Melee:\n"
            );
            for f in (t..t + 4).take_while(|&f| self.tiles.get(f).is_some()) {
                let (n, l) = self.out.sprites.frame(class, f, 1, 0);
                let bright = if base == "Blood" { "" } else { " Bright" };
                let _ = writeln!(s, "    {n} {l} {tics}{bright}");
            }
            s.push_str("    Stop\n  }\n}\n\n");
            self.out.decorate.push_str(&s);
        }
    }

    fn missiles(&mut self) {
        let explosion = self.explosion();
        for &(class, tile, speed, damage, sfx, explodes) in MISSILES {
            let Some(t) = self.tile(tile).or_else(|| {
                (class == "DukeGrowSpark")
                    .then(|| self.tile("SHRINKSPARK"))
                    .flatten()
            }) else {
                continue;
            };
            let fly = self.anim_frames(class, t, 0);
            let see = self.sound(sfx);
            let mut s = format!(
                "actor {class}\n{{\n  Radius 6\n  Height 8\n  Speed {speed}\n  Damage {damage}\n  \
                 +NOBLOCKMAP\n  +NOGRAVITY\n  +DROPOFF\n  +MISSILE\n"
            );
            if class == "DukeDevRocket" {
                s.push_str("  Scale 0.5\n");
            }
            if class == "DukeShrinkSpark" {
                s.push_str("  DamageType \"Shrink\"\n");
            }
            if class == "DukeGrowSpark" {
                s.push_str("  DamageType \"Grow\"\n");
            }
            if class == "DukeFreezeBlast" {
                s.push_str("  DamageType \"Ice\"\n");
            }
            if let Some(see) = see {
                let _ = writeln!(s, "  SeeSound \"{see}\"");
            }
            if explodes {
                s.push_str("  SeeSound \"weapons/rocklf\"\n  DeathSound \"weapons/rocklx\"\n");
            }
            s.push_str("  States\n  {\n  Spawn:\n");
            for (n, l, tics) in &fly {
                let _ = writeln!(s, "    {n} {l} {tics} Bright");
            }
            s.push_str("    Loop\n  Death:\n");
            let boom = if explodes && !explosion.is_empty() {
                &explosion
            } else {
                &fly
            };
            for (i, (n, l, _)) in boom.iter().enumerate() {
                let act = if i == 0 && explodes { " A_Explode" } else { "" };
                let _ = writeln!(s, "    {n} {l} 2 Bright{act}");
            }
            s.push_str("    Stop\n  }\n}\n\n");
            self.out.decorate.push_str(&s);
        }
    }

    fn weapons(&mut self) {
        let mut slots: BTreeMap<u8, Vec<&str>> = BTreeMap::new();
        for g in GUNS {
            let mut frames: Vec<Option<(String, char)>> = Vec::new();
            for &(name, add, x, y) in g.frames {
                let tile = self
                    .tile(name)
                    .map(|t| t + add)
                    .filter(|&t| self.tiles.get(t).is_some());
                frames.push(tile.map(|t| self.out.sprites.hud_frame(g.class, t, x, y)));
            }
            let frame = |i: usize| -> String {
                frames
                    .get(i)
                    .and_then(Option::as_ref)
                    .map_or_else(|| "TNT1 A".to_owned(), |(n, l)| format!("{n} {l}"))
            };
            let pickup = self.tile(g.pickup);
            let mut s = format!("actor {} : {}", g.class, g.parent);
            if let Some(t) = pickup {
                let _ = write!(s, " {}", CLASS_BASE + t);
                self.things.insert(t, CLASS_BASE + t);
            }
            s.push_str("\n{\n");
            if !g.ammo.is_empty() {
                let give = pickup
                    .and_then(|t| self.con.actors.get(&t))
                    .and_then(|a| a.pickup.weapon.map(|w| w.1))
                    .filter(|&n| n > 0)
                    .unwrap_or(g.give);
                let _ = writeln!(
                    s,
                    "  Weapon.AmmoType \"{}\"\n  Weapon.AmmoGive {give}\n  Weapon.AmmoUse 1",
                    g.ammo
                );
            }
            // Lower = preferred on ammo dry (docs/30 N3). Keep `Gun.slot` for keys only.
            let order = match g.class {
                "DukeDevastator" => 400,
                "DukeRipper" => 700,
                "DukeShotgun" => 1300,
                "DukePistol" => 1900,
                "DukeFreezer" => 2000,
                "DukeShrinker" | "DukeExpander" => 2300,
                "DukeRPG" => 2500,
                "DukeKick" => 3700,
                _ => u32::from(g.slot) * 1000,
            };
            let _ = writeln!(s, "  Weapon.SelectionOrder {order}");
            s.push_str("  States\n  {\n");
            if let Some(t) = pickup {
                let (n, l) = self.out.sprites.frame(g.class, t, 1, 0);
                let _ = writeln!(s, "  Spawn:\n    {n} {l} -1\n    Stop");
            }
            for (label, act) in [
                ("Ready", "A_WeaponReady"),
                ("Deselect", "A_Lower"),
                ("Select", "A_Raise"),
            ] {
                let _ = writeln!(s, "  {label}:\n    {} 1 {act}\n    Loop", frame(g.ready));
            }
            s.push_str("  Fire:\n");
            for step in g.fire {
                match *step {
                    F(_, 0, act) if act.starts_with("Goto ") => {
                        let _ = writeln!(s, "    {act}");
                    }
                    F(i, tics, act) => {
                        let _ = writeln!(s, "    {} {tics} {act}", frame(i));
                    }
                    Label(l) => {
                        let _ = writeln!(s, "  {l}:");
                    }
                }
            }
            s.push_str("    Goto Ready\n");
            if !g.flash.is_empty() {
                s.push_str("  Flash:\n");
                for (k, &(i, tics)) in g.flash.iter().enumerate() {
                    let light = if k == 0 { " A_Light1" } else { "" };
                    let _ = writeln!(s, "    {} {tics} Bright{light}", frame(i));
                }
                s.push_str("    Goto LightDone\n");
            }
            s.push_str("  }\n}\n\n");
            self.out.decorate.push_str(&s);
            slots.entry(g.slot).or_default().push(g.class);
        }
        self.out.decorate.push_str(
            "actor DukeGrowAmmo : Actor\n{\n  Inventory.Amount 5\n  Inventory.MaxAmount 50\n}\n\n",
        );
        self.bombs(&mut slots);
        self.out
            .keyconf
            .push_str("clearplayerclasses\naddplayerclass DukePlayer\n");
        // Every slot, so none falls back to DoomPlayer's defaults.
        for slot in 0..=9 {
            let list = slots.get(&slot).map(|l| l.join(" ")).unwrap_or_default();
            let _ = writeln!(self.out.keyconf, "setslot {slot} {list}");
        }
        for sfx in ["SHOTGUN_COCK", "EJECT_CLIP", "INSERT_CLIP"] {
            let _ = self.sound(sfx);
        }
        self.out.decorate.push_str(
            "actor DukePistolShots : Actor\n{\n  Inventory.Amount 1\n  Inventory.MaxAmount 12\n}\n\n",
        );
    }

    /// Pipebombs (slot 6: throw, then fire again to detonate with the remote)
    /// and tripbombs (slot 9). A placed tripbomb arms after 2 s and blows when a
    /// shootable actor crosses its facing laser (`A_JumpIfLaserHit`).
    fn bombs(&mut self, slots: &mut BTreeMap<u8, Vec<&'static str>>) {
        let explosion = self.explosion();
        let boom = |s: &mut String, damage: i32, radius: i32| {
            s.push_str("  Death:\n    TNT1 A 0 A_Scream\n");
            scatter(s, "TNT1 A", "DukeScrap", 8, 4);
            if explosion.is_empty() {
                let _ = writeln!(s, "    TNT1 A 2 A_Explode({damage}, {radius})");
            }
            for (i, (n, l, _)) in explosion.iter().enumerate() {
                let act = if i == 0 {
                    format!(" A_Explode({damage}, {radius})")
                } else {
                    String::new()
                };
                let _ = writeln!(s, "    {n} {l} 2 Bright{act}");
            }
            s.push_str("    Stop\n  }\n}\n\n");
        };
        let max = |name: &str, fallback: i32| self.con.value(name).unwrap_or(fallback);
        let (pipe_max, trip_max) = (max("MAXHANDBOMBAMMO", 50), max("MAXTRIPBOMBAMMO", 10));
        let box_give = max("HANDBOMBBOX", 5);
        let pipe_damage = max("HANDBOMB_WEAPON_STRENGTH", 140);
        let pipe_radius = max("PIPEBOMBRADIUS", 2500) / 16;
        let trip_radius = max("TRIPBOMBBLASTRADIUS", 3880) / 16;
        let _ = write!(
            self.out.decorate,
            "actor DukePipebombAmmo : Actor\n{{\n  Inventory.Amount 1\n  Inventory.MaxAmount {pipe_max}\n}}\n\n\
             actor DukeTripbombAmmo : Actor\n{{\n  Inventory.Amount 1\n  Inventory.MaxAmount {trip_max}\n}}\n\n\
             actor DukeBombsOut : Actor\n{{\n  Inventory.Amount 1\n  Inventory.MaxAmount 1\n}}\n\n\
             actor DukeThrowPower : Actor\n{{\n  Inventory.Amount 1\n  Inventory.MaxAmount {THROW_HOLD}\n}}\n\n"
        );

        // The live bomb; a map-placed HEAVYHBOMB is one to pick up.
        let bomb_tile = self.tile("HEAVYHBOMB");
        let bomb =
            bomb_tile.map_or_else(Vec::new, |t| self.anim_frames("DukeThrownPipebomb", t, 0));
        let pipe_boom = self
            .sound("PIPEBOMB_EXPLODE")
            .unwrap_or_else(|| "weapons/rocklx".into());
        let bounce = self.sound("PIPEBOMB_BOUNCE").unwrap_or_default();
        let mut s = format!(
            "actor DukeThrownPipebomb\n{{\n  Health 1\n  Radius 4\n  Height 4\n  \
             XScale {}\n  YScale {}\n  +SHOOTABLE\n  \
             +NOBLOOD\n  +DROPOFF\n  +BOUNCEONFLOORS\n  +BOUNCEONWALLS\n  BounceFactor 0.5\n  \
             BounceSound \"{bounce}\"\n  DeathSound \"{pipe_boom}\"\n  States\n  {{\n  Spawn:\n",
            // Duke `EGS` spawns HEAVYHBOMB at xrepeat/yrepeat 9. Face sprites
            // are `xrepeat/80` wide and `yrepeat/64` tall.
            9.0 / 80.0,
            9.0 / 64.0
        );
        push_loop(&mut s, &bomb);
        boom(&mut s, pipe_damage, pipe_radius);
        self.out.decorate.push_str(&s);

        let hud = |d: &mut Self, tile: &str, add: i32, x: i32, y: i32| -> String {
            d.tile(tile)
                .map(|t| t + add)
                .filter(|&t| d.tiles.get(t).is_some())
                .map_or_else(
                    || "TNT1 A".to_owned(),
                    |t| {
                        let (n, l) = d.out.sprites.hud_frame("DukePipebombWeapon", t, x, y);
                        format!("{n} {l}")
                    },
                )
        };
        let throw: Vec<String> = (0..3)
            .map(|i| hud(self, "HANDTHROW", i, 190, 180))
            .collect();
        let remote: Vec<String> = (0..3)
            .map(|i| hud(self, "HANDREMOTE", i, 102, 178))
            .collect();
        let weapon = |class: &str, parent: &str, ednum: Option<(i32, i32)>, give: i32| {
            let mut s = format!("actor {class} : {parent}");
            if let Some((ednum, _)) = ednum {
                let _ = write!(s, " {ednum}");
            }
            let _ = write!(
                s,
                "\n{{\n  Weapon.AmmoType \"DukePipebombAmmo\"\n  Weapon.AmmoGive {give}\n  \
                 Weapon.AmmoUse 0\n  Weapon.SelectionOrder 5000\n"
            );
            s
        };
        let box_tile = self.tile("HBOMBAMMO");
        let mut s = weapon(
            "DukePipebombWeapon",
            "Actor",
            bomb_tile.map(|t| (CLASS_BASE + t, t)),
            1,
        );
        s.push_str("  States\n  {\n");
        if let Some(t) = bomb_tile {
            self.things.insert(t, CLASS_BASE + t);
            let (n, l) = self.out.sprites.frame("DukeThrownPipebomb", t, 1, 0);
            let _ = writeln!(s, "  Spawn:\n    {n} {l} -1\n    Stop");
        }
        let _ = write!(
            s,
            "  Ready:\n    TNT1 A 0 A_JumpIfInventory(\"DukeBombsOut\", 1, \"ReadyRemote\")\n    \
             {t0} 1 A_WeaponReady\n    Loop\n  ReadyRemote:\n    {r0} 1 A_WeaponReady\n    \
             Goto Ready\n  Deselect:\n    {t0} 1 A_Lower\n    Loop\n  Select:\n    {t0} 1 A_Raise\n    \
             Loop\n  Fire:\n    TNT1 A 0 A_JumpIfInventory(\"DukeBombsOut\", 1, \"Detonate\")\n    \
             TNT1 A 0 A_JumpIfInventory(\"DukePipebombAmmo\", 1, \"Throw\")\n    {t0} 8\n    Goto Ready\n  \
             Throw:\n    {t0} 0 A_TakeInventory(\"DukeThrowPower\", {THROW_HOLD})\n  \
             Charge:\n    {t0} 1 A_GiveInventory(\"DukeThrowPower\", 1)\n    \
             {t0} 0 A_JumpIfInventory(\"DukeThrowPower\", {THROW_HOLD}, \"Wound\")\n    \
             {t0} 0 A_ReFire(\"Charge\")\n    Goto Release\n  \
             Wound:\n    {t0} 1\n    {t0} 0 A_ReFire(\"Wound\")\n  \
             Release:\n    {t1} 4 A_SpawnItemEx(\"DukeThrownPipebomb\", 16, 0, \
             32 - 24 * playercrouch, \
             ({THROW_BASE} + {THROW_STEP} * CountInv(\"DukeThrowPower\")) * (1 - playercrouch) + \
             ({THROW_CROUCH} + {THROW_CROUCH_STEP} * CountInv(\"DukeThrowPower\")) * playercrouch, 0, \
             ({THROW_LIFT} - {THROW_PITCH} * pitch) * (1 - playercrouch) + \
             (-{THROW_PITCH} * pitch) * playercrouch)\n    \
             {t1} 0 A_TakeInventory(\"DukePipebombAmmo\", 1)\n    \
             {t1} 0 A_GiveInventory(\"DukeBombsOut\", 1)\n    {t2} 6\n    Goto Ready\n  \
             Detonate:\n    {r0} 3\n    {r1} 4 A_KillChildren\n    \
             {r1} 0 A_TakeInventory(\"DukeBombsOut\", 1)\n    {r2} 6\n    Goto Ready\n  }}\n}}\n\n",
            t0 = throw[0],
            t1 = throw[1],
            t2 = throw[2],
            r0 = remote[0],
            r1 = remote[1],
            r2 = remote[2],
        );
        self.out.decorate.push_str(&s);
        slots.entry(6).or_default().push("DukePipebombWeapon");
        // The box gives the same weapon in GZDoom terms; a subclass in the same
        // slot shares the ammo.
        if let Some(t) = box_tile {
            let ednum = CLASS_BASE + t;
            self.things.insert(t, ednum);
            let frames = self.anim_frames("DukePipebombBox", t, 0);
            let mut s = format!(
                "actor DukePipebombBox : DukePipebombWeapon {ednum}\n{{\n  Weapon.AmmoGive {box_give}\n"
            );
            s.push_str("  States\n  {\n  Spawn:\n");
            push_loop(&mut s, &frames);
            s.push_str("  }\n}\n\n");
            self.out.decorate.push_str(&s);
            slots.entry(6).or_default().push("DukePipebombBox");
        }

        // Armed tripbomb: player-placed (pointers cleared, so the remote
        // can't set it off) or map-placed `TRIPBOMB`.
        let mine_tile = self.tile("TRIPBOMB");
        let mine = mine_tile.map_or_else(Vec::new, |t| self.anim_frames("DukeTripbomb", t, 0));
        let mine_frame = mine
            .first()
            .map_or_else(|| "TNT1 A".to_owned(), |(n, l, _)| format!("{n} {l}"));
        let trip_boom = self
            .sound("LASERTRIP_EXPLODE")
            .unwrap_or_else(|| pipe_boom.clone());
        if let Some(t) = self.tile("LASERLINE") {
            let (n, l) = self.out.sprites.frame("DukeLaserLine", t, 1, 0);
            let _ = write!(
                self.out.decorate,
                "actor DukeLaserLine\n{{\n  Radius 2\n  Height 2\n  Scale 0.35\n  +NOBLOCKMAP\n  \
                 +NOGRAVITY\n  +DONTTHRUST\n  RenderStyle Add\n  Alpha 0.8\n  States\n  {{\n  \
                 Spawn:\n    {n} {l} 2 Bright\n    Stop\n  }}\n}}\n\n"
            );
        }
        let mut s = String::from("actor DukeTripbomb");
        if let Some(t) = mine_tile {
            let _ = write!(s, " {}", CLASS_BASE + t);
            self.things.insert(t, CLASS_BASE + t);
        }
        let _ = write!(
            s,
            "\n{{\n  Health 1\n  Radius 4\n  Height 4\n  +SHOOTABLE\n  +NOBLOOD\n  +NOGRAVITY\n  \
             DeathSound \"{trip_boom}\"\n  States\n  {{\n  Spawn:\n    {mine_frame} 1\n    \
             {mine_frame} 69 A_ClearMaster\n  \
             Armed:\n    {mine_frame} 2 A_JumpIfLaserHit(\"Death\", {TRIPBOMB_LASER})\n    \
             Loop\n"
        );
        boom(&mut s, 100, trip_radius);
        self.out.decorate.push_str(&s);

        let pickup = self.tile("TRIPBOMBSPRITE");
        let hold: Vec<String> = (0..3)
            .map(|i| {
                self.tile("HANDHOLDINGLASER")
                    .map(|t| t + i)
                    .filter(|&t| self.tiles.get(t).is_some())
                    .map_or_else(
                        || "TNT1 A".to_owned(),
                        |t| {
                            let (n, l) =
                                self.out
                                    .sprites
                                    .hud_frame("DukeTripbombWeapon", t, 138, 159);
                            format!("{n} {l}")
                        },
                    )
            })
            .collect();
        let mut s = String::from("actor DukeTripbombWeapon : Actor");
        if let Some(t) = pickup {
            let _ = write!(s, " {}", CLASS_BASE + t);
            self.things.insert(t, CLASS_BASE + t);
        }
        let _ = write!(
            s,
            "\n{{\n  Weapon.AmmoType \"DukeTripbombAmmo\"\n  Weapon.AmmoGive 1\n  Weapon.AmmoUse 1\n  \
             Weapon.SelectionOrder 5000\n  States\n  {{\n"
        );
        if let Some(t) = pickup {
            let frames = self.anim_frames("DukeTripbombWeapon", t, 0);
            s.push_str("  Spawn:\n");
            push_loop(&mut s, &frames);
        }
        let _ = write!(
            s,
            "  Ready:\n    {h0} 1 A_WeaponReady\n    Loop\n  Deselect:\n    {h0} 1 A_Lower\n    Loop\n  \
             Select:\n    {h0} 1 A_Raise\n    Loop\n  Fire:\n    {h1} 4\n    \
             {h2} 4 A_SpawnItemEx(\"DukeTripbomb\", 12, 0, 32)\n    {h0} 10\n    Goto Ready\n  }}\n}}\n\n",
            h0 = hold[0],
            h1 = hold[1],
            h2 = hold[2],
        );
        self.out.decorate.push_str(&s);
        slots.entry(9).or_default().push("DukeTripbombWeapon");
    }

    fn player(&mut self) {
        let base = self.tile("APLAYER").unwrap_or(1405);
        let stand = self.action_frames("DukePlayer", base, "PSTAND");
        let walk = self.action_frames("DukePlayer", base, "PWALK");
        let dying = self.action_frames("DukePlayer", base, "PEXPLODE");
        let dead = self.action_frames("DukePlayer", base, "PLYINGDEAD");
        let mut s = String::from(
            "actor DukePlayer : DoomPlayer\n{\n  Radius 10\n  Height 40\n  ViewHeight 36\n  \
             Player.StartItem \"DukePistol\"\n  Player.StartItem \"DukeKick\"\n  \
             Player.StartItem \"Clip\", 48\n  JumpZ 10\n  Player.ForwardMove 1.25\n  \
             MaxStepHeight 48\n",
        );
        for define in ["DUKE_YES", "DUKE_GETWEAPON2", "DUKE_CRACK2"] {
            if let Some(logical) = self.sound(define) {
                let _ = writeln!(s, "  KillTaunt \"{logical}\"");
            }
        }
        for define in ["DUKE_CRACK_FIRST", "DUKE_CRACK", "DUKE_CRACK2"] {
            if let Some(logical) = self.sound(define) {
                let _ = writeln!(s, "  IdleTaunt \"{logical}\"");
            }
        }
        if let Some((sn, sl, _)) = stand.first() {
            let idle = format!("{sn} {sl}");
            s.push_str("  States\n  {\n  Spawn:\n");
            let _ = writeln!(s, "    {idle} -1\n    Stop\n  See:");
            for (n, l, _) in &walk {
                let _ = writeln!(s, "    {n} {l} 4");
            }
            if walk.is_empty() {
                let _ = writeln!(s, "    {idle} 4");
            }
            let _ = writeln!(
                s,
                "    Loop\n  Missile:\n    {idle} 12\n    Goto Spawn\n  \
                 Pain:\n    {idle} 4\n    {idle} 4 A_Pain\n    Goto Spawn\n  Death:"
            );
            for (i, (n, l, tics)) in dying.iter().enumerate() {
                let act = match i {
                    0 => " A_PlayerScream",
                    1 => " A_NoBlocking",
                    _ => "",
                };
                let _ = writeln!(s, "    {n} {l} {tics}{act}");
            }
            let corpse = dead
                .first()
                .or(dying.last())
                .map_or(idle.clone(), |(n, l, _)| format!("{n} {l}"));
            let _ = writeln!(s, "    {corpse} -1\n    Stop\n  }}");
        }
        s.push_str("}\n\n");
        self.out.decorate.push_str(&s);
    }

    fn items(&mut self) {
        for &(tile, class, parent, amount, max) in ITEMS {
            let Some(t) = self.tile(tile) else { continue };
            let con = self.con.actors.get(&t).map(|a| a.pickup.clone());
            let amount = con
                .as_ref()
                .and_then(|p| p.health.or(p.ammo.map(|a| a.1)))
                .filter(|&n| n > 0)
                .unwrap_or(amount);
            let quote = con
                .as_ref()
                .and_then(|p| p.quote)
                .and_then(|q| self.con.quotes.get(&q))
                .map(|q| q.replace('"', "'"));
            let frames = self.anim_frames(class, t, 0);
            let ednum = CLASS_BASE + t;
            self.things.insert(t, ednum);
            let mut s = format!(
                "actor {class} : {parent} {ednum}\n{{\n  Inventory.Amount {amount}\n  \
                 Inventory.MaxAmount {max}\n  Radius 16\n  Height 16\n"
            );
            if class == "DukeFirstAid" || class == "DukeSteroids" {
                s.push_str("  +COUNTITEM\n");
            } else if parent == "Actor" && max > 2 {
                // Its own ammo stack (a Duke weapon's `Weapon.AmmoType`).
            } else if parent == "Actor" {
                s.push_str("  +COUNTITEM\n");
            }
            if let Some(q) = quote {
                let _ = writeln!(s, "  Inventory.PickupMessage \"{q}\"");
            }
            s.push_str("  States\n  {\n  Spawn:\n");
            push_loop(&mut s, &frames);
            s.push_str("  }\n}\n\n");
            self.out.decorate.push_str(&s);
        }
        if let Some(t) = self.tile("APLAYER").or_else(|| self.tile("HOLODUKE")) {
            let frames = self.anim_frames("DukeHolodukeDecoy", t, 0);
            if !frames.is_empty() {
                let mut s = String::from(
                    "actor DukeHolodukeDecoy\n{\n  Health 1\n  Radius 16\n  Height 56\n  \
                     Speed 8\n  +SHOOTABLE\n  +SOLID\n  +FRIENDLY\n  States\n  {\n  Spawn:\n",
                );
                if frames.is_empty() {
                    s.push_str("    TNT1 A 4 A_Wander\n    Loop\n");
                } else {
                    for (n, l, tics) in &frames {
                        let _ = writeln!(s, "    {n} {l} {} A_Wander", (*tics).max(4));
                    }
                    s.push_str("    Loop\n");
                }
                s.push_str("  }\n}\n\n");
                self.out.decorate.push_str(&s);
            }
        }
    }

    /// Doom's key classes keep their numbers; their `BKEY` / `RKEY` / `YKEY`
    /// sprites are the access card in the matching palette.
    fn keys(&mut self) {
        let Some(card) = self.tile("ACCESSCARD") else {
            return;
        };
        for (sprite, pal) in [("BKEY", 0u8), ("RKEY", 21), ("YKEY", 23)] {
            for frame in ['A', 'B'] {
                self.out
                    .sprites
                    .named(&format!("{sprite}{frame}0"), card, pal);
            }
        }
    }

    fn enemy(&mut self, e: &Enemy) {
        let Some(base) = self.tile(e.tile) else {
            return;
        };
        let class = e.class;
        let [stand, walk, shoot, pain, dying, dead] =
            e.actions.map(|a| self.action_frames(class, base, a));
        let pick = |v: Vec<(String, char, i32)>, fallback: &[(String, char, i32)]| {
            if v.is_empty() { fallback.to_vec() } else { v }
        };
        let walk = if walk.is_empty() && stand.is_empty() {
            let (n, l) = self.out.sprites.frame(class, base, 1, 0);
            vec![(n, l, 8)]
        } else {
            pick(walk, &stand)
        };
        let stand = pick(stand, &walk);
        let shoot = pick(shoot, &stand);
        let strength = self
            .con
            .actors
            .get(&base)
            .map(|a| a.strength)
            .filter(|&s| s > 0)
            .or_else(|| self.con.value(e.strength))
            .unwrap_or(50);
        let ednum = CLASS_BASE + base;
        self.things.insert(base, ednum);
        self.enemies.insert(ednum, e.size);
        for v in e.variants {
            if let Some(t) = self.con.value(v) {
                self.things.entry(t).or_insert(ednum);
            }
        }
        let sfx: BTreeMap<&str, Option<String>> = if e.sfx.is_empty() {
            BTreeMap::new()
        } else {
            ["RECOG", "ROAM", "PAIN", "DYING", e.attack_sfx]
                .into_iter()
                .filter(|s| !s.is_empty())
                .map(|s| (s, self.sound(&format!("{}_{s}", e.sfx))))
                .collect()
        };
        let walk_tics = walk.first().map_or(4, |f| f.2).clamp(2, 8);
        let mut s = format!(
            "actor {class} : DukeMonster {ednum}\n{{\n  Health {strength}\n  Radius {}\n  Height {}\n  \
             Speed {}\n  PainChance 100\n  Monster\n",
            e.size.0,
            e.size.1,
            if matches!(e.class, "DukeTurret") {
                0
            } else {
                walk_tics * 2
            }
        );
        if e.float {
            s.push_str("  +FLOAT\n  +NOGRAVITY\n");
        }
        if e.size.1 >= 90 {
            s.push_str("  +BOSS\n");
        }
        for (key, prop) in [
            ("RECOG", "SeeSound"),
            ("ROAM", "ActiveSound"),
            ("PAIN", "PainSound"),
            ("DYING", "DeathSound"),
        ] {
            if let Some(Some(name)) = sfx.get(key) {
                let _ = writeln!(s, "  {prop} \"{name}\"");
            }
        }
        let attack_sound = sfx.get(e.attack_sfx).cloned().flatten();
        if let Some(a) = &attack_sound {
            let _ = writeln!(s, "  AttackSound \"{a}\"");
        }
        let line = |s: &mut String, f: &(String, char, i32), tics: i32, act: &str| {
            let _ = writeln!(s, "    {} {} {tics} {act}", f.0, f.1);
        };
        s.push_str("  States\n  {\n  Spawn:\n");
        for f in &stand {
            line(&mut s, f, 10, "A_Look");
        }
        s.push_str("    Loop\n  See:\n");
        for f in &walk {
            line(&mut s, f, walk_tics, "A_Chase");
        }
        s.push_str("    Loop\n");
        let attack = match e.attack {
            Attack::None => None,
            Attack::Missile(m) => Some(format!("A_CustomMissile(\"{m}\", 32)")),
            Attack::Bullets(n, d) => Some(format!("A_CustomBulletAttack(8, 0, {n}, {d})")),
            Attack::Charge => Some("A_SkullAttack".to_owned()),
        };
        if let Some(attack) = attack {
            s.push_str("  Missile:\n");
            line(&mut s, &shoot[0], 8, "A_FaceTarget");
            if let (Some(snd), Attack::Bullets(..)) = (&attack_sound, e.attack) {
                line(&mut s, &shoot[0], 0, &format!("A_PlaySound(\"{snd}\")"));
            }
            for (i, f) in shoot.iter().enumerate() {
                let act = if i == 0 { attack.as_str() } else { "" };
                line(&mut s, f, f.2.max(4), act);
            }
            if e.class == "DukeOverlord" {
                line(
                    &mut s,
                    &shoot[0],
                    0,
                    "A_SpawnItemEx(\"DukeForceSphere\", 0, 0, 32)",
                );
            }
            if let Attack::Charge = e.attack {
                line(&mut s, &shoot[0], 4, "");
                s.push_str("    Goto Missile+2\n");
            } else {
                s.push_str("    Goto See\n");
            }
        }
        if e.melee > 0 {
            s.push_str("  Melee:\n");
            line(&mut s, &shoot[0], 6, "A_FaceTarget");
            line(
                &mut s,
                &shoot[0],
                6,
                &format!("A_CustomMeleeAttack({})", e.melee),
            );
            s.push_str("    Goto See\n");
        }
        if let Some(p) = pain.first() {
            s.push_str("  Pain:\n");
            line(&mut s, p, 3, "");
            line(&mut s, p, 3, "A_Pain");
            s.push_str("    Goto See\n");
        }
        s.push_str("  Death:\n");
        if dying.is_empty() {
            let boom = self.explosion();
            let first = boom.first().cloned().unwrap_or_else(|| stand[0].clone());
            line(&mut s, &first, 3, "A_Scream");
            line(&mut s, &first, 0, "A_NoBlocking");
            for f in boom.iter().skip(1) {
                line(&mut s, f, 3, "");
            }
            if let Some(d) = dead.last() {
                line(&mut s, d, -1, "");
            }
            s.push_str("    Stop\n");
        } else {
            for (i, f) in dying.iter().enumerate() {
                let act = match i {
                    0 => "A_Scream",
                    1 => "A_NoBlocking",
                    _ => "",
                };
                line(&mut s, f, f.2, act);
            }
            if dying.len() == 1 {
                line(&mut s, &dying[0], 0, "A_NoBlocking");
            }
            let corpse = dead.last().unwrap_or(&dying[dying.len() - 1]);
            line(&mut s, corpse, -1, "");
            s.push_str("    Stop\n");
        }
        let pose = stand.first().or(walk.first()).cloned();
        if let Some(f) = pose {
            s.push_str("  Ice:\n");
            line(&mut s, &f, -1, "");
            s.push_str("    Stop\n  Shrink:\n");
            line(&mut s, &f, -1, "");
            s.push_str("    Stop\n");
        }
        s.push_str("  }\n}\n\n");
        self.out.decorate.push_str(&s);
        for (name, flags) in VARIANT_FLAGS {
            if !e.variants.contains(name) {
                continue;
            }
            let Some(t) = self.con.value(name).filter(|&t| t != base) else {
                continue;
            };
            let ednum = CLASS_BASE + t;
            self.things.insert(t, ednum);
            self.enemies.insert(ednum, e.size);
            let _ = write!(
                self.out.decorate,
                "actor {class}_{name} : {class} {ednum}\n{{\n  {flags}\n}}\n\n"
            );
        }
        if e.class == "DukeTrooper" && self.next_decor <= DECOR_LAST {
            let cap = self.next_decor;
            self.next_decor += 1;
            self.captain_ednum = Some(cap);
            self.captain_tiles.insert(base);
            for v in e.variants {
                if let Some(t) = self.con.value(v) {
                    self.captain_tiles.insert(t);
                }
            }
            self.enemies.insert(cap, e.size);
            let mut cap_s = format!(
                "actor DukeTrooperCaptain : DukeTrooper {cap}\n{{\n  States\n  {{\n  See:\n"
            );
            for f in &walk {
                let _ = writeln!(cap_s, "    {} {} {walk_tics} A_Chase", f.0, f.1);
            }
            cap_s.push_str(
                "    TNT1 A 0 A_Jump(24, \"Warp\")\n    Loop\n  Warp:\n    \
                 TNT1 A 0 A_RandomTeleport(192)\n    Goto See\n  }\n}\n\n",
            );
            self.out.decorate.push_str(&cap_s);
        } else if e.size.1 < 90 && self.next_decor <= DECOR_LAST {
            let mini = self.next_decor;
            self.next_decor += 1;
            self.miniboss.insert(base, mini);
            for v in e.variants {
                if let Some(t) = self.con.value(v) {
                    self.miniboss.insert(t, mini);
                }
            }
            self.enemies.insert(mini, e.size);
            let hp = strength.saturating_mul(2).max(1);
            let _ = write!(
                self.out.decorate,
                "actor {class}Mini : {class} {mini}\n{{\n  Health {hp}\n}}\n\n"
            );
        }
    }

    /// Security cameras and viewscreens: Use a screen to look through the
    /// nearest `DukeCamera` (`A_ChangeCameraToNearest`, reverts on move).
    fn cameras(&mut self) {
        if let Some(t) = self.tile("CAMERA1") {
            let frames = self.anim_frames("DukeCamera", t, 0);
            let still = frames
                .first()
                .map_or_else(|| "TNT1 A".to_owned(), |(n, l, _)| format!("{n} {l}"));
            let mut s = format!(
                "actor DukeCamera {CAMERA_EDNUM}\n{{\n  Health 1\n  Radius 8\n  Height 16\n  \
                 +SHOOTABLE\n  +NOGRAVITY\n  +NOBLOOD\n  +DONTTHRUST\n  States\n  {{\n  Spawn:\n"
            );
            if frames.is_empty() {
                s.push_str("    TNT1 A -1\n    Stop\n");
            } else {
                for (n, l, tics) in &frames {
                    let _ = writeln!(s, "    {n} {l} {}", (*tics).max(1));
                }
                s.push_str("    Loop\n");
            }
            let _ = writeln!(
                s,
                "  Death:\n    {still} 0 A_NoBlocking\n    Stop\n  }}\n}}\n\n"
            );
            self.out.decorate.push_str(&s);
        }
        if let Some(t) = self.tile("VIEWSCREEN") {
            self.out.sprites.named("VCAMA0", t, 0);
            let _ = write!(
                self.out.decorate,
                "actor DukeViewScreen {VIEWSCREEN_EDNUM}\n{{\n  Radius 16\n  Height 32\n  \
                 +NOGRAVITY\n  +USESPECIAL\n  States\n  {{\n  Spawn:\n    VCAM A -1\n    Stop\n  \
                 Use:\n    VCAM A 0 A_ChangeCameraToNearest(\"DukeCamera\", 1)\n    VCAM A 1\n    \
                 Goto Spawn\n  }}\n}}\n\n"
            );
        }
    }

    /// `SE 33`: scrap falls from the ceiling while a quake is felt.
    fn quake_debris(&mut self) {
        self.out.decorate.push_str(
            "actor DukeQuakeDebris 26991\n{\n  +NOBLOCKMAP\n  +NOGRAVITY\n  +NOCLIP\n  \
             +SPAWNCEILING\n  +DONTDRAW\n  States\n  {\n  Spawn:\n    \
             TNT1 A 8 A_JumpIfQuake(\"Drop\")\n    Loop\n  Drop:\n    \
             TNT1 A 1 A_SpawnItemEx(\"DukeScrap\", 0, 0, 0, frandom(-4, 4), \
             frandom(-4, 4), frandom(-3, -0.5))\n    Goto Spawn\n  }\n}\n\n",
        );
    }

    /// `RESPAWN`: invisible marker that spawns its lotag actor after a delay.
    fn respawn_marker(&mut self) {
        self.out.decorate.push_str(
            "actor DukeRespawn 26994\n{\n  +NOBLOCKMAP\n  +NOGRAVITY\n  +NOCLIP\n  \
             +DONTDRAW\n  States\n  {\n  Spawn:\n    TNT1 A 8 A_DukeRespawn\n    Loop\n  }\n}\n\n",
        );
    }

    /// `WATERBUBBLEMAKER`: invisible emitter that pops bubbles upward.
    fn water_bubbles(&mut self) {
        let frame = self.tile("WATERBUBBLE").map_or_else(
            || "TNT1 A".to_owned(),
            |t| {
                let (n, l) = self.out.sprites.frame("DukeWaterBubble", t, 1, 0);
                format!("{n} {l}")
            },
        );
        let _ = write!(
            self.out.decorate,
            "actor DukeWaterBubble\n{{\n  Radius 2\n  Height 2\n  Scale 0.4\n  +NOBLOCKMAP\n  \
             +NOGRAVITY\n  +NOTELEPORT\n  States\n  {{\n  Spawn:\n    {frame} 4\n    {frame} 4\n    \
             {frame} 4\n    {frame} 4\n    {frame} 4\n    {frame} 4\n    {frame} 4\n    Stop\n  \
             }}\n}}\n\n\
             actor DukeWaterBubbleMaker {BUBBLE_MAKER_EDNUM}\n{{\n  +NOBLOCKMAP\n  +NOGRAVITY\n  \
             +NOCLIP\n  +DONTDRAW\n  States\n  {{\n  Spawn:\n    TNT1 A 16 \
             A_SpawnItemEx(\"DukeWaterBubble\", frandom(-4, 4), frandom(-4, 4), 4, 0, 0, \
             frandom(0.5, 1.5))\n    Loop\n  }}\n}}\n\n"
        );
    }

    /// `NATURALLIGHTNING`: random map-wide flashes (`Light_ForceLightning`).
    fn natural_lightning(&mut self) {
        self.out.decorate.push_str(
            "actor DukeNaturalLightning 26996\n{\n  +NOBLOCKMAP\n  +NOGRAVITY\n  +NOCLIP\n  \
             +DONTDRAW\n  States\n  {\n  Spawn:\n    TNT1 A 70 A_Jump(96, \"Flash\")\n    Loop\n  \
             Flash:\n    TNT1 A 0 A_CallSpecial(109)\n    TNT1 A 35\n    Goto Spawn\n  }\n}\n\n",
        );
    }

    /// `FORCESPHERE`: a translucent ball that pulses (Overlord / force FX).
    fn force_sphere(&mut self) {
        let frame = self.tile("FORCESPHERE").map_or_else(
            || "TNT1 A".to_owned(),
            |t| {
                let (n, l) = self.out.sprites.frame("DukeForceSphere", t, 1, 0);
                format!("{n} {l}")
            },
        );
        let _ = write!(
            self.out.decorate,
            "actor DukeForceSphere {FORCE_SPHERE_EDNUM}\n{{\n  Radius 16\n  Height 32\n  \
             +NOBLOCKMAP\n  +NOGRAVITY\n  +NOCLIP\n  +NOTELEPORT\n  RenderStyle Translucent\n  \
             Alpha 0.5\n  States\n  {{\n  Spawn:\n    {frame} 8 A_SetScale(2)\n    {frame} 8 \
             A_SetScale(4)\n    {frame} 8 A_SetScale(2)\n    Loop\n  }}\n}}\n\n"
        );
    }

    /// `FRAMEEFFECT1`: a translucent afterimage that copies the nearest monster.
    fn frame_effect(&mut self) {
        self.out.decorate.push_str(
            "actor DukeFrameEffect 26998\n{\n  Radius 8\n  Height 16\n  +NOBLOCKMAP\n  \
             +NOGRAVITY\n  +NOCLIP\n  +NOTELEPORT\n  RenderStyle Translucent\n  Alpha 0.4\n  \
             States\n  {\n  Spawn:\n    TNT1 A 1 A_DukeFrameEffect\n    Loop\n  }\n}\n\n",
        );
    }

    /// `SE 36`: a sprite that fires a trooper laser every few seconds.
    fn shooter(&mut self) {
        self.out.decorate.push_str(
            "actor DukeShooter 26990\n{\n  +NOBLOCKMAP\n  +NOGRAVITY\n  +NOCLIP\n  \
             States\n  {\n  Spawn:\n    TNT1 A 175 A_CustomMissile(\"DukeLaser\", 32)\n    \
             Loop\n  }\n}\n\n",
        );
    }

    /// Barrels, cracks' explosives, force fields, eggs and babes.
    fn breakable(&mut self, sp: &Sprite) -> Option<i32> {
        let tile = i32::from(sp.picnum);
        let kind = prop_kind(self.con, tile)?;
        let key = (tile, sp.pal, 0x8000 | kind);
        if let Some(&e) = self.decor.get(&key) {
            return Some(e);
        }
        if self.next_decor > DECOR_LAST {
            return None;
        }
        let t = self.tiles.get(tile)?;
        let ednum = self.next_decor;
        self.next_decor += 1;
        self.decor.insert(key, ednum);
        let class = if kind == PROP_BABE {
            format!("DukeBabe{ednum}")
        } else {
            format!("DukeProp{ednum}")
        };
        let frames = self.anim_frames(&class, tile, sp.pal);
        // A gas can has no CON actor. Duke blows it up on the first hit.
        // Health 10 survived a pistol shot (strength 6) and the knockback,
        // with `+NOGRAVITY`, left it hanging in the air until the next shot.
        let gas_can = [
            "CANWITHSOMETHING",
            "CANWITHSOMETHING2",
            "CANWITHSOMETHING3",
            "CANWITHSOMETHING4",
        ]
        .iter()
        .any(|n| self.con.value(n) == Some(tile));
        let health = if gas_can {
            1
        } else {
            match kind {
                PROP_FORCE => 40,
                PROP_BABE | PROP_EGG => 1,
                _ => 10,
            }
        };
        let solid = sp.cstat & 1 != 0 && kind != PROP_BABE && kind != PROP_EGG;
        let mut s = format!(
            "actor {class} {ednum}\n{{\n  Health {health}\n  Radius {}\n  Height {}\n  \
             +SHOOTABLE\n  +NOBLOOD\n  +NOGRAVITY\n",
            (i32::from(t.width) / 4).clamp(8, 32),
            (i32::from(t.height) / 2).clamp(8, 64)
        );
        if solid {
            s.push_str("  +SOLID\n");
        }
        if kind == PROP_FORCE {
            s.push_str("  ContactDamage 8\n");
        }
        if gas_can {
            s.push_str("  +DONTTHRUST\n");
        }
        if kind == PROP_BABE {
            s.push_str("  +USESPECIAL\n");
        }
        s.push_str("  States\n  {\n  Spawn:\n");
        if kind == PROP_EGG {
            if frames.is_empty() {
                s.push_str("    TNT1 A 35\n");
            } else {
                for (n, l, tics) in &frames {
                    let _ = writeln!(s, "    {n} {l} {}", (*tics).max(5));
                }
            }
            s.push_str(
                "    TNT1 A 0 A_Jump(32, \"Hatch\")\n    Loop\n  Hatch:\n    \
                 TNT1 A 0 A_SpawnItem(\"DukeSlimer\")\n    Stop\n  Death:\n    \
                 TNT1 A 0 A_SpawnItem(\"DukeSlimer\")\n    Stop\n",
            );
        } else {
            push_loop(&mut s, &frames);
            if kind == PROP_BABE {
                self.babe_use_and_death(&mut s, &class, tile, sp.pal, &frames);
            } else {
                let charge = ["SEENINE", "OOZFILTER"]
                    .iter()
                    .any(|n| self.con.value(n) == Some(tile));
                let still = frames
                    .first()
                    .map_or_else(|| "TNT1 A".to_owned(), |(n, l, _)| format!("{n} {l}"));
                if charge {
                    // Lit by its group (`arg2` = fuse tics), or shot: it leaves
                    // the group and blinks ~9 Duke tics before blowing.
                    let _ = write!(
                        s,
                        "  Fuse:\n    {still} 1\n    {still} 1 A_SetTics(args[2])\n    \
                         {still} 0 A_Die\n    {still} -1\n    Stop\n  Death:\n    \
                         {still} 0 A_SetTid(0)\n    {still} {CHARGE_BLINK} Bright\n"
                    );
                } else {
                    s.push_str("  Death:\n");
                }
                if kind == PROP_BOOM {
                    self.blast(&mut s);
                } else if let Some(f) = frames.first() {
                    let _ = writeln!(s, "    {} {} 1", f.0, f.1);
                } else {
                    s.push_str("    TNT1 A 1\n");
                }
                s.push_str("    Stop\n");
            }
        }
        s.push_str("  }\n}\n\n");
        self.out.decorate.push_str(&s);
        Some(ednum)
    }

    /// CON `tipme` / `femcode`: tip lines, money, leftover pictures, scream.
    fn babe_use_and_death(
        &mut self,
        s: &mut String,
        class: &str,
        tile: i32,
        pal: u8,
        frames: &[(String, char, i32)],
    ) {
        let still = frames
            .first()
            .map_or_else(|| "TNT1 A".to_owned(), |(n, l, _)| format!("{n} {l}"));
        let tip1 = self.sound("DUKE_TIP1");
        let tip2 = self.sound("DUKE_TIP2");
        let hit1 = self.sound("DUKE_HIT_STRIPPER1");
        let hit2 = self.sound("DUKE_HIT_STRIPPER2");
        let scream = self.sound("LADY_SCREAM");
        let _ = write!(
            s,
            "  Use:\n    {still} 0 A_JumpIfInTargetInventory(\"DukeMoney\", 1, \"Tip\")\n    \
             {still} 1 A_Print(\"She wants a tip.\")\n    Goto Spawn\n  Tip:\n    \
             {still} 0 A_TakeFromTarget(\"DukeMoney\", 1)\n"
        );
        if tip1.is_some() && tip2.is_some() {
            s.push_str(&format!("    {still} 0 A_Jump(128, \"Tip2\")\n"));
        }
        if let Some(tip) = &tip1 {
            let _ = writeln!(s, "    {still} 0 A_PlaySound(\"{tip}\")");
        }
        if tip2.is_some() {
            s.push_str("    Goto Tipped\n  Tip2:\n");
            if let Some(tip) = &tip2 {
                let _ = writeln!(s, "    {still} 0 A_PlaySound(\"{tip}\")");
            }
        }
        let _ = writeln!(
            s,
            "  Tipped:\n    {still} 1 A_Print(\"You tip her.\")\n    Goto Spawn"
        );
        let leftover = ["FEM2", "FEM5", "FEM6", "FEM8"]
            .into_iter()
            .find(|n| self.con.value(n) == Some(tile))
            .and_then(|n| match n {
                "FEM2" => self.tile("BARBROKE"),
                "FEM5" | "FEM8" => self.tile("BLOODYPOLE"),
                "FEM6" => self.tile("FEM6PAD"),
                _ => None,
            });
        let remain = leftover.map(|t| {
            let (n, l) = self.out.sprites.frame(class, t, 1, pal);
            format!("{n} {l}")
        });
        s.push_str("  Death:\n");
        if hit1.is_some() && hit2.is_some() {
            let _ = writeln!(s, "    {still} 0 A_Jump(128, \"Hit2\")");
        }
        if let Some(hit) = &hit1 {
            let _ = writeln!(s, "    {still} 0 A_PlaySound(\"{hit}\")");
        }
        if hit2.is_some() {
            s.push_str("    Goto Die\n  Hit2:\n");
            if let Some(hit) = &hit2 {
                let _ = writeln!(s, "    {still} 0 A_PlaySound(\"{hit}\")");
            }
        }
        s.push_str("  Die:\n    TNT1 A 0 A_NoBlocking\n");
        let bills = if self.con.value("FEM1") == Some(tile) {
            5
        } else if self.con.value("FEM2") == Some(tile) {
            7
        } else if self.con.value("FEM3") == Some(tile) {
            4
        } else if self.con.value("FEM7") == Some(tile) {
            8
        } else {
            0
        };
        scatter(s, "TNT1 A", "DukeMoney", bills, 4);
        scatter(s, "TNT1 A", "DukeScrap", 4, 8);
        if let Some(remain) = remain {
            let _ = writeln!(s, "    {remain} -1\n    Stop");
        } else {
            if let Some(scream) = scream {
                let _ = writeln!(s, "    TNT1 A 0 A_PlaySound(\"{scream}\")");
            }
            if ["NAKED1", "PODFEM1"]
                .iter()
                .any(|n| self.con.value(n) == Some(tile))
            {
                scatter(s, "TNT1 A", "DukeScrap", 14, 8);
            }
            s.push_str("    TNT1 A 1\n    Stop\n");
        }
    }

    /// Explosion of a barrel or charge: sound, scrap, blast.
    fn blast(&mut self, s: &mut String) {
        if let Some(sfx) = self.sound("PIPEBOMB_EXPLODE") {
            let _ = writeln!(s, "    TNT1 A 0 A_PlaySound(\"{sfx}\")");
        }
        scatter(s, "TNT1 A", "DukeScrap", 8, 16);
        let explosion = self.explosion();
        if explosion.is_empty() {
            s.push_str("    TNT1 A 2 A_Explode(64, 128)\n");
        }
        for (i, (n, l, _)) in explosion.iter().enumerate() {
            let act = if i == 0 { " A_Explode(64, 128)" } else { "" };
            let _ = writeln!(s, "    {n} {l} 2 Bright{act}");
        }
    }

    /// An invisible charge (`xrepeat` ≤ [`HIDDEN_CHARGE_XREPEAT`]): only its
    /// group's fuse sets it off.
    fn hidden_charge(&mut self) -> Option<i32> {
        if let Some(e) = self.hidden_charge {
            return Some(e);
        }
        if self.next_decor > DECOR_LAST {
            return None;
        }
        let ednum = self.next_decor;
        self.next_decor += 1;
        self.hidden_charge = Some(ednum);
        let mut s = format!(
            "actor DukeHiddenCharge {ednum}\n{{\n  +NOBLOCKMAP\n  +NOGRAVITY\n  States\n  {{\n  \
             Spawn:\n    TNT1 A -1\n    Stop\n  Fuse:\n    TNT1 A 1\n    \
             TNT1 A 1 A_SetTics(args[2])\n    TNT1 A 0 A_SetTid(0)\n"
        );
        self.blast(&mut s);
        s.push_str("    Stop\n  }\n}\n\n");
        self.out.decorate.push_str(&s);
        Some(ednum)
    }

    /// Dormant spot whose `Active` state plays a wall break's sounds and
    /// throws its glass.
    fn wall_break_spot(&mut self, kind: WallKind) -> Option<i32> {
        if let Some(&e) = self.wall_breaks.get(&kind) {
            return Some(e);
        }
        if self.next_decor > DECOR_LAST {
            return None;
        }
        let ednum = self.next_decor;
        self.next_decor += 1;
        self.wall_breaks.insert(kind, ednum);
        let (sounds, glass): (&[&str], usize) = match kind {
            WallKind::Glass => (&["GLASS_BREAKING"], 10),
            WallKind::Stained => (&["VENT_BUST", "GLASS_BREAKING"], 16),
            WallKind::Fan => (&["VENT_BUST", "GLASS_BREAKING"], 0),
            WallKind::Machine => (&["VENT_BUST", "GLASS_HEAVYBREAK"], 10),
            WallKind::Screen | WallKind::Light => (&["GLASS_HEAVYBREAK"], 16),
            WallKind::Atm => (&["GLASS_HEAVYBREAK"], 8),
            WallKind::Force => (&["GLASS_HEAVYBREAK"], 8),
        };
        let money = matches!(kind, WallKind::Atm);
        let mut s = format!(
            "actor DukeWallBreak{ednum} {ednum}\n{{\n  +NOBLOCKMAP\n  +NOGRAVITY\n  States\n  \
             {{\n  Spawn:\n    TNT1 A -1\n    Stop\n  Active:\n"
        );
        for define in sounds {
            if let Some(sfx) = self.sound(define) {
                let _ = writeln!(s, "    TNT1 A 0 A_PlaySound(\"{sfx}\")");
            }
        }
        scatter(&mut s, "TNT1 A", "DukeGlassPiece", glass, 0);
        if money {
            scatter(&mut s, "TNT1 A", "DukeMoney", 16, 0);
        }
        s.push_str("    TNT1 A 1\n    Stop\n  }\n}\n\n");
        self.out.decorate.push_str(&s);
        Some(ednum)
    }

    /// Raze `DukeGenericDestructible` & co. ([`BREAKABLES`]): breaks on the
    /// first hit into its broken picture (or vanishes) with a sound and debris.
    fn destructible(&mut self, sp: &Sprite) -> Option<i32> {
        let tile = i32::from(sp.picnum);
        let &(_, broken, sound, debris) = BREAKABLES
            .iter()
            .find(|(name, ..)| self.con.value(name) == Some(tile))?;
        let orient = sp.cstat & 48;
        let translucent = sp.cstat & (2 | 512);
        let key = (tile, sp.pal, 0x2000 | orient | translucent);
        if let Some(&e) = self.decor.get(&key) {
            return Some(e);
        }
        if self.next_decor > DECOR_LAST {
            return None;
        }
        let t = self.tiles.get(tile)?;
        let (w, h) = (i32::from(t.width), i32::from(t.height));
        let ednum = self.next_decor;
        self.next_decor += 1;
        self.decor.insert(key, ednum);
        let class = format!("DukeBreak{ednum}");
        let frames = self.anim_frames(&class, tile, sp.pal);
        let broken = self
            .tile(broken)
            .map(|b| self.out.sprites.frame(&class, b, 1, sp.pal))
            .map_or_else(|| "TNT1 A".to_owned(), |(n, l)| format!("{n} {l}"));
        // A wall sprite blocks across its width; Room's actors are cylinders.
        let (radius, height) = match orient {
            16 => ((w / 2).clamp(4, 32), h.clamp(8, 128)),
            32 => ((w / 4).clamp(4, 48), 4),
            _ => ((w / 4).clamp(4, 48), (h / 2).clamp(8, 128)),
        };
        let health = if debris == Debris::Blast { 30 } else { 1 };
        let mut s = format!(
            "actor {class} {ednum}\n{{\n  Health {health}\n  Radius {radius}\n  Height {height}\n  \
             +SHOOTABLE\n  +NOBLOOD\n  +NOGRAVITY\n  +DONTTHRUST\n  +SOLID\n"
        );
        match orient {
            16 => s.push_str("  +WALLSPRITE\n"),
            32 => s.push_str("  +FLATSPRITE\n"),
            _ => {}
        }
        if translucent != 0 {
            let alpha = if sp.cstat & 512 != 0 { "0.33" } else { "0.66" };
            let _ = writeln!(s, "  RenderStyle Translucent\n  Alpha {alpha}");
        }
        if let Some(sfx) = self.sound(sound) {
            let _ = writeln!(s, "  DeathSound \"{sfx}\"");
        }
        s.push_str("  States\n  {\n  Spawn:\n");
        push_loop(&mut s, &frames);
        let _ = writeln!(
            s,
            "  Death:\n    {broken} 0 A_NoBlocking\n    {broken} 0 A_Scream"
        );
        let mid = (height / 2).max(2);
        match debris {
            Debris::None => {}
            Debris::Scrap => scatter(&mut s, &broken, "DukeScrap", 16, mid),
            Debris::Glass => scatter(&mut s, &broken, "DukeGlassPiece", 10, mid),
            Debris::Smoke => {
                let _ = writeln!(
                    s,
                    "    {broken} 0 A_SpawnItemEx(\"DukeSmallSmoke\", 0, 0, {mid})"
                );
            }
            Debris::Blast => {
                let _ = writeln!(
                    s,
                    "    TNT1 A 0 A_SpawnItemEx(\"DukeCarFire\", 0, 0, {mid})\n    \
                     TNT1 A 0 A_Explode(30, 64)"
                );
                scatter(&mut s, "TNT1 A", "DukeScrap", 15, mid);
            }
        }
        if broken == "TNT1 A" {
            s.push_str("    TNT1 A 1\n    Stop\n");
        } else {
            let _ = writeln!(s, "    {broken} -1\n    Stop");
        }
        s.push_str("  }\n}\n\n");
        self.out.decorate.push_str(&s);
        Some(ednum)
    }

    /// `DukeCarFire`: a bare `EXPLOSION2` burst (no damage).
    fn fire(&mut self) {
        let mut fire = String::from(
            "actor DukeCarFire\n{\n  +NOBLOCKMAP\n  +NOGRAVITY\n  States\n  {\n  Spawn:\n",
        );
        let boom = self.explosion();
        if boom.is_empty() {
            fire.push_str("    TNT1 A 1\n");
        }
        for (bn, bl, tics) in &boom {
            let _ = writeln!(fire, "    {bn} {bl} {tics} Bright");
        }
        fire.push_str("    Stop\n  }\n}\n\n");
        self.out.decorate.push_str(&fire);
    }

    /// Flying debris, ejected shells and smoke puffs (Raze `DukeScrap`,
    /// `DukeGlassPieces`, `DukeShell`, `DukeSmallSmoke`).
    fn debris(&mut self) {
        let frames = |d: &mut Self, class: &str, name: &str, n: i32| -> Vec<String> {
            d.tile(name).map_or_else(Vec::new, |t| {
                (t..t + n)
                    .filter(|&f| d.tiles.get(f).is_some())
                    .map(|f| {
                        let (n, l) = d.out.sprites.frame(class, f, 1, 0);
                        format!("{n} {l}")
                    })
                    .collect()
            })
        };
        // Falls until it lands, then goes.
        let faller = |s: &mut String, frames: &[String], tics: i32, rest: i32| {
            for f in frames {
                let _ = writeln!(s, "    {f} {tics} A_CheckFloor(\"Land\")");
            }
            s.push_str("    Loop\n  Land:\n");
            if let Some(f) = frames.first() {
                let _ = writeln!(s, "    {f} {rest}");
            }
            s.push_str("    Stop\n  }\n}\n\n");
        };
        let mut scrap = Vec::new();
        for name in ["SCRAP1", "SCRAP2", "SCRAP3", "SCRAP4", "SCRAP5", "SCRAP6"] {
            scrap.extend(frames(self, "DukeScrap", name, 1));
        }
        let mut s = String::from(
            "actor DukeScrap\n{\n  Radius 2\n  Height 2\n  Scale 0.6\n  +NOBLOCKMAP\n  \
             +DROPOFF\n  +NOTELEPORT\n  States\n  {\n  Spawn:\n",
        );
        if scrap.is_empty() {
            scrap.push("TNT1 A".into());
        }
        faller(&mut s, &scrap, 3, 1);
        self.out.decorate.push_str(&s);

        let glass = frames(self, "DukeGlassPiece", "GLASSPIECES", 3);
        let mut s = String::from(
            "actor DukeGlassPiece\n{\n  Radius 2\n  Height 2\n  Scale 0.5\n  BounceFactor 0.5\n  \
             +NOBLOCKMAP\n  +DROPOFF\n  +NOTELEPORT\n  +BOUNCEONFLOORS\n  States\n  {\n  Spawn:\n",
        );
        for _ in 0..4 {
            for f in &glass {
                let _ = writeln!(s, "    {f} 3");
            }
        }
        s.push_str("    TNT1 A 1\n    Stop\n  }\n}\n\n");
        self.out.decorate.push_str(&s);

        // Raze `lotsofmoney`: paper that flies out of a shot ATM, then a pickup.
        let cash = frames(self, "DukeMoney", "MONEY", 4);
        let rest = cash.first().cloned().unwrap_or_else(|| "TNT1 A".to_owned());
        let mut s = String::from(
            "actor DukeMoney\n{\n  Radius 8\n  Height 8\n  Scale 0.4\n  BounceFactor 0.4\n  \
             Inventory.Amount 1\n  Inventory.MaxAmount 999\n  +COUNTITEM\n  \
             +DROPOFF\n  +NOTELEPORT\n  +BOUNCEONFLOORS\n  States\n  {\n  Spawn:\n",
        );
        if cash.is_empty() {
            s.push_str("    TNT1 A -1\n    Stop\n  }\n}\n\n");
        } else {
            for _ in 0..6 {
                for f in &cash {
                    let _ = writeln!(s, "    {f} 3");
                }
            }
            let _ = writeln!(s, "    {rest} -1\n    Stop\n  }}\n}}\n\n");
        }
        self.out.decorate.push_str(&s);

        let smoke = self
            .tile("SMALLSMOKE")
            .map_or_else(Vec::new, |t| self.anim_frames("DukeSmallSmoke", t, 0));
        let mut s = String::from(
            "actor DukeSmallSmoke\n{\n  Scale 0.4\n  +NOBLOCKMAP\n  +NOGRAVITY\n  \
             RenderStyle Translucent\n  Alpha 0.66\n  States\n  {\n  Spawn:\n",
        );
        for (n, l, _) in &smoke {
            let _ = writeln!(s, "    {n} {l} 4 A_ChangeVelocity(0, 0, 1, 2)");
        }
        s.push_str("    TNT1 A 1\n    Stop\n  }\n}\n\n");
        self.out.decorate.push_str(&s);

        for (class, name, n) in [
            ("DukeShell", "SHELL", 2),
            ("DukeShotgunShell", "SHOTGUNSHELL", 1),
        ] {
            let mut f = frames(self, class, name, n);
            if f.is_empty() {
                f.push("TNT1 A".into());
            }
            let mut s = format!(
                "actor {class}\n{{\n  Radius 1\n  Height 1\n  Scale 0.1\n  +NOBLOCKMAP\n  \
                 +DROPOFF\n  +NOTELEPORT\n  States\n  {{\n  Spawn:\n"
            );
            faller(&mut s, &f, 2, 35);
            self.out.decorate.push_str(&s);
        }
    }

    fn decoration(&mut self, sp: &Sprite) -> Option<i32> {
        let tile = i32::from(sp.picnum);
        let t = self.tiles.get(tile)?;
        let (w, h) = (i32::from(t.width), i32::from(t.height));
        let orient = sp.cstat & 48;
        let solid = sp.cstat & 1 != 0 && orient == 0;
        let translucent = sp.cstat & (2 | 512);
        let variant = orient | u16::from(solid) | translucent;
        if let Some(&e) = self.decor.get(&(tile, sp.pal, variant)) {
            return Some(e);
        }
        if self.next_decor > DECOR_LAST {
            return None;
        }
        let ednum = self.next_decor;
        self.next_decor += 1;
        self.decor.insert((tile, sp.pal, variant), ednum);
        let fixture = fixture_kind(self.con, tile);
        let class = match fixture {
            Some(Fixture::Fountain) => format!("DukeFountain{ednum}"),
            Some(Fixture::Toilet) => format!("DukeToilet{ednum}"),
            Some(Fixture::Stall) => format!("DukeStall{ednum}"),
            None => format!("DukeDecor{ednum}"),
        };
        let frames = self.actor_frames(&class, tile, sp.pal);
        let mut s = format!(
            "actor {class} {ednum}\n{{\n  Radius {}\n  Height {}\n  +NOGRAVITY\n",
            (w / 4).clamp(4, 48),
            (h / 2).clamp(8, 128)
        );
        s.push_str(if solid {
            "  +SOLID\n"
        } else {
            "  +NOBLOCKMAP\n"
        });
        match orient {
            16 => s.push_str("  +WALLSPRITE\n"),
            32 => s.push_str("  +FLATSPRITE\n"),
            _ => {}
        }
        if translucent != 0 {
            let alpha = if sp.cstat & 512 != 0 { "0.33" } else { "0.66" };
            let _ = writeln!(s, "  RenderStyle Translucent\n  Alpha {alpha}");
        } else if self.flame_actor(tile) {
            s.push_str("  RenderStyle Add\n  Alpha 0.85\n");
        }
        if fixture.is_some() {
            s.push_str("  Health 1\n  +USESPECIAL\n  +SHOOTABLE\n  +NOBLOOD\n  +DONTTHRUST\n");
        }
        s.push_str("  States\n  {\n  Spawn:\n");
        push_loop(&mut s, &frames);
        if let Some(kind) = fixture {
            let still = frames
                .first()
                .map_or_else(|| "TNT1 A".to_owned(), |(n, l, _)| format!("{n} {l}"));
            let sfx = match kind {
                Fixture::Fountain => self
                    .sound("DUKE_DRINKING")
                    .or_else(|| self.sound("WATER_GURGLE")),
                Fixture::Toilet | Fixture::Stall => self.sound("FLUSH_TOILET"),
            };
            s.push_str("  Use:\n");
            if let Some(sfx) = sfx {
                let _ = writeln!(s, "    {still} 0 A_PlaySound(\"{sfx}\")");
            }
            if kind == Fixture::Fountain {
                let cap = self.con.value("MAXWATERFOUNTAINHEALTH").unwrap_or(50);
                let _ = writeln!(s, "    {still} 0 A_HealTarget(1, {cap})");
            }
            let _ = writeln!(s, "    {still} 1\n    Goto Spawn");
            let broken_tile = match kind {
                Fixture::Fountain => "WATERFOUNTAINBROKE",
                Fixture::Toilet => "TOILETBROKE",
                Fixture::Stall => "STALLBROKE",
            };
            let broken = self
                .tile(broken_tile)
                .map(|b| self.out.sprites.frame(&class, b, 1, sp.pal))
                .map_or_else(|| "TNT1 A".to_owned(), |(n, l)| format!("{n} {l}"));
            let _ = writeln!(s, "  Death:\n    {broken} 0 A_NoBlocking");
            if kind != Fixture::Fountain {
                if let Some(glass) = self.sound("GLASS_HEAVYBREAK") {
                    let _ = writeln!(s, "    {broken} 0 A_PlaySound(\"{glass}\")");
                }
                scatter(&mut s, &broken, "DukeGlassPiece", 10, (h / 2).max(2));
            }
            if broken == "TNT1 A" {
                s.push_str("    TNT1 A 1\n    Stop\n");
            } else {
                let _ = writeln!(s, "    {broken} -1\n    Stop");
            }
        }
        s.push_str("  }\n}\n\n");
        self.out.decorate.push_str(&s);
        Some(ednum)
    }

    /// `DUKECAR` / `HELECOPT`: the burning wreck of E1L1's opening flies
    /// forward while sinking, trailing explosions, then blows up.
    fn crashing_car(&mut self, sp: &Sprite) -> Option<i32> {
        let tile = i32::from(sp.picnum);
        let key = (tile, 0, 0x4000);
        if let Some(&e) = self.decor.get(&key) {
            return Some(e);
        }
        if self.next_decor > DECOR_LAST {
            return None;
        }
        self.tiles.get(tile)?;
        let ednum = self.next_decor;
        self.next_decor += 1;
        self.decor.insert(key, ednum);
        let class = format!("DukeCar{ednum}");
        let (n, l) = self.out.sprites.frame(&class, tile, 1, 0);
        let ambience = self.sound("WAR_AMBIENCE2");
        let blast = self.sound("RPG_EXPLODE");
        // Duke: 292 units forward and 360 z units down per 30 Hz tic.
        let fly = "A_ChangeVelocity(15.6, 0, -1.2, 3)";
        let mut s = format!(
            "actor {class} {ednum}\n{{\n  Radius 16\n  Height 32\n  +NOBLOCKMAP\n  \
             +NOGRAVITY\n  +DROPOFF\n  States\n  {{\n  Spawn:\n    {n} {l} 1 {fly}\n"
        );
        if let Some(a) = &ambience {
            let _ = writeln!(s, "    {n} {l} 4 A_PlaySound(\"{a}\")");
        }
        for _ in 0..CAR_LIFE / 5 {
            let _ = writeln!(
                s,
                "    {n} {l} 0 {fly}\n    {n} {l} 5 A_SpawnItemEx(\"DukeCarFire\")"
            );
        }
        if let Some(b) = &blast {
            let _ = writeln!(s, "    {n} {l} 0 A_PlaySound(\"{b}\")");
        }
        let _ = write!(
            s,
            "    {n} {l} 0 A_Quake(4, 16, 0, 2048)\n    \
             TNT1 A 0 A_SpawnItemEx(\"DukeCarFire\", 0, 0, 8)\n    \
             TNT1 A 0 A_SpawnItemEx(\"DukeCarFire\", 16, 16, 24)\n    \
             TNT1 A 0 A_SpawnItemEx(\"DukeCarFire\", -16, -16, 0)\n    Stop\n  }}\n}}\n\n"
        );
        self.out.decorate.push_str(&s);
        Some(ednum)
    }

    /// Looping `MUSICANDSFX` sound: replayed every clip length.
    fn ambient(&mut self, sp: &Sprite) -> Option<i32> {
        let id = i32::from(sp.lotag);
        if let Some(&e) = self.ambient.get(&id) {
            return Some(e);
        }
        let def = self
            .con
            .sounds
            .get(&id)
            .filter(|d| d.kind & 1 != 0)?
            .clone();
        if self.next_decor > DECOR_LAST {
            return None;
        }
        let logical = self.sound(&def.name)?;
        let tics = self
            .grp
            .get(&def.file)
            .and_then(|b| room_formats::sound::decode_sound(b).ok())
            .filter(|d| d.rate > 0 && d.channels > 0)
            .map_or(70, |d| {
                let frames = d.samples.len() as u64 / u64::from(d.channels);
                (frames * 35).div_ceil(u64::from(d.rate)) as i32
            })
            .clamp(35, 35 * 60);
        let ednum = self.next_decor;
        self.next_decor += 1;
        self.ambient.insert(id, ednum);
        let _ = write!(
            self.out.decorate,
            "actor DukeAmbient{id} {ednum}\n{{\n  +NOBLOCKMAP\n  +NOGRAVITY\n  States\n  {{\n  \
             Spawn:\n    TNT1 A 1\n  Idle:\n    TNT1 A {tics} A_PlaySound(\"{logical}\")\n    \
             Loop\n  }}\n}}\n\n"
        );
        Some(ednum)
    }
}

/// Kinds of breakable wall pictures (Raze `checkhitwall`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum WallKind {
    Glass,
    Stained,
    Fan,
    Machine,
    Screen,
    Atm,
    Light,
    Force,
}

/// Doom tics a shot C-9 blinks before blowing (Raze: 3 picture steps of 3).
const CHARGE_BLINK: i32 = 10;

/// What a breakable sprite throws when it breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Debris {
    None,
    Scrap,
    Glass,
    Smoke,
    /// Blows up (explosion, scrap); explosives only in Duke.
    Blast,
}

/// Raze's destructible sprites: tile, broken tile ("" = gone), sound, debris.
const BREAKABLES: &[(&str, &str, &str, Debris)] = &[
    (
        "FANSPRITE",
        "FANSPRITEBROKE",
        "GLASS_HEAVYBREAK",
        Debris::Scrap,
    ),
    ("GRATE1", "BGRATE1", "VENT_BUST", Debris::None),
    (
        "CIRCLEPANNEL",
        "CIRCLEPANNELBROKE",
        "VENT_BUST",
        Debris::None,
    ),
    ("PANNEL1", "BPANNEL1", "VENT_BUST", Debris::None),
    ("PANNEL2", "BPANNEL3", "VENT_BUST", Debris::None),
    ("PANNEL3", "BPANNEL3", "VENT_BUST", Debris::None),
    ("CHAIR1", "BROKENCHAIR", "", Debris::None),
    ("CHAIR2", "BROKENCHAIR", "", Debris::None),
    ("MOVIECAMERA", "", "GLASS_HEAVYBREAK", Debris::Scrap),
    ("SCALE", "", "GLASS_HEAVYBREAK", Debris::Scrap),
    ("CAMERALIGHT", "", "GLASS_HEAVYBREAK", Debris::Scrap),
    ("IVUNIT", "", "GLASS_HEAVYBREAK", Debris::Scrap),
    ("POT1", "", "GLASS_HEAVYBREAK", Debris::Scrap),
    ("POT2", "", "GLASS_HEAVYBREAK", Debris::Scrap),
    ("POT3", "", "GLASS_HEAVYBREAK", Debris::Scrap),
    ("TRIPODCAMERA", "", "GLASS_HEAVYBREAK", Debris::Scrap),
    ("VACUUM", "", "GLASS_HEAVYBREAK", Debris::Scrap),
    ("HANGLIGHT", "", "GLASS_HEAVYBREAK", Debris::Scrap),
    ("GENERICPOLE2", "", "GLASS_HEAVYBREAK", Debris::Scrap),
    ("CHAIR3", "", "GLASS_HEAVYBREAK", Debris::Scrap),
    ("OCEANSPRITE1", "", "", Debris::Smoke),
    ("OCEANSPRITE2", "", "", Debris::Smoke),
    ("OCEANSPRITE3", "", "", Debris::Smoke),
    ("OCEANSPRITE4", "", "", Debris::Smoke),
    ("OCEANSPRITE5", "", "", Debris::Smoke),
    ("PIPE1", "PIPE1B", "", Debris::Smoke),
    ("PIPE2", "PIPE2B", "", Debris::Smoke),
    ("PIPE3", "PIPE3B", "", Debris::Smoke),
    ("PIPE4", "PIPE4B", "", Debris::Smoke),
    ("PIPE5", "PIPE5B", "", Debris::Smoke),
    ("PIPE6", "PIPE6B", "", Debris::Smoke),
    ("BOTTLE7", "", "GLASS_BREAKING", Debris::Glass),
    ("FETUS", "FETUSBROKE", "GLASS_BREAKING", Debris::Glass),
    (
        "HYDROPLANT",
        "BROKEHYDROPLANT",
        "GLASS_BREAKING",
        Debris::Glass,
    ),
    (
        "HYDRENT",
        "BROKEFIREHYDRENT",
        "GLASS_HEAVYBREAK",
        Debris::None,
    ),
    ("VASE", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("BOTTLE10", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("DOMELITE", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("BOTTLE1", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("BOTTLE2", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("BOTTLE3", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("BOTTLE4", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("BOTTLE5", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("BOTTLE6", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("BOTTLE8", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("BOTTLE11", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("BOTTLE12", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("BOTTLE13", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("BOTTLE14", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("BOTTLE15", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("BOTTLE16", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("BOTTLE17", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("BOTTLE18", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("BOTTLE19", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("WAITTOBESEATED", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("SUSHIPLATE1", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("SUSHIPLATE2", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("SUSHIPLATE3", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("SUSHIPLATE4", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("SUSHIPLATE5", "", "GLASS_HEAVYBREAK", Debris::Glass),
    ("SATELLITE", "", "PIPEBOMB_EXPLODE", Debris::Blast),
    ("FUELPOD", "", "PIPEBOMB_EXPLODE", Debris::Blast),
    ("SOLARPANNEL", "", "PIPEBOMB_EXPLODE", Debris::Blast),
    ("ANTENNA", "", "PIPEBOMB_EXPLODE", Debris::Blast),
];

/// `n` `A_SpawnItemEx` lines (on `frame`) throwing `class` out at random
/// (Raze `spawnguts` / `lotsofglass`: random direction, speed and lift).
fn scatter(s: &mut String, frame: &str, class: &str, n: usize, z: i32) {
    for _ in 0..n {
        let _ = writeln!(
            s,
            "    {frame} 0 A_SpawnItemEx(\"{class}\", 0, 0, {z}, frandom(-5, 5), \
             frandom(-5, 5), frandom(1, 6))"
        );
    }
}

/// Spawn frames: `-1` for a still picture, else an animation loop.
const PROP_BOOM: u16 = 1;
const PROP_EGG: u16 = 2;
const PROP_FORCE: u16 = 3;
const PROP_BABE: u16 = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fixture {
    Fountain,
    Toilet,
    Stall,
}

fn fixture_kind(con: &ConData, tile: i32) -> Option<Fixture> {
    if con.value("WATERFOUNTAIN") == Some(tile) {
        Some(Fixture::Fountain)
    } else if con.value("TOILET") == Some(tile) {
        Some(Fixture::Toilet)
    } else if con.value("STALL") == Some(tile) {
        Some(Fixture::Stall)
    } else {
        None
    }
}

fn prop_kind(con: &ConData, tile: i32) -> Option<u16> {
    const BOOM: &[&str] = &[
        "EXPLODINGBARREL",
        "CANWITHSOMETHING",
        "FIREEXT",
        "SEENINE",
        "OOZFILTER",
    ];
    const FORCE: &[&str] = &["W_FORCEFIELD", "FORCERIPPLE"];
    const BABE: &[&str] = &[
        "FEM1", "FEM2", "FEM3", "FEM4", "FEM5", "FEM6", "FEM7", "FEM8", "FEM9", "FEM10", "NAKED1",
        "TOUGHGAL", "PODFEM1",
    ];
    let is = |names: &[&str]| names.iter().any(|n| con.value(n) == Some(tile));
    if con.value("EGG") == Some(tile) {
        Some(PROP_EGG)
    } else if is(BOOM) {
        Some(PROP_BOOM)
    } else if is(FORCE) {
        Some(PROP_FORCE)
    } else if is(BABE) {
        Some(PROP_BABE)
    } else {
        None
    }
}

fn push_loop(s: &mut String, frames: &[(String, char, i32)]) {
    match frames {
        [] => s.push_str("    TNT1 A -1\n    Stop\n"),
        [(n, l, _)] => {
            let _ = writeln!(s, "    {n} {l} -1\n    Stop");
        }
        _ => {
            for (n, l, tics) in frames {
                let _ = writeln!(s, "    {n} {l} {tics}");
            }
            s.push_str("    Loop\n");
        }
    }
}

impl Duke<'_> {
    /// `:DukeSnd<n>` playing CON sound number `n`, if it exists.
    fn sound_seq(&mut self, n: i16) -> Option<String> {
        let name = format!("DukeSnd{n}");
        if self.seq_sounds.contains(&n) {
            return Some(name);
        }
        let define = self.con.sounds.get(&i32::from(n))?.name.clone();
        let logical = self.sound(&define)?;
        let _ = writeln!(self.out.sndseq, ":{name}\n  play {logical}\nend\n");
        self.seq_sounds.insert(n);
        Some(name)
    }
}

impl SpriteMapper for Duke<'_> {
    fn mover_sound(&mut self, sfx: Option<(i16, i16)>) -> MoverSound {
        let Some((lo, hi)) = sfx else {
            return MoverSound::Silent;
        };
        if let Some(seq) = self.mover_seqs.get(&(lo, hi)) {
            return seq.clone().map_or(MoverSound::Silent, MoverSound::Sequence);
        }
        let open = self.sound_seq(lo);
        let close = (1..1000)
            .contains(&hi)
            .then(|| self.sound_seq(hi))
            .flatten()
            .or_else(|| open.clone());
        let seq = match (open, close) {
            (Some(open), Some(close)) => {
                let name = format!("DukeMover{lo}_{hi}");
                let _ = writeln!(self.out.sndseq, "[{name}\n  0 {open}\n  1 {close}\n]\n");
                Some(name)
            }
            _ => None,
        };
        self.mover_seqs.insert((lo, hi), seq.clone());
        seq.map_or(MoverSound::Silent, MoverSound::Sequence)
    }

    fn wall_break(&mut self, pic: i32, masked: bool) -> Option<WallBreak> {
        let tile = |name: &str| self.con.value(name);
        let is = |name: &str| tile(name) == Some(pic);
        let (kind, broken) = if masked {
            if is("FANSPRITE") {
                (WallKind::Fan, Some(tile("FANSPRITEBROKE")?))
            } else if is("GLASS") {
                (WallKind::Glass, None)
            } else if is("STAINGLASS1") {
                (WallKind::Stained, None)
            } else if is("BIGFORCE") || is("W_FORCEFIELD") || is("FORCERIPPLE") {
                (WallKind::Force, None)
            } else {
                return None;
            }
        } else {
            let hit = |name: &str| tile(name);
            let indexed = |base: &str, range: std::ops::RangeInclusive<i32>| {
                range.into_iter().any(|k| is(&format!("{base}{k}")))
            };
            let tech = |k: i32| is(&format!("W_TECHWALL{k}"));
            if is("GLASS") {
                (WallKind::Glass, None)
            } else if is("STAINGLASS1") {
                (WallKind::Stained, None)
            } else if is("BIGFORCE") || is("W_FORCEFIELD") || is("FORCERIPPLE") {
                (WallKind::Force, None)
            } else if is("COLAMACHINE") || is("VENDMACHINE") {
                (WallKind::Machine, Some(pic + 2))
            } else if is("OJ")
                || is("FEMPIC2")
                || is("FEMPIC3")
                || is("BORNTOBEWILDSCREEN")
                || indexed("SCREENBREAK", 1..=19)
            {
                (
                    WallKind::Screen,
                    Some(tile("W_SCREENBREAK")? + pic.rem_euclid(3)),
                )
            } else if (5..=9).any(tech) {
                (WallKind::Machine, Some(pic + 1))
            } else if is("W_MILKSHELF") {
                (WallKind::Machine, Some(hit("W_MILKSHELFBROKE")?))
            } else if tech(10) {
                (WallKind::Machine, Some(hit("W_HITTECHWALL10")?))
            } else if tech(1) || (11..=14).any(tech) {
                (WallKind::Machine, Some(hit("W_HITTECHWALL1")?))
            } else if let Some(k) = [2, 3, 4, 15, 16].into_iter().find(|&k| tech(k)) {
                (WallKind::Machine, Some(hit(&format!("W_HITTECHWALL{k}"))?))
            } else if is("ATM") {
                (WallKind::Atm, Some(hit("ATMBROKE")?))
            } else if let Some(k) = (1..=4).find(|k| is(&format!("WALLLIGHT{k}"))) {
                (WallKind::Light, Some(hit(&format!("WALLLIGHTBUST{k}"))?))
            } else {
                let k = [2, 4].into_iter().find(|k| is(&format!("TECHLIGHT{k}")))?;
                (WallKind::Light, Some(hit(&format!("TECHLIGHTBUST{k}"))?))
            }
        };
        if broken.is_some_and(|b| self.tiles.get(b).is_none()) {
            return None;
        }
        Some(WallBreak {
            broken,
            debris: self.wall_break_spot(kind)?,
            unblock: masked || kind == WallKind::Force,
            light: kind == WallKind::Light,
            chain: matches!(kind, WallKind::Glass | WallKind::Stained),
        })
    }

    fn thing(&mut self, sp: &Sprite, in_mover: bool) -> Option<MapThing> {
        let tile = i32::from(sp.picnum);
        let plain = |ednum| MapThing {
            ednum,
            min_skill: 0,
            size: None,
            args: [0; 5],
        };
        // Hidden SE 33 / 36 still have to spawn (cstat 32768).
        if self.con.value("SECTOREFFECTOR") == Some(tile) && sp.lotag == 36 {
            return Some(plain(SHOOTER_EDNUM));
        }
        if self.con.value("SECTOREFFECTOR") == Some(tile) && sp.lotag == 33 {
            return Some(plain(QUAKE_DEBRIS_EDNUM));
        }
        if self.con.value("SECTOREFFECTOR") == Some(tile) && sp.lotag == 27 {
            return Some(plain(CAMERA_EDNUM));
        }
        if self.con.value("CAMERA1") == Some(tile) {
            return Some(plain(CAMERA_EDNUM));
        }
        if self.con.value("VIEWSCREEN") == Some(tile) {
            return Some(plain(VIEWSCREEN_EDNUM));
        }
        if self.con.value("FORCESPHERE") == Some(tile) {
            return Some(plain(FORCE_SPHERE_EDNUM));
        }
        if self.con.value("FRAMEEFFECT1") == Some(tile) {
            return Some(plain(FRAME_EFFECT_EDNUM));
        }
        if self.con.value("NATURALLIGHTNING") == Some(tile) {
            return Some(plain(LIGHTNING_EDNUM));
        }
        if self.con.value("WATERBUBBLEMAKER") == Some(tile) {
            return Some(plain(BUBBLE_MAKER_EDNUM));
        }
        if self.con.value("RESPAWN") == Some(tile) {
            let spawn = i32::from(sp.lotag);
            let &ednum = self.things.get(&spawn)?;
            let delay = if sp.hitag > 0 {
                (i32::from(sp.hitag) * 35 / 30).max(35)
            } else {
                1050
            };
            return Some(MapThing {
                ednum: RESPAWN_EDNUM,
                min_skill: 0,
                size: None,
                args: [ednum, delay, 0, 0, 0],
            });
        }
        if sp.cstat & 32768 != 0 {
            return None;
        }
        if self.con.value("MUSICANDSFX") == Some(tile) {
            return if in_mover {
                None
            } else {
                self.ambient(sp).map(plain)
            };
        }
        if self.con.value("SECTOREFFECTOR") == Some(tile) && sp.lotag == 5 {
            if let Some(t) = self.con.value("ROTATEGUN") {
                if let Some(&ednum) = self.things.get(&t) {
                    return Some(MapThing {
                        ednum,
                        min_skill: 0,
                        size: self.enemies.get(&ednum).copied(),
                        args: [0; 5],
                    });
                }
            }
        }
        if ["DUKECAR", "HELECOPT"]
            .iter()
            .any(|n| self.con.value(n) == Some(tile))
        {
            return self.crashing_car(sp).map(plain);
        }
        if sp.xrepeat <= HIDDEN_CHARGE_XREPEAT
            && ["SEENINE", "OOZFILTER"]
                .iter()
                .any(|n| self.con.value(n) == Some(tile))
        {
            return self.hidden_charge().map(plain);
        }
        if let Some(ednum) = self.breakable(sp).or_else(|| self.destructible(sp)) {
            return Some(plain(ednum));
        }
        if MARKERS.iter().any(|m| self.con.value(m) == Some(tile)) {
            return None;
        }
        if self.con.value("ACCESSCARD") == Some(tile) {
            return Some(plain(match lock_for_pal(sp.pal) {
                LOCK_RED => 13,
                LOCK_YELLOW => 6,
                _ => 5,
            }));
        }
        if let Some(&ednum) = self.things.get(&tile) {
            // Duke drops enemies whose lotag is above the skill (1–4).
            let size = self.enemies.get(&ednum).copied();
            let min_skill = if size.is_some() {
                u8::try_from(sp.lotag.clamp(0, 4)).unwrap_or(0)
            } else {
                0
            };
            let ednum = if sp.pal == 21 {
                self.captain_ednum
                    .filter(|_| self.captain_tiles.contains(&tile))
                    .or_else(|| self.miniboss.get(&tile).copied())
                    .unwrap_or(ednum)
            } else {
                ednum
            };
            let size = self.enemies.get(&ednum).copied().or(size);
            return Some(MapThing {
                ednum,
                min_skill,
                size,
                args: [0; 5],
            });
        }
        self.decoration(sp).map(plain)
    }
}
