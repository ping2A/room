//! Game data from the CON scripts (`DEFS.CON`, `USER.CON`, `GAME.CON`):
//! `define`, `definesound`, `definequote`, global `action` frames and, per
//! `actor` / `useractor` block, its strength, first action and pickup
//! effects (`addphealth`, `addammo`, `addweapon`, `addinventory`, `spawn`, `guts`, `shoot`, `zshoot`, `eshoot`, `ezshoot`, `pstomp`, `findplayer`, `findotherplayer`, `getzrange`, `getflorzofslope`, `getceilzofslope`, `updatesector`, `clipmove`, `changespritestat`, `wackmove`, `respawnhitag`, `debug`, `killit`, `wackplayer`, `operate`, `money`, `lotsofglass`, `lotsofcolour`, `spritepal`, `sizeat`, `sizeto`, `cstat`, `cstator`, `addstrength`, `strength`, `clipdist`, `cactor`, `count`, `resetcount`, `actioncount`, `resetactioncount`, `addkills`, `hitradius`, `fall`, `sleeptime`, `getlastpal`, `debris`, `mail`, `paper`, `tip`, `resetplayer`, `endofgame`, `tossweapon`, `resetweapons`, `resetinventory`, `movesprite`, `setsprite`, `lockplayer`, `setplayerangle`, `setactorangle`, `getplayerangle`, `getactorangle`, `quake`, `soundonce`, `stopsound`, `quote`).
//! `onevent` bodies run a tiny VM (`quote` / `sound` / `ifrnd`) on map enter.

use std::collections::BTreeMap;

/// `action NAME start frames views inc delay`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Action {
    /// First tile, relative to the actor's base tile.
    pub start: i32,
    /// Frame count (at least 1).
    pub frames: i32,
    /// Rotations stored per frame: 1, 5, 7 or 8.
    pub views: i32,
    /// Frame step (may be negative).
    pub inc: i32,
    /// Game tics (30 Hz) between frames.
    pub delay: i32,
}

impl Action {
    /// Base tile of `frame` for an actor whose art starts at `picnum`.
    #[must_use]
    pub fn tile(&self, picnum: i32, frame: i32) -> i32 {
        picnum + self.start + self.views.max(1) * frame * self.inc
    }
}

/// `move NAME vel angvel [flags…]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MoveDef {
    /// Forward speed in Build XY units per Duke tic.
    pub vel: i32,
    /// Yaw speed in Build angle units per Duke tic (`2048` = 360°).
    pub angvel: i32,
    /// CON move flags (`FACEPLAYER`, `FLEEENEMY`, …).
    pub flags: i32,
}

/// Snap yaw to the player.
pub const MOVE_FACEPLAYER: i32 = 1;
/// Snap yaw away from the player.
pub const MOVE_FLEEENEMY: i32 = 2;
/// Same snap as `faceplayer` (CON `seekplayer`).
pub const MOVE_SEEKPLAYER: i32 = 4;
/// Turn at most 22.5° toward the player (CON `faceplayerslow`).
pub const MOVE_FACEPLAYERSLOW: i32 = 8;
/// Add 22.5° yaw each tic (CON `spin`).
pub const MOVE_SPIN: i32 = 16;
/// Face away from a nearby `MISSILE` (CON `dodgebullet`).
pub const MOVE_DODGEBULLET: i32 = 32;
/// Hop if standing on the floor (CON `jumptoplayer`).
pub const MOVE_JUMPTOPLAYER: i32 = 64;
/// Extra XY step toward the player (CON `geth`).
pub const MOVE_GETH: i32 = 128;
/// Step Z toward the player (CON `getv`).
pub const MOVE_GETV: i32 = 256;
/// Set yaw from the playsim RNG (CON `randomangle`).
pub const MOVE_RANDOMANGLE: i32 = 512;
/// Face where the player is moving (CON `faceplayersmart`).
pub const MOVE_FACEPLAYERSMART: i32 = 1024;
/// Face the 8-way heading furthest from the player (CON `furthestdir`).
pub const MOVE_FURTHESTDIR: i32 = 2048;

fn move_flag_bit(tok: &str) -> Option<i32> {
    Some(match tok.to_ascii_lowercase().as_str() {
        "faceplayer" => MOVE_FACEPLAYER,
        "fleeenemy" => MOVE_FLEEENEMY,
        "seekplayer" => MOVE_SEEKPLAYER,
        "faceplayerslow" => MOVE_FACEPLAYERSLOW,
        "spin" => MOVE_SPIN,
        "dodgebullet" => MOVE_DODGEBULLET,
        "jumptoplayer" => MOVE_JUMPTOPLAYER,
        "geth" => MOVE_GETH,
        "getv" => MOVE_GETV,
        "randomangle" => MOVE_RANDOMANGLE,
        "faceplayersmart" => MOVE_FACEPLAYERSMART,
        "furthestdir" => MOVE_FURTHESTDIR,
        _ => return None,
    })
}

/// `definesound` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundDef {
    /// CON name (`PISTOL_FIRE`).
    pub name: String,
    /// File in the GRP (upper case).
    pub file: String,
    /// Type bits: 1 loop, 2 ambient, 4 Duke talk, 8 parental lock.
    pub kind: i32,
}

/// What an actor block does when the player touches it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pickup {
    /// `addphealth N`.
    pub health: Option<i32>,
    /// `addammo WEAPON N`.
    pub ammo: Option<(i32, i32)>,
    /// `addweapon WEAPON N`.
    pub weapon: Option<(i32, i32)>,
    /// `addinventory ITEM N`.
    pub inventory: Option<(i32, i32)>,
    /// First `quote N`.
    pub quote: Option<i32>,
}

/// One `actor` / `useractor` block.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConActor {
    /// Starting strength (health).
    pub strength: i32,
    /// Action named in the header.
    pub action: Option<String>,
    /// `state` the body runs (`firestate`).
    pub state: Option<String>,
    /// Pickup effects found in the body.
    pub pickup: Pickup,
}

/// Everything read from the CON scripts.
#[derive(Debug, Clone, Default)]
pub struct ConData {
    /// `define NAME value` (upper-case names).
    pub defines: BTreeMap<String, i32>,
    /// Sounds by number.
    pub sounds: BTreeMap<i32, SoundDef>,
    /// `definequote` texts.
    pub quotes: BTreeMap<i32, String>,
    /// Global actions by upper-case name.
    pub actions: BTreeMap<String, Action>,
    /// `state` block → the first `action` it starts.
    pub state_action: BTreeMap<String, String>,
    /// Global `move NAME vel angvel` by upper-case name.
    pub moves: BTreeMap<String, MoveDef>,
    /// Actor blocks by tile number.
    pub actors: BTreeMap<i32, ConActor>,
    /// `onevent` bodies (`ENTERLEVEL`, …) as `quote` / `sound` ops.
    pub events: BTreeMap<String, Vec<ConOp>>,
}

/// One CON op the event VM runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConOp {
    /// `quote N`.
    Quote(i32),
    /// `sound` / `globalsound` name.
    Sound(String),
    /// `soundonce` name — play only if that sound is not already queued.
    SoundOnce(String),
    /// `spritesound` name — play from the pawn (same as `sound`).
    SpriteSound(String),
    /// `stopsound` name — drop that sound from the queue.
    StopSound(String),
    /// `ifrnd N` — gate the following ops (`krand` byte `< N`).
    IfRand(i32),
    /// `ifphealthl N` — player health `< N`.
    IfHealthL(i32),
    /// `ifphealthg N` — player health `> N`.
    IfHealthG(i32),
    /// `ifinwater` — fully underwater.
    IfInWater,
    /// `ifonwater` — wading.
    IfOnWater,
    /// `ifdead`.
    IfDead,
    /// `ifmultiplayer`.
    IfMultiplayer,
    /// `ifp palive` / `ponground` / `pfalling` / …
    IfP(String),
    /// `ifoutside` — sky ceiling.
    IfOutside,
    /// `ifhitweapon` — player was just hurt.
    IfHitWeapon,
    /// `ifpalswap` — HUD palette flash (`palfrom` / damage count).
    IfPalSwap,
    /// `ifsomethingonplayer` — a solid actor is overlapping the player.
    IfSomethingOnPlayer,
    /// `ifsquished` — floor/ceiling gap too tight (or last crush).
    IfSquished,
    /// `ifpinventory NAME` — player holds that item.
    IfInventory(String),
    /// `ifcansee` — player has line of sight to a living `COUNTKILL`.
    IfCanSee,
    /// `ifpdistl N` — nearest `COUNTKILL` closer than N Build units.
    IfDistL(i32),
    /// `ifpdistg N` — nearest `COUNTKILL` farther than N Build units.
    IfDistG(i32),
    /// `ifhitspace` — Use is held.
    IfHitSpace,
    /// `ifgotweaponce` — player owns a weapon.
    IfGotWeapon,
    /// `ifnotmoving` — no move input.
    IfNotMoving,
    /// `ifrespawn` — DukeMatch / monster-respawn rules.
    IfRespawn,
    /// `iffloordistl N` — feet-to-floor < N Build z.
    IfFloorDistL(i32),
    /// `ifceilingdistl N` — head-to-ceiling < N Build z.
    IfCeilDistL(i32),
    /// `iffloordistg N` — feet-to-floor ≥ N Build z.
    IfFloorDistG(i32),
    /// `ifceilingdistg N` — head-to-ceiling ≥ N Build z.
    IfCeilDistG(i32),
    /// `ifawayfromwall` — not hugging a blocking line.
    IfAwayFromWall,
    /// `ifinspace` — sky floor and sky ceiling.
    IfInSpace,
    /// `ifgapzl N` — floor-to-ceiling gap < N Build z.
    IfGapZL(i32),
    /// `ifgapzg N` — floor-to-ceiling gap ≥ N Build z.
    IfGapZG(i32),
    /// `ifangdiffl N` — facing delta to nearest `COUNTKILL` < N (2048/circle).
    IfAngDiffL(i32),
    /// `ifangdiffg N` — facing delta to nearest `COUNTKILL` ≥ N (2048/circle).
    IfAngDiffG(i32),
    /// `ifbulletnear` — a `MISSILE` is close.
    IfBulletNear,
    /// `ifinouterspace` — sky floor.
    IfInOuterSpace,
    /// `ifwasweapon NAME` — last hit was that weapon / projectile.
    IfWasWeapon(String),
    /// `ifnosounds` — no queued sim sounds.
    IfNoSounds,
    /// `ifsound NAME` — that sound is queued.
    IfSound(String),
    /// `ifcanshoottarget` — a living `COUNTKILL` is in the aim cone.
    IfCanShootTarget,
    /// `ifspritepal N` — pawn translation / palette index.
    IfSpritePal(i32),
    /// `ifactor NAME` — pawn class matches that CON actor.
    IfActor(String),
    /// `ifcount N` — map tics ≥ N (Duke action counter).
    IfCount(i32),
    /// `ifcanseetarget` — line of sight to the current target.
    IfCanSeeTarget,
    /// `ifactioncount N` — map tics ≥ N (Duke action-frame counter).
    IfActionCount(i32),
    /// `ifstrength N` — pawn health `< N`.
    IfStrength(i32),
    /// `ifspawnedby NAME` — master / last attacker class matches.
    IfSpawnedBy(String),
    /// `ifactornotstayput` — pawn is not a stay-put guard.
    IfActorNotStayPut,
    /// `ifaction NAME` — current action / state label matches.
    IfAction(String),
    /// `ifai NAME` — current AI / state label matches.
    IfAi(String),
    /// `ifmove NAME` — current move / state label matches.
    IfMove(String),
    /// `state NAME` — jump the pawn to that state label.
    State(String),
    /// `ai NAME` — jump the pawn to that AI / state label.
    Ai(String),
    /// `move NAME` — jump the pawn to that move / state label.
    Move(String),
    /// `action NAME` — jump the pawn to that action / state label.
    Action(String),
    /// `else` — run the rest when the last `if` failed.
    Else,
    /// `break` — stop the rest of the event.
    Break,
    /// `palfrom N` — HUD palette flash (damage count).
    PalFrom(i32),
    /// `addphealth N` — add to pawn health.
    AddHealth(i32),
    /// `addammo NAME N`.
    AddAmmo(String, i32),
    /// `addweapon NAME N`.
    AddWeapon(String, i32),
    /// `addinventory NAME N`.
    AddInventory(String, i32),
    /// `spawn NAME` — drop that actor at the pawn.
    Spawn(String),
    /// `guts NAME N` — throw N pieces of that actor at the pawn.
    Guts(String, i32),
    /// `shoot NAME` — fire that actor along the pawn's facing.
    Shoot(String),
    /// `killit` — drop the pawn's health to 0.
    KillIt,
    /// `wackplayer` — hurt the pawn and flash the HUD.
    WackPlayer,
    /// `pkick` — start the pawn's quick kick.
    PKick,
    /// `operate` — use the special in front of the pawn.
    Operate,
    /// `operateactivators N` — use every special with that tag.
    OperateActivators(i32),
    /// `operatesectors` — use specials tagged to the pawn's sector.
    OperateSectors,
    /// `money N` — drop N bills at the pawn.
    Money(i32),
    /// `lotsofglass N` — throw N glass pieces at the pawn.
    LotsOfGlass(i32),
    /// `spritepal N` — set the pawn's palette index.
    SpritePal(i32),
    /// `sizeat X Y` — set the pawn's Build xrepeat / yrepeat.
    SizeAt(i32, i32),
    /// `cstat N` — set Build sprite flags on the pawn.
    Cstat(i32),
    /// `cstator N` — OR Build sprite flags onto the pawn.
    CstatOr(i32),
    /// `addstrength N` — add to pawn health.
    AddStrength(i32),
    /// `strength N` — set pawn health.
    Strength(i32),
    /// `clipdist N` — set pawn collision radius (Build units / 16).
    ClipDist(i32),
    /// `cactor NAME` — change the pawn's class.
    Cactor(String),
    /// `sizeto X Y` — step the pawn's Build xrepeat / yrepeat toward the target.
    SizeTo(i32, i32),
    /// `count N` — set the pawn action counter.
    Count(i32),
    /// `resetcount` — zero the pawn action counter.
    ResetCount,
    /// `actioncount N` — set the pawn action-frame counter.
    ActionCount(i32),
    /// `resetactioncount` — zero the pawn action-frame counter.
    ResetActionCount,
    /// `addkills N` — add to the map kill count.
    AddKills(i32),
    /// `hitradius R D` — blast damage of D within Build radius R.
    HitRadius(i32, i32),
    /// `fall` — drop the pawn onto the sector floor.
    Fall,
    /// `sleeptime N` — set the pawn reaction timer.
    SleepTime(i32),
    /// `getlastpal` — restore the pawn's previous palette.
    GetLastPal,
    /// `debris NAME N` — throw N pieces of that actor at the pawn.
    Debris(String, i32),
    /// `mail N` — drop N mail pickups at the pawn.
    Mail(i32),
    /// `paper N` — drop N paper pickups at the pawn.
    Paper(i32),
    /// `tip` — play the tip line.
    Tip,
    /// `mikesnd` — play the pawn's use chatter.
    MikeSnd,
    /// `resetplayer` — restore the pawn's health and spawn pose.
    ResetPlayer,
    /// `endofgame` — finish the map / episode.
    EndOfGame,
    /// `tossweapon` — drop the pawn's ready weapon.
    TossWeapon,
    /// `resetweapons` — keep kick/pistol, strip the rest.
    ResetWeapons,
    /// `resetinventory` — strip inventory items, keep weapons.
    ResetInventory,
    /// `movesprite X Y Z` — shift the pawn by Build units (`/16`, `−y/16`, `−z/256`).
    MoveSprite(i32, i32, i32),
    /// `clipmove X Y Z` — try-move the pawn by Build units (blocked = stay).
    ClipMove(i32, i32, i32),
    /// `changespritestat N` — store the pawn's sprite stat (`user_spritestat`).
    ChangeSpriteStat(i32),
    /// `wackmove X Y Z` — clipmove, and wack a solid that blocked.
    WackMove(i32, i32, i32),
    /// `respawnhitag N` — revive dead actors whose tid is N.
    RespawnHitag(i32),
    /// `setsprite X Y Z` — snap the pawn to Build units (`/16`, `−y/16`, `−z/256`).
    SetSprite(i32, i32, i32),
    /// `lockplayer N` — freeze the pawn (`N > 0`); `0` unlocks.
    LockPlayer(i32),
    /// `setplayerangle N` — set pawn yaw from a Build angle (0..=2047).
    SetPlayerAngle(i32),
    /// `setactorangle N` — set pawn yaw from a Build angle (0..=2047).
    SetActorAngle(i32),
    /// `getplayerangle` — store pawn yaw as a Build angle (`user_playerangle`).
    GetPlayerAngle,
    /// `getactorangle` — store pawn yaw as a Build angle (`user_actorangle`).
    GetActorAngle,
    /// `lotsofcolour N` — throw N tinted glass pieces.
    LotsOfColour(i32),
    /// `zshoot NAME Z` — fire along facing with extra Build Z velocity (`−z/256`).
    ZShoot(String, i32),
    /// `eshoot NAME` — fire along facing (extra projectile).
    EShoot(String),
    /// `ezshoot NAME Z` — extra projectile with Build Z velocity (`−z/256`).
    EzShoot(String, i32),
    /// `pstomp` — stomp a shootable actor the pawn is standing on.
    PStomp,
    /// `findplayer` — store Build XY distance to the nearest `COUNTKILL`.
    FindPlayer,
    /// `debug` / `addlog` — print a HUD debug line.
    Debug,
    /// `findotherplayer` — store Build XY distance to another player.
    FindOtherPlayer,
    /// `getzrange` — store floor / ceiling Build Z (`user_floorz` / `user_ceilz`).
    GetZRange,
    /// `updatesector` — store the pawn's sector index (`user_sector`).
    UpdateSector,
    /// `getflorzofslope` — store floor Build Z at the pawn (`user_florz`).
    GetFlorZOfSlope,
    /// `getceilzofslope` — store ceiling Build Z at the pawn (`user_ceilzofslope`).
    GetCeilZOfSlope,
    /// `quake N` — start a map-wide rumble for N tics.
    Quake(i32),
}

/// Compiled CON event (`CONEVT` lump).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConEvent {
    /// `ENTERLEVEL`, `EGS`, …
    pub name: String,
    /// Expanded print / sound ops.
    pub ops: Vec<ConEvtOp>,
}

/// Runtime op after quotes and sound names are resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConEvtOp {
    /// HUD text.
    Print(String),
    /// SNDINFO logical name (`duke/pistol_fire`).
    Sound(String),
    /// `soundonce` logical name.
    SoundOnce(String),
    /// `spritesound` logical name.
    SpriteSound(String),
    /// `stopsound` logical name.
    StopSound(String),
    /// `ifrnd N` — skip the rest of the event when the roll fails.
    IfRand(i32),
    /// `ifphealthl N`.
    IfHealthL(i32),
    /// `ifphealthg N`.
    IfHealthG(i32),
    /// `ifinwater`.
    IfInWater,
    /// `ifonwater`.
    IfOnWater,
    /// `ifdead`.
    IfDead,
    /// `ifmultiplayer`.
    IfMultiplayer,
    /// `ifp FLAG`.
    IfP(String),
    /// `ifoutside`.
    IfOutside,
    /// `ifhitweapon`.
    IfHitWeapon,
    /// `ifpalswap`.
    IfPalSwap,
    /// `ifsomethingonplayer`.
    IfSomethingOnPlayer,
    /// `ifsquished`.
    IfSquished,
    /// `ifpinventory NAME`.
    IfInventory(String),
    /// `ifcansee`.
    IfCanSee,
    /// `ifpdistl N`.
    IfDistL(i32),
    /// `ifpdistg N`.
    IfDistG(i32),
    /// `ifhitspace`.
    IfHitSpace,
    /// `ifgotweaponce`.
    IfGotWeapon,
    /// `ifnotmoving`.
    IfNotMoving,
    /// `ifrespawn`.
    IfRespawn,
    /// `iffloordistl N`.
    IfFloorDistL(i32),
    /// `ifceilingdistl N`.
    IfCeilDistL(i32),
    /// `iffloordistg N`.
    IfFloorDistG(i32),
    /// `ifceilingdistg N`.
    IfCeilDistG(i32),
    /// `ifawayfromwall`.
    IfAwayFromWall,
    /// `ifinspace`.
    IfInSpace,
    /// `ifgapzl N`.
    IfGapZL(i32),
    /// `ifgapzg N`.
    IfGapZG(i32),
    /// `ifangdiffl N`.
    IfAngDiffL(i32),
    /// `ifangdiffg N`.
    IfAngDiffG(i32),
    /// `ifbulletnear`.
    IfBulletNear,
    /// `ifinouterspace`.
    IfInOuterSpace,
    /// `ifwasweapon NAME`.
    IfWasWeapon(String),
    /// `ifnosounds`.
    IfNoSounds,
    /// `ifsound NAME`.
    IfSound(String),
    /// `ifcanshoottarget`.
    IfCanShootTarget,
    /// `ifspritepal N`.
    IfSpritePal(i32),
    /// `ifactor NAME`.
    IfActor(String),
    /// `ifcount N`.
    IfCount(i32),
    /// `ifcanseetarget`.
    IfCanSeeTarget,
    /// `ifactioncount N`.
    IfActionCount(i32),
    /// `ifstrength N`.
    IfStrength(i32),
    /// `ifspawnedby NAME`.
    IfSpawnedBy(String),
    /// `ifactornotstayput`.
    IfActorNotStayPut,
    /// `ifaction NAME`.
    IfAction(String),
    /// `ifai NAME`.
    IfAi(String),
    /// `ifmove NAME`.
    IfMove(String),
    /// `state NAME`.
    State(String),
    /// `ai NAME`.
    Ai(String),
    /// `move NAME`.
    Move(String),
    /// `action NAME`.
    Action(String),
    /// `else`.
    Else,
    /// `break`.
    Break,
    /// `palfrom N`.
    PalFrom(i32),
    /// `addphealth N`.
    AddHealth(i32),
    /// `addammo NAME N`.
    AddAmmo(String, i32),
    /// `addweapon NAME N`.
    AddWeapon(String, i32),
    /// `addinventory NAME N`.
    AddInventory(String, i32),
    /// `spawn NAME`.
    Spawn(String),
    /// `guts NAME N`.
    Guts(String, i32),
    /// `shoot NAME`.
    Shoot(String),
    /// `killit`.
    KillIt,
    /// `wackplayer`.
    WackPlayer,
    /// `pkick`.
    PKick,
    /// `operate`.
    Operate,
    /// `operateactivators N`.
    OperateActivators(i32),
    /// `operatesectors`.
    OperateSectors,
    /// `money N`.
    Money(i32),
    /// `lotsofglass N`.
    LotsOfGlass(i32),
    /// `spritepal N`.
    SpritePal(i32),
    /// `sizeat X Y`.
    SizeAt(i32, i32),
    /// `cstat N`.
    Cstat(i32),
    /// `cstator N`.
    CstatOr(i32),
    /// `addstrength N`.
    AddStrength(i32),
    /// `strength N`.
    Strength(i32),
    /// `clipdist N`.
    ClipDist(i32),
    /// `cactor NAME`.
    Cactor(String),
    /// `sizeto X Y`.
    SizeTo(i32, i32),
    /// `count N`.
    Count(i32),
    /// `resetcount`.
    ResetCount,
    /// `actioncount N`.
    ActionCount(i32),
    /// `resetactioncount`.
    ResetActionCount,
    /// `addkills N`.
    AddKills(i32),
    /// `hitradius R D`.
    HitRadius(i32, i32),
    /// `fall`.
    Fall,
    /// `sleeptime N`.
    SleepTime(i32),
    /// `getlastpal`.
    GetLastPal,
    /// `debris NAME N`.
    Debris(String, i32),
    /// `mail N`.
    Mail(i32),
    /// `paper N`.
    Paper(i32),
    /// `tip`.
    Tip,
    /// `mikesnd`.
    MikeSnd,
    /// `resetplayer`.
    ResetPlayer,
    /// `endofgame`.
    EndOfGame,
    /// `tossweapon`.
    TossWeapon,
    /// `resetweapons`.
    ResetWeapons,
    /// `resetinventory`.
    ResetInventory,
    /// `movesprite X Y Z`.
    MoveSprite(i32, i32, i32),
    /// `clipmove X Y Z`.
    ClipMove(i32, i32, i32),
    /// `changespritestat N`.
    ChangeSpriteStat(i32),
    /// `wackmove X Y Z`.
    WackMove(i32, i32, i32),
    /// `respawnhitag N`.
    RespawnHitag(i32),
    /// `setsprite X Y Z`.
    SetSprite(i32, i32, i32),
    /// `lockplayer N`.
    LockPlayer(i32),
    /// `setplayerangle N`.
    SetPlayerAngle(i32),
    /// `setactorangle N`.
    SetActorAngle(i32),
    /// `getplayerangle`.
    GetPlayerAngle,
    /// `getactorangle`.
    GetActorAngle,
    /// `lotsofcolour N`.
    LotsOfColour(i32),
    /// `zshoot NAME Z`.
    ZShoot(String, i32),
    /// `eshoot NAME`.
    EShoot(String),
    /// `ezshoot NAME Z`.
    EzShoot(String, i32),
    /// `pstomp`.
    PStomp,
    /// `findplayer`.
    FindPlayer,
    /// `debug` / `addlog`.
    Debug,
    /// `findotherplayer`.
    FindOtherPlayer,
    /// `getzrange`.
    GetZRange,
    /// `updatesector`.
    UpdateSector,
    /// `getflorzofslope`.
    GetFlorZOfSlope,
    /// `getceilzofslope`.
    GetCeilZOfSlope,
    /// `quake N`.
    Quake(i32),
}

impl ConData {
    /// Parse the concatenated CON text (definitions first).
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut data = Self::default();
        let clean = strip_comments(text);
        for line in clean.lines() {
            data.line_definition(line);
        }
        let tokens: Vec<&str> = clean.split_whitespace().collect();
        data.collect_actions(&tokens);
        data.collect_moves(&tokens);
        data.collect_states(&tokens);
        data.collect_actors(&tokens);
        data.collect_events(&tokens);
        data.collect_useractor_events(&tokens);
        data
    }

    /// `CONEVT` lump: resolved `quote` / `sound` ops per event.
    #[must_use]
    pub fn conevt(&self) -> String {
        let mut out = String::new();
        for (name, ops) in &self.events {
            out.push_str("event ");
            out.push_str(name);
            out.push('\n');
            for op in ops {
                match op {
                    ConOp::Quote(n) => {
                        if let Some(t) = self.quotes.get(n) {
                            if !t.is_empty() {
                                out.push_str("print ");
                                out.push_str(t);
                                out.push('\n');
                            }
                        }
                    }
                    ConOp::Sound(n) => {
                        let logical = self.sound(n).map_or_else(
                            || format!("duke/{}", n.to_ascii_lowercase()),
                            |s| format!("duke/{}", s.name.to_ascii_lowercase()),
                        );
                        out.push_str("sound ");
                        out.push_str(&logical);
                        out.push('\n');
                    }
                    ConOp::SoundOnce(n) => {
                        let logical = self.sound(n).map_or_else(
                            || format!("duke/{}", n.to_ascii_lowercase()),
                            |s| format!("duke/{}", s.name.to_ascii_lowercase()),
                        );
                        out.push_str("soundonce ");
                        out.push_str(&logical);
                        out.push('\n');
                    }
                    ConOp::SpriteSound(n) => {
                        let logical = self.sound(n).map_or_else(
                            || format!("duke/{}", n.to_ascii_lowercase()),
                            |s| format!("duke/{}", s.name.to_ascii_lowercase()),
                        );
                        out.push_str("spritesound ");
                        out.push_str(&logical);
                        out.push('\n');
                    }
                    ConOp::StopSound(n) => {
                        let logical = self.sound(n).map_or_else(
                            || format!("duke/{}", n.to_ascii_lowercase()),
                            |s| format!("duke/{}", s.name.to_ascii_lowercase()),
                        );
                        out.push_str("stopsound ");
                        out.push_str(&logical);
                        out.push('\n');
                    }
                    ConOp::IfRand(n) => {
                        out.push_str("ifrand ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::IfHealthL(n) => {
                        out.push_str("ifhealthl ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::IfHealthG(n) => {
                        out.push_str("ifhealthg ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::IfInWater => out.push_str("ifinwater\n"),
                    ConOp::IfOnWater => out.push_str("ifonwater\n"),
                    ConOp::IfDead => out.push_str("ifdead\n"),
                    ConOp::IfMultiplayer => out.push_str("ifmultiplayer\n"),
                    ConOp::IfP(flag) => {
                        out.push_str("ifp ");
                        out.push_str(flag);
                        out.push('\n');
                    }
                    ConOp::IfOutside => out.push_str("ifoutside\n"),
                    ConOp::IfHitWeapon => out.push_str("ifhitweapon\n"),
                    ConOp::IfPalSwap => out.push_str("ifpalswap\n"),
                    ConOp::IfSomethingOnPlayer => out.push_str("ifsomethingonplayer\n"),
                    ConOp::IfSquished => out.push_str("ifsquished\n"),
                    ConOp::IfInventory(name) => {
                        out.push_str("ifpinventory ");
                        out.push_str(name);
                        out.push('\n');
                    }
                    ConOp::IfCanSee => out.push_str("ifcansee\n"),
                    ConOp::IfDistL(n) => {
                        out.push_str("ifpdistl ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::IfDistG(n) => {
                        out.push_str("ifpdistg ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::IfHitSpace => out.push_str("ifhitspace\n"),
                    ConOp::IfGotWeapon => out.push_str("ifgotweaponce\n"),
                    ConOp::IfNotMoving => out.push_str("ifnotmoving\n"),
                    ConOp::IfRespawn => out.push_str("ifrespawn\n"),
                    ConOp::IfFloorDistL(n) => {
                        out.push_str("iffloordistl ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::IfCeilDistL(n) => {
                        out.push_str("ifceilingdistl ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::IfFloorDistG(n) => {
                        out.push_str("iffloordistg ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::IfCeilDistG(n) => {
                        out.push_str("ifceilingdistg ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::IfAwayFromWall => out.push_str("ifawayfromwall\n"),
                    ConOp::IfInSpace => out.push_str("ifinspace\n"),
                    ConOp::IfGapZL(n) => {
                        out.push_str("ifgapzl ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::IfGapZG(n) => {
                        out.push_str("ifgapzg ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::IfAngDiffL(n) => {
                        out.push_str("ifangdiffl ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::IfAngDiffG(n) => {
                        out.push_str("ifangdiffg ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::IfBulletNear => out.push_str("ifbulletnear\n"),
                    ConOp::IfInOuterSpace => out.push_str("ifinouterspace\n"),
                    ConOp::IfWasWeapon(name) => {
                        out.push_str("ifwasweapon ");
                        out.push_str(name);
                        out.push('\n');
                    }
                    ConOp::IfNoSounds => out.push_str("ifnosounds\n"),
                    ConOp::IfSound(name) => {
                        out.push_str("ifsound ");
                        out.push_str(name);
                        out.push('\n');
                    }
                    ConOp::IfCanShootTarget => out.push_str("ifcanshoottarget\n"),
                    ConOp::IfSpritePal(n) => {
                        out.push_str("ifspritepal ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::IfActor(name) => {
                        out.push_str("ifactor ");
                        out.push_str(name);
                        out.push('\n');
                    }
                    ConOp::IfCount(n) => {
                        out.push_str("ifcount ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::IfCanSeeTarget => out.push_str("ifcanseetarget\n"),
                    ConOp::IfActionCount(n) => {
                        out.push_str("ifactioncount ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::IfStrength(n) => {
                        out.push_str("ifstrength ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::IfSpawnedBy(name) => {
                        out.push_str("ifspawnedby ");
                        out.push_str(name);
                        out.push('\n');
                    }
                    ConOp::IfActorNotStayPut => out.push_str("ifactornotstayput\n"),
                    ConOp::IfAction(name) => {
                        out.push_str("ifaction ");
                        out.push_str(name);
                        out.push('\n');
                    }
                    ConOp::IfAi(name) => {
                        out.push_str("ifai ");
                        out.push_str(name);
                        out.push('\n');
                    }
                    ConOp::IfMove(name) => {
                        out.push_str("ifmove ");
                        out.push_str(name);
                        out.push('\n');
                    }
                    ConOp::State(name) => {
                        out.push_str("state ");
                        out.push_str(name);
                        out.push('\n');
                    }
                    ConOp::Ai(name) => {
                        out.push_str("ai ");
                        out.push_str(name);
                        out.push('\n');
                    }
                    ConOp::Move(name) => {
                        out.push_str("move ");
                        out.push_str(name);
                        out.push('\n');
                        if let Some(m) = self.moves.get(name) {
                            out.push_str("movevel ");
                            out.push_str(&m.vel.to_string());
                            out.push(' ');
                            out.push_str(&m.angvel.to_string());
                            out.push(' ');
                            out.push_str(&m.flags.to_string());
                            out.push('\n');
                        }
                    }
                    ConOp::Action(name) => {
                        out.push_str("action ");
                        out.push_str(name);
                        out.push('\n');
                    }
                    ConOp::Else => out.push_str("else\n"),
                    ConOp::Break => out.push_str("break\n"),
                    ConOp::PalFrom(n) => {
                        out.push_str("palfrom ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::AddHealth(n) => {
                        out.push_str("addphealth ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::AddAmmo(name, n) => {
                        out.push_str("addammo ");
                        out.push_str(name);
                        out.push(' ');
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::AddWeapon(name, n) => {
                        out.push_str("addweapon ");
                        out.push_str(name);
                        out.push(' ');
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::AddInventory(name, n) => {
                        out.push_str("addinventory ");
                        out.push_str(name);
                        out.push(' ');
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::Spawn(name) => {
                        out.push_str("spawn ");
                        out.push_str(name);
                        out.push('\n');
                    }
                    ConOp::Guts(name, n) => {
                        out.push_str("guts ");
                        out.push_str(name);
                        out.push(' ');
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::Shoot(name) => {
                        out.push_str("shoot ");
                        out.push_str(name);
                        out.push('\n');
                    }
                    ConOp::KillIt => out.push_str("killit\n"),
                    ConOp::WackPlayer => out.push_str("wackplayer\n"),
                    ConOp::PKick => out.push_str("pkick\n"),
                    ConOp::Operate => out.push_str("operate\n"),
                    ConOp::OperateActivators(n) => {
                        out.push_str("operateactivators ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::OperateSectors => out.push_str("operatesectors\n"),
                    ConOp::Money(n) => {
                        out.push_str("money ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::LotsOfGlass(n) => {
                        out.push_str("lotsofglass ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::SpritePal(n) => {
                        out.push_str("spritepal ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::SizeAt(x, y) => {
                        out.push_str("sizeat ");
                        out.push_str(&x.to_string());
                        out.push(' ');
                        out.push_str(&y.to_string());
                        out.push('\n');
                    }
                    ConOp::Cstat(n) => {
                        out.push_str("cstat ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::CstatOr(n) => {
                        out.push_str("cstator ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::AddStrength(n) => {
                        out.push_str("addstrength ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::Strength(n) => {
                        out.push_str("strength ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::ClipDist(n) => {
                        out.push_str("clipdist ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::Cactor(name) => {
                        out.push_str("cactor ");
                        out.push_str(name);
                        out.push('\n');
                    }
                    ConOp::SizeTo(x, y) => {
                        out.push_str("sizeto ");
                        out.push_str(&x.to_string());
                        out.push(' ');
                        out.push_str(&y.to_string());
                        out.push('\n');
                    }
                    ConOp::Count(n) => {
                        out.push_str("count ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::ResetCount => out.push_str("resetcount\n"),
                    ConOp::ActionCount(n) => {
                        out.push_str("actioncount ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::ResetActionCount => out.push_str("resetactioncount\n"),
                    ConOp::AddKills(n) => {
                        out.push_str("addkills ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::HitRadius(r, d) => {
                        out.push_str("hitradius ");
                        out.push_str(&r.to_string());
                        out.push(' ');
                        out.push_str(&d.to_string());
                        out.push('\n');
                    }
                    ConOp::Fall => out.push_str("fall\n"),
                    ConOp::SleepTime(n) => {
                        out.push_str("sleeptime ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::GetLastPal => out.push_str("getlastpal\n"),
                    ConOp::Debris(name, n) => {
                        out.push_str("debris ");
                        out.push_str(name);
                        out.push(' ');
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::Mail(n) => {
                        out.push_str("mail ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::Paper(n) => {
                        out.push_str("paper ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::Tip => out.push_str("tip\n"),
                    ConOp::MikeSnd => out.push_str("mikesnd\n"),
                    ConOp::ResetPlayer => out.push_str("resetplayer\n"),
                    ConOp::EndOfGame => out.push_str("endofgame\n"),
                    ConOp::TossWeapon => out.push_str("tossweapon\n"),
                    ConOp::ResetWeapons => out.push_str("resetweapons\n"),
                    ConOp::ResetInventory => out.push_str("resetinventory\n"),
                    ConOp::MoveSprite(x, y, z) => {
                        out.push_str("movesprite ");
                        out.push_str(&x.to_string());
                        out.push(' ');
                        out.push_str(&y.to_string());
                        out.push(' ');
                        out.push_str(&z.to_string());
                        out.push('\n');
                    }
                    ConOp::ClipMove(x, y, z) => {
                        out.push_str("clipmove ");
                        out.push_str(&x.to_string());
                        out.push(' ');
                        out.push_str(&y.to_string());
                        out.push(' ');
                        out.push_str(&z.to_string());
                        out.push('\n');
                    }
                    ConOp::ChangeSpriteStat(n) => {
                        out.push_str("changespritestat ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::WackMove(x, y, z) => {
                        out.push_str("wackmove ");
                        out.push_str(&x.to_string());
                        out.push(' ');
                        out.push_str(&y.to_string());
                        out.push(' ');
                        out.push_str(&z.to_string());
                        out.push('\n');
                    }
                    ConOp::RespawnHitag(n) => {
                        out.push_str("respawnhitag ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::SetSprite(x, y, z) => {
                        out.push_str("setsprite ");
                        out.push_str(&x.to_string());
                        out.push(' ');
                        out.push_str(&y.to_string());
                        out.push(' ');
                        out.push_str(&z.to_string());
                        out.push('\n');
                    }
                    ConOp::LockPlayer(n) => {
                        out.push_str("lockplayer ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::SetPlayerAngle(n) => {
                        out.push_str("setplayerangle ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::SetActorAngle(n) => {
                        out.push_str("setactorangle ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::GetPlayerAngle => out.push_str("getplayerangle\n"),
                    ConOp::GetActorAngle => out.push_str("getactorangle\n"),
                    ConOp::LotsOfColour(n) => {
                        out.push_str("lotsofcolour ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                    ConOp::ZShoot(name, z) => {
                        out.push_str("zshoot ");
                        out.push_str(name);
                        out.push(' ');
                        out.push_str(&z.to_string());
                        out.push('\n');
                    }
                    ConOp::EShoot(name) => {
                        out.push_str("eshoot ");
                        out.push_str(name);
                        out.push('\n');
                    }
                    ConOp::EzShoot(name, z) => {
                        out.push_str("ezshoot ");
                        out.push_str(name);
                        out.push(' ');
                        out.push_str(&z.to_string());
                        out.push('\n');
                    }
                    ConOp::PStomp => out.push_str("pstomp\n"),
                    ConOp::FindPlayer => out.push_str("findplayer\n"),
                    ConOp::Debug => out.push_str("debug\n"),
                    ConOp::FindOtherPlayer => out.push_str("findotherplayer\n"),
                    ConOp::GetZRange => out.push_str("getzrange\n"),
                    ConOp::UpdateSector => out.push_str("updatesector\n"),
                    ConOp::GetFlorZOfSlope => out.push_str("getflorzofslope\n"),
                    ConOp::GetCeilZOfSlope => out.push_str("getceilzofslope\n"),
                    ConOp::Quake(n) => {
                        out.push_str("quake ");
                        out.push_str(&n.to_string());
                        out.push('\n');
                    }
                }
            }
            out.push_str("end\n");
        }
        out
    }

    /// Value of a number literal or `define`.
    #[must_use]
    pub fn value(&self, token: &str) -> Option<i32> {
        token
            .parse()
            .ok()
            .or_else(|| self.defines.get(&token.to_ascii_uppercase()).copied())
    }

    /// Sound by CON name.
    #[must_use]
    pub fn sound(&self, name: &str) -> Option<&SoundDef> {
        self.value(name).and_then(|n| self.sounds.get(&n))
    }

    fn line_definition(&mut self, line: &str) {
        let mut words = line.split_whitespace();
        match words.next().map(str::to_ascii_lowercase).as_deref() {
            Some("define") => {
                if let (Some(name), Some(v)) = (words.next(), words.next()) {
                    if let Some(v) = self.value(v) {
                        self.defines.insert(name.to_ascii_uppercase(), v);
                    }
                }
            }
            Some("definesound") => {
                let (Some(name), Some(file)) = (words.next(), words.next()) else {
                    return;
                };
                let Some(id) = self.value(name) else {
                    return;
                };
                let kind = words.nth(3).and_then(|w| w.parse().ok()).unwrap_or(0);
                self.sounds.insert(
                    id,
                    SoundDef {
                        name: name.to_ascii_uppercase(),
                        file: file.to_ascii_uppercase(),
                        kind,
                    },
                );
            }
            Some("definequote") => {
                if let Some(id) = words.next().and_then(|w| w.parse().ok()) {
                    self.quotes.insert(id, words.collect::<Vec<_>>().join(" "));
                }
            }
            _ => {}
        }
    }

    fn collect_actions(&mut self, tokens: &[&str]) {
        let mut i = 0;
        while i < tokens.len() {
            if tokens[i].eq_ignore_ascii_case("action") {
                if let Some(name) = tokens.get(i + 1) {
                    let nums: Vec<i32> = tokens[i + 2..]
                        .iter()
                        .take(5)
                        .map_while(|t| t.parse::<i32>().ok())
                        .collect();
                    if !nums.is_empty() {
                        let get = |k: usize, d: i32| nums.get(k).copied().unwrap_or(d);
                        self.actions.insert(
                            name.to_ascii_uppercase(),
                            Action {
                                start: get(0, 0),
                                frames: get(1, 1).max(1),
                                views: get(2, 1).max(1),
                                inc: get(3, 1),
                                delay: get(4, 0),
                            },
                        );
                        i += 2 + nums.len();
                        continue;
                    }
                }
            }
            i += 1;
        }
    }

    fn collect_moves(&mut self, tokens: &[&str]) {
        let mut i = 0;
        while i < tokens.len() {
            if tokens[i].eq_ignore_ascii_case("move") {
                if let Some(name) = tokens.get(i + 1) {
                    let nums: Vec<i32> = tokens[i + 2..]
                        .iter()
                        .take(2)
                        .map_while(|t| self.value(t))
                        .collect();
                    if !nums.is_empty() {
                        let mut flags = 0;
                        let mut extra = 0;
                        for t in &tokens[i + 2 + nums.len()..] {
                            let Some(bit) = move_flag_bit(t) else {
                                break;
                            };
                            flags |= bit;
                            extra += 1;
                        }
                        self.moves.insert(
                            name.to_ascii_uppercase(),
                            MoveDef {
                                vel: nums.first().copied().unwrap_or(0),
                                angvel: nums.get(1).copied().unwrap_or(0),
                                flags,
                            },
                        );
                        i += 2 + nums.len() + extra;
                        continue;
                    }
                }
            }
            i += 1;
        }
    }

    /// `state NAME … action ACTION … ends` → that action. A `state NAME enda`
    /// call has no `ends` and is skipped.
    fn collect_states(&mut self, tokens: &[&str]) {
        let mut i = 0;
        while i < tokens.len() {
            if !tokens[i].eq_ignore_ascii_case("state") {
                i += 1;
                continue;
            }
            let Some(name) = tokens.get(i + 1) else {
                i += 1;
                continue;
            };
            let mut action = None;
            let mut j = i + 2;
            let mut defined = false;
            while j < tokens.len() {
                let t = tokens[j];
                if t.eq_ignore_ascii_case("ends") {
                    defined = true;
                    break;
                }
                if t.eq_ignore_ascii_case("enda")
                    || t.eq_ignore_ascii_case("state")
                    || t.eq_ignore_ascii_case("actor")
                    || t.eq_ignore_ascii_case("useractor")
                {
                    break;
                }
                if t.eq_ignore_ascii_case("action") {
                    if let Some(n) = tokens.get(j + 1) {
                        let n = n.to_ascii_uppercase();
                        if self.actions.contains_key(&n) {
                            action.get_or_insert(n);
                        }
                    }
                }
                j += 1;
            }
            if defined {
                if let Some(action) = action {
                    self.state_action.insert(name.to_ascii_uppercase(), action);
                }
            }
            i = j + 1;
        }
    }

    fn collect_actors(&mut self, tokens: &[&str]) {
        let mut i = 0;
        while i < tokens.len() {
            let t = tokens[i].to_ascii_lowercase();
            let header = match t.as_str() {
                "actor" => i + 1,
                // `useractor <type> NAME …`
                "useractor" => i + 2,
                _ => {
                    i += 1;
                    continue;
                }
            };
            let Some(tile) = tokens.get(header).and_then(|n| self.value(n)) else {
                i += 1;
                continue;
            };
            let mut actor = ConActor::default();
            let mut j = header + 1;
            if let Some(s) = tokens.get(j).and_then(|w| self.value(w)) {
                actor.strength = s;
                j += 1;
                if let Some(a) = tokens.get(j) {
                    let a = a.to_ascii_uppercase();
                    if self.actions.contains_key(&a) {
                        actor.action = Some(a);
                    }
                }
            }
            while j < tokens.len() && !tokens[j].eq_ignore_ascii_case("enda") {
                let arg = |k: usize| tokens.get(j + k).and_then(|w| self.value(w));
                let p = &mut actor.pickup;
                match tokens[j].to_ascii_lowercase().as_str() {
                    "addphealth" if p.health.is_none() => p.health = arg(1),
                    "addammo" if p.ammo.is_none() => p.ammo = arg(1).zip(arg(2)),
                    "addweapon" if p.weapon.is_none() => p.weapon = arg(1).zip(arg(2)),
                    "addinventory" if p.inventory.is_none() => {
                        p.inventory = arg(1).zip(arg(2));
                    }
                    "quote" if p.quote.is_none() => p.quote = arg(1),
                    "state" if actor.state.is_none() => {
                        actor.state = tokens.get(j + 1).map(|n| n.to_ascii_uppercase());
                    }
                    _ => {}
                }
                j += 1;
            }
            self.actors.entry(tile).or_insert(actor);
            i = j + 1;
        }
    }

    fn collect_events(&mut self, tokens: &[&str]) {
        let mut i = 0;
        while i < tokens.len() {
            if !tokens[i].eq_ignore_ascii_case("onevent") {
                i += 1;
                continue;
            }
            let Some(raw) = tokens.get(i + 1) else {
                i += 1;
                continue;
            };
            let mut name = raw.to_ascii_uppercase();
            if let Some(rest) = name.strip_prefix("EVENT_") {
                name = rest.to_owned();
            }
            i += 2;
            let mut ops = Vec::new();
            while i < tokens.len() && !tokens[i].eq_ignore_ascii_case("endevent") {
                match tokens[i].to_ascii_lowercase().as_str() {
                    "quote" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::Quote(n));
                            i += 2;
                            continue;
                        }
                    }
                    "sound" | "globalsound" => {
                        if let Some(n) = tokens.get(i + 1) {
                            ops.push(ConOp::Sound(n.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "soundonce" => {
                        if let Some(n) = tokens.get(i + 1) {
                            ops.push(ConOp::SoundOnce(n.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "spritesound" => {
                        if let Some(n) = tokens.get(i + 1) {
                            ops.push(ConOp::SpriteSound(n.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "stopsound" => {
                        if let Some(n) = tokens.get(i + 1) {
                            ops.push(ConOp::StopSound(n.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ifrnd" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfRand(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifphealthl" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfHealthL(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifphealthg" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfHealthG(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifinwater" => {
                        ops.push(ConOp::IfInWater);
                        i += 1;
                        continue;
                    }
                    "ifonwater" => {
                        ops.push(ConOp::IfOnWater);
                        i += 1;
                        continue;
                    }
                    "ifdead" => {
                        ops.push(ConOp::IfDead);
                        i += 1;
                        continue;
                    }
                    "ifmultiplayer" => {
                        ops.push(ConOp::IfMultiplayer);
                        i += 1;
                        continue;
                    }
                    "ifp" => {
                        if let Some(flag) = tokens.get(i + 1) {
                            ops.push(ConOp::IfP(flag.to_ascii_lowercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ifoutside" => {
                        ops.push(ConOp::IfOutside);
                        i += 1;
                        continue;
                    }
                    "ifhitweapon" => {
                        ops.push(ConOp::IfHitWeapon);
                        i += 1;
                        continue;
                    }
                    "ifpalswap" => {
                        ops.push(ConOp::IfPalSwap);
                        i += 1;
                        continue;
                    }
                    "ifsomethingonplayer" => {
                        ops.push(ConOp::IfSomethingOnPlayer);
                        i += 1;
                        continue;
                    }
                    "ifsquished" => {
                        ops.push(ConOp::IfSquished);
                        i += 1;
                        continue;
                    }
                    "ifpinventory" => {
                        if let Some(item) = tokens.get(i + 1) {
                            ops.push(ConOp::IfInventory(item.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ifcansee" => {
                        ops.push(ConOp::IfCanSee);
                        i += 1;
                        continue;
                    }
                    "ifpdistl" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfDistL(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifpdistg" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfDistG(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifhitspace" => {
                        ops.push(ConOp::IfHitSpace);
                        i += 1;
                        continue;
                    }
                    "ifgotweaponce" => {
                        ops.push(ConOp::IfGotWeapon);
                        i += 1;
                        continue;
                    }
                    "ifnotmoving" => {
                        ops.push(ConOp::IfNotMoving);
                        i += 1;
                        continue;
                    }
                    "ifrespawn" => {
                        ops.push(ConOp::IfRespawn);
                        i += 1;
                        continue;
                    }
                    "iffloordistl" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfFloorDistL(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifceilingdistl" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfCeilDistL(n));
                            i += 2;
                            continue;
                        }
                    }
                    "iffloordistg" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfFloorDistG(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifceilingdistg" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfCeilDistG(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifawayfromwall" => {
                        ops.push(ConOp::IfAwayFromWall);
                        i += 1;
                        continue;
                    }
                    "ifinspace" => {
                        ops.push(ConOp::IfInSpace);
                        i += 1;
                        continue;
                    }
                    "ifgapzl" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfGapZL(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifgapzg" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfGapZG(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifangdiffl" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfAngDiffL(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifangdiffg" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfAngDiffG(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifbulletnear" => {
                        ops.push(ConOp::IfBulletNear);
                        i += 1;
                        continue;
                    }
                    "ifinouterspace" => {
                        ops.push(ConOp::IfInOuterSpace);
                        i += 1;
                        continue;
                    }
                    "ifwasweapon" => {
                        if let Some(wep) = tokens.get(i + 1) {
                            ops.push(ConOp::IfWasWeapon(wep.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ifnosounds" => {
                        ops.push(ConOp::IfNoSounds);
                        i += 1;
                        continue;
                    }
                    "ifsound" => {
                        if let Some(s) = tokens.get(i + 1) {
                            ops.push(ConOp::IfSound(s.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ifcanshoottarget" => {
                        ops.push(ConOp::IfCanShootTarget);
                        i += 1;
                        continue;
                    }
                    "ifspritepal" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfSpritePal(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifactor" => {
                        if let Some(act) = tokens.get(i + 1) {
                            ops.push(ConOp::IfActor(act.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ifcount" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfCount(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifcanseetarget" => {
                        ops.push(ConOp::IfCanSeeTarget);
                        i += 1;
                        continue;
                    }
                    "ifactioncount" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfActionCount(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifstrength" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfStrength(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifspawnedby" => {
                        if let Some(act) = tokens.get(i + 1) {
                            ops.push(ConOp::IfSpawnedBy(act.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ifactornotstayput" => {
                        ops.push(ConOp::IfActorNotStayPut);
                        i += 1;
                        continue;
                    }
                    "ifaction" => {
                        if let Some(act) = tokens.get(i + 1) {
                            ops.push(ConOp::IfAction(act.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ifai" => {
                        if let Some(act) = tokens.get(i + 1) {
                            ops.push(ConOp::IfAi(act.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ifmove" => {
                        if let Some(act) = tokens.get(i + 1) {
                            ops.push(ConOp::IfMove(act.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "state" => {
                        if let Some(act) = tokens.get(i + 1) {
                            ops.push(ConOp::State(act.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ai" => {
                        if let Some(act) = tokens.get(i + 1) {
                            ops.push(ConOp::Ai(act.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "move" => {
                        if let Some(act) = tokens.get(i + 1) {
                            ops.push(ConOp::Move(act.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "action" => {
                        if let Some(act) = tokens.get(i + 1) {
                            ops.push(ConOp::Action(act.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "else" => {
                        ops.push(ConOp::Else);
                        i += 1;
                        continue;
                    }
                    "break" => {
                        ops.push(ConOp::Break);
                        i += 1;
                        continue;
                    }
                    "palfrom" => {
                        let n = tokens
                            .get(i + 1)
                            .and_then(|w| self.value(w))
                            .unwrap_or(32)
                            .clamp(0, 100);
                        ops.push(ConOp::PalFrom(n));
                        i += if tokens.get(i + 1).and_then(|w| self.value(w)).is_some() {
                            2
                        } else {
                            1
                        };
                        continue;
                    }
                    "addphealth" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::AddHealth(n));
                            i += 2;
                            continue;
                        }
                    }
                    "addammo" => {
                        if let (Some(name), Some(n)) = (
                            tokens.get(i + 1),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::AddAmmo(name.to_ascii_uppercase(), n));
                            i += 3;
                            continue;
                        }
                    }
                    "addweapon" => {
                        if let (Some(name), Some(n)) = (
                            tokens.get(i + 1),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::AddWeapon(name.to_ascii_uppercase(), n));
                            i += 3;
                            continue;
                        }
                    }
                    "addinventory" => {
                        if let (Some(name), Some(n)) = (
                            tokens.get(i + 1),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::AddInventory(name.to_ascii_uppercase(), n));
                            i += 3;
                            continue;
                        }
                    }
                    "spawn" => {
                        if let Some(name) = tokens.get(i + 1) {
                            ops.push(ConOp::Spawn(name.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "guts" => {
                        if let (Some(name), Some(n)) = (
                            tokens.get(i + 1),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::Guts(name.to_ascii_uppercase(), n));
                            i += 3;
                            continue;
                        }
                    }
                    "debris" => {
                        if let (Some(name), Some(n)) = (
                            tokens.get(i + 1),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::Debris(name.to_ascii_uppercase(), n));
                            i += 3;
                            continue;
                        }
                    }
                    "shoot" => {
                        if let Some(name) = tokens.get(i + 1) {
                            ops.push(ConOp::Shoot(name.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "killit" => {
                        ops.push(ConOp::KillIt);
                        i += 1;
                        continue;
                    }
                    "wackplayer" => {
                        ops.push(ConOp::WackPlayer);
                        i += 1;
                        continue;
                    }
                    "pkick" => {
                        ops.push(ConOp::PKick);
                        i += 1;
                        continue;
                    }
                    "operate" => {
                        ops.push(ConOp::Operate);
                        i += 1;
                        continue;
                    }
                    "operateactivators" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::OperateActivators(n));
                            i += 2;
                            continue;
                        }
                    }
                    "operatesectors" => {
                        ops.push(ConOp::OperateSectors);
                        i += 1;
                        continue;
                    }
                    "money" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::Money(n));
                            i += 2;
                            continue;
                        }
                    }
                    "mail" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::Mail(n));
                            i += 2;
                            continue;
                        }
                    }
                    "paper" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::Paper(n));
                            i += 2;
                            continue;
                        }
                    }
                    "tip" => {
                        ops.push(ConOp::Tip);
                        i += 1;
                        continue;
                    }
                    "mikesnd" => {
                        ops.push(ConOp::MikeSnd);
                        i += 1;
                        continue;
                    }
                    "resetplayer" => {
                        ops.push(ConOp::ResetPlayer);
                        i += 1;
                        continue;
                    }
                    "endofgame" => {
                        ops.push(ConOp::EndOfGame);
                        i += 1;
                        continue;
                    }
                    "tossweapon" => {
                        ops.push(ConOp::TossWeapon);
                        i += 1;
                        continue;
                    }
                    "resetweapons" => {
                        ops.push(ConOp::ResetWeapons);
                        i += 1;
                        continue;
                    }
                    "resetinventory" => {
                        ops.push(ConOp::ResetInventory);
                        i += 1;
                        continue;
                    }
                    "movesprite" => {
                        if let (Some(x), Some(y), Some(z)) = (
                            tokens.get(i + 1).and_then(|w| self.value(w)),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                            tokens.get(i + 3).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::MoveSprite(x, y, z));
                            i += 4;
                            continue;
                        }
                    }
                    "clipmove" => {
                        if let (Some(x), Some(y), Some(z)) = (
                            tokens.get(i + 1).and_then(|w| self.value(w)),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                            tokens.get(i + 3).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::ClipMove(x, y, z));
                            i += 4;
                            continue;
                        }
                    }
                    "changespritestat" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::ChangeSpriteStat(n));
                            i += 2;
                            continue;
                        }
                    }
                    "wackmove" => {
                        if let (Some(x), Some(y), Some(z)) = (
                            tokens.get(i + 1).and_then(|w| self.value(w)),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                            tokens.get(i + 3).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::WackMove(x, y, z));
                            i += 4;
                            continue;
                        }
                    }
                    "respawnhitag" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::RespawnHitag(n));
                            i += 2;
                            continue;
                        }
                    }
                    "setsprite" => {
                        if let (Some(x), Some(y), Some(z)) = (
                            tokens.get(i + 1).and_then(|w| self.value(w)),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                            tokens.get(i + 3).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::SetSprite(x, y, z));
                            i += 4;
                            continue;
                        }
                    }
                    "lockplayer" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::LockPlayer(n));
                            i += 2;
                            continue;
                        }
                    }
                    "setplayerangle" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::SetPlayerAngle(n));
                            i += 2;
                            continue;
                        }
                    }
                    "setactorangle" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::SetActorAngle(n));
                            i += 2;
                            continue;
                        }
                    }
                    "getplayerangle" => {
                        ops.push(ConOp::GetPlayerAngle);
                        i += 1;
                        continue;
                    }
                    "getactorangle" => {
                        ops.push(ConOp::GetActorAngle);
                        i += 1;
                        continue;
                    }
                    "lotsofcolour" | "lotsofcolor" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::LotsOfColour(n));
                            i += 2;
                            continue;
                        }
                    }
                    "zshoot" => {
                        if let (Some(name), Some(z)) = (
                            tokens.get(i + 1).copied(),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::ZShoot(name.to_ascii_uppercase(), z));
                            i += 3;
                            continue;
                        }
                    }
                    "eshoot" => {
                        if let Some(name) = tokens.get(i + 1) {
                            ops.push(ConOp::EShoot(name.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ezshoot" => {
                        if let (Some(name), Some(z)) = (
                            tokens.get(i + 1).copied(),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::EzShoot(name.to_ascii_uppercase(), z));
                            i += 3;
                            continue;
                        }
                    }
                    "pstomp" => {
                        ops.push(ConOp::PStomp);
                        i += 1;
                        continue;
                    }
                    "findplayer" => {
                        ops.push(ConOp::FindPlayer);
                        i += 1;
                        continue;
                    }
                    "debug" | "addlog" => {
                        ops.push(ConOp::Debug);
                        i += 1;
                        continue;
                    }
                    "findotherplayer" => {
                        ops.push(ConOp::FindOtherPlayer);
                        i += 1;
                        continue;
                    }
                    "getzrange" => {
                        ops.push(ConOp::GetZRange);
                        i += 1;
                        continue;
                    }
                    "updatesector" | "updatesectorz" => {
                        ops.push(ConOp::UpdateSector);
                        i += 1;
                        continue;
                    }
                    "getflorzofslope" => {
                        ops.push(ConOp::GetFlorZOfSlope);
                        i += 1;
                        continue;
                    }
                    "getceilzofslope" => {
                        ops.push(ConOp::GetCeilZOfSlope);
                        i += 1;
                        continue;
                    }
                    "quake" | "earthquake" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::Quake(n));
                            i += 2;
                            continue;
                        }
                    }
                    "lotsofglass" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::LotsOfGlass(n));
                            i += 2;
                            continue;
                        }
                    }
                    "spritepal" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::SpritePal(n));
                            i += 2;
                            continue;
                        }
                    }
                    "sizeat" => {
                        if let (Some(x), Some(y)) = (
                            tokens.get(i + 1).and_then(|w| self.value(w)),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::SizeAt(x, y));
                            i += 3;
                            continue;
                        }
                    }
                    "cstat" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::Cstat(n));
                            i += 2;
                            continue;
                        }
                    }
                    "cstator" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::CstatOr(n));
                            i += 2;
                            continue;
                        }
                    }
                    "addstrength" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::AddStrength(n));
                            i += 2;
                            continue;
                        }
                    }
                    "strength" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::Strength(n));
                            i += 2;
                            continue;
                        }
                    }
                    "clipdist" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::ClipDist(n));
                            i += 2;
                            continue;
                        }
                    }
                    "cactor" => {
                        if let Some(name) = tokens.get(i + 1) {
                            ops.push(ConOp::Cactor(name.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "sizeto" => {
                        if let (Some(x), Some(y)) = (
                            tokens.get(i + 1).and_then(|w| self.value(w)),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::SizeTo(x, y));
                            i += 3;
                            continue;
                        }
                    }
                    "count" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::Count(n));
                            i += 2;
                            continue;
                        }
                    }
                    "resetcount" => {
                        ops.push(ConOp::ResetCount);
                        i += 1;
                        continue;
                    }
                    "actioncount" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::ActionCount(n));
                            i += 2;
                            continue;
                        }
                    }
                    "resetactioncount" => {
                        ops.push(ConOp::ResetActionCount);
                        i += 1;
                        continue;
                    }
                    "addkills" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::AddKills(n));
                            i += 2;
                            continue;
                        }
                    }
                    "hitradius" => {
                        if let (Some(r), Some(d)) = (
                            tokens.get(i + 1).and_then(|w| self.value(w)),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::HitRadius(r, d));
                            i += 3;
                            continue;
                        }
                    }
                    "fall" => {
                        ops.push(ConOp::Fall);
                        i += 1;
                        continue;
                    }
                    "sleeptime" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::SleepTime(n));
                            i += 2;
                            continue;
                        }
                    }
                    "getlastpal" => {
                        ops.push(ConOp::GetLastPal);
                        i += 1;
                        continue;
                    }
                    _ => {}
                }
                i += 1;
            }
            if !ops.is_empty() {
                self.events.entry(name).or_insert(ops);
            }
            i += 1;
        }
    }

    fn collect_useractor_events(&mut self, tokens: &[&str]) {
        let mut i = 0;
        let mut ua = Vec::new();
        while i < tokens.len() {
            if !tokens[i].eq_ignore_ascii_case("useractor") {
                i += 1;
                continue;
            }
            let Some(name) = tokens.get(i + 2) else {
                i += 1;
                continue;
            };
            let actor = name.to_ascii_uppercase();
            i += 3;
            if tokens.get(i).and_then(|w| self.value(w)).is_some() {
                i += 1;
            }
            if tokens
                .get(i)
                .is_some_and(|w| self.actions.contains_key(&w.to_ascii_uppercase()))
            {
                i += 1;
            }
            let mut ops = Vec::new();
            while i < tokens.len() && !tokens[i].eq_ignore_ascii_case("enda") {
                match tokens[i].to_ascii_lowercase().as_str() {
                    "quote" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::Quote(n));
                            i += 2;
                            continue;
                        }
                    }
                    "sound" | "globalsound" => {
                        if let Some(n) = tokens.get(i + 1) {
                            ops.push(ConOp::Sound(n.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ifrnd" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfRand(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifcansee" => {
                        ops.push(ConOp::IfCanSee);
                        i += 1;
                        continue;
                    }
                    "shoot" => {
                        if let Some(n) = tokens.get(i + 1) {
                            ops.push(ConOp::Shoot(n.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ifstrength" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfStrength(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifcount" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfCount(n));
                            i += 2;
                            continue;
                        }
                    }
                    "findplayer" => {
                        ops.push(ConOp::FindPlayer);
                        i += 1;
                        continue;
                    }
                    "spawn" => {
                        if let Some(n) = tokens.get(i + 1) {
                            ops.push(ConOp::Spawn(n.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "guts" => {
                        if let Some(name) = tokens.get(i + 1) {
                            let n = tokens.get(i + 2).and_then(|w| self.value(w)).unwrap_or(1);
                            ops.push(ConOp::Guts(name.to_ascii_uppercase(), n));
                            i += 3;
                            continue;
                        }
                    }
                    "ifpdistl" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfDistL(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifpdistg" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfDistG(n));
                            i += 2;
                            continue;
                        }
                    }
                    "killit" => {
                        ops.push(ConOp::KillIt);
                        i += 1;
                        continue;
                    }
                    "soundonce" => {
                        if let Some(n) = tokens.get(i + 1) {
                            ops.push(ConOp::SoundOnce(n.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "spritesound" => {
                        if let Some(n) = tokens.get(i + 1) {
                            ops.push(ConOp::SpriteSound(n.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "cstat" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::Cstat(n));
                            i += 2;
                            continue;
                        }
                    }
                    "spritepal" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::SpritePal(n));
                            i += 2;
                            continue;
                        }
                    }
                    "sizeat" => {
                        if let (Some(x), Some(y)) = (
                            tokens.get(i + 1).and_then(|w| self.value(w)),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::SizeAt(x, y));
                            i += 3;
                            continue;
                        }
                    }
                    "clipdist" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::ClipDist(n));
                            i += 2;
                            continue;
                        }
                    }
                    "cstator" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::CstatOr(n));
                            i += 2;
                            continue;
                        }
                    }
                    "fall" => {
                        ops.push(ConOp::Fall);
                        i += 1;
                        continue;
                    }
                    "hitradius" => {
                        if let (Some(r), Some(d)) = (
                            tokens.get(i + 1).and_then(|w| self.value(w)),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::HitRadius(r, d));
                            i += 3;
                            continue;
                        }
                    }
                    "operate" => {
                        ops.push(ConOp::Operate);
                        i += 1;
                        continue;
                    }
                    "operateactivators" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::OperateActivators(n));
                            i += 2;
                            continue;
                        }
                    }
                    "operatesectors" => {
                        ops.push(ConOp::OperateSectors);
                        i += 1;
                        continue;
                    }
                    "sizeto" => {
                        if let (Some(x), Some(y)) = (
                            tokens.get(i + 1).and_then(|w| self.value(w)),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::SizeTo(x, y));
                            i += 3;
                            continue;
                        }
                    }
                    "cactor" => {
                        if let Some(name) = tokens.get(i + 1) {
                            ops.push(ConOp::Cactor(name.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "count" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::Count(n));
                            i += 2;
                            continue;
                        }
                    }
                    "resetcount" => {
                        ops.push(ConOp::ResetCount);
                        i += 1;
                        continue;
                    }
                    "actioncount" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::ActionCount(n));
                            i += 2;
                            continue;
                        }
                    }
                    "resetactioncount" => {
                        ops.push(ConOp::ResetActionCount);
                        i += 1;
                        continue;
                    }
                    "money" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::Money(n));
                            i += 2;
                            continue;
                        }
                    }
                    "mail" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::Mail(n));
                            i += 2;
                            continue;
                        }
                    }
                    "paper" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::Paper(n));
                            i += 2;
                            continue;
                        }
                    }
                    "lotsofglass" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::LotsOfGlass(n));
                            i += 2;
                            continue;
                        }
                    }
                    "addkills" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::AddKills(n));
                            i += 2;
                            continue;
                        }
                    }
                    "debris" => {
                        if let (Some(name), Some(n)) = (
                            tokens.get(i + 1),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::Debris(name.to_ascii_uppercase(), n));
                            i += 3;
                            continue;
                        }
                    }
                    "tip" => {
                        ops.push(ConOp::Tip);
                        i += 1;
                        continue;
                    }
                    "mikesnd" => {
                        ops.push(ConOp::MikeSnd);
                        i += 1;
                        continue;
                    }
                    "ifinwater" => {
                        ops.push(ConOp::IfInWater);
                        i += 1;
                        continue;
                    }
                    "ifonwater" => {
                        ops.push(ConOp::IfOnWater);
                        i += 1;
                        continue;
                    }
                    "ifoutside" => {
                        ops.push(ConOp::IfOutside);
                        i += 1;
                        continue;
                    }
                    "stopsound" => {
                        if let Some(n) = tokens.get(i + 1) {
                            ops.push(ConOp::StopSound(n.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ifphealthl" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfHealthL(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifphealthg" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfHealthG(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifdead" => {
                        ops.push(ConOp::IfDead);
                        i += 1;
                        continue;
                    }
                    "ifmultiplayer" => {
                        ops.push(ConOp::IfMultiplayer);
                        i += 1;
                        continue;
                    }
                    "ifp" => {
                        if let Some(flag) = tokens.get(i + 1) {
                            ops.push(ConOp::IfP(flag.to_ascii_lowercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ifhitweapon" => {
                        ops.push(ConOp::IfHitWeapon);
                        i += 1;
                        continue;
                    }
                    "ifpalswap" => {
                        ops.push(ConOp::IfPalSwap);
                        i += 1;
                        continue;
                    }
                    "ifsomethingonplayer" => {
                        ops.push(ConOp::IfSomethingOnPlayer);
                        i += 1;
                        continue;
                    }
                    "ifsquished" => {
                        ops.push(ConOp::IfSquished);
                        i += 1;
                        continue;
                    }
                    "ifpinventory" => {
                        if let Some(item) = tokens.get(i + 1) {
                            ops.push(ConOp::IfInventory(item.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ifhitspace" => {
                        ops.push(ConOp::IfHitSpace);
                        i += 1;
                        continue;
                    }
                    "ifgotweaponce" => {
                        ops.push(ConOp::IfGotWeapon);
                        i += 1;
                        continue;
                    }
                    "ifnotmoving" => {
                        ops.push(ConOp::IfNotMoving);
                        i += 1;
                        continue;
                    }
                    "ifrespawn" => {
                        ops.push(ConOp::IfRespawn);
                        i += 1;
                        continue;
                    }
                    "iffloordistl" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfFloorDistL(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifceilingdistl" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfCeilDistL(n));
                            i += 2;
                            continue;
                        }
                    }
                    "iffloordistg" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfFloorDistG(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifceilingdistg" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfCeilDistG(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifawayfromwall" => {
                        ops.push(ConOp::IfAwayFromWall);
                        i += 1;
                        continue;
                    }
                    "ifinspace" => {
                        ops.push(ConOp::IfInSpace);
                        i += 1;
                        continue;
                    }
                    "ifgapzl" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfGapZL(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifgapzg" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfGapZG(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifangdiffl" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfAngDiffL(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifangdiffg" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfAngDiffG(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifbulletnear" => {
                        ops.push(ConOp::IfBulletNear);
                        i += 1;
                        continue;
                    }
                    "ifinouterspace" => {
                        ops.push(ConOp::IfInOuterSpace);
                        i += 1;
                        continue;
                    }
                    "ifwasweapon" => {
                        if let Some(wep) = tokens.get(i + 1) {
                            ops.push(ConOp::IfWasWeapon(wep.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ifnosounds" => {
                        ops.push(ConOp::IfNoSounds);
                        i += 1;
                        continue;
                    }
                    "ifsound" => {
                        if let Some(s) = tokens.get(i + 1) {
                            ops.push(ConOp::IfSound(s.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ifcanshoottarget" => {
                        ops.push(ConOp::IfCanShootTarget);
                        i += 1;
                        continue;
                    }
                    "ifspritepal" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfSpritePal(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifcanseetarget" => {
                        ops.push(ConOp::IfCanSeeTarget);
                        i += 1;
                        continue;
                    }
                    "ifactioncount" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::IfActionCount(n));
                            i += 2;
                            continue;
                        }
                    }
                    "ifspawnedby" => {
                        if let Some(act) = tokens.get(i + 1) {
                            ops.push(ConOp::IfSpawnedBy(act.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ifactornotstayput" => {
                        ops.push(ConOp::IfActorNotStayPut);
                        i += 1;
                        continue;
                    }
                    "ifaction" => {
                        if let Some(act) = tokens.get(i + 1) {
                            ops.push(ConOp::IfAction(act.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ifai" => {
                        if let Some(act) = tokens.get(i + 1) {
                            ops.push(ConOp::IfAi(act.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ifmove" => {
                        if let Some(act) = tokens.get(i + 1) {
                            ops.push(ConOp::IfMove(act.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "state" => {
                        if let Some(act) = tokens.get(i + 1) {
                            ops.push(ConOp::State(act.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ai" => {
                        if let Some(act) = tokens.get(i + 1) {
                            ops.push(ConOp::Ai(act.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "move" => {
                        if let Some(act) = tokens.get(i + 1) {
                            ops.push(ConOp::Move(act.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "action" => {
                        if let Some(act) = tokens.get(i + 1) {
                            ops.push(ConOp::Action(act.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "else" => {
                        ops.push(ConOp::Else);
                        i += 1;
                        continue;
                    }
                    "break" => {
                        ops.push(ConOp::Break);
                        i += 1;
                        continue;
                    }
                    "palfrom" => {
                        let n = tokens
                            .get(i + 1)
                            .and_then(|w| self.value(w))
                            .unwrap_or(32)
                            .clamp(0, 100);
                        ops.push(ConOp::PalFrom(n));
                        i += if tokens.get(i + 1).and_then(|w| self.value(w)).is_some() {
                            2
                        } else {
                            1
                        };
                        continue;
                    }
                    "addphealth" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::AddHealth(n));
                            i += 2;
                            continue;
                        }
                    }
                    "addammo" => {
                        if let (Some(name), Some(n)) = (
                            tokens.get(i + 1),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::AddAmmo(name.to_ascii_uppercase(), n));
                            i += 3;
                            continue;
                        }
                    }
                    "addweapon" => {
                        if let (Some(name), Some(n)) = (
                            tokens.get(i + 1),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::AddWeapon(name.to_ascii_uppercase(), n));
                            i += 3;
                            continue;
                        }
                    }
                    "addinventory" => {
                        if let (Some(name), Some(n)) = (
                            tokens.get(i + 1),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::AddInventory(name.to_ascii_uppercase(), n));
                            i += 3;
                            continue;
                        }
                    }
                    "wackplayer" => {
                        ops.push(ConOp::WackPlayer);
                        i += 1;
                        continue;
                    }
                    "pkick" => {
                        ops.push(ConOp::PKick);
                        i += 1;
                        continue;
                    }
                    "resetplayer" => {
                        ops.push(ConOp::ResetPlayer);
                        i += 1;
                        continue;
                    }
                    "endofgame" => {
                        ops.push(ConOp::EndOfGame);
                        i += 1;
                        continue;
                    }
                    "tossweapon" => {
                        ops.push(ConOp::TossWeapon);
                        i += 1;
                        continue;
                    }
                    "resetweapons" => {
                        ops.push(ConOp::ResetWeapons);
                        i += 1;
                        continue;
                    }
                    "resetinventory" => {
                        ops.push(ConOp::ResetInventory);
                        i += 1;
                        continue;
                    }
                    "movesprite" => {
                        if let (Some(x), Some(y), Some(z)) = (
                            tokens.get(i + 1).and_then(|w| self.value(w)),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                            tokens.get(i + 3).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::MoveSprite(x, y, z));
                            i += 4;
                            continue;
                        }
                    }
                    "clipmove" => {
                        if let (Some(x), Some(y), Some(z)) = (
                            tokens.get(i + 1).and_then(|w| self.value(w)),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                            tokens.get(i + 3).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::ClipMove(x, y, z));
                            i += 4;
                            continue;
                        }
                    }
                    "changespritestat" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::ChangeSpriteStat(n));
                            i += 2;
                            continue;
                        }
                    }
                    "wackmove" => {
                        if let (Some(x), Some(y), Some(z)) = (
                            tokens.get(i + 1).and_then(|w| self.value(w)),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                            tokens.get(i + 3).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::WackMove(x, y, z));
                            i += 4;
                            continue;
                        }
                    }
                    "respawnhitag" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::RespawnHitag(n));
                            i += 2;
                            continue;
                        }
                    }
                    "setsprite" => {
                        if let (Some(x), Some(y), Some(z)) = (
                            tokens.get(i + 1).and_then(|w| self.value(w)),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                            tokens.get(i + 3).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::SetSprite(x, y, z));
                            i += 4;
                            continue;
                        }
                    }
                    "lockplayer" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::LockPlayer(n));
                            i += 2;
                            continue;
                        }
                    }
                    "setplayerangle" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::SetPlayerAngle(n));
                            i += 2;
                            continue;
                        }
                    }
                    "setactorangle" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::SetActorAngle(n));
                            i += 2;
                            continue;
                        }
                    }
                    "getplayerangle" => {
                        ops.push(ConOp::GetPlayerAngle);
                        i += 1;
                        continue;
                    }
                    "getactorangle" => {
                        ops.push(ConOp::GetActorAngle);
                        i += 1;
                        continue;
                    }
                    "lotsofcolour" | "lotsofcolor" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::LotsOfColour(n));
                            i += 2;
                            continue;
                        }
                    }
                    "zshoot" => {
                        if let (Some(name), Some(z)) = (
                            tokens.get(i + 1).copied(),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::ZShoot(name.to_ascii_uppercase(), z));
                            i += 3;
                            continue;
                        }
                    }
                    "eshoot" => {
                        if let Some(name) = tokens.get(i + 1) {
                            ops.push(ConOp::EShoot(name.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    "ezshoot" => {
                        if let (Some(name), Some(z)) = (
                            tokens.get(i + 1).copied(),
                            tokens.get(i + 2).and_then(|w| self.value(w)),
                        ) {
                            ops.push(ConOp::EzShoot(name.to_ascii_uppercase(), z));
                            i += 3;
                            continue;
                        }
                    }
                    "pstomp" => {
                        ops.push(ConOp::PStomp);
                        i += 1;
                        continue;
                    }
                    "findotherplayer" => {
                        ops.push(ConOp::FindOtherPlayer);
                        i += 1;
                        continue;
                    }
                    "debug" | "addlog" => {
                        ops.push(ConOp::Debug);
                        i += 1;
                        continue;
                    }
                    "getzrange" => {
                        ops.push(ConOp::GetZRange);
                        i += 1;
                        continue;
                    }
                    "updatesector" | "updatesectorz" => {
                        ops.push(ConOp::UpdateSector);
                        i += 1;
                        continue;
                    }
                    "getflorzofslope" => {
                        ops.push(ConOp::GetFlorZOfSlope);
                        i += 1;
                        continue;
                    }
                    "getceilzofslope" => {
                        ops.push(ConOp::GetCeilZOfSlope);
                        i += 1;
                        continue;
                    }
                    "quake" | "earthquake" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::Quake(n));
                            i += 2;
                            continue;
                        }
                    }
                    "addstrength" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::AddStrength(n));
                            i += 2;
                            continue;
                        }
                    }
                    "strength" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::Strength(n));
                            i += 2;
                            continue;
                        }
                    }
                    "sleeptime" => {
                        if let Some(n) = tokens.get(i + 1).and_then(|w| self.value(w)) {
                            ops.push(ConOp::SleepTime(n));
                            i += 2;
                            continue;
                        }
                    }
                    "getlastpal" => {
                        ops.push(ConOp::GetLastPal);
                        i += 1;
                        continue;
                    }
                    "ifactor" => {
                        if let Some(act) = tokens.get(i + 1) {
                            ops.push(ConOp::IfActor(act.to_ascii_uppercase()));
                            i += 2;
                            continue;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            if !ops.is_empty() {
                ua.push(ConOp::IfActor(actor));
                ua.extend(ops);
                ua.push(ConOp::Else);
            }
            i += 1;
        }
        if !ua.is_empty() {
            self.events
                .entry("USERACTOR".to_owned())
                .or_default()
                .extend(ua);
        }
    }
}

/// Parse a `CONEVT` lump.
#[must_use]
pub fn parse_conevt(text: &str) -> Vec<ConEvent> {
    let mut out = Vec::new();
    let mut cur: Option<ConEvent> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut words = line.split_whitespace();
        match words.next().map(str::to_ascii_lowercase).as_deref() {
            Some("event") => {
                if let Some(done) = cur.take() {
                    out.push(done);
                }
                let name = words.next().unwrap_or("").to_ascii_uppercase();
                cur = Some(ConEvent {
                    name,
                    ops: Vec::new(),
                });
            }
            Some("print") => {
                if let Some(e) = cur.as_mut() {
                    let t = line.get(5..).unwrap_or("").trim();
                    if !t.is_empty() {
                        e.ops.push(ConEvtOp::Print(t.to_owned()));
                    }
                }
            }
            Some("sound") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(s) = words.next() {
                        e.ops.push(ConEvtOp::Sound(s.to_owned()));
                    }
                }
            }
            Some("soundonce") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(s) = words.next() {
                        e.ops.push(ConEvtOp::SoundOnce(s.to_owned()));
                    }
                }
            }
            Some("spritesound") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(s) = words.next() {
                        e.ops.push(ConEvtOp::SpriteSound(s.to_owned()));
                    }
                }
            }
            Some("stopsound") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(s) = words.next() {
                        e.ops.push(ConEvtOp::StopSound(s.to_owned()));
                    }
                }
            }
            Some("ifrand") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::IfRand(n));
                    }
                }
            }
            Some("ifhealthl") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::IfHealthL(n));
                    }
                }
            }
            Some("ifhealthg") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::IfHealthG(n));
                    }
                }
            }
            Some("ifinwater") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfInWater);
                }
            }
            Some("ifonwater") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfOnWater);
                }
            }
            Some("ifdead") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfDead);
                }
            }
            Some("ifmultiplayer") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfMultiplayer);
                }
            }
            Some("ifp") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(flag) = words.next() {
                        e.ops.push(ConEvtOp::IfP(flag.to_ascii_lowercase()));
                    }
                }
            }
            Some("ifoutside") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfOutside);
                }
            }
            Some("ifhitweapon") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfHitWeapon);
                }
            }
            Some("ifpalswap") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfPalSwap);
                }
            }
            Some("ifsomethingonplayer") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfSomethingOnPlayer);
                }
            }
            Some("ifsquished") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfSquished);
                }
            }
            Some("ifpinventory") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(item) = words.next() {
                        e.ops.push(ConEvtOp::IfInventory(item.to_ascii_uppercase()));
                    }
                }
            }
            Some("ifcansee") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfCanSee);
                }
            }
            Some("ifpdistl") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::IfDistL(n));
                    }
                }
            }
            Some("ifpdistg") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::IfDistG(n));
                    }
                }
            }
            Some("ifhitspace") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfHitSpace);
                }
            }
            Some("ifgotweaponce") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfGotWeapon);
                }
            }
            Some("ifnotmoving") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfNotMoving);
                }
            }
            Some("ifrespawn") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfRespawn);
                }
            }
            Some("iffloordistl") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::IfFloorDistL(n));
                    }
                }
            }
            Some("ifceilingdistl") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::IfCeilDistL(n));
                    }
                }
            }
            Some("iffloordistg") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::IfFloorDistG(n));
                    }
                }
            }
            Some("ifceilingdistg") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::IfCeilDistG(n));
                    }
                }
            }
            Some("ifawayfromwall") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfAwayFromWall);
                }
            }
            Some("ifinspace") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfInSpace);
                }
            }
            Some("ifgapzl") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::IfGapZL(n));
                    }
                }
            }
            Some("ifgapzg") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::IfGapZG(n));
                    }
                }
            }
            Some("ifangdiffl") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::IfAngDiffL(n));
                    }
                }
            }
            Some("ifangdiffg") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::IfAngDiffG(n));
                    }
                }
            }
            Some("ifbulletnear") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfBulletNear);
                }
            }
            Some("ifinouterspace") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfInOuterSpace);
                }
            }
            Some("ifwasweapon") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(wep) = words.next() {
                        e.ops.push(ConEvtOp::IfWasWeapon(wep.to_ascii_uppercase()));
                    }
                }
            }
            Some("ifnosounds") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfNoSounds);
                }
            }
            Some("ifsound") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(s) = words.next() {
                        e.ops.push(ConEvtOp::IfSound(s.to_ascii_uppercase()));
                    }
                }
            }
            Some("ifcanshoottarget") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfCanShootTarget);
                }
            }
            Some("ifspritepal") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::IfSpritePal(n));
                    }
                }
            }
            Some("ifactor") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(act) = words.next() {
                        e.ops.push(ConEvtOp::IfActor(act.to_ascii_uppercase()));
                    }
                }
            }
            Some("ifcount") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::IfCount(n));
                    }
                }
            }
            Some("ifcanseetarget") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfCanSeeTarget);
                }
            }
            Some("ifactioncount") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::IfActionCount(n));
                    }
                }
            }
            Some("ifstrength") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::IfStrength(n));
                    }
                }
            }
            Some("ifspawnedby") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(act) = words.next() {
                        e.ops.push(ConEvtOp::IfSpawnedBy(act.to_ascii_uppercase()));
                    }
                }
            }
            Some("ifactornotstayput") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::IfActorNotStayPut);
                }
            }
            Some("ifaction") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(act) = words.next() {
                        e.ops.push(ConEvtOp::IfAction(act.to_ascii_uppercase()));
                    }
                }
            }
            Some("ifai") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(act) = words.next() {
                        e.ops.push(ConEvtOp::IfAi(act.to_ascii_uppercase()));
                    }
                }
            }
            Some("ifmove") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(act) = words.next() {
                        e.ops.push(ConEvtOp::IfMove(act.to_ascii_uppercase()));
                    }
                }
            }
            Some("state") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(act) = words.next() {
                        e.ops.push(ConEvtOp::State(act.to_ascii_uppercase()));
                    }
                }
            }
            Some("ai") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(act) = words.next() {
                        e.ops.push(ConEvtOp::Ai(act.to_ascii_uppercase()));
                    }
                }
            }
            Some("move") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(act) = words.next() {
                        e.ops.push(ConEvtOp::Move(act.to_ascii_uppercase()));
                    }
                }
            }
            Some("action") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(act) = words.next() {
                        e.ops.push(ConEvtOp::Action(act.to_ascii_uppercase()));
                    }
                }
            }
            Some("else") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::Else);
                }
            }
            Some("break") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::Break);
                }
            }
            Some("palfrom") => {
                if let Some(e) = cur.as_mut() {
                    let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(32);
                    e.ops.push(ConEvtOp::PalFrom(n));
                }
            }
            Some("addphealth") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::AddHealth(n));
                    }
                }
            }
            Some("addammo") => {
                if let Some(e) = cur.as_mut() {
                    if let (Some(name), Some(n)) =
                        (words.next(), words.next().and_then(|w| w.parse().ok()))
                    {
                        e.ops.push(ConEvtOp::AddAmmo(name.to_ascii_uppercase(), n));
                    }
                }
            }
            Some("addweapon") => {
                if let Some(e) = cur.as_mut() {
                    if let (Some(name), Some(n)) =
                        (words.next(), words.next().and_then(|w| w.parse().ok()))
                    {
                        e.ops
                            .push(ConEvtOp::AddWeapon(name.to_ascii_uppercase(), n));
                    }
                }
            }
            Some("addinventory") => {
                if let Some(e) = cur.as_mut() {
                    if let (Some(name), Some(n)) =
                        (words.next(), words.next().and_then(|w| w.parse().ok()))
                    {
                        e.ops
                            .push(ConEvtOp::AddInventory(name.to_ascii_uppercase(), n));
                    }
                }
            }
            Some("spawn") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(name) = words.next() {
                        e.ops.push(ConEvtOp::Spawn(name.to_ascii_uppercase()));
                    }
                }
            }
            Some("guts") => {
                if let Some(e) = cur.as_mut() {
                    if let (Some(name), Some(n)) =
                        (words.next(), words.next().and_then(|w| w.parse().ok()))
                    {
                        e.ops.push(ConEvtOp::Guts(name.to_ascii_uppercase(), n));
                    }
                }
            }
            Some("debris") => {
                if let Some(e) = cur.as_mut() {
                    if let (Some(name), Some(n)) =
                        (words.next(), words.next().and_then(|w| w.parse().ok()))
                    {
                        e.ops.push(ConEvtOp::Debris(name.to_ascii_uppercase(), n));
                    }
                }
            }
            Some("shoot") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(name) = words.next() {
                        e.ops.push(ConEvtOp::Shoot(name.to_ascii_uppercase()));
                    }
                }
            }
            Some("killit") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::KillIt);
                }
            }
            Some("wackplayer") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::WackPlayer);
                }
            }
            Some("pkick") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::PKick);
                }
            }
            Some("operate") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::Operate);
                }
            }
            Some("operateactivators") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::OperateActivators(n));
                    }
                }
            }
            Some("operatesectors") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::OperateSectors);
                }
            }
            Some("money") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::Money(n));
                    }
                }
            }
            Some("mail") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::Mail(n));
                    }
                }
            }
            Some("paper") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::Paper(n));
                    }
                }
            }
            Some("tip") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::Tip);
                }
            }
            Some("mikesnd") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::MikeSnd);
                }
            }
            Some("resetplayer") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::ResetPlayer);
                }
            }
            Some("endofgame") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::EndOfGame);
                }
            }
            Some("tossweapon") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::TossWeapon);
                }
            }
            Some("resetweapons") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::ResetWeapons);
                }
            }
            Some("resetinventory") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::ResetInventory);
                }
            }
            Some("movesprite") => {
                if let Some(e) = cur.as_mut() {
                    if let (Some(x), Some(y), Some(z)) = (
                        words.next().and_then(|w| w.parse().ok()),
                        words.next().and_then(|w| w.parse().ok()),
                        words.next().and_then(|w| w.parse().ok()),
                    ) {
                        e.ops.push(ConEvtOp::MoveSprite(x, y, z));
                    }
                }
            }
            Some("clipmove") => {
                if let Some(e) = cur.as_mut() {
                    if let (Some(x), Some(y), Some(z)) = (
                        words.next().and_then(|w| w.parse().ok()),
                        words.next().and_then(|w| w.parse().ok()),
                        words.next().and_then(|w| w.parse().ok()),
                    ) {
                        e.ops.push(ConEvtOp::ClipMove(x, y, z));
                    }
                }
            }
            Some("changespritestat") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::ChangeSpriteStat(n));
                    }
                }
            }
            Some("wackmove") => {
                if let Some(e) = cur.as_mut() {
                    if let (Some(x), Some(y), Some(z)) = (
                        words.next().and_then(|w| w.parse().ok()),
                        words.next().and_then(|w| w.parse().ok()),
                        words.next().and_then(|w| w.parse().ok()),
                    ) {
                        e.ops.push(ConEvtOp::WackMove(x, y, z));
                    }
                }
            }
            Some("respawnhitag") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::RespawnHitag(n));
                    }
                }
            }
            Some("setsprite") => {
                if let Some(e) = cur.as_mut() {
                    if let (Some(x), Some(y), Some(z)) = (
                        words.next().and_then(|w| w.parse().ok()),
                        words.next().and_then(|w| w.parse().ok()),
                        words.next().and_then(|w| w.parse().ok()),
                    ) {
                        e.ops.push(ConEvtOp::SetSprite(x, y, z));
                    }
                }
            }
            Some("lockplayer") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::LockPlayer(n));
                    }
                }
            }
            Some("setplayerangle") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::SetPlayerAngle(n));
                    }
                }
            }
            Some("setactorangle") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::SetActorAngle(n));
                    }
                }
            }
            Some("getplayerangle") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::GetPlayerAngle);
                }
            }
            Some("getactorangle") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::GetActorAngle);
                }
            }
            Some("lotsofcolour") | Some("lotsofcolor") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::LotsOfColour(n));
                    }
                }
            }
            Some("zshoot") => {
                if let Some(e) = cur.as_mut() {
                    if let (Some(name), Some(z)) =
                        (words.next(), words.next().and_then(|w| w.parse().ok()))
                    {
                        e.ops.push(ConEvtOp::ZShoot(name.to_ascii_uppercase(), z));
                    }
                }
            }
            Some("eshoot") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(name) = words.next() {
                        e.ops.push(ConEvtOp::EShoot(name.to_ascii_uppercase()));
                    }
                }
            }
            Some("ezshoot") => {
                if let Some(e) = cur.as_mut() {
                    if let (Some(name), Some(z)) =
                        (words.next(), words.next().and_then(|w| w.parse().ok()))
                    {
                        e.ops.push(ConEvtOp::EzShoot(name.to_ascii_uppercase(), z));
                    }
                }
            }
            Some("pstomp") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::PStomp);
                }
            }
            Some("findplayer") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::FindPlayer);
                }
            }
            Some("debug") | Some("addlog") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::Debug);
                }
            }
            Some("findotherplayer") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::FindOtherPlayer);
                }
            }
            Some("getzrange") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::GetZRange);
                }
            }
            Some("updatesector") | Some("updatesectorz") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::UpdateSector);
                }
            }
            Some("getflorzofslope") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::GetFlorZOfSlope);
                }
            }
            Some("getceilzofslope") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::GetCeilZOfSlope);
                }
            }
            Some("quake") | Some("earthquake") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::Quake(n));
                    }
                }
            }
            Some("lotsofglass") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::LotsOfGlass(n));
                    }
                }
            }
            Some("spritepal") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::SpritePal(n));
                    }
                }
            }
            Some("sizeat") => {
                if let Some(e) = cur.as_mut() {
                    if let (Some(x), Some(y)) = (
                        words.next().and_then(|w| w.parse().ok()),
                        words.next().and_then(|w| w.parse().ok()),
                    ) {
                        e.ops.push(ConEvtOp::SizeAt(x, y));
                    }
                }
            }
            Some("cstat") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::Cstat(n));
                    }
                }
            }
            Some("cstator") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::CstatOr(n));
                    }
                }
            }
            Some("addstrength") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::AddStrength(n));
                    }
                }
            }
            Some("strength") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::Strength(n));
                    }
                }
            }
            Some("clipdist") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::ClipDist(n));
                    }
                }
            }
            Some("cactor") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(name) = words.next() {
                        e.ops.push(ConEvtOp::Cactor(name.to_ascii_uppercase()));
                    }
                }
            }
            Some("sizeto") => {
                if let Some(e) = cur.as_mut() {
                    if let (Some(x), Some(y)) = (
                        words.next().and_then(|w| w.parse().ok()),
                        words.next().and_then(|w| w.parse().ok()),
                    ) {
                        e.ops.push(ConEvtOp::SizeTo(x, y));
                    }
                }
            }
            Some("count") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::Count(n));
                    }
                }
            }
            Some("resetcount") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::ResetCount);
                }
            }
            Some("actioncount") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::ActionCount(n));
                    }
                }
            }
            Some("resetactioncount") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::ResetActionCount);
                }
            }
            Some("addkills") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::AddKills(n));
                    }
                }
            }
            Some("hitradius") => {
                if let Some(e) = cur.as_mut() {
                    if let (Some(r), Some(d)) = (
                        words.next().and_then(|w| w.parse().ok()),
                        words.next().and_then(|w| w.parse().ok()),
                    ) {
                        e.ops.push(ConEvtOp::HitRadius(r, d));
                    }
                }
            }
            Some("fall") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::Fall);
                }
            }
            Some("sleeptime") => {
                if let Some(e) = cur.as_mut() {
                    if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                        e.ops.push(ConEvtOp::SleepTime(n));
                    }
                }
            }
            Some("getlastpal") => {
                if let Some(e) = cur.as_mut() {
                    e.ops.push(ConEvtOp::GetLastPal);
                }
            }
            Some("end") => {
                if let Some(done) = cur.take() {
                    out.push(done);
                }
            }
            _ => {}
        }
    }
    if let Some(done) = cur.take() {
        out.push(done);
    }
    out
}

/// Run `name` (`ENTERLEVEL`) against `print` / `sound` callbacks.
pub fn run_conevt(
    text: &str,
    name: &str,
    mut print: impl FnMut(&str),
    mut sound: impl FnMut(&str),
) {
    let mut want = name.trim().to_ascii_uppercase();
    if let Some(rest) = want.strip_prefix("EVENT_") {
        want = rest.to_owned();
    }
    for ev in parse_conevt(text) {
        if ev.name != want {
            continue;
        }
        let mut skip = false;
        let mut stop = false;
        for op in ev.ops {
            if stop {
                continue;
            }
            match op {
                ConEvtOp::IfRand(n) => skip = n <= 0,
                ConEvtOp::IfHealthL(_) | ConEvtOp::IfHealthG(_) => {}
                ConEvtOp::IfInWater | ConEvtOp::IfOnWater => {}
                ConEvtOp::IfDead | ConEvtOp::IfMultiplayer => {}
                ConEvtOp::IfP(_) => {}
                ConEvtOp::IfOutside
                | ConEvtOp::IfHitWeapon
                | ConEvtOp::IfPalSwap
                | ConEvtOp::IfSomethingOnPlayer
                | ConEvtOp::IfSquished => {}
                ConEvtOp::IfInventory(_) | ConEvtOp::IfCanSee => {}
                ConEvtOp::IfDistL(_) | ConEvtOp::IfDistG(_) | ConEvtOp::IfHitSpace => {}
                ConEvtOp::IfGotWeapon | ConEvtOp::IfNotMoving | ConEvtOp::IfRespawn => {}
                ConEvtOp::IfFloorDistL(_) | ConEvtOp::IfCeilDistL(_) => {}
                ConEvtOp::IfFloorDistG(_) | ConEvtOp::IfCeilDistG(_) => {}
                ConEvtOp::IfAwayFromWall | ConEvtOp::IfInSpace | ConEvtOp::IfGapZL(_) => {}
                ConEvtOp::IfGapZG(_) | ConEvtOp::IfAngDiffG(_) => {}
                ConEvtOp::IfAngDiffL(_) | ConEvtOp::IfBulletNear | ConEvtOp::IfInOuterSpace => {}
                ConEvtOp::IfWasWeapon(_) | ConEvtOp::IfNoSounds | ConEvtOp::IfSound(_) => {}
                ConEvtOp::IfCanShootTarget | ConEvtOp::IfSpritePal(_) | ConEvtOp::IfActor(_) => {}
                ConEvtOp::IfCount(_) | ConEvtOp::IfCanSeeTarget => {}
                ConEvtOp::IfActionCount(_) | ConEvtOp::IfStrength(_) => {}
                ConEvtOp::IfSpawnedBy(_) | ConEvtOp::IfActorNotStayPut => {}
                ConEvtOp::IfAction(_) | ConEvtOp::IfAi(_) | ConEvtOp::IfMove(_) => {}
                ConEvtOp::State(_) | ConEvtOp::Ai(_) | ConEvtOp::Move(_) | ConEvtOp::Action(_) => {}
                ConEvtOp::Else => skip = !skip,
                ConEvtOp::Break if !skip => stop = true,
                ConEvtOp::PalFrom(_)
                | ConEvtOp::AddHealth(_)
                | ConEvtOp::AddAmmo(_, _)
                | ConEvtOp::AddWeapon(_, _)
                | ConEvtOp::AddInventory(_, _)
                | ConEvtOp::Spawn(_)
                | ConEvtOp::Guts(_, _)
                | ConEvtOp::Shoot(_)
                | ConEvtOp::KillIt
                | ConEvtOp::WackPlayer
                | ConEvtOp::PKick
                | ConEvtOp::Operate
                | ConEvtOp::OperateActivators(_)
                | ConEvtOp::OperateSectors
                | ConEvtOp::Money(_)
                | ConEvtOp::LotsOfGlass(_)
                | ConEvtOp::SpritePal(_)
                | ConEvtOp::SizeAt(_, _)
                | ConEvtOp::Cstat(_)
                | ConEvtOp::CstatOr(_)
                | ConEvtOp::AddStrength(_)
                | ConEvtOp::Strength(_)
                | ConEvtOp::ClipDist(_)
                | ConEvtOp::Cactor(_)
                | ConEvtOp::SizeTo(_, _)
                | ConEvtOp::Count(_)
                | ConEvtOp::ResetCount
                | ConEvtOp::ActionCount(_)
                | ConEvtOp::ResetActionCount
                | ConEvtOp::AddKills(_)
                | ConEvtOp::HitRadius(_, _)
                | ConEvtOp::Fall
                | ConEvtOp::SleepTime(_)
                | ConEvtOp::GetLastPal
                | ConEvtOp::Debris(_, _)
                | ConEvtOp::Mail(_)
                | ConEvtOp::Paper(_)
                | ConEvtOp::Tip
                | ConEvtOp::MikeSnd
                | ConEvtOp::ResetPlayer
                | ConEvtOp::EndOfGame
                | ConEvtOp::TossWeapon
                | ConEvtOp::ResetWeapons
                | ConEvtOp::ResetInventory
                | ConEvtOp::MoveSprite(_, _, _)
                | ConEvtOp::ClipMove(_, _, _)
                | ConEvtOp::ChangeSpriteStat(_)
                | ConEvtOp::WackMove(_, _, _)
                | ConEvtOp::RespawnHitag(_)
                | ConEvtOp::SetSprite(_, _, _)
                | ConEvtOp::LockPlayer(_)
                | ConEvtOp::SetPlayerAngle(_)
                | ConEvtOp::SetActorAngle(_)
                | ConEvtOp::GetPlayerAngle
                | ConEvtOp::GetActorAngle
                | ConEvtOp::LotsOfColour(_)
                | ConEvtOp::ZShoot(_, _)
                | ConEvtOp::EShoot(_)
                | ConEvtOp::EzShoot(_, _)
                | ConEvtOp::PStomp
                | ConEvtOp::FindPlayer
                | ConEvtOp::Debug
                | ConEvtOp::FindOtherPlayer
                | ConEvtOp::GetZRange
                | ConEvtOp::UpdateSector
                | ConEvtOp::GetFlorZOfSlope
                | ConEvtOp::GetCeilZOfSlope
                | ConEvtOp::Quake(_) => {}
                ConEvtOp::Print(s) if !skip => print(&s),
                ConEvtOp::Sound(s) if !skip => sound(&s),
                ConEvtOp::SoundOnce(s) if !skip => sound(&s),
                ConEvtOp::SpriteSound(s) if !skip => sound(&s),
                ConEvtOp::StopSound(_) => {}
                _ => {}
            }
        }
    }
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        let line = rest.find("//");
        let block = rest.find("/*");
        match (line, block) {
            (Some(l), b) if b.is_none_or(|b| l < b) => {
                out.push_str(&rest[..l]);
                rest = rest[l..].find('\n').map_or("", |n| &rest[l + n..]);
            }
            (_, Some(b)) => {
                out.push_str(&rest[..b]);
                rest = rest[b + 2..].find("*/").map_or("", |e| &rest[b + 4 + e..]);
            }
            _ => {
                out.push_str(rest);
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    const CON: &str = "define LIZTROOP 1680\n\
        define TROOPSTRENGTH 30 // comment\n\
        define COLA 51\ndefine PISTOL_WEAPON 1\ndefine PISTOL_FIRE 3\n\
        definesound PISTOL_FIRE pistol.voc -64 0 254 1 0\n\
        definequote 61 SMALL MEDKIT: +10\n\
        /* block\n comment */\n\
        action ATROOPWALKING 0 4 5 1 12\n\
        action ATROOPDEAD 54\n\
        actor LIZTROOP TROOPSTRENGTH ATROOPWALKING\n  action ATROOPDEAD\nenda\n\
        actor COLA\n ifp palive { addphealth 10 quote 61 }\n addammo PISTOL_WEAPON 12\nenda\n\
        onevent EVENT_ENTERLEVEL\n quote 61\n globalsound PISTOL_FIRE\nendevent\n";

    #[test]
    fn reads_defines_sounds_actions_and_actors() {
        let con = ConData::parse(CON);
        assert_eq!(con.value("troopstrength"), Some(30));
        assert_eq!(con.sound("PISTOL_FIRE").unwrap().file, "PISTOL.VOC");
        assert_eq!(con.sound("PISTOL_FIRE").unwrap().kind, 1);
        assert_eq!(con.quotes[&61], "SMALL MEDKIT: +10");
        let walk = con.actions["ATROOPWALKING"];
        assert_eq!((walk.frames, walk.views, walk.delay), (4, 5, 12));
        assert_eq!(walk.tile(1680, 2), 1690);
        assert_eq!(con.actions["ATROOPDEAD"].frames, 1);
        let trooper = &con.actors[&1680];
        assert_eq!(trooper.strength, 30);
        assert_eq!(trooper.action.as_deref(), Some("ATROOPWALKING"));
        let cola = &con.actors[&51].pickup;
        assert_eq!(
            (cola.health, cola.quote, cola.ammo),
            (Some(10), Some(61), Some((1, 12)))
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::Quote(61), ConOp::Sound("PISTOL_FIRE".into())]
        );
        let evt = con.conevt();
        let mut prints = Vec::new();
        let mut sounds = Vec::new();
        run_conevt(
            &evt,
            "ENTERLEVEL",
            |s| prints.push(s.to_owned()),
            |s| sounds.push(s.to_owned()),
        );
        assert_eq!(prints, vec!["SMALL MEDKIT: +10"]);
        assert_eq!(sounds, vec!["duke/pistol_fire"]);
    }

    #[test]
    fn garbage_never_panics() {
        for cut in 0..CON.len() {
            if CON.is_char_boundary(cut) {
                let _ = ConData::parse(&CON[..cut]);
            }
        }
        let _ = ConData::parse("actor\nuseractor x\naction\ndefinesound\n/* open");
    }

    #[test]
    fn ifrnd_gates_the_following_quote() {
        let con = ConData::parse(
            "definequote 1 HELLO\n\
             onevent EVENT_GETSHOT\n ifrnd 0 quote 1\nendevent\n\
             onevent EVENT_KILLIT\n ifrnd 256 quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["GETSHOT"],
            vec![ConOp::IfRand(0), ConOp::Quote(1)]
        );
        let evt = con.conevt();
        let mut miss = Vec::new();
        run_conevt(&evt, "GETSHOT", |s| miss.push(s.to_owned()), |_| {});
        assert!(miss.is_empty(), "{miss:?}");
        let mut hit = Vec::new();
        run_conevt(&evt, "KILLIT", |s| hit.push(s.to_owned()), |_| {});
        assert_eq!(hit, vec!["HELLO"]);
    }

    #[test]
    fn ifphealthl_compiles() {
        let con = ConData::parse(
            "definequote 1 LOW\n\
             onevent EVENT_GETSHOT\n ifphealthl 40 quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["GETSHOT"],
            vec![ConOp::IfHealthL(40), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifhealthl 40"));
    }

    #[test]
    fn ifinwater_compiles() {
        let con = ConData::parse(
            "definequote 1 GULP\n\
             onevent EVENT_ENTERLEVEL\n ifinwater quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfInWater, ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifinwater"));
    }

    #[test]
    fn ifdead_and_ifmultiplayer_compile() {
        let con = ConData::parse(
            "definequote 1 X\n\
             onevent EVENT_KILLIT\n ifdead quote 1\nendevent\n\
             onevent EVENT_ENTERLEVEL\n ifmultiplayer quote 1\nendevent\n",
        );
        assert_eq!(con.events["KILLIT"], vec![ConOp::IfDead, ConOp::Quote(1)]);
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfMultiplayer, ConOp::Quote(1)]
        );
        let evt = con.conevt();
        assert!(evt.contains("ifdead"));
        assert!(evt.contains("ifmultiplayer"));
    }

    #[test]
    fn ifp_palive_compiles() {
        let con = ConData::parse(
            "definequote 1 OK\n\
             onevent EVENT_ENTERLEVEL\n ifp palive quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfP("palive".into()), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifp palive"));
    }

    #[test]
    fn ifp_pducking_compiles() {
        let con = ConData::parse(
            "definequote 1 DOWN\n\
             onevent EVENT_ENTERLEVEL\n ifp pducking quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfP("pducking".into()), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifp pducking"));
    }

    #[test]
    fn ifp_pshrunk_compiles() {
        let con = ConData::parse(
            "definequote 1 TINY\n\
             onevent EVENT_ENTERLEVEL\n ifp pshrunk quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfP("pshrunk".into()), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifp pshrunk"));
    }

    #[test]
    fn ifp_phigher_compiles() {
        let con = ConData::parse(
            "definequote 1 UP\n\
             onevent EVENT_ENTERLEVEL\n ifp phigher quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfP("phigher".into()), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifp phigher"));
    }

    #[test]
    fn ifp_pwalkingback_compiles() {
        let con = ConData::parse(
            "definequote 1 BACK\n\
             onevent EVENT_ENTERLEVEL\n ifp pwalkingback quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfP("pwalkingback".into()), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifp pwalkingback"));
    }

    #[test]
    fn ifp_paiming_compiles() {
        let con = ConData::parse(
            "definequote 1 AIM\n\
             onevent EVENT_ENTERLEVEL\n ifp paiming quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfP("paiming".into()), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifp paiming"));
    }

    #[test]
    fn ifp_pweapon_compiles() {
        let con = ConData::parse(
            "definequote 1 GUN\n\
             onevent EVENT_ENTERLEVEL\n ifp pweapon quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfP("pweapon".into()), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifp pweapon"));
    }

    #[test]
    fn ifoutside_compiles() {
        let con = ConData::parse(
            "definequote 1 OPEN\n\
             onevent EVENT_ENTERLEVEL\n ifoutside quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfOutside, ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifoutside"));
    }

    #[test]
    fn ifsquished_compiles() {
        let con = ConData::parse(
            "definequote 1 FLAT\n\
             onevent EVENT_GETSHOT\n ifsquished quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["GETSHOT"],
            vec![ConOp::IfSquished, ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifsquished"));
    }

    #[test]
    fn ifpinventory_compiles() {
        let con = ConData::parse(
            "definequote 1 KIT\n\
             onevent EVENT_ENTERLEVEL\n ifpinventory FIRSTAID quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfInventory("FIRSTAID".into()), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifpinventory FIRSTAID"));
    }

    #[test]
    fn ifcansee_compiles() {
        let con = ConData::parse(
            "definequote 1 SPOTTED\n\
             onevent EVENT_ENTERLEVEL\n ifcansee quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfCanSee, ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifcansee"));
    }

    #[test]
    fn ifpdistl_compiles() {
        let con = ConData::parse(
            "definequote 1 CLOSE\n\
             onevent EVENT_ENTERLEVEL\n ifpdistl 4096 quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfDistL(4096), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifpdistl 4096"));
    }

    #[test]
    fn ifhitspace_compiles() {
        let con = ConData::parse(
            "definequote 1 OPEN\n\
             onevent EVENT_USE\n ifhitspace quote 1\nendevent\n",
        );
        assert_eq!(con.events["USE"], vec![ConOp::IfHitSpace, ConOp::Quote(1)]);
        assert!(con.conevt().contains("ifhitspace"));
    }

    #[test]
    fn ifgotweaponce_compiles() {
        let con = ConData::parse(
            "definequote 1 GUN\n\
             onevent EVENT_ENTERLEVEL\n ifgotweaponce quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfGotWeapon, ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifgotweaponce"));
    }

    #[test]
    fn ifnotmoving_compiles() {
        let con = ConData::parse(
            "definequote 1 STILL\n\
             onevent EVENT_ENTERLEVEL\n ifnotmoving quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfNotMoving, ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifnotmoving"));
    }

    #[test]
    fn ifrespawn_compiles() {
        let con = ConData::parse(
            "definequote 1 AGAIN\n\
             onevent EVENT_RESPAWN\n ifrespawn quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["RESPAWN"],
            vec![ConOp::IfRespawn, ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifrespawn"));
    }

    #[test]
    fn iffloordistl_compiles() {
        let con = ConData::parse(
            "definequote 1 LOW\n\
             onevent EVENT_ENTERLEVEL\n iffloordistl 1024 quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfFloorDistL(1024), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("iffloordistl 1024"));
    }

    #[test]
    fn iffloordistg_compiles() {
        let con = ConData::parse(
            "definequote 1 HIGH\n\
             onevent EVENT_ENTERLEVEL\n iffloordistg 1024 quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfFloorDistG(1024), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("iffloordistg 1024"));
    }

    #[test]
    fn ifceilingdistg_compiles() {
        let con = ConData::parse(
            "definequote 1 TALL\n\
             onevent EVENT_ENTERLEVEL\n ifceilingdistg 100 quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfCeilDistG(100), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifceilingdistg 100"));
    }

    #[test]
    fn ifawayfromwall_compiles() {
        let con = ConData::parse(
            "definequote 1 OPEN\n\
             onevent EVENT_ENTERLEVEL\n ifawayfromwall quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfAwayFromWall, ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifawayfromwall"));
    }

    #[test]
    fn ifinspace_compiles() {
        let con = ConData::parse(
            "definequote 1 VOID\n\
             onevent EVENT_ENTERLEVEL\n ifinspace quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfInSpace, ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifinspace"));
    }

    #[test]
    fn ifgapzl_compiles() {
        let con = ConData::parse(
            "definequote 1 CRAWL\n\
             onevent EVENT_ENTERLEVEL\n ifgapzl 8192 quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfGapZL(8192), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifgapzl 8192"));
    }

    #[test]
    fn ifgapzg_compiles() {
        let con = ConData::parse(
            "definequote 1 TALL\n\
             onevent EVENT_ENTERLEVEL\n ifgapzg 20000 quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfGapZG(20000), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifgapzg 20000"));
    }

    #[test]
    fn ifangdiffl_compiles() {
        let con = ConData::parse(
            "definequote 1 LOOK\n\
             onevent EVENT_ENTERLEVEL\n ifangdiffl 256 quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfAngDiffL(256), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifangdiffl 256"));
    }

    #[test]
    fn ifangdiffg_compiles() {
        let con = ConData::parse(
            "definequote 1 AWAY\n\
             onevent EVENT_ENTERLEVEL\n ifangdiffg 256 quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfAngDiffG(256), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifangdiffg 256"));
    }

    #[test]
    fn ifbulletnear_compiles() {
        let con = ConData::parse(
            "definequote 1 INCOMING\n\
             onevent EVENT_GETSHOT\n ifbulletnear quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["GETSHOT"],
            vec![ConOp::IfBulletNear, ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifbulletnear"));
    }

    #[test]
    fn ifinouterspace_compiles() {
        let con = ConData::parse(
            "definequote 1 VOID\n\
             onevent EVENT_ENTERLEVEL\n ifinouterspace quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfInOuterSpace, ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifinouterspace"));
    }

    #[test]
    fn ifwasweapon_compiles() {
        let con = ConData::parse(
            "definequote 1 RPG\n\
             onevent EVENT_GETSHOT\n ifwasweapon RPG quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["GETSHOT"],
            vec![ConOp::IfWasWeapon("RPG".into()), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifwasweapon RPG"));
    }

    #[test]
    fn ifnosounds_compiles() {
        let con = ConData::parse(
            "definequote 1 QUIET\n\
             onevent EVENT_ENTERLEVEL\n ifnosounds quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfNoSounds, ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifnosounds"));
    }

    #[test]
    fn ifsound_compiles() {
        let con = ConData::parse(
            "definequote 1 BANG\n\
             onevent EVENT_FIRE\n ifsound PISTOL_FIRE quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["FIRE"],
            vec![ConOp::IfSound("PISTOL_FIRE".into()), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifsound PISTOL_FIRE"));
    }

    #[test]
    fn ifcanshoottarget_compiles() {
        let con = ConData::parse(
            "definequote 1 FIRE\n\
             onevent EVENT_ENTERLEVEL\n ifcanshoottarget quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfCanShootTarget, ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifcanshoottarget"));
    }

    #[test]
    fn ifspritepal_compiles() {
        let con = ConData::parse(
            "definequote 1 PAL\n\
             onevent EVENT_ENTERLEVEL\n ifspritepal 21 quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfSpritePal(21), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifspritepal 21"));
    }

    #[test]
    fn ifactor_compiles() {
        let con = ConData::parse(
            "definequote 1 ME\n\
             onevent EVENT_ENTERLEVEL\n ifactor APLAYER quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfActor("APLAYER".into()), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifactor APLAYER"));
    }

    #[test]
    fn useractor_body_emits_useractor_event() {
        let con = ConData::parse(
            "definequote 1 DUMMY THINK\n\
             useractor enemy DUMMY 30\n\
             ifrnd 256\n\
             quote 1\n\
             ifcansee\n\
             shoot RPG\n\
             findplayer\n\
             spawn BLOOD\n\
             guts JIBS1 4\n\
             ifpdistl 4096\n\
             killit\n\
             soundonce ALERT\n\
             spritesound ALERT\n\
             cstat 32768\n\
             spritepal 21\n\
             sizeat 32 32\n\
             clipdist 128\n\
             cstator 2\n\
             fall\n\
             hitradius 4096 50\n\
             operate\n\
             operateactivators 7\n\
             operatesectors\n\
             sizeto 32 32\n\
             cactor LIZTROOP\n\
             count 8\n\
             resetcount\n\
             actioncount 4\n\
             resetactioncount\n\
             money 3\n\
             mail 2\n\
             paper 1\n\
             lotsofglass 8\n\
             addkills 1\n\
             debris SCRAP 3\n\
             tip\n\
             mikesnd\n\
             ifinwater\n\
             ifonwater\n\
             ifoutside\n\
             stopsound ALERT\n\
             ifphealthl 40\n\
             ifphealthg 80\n\
             ifdead\n\
             ifmultiplayer\n\
             ifp palive\n\
             ifhitweapon\n\
             ifpalswap\n\
             ifsomethingonplayer\n\
             ifsquished\n\
             ifpinventory FIRSTAID\n\
             ifhitspace\n\
             ifgotweaponce\n\
             ifnotmoving\n\
             ifrespawn\n\
             iffloordistl 32\n\
             ifceilingdistl 64\n\
             iffloordistg 128\n\
             ifceilingdistg 256\n\
             ifawayfromwall\n\
             ifinspace\n\
             ifgapzl 8192\n\
             ifgapzg 20000\n\
             ifangdiffl 256\n\
             ifangdiffg 512\n\
             ifbulletnear\n\
             ifinouterspace\n\
             ifwasweapon RPG\n\
             ifnosounds\n\
             ifsound ALERT\n\
             ifcanshoottarget\n\
             ifspritepal 21\n\
             ifcanseetarget\n\
             ifactioncount 8\n\
             ifspawnedby LIZTROOP\n\
             ifactornotstayput\n\
             ifaction STAND\n\
             ifai AIZOMBIE\n\
             ifmove FACEVELS\n\
             state SEE\n\
             ai SEE\n\
             move SEE\n\
             action SEE\n\
             break\n\
             palfrom 32\n\
             addphealth 10\n\
             addammo PISTOL_WEAPON 12\n\
             addweapon RPG 1\n\
             addinventory STEROIDS 1\n\
             wackplayer\n\
             pkick\n\
             resetplayer\n\
             endofgame\n\
             tossweapon\n\
             resetweapons\n\
             resetinventory\n\
             movesprite 256 0 0\n\
             clipmove 256 0 0\n\
             changespritestat 1\n\
             wackmove 128 0 0\n\
             respawnhitag 5\n\
             setsprite 2304 -2048 0\n\
             lockplayer 30\n\
             setplayerangle 512\n\
             setactorangle 1024\n\
             getplayerangle\n\
             getactorangle\n\
             lotsofcolour 4\n\
             zshoot RPG 256\n\
             eshoot RPG\n\
             ezshoot RPG 256\n\
             pstomp\n\
             findotherplayer\n\
             debug\n\
             getzrange\n\
             updatesector\n\
             getflorzofslope\n\
             getceilzofslope\n\
             quake 35\n\
             addstrength 25\n\
             strength 40\n\
             sleeptime 30\n\
             getlastpal\n\
             ifactor LIZTROOP\n\
             enda\n",
        );
        assert_eq!(
            con.events["USERACTOR"],
            vec![
                ConOp::IfActor("DUMMY".into()),
                ConOp::IfRand(256),
                ConOp::Quote(1),
                ConOp::IfCanSee,
                ConOp::Shoot("RPG".into()),
                ConOp::FindPlayer,
                ConOp::Spawn("BLOOD".into()),
                ConOp::Guts("JIBS1".into(), 4),
                ConOp::IfDistL(4096),
                ConOp::KillIt,
                ConOp::SoundOnce("ALERT".into()),
                ConOp::SpriteSound("ALERT".into()),
                ConOp::Cstat(32768),
                ConOp::SpritePal(21),
                ConOp::SizeAt(32, 32),
                ConOp::ClipDist(128),
                ConOp::CstatOr(2),
                ConOp::Fall,
                ConOp::HitRadius(4096, 50),
                ConOp::Operate,
                ConOp::OperateActivators(7),
                ConOp::OperateSectors,
                ConOp::SizeTo(32, 32),
                ConOp::Cactor("LIZTROOP".into()),
                ConOp::Count(8),
                ConOp::ResetCount,
                ConOp::ActionCount(4),
                ConOp::ResetActionCount,
                ConOp::Money(3),
                ConOp::Mail(2),
                ConOp::Paper(1),
                ConOp::LotsOfGlass(8),
                ConOp::AddKills(1),
                ConOp::Debris("SCRAP".into(), 3),
                ConOp::Tip,
                ConOp::MikeSnd,
                ConOp::IfInWater,
                ConOp::IfOnWater,
                ConOp::IfOutside,
                ConOp::StopSound("ALERT".into()),
                ConOp::IfHealthL(40),
                ConOp::IfHealthG(80),
                ConOp::IfDead,
                ConOp::IfMultiplayer,
                ConOp::IfP("palive".into()),
                ConOp::IfHitWeapon,
                ConOp::IfPalSwap,
                ConOp::IfSomethingOnPlayer,
                ConOp::IfSquished,
                ConOp::IfInventory("FIRSTAID".into()),
                ConOp::IfHitSpace,
                ConOp::IfGotWeapon,
                ConOp::IfNotMoving,
                ConOp::IfRespawn,
                ConOp::IfFloorDistL(32),
                ConOp::IfCeilDistL(64),
                ConOp::IfFloorDistG(128),
                ConOp::IfCeilDistG(256),
                ConOp::IfAwayFromWall,
                ConOp::IfInSpace,
                ConOp::IfGapZL(8192),
                ConOp::IfGapZG(20000),
                ConOp::IfAngDiffL(256),
                ConOp::IfAngDiffG(512),
                ConOp::IfBulletNear,
                ConOp::IfInOuterSpace,
                ConOp::IfWasWeapon("RPG".into()),
                ConOp::IfNoSounds,
                ConOp::IfSound("ALERT".into()),
                ConOp::IfCanShootTarget,
                ConOp::IfSpritePal(21),
                ConOp::IfCanSeeTarget,
                ConOp::IfActionCount(8),
                ConOp::IfSpawnedBy("LIZTROOP".into()),
                ConOp::IfActorNotStayPut,
                ConOp::IfAction("STAND".into()),
                ConOp::IfAi("AIZOMBIE".into()),
                ConOp::IfMove("FACEVELS".into()),
                ConOp::State("SEE".into()),
                ConOp::Ai("SEE".into()),
                ConOp::Move("SEE".into()),
                ConOp::Action("SEE".into()),
                ConOp::Break,
                ConOp::PalFrom(32),
                ConOp::AddHealth(10),
                ConOp::AddAmmo("PISTOL_WEAPON".into(), 12),
                ConOp::AddWeapon("RPG".into(), 1),
                ConOp::AddInventory("STEROIDS".into(), 1),
                ConOp::WackPlayer,
                ConOp::PKick,
                ConOp::ResetPlayer,
                ConOp::EndOfGame,
                ConOp::TossWeapon,
                ConOp::ResetWeapons,
                ConOp::ResetInventory,
                ConOp::MoveSprite(256, 0, 0),
                ConOp::ClipMove(256, 0, 0),
                ConOp::ChangeSpriteStat(1),
                ConOp::WackMove(128, 0, 0),
                ConOp::RespawnHitag(5),
                ConOp::SetSprite(2304, -2048, 0),
                ConOp::LockPlayer(30),
                ConOp::SetPlayerAngle(512),
                ConOp::SetActorAngle(1024),
                ConOp::GetPlayerAngle,
                ConOp::GetActorAngle,
                ConOp::LotsOfColour(4),
                ConOp::ZShoot("RPG".into(), 256),
                ConOp::EShoot("RPG".into()),
                ConOp::EzShoot("RPG".into(), 256),
                ConOp::PStomp,
                ConOp::FindOtherPlayer,
                ConOp::Debug,
                ConOp::GetZRange,
                ConOp::UpdateSector,
                ConOp::GetFlorZOfSlope,
                ConOp::GetCeilZOfSlope,
                ConOp::Quake(35),
                ConOp::AddStrength(25),
                ConOp::Strength(40),
                ConOp::SleepTime(30),
                ConOp::GetLastPal,
                ConOp::IfActor("LIZTROOP".into()),
                ConOp::Else,
            ]
        );
        let evt = con.conevt();
        assert!(evt.contains("event USERACTOR"), "{evt}");
        assert!(evt.contains("ifactor DUMMY"), "{evt}");
        assert!(evt.contains("ifrand 256"), "{evt}");
        assert!(evt.contains("shoot RPG"), "{evt}");
        assert!(evt.contains("findplayer"), "{evt}");
        assert!(evt.contains("spawn BLOOD"), "{evt}");
        assert!(evt.contains("guts JIBS1 4"), "{evt}");
        assert!(evt.contains("ifpdistl 4096"), "{evt}");
        assert!(evt.contains("killit"), "{evt}");
        assert!(evt.contains("soundonce"), "{evt}");
        assert!(evt.contains("spritesound"), "{evt}");
        assert!(evt.contains("cstat 32768"), "{evt}");
        assert!(evt.contains("spritepal 21"), "{evt}");
        assert!(evt.contains("sizeat 32 32"), "{evt}");
        assert!(evt.contains("clipdist 128"), "{evt}");
        assert!(evt.contains("cstator 2"), "{evt}");
        assert!(evt.contains("fall"), "{evt}");
        assert!(evt.contains("hitradius 4096 50"), "{evt}");
        assert!(evt.contains("operate"), "{evt}");
        assert!(evt.contains("operateactivators 7"), "{evt}");
        assert!(evt.contains("operatesectors"), "{evt}");
        assert!(evt.contains("sizeto 32 32"), "{evt}");
        assert!(evt.contains("cactor LIZTROOP"), "{evt}");
        assert!(evt.contains("count 8"), "{evt}");
        assert!(evt.contains("resetcount"), "{evt}");
        assert!(evt.contains("actioncount 4"), "{evt}");
        assert!(evt.contains("resetactioncount"), "{evt}");
        assert!(evt.contains("money 3"), "{evt}");
        assert!(evt.contains("mail 2"), "{evt}");
        assert!(evt.contains("paper 1"), "{evt}");
        assert!(evt.contains("lotsofglass 8"), "{evt}");
        assert!(evt.contains("addkills 1"), "{evt}");
        assert!(evt.contains("debris SCRAP 3"), "{evt}");
        assert!(evt.contains("tip"), "{evt}");
        assert!(evt.contains("mikesnd"), "{evt}");
        assert!(evt.contains("ifinwater"), "{evt}");
        assert!(evt.contains("ifonwater"), "{evt}");
        assert!(evt.contains("ifoutside"), "{evt}");
        assert!(evt.contains("stopsound"), "{evt}");
        assert!(evt.contains("ifhealthl 40"), "{evt}");
        assert!(evt.contains("ifhealthg 80"), "{evt}");
        assert!(evt.contains("ifdead"), "{evt}");
        assert!(evt.contains("ifmultiplayer"), "{evt}");
        assert!(evt.contains("ifp palive"), "{evt}");
        assert!(evt.contains("ifhitweapon"), "{evt}");
        assert!(evt.contains("ifpalswap"), "{evt}");
        assert!(evt.contains("ifsomethingonplayer"), "{evt}");
        assert!(evt.contains("ifsquished"), "{evt}");
        assert!(evt.contains("ifpinventory FIRSTAID"), "{evt}");
        assert!(evt.contains("ifhitspace"), "{evt}");
        assert!(evt.contains("ifgotweaponce"), "{evt}");
        assert!(evt.contains("ifnotmoving"), "{evt}");
        assert!(evt.contains("ifrespawn"), "{evt}");
        assert!(evt.contains("iffloordistl 32"), "{evt}");
        assert!(evt.contains("ifceilingdistl 64"), "{evt}");
        assert!(evt.contains("iffloordistg 128"), "{evt}");
        assert!(evt.contains("ifceilingdistg 256"), "{evt}");
        assert!(evt.contains("ifawayfromwall"), "{evt}");
        assert!(evt.contains("ifinspace"), "{evt}");
        assert!(evt.contains("ifgapzl 8192"), "{evt}");
        assert!(evt.contains("ifgapzg 20000"), "{evt}");
        assert!(evt.contains("ifangdiffl 256"), "{evt}");
        assert!(evt.contains("ifangdiffg 512"), "{evt}");
        assert!(evt.contains("ifbulletnear"), "{evt}");
        assert!(evt.contains("ifinouterspace"), "{evt}");
        assert!(evt.contains("ifwasweapon RPG"), "{evt}");
        assert!(evt.contains("ifnosounds"), "{evt}");
        assert!(evt.contains("ifsound ALERT"), "{evt}");
        assert!(evt.contains("ifcanshoottarget"), "{evt}");
        assert!(evt.contains("ifspritepal 21"), "{evt}");
        assert!(evt.contains("ifcanseetarget"), "{evt}");
        assert!(evt.contains("ifactioncount 8"), "{evt}");
        assert!(evt.contains("ifspawnedby LIZTROOP"), "{evt}");
        assert!(evt.contains("ifactornotstayput"), "{evt}");
        assert!(evt.contains("ifaction STAND"), "{evt}");
        assert!(evt.contains("ifai AIZOMBIE"), "{evt}");
        assert!(evt.contains("ifmove FACEVELS"), "{evt}");
        assert!(evt.contains("state SEE"), "{evt}");
        assert!(evt.contains("ai SEE"), "{evt}");
        assert!(evt.contains("move SEE"), "{evt}");
        assert!(evt.contains("action SEE"), "{evt}");
        assert!(evt.contains("break"), "{evt}");
        assert!(evt.contains("palfrom 32"), "{evt}");
        assert!(evt.contains("addphealth 10"), "{evt}");
        assert!(evt.contains("addammo PISTOL_WEAPON 12"), "{evt}");
        assert!(evt.contains("addweapon RPG 1"), "{evt}");
        assert!(evt.contains("addinventory STEROIDS 1"), "{evt}");
        assert!(evt.contains("wackplayer"), "{evt}");
        assert!(evt.contains("pkick"), "{evt}");
        assert!(evt.contains("resetplayer"), "{evt}");
        assert!(evt.contains("endofgame"), "{evt}");
        assert!(evt.contains("tossweapon"), "{evt}");
        assert!(evt.contains("resetweapons"), "{evt}");
        assert!(evt.contains("resetinventory"), "{evt}");
        assert!(evt.contains("movesprite 256 0 0"), "{evt}");
        assert!(evt.contains("clipmove 256 0 0"), "{evt}");
        assert!(evt.contains("changespritestat 1"), "{evt}");
        assert!(evt.contains("wackmove 128 0 0"), "{evt}");
        assert!(evt.contains("respawnhitag 5"), "{evt}");
        assert!(evt.contains("setsprite 2304 -2048 0"), "{evt}");
        assert!(evt.contains("lockplayer 30"), "{evt}");
        assert!(evt.contains("setplayerangle 512"), "{evt}");
        assert!(evt.contains("setactorangle 1024"), "{evt}");
        assert!(evt.contains("getplayerangle"), "{evt}");
        assert!(evt.contains("getactorangle"), "{evt}");
        assert!(evt.contains("lotsofcolour 4"), "{evt}");
        assert!(evt.contains("zshoot RPG 256"), "{evt}");
        assert!(evt.contains("eshoot RPG"), "{evt}");
        assert!(evt.contains("ezshoot RPG 256"), "{evt}");
        assert!(evt.contains("pstomp"), "{evt}");
        assert!(evt.contains("findotherplayer"), "{evt}");
        assert!(evt.contains("debug"), "{evt}");
        assert!(evt.contains("getzrange"), "{evt}");
        assert!(evt.contains("updatesector"), "{evt}");
        assert!(evt.contains("getflorzofslope"), "{evt}");
        assert!(evt.contains("getceilzofslope"), "{evt}");
        assert!(evt.contains("quake 35"), "{evt}");
        assert!(evt.contains("addstrength 25"), "{evt}");
        assert!(evt.contains("strength 40"), "{evt}");
        assert!(evt.contains("sleeptime 30"), "{evt}");
        assert!(evt.contains("getlastpal"), "{evt}");
        assert!(evt.contains("ifactor LIZTROOP"), "{evt}");
    }

    #[test]
    fn move_definition_emits_movevel() {
        let con = ConData::parse(
            "define DUMMY 1680\n\
             move WALKENEMY 150 0 faceplayer\n\
             move FLEE 80 0 fleeenemy\n\
             move SPINNY 0 0 spin\n\
             move DUCK 0 0 dodgebullet\n\
             move SLIDE 0 0 geth getv\n\
             move RAND 0 0 randomangle\n\
             move SMART 0 0 faceplayersmart\n\
             move FAR 0 0 furthestdir\n\
             useractor enemy DUMMY 30\n\
             move WALKENEMY\n\
             move FLEE\n\
             move SPINNY\n\
             move DUCK\n\
             move SLIDE\n\
             move RAND\n\
             move SMART\n\
             move FAR\n\
             enda\n",
        );
        assert_eq!(
            con.moves["WALKENEMY"],
            MoveDef {
                vel: 150,
                angvel: 0,
                flags: MOVE_FACEPLAYER,
            }
        );
        assert_eq!(
            con.moves["FLEE"],
            MoveDef {
                vel: 80,
                angvel: 0,
                flags: MOVE_FLEEENEMY,
            }
        );
        assert_eq!(con.moves["SPINNY"].flags, MOVE_SPIN);
        assert_eq!(con.moves["DUCK"].flags, MOVE_DODGEBULLET);
        assert_eq!(con.moves["SLIDE"].flags, MOVE_GETH | MOVE_GETV);
        assert_eq!(con.moves["RAND"].flags, MOVE_RANDOMANGLE);
        assert_eq!(con.moves["SMART"].flags, MOVE_FACEPLAYERSMART);
        assert_eq!(con.moves["FAR"].flags, MOVE_FURTHESTDIR);
        let evt = con.conevt();
        assert!(evt.contains("move WALKENEMY"), "{evt}");
        assert!(evt.contains("movevel 150 0 1"), "{evt}");
        assert!(evt.contains("movevel 80 0 2"), "{evt}");
        assert!(evt.contains("movevel 0 0 16"), "{evt}");
        assert!(evt.contains("movevel 0 0 32"), "{evt}");
        assert!(evt.contains("movevel 0 0 384"), "{evt}");
        assert!(evt.contains("movevel 0 0 512"), "{evt}");
        assert!(evt.contains("movevel 0 0 1024"), "{evt}");
        assert!(evt.contains("movevel 0 0 2048"), "{evt}");
    }

    #[test]
    fn ifcount_compiles() {
        let con = ConData::parse(
            "definequote 1 WAIT\n\
             onevent EVENT_ENTERLEVEL\n ifcount 8 quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfCount(8), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifcount 8"));
    }

    #[test]
    fn ifcanseetarget_compiles() {
        let con = ConData::parse(
            "definequote 1 LOCKED\n\
             onevent EVENT_ENTERLEVEL\n ifcanseetarget quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfCanSeeTarget, ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifcanseetarget"));
    }

    #[test]
    fn ifactioncount_compiles() {
        let con = ConData::parse(
            "definequote 1 WAIT\n\
             onevent EVENT_ENTERLEVEL\n ifactioncount 8 quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfActionCount(8), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifactioncount 8"));
    }

    #[test]
    fn ifstrength_compiles() {
        let con = ConData::parse(
            "definequote 1 WEAK\n\
             onevent EVENT_ENTERLEVEL\n ifstrength 40 quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfStrength(40), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifstrength 40"));
    }

    #[test]
    fn ifspawnedby_compiles() {
        let con = ConData::parse(
            "definequote 1 FROM\n\
             onevent EVENT_ENTERLEVEL\n ifspawnedby LIZTROOP quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfSpawnedBy("LIZTROOP".into()), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifspawnedby LIZTROOP"));
    }

    #[test]
    fn ifactornotstayput_compiles() {
        let con = ConData::parse(
            "definequote 1 FREE\n\
             onevent EVENT_ENTERLEVEL\n ifactornotstayput quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfActorNotStayPut, ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifactornotstayput"));
    }

    #[test]
    fn ifaction_compiles() {
        let con = ConData::parse(
            "definequote 1 POSE\n\
             onevent EVENT_ENTERLEVEL\n ifaction STAND quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfAction("STAND".into()), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifaction STAND"));
    }

    #[test]
    fn ifai_compiles() {
        let con = ConData::parse(
            "definequote 1 HUNT\n\
             onevent EVENT_ENTERLEVEL\n ifai AIZOMBIE quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfAi("AIZOMBIE".into()), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifai AIZOMBIE"));
    }

    #[test]
    fn ifmove_compiles() {
        let con = ConData::parse(
            "definequote 1 GO\n\
             onevent EVENT_ENTERLEVEL\n ifmove FACEVELS quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfMove("FACEVELS".into()), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ifmove FACEVELS"));
    }

    #[test]
    fn state_compiles() {
        let con = ConData::parse(
            "definequote 1 GO\n\
             onevent EVENT_USE\n state SEE quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["USE"],
            vec![ConOp::State("SEE".into()), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("state SEE"));
    }

    #[test]
    fn state_block_records_its_action() {
        let con = ConData::parse(
            "action FIRE_FRAMES -1 14 1 1 1\n\
             state firestate\n\
               ifaction 0\n\
                 action FIRE_FRAMES\n\
             ends\n\
             define FIRE 2271\n\
             useractor notenemy FIRE 5 0 FIREVELS state firestate enda\n",
        );
        assert_eq!(
            con.state_action.get("FIRESTATE").map(String::as_str),
            Some("FIRE_FRAMES")
        );
        let actor = &con.actors[&2271];
        assert_eq!(actor.state.as_deref(), Some("FIRESTATE"));
        assert!(actor.action.is_none());
    }

    #[test]
    fn ai_compiles() {
        let con = ConData::parse(
            "definequote 1 GO\n\
             onevent EVENT_USE\n ai SEE quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["USE"],
            vec![ConOp::Ai("SEE".into()), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("ai SEE"));
    }

    #[test]
    fn move_compiles() {
        let con = ConData::parse(
            "definequote 1 GO\n\
             onevent EVENT_USE\n move SEE quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["USE"],
            vec![ConOp::Move("SEE".into()), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("move SEE"));
    }

    #[test]
    fn action_compiles() {
        let con = ConData::parse(
            "definequote 1 GO\n\
             onevent EVENT_USE\n action SEE quote 1\nendevent\n",
        );
        assert_eq!(
            con.events["USE"],
            vec![ConOp::Action("SEE".into()), ConOp::Quote(1)]
        );
        assert!(con.conevt().contains("action SEE"));
    }

    #[test]
    fn else_compiles() {
        let con = ConData::parse(
            "definequote 1 DEAD\n\
             definequote 2 ALIVE\n\
             onevent EVENT_ENTERLEVEL\n ifdead quote 1 else quote 2\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::IfDead, ConOp::Quote(1), ConOp::Else, ConOp::Quote(2)]
        );
        assert!(con.conevt().contains("else"));
    }

    #[test]
    fn break_compiles() {
        let con = ConData::parse(
            "definequote 1 HI\n\
             definequote 2 BYE\n\
             onevent EVENT_ENTERLEVEL\n quote 1 break quote 2\nendevent\n",
        );
        assert_eq!(
            con.events["ENTERLEVEL"],
            vec![ConOp::Quote(1), ConOp::Break, ConOp::Quote(2)]
        );
        assert!(con.conevt().contains("break"));
    }

    #[test]
    fn palfrom_compiles() {
        let con = ConData::parse("onevent EVENT_GETSHOT\n palfrom 32\nendevent\n");
        assert_eq!(con.events["GETSHOT"], vec![ConOp::PalFrom(32)]);
        assert!(con.conevt().contains("palfrom 32"));
    }

    #[test]
    fn addphealth_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n addphealth 10\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::AddHealth(10)]);
        assert!(con.conevt().contains("addphealth 10"));
    }

    #[test]
    fn addammo_compiles() {
        let con = ConData::parse(
            "define PISTOL_WEAPON 1\n\
             onevent EVENT_USE\n addammo PISTOL_WEAPON 12\nendevent\n",
        );
        assert_eq!(
            con.events["USE"],
            vec![ConOp::AddAmmo("PISTOL_WEAPON".into(), 12)]
        );
        assert!(con.conevt().contains("addammo PISTOL_WEAPON 12"));
    }

    #[test]
    fn addweapon_compiles() {
        let con = ConData::parse(
            "define SHOTGUN_WEAPON 2\n\
             onevent EVENT_USE\n addweapon SHOTGUN_WEAPON 8\nendevent\n",
        );
        assert_eq!(
            con.events["USE"],
            vec![ConOp::AddWeapon("SHOTGUN_WEAPON".into(), 8)]
        );
        assert!(con.conevt().contains("addweapon SHOTGUN_WEAPON 8"));
    }

    #[test]
    fn addinventory_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n addinventory FIRSTAID 1\nendevent\n");
        assert_eq!(
            con.events["USE"],
            vec![ConOp::AddInventory("FIRSTAID".into(), 1)]
        );
        assert!(con.conevt().contains("addinventory FIRSTAID 1"));
    }

    #[test]
    fn spawn_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n spawn BLOOD\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::Spawn("BLOOD".into())]);
        assert!(con.conevt().contains("spawn BLOOD"));
    }

    #[test]
    fn guts_compiles() {
        let con = ConData::parse("onevent EVENT_GETSHOT\n guts JIBS1 4\nendevent\n");
        assert_eq!(con.events["GETSHOT"], vec![ConOp::Guts("JIBS1".into(), 4)]);
        assert!(con.conevt().contains("guts JIBS1 4"));
    }

    #[test]
    fn shoot_compiles() {
        let con = ConData::parse("onevent EVENT_FIRE\n shoot RPG\nendevent\n");
        assert_eq!(con.events["FIRE"], vec![ConOp::Shoot("RPG".into())]);
        assert!(con.conevt().contains("shoot RPG"));
    }

    #[test]
    fn killit_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n killit\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::KillIt]);
        assert!(con.conevt().contains("killit"));
    }

    #[test]
    fn wackplayer_compiles() {
        let con = ConData::parse("onevent EVENT_GETSHOT\n wackplayer\nendevent\n");
        assert_eq!(con.events["GETSHOT"], vec![ConOp::WackPlayer]);
        assert!(con.conevt().contains("wackplayer"));
    }

    #[test]
    fn pkick_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n pkick\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::PKick]);
        assert!(con.conevt().contains("pkick"));
    }

    #[test]
    fn operate_compiles() {
        let con = ConData::parse("onevent EVENT_ENTERLEVEL\n operate\nendevent\n");
        assert_eq!(con.events["ENTERLEVEL"], vec![ConOp::Operate]);
        assert!(con.conevt().contains("operate"));
    }

    #[test]
    fn operateactivators_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n operateactivators 7\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::OperateActivators(7)]);
        assert!(con.conevt().contains("operateactivators 7"));
    }

    #[test]
    fn operatesectors_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n operatesectors\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::OperateSectors]);
        assert!(con.conevt().contains("operatesectors"));
    }

    #[test]
    fn money_compiles() {
        let con = ConData::parse("onevent EVENT_KILLIT\n money 4\nendevent\n");
        assert_eq!(con.events["KILLIT"], vec![ConOp::Money(4)]);
        assert!(con.conevt().contains("money 4"));
    }

    #[test]
    fn lotsofglass_compiles() {
        let con = ConData::parse("onevent EVENT_GETSHOT\n lotsofglass 8\nendevent\n");
        assert_eq!(con.events["GETSHOT"], vec![ConOp::LotsOfGlass(8)]);
        assert!(con.conevt().contains("lotsofglass 8"));
    }

    #[test]
    fn spritepal_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n spritepal 21\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::SpritePal(21)]);
        assert!(con.conevt().contains("spritepal 21"));
    }

    #[test]
    fn sizeat_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n sizeat 32 32\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::SizeAt(32, 32)]);
        assert!(con.conevt().contains("sizeat 32 32"));
    }

    #[test]
    fn cstat_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n cstat 32768\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::Cstat(32768)]);
        assert!(con.conevt().contains("cstat 32768"));
    }

    #[test]
    fn cstator_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n cstator 2\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::CstatOr(2)]);
        assert!(con.conevt().contains("cstator 2"));
    }

    #[test]
    fn addstrength_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n addstrength 25\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::AddStrength(25)]);
        assert!(con.conevt().contains("addstrength 25"));
    }

    #[test]
    fn strength_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n strength 40\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::Strength(40)]);
        assert!(con.conevt().contains("strength 40"));
    }

    #[test]
    fn clipdist_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n clipdist 128\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::ClipDist(128)]);
        assert!(con.conevt().contains("clipdist 128"));
    }

    #[test]
    fn cactor_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n cactor LIZTROOP\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::Cactor("LIZTROOP".into())]);
        assert!(con.conevt().contains("cactor LIZTROOP"));
    }

    #[test]
    fn sizeto_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n sizeto 32 32\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::SizeTo(32, 32)]);
        assert!(con.conevt().contains("sizeto 32 32"));
    }

    #[test]
    fn count_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n count 8\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::Count(8)]);
        assert!(con.conevt().contains("count 8"));
    }

    #[test]
    fn resetcount_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n resetcount\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::ResetCount]);
        assert!(con.conevt().contains("resetcount"));
    }

    #[test]
    fn actioncount_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n actioncount 8\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::ActionCount(8)]);
        assert!(con.conevt().contains("actioncount 8"));
    }

    #[test]
    fn resetactioncount_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n resetactioncount\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::ResetActionCount]);
        assert!(con.conevt().contains("resetactioncount"));
    }

    #[test]
    fn addkills_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n addkills 3\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::AddKills(3)]);
        assert!(con.conevt().contains("addkills 3"));
    }

    #[test]
    fn hitradius_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n hitradius 4096 50\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::HitRadius(4096, 50)]);
        assert!(con.conevt().contains("hitradius 4096 50"));
    }

    #[test]
    fn fall_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n fall\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::Fall]);
        assert!(con.conevt().contains("fall"));
    }

    #[test]
    fn sleeptime_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n sleeptime 30\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::SleepTime(30)]);
        assert!(con.conevt().contains("sleeptime 30"));
    }

    #[test]
    fn getlastpal_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n getlastpal\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::GetLastPal]);
        assert!(con.conevt().contains("getlastpal"));
    }

    #[test]
    fn debris_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n debris SCRAP 3\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::Debris("SCRAP".into(), 3)]);
        assert!(con.conevt().contains("debris SCRAP 3"));
    }

    #[test]
    fn mail_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n mail 4\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::Mail(4)]);
        assert!(con.conevt().contains("mail 4"));
    }

    #[test]
    fn paper_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n paper 4\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::Paper(4)]);
        assert!(con.conevt().contains("paper 4"));
    }

    #[test]
    fn tip_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n tip\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::Tip]);
        assert!(con.conevt().contains("tip"));
    }

    #[test]
    fn mikesnd_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n mikesnd\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::MikeSnd]);
        assert!(con.conevt().contains("mikesnd"));
    }

    #[test]
    fn resetplayer_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n resetplayer\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::ResetPlayer]);
        assert!(con.conevt().contains("resetplayer"));
    }

    #[test]
    fn endofgame_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n endofgame\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::EndOfGame]);
        assert!(con.conevt().contains("endofgame"));
    }

    #[test]
    fn tossweapon_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n tossweapon\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::TossWeapon]);
        assert!(con.conevt().contains("tossweapon"));
    }

    #[test]
    fn resetweapons_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n resetweapons\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::ResetWeapons]);
        assert!(con.conevt().contains("resetweapons"));
    }

    #[test]
    fn resetinventory_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n resetinventory\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::ResetInventory]);
        assert!(con.conevt().contains("resetinventory"));
    }

    #[test]
    fn movesprite_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n movesprite 256 0 0\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::MoveSprite(256, 0, 0)]);
        assert!(con.conevt().contains("movesprite 256 0 0"));
    }

    #[test]
    fn clipmove_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n clipmove 256 0 0\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::ClipMove(256, 0, 0)]);
        assert!(con.conevt().contains("clipmove 256 0 0"));
    }

    #[test]
    fn changespritestat_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n changespritestat 1\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::ChangeSpriteStat(1)]);
        assert!(con.conevt().contains("changespritestat 1"));
    }

    #[test]
    fn wackmove_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n wackmove 256 0 0\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::WackMove(256, 0, 0)]);
        assert!(con.conevt().contains("wackmove 256 0 0"));
    }

    #[test]
    fn respawnhitag_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n respawnhitag 7\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::RespawnHitag(7)]);
        assert!(con.conevt().contains("respawnhitag 7"));
    }

    #[test]
    fn setsprite_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n setsprite 2304 -2048 0\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::SetSprite(2304, -2048, 0)]);
        assert!(con.conevt().contains("setsprite 2304 -2048 0"));
    }

    #[test]
    fn lockplayer_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n lockplayer 30\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::LockPlayer(30)]);
        assert!(con.conevt().contains("lockplayer 30"));
    }

    #[test]
    fn setplayerangle_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n setplayerangle 512\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::SetPlayerAngle(512)]);
        assert!(con.conevt().contains("setplayerangle 512"));
    }

    #[test]
    fn quake_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n quake 35\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::Quake(35)]);
        assert!(con.conevt().contains("quake 35"));
    }

    #[test]
    fn soundonce_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n soundonce PISTOL_FIRE\nendevent\n");
        assert_eq!(
            con.events["USE"],
            vec![ConOp::SoundOnce("PISTOL_FIRE".into())]
        );
        assert!(con.conevt().contains("soundonce duke/pistol_fire"));
    }

    #[test]
    fn spritesound_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n spritesound PISTOL_FIRE\nendevent\n");
        assert_eq!(
            con.events["USE"],
            vec![ConOp::SpriteSound("PISTOL_FIRE".into())]
        );
        assert!(con.conevt().contains("spritesound duke/pistol_fire"));
    }

    #[test]
    fn stopsound_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n stopsound PISTOL_FIRE\nendevent\n");
        assert_eq!(
            con.events["USE"],
            vec![ConOp::StopSound("PISTOL_FIRE".into())]
        );
        assert!(con.conevt().contains("stopsound duke/pistol_fire"));
    }

    #[test]
    fn setactorangle_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n setactorangle 1024\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::SetActorAngle(1024)]);
        assert!(con.conevt().contains("setactorangle 1024"));
    }

    #[test]
    fn getplayerangle_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n getplayerangle\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::GetPlayerAngle]);
        assert!(con.conevt().contains("getplayerangle"));
    }

    #[test]
    fn getactorangle_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n getactorangle\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::GetActorAngle]);
        assert!(con.conevt().contains("getactorangle"));
    }

    #[test]
    fn lotsofcolour_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n lotsofcolour 4\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::LotsOfColour(4)]);
        assert!(con.conevt().contains("lotsofcolour 4"));
    }

    #[test]
    fn zshoot_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n zshoot RPG 256\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::ZShoot("RPG".into(), 256)]);
        assert!(con.conevt().contains("zshoot RPG 256"));
    }

    #[test]
    fn eshoot_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n eshoot RPG\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::EShoot("RPG".into())]);
        assert!(con.conevt().contains("eshoot RPG"));
    }

    #[test]
    fn ezshoot_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n ezshoot RPG 256\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::EzShoot("RPG".into(), 256)]);
        assert!(con.conevt().contains("ezshoot RPG 256"));
    }

    #[test]
    fn pstomp_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n pstomp\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::PStomp]);
        assert!(con.conevt().contains("pstomp"));
    }

    #[test]
    fn findplayer_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n findplayer\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::FindPlayer]);
        assert!(con.conevt().contains("findplayer"));
    }

    #[test]
    fn debug_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n debug\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::Debug]);
        assert!(con.conevt().contains("debug"));
    }

    #[test]
    fn findotherplayer_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n findotherplayer\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::FindOtherPlayer]);
        assert!(con.conevt().contains("findotherplayer"));
    }

    #[test]
    fn getzrange_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n getzrange\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::GetZRange]);
        assert!(con.conevt().contains("getzrange"));
    }

    #[test]
    fn updatesector_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n updatesector\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::UpdateSector]);
        assert!(con.conevt().contains("updatesector"));
    }

    #[test]
    fn getflorzofslope_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n getflorzofslope\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::GetFlorZOfSlope]);
        assert!(con.conevt().contains("getflorzofslope"));
    }

    #[test]
    fn getceilzofslope_compiles() {
        let con = ConData::parse("onevent EVENT_USE\n getceilzofslope\nendevent\n");
        assert_eq!(con.events["USE"], vec![ConOp::GetCeilZOfSlope]);
        assert!(con.conevt().contains("getceilzofslope"));
    }
}
