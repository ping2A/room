//! Deterministic storage for EDuke32 CON variables and arrays (ED2 foundation).

use super::compile::EDUKE_MAX_QUOTES;
use super::{ConDialect, EdArrayDefinition, EdVariableDefinition, ScriptImage};
use room_build_engine::helpers::{getangle, sincos_q14};
use room_build_engine::numeric::{divscale, krand_step, mulscale};
use room_build_engine::world::{
    HudGameTextDraw, HudNumberDraw, HudSpriteDraw, HudTextDraw, MAXSECTORS, MAXSPRITES, MAXWALLS,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write as _;
use thiserror::Error;

/// EDuke32 `MAXPLAYERS` storage extent used by CON per-player variables.
pub const EDUKE_MAXPLAYERS: usize = 16;
const NO_EVENT_CONTEXT: usize = usize::MAX;

/// Maximum formatted text size accepted by the EDuke32 VM.
const EDUKE_MAX_QUOTE_BYTES: usize = 4096;
/// EDuke32's initial shared CON game-random seed.
const EDUKE_INITIAL_GAME_RANDOM_SEED: i32 = 1996;
/// Bound recursive `EVENT_EGS` / `EVENT_SPAWN` dispatch from runtime spawns.
const EDUKE_SPAWN_EVENT_DEPTH_LIMIT: usize = 32;

/// Upper bound for total script-owned values allocated from an untrusted CON image.
const MAX_EDUKE_VALUE_CELLS: usize = 16 * 1024 * 1024;
/// Maximum number of uncollected CON `echo` messages retained by a VM.
const MAX_EDUKE_CONSOLE_MESSAGES: usize = 256;

/// Per-variable storage selected by EDuke32 gamevar flag bits 0 and 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdVariableScope {
    /// One value shared by the whole game.
    Global,
    /// One value per `MAXPLAYERS` player slot.
    Player,
    /// One value per Build sprite slot.
    Actor,
}

/// Deterministic integer operators used by EDuke32 CON arithmetic commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdBinaryOperator {
    /// 32-bit wrapping addition.
    Add,
    /// 32-bit wrapping subtraction.
    Subtract,
    /// 32-bit wrapping multiplication.
    Multiply,
    /// Signed integer division.
    Divide,
    /// Signed integer remainder.
    Modulo,
    /// Bitwise AND.
    And,
    /// Bitwise OR.
    Or,
    /// Bitwise XOR.
    Xor,
    /// Left shift with the count masked to 0–31.
    ShiftLeft,
    /// Arithmetic right shift with the count masked to 0–31.
    ShiftRight,
}

/// Integer comparison supported by EDuke32 conditional commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdComparison {
    /// Equality (`ife`).
    Equal,
    /// Inequality (`ifn`).
    NotEqual,
    /// Less than (`ifl`).
    Less,
    /// Less than or equal (`ifle`).
    LessEqual,
    /// Greater than (`ifg`).
    Greater,
    /// Greater than or equal (`ifge`).
    GreaterEqual,
    /// Nonzero bitwise intersection (`ifand`).
    BitAndNonZero,
    /// Zero bitwise intersection (the inverse of `ifand`).
    BitAndZero,
    /// At least one operand is nonzero (`ifor`).
    EitherNonZero,
    /// Both operands are zero (the inverse of `ifor`).
    BothZero,
    /// The operands have a nonzero bitwise XOR (`ifxor`).
    BitXorNonZero,
    /// The operands have no differing bit (`ifxor` inverse).
    BitXorZero,
    /// Both operands are nonzero (`ifboth`).
    BothNonZero,
    /// At least one operand is zero (`ifboth` inverse).
    EitherZero,
    /// Unsigned greater than (`ifa`).
    UnsignedGreater,
    /// Unsigned greater than or equal (`ifae`).
    UnsignedGreaterEqual,
    /// Unsigned less than (`ifb`).
    UnsignedLess,
    /// Unsigned less than or equal (`ifbe`).
    UnsignedLessEqual,
}

/// EDuke32 native structure addressed by `get*` / `set*` instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdStructKind {
    /// Sprite, actor runtime, and sprite extension (`getactor` / `geta`).
    Actor,
    /// Build sector (`getsector` / `gets`).
    Sector,
    /// Build wall (`getwall` / `getw`).
    Wall,
    /// Player runtime (`getplayer` / `getp`).
    Player,
    /// Engine input state (`getinput` / `geti`).
    Input,
    /// User preferences and game settings (`getuserdef` / `getu`).
    UserDef,
    /// ART tile metadata (`gettiledata`).
    TileData,
    /// Temporary renderer sprite (`gettspr`).
    Tspr,
    /// Projectile definition (`getprojectile`).
    Projectile,
    /// Active projectile sprite (`getthisprojectile`).
    ThisProjectile,
}

/// Build sprite-list query/iteration operation used by EDuke32 CON commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdSpriteListKind {
    /// Read the head sprite for a status list.
    HeadStatus,
    /// Read the next sprite in a status list.
    NextStatus,
    /// Read the head sprite for a sector list.
    HeadSector,
    /// Read the next sprite in a sector list.
    NextSector,
}

/// Playback routing for EDuke32 CON sound commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdSoundMode {
    /// Play from the current actor's position.
    Actor,
    /// Play from the actor if the sound is not already active.
    ActorOnce,
    /// Play at the active local player's position without distance attenuation.
    Global,
}

/// Loose item effect spawned by the classic `money`, `mail` and `paper` commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdItemSprayKind {
    /// Dollar bills (Atomic tile 1233).
    Money,
    /// Envelopes (Atomic tile 4410).
    Mail,
    /// Paper pages (Atomic tile 4460).
    Paper,
}

/// Live sprite member targeted by classic EDuke actor commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdActorMember {
    /// Build sprite `extra` (health / strength).
    Strength,
    /// Add to Build sprite `extra` (`addstrength`).
    AddStrength,
    /// Build sprite `cstat`.
    Cstat,
    /// OR bits into Build sprite `cstat` (`cstator`).
    CstatOr,
    /// Build sprite `picnum` (`cactor`).
    Tile,
    /// Build sprite palette (`spritepal`).
    Palette,
    /// Build sprite `clipdist`.
    ClipDistance,
    /// Actor T0 (`count`).
    Count,
    /// Actor sleep countdown (`sleeptime`).
    SleepTime,
}

/// A value read by the EDuke32 VM core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdOperand {
    /// Immediate signed integer.
    Immediate(i32),
    /// Current actor/sprite index for `THISACTOR`.
    CurrentActor,
    /// Current player slot for `THISPLAYER`.
    CurrentPlayer,
    /// Nearest player distance for the active actor (`g_x` in EDuke32).
    PlayerDistance,
    /// Absolute Build-angle difference between the active player and actor.
    ActorPlayerAngleDifference,
    /// Floor-to-ceiling gap at the active actor, in Build vertical units.
    ActorSectorGap,
    /// Active actor center-to-floor distance in Build units.
    ActorFloorDistance,
    /// Active actor center-to-ceiling distance in Build units.
    ActorCeilingDistance,
    /// Result of the actor's `ifpinventory` test, including palette-based keys.
    PlayerInventoryDiffers {
        /// Inventory item id (0–10).
        item: Box<EdOperand>,
        /// Expected inventory value; access-card tests ignore it.
        amount: Box<EdOperand>,
    },
    /// Result of evaluating EDuke32 `ifp` player-state flags.
    PlayerFlags(i32),
    /// Whether the active player's sync input contains the requested bits.
    PlayerInputBits(i32),
    /// Result of the classic actor `ifdead` health test.
    ActorDead,
    /// Result of the live sector/palette `ifsquished` test.
    ActorSquished,
    /// Result of `ifhitweapon`, which consumes queued actor damage.
    ActorHitByWeapon,
    /// Line of sight from the active actor to the active player (`ifcansee`).
    ActorCanSeePlayer,
    /// Gamevar value using the active player and actor context.
    Variable(String),
    /// Array value with a dynamically evaluated index.
    ArrayElement {
        /// Array identifier.
        name: String,
        /// Element expression.
        index: Box<EdOperand>,
    },
    /// Native structure field read through the active live host bridge.
    StructMember {
        /// Structure family.
        kind: EdStructKind,
        /// Structure instance id.
        id: Box<EdOperand>,
        /// Structure member name.
        member: String,
        /// Optional member-array subscript.
        member_index: Option<Box<EdOperand>>,
    },
    /// Direct actor-to-player visibility check used by `ifcanseetarget`.
    ActorCanSeeTarget,
    /// Whether the current actor has a clear firing corridor to its target.
    ActorCanShootTarget,
    /// Whether a live projectile is travelling close enough to threaten the actor.
    ActorBulletNear,
    /// Whether the active actor's monster, inventory, or item respawn option is enabled.
    ActorRespawnEnabled,
    /// Whether the active actor has no stay-put sector restriction.
    ActorNotStayPut,
    /// Test or register the active actor in the player's co-op weapon table.
    ActorWeaponReservation {
        /// Nonzero registers this actor's tile before testing its owner.
        register: Box<EdOperand>,
    },
    /// Whether the current actor is in an above-water sector near its floor.
    ActorOnWater,
    /// Whether the actor has at least 108 Build units of room in four diagonals.
    ActorAwayFromWall,
    /// Whether the actor's sector uses the Atomic outer-space floor.
    ActorInOuterSpace,
    /// Whether this actor owns a currently active sound voice.
    ActorHasActiveSound,
    /// Whether a selected actor currently owns a selected sound voice.
    ActorHasSound {
        /// Sprite whose sound ownership is checked.
        actor: Box<EdOperand>,
        /// Sound id to inspect.
        sound: Box<EdOperand>,
    },
}

/// A writable EDuke32 VM destination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdLValue {
    /// Gamevar selected by the active player and actor context.
    Variable(String),
    /// Array element selected by a dynamically evaluated index.
    ArrayElement {
        /// Array identifier.
        name: String,
        /// Element expression.
        index: Box<EdOperand>,
    },
}

/// Partial EDuke32 VM instruction emitted by the source compiler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdInstruction {
    /// Preserve an instruction boundary for a host command not yet handled by
    /// the EDuke32 VM core.
    NoOp,
    /// Select EDuke32's per-command HUD interpolation identity.
    SetHudUniqueId(EdOperand),
    /// Capture one screen-space ART draw for the current render event.
    RotateSprite(Vec<EdOperand>),
    /// Capture a quote rendered with EDuke32's small HUD font.
    Minitext {
        /// Top-left X coordinate.
        x: EdOperand,
        /// Top-left Y coordinate.
        y: EdOperand,
        /// Quote slot.
        quote: EdOperand,
        /// Build shade value.
        shade: EdOperand,
        /// Palette lookup id.
        palette: EdOperand,
    },
    /// Capture one quote rendered with EDuke32's large game font.
    GameText(Vec<EdOperand>),
    /// Capture one EDuke32 `digitalnumber` or `digitalnumberz` command.
    DigitalNumber(Vec<EdOperand>),
    /// End the current event handler while allowing later appended handlers.
    EndEvent,
    /// Delete or hide the current actor and stop the current script body.
    KillIt,
    /// Spawn the count of randomized debris sprites from the current actor.
    Debris {
        /// Fragment tile base.
        tile: EdOperand,
        /// Number of fragments.
        count: EdOperand,
    },
    /// Spawn randomized gore fragments from the current actor.
    Guts {
        /// Fragment tile.
        tile: EdOperand,
        /// Number of fragments.
        count: EdOperand,
    },
    /// Spawn a randomized glass burst from the current actor.
    LotsOfGlass {
        /// Number of fragments.
        count: EdOperand,
    },
    /// Spawn randomized glass fragments along a sector's ceiling perimeter.
    SpawnCeilingGlass {
        /// Sprite used as the fragment owner.
        sprite: EdOperand,
        /// Sector whose ceiling perimeter emits fragments.
        sector: EdOperand,
        /// Requested count per wall (EDuke32 emits count plus one).
        count: EdOperand,
    },
    /// Spawn the requested number of money/mail/paper sprites.
    ItemSpray {
        /// Item kind and its Atomic default tile.
        kind: EdItemSprayKind,
        /// Number of sprites to spawn.
        count: EdOperand,
    },
    /// Queue the current actor's nearby-sector operation for the session phase.
    Operate,
    /// Arm every untriggered masterswitch with the requested lotag.
    OperateMasterSwitches {
        /// Masterswitch lotag to match.
        lotag: EdOperand,
    },
    /// Activate matching Build respawn markers for the current actor's tag.
    RespawnHitag,
    /// Request a single-player map restart and stop this event chain.
    ResetPlayer,
    /// Apply a four-band CON blast to actors, players and nearby walls.
    HitRadius {
        /// Blast radius in Build distance units.
        radius: EdOperand,
        /// Damage bands, from farthest to nearest.
        damage: [EdOperand; 4],
    },
    /// Request the current actor's nearest player to perform a quick kick.
    PlayerQuickKick,
    /// Request the current actor's nearest player to start a stomp attack.
    PlayerStomp,
    /// Request the current actor's nearest player view to tilt.
    PlayerWack,
    /// Drop the nearest player's current weapon using the shared Build RNG.
    TossWeapon,
    /// Queue an actor or global sound through the live session audio bridge.
    PlaySound {
        /// Sound definition ID.
        sound: EdOperand,
        /// Actor or global playback policy.
        mode: EdSoundMode,
        /// Explicit source sprite for EDuke32 `actorsound`; otherwise `THISACTOR`.
        actor: Option<EdOperand>,
    },
    /// Adjust the pitch of an active actor sound by cents.
    SetActorSoundPitch {
        /// Actor source ID, with `-1` selecting all sources.
        actor: EdOperand,
        /// Sound definition ID.
        sound: EdOperand,
        /// Additional pitch offset in 1/100 semitone units.
        pitch_offset: EdOperand,
    },
    /// Stop every active voice for a sound ID.
    StopSound {
        /// Sound definition ID.
        sound: EdOperand,
        /// Explicit source sprite for EDuke32 `stopactorsound`; otherwise `THISACTOR`.
        actor: Option<EdOperand>,
    },
    /// Add health to the current/nearest player while respecting health caps.
    AddPlayerHealth {
        /// Health delta.
        amount: EdOperand,
    },
    /// Grant ammunition or a weapon to the nearest player in this actor's context.
    PlayerWeapon {
        /// Whether this is `addweapon` instead of `addammo`.
        add_weapon: bool,
        /// Weapon ID.
        weapon: EdOperand,
        /// Ammunition delta.
        ammo: EdOperand,
    },
    /// Set player inventory, add armor, or grant the actor-palette access key.
    PlayerInventory {
        /// Inventory ID (EDuke32 GET_* value).
        item: EdOperand,
        /// Exact counter value or armor delta.
        amount: EdOperand,
    },
    /// Select an actor action label and reset its frame counters.
    SetActorAction {
        /// Action-table word pointer.
        action: EdOperand,
    },
    /// Select an actor movement label and apply its flags.
    SetActorMove {
        /// Move-table word pointer.
        movement: EdOperand,
        /// Movement flags, including `random_angle`.
        flags: EdOperand,
    },
    /// Select an AI label, loading its action, move and flags.
    SetActorAi {
        /// AI-table word pointer.
        ai: EdOperand,
    },
    /// Write an actor's strength, cstat, tile or palette field.
    SetActorMember {
        /// Sprite member selected by the source command.
        member: EdActorMember,
        /// Value to write.
        value: EdOperand,
    },
    /// Credit the active player and release the actor's stay-put restriction.
    AddActorKills {
        /// Number of kills to add, narrowed to the Atomic counter width.
        amount: EdOperand,
    },
    /// Restore the saved pre-`spritepal` color using `getlastpal`.
    RestoreActorPalette,
    /// Move actor repeats toward a target size using vanilla `sizeto` rules.
    SetActorSizeTo {
        /// Target X repeat.
        x: EdOperand,
        /// Target Y repeat.
        y: EdOperand,
    },
    /// Apply vanilla CON gravity/floor snapping to the actor immediately.
    FallActor,
    /// Reset T0 or the action frame counter T2.
    ResetActorCounter {
        /// True for T2 (`resetactioncount`), false for T0 (`resetcount`).
        action: bool,
    },
    /// Set actor repeats immediately (`sizeat`).
    SetActorSize {
        /// X repeat.
        x: EdOperand,
        /// Y repeat.
        y: EdOperand,
    },
    /// Assign a value to a gamevar or array element.
    Assign {
        /// Destination.
        target: EdLValue,
        /// Source value.
        value: EdOperand,
    },
    /// Change the length of a declared gamearray, zero-filling any new cells.
    ResizeArray {
        /// Array identifier.
        name: String,
        /// Requested length.
        length: EdOperand,
    },
    /// Apply an integer operation to a gamevar or array element.
    Binary {
        /// Destination, whose current value is the left operand.
        target: EdLValue,
        /// Operator.
        operator: EdBinaryOperator,
        /// Right operand.
        right: EdOperand,
    },
    /// Jump to `target` when the comparison evaluates to true.
    JumpIf {
        /// Left operand.
        left: EdOperand,
        /// Comparison.
        comparison: EdComparison,
        /// Right operand.
        right: EdOperand,
        /// Instruction index to jump to when true.
        target: usize,
    },
    /// Consume one gameplay random byte and skip the branch when it exceeds
    /// the inclusive EDuke32 `ifrnd` threshold.
    IfRandom {
        /// Probability threshold; values below 0 never pass and values >=255 always pass.
        threshold: EdOperand,
        /// Instruction index for the `else` branch, or after the conditional.
        target: usize,
    },
    /// Unconditional jump (used to compile loops and branch tails).
    Jump {
        /// Instruction index to jump to.
        target: usize,
    },
    /// Select a switch branch. Case values are compared in declaration order.
    Switch {
        /// Value to compare.
        value: EdOperand,
        /// `(case value, instruction index)` arms.
        cases: Vec<(i32, usize)>,
        /// Target when no case matches.
        default_target: usize,
    },
    /// Call a named state and return to the next instruction afterwards.
    CallState {
        /// Exact state label spelling.
        name: String,
    },
    /// Return from a state, or finish the event when no state is active.
    Return,
    /// Replace a quote string at runtime (`redefinequote`).
    SetQuote {
        /// Quote slot.
        quote: usize,
        /// Replacement text.
        text: String,
    },
    /// Format a quote into another preallocated quote slot (`qsprintf`).
    Qsprintf {
        /// Destination quote slot.
        output_quote: usize,
        /// Source format quote slot.
        format_quote: usize,
        /// Integer operands inserted for `%d` placeholders.
        arguments: Vec<EdOperand>,
    },
    /// Read one native structure member into a gamevar/array element.
    GetStructMember {
        /// Structure to access.
        kind: EdStructKind,
        /// Structure instance ID.
        id: EdOperand,
        /// Member identifier as spelled in the source.
        member: String,
        /// Optional subscript for array-valued members.
        member_index: Option<EdOperand>,
        /// Destination gamevar/array element.
        target: EdLValue,
    },
    /// Write one writable native structure member from a gamevar/immediate.
    SetStructMember {
        /// Structure to access.
        kind: EdStructKind,
        /// Structure instance ID.
        id: EdOperand,
        /// Member identifier as spelled in the source.
        member: String,
        /// Optional subscript for array-valued members.
        member_index: Option<EdOperand>,
        /// Value to write.
        value: EdOperand,
    },
    /// Read a player- or actor-scoped gamevar using an explicit instance ID.
    GetScopedVariable {
        /// Declared storage scope.
        scope: EdVariableScope,
        /// Gamevar name.
        name: String,
        /// Player or actor ID expression.
        id: EdOperand,
        /// Destination gamevar/array element.
        target: EdLValue,
    },
    /// Write a player- or actor-scoped gamevar using an explicit instance ID.
    SetScopedVariable {
        /// Declared storage scope.
        scope: EdVariableScope,
        /// Gamevar name.
        name: String,
        /// Player or actor ID expression.
        id: EdOperand,
        /// Value to write.
        value: EdOperand,
    },
    /// Write masked random bits using the gameplay or display stream.
    RandomBits {
        /// Destination gamevar or array element.
        target: EdLValue,
        /// Mask applied to the random word (`word & mask`).
        mask: EdOperand,
        /// Select the independent display stream when true.
        display: bool,
    },
    /// Assign a value uniformly from zero through the evaluated maximum.
    RandomRange {
        /// Destination gamevar or array element.
        target: EdLValue,
        /// Inclusive maximum, or EDuke32's 32767 display maximum when absent.
        maximum: Option<EdOperand>,
        /// Select the independent display stream when true.
        display: bool,
    },
    /// Set a gamevar or array element to `(left * right) >> shift`.
    MulScale {
        /// Destination.
        target: EdLValue,
        /// Left operand.
        left: EdOperand,
        /// Right operand.
        right: EdOperand,
        /// Shift count.
        shift: u32,
    },
    /// Set a gamevar or array element to `(left << shift) / right`.
    DivScale {
        /// Destination.
        target: EdLValue,
        /// Left operand.
        left: EdOperand,
        /// Right operand.
        right: EdOperand,
        /// Shift count.
        shift: u32,
    },
    /// Set a gamevar or array element to `value * numerator / denominator`.
    ScaleVar {
        /// Destination.
        target: EdLValue,
        /// Value operand.
        value: EdOperand,
        /// Numerator operand.
        numerator: EdOperand,
        /// Denominator operand.
        denominator: EdOperand,
    },
    /// Write Build sine-table Q14 result to a destination.
    BuildSin {
        /// Destination.
        target: EdLValue,
        /// Build angle in 0–2047 units.
        angle: EdOperand,
    },
    /// Write Build cosine-table Q14 result to a destination.
    BuildCos {
        /// Destination.
        target: EdLValue,
        /// Build angle in 0–2047 units.
        angle: EdOperand,
    },
    /// Write Build angle in 0–2047 units for `(x, y)` to a destination.
    BuildGetAngle {
        /// Destination.
        target: EdLValue,
        /// X component.
        x: EdOperand,
        /// Y component.
        y: EdOperand,
    },
    /// Intersect a 3D segment with the vertical plane through an XY segment.
    LineIntersect {
        /// Origin `(x, y, z)`.
        origin: [EdOperand; 3],
        /// Destination `(x, y, z)`.
        destination: [EdOperand; 3],
        /// XY endpoints defining the vertical plane.
        line: [EdOperand; 4],
        /// Output intersection X, Y, Z, and boolean result destinations.
        outputs: [EdLValue; 4],
    },
    /// Read a sector's floor or ceiling plane at a Build-space position.
    GetSlopeZ {
        /// Destination.
        target: EdLValue,
        /// Sector ID expression.
        sector: EdOperand,
        /// X position.
        x: EdOperand,
        /// Y position.
        y: EdOperand,
        /// True to return the ceiling plane, false for the floor plane.
        ceiling: bool,
    },
    /// Resolve the owning sector for a wall index, writing `-1` when unowned.
    SectorOfWall {
        /// Output gamevar or array element.
        target: EdLValue,
        /// Wall ID to locate.
        wall: EdOperand,
    },
    /// Test Build visibility between two positions and sectors.
    CanSee {
        /// Destination receiving zero or one.
        target: EdLValue,
        /// First endpoint `(x, y, z, sector)`.
        start: [EdOperand; 4],
        /// Second endpoint `(x, y, z, sector)`.
        end: [EdOperand; 4],
    },
    /// Cast an EDuke32 Build hitscan and write its six result fields.
    HitScan {
        /// Origin and sector followed by the three direction components.
        ray: [EdOperand; 7],
        /// Outputs in hit-sector, hit-wall, hit-sprite, X, Y, Z order.
        outputs: [EdLValue; 6],
        /// Build wall/sprite clip mask.
        clipmask: EdOperand,
    },
    /// Query the Build Z range and write ceiling/floor values and hit codes.
    GetZRange {
        /// X, Y, Z and sector operands.
        point: [EdOperand; 4],
        /// Ceiling Z, ceiling hit, floor Z and floor hit destinations.
        outputs: [EdLValue; 4],
        /// Build-space horizontal collision radius.
        wall_distance: EdOperand,
        /// Build wall/sprite clip mask.
        clipmask: EdOperand,
    },
    /// Begin iterating a snapshot of all active sprites.
    ForAllSpritesBegin {
        /// Gamevar or array receiving each sprite ID.
        target: EdLValue,
        /// Instruction after the paired `ForAllSpritesNext`.
        end: usize,
    },
    /// Advance an active-sprite iterator or finish it.
    ForAllSpritesNext {
        /// Gamevar or array receiving each sprite ID.
        target: EdLValue,
        /// Paired begin instruction.
        begin: usize,
    },
    /// Find the Build sector containing a point, updating an in/out sector
    /// destination. A Z coordinate selects this `updatesectorz` variant.
    UpdateSector {
        /// Destination gamevar/array element, also used as the sector hint.
        target: EdLValue,
        /// Build-space X position.
        x: EdOperand,
        /// Build-space Y position.
        y: EdOperand,
        /// Optional Build-space Z position for vertical/stacked-sector checks.
        z: Option<EdOperand>,
    },
    /// Measure the 2D (`ldist`) or 3D (`dist`) distance between sprites.
    SpriteDistance {
        /// Destination gamevar/array element.
        target: EdLValue,
        /// First sprite index.
        first: EdOperand,
        /// Second sprite index.
        second: EdOperand,
        /// Include vertical distance when true.
        three_dimensional: bool,
    },
    /// Find an active actor of a tile within a strict three-dimensional radius.
    FindNearActor3D {
        /// Search tile number.
        tile: EdOperand,
        /// Maximum distance; candidates at this exact distance are excluded.
        distance: EdOperand,
        /// Destination gamevar or array element receiving the sprite id.
        target: EdLValue,
    },
    /// Read a sprite-list head or next link into a gamevar/array element.
    SpriteListRead {
        /// Destination gamevar/array element.
        target: EdLValue,
        /// Selected list and query operation.
        kind: EdSpriteListKind,
        /// Status, sector, or sprite index depending on `kind`.
        index: EdOperand,
    },
    /// Move a live sprite to another Build status or sector list.
    SpriteListChange {
        /// Sprite index to move.
        sprite: EdOperand,
        /// New status or sector index.
        index: EdOperand,
        /// True for a status-list change, false for a sector-list change.
        status: bool,
    },
    /// Move a live sprite to Build-space coordinates and update its sector.
    SetSpritePosition {
        /// Sprite index to move.
        sprite: EdOperand,
        /// Build-space X coordinate.
        x: EdOperand,
        /// Build-space Y coordinate.
        y: EdOperand,
        /// Build-space Z coordinate.
        z: EdOperand,
    },
    /// Spawn an EDuke32 CON actor at the current actor's location.
    SpawnActor {
        /// Current actor that supplies the spawn pose and sector.
        parent: EdOperand,
        /// Tile number or variable used as the spawned actor's picnum.
        tile: EdOperand,
        /// Write the spawned sprite index (or -1 on failure) to `RETURN`.
        return_sprite: bool,
    },
    /// Fire a projectile from the current actor through the live world host.
    ShootActor {
        /// Current actor that supplies the shooter and spawn pose.
        parent: EdOperand,
        /// Projectile tile number.
        tile: EdOperand,
        /// Optional explicit Build-space vertical velocity (`zshoot`).
        vertical_velocity: Option<EdOperand>,
        /// Write the spawned sprite index (or -1 on failure) to `RETURN`.
        return_sprite: bool,
    },
    /// Replace a gamevar/array element with its wrapping signed absolute value.
    Abs {
        /// Mutable operand.
        target: EdLValue,
    },
    /// Clamp a gamevar/array element to an inclusive range.
    Clamp {
        /// Mutable operand.
        target: EdLValue,
        /// Inclusive lower bound.
        minimum: EdOperand,
        /// Inclusive upper bound.
        maximum: EdOperand,
    },
    /// Calculate the integer hypotenuse of two signed coordinates.
    CalculateHypotenuse {
        /// Destination.
        target: EdLValue,
        /// X component.
        x: EdOperand,
        /// Y component.
        y: EdOperand,
    },
    /// Calculate the floor square root of an integer operand.
    Sqrt {
        /// Value to read.
        input: EdOperand,
        /// Destination.
        target: EdLValue,
    },
    /// Emit a quote to the host console/log output stream.
    EchoQuote {
        /// Quote slot selected by the compile-time quote number.
        quote: usize,
    },
    /// Display a CON quote for the active player.
    DisplayQuote {
        /// Quote slot selected by the compile-time quote number.
        quote: usize,
    },
    /// Apply a player palette flash using CON `palfrom` arguments.
    PaletteFrom {
        /// Flash duration followed by red, green and blue channel values.
        values: [EdOperand; 4],
    },
    /// Write deterministic elapsed game time in milliseconds to a gamevar.
    GetTicks {
        /// Destination gamevar or array element.
        target: EdLValue,
    },
    /// Select the full-screen palette for a player view.
    SetGamePalette {
        /// Base palette or lookup-table identifier.
        palette: EdOperand,
    },
    /// Set the wall-time pacing rate without changing per-tic simulation math.
    SetTimerRate {
        /// EDuke32 timer clock rate (default 120; Room uses one tic per four ticks).
        rate: EdOperand,
    },
    /// Start the owning player's end-of-episode countdown.
    EndOfGame {
        /// Countdown duration in EDuke32 time units; defaults to 52.
        delay: EdOperand,
    },
    /// Start the active player's 26-tic TIP HUD animation.
    PlayerTip,
}

/// Compiled EDuke32 VM instruction stream and state entry points.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdukeProgram {
    /// Instructions in deterministic source order.
    pub instructions: Vec<EdInstruction>,
    /// State entry points keyed by exact source label spelling.
    pub states: BTreeMap<String, usize>,
}

/// Result of running an EDuke32 instruction stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdExecution {
    /// Instructions executed, including calls and returns.
    pub steps: usize,
    /// True when an explicit top-level `return` ended the event chain.
    pub returned: bool,
    /// True when `killit` stopped execution for the current actor.
    pub killed_actor: bool,
    /// Actor ID requested by `killit`, when this execution had an actor context.
    pub kill_actor_id: Option<i32>,
    /// True when CON `resetplayer` requested the host's single-player restart.
    pub restart_player: bool,
}

/// Host bridge for native structure fields used by EDuke32 CON scripts.
///
/// The host performs the final member-name, type, access, and object-bound
/// checks against its live Build/player state. Calls are made in CON order.
pub trait EdukeStructAccess {
    /// Read a structure member, optionally selecting an element in an array
    /// member such as `player.ammo_amount[weapon]`.
    fn get_member(
        &mut self,
        kind: EdStructKind,
        id: i32,
        member: &str,
        member_index: Option<i32>,
    ) -> Result<i32, EdukeVmError>;

    /// Write a structure member, optionally selecting an element in an array
    /// member such as `player.ammo_amount[weapon]`.
    fn set_member(
        &mut self,
        kind: EdStructKind,
        id: i32,
        member: &str,
        member_index: Option<i32>,
        value: i32,
    ) -> Result<(), EdukeVmError>;

    /// Set `guniqhudid` for following screen-space sprite commands.
    fn set_hud_unique_id(&mut self, _id: i32) {}

    /// Append one render-only `rotatesprite` request.
    fn draw_hud_sprite(&mut self, _draw: HudSpriteDraw) -> Result<(), EdukeVmError> {
        Ok(())
    }

    /// Append one render-only `minitext` request.
    fn draw_hud_text(&mut self, _draw: HudTextDraw) -> Result<(), EdukeVmError> {
        Ok(())
    }

    /// Append one render-only `gametext` request.
    fn draw_hud_gametext(&mut self, _draw: HudGameTextDraw) -> Result<(), EdukeVmError> {
        Ok(())
    }

    /// Append one render-only digital-number request.
    fn draw_hud_number(&mut self, _draw: HudNumberDraw) -> Result<(), EdukeVmError> {
        Ok(())
    }

    /// Current `guniqhudid` value for constructing interpolation metadata.
    fn current_hud_unique_id(&self) -> i32 {
        0
    }

    /// Queue a sound using the current Build actor or local player position.
    fn play_sound(
        &mut self,
        _actor: i32,
        _sound: i32,
        _mode: EdSoundMode,
    ) -> Result<(), EdukeVmError> {
        Ok(())
    }

    /// Adjust the pitch of an active sound, optionally restricted to one actor.
    fn set_actor_sound_pitch(
        &mut self,
        _context_actor: i32,
        _actor: i32,
        _sound: i32,
        _pitch_offset: i32,
    ) -> Result<(), EdukeVmError> {
        Ok(())
    }

    /// Stop all active voices for a sound ID.
    fn stop_sound(&mut self, _actor: i32, _sound: i32) -> Result<(), EdukeVmError> {
        Ok(())
    }

    /// Stop a sound owned by one actor, without stopping voices from other
    /// actors that use the same sound ID.
    fn stop_actor_sound(&mut self, actor: i32, sound: i32) -> Result<(), EdukeVmError> {
        self.stop_sound(actor, sound)
    }

    /// Set an actor's active action pointer and reset action counters.
    fn set_actor_action(&mut self, _actor: i32, _action: i32) -> Result<(), EdukeVmError> {
        Ok(())
    }

    /// Set an actor's movement pointer, flags and optional random angle.
    fn set_actor_move(
        &mut self,
        _actor: i32,
        _movement: i32,
        _flags: i32,
    ) -> Result<(), EdukeVmError> {
        Ok(())
    }

    /// Set an actor's AI pointer and load its action, movement and flags.
    fn set_actor_ai(&mut self, _actor: i32, _ai: i32) -> Result<(), EdukeVmError> {
        Ok(())
    }

    /// Write an actor field through a classic CON command.
    fn set_actor_member(
        &mut self,
        _actor: i32,
        _member: EdActorMember,
        _value: i32,
    ) -> Result<(), EdukeVmError> {
        Ok(())
    }

    /// Apply CON `addkills` for the active player and actor.
    fn add_actor_kills(
        &mut self,
        _player: i32,
        _actor: i32,
        _amount: i32,
    ) -> Result<(), EdukeVmError> {
        Ok(())
    }

    /// Restore the actor's palette saved by `spritepal`.
    fn restore_actor_palette(&mut self, _actor: i32) -> Result<(), EdukeVmError> {
        Ok(())
    }

    /// Apply `sizeto` to the live actor and its bounded size cache.
    fn set_actor_size_to(&mut self, _actor: i32, _x: i32, _y: i32) -> Result<(), EdukeVmError> {
        Ok(())
    }

    /// Apply the classic CON `fall` operation to the live actor.
    fn fall_actor(&mut self, _actor: i32) -> Result<(), EdukeVmError> {
        Ok(())
    }

    /// Reset T0 or T2 on the live actor.
    fn reset_actor_counter(&mut self, _actor: i32, _action: bool) -> Result<(), EdukeVmError> {
        Ok(())
    }

    /// Assign `sizeat` repeats on the live actor.
    fn set_actor_size(&mut self, _actor: i32, _x: i32, _y: i32) -> Result<(), EdukeVmError> {
        Ok(())
    }

    /// Read the actor context selected by `userdef.vm_sprite`, when the host
    /// models that EDuke32 VM setting.
    fn current_actor(&mut self) -> Option<i32> {
        None
    }

    /// Temporarily set the live actor context while dispatching a nested event.
    fn set_current_actor(&mut self, _actor: i32) {}

    /// Return the player seat nearest to an actor for spawn callbacks.
    fn closest_player_for_actor(&mut self, _actor: i32, fallback: usize) -> usize {
        fallback
    }

    /// Drain sprites spawned by the last host instruction so nested events can
    /// run at the exact CON instruction that created them.
    fn take_spawned_actor_ids(&mut self) -> Vec<i32> {
        Vec::new()
    }

    /// Read an EDuke32 predefined value whose backing storage belongs to the
    /// live engine rather than the script VM.
    fn predefined_variable(&mut self, _name: &str) -> Option<i32> {
        None
    }

    /// Write an engine-backed predefined variable such as `RESPAWN_ITEMS`.
    /// Returns false when the host does not model that variable.
    fn set_predefined_variable(&mut self, _name: &str, _value: i32) -> bool {
        false
    }

    /// Read the host gameplay PRNG seed when CON execution shares the live
    /// Build random stream. `None` keeps standalone VM execution self-contained.
    fn game_random_seed(&mut self) -> Option<i32> {
        None
    }

    /// Replace the host gameplay PRNG seed for writes to CON `randomseed`.
    fn set_game_random_seed(&mut self, _seed: i32) -> bool {
        false
    }

    /// Advance the host gameplay PRNG and return its unmasked 32-bit result.
    fn next_game_random(&mut self) -> Option<i32> {
        None
    }

    /// Apply EDuke32's `addphealth` to the selected player in actor context.
    fn add_player_health(
        &mut self,
        _player: usize,
        _actor: usize,
        _amount: i32,
    ) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Apply `addammo` or `addweapon` to the nearest player. Returns `false`
    /// when full ammo halts the current CON event, as EDuke32 does.
    fn player_weapon_op(
        &mut self,
        _player: usize,
        _actor: i32,
        _add_weapon: bool,
        _weapon: i32,
        _ammo: i32,
    ) -> Result<bool, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Apply EDuke32 `addinventory` to the nearest player in actor context.
    fn add_player_inventory(
        &mut self,
        _player: usize,
        _actor: i32,
        _item: i32,
        _amount: i32,
    ) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Drop the selected player's current weapon from the live player sprite.
    fn toss_player_weapon(&mut self, _player: usize, _actor: i32) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Spawn an EDuke32 `debris` burst from the selected actor.
    fn spawn_debris(&mut self, _actor: i32, _tile: i32, _count: i32) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Spawn an EDuke32 `guts` burst from the selected actor.
    fn spawn_guts(
        &mut self,
        _player: usize,
        _actor: i32,
        _tile: i32,
        _count: i32,
    ) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Spawn an EDuke32 `lotsofglass` burst from the selected actor.
    fn spawn_lots_of_glass(&mut self, _actor: i32, _count: i32) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Spawn EDuke32 `spawnceilingglass` fragments around a sector ceiling.
    fn spawn_ceiling_glass(
        &mut self,
        _sprite: i32,
        _sector: i32,
        _count: i32,
    ) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Spawn an EDuke32 CON item spray from the current actor.
    fn spawn_item_spray(
        &mut self,
        _actor: i32,
        _kind: EdItemSprayKind,
        _count: i32,
    ) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Queue the classic CON `operate` action for the current actor.
    fn request_actor_operate(&mut self, _actor: i32) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Arm matching masterswitch sprites for their native delayed tick.
    fn operate_master_switches(&mut self, _lotag: i32) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Run EDuke32 `respawnhitag` for the selected actor.
    fn respawn_hitag(&mut self, _actor: i32) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Queue the single-player map restart requested by CON `resetplayer`.
    fn request_player_restart(&mut self, _player: usize) -> Result<bool, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Select the EDuke32 full-screen game palette for the selected view.
    fn set_game_palette(&mut self, _player: usize, _palette: i32) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Set the deterministic runtime's requested EDuke32 timer rate.
    fn set_timer_rate(&mut self, _rate: i32) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Start the end-of-episode countdown for a player context.
    fn begin_end_of_game(&mut self, _player: usize, _delay: i32) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Start the TIP HUD animation for a player seat.
    fn show_tip(&mut self, _player: usize) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Apply EDuke32 CON `hitradius` to the current actor's live world.
    fn apply_hitradius(
        &mut self,
        _actor: i32,
        _radius: i32,
        _damage: [i32; 4],
    ) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Queue the classic CON `pkick` effect for the current actor.
    fn request_player_quick_kick(&mut self, _actor: i32) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Start the CON `pstomp` knee sequence when the player can see the actor.
    fn request_player_stomp(&mut self, _player: usize, _actor: i32) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Queue the CON `wackplayer` view angle for the current actor's player.
    fn request_player_wack(
        &mut self,
        _player: usize,
        _actor: i32,
        _angle: i32,
    ) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Display a CON quote for a player context.
    fn display_quote(&mut self, _player: usize, _quote: usize) -> Result<(), EdukeVmError> {
        Ok(())
    }

    /// Apply an EDuke32 `palfrom` request to the selected player.
    fn palette_from(&mut self, _player: usize, _values: [i32; 4]) -> Result<(), EdukeVmError> {
        Ok(())
    }

    /// Return `(floor_z, ceiling_z)` for a sector and Build-space position.
    fn get_slope_z(&mut self, _sector: i32, _x: i32, _y: i32) -> Result<(i32, i32), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Find which Build sector's wall range owns a wall ID.
    fn sector_of_wall(&mut self, _wall: i32) -> Result<i32, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Query live Build floor/ceiling range values and hit codes.
    fn get_z_range(
        &mut self,
        _point: [i32; 4],
        _wall_distance: i32,
        _clipmask: u32,
    ) -> Result<[i32; 4], EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Query Build line-of-sight between two positions and sectors.
    fn can_see(&mut self, _start: [i32; 4], _end: [i32; 4]) -> Result<bool, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Refresh Chocolate's wake timer when a CON visibility predicate sees
    /// the player.
    fn actor_seen_by_player(&mut self, _actor: usize) -> Result<(), EdukeVmError> {
        Ok(())
    }

    /// Evaluate EDuke32's `ifcanshoottarget` firing-corridor predicate.
    fn actor_can_shoot_target(
        &mut self,
        _player: usize,
        _actor: usize,
    ) -> Result<bool, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Evaluate the incoming-projectile test used by `ifbulletnear`.
    fn actor_bullet_near(&mut self, _actor: usize) -> Result<bool, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Select the current actor's respawn mode from its tile classification.
    fn actor_respawn_enabled(&mut self, _actor: usize) -> Result<bool, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Evaluate `ifactornotstayput` from the active actor's temp state.
    fn actor_not_stayput(&mut self, _actor: usize) -> Result<bool, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Test EDuke32 `ifawayfromwall` against all four diagonal points.
    fn actor_away_from_wall(&mut self, _actor: usize) -> Result<bool, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Query whether this actor owns a currently active Atomic sound voice.
    fn actor_has_active_sound(&mut self, _actor: usize) -> Result<bool, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Query whether one actor currently owns a specific sound ID.
    fn actor_has_sound(&mut self, _actor: i32, _sound: i32) -> Result<bool, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Evaluate the co-op weapon reservation condition for an actor.
    fn actor_weapon_reservation(
        &mut self,
        _player: usize,
        _actor: usize,
        _register: bool,
    ) -> Result<bool, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Cast a Build hitscan and return `(hitsect, hitwall, hitsprite, x, y, z)`.
    fn hit_scan(
        &mut self,
        _origin: [i32; 4],
        _vector: [i32; 3],
        _clipmask: u32,
    ) -> Result<[i32; 6], EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Snapshot active sprite IDs for EDuke32's `for <var> allsprites` loop.
    fn active_sprite_ids(&mut self) -> Result<Vec<i32>, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Resolve a Build sector for a point, starting with the supplied hint.
    fn update_sector(
        &mut self,
        _hint: i32,
        _x: i32,
        _y: i32,
        _z: Option<i32>,
    ) -> Result<i32, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Measure the live Build-space distance between two sprite slots.
    fn sprite_distance(
        &mut self,
        _first: i32,
        _second: i32,
        _three_dimensional: bool,
    ) -> Result<i32, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Find a matching active actor within the strict `findnearactor3d` range.
    fn find_near_actor_3d(
        &mut self,
        _source_actor: i32,
        _tile: i32,
        _distance: i32,
    ) -> Result<i32, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Return EDuke32's current nearest-player distance for an actor.
    fn player_distance(&mut self, _actor: i32) -> Result<i32, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Evaluate `ifpinventory` for the nearest live player to the active actor.
    fn player_inventory_condition(
        &mut self,
        _item: i32,
        _amount: i32,
        _player: usize,
        _actor: usize,
    ) -> Result<bool, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Evaluate EDuke32's OR-combined `ifp` player-state flag mask.
    fn if_player(
        &mut self,
        _flags: i32,
        _player: usize,
        _actor: usize,
    ) -> Result<bool, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Read the active player's current sync input bits.
    fn player_input_bits(&mut self, _player: usize) -> Result<u32, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Evaluate `ifsquished`, including the palette-one queued hit response.
    fn actor_squished(&mut self, _actor: usize) -> Result<bool, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Apply and consume a pending actor weapon hit, returning whether it hit.
    fn actor_hit_by_weapon(&mut self, _actor: usize) -> Result<bool, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Read a Build sprite-list head or next link.
    fn read_sprite_list(
        &mut self,
        _kind: EdSpriteListKind,
        _index: i32,
    ) -> Result<i32, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Move a sprite between Build status or sector lists.
    fn change_sprite_list(
        &mut self,
        _sprite: i32,
        _index: i32,
        _status: bool,
    ) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Move a sprite in the Build world, including its sector-list membership.
    fn set_sprite_position(
        &mut self,
        _sprite: i32,
        _x: i32,
        _y: i32,
        _z: i32,
    ) -> Result<(), EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Spawn an actor at the current actor's position and return its sprite ID.
    fn spawn_actor(&mut self, _parent: i32, _tile: i32) -> Result<i32, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }

    /// Fire a projectile from an actor. Hosts may initialize tile-defined
    /// projectile properties in addition to the optional explicit z velocity.
    fn shoot_actor(
        &mut self,
        _parent: i32,
        _tile: i32,
        _vertical_velocity: Option<i32>,
    ) -> Result<i32, EdukeVmError> {
        Err(EdukeVmError::StructAccessUnavailable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum EdVariableStorage {
    Global(i32),
    Player(Vec<i32>),
    Actor(Vec<i32>),
}

#[derive(Debug)]
struct EdSpriteLoopFrame {
    begin: usize,
    after: usize,
    sprite_ids: Vec<i32>,
    cursor: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct EdVariableCell {
    name: String,
    storage: EdVariableStorage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct EdArrayCell {
    name: String,
    values: Vec<i32>,
}

/// Saveable EDuke32 gamevar / gamearray values in stable declaration order.
///
/// This state object provides deterministic, bounds-checked storage and runs
/// the source compiler's supported EDuke32 instruction subset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdukeVmState {
    variables: Vec<EdVariableCell>,
    arrays: Vec<EdArrayCell>,
    /// Mutable CON quote strings, serialized as script state.
    #[serde(default)]
    quotes: Vec<String>,
    variable_ids: BTreeMap<String, usize>,
    array_ids: BTreeMap<String, usize>,
    /// Serialized gameplay random state, kept separate from Build's `krand`.
    #[serde(default)]
    game_random_seed: i32,
    /// Serialized display-only random state, never shared with gameplay draws.
    #[serde(default)]
    display_random_seed: i32,
    /// EDuke32's live `numplayers` predefined gamevar.
    #[serde(default = "default_num_players")]
    num_players: i32,
    /// EDuke32's live `MULTIMODE` predefined gamevar.
    #[serde(default = "default_num_players")]
    multi_mode: i32,
    /// Last CON quote emitted to the console, suppressing consecutive repeats.
    #[serde(default)]
    last_displayed_quote: Option<usize>,
    /// Ephemeral CON console messages; output is deliberately excluded from saves.
    #[serde(skip)]
    console_output: VecDeque<String>,
}

impl Default for EdukeVmState {
    fn default() -> Self {
        Self {
            variables: Vec::new(),
            arrays: Vec::new(),
            quotes: Vec::new(),
            variable_ids: BTreeMap::new(),
            array_ids: BTreeMap::new(),
            game_random_seed: 0,
            display_random_seed: 0,
            num_players: default_num_players(),
            multi_mode: default_num_players(),
            last_displayed_quote: None,
            console_output: VecDeque::new(),
        }
    }
}

fn default_num_players() -> i32 {
    1
}

fn random_range_from_bits(maximum: i32, random: i32) -> i32 {
    if maximum < 0 {
        0
    } else {
        (i64::from(random) % (i64::from(maximum) + 1)) as i32
    }
}

fn line_intersect_3d(origin: [i32; 3], destination: [i32; 3], line: [i32; 4]) -> Option<[i32; 3]> {
    let ray_x = i128::from(destination[0]) - i128::from(origin[0]);
    let ray_y = i128::from(destination[1]) - i128::from(origin[1]);
    let wall_x = i128::from(line[2]) - i128::from(line[0]);
    let wall_y = i128::from(line[3]) - i128::from(line[1]);
    let mut denominator = ray_x * wall_y - ray_y * wall_x;
    if denominator == 0 {
        return None;
    }
    let offset_x = i128::from(line[0]) - i128::from(origin[0]);
    let offset_y = i128::from(line[1]) - i128::from(origin[1]);
    let mut ray_numerator = offset_x * wall_y - offset_y * wall_x;
    let mut wall_numerator = offset_x * ray_y - offset_y * ray_x;
    if denominator < 0 {
        denominator = -denominator;
        ray_numerator = -ray_numerator;
        wall_numerator = -wall_numerator;
    }
    if !(0..=denominator).contains(&ray_numerator) || !(0..=denominator).contains(&wall_numerator) {
        return None;
    }
    let interpolate = |start: i32, end: i32| {
        let delta = i128::from(end) - i128::from(start);
        i32::try_from(i128::from(start) + delta * ray_numerator / denominator).ok()
    };
    Some([
        interpolate(origin[0], destination[0])?,
        interpolate(origin[1], destination[1])?,
        interpolate(origin[2], destination[2])?,
    ])
}

/// Failures while constructing or accessing EDuke32 VM storage.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EdukeVmError {
    /// An Atomic image was passed to the EDuke32 storage layer.
    #[error("EDuke32 runtime state requires an EDuke32 CON image")]
    WrongDialect,
    /// A malformed compiled instruction has an unexpected operand count.
    #[error("EDuke32 instruction `{instruction}` expects {expected} operands, got {actual}")]
    InvalidInstructionArity {
        /// Instruction name.
        instruction: &'static str,
        /// Required operand count.
        expected: usize,
        /// Compiled operand count.
        actual: usize,
    },
    /// A serialized EDuke32 state does not match its compiled declarations.
    #[error("EDuke32 VM snapshot does not match the compiled CON declarations")]
    InvalidSnapshot,
    /// The session has no initialized EDuke32 VM to receive an event.
    #[error("EDuke32 VM is unavailable: {0}")]
    RuntimeUnavailable(String),
    /// A declaration refers to a value that the compiler did not resolve.
    #[error("cannot resolve `{value}` in EDuke32 declaration `{declaration}`")]
    UnresolvedValue {
        /// Variable or array declaration name.
        declaration: String,
        /// Unresolved source token.
        value: String,
    },
    /// Two declarations use the same exact name.
    #[error("duplicate EDuke32 {kind} `{name}`")]
    DuplicateName {
        /// Declaration category.
        kind: &'static str,
        /// Duplicate identifier.
        name: String,
    },
    /// A declared array would exceed the allocation limit.
    #[error("EDuke32 value allocation limit exceeded by `{name}` ({cells} cells)")]
    AllocationLimit {
        /// Variable or array identifier crossing the bound.
        name: String,
        /// Total requested number of value cells.
        cells: usize,
    },
    /// Storage allocation failed without panicking.
    #[error("could not allocate EDuke32 {kind} `{name}`")]
    AllocationFailed {
        /// Allocation category.
        kind: &'static str,
        /// Variable or array identifier.
        name: String,
    },
    /// A resizearray command requested a negative length.
    #[error("EDuke32 array `{name}` cannot be resized to negative length {length}")]
    NegativeArraySize {
        /// Array identifier.
        name: String,
        /// Requested length.
        length: i32,
    },
    /// A quote reference is outside the bounded EDuke32 quote table.
    #[error("EDuke32 quote index {index} is outside 0..{extent}")]
    QuoteIndexOutOfBounds {
        /// Requested quote slot.
        index: usize,
        /// Quote table length.
        extent: usize,
    },
    /// Formatted text exceeded the bounded quote size.
    #[error("EDuke32 quote text exceeds {limit} bytes")]
    QuoteTooLong {
        /// Maximum formatted size.
        limit: usize,
    },
    /// Format text has a malformed or unsupported placeholder.
    #[error("invalid EDuke32 qsprintf format: {0}")]
    InvalidQuoteFormat(String),
    /// A variable or array is not present in the compiled image.
    #[error("unknown EDuke32 {kind} `{name}`")]
    UnknownName {
        /// Declaration category.
        kind: &'static str,
        /// Requested identifier.
        name: String,
    },
    /// The selected per-player or per-actor slot is outside its fixed extent.
    #[error("{kind} index {index} is outside the extent {extent} for `{name}`")]
    IndexOutOfBounds {
        /// Index category (`player`, `actor` or `array`).
        kind: &'static str,
        /// Name of the variable or array.
        name: String,
        /// Requested element index.
        index: usize,
        /// Number of allocated slots.
        extent: usize,
    },
    /// A CON division or remainder operation had an invalid divisor/result.
    #[error("invalid `{operator}` operation for `{name}` with right operand {right}")]
    InvalidArithmetic {
        /// Variable being updated or expression label.
        name: String,
        /// Operator name.
        operator: &'static str,
        /// Right operand.
        right: i32,
    },
    /// A `clamp` instruction received an inverted range.
    #[error("invalid EDuke32 clamp range {minimum}..={maximum}")]
    InvalidClampRange {
        /// Lower bound.
        minimum: i32,
        /// Upper bound.
        maximum: i32,
    },
    /// The instruction budget was exhausted (usually an unbounded script loop).
    #[error("EDuke32 instruction budget exhausted after {budget} instructions")]
    InstructionBudgetExceeded {
        /// Maximum instructions allowed for this invocation.
        budget: usize,
    },
    /// A program contains an invalid instruction or branch address.
    #[error("EDuke32 program counter {address} is outside {length} instructions")]
    InvalidProgramCounter {
        /// Invalid instruction address.
        address: usize,
        /// Instruction count.
        length: usize,
    },
    /// A called state name is absent from the program.
    #[error("unknown EDuke32 state `{name}`")]
    UnknownState {
        /// State identifier.
        name: String,
    },
    /// A structure instruction was run without a live host bridge.
    #[error("EDuke32 structure access requires a live host context")]
    StructAccessUnavailable,
    /// A native structure ID is outside the live host's object range.
    #[error("EDuke32 {kind:?} structure id {id} is out of range")]
    InvalidStructId {
        /// Structure family.
        kind: EdStructKind,
        /// Requested object ID.
        id: i32,
    },
    /// The host does not expose the requested structure member.
    #[error("unsupported EDuke32 {kind:?} member `{member}`")]
    UnknownStructMember {
        /// Structure family.
        kind: EdStructKind,
        /// Member identifier.
        member: String,
    },
    /// The requested member is part of the host's read-only view.
    #[error("EDuke32 {kind:?} member `{member}` is read-only")]
    ReadOnlyStructMember {
        /// Structure family.
        kind: EdStructKind,
        /// Member identifier.
        member: String,
    },
    /// A script integer cannot be represented by the native member's type.
    #[error("value {value} is out of range for EDuke32 {kind:?} member `{member}`")]
    StructMemberValueOutOfRange {
        /// Structure family.
        kind: EdStructKind,
        /// Member identifier.
        member: String,
        /// Requested value.
        value: i32,
    },
    /// A get/set*var command does not match its declaration's storage scope.
    #[error("EDuke32 variable `{name}` is not declared with {expected:?} scope")]
    WrongVariableScope {
        /// Variable identifier.
        name: String,
        /// Required storage scope.
        expected: EdVariableScope,
    },
    /// State recursion exceeded the per-call bound.
    #[error("EDuke32 state-call depth exceeded {limit}")]
    StateCallDepthExceeded {
        /// Maximum active state calls.
        limit: usize,
    },
    /// Nested EGS/SPAWN callback chains exceeded their bound.
    #[error("EDuke32 spawn-event depth exceeded {limit}")]
    SpawnEventDepthExceeded {
        /// Maximum nested spawn-event calls.
        limit: usize,
    },
}

impl EdukeVmState {
    /// Allocate deterministic storage from EDuke32 declarations in `image`.
    pub fn from_image(image: &ScriptImage) -> Result<Self, EdukeVmError> {
        if image.dialect != ConDialect::EDuke32 {
            return Err(EdukeVmError::WrongDialect);
        }
        if image.quotes.len() > EDUKE_MAX_QUOTES {
            return Err(EdukeVmError::QuoteIndexOutOfBounds {
                index: image.quotes.len() - 1,
                extent: EDUKE_MAX_QUOTES,
            });
        }
        if image
            .quotes
            .iter()
            .any(|quote| quote.len() > EDUKE_MAX_QUOTE_BYTES)
        {
            return Err(EdukeVmError::QuoteTooLong {
                limit: EDUKE_MAX_QUOTE_BYTES,
            });
        }
        let mut state = Self::default();
        state.game_random_seed = EDUKE_INITIAL_GAME_RANDOM_SEED;
        let mut allocated = 0usize;
        for definition in &image.eduke_variables {
            if definition.name == "randomseed"
                || definition.name == "numplayers"
                || definition.name == "MULTIMODE"
                || matches!(
                    definition.name.as_str(),
                    "RESPAWN_MONSTERS" | "RESPAWN_ITEMS" | "RESPAWN_INVENTORY"
                )
            {
                return Err(EdukeVmError::DuplicateName {
                    kind: "built-in variable",
                    name: definition.name.clone(),
                });
            }
            state.push_variable(image, definition, &mut allocated)?;
        }
        if !state.variable_ids.contains_key("RETURN") {
            // EDuke32 reserves RETURN as the result slot for e-prefixed
            // commands such as `espawn`; it is available without a CON var.
            state.push_variable(
                image,
                &EdVariableDefinition {
                    name: "RETURN".to_owned(),
                    initial_value: "0".to_owned(),
                    flags: Vec::new(),
                },
                &mut allocated,
            )?;
        }
        for definition in &image.eduke_arrays {
            state.push_array(image, definition, &mut allocated)?;
        }
        copy_quotes(&mut state.quotes, &image.quotes)?;
        if state.quotes.len() < EDUKE_MAX_QUOTES {
            state
                .quotes
                .try_reserve_exact(EDUKE_MAX_QUOTES - state.quotes.len())
                .map_err(|_| EdukeVmError::AllocationFailed {
                    kind: "quote table",
                    name: "quotes".to_owned(),
                })?;
            state.quotes.resize_with(EDUKE_MAX_QUOTES, String::new);
        }
        Ok(state)
    }

    /// Check that deserialized VM state still matches the loaded CON image and
    /// remains within the runtime allocation and quote bounds.
    pub fn validate_for_image(&mut self, image: &ScriptImage) -> Result<(), EdukeVmError> {
        let expected = Self::from_image(image)?;
        let legacy_return_missing = expected.variable_ids.contains_key("RETURN")
            && self
                .variables
                .iter()
                .all(|variable| variable.name != "RETURN");
        let variable_ids_match = if legacy_return_missing {
            self.variable_ids.len().saturating_add(1) == expected.variable_ids.len()
                && self.variable_ids.iter().all(|(name, id)| {
                    expected
                        .variable_ids
                        .iter()
                        .any(|(expected_name, expected_id)| {
                            expected_name.eq_ignore_ascii_case(name) && expected_id == id
                        })
                })
        } else {
            self.variable_ids == expected.variable_ids
                || legacy_case_folded_ids_match(&self.variable_ids, &expected.variable_ids)
        };
        let array_ids_match = self.array_ids == expected.array_ids
            || legacy_case_folded_ids_match(&self.array_ids, &expected.array_ids);
        let variable_count_matches = self.variables.len()
            == expected
                .variables
                .len()
                .saturating_sub(usize::from(legacy_return_missing));
        if !variable_ids_match
            || !array_ids_match
            || !variable_count_matches
            || self.arrays.len() != expected.arrays.len()
            || self.quotes.len() != expected.quotes.len()
            || !(0..=EDUKE_MAXPLAYERS as i32).contains(&self.num_players)
            || !(0..=EDUKE_MAXPLAYERS as i32).contains(&self.multi_mode)
            || self
                .quotes
                .iter()
                .any(|quote| quote.len() > EDUKE_MAX_QUOTE_BYTES)
        {
            return Err(EdukeVmError::InvalidSnapshot);
        }
        for (saved, declared) in self.variables.iter().zip(&expected.variables) {
            let same_storage_shape = match (&saved.storage, &declared.storage) {
                (EdVariableStorage::Global(_), EdVariableStorage::Global(_)) => true,
                (EdVariableStorage::Player(saved), EdVariableStorage::Player(declared))
                | (EdVariableStorage::Actor(saved), EdVariableStorage::Actor(declared)) => {
                    saved.len() == declared.len()
                }
                _ => false,
            };
            if saved.name != declared.name || !same_storage_shape {
                return Err(EdukeVmError::InvalidSnapshot);
            }
        }
        let array_cells = self
            .arrays
            .iter()
            .try_fold(0usize, |total, array| total.checked_add(array.values.len()));
        let variable_cells = self.variables.iter().try_fold(0usize, |total, cell| {
            let cells = match &cell.storage {
                EdVariableStorage::Global(_) => 1,
                EdVariableStorage::Player(values) | EdVariableStorage::Actor(values) => {
                    values.len()
                }
            };
            total.checked_add(cells)
        });
        if array_cells
            .zip(variable_cells)
            .is_none_or(|(arrays, variables)| {
                arrays.saturating_add(variables) > MAX_EDUKE_VALUE_CELLS
            })
        {
            return Err(EdukeVmError::InvalidSnapshot);
        }
        for (saved, declared) in self.arrays.iter().zip(&expected.arrays) {
            if saved.name != declared.name {
                return Err(EdukeVmError::InvalidSnapshot);
            }
        }
        // Older version-2 snapshots serialized case-folded lookup keys. The
        // value vectors retain declaration order and exact spelling, so once
        // their shape is verified it is safe to rebuild the key maps.
        self.variable_ids.clone_from(&expected.variable_ids);
        self.array_ids.clone_from(&expected.array_ids);
        Ok(())
    }

    /// Add the implicit `RETURN` slot when migrating a version-2 save written
    /// before the built-in result variable was part of the VM image shape.
    pub fn ensure_return_variable(&mut self) -> Result<(), EdukeVmError> {
        if self.variable_ids.contains_key("RETURN") {
            return Ok(());
        }
        let allocated = self
            .variables
            .iter()
            .map(|cell| match &cell.storage {
                EdVariableStorage::Global(_) => 1,
                EdVariableStorage::Player(values) | EdVariableStorage::Actor(values) => {
                    values.len()
                }
            })
            .chain(self.arrays.iter().map(|array| array.values.len()))
            .fold(0usize, usize::saturating_add);
        check_limit("RETURN", allocated.saturating_add(1))?;
        self.variables
            .try_reserve(1)
            .map_err(|_| EdukeVmError::AllocationFailed {
                kind: "gamevar",
                name: "RETURN".to_owned(),
            })?;
        let id = self.variables.len();
        self.variables.push(EdVariableCell {
            name: "RETURN".to_owned(),
            storage: EdVariableStorage::Global(0),
        });
        self.variable_ids.insert("RETURN".to_owned(), id);
        Ok(())
    }

    /// Read a named gamevar using its global, player or actor scope.
    pub fn get_variable(
        &self,
        name: &str,
        player: usize,
        actor: usize,
    ) -> Result<i32, EdukeVmError> {
        if name == "randomseed" {
            return Ok(self.game_random_seed);
        }
        if name == "numplayers" {
            return Ok(self.num_players);
        }
        if name == "MULTIMODE" {
            return Ok(self.multi_mode);
        }
        let cell = self.variable(name)?;
        match &cell.storage {
            EdVariableStorage::Global(value) => Ok(*value),
            EdVariableStorage::Player(values) => scoped_read("player", &cell.name, values, player),
            EdVariableStorage::Actor(values) => scoped_read("actor", &cell.name, values, actor),
        }
    }

    /// Write a named gamevar using its global, player or actor scope.
    pub fn set_variable(
        &mut self,
        name: &str,
        player: usize,
        actor: usize,
        value: i32,
    ) -> Result<(), EdukeVmError> {
        if name == "randomseed" {
            self.game_random_seed = value;
            return Ok(());
        }
        if name == "numplayers" {
            self.num_players = value;
            return Ok(());
        }
        if name == "MULTIMODE" {
            self.multi_mode = value;
            return Ok(());
        }
        let index = self.variable_id(name)?;
        let cell = &mut self.variables[index];
        match &mut cell.storage {
            EdVariableStorage::Global(slot) => *slot = value,
            EdVariableStorage::Player(values) => {
                scoped_write("player", &cell.name, values, player, value)?;
            }
            EdVariableStorage::Actor(values) => {
                scoped_write("actor", &cell.name, values, actor, value)?;
            }
        }
        Ok(())
    }

    /// Read a gamevar at an explicit player or actor slot.
    pub fn get_scoped_variable(
        &self,
        name: &str,
        scope: EdVariableScope,
        index: usize,
    ) -> Result<i32, EdukeVmError> {
        let cell = self.variable(name)?;
        match (&cell.storage, scope) {
            (EdVariableStorage::Player(values), EdVariableScope::Player) => {
                scoped_read("player", &cell.name, values, index)
            }
            (EdVariableStorage::Actor(values), EdVariableScope::Actor) => {
                scoped_read("actor", &cell.name, values, index)
            }
            (EdVariableStorage::Global(value), EdVariableScope::Global) => Ok(*value),
            _ => Err(EdukeVmError::WrongVariableScope {
                name: cell.name.clone(),
                expected: scope,
            }),
        }
    }

    /// Write a gamevar at an explicit player or actor slot.
    pub fn set_scoped_variable(
        &mut self,
        name: &str,
        scope: EdVariableScope,
        index: usize,
        value: i32,
    ) -> Result<(), EdukeVmError> {
        let variable_id = self.variable_id(name)?;
        let cell = &mut self.variables[variable_id];
        match (&mut cell.storage, scope) {
            (EdVariableStorage::Player(values), EdVariableScope::Player) => {
                scoped_write("player", &cell.name, values, index, value)
            }
            (EdVariableStorage::Actor(values), EdVariableScope::Actor) => {
                scoped_write("actor", &cell.name, values, index, value)
            }
            (EdVariableStorage::Global(slot), EdVariableScope::Global) => {
                *slot = value;
                Ok(())
            }
            _ => Err(EdukeVmError::WrongVariableScope {
                name: cell.name.clone(),
                expected: scope,
            }),
        }
    }

    /// Apply one integer operator to a gamevar in its selected scope.
    pub fn apply_binary(
        &mut self,
        name: &str,
        player: usize,
        actor: usize,
        operator: EdBinaryOperator,
        right: i32,
    ) -> Result<i32, EdukeVmError> {
        let left = self.get_variable(name, player, actor)?;
        let value = evaluate_binary(name, operator, left, right)?;
        self.set_variable(name, player, actor, value)?;
        Ok(value)
    }

    /// Execute a program with deterministic player/actor context and a hard
    /// per-invocation instruction budget.
    pub fn execute(
        &mut self,
        program: &EdukeProgram,
        player: usize,
        actor: usize,
        instruction_budget: usize,
        state_depth_limit: usize,
    ) -> Result<EdExecution, EdukeVmError> {
        self.execute_from(
            program,
            0,
            player,
            actor,
            instruction_budget,
            state_depth_limit,
            None,
            false,
            0,
            None,
        )
    }

    /// Execute the instruction stream starting at a compiler-recorded event or
    /// state entry point.
    pub fn execute_at(
        &mut self,
        program: &EdukeProgram,
        entry: usize,
        player: usize,
        actor: usize,
        instruction_budget: usize,
        state_depth_limit: usize,
    ) -> Result<EdExecution, EdukeVmError> {
        self.execute_from(
            program,
            entry,
            player,
            actor,
            instruction_budget,
            state_depth_limit,
            None,
            false,
            0,
            None,
        )
    }

    /// Execute from an entry point with access to live engine structures.
    pub fn execute_at_with_access(
        &mut self,
        program: &EdukeProgram,
        entry: usize,
        player: usize,
        actor: usize,
        instruction_budget: usize,
        state_depth_limit: usize,
        access: &mut dyn EdukeStructAccess,
    ) -> Result<EdExecution, EdukeVmError> {
        self.execute_from(
            program,
            entry,
            player,
            actor,
            instruction_budget,
            state_depth_limit,
            None,
            false,
            0,
            Some(access),
        )
    }

    /// Execute an entry point with live host access and registered-event
    /// metadata, enabling nested `EVENT_SOUND`, `EVENT_EGS`, and `EVENT_SPAWN`.
    pub fn execute_at_with_access_and_events(
        &mut self,
        image: &ScriptImage,
        entry: usize,
        player: usize,
        actor: usize,
        instruction_budget: usize,
        state_depth_limit: usize,
        access: &mut dyn EdukeStructAccess,
    ) -> Result<EdExecution, EdukeVmError> {
        self.execute_from(
            &image.eduke_program,
            entry,
            player,
            actor,
            instruction_budget,
            state_depth_limit,
            Some(image),
            false,
            0,
            Some(access),
        )
    }

    /// Execute every source-registered hook for an EDuke32 event in source
    /// order. The remaining instruction budget is shared across all matching
    /// hooks so a large event chain cannot multiply the caller's limit.
    ///
    /// Built-in event ordering and `RETURN` semantics remain the responsibility
    /// of the engine integration layer.
    pub fn execute_registered_event_hooks(
        &mut self,
        image: &ScriptImage,
        event: &str,
        player: usize,
        actor: usize,
        instruction_budget: usize,
        state_depth_limit: usize,
    ) -> Result<Vec<EdExecution>, EdukeVmError> {
        self.execute_registered_event_hooks_inner(
            image,
            event,
            player,
            actor,
            instruction_budget,
            state_depth_limit,
            false,
            0,
            None,
        )
    }

    /// Execute registered event hooks with the host bridge used for native
    /// structure access.
    pub fn execute_registered_event_hooks_with_access(
        &mut self,
        image: &ScriptImage,
        event: &str,
        player: usize,
        actor: usize,
        instruction_budget: usize,
        state_depth_limit: usize,
        access: &mut dyn EdukeStructAccess,
    ) -> Result<Vec<EdExecution>, EdukeVmError> {
        self.execute_registered_event_hooks_inner(
            image,
            event,
            player,
            actor,
            instruction_budget,
            state_depth_limit,
            false,
            0,
            Some(access),
        )
    }

    fn execute_registered_event_hooks_inner(
        &mut self,
        image: &ScriptImage,
        event: &str,
        player: usize,
        mut actor: usize,
        instruction_budget: usize,
        state_depth_limit: usize,
        sound_event_active: bool,
        spawn_event_depth: usize,
        mut access: Option<&mut dyn EdukeStructAccess>,
    ) -> Result<Vec<EdExecution>, EdukeVmError> {
        if image.dialect != ConDialect::EDuke32 {
            return Err(EdukeVmError::WrongDialect);
        }
        let event_key = normalize(event);
        let mut remaining = instruction_budget;
        let mut executions = Vec::new();
        for (_, entry, _) in image
            .eduke_event_entries
            .iter()
            .filter(|(name, _, _)| normalize(name) == event_key)
        {
            let execution = match access.as_mut() {
                Some(host) => self.execute_from(
                    &image.eduke_program,
                    *entry,
                    player,
                    actor,
                    remaining,
                    state_depth_limit,
                    Some(image),
                    sound_event_active || event_key == "EVENT_SOUND",
                    spawn_event_depth,
                    Some(&mut **host),
                )?,
                None => self.execute_from(
                    &image.eduke_program,
                    *entry,
                    player,
                    actor,
                    remaining,
                    state_depth_limit,
                    Some(image),
                    sound_event_active || event_key == "EVENT_SOUND",
                    spawn_event_depth,
                    None,
                )?,
            };
            if let Some(host) = access.as_deref_mut() {
                if let Some(current_actor) = host.current_actor() {
                    actor = event_actor_context(current_actor)?;
                }
            }
            remaining = remaining.saturating_sub(execution.steps);
            let killed_actor = execution.killed_actor;
            let returned = execution.returned;
            executions.push(execution);
            if killed_actor || returned {
                break;
            }
        }
        Ok(executions)
    }

    /// Execute a named state with deterministic player/actor context.
    pub fn execute_state(
        &mut self,
        program: &EdukeProgram,
        name: &str,
        player: usize,
        actor: usize,
        instruction_budget: usize,
        state_depth_limit: usize,
    ) -> Result<EdExecution, EdukeVmError> {
        let entry =
            program
                .states
                .get(name)
                .copied()
                .ok_or_else(|| EdukeVmError::UnknownState {
                    name: name.to_owned(),
                })?;
        self.execute_from(
            program,
            entry,
            player,
            actor,
            instruction_budget,
            state_depth_limit,
            None,
            false,
            0,
            None,
        )
    }

    /// Seed the independent script gameplay and display random streams.
    pub fn seed_random_streams(&mut self, gameplay_seed: i32, display_seed: i32) {
        self.game_random_seed = gameplay_seed;
        self.display_random_seed = display_seed;
    }

    /// Advance and return masked bits from the chosen script random stream.
    pub fn random_bits(&mut self, mask: i32, display: bool) -> i32 {
        let seed = if display {
            &mut self.display_random_seed
        } else {
            &mut self.game_random_seed
        };
        krand_step(seed) & mask
    }

    /// Advance the chosen EDuke32 script stream and return 0..=maximum.
    pub fn random_range(&mut self, maximum: i32, display: bool) -> i32 {
        if maximum < 0 {
            return 0;
        }
        let seed = if display {
            &mut self.display_random_seed
        } else {
            &mut self.game_random_seed
        };
        let random = krand_step(seed) & 0x7fff;
        random_range_from_bits(maximum, random)
    }

    fn next_game_random(&mut self, access: &mut Option<&mut dyn EdukeStructAccess>) -> i32 {
        if let Some(host) = access.as_deref_mut()
            && let Some(random) = host.next_game_random()
        {
            if let Some(seed) = access
                .as_deref_mut()
                .and_then(|host| host.game_random_seed())
            {
                self.game_random_seed = seed;
            }
            random
        } else {
            krand_step(&mut self.game_random_seed)
        }
    }

    /// Return the storage scope selected for a named gamevar.
    pub fn variable_scope(&self, name: &str) -> Result<EdVariableScope, EdukeVmError> {
        if name == "randomseed" || name == "numplayers" || name == "MULTIMODE" {
            return Ok(EdVariableScope::Global);
        }
        let cell = self.variable(name)?;
        Ok(match &cell.storage {
            EdVariableStorage::Global(_) => EdVariableScope::Global,
            EdVariableStorage::Player(_) => EdVariableScope::Player,
            EdVariableStorage::Actor(_) => EdVariableScope::Actor,
        })
    }

    /// Update EDuke32's live predefined player-count gamevar for the next hook.
    pub fn set_num_players(&mut self, value: i32) {
        self.num_players = value.max(0);
    }

    /// Update EDuke32's live predefined multiplayer-mode gamevar.
    pub fn set_multimode(&mut self, value: i32) {
        self.multi_mode = value.max(0);
    }

    /// Read an element of a named EDuke32 array.
    pub fn get_array(&self, name: &str, element: usize) -> Result<i32, EdukeVmError> {
        let cell = self.array(name)?;
        cell.values
            .get(element)
            .copied()
            .ok_or_else(|| EdukeVmError::IndexOutOfBounds {
                kind: "array",
                name: cell.name.clone(),
                index: element,
                extent: cell.values.len(),
            })
    }

    /// Write an element of a named EDuke32 array.
    pub fn set_array(
        &mut self,
        name: &str,
        element: usize,
        value: i32,
    ) -> Result<(), EdukeVmError> {
        let index = self.array_id(name)?;
        let cell = &mut self.arrays[index];
        let extent = cell.values.len();
        let Some(slot) = cell.values.get_mut(element) else {
            return Err(EdukeVmError::IndexOutOfBounds {
                kind: "array",
                name: cell.name.clone(),
                index: element,
                extent,
            });
        };
        *slot = value;
        Ok(())
    }

    /// Read a quote string by its EDuke32 quote ID.
    pub fn quote(&self, index: usize) -> Result<&str, EdukeVmError> {
        self.quotes
            .get(index)
            .map(String::as_str)
            .ok_or(EdukeVmError::QuoteIndexOutOfBounds {
                index,
                extent: self.quotes.len(),
            })
    }

    /// Replace a quote string using the same bound as runtime `redefinequote`.
    pub fn set_quote(&mut self, index: usize, text: &str) -> Result<(), EdukeVmError> {
        let extent = self.quotes.len();
        let Some(slot) = self.quotes.get_mut(index) else {
            return Err(EdukeVmError::QuoteIndexOutOfBounds { index, extent });
        };
        if text.len() > EDUKE_MAX_QUOTE_BYTES {
            return Err(EdukeVmError::QuoteTooLong {
                limit: EDUKE_MAX_QUOTE_BYTES,
            });
        }
        let mut replacement = String::new();
        replacement
            .try_reserve_exact(text.len())
            .map_err(|_| EdukeVmError::AllocationFailed {
                kind: "quote",
                name: index.to_string(),
            })?;
        replacement.push_str(text);
        *slot = replacement;
        Ok(())
    }

    /// Drain messages emitted by CON `echo` instructions since the previous
    /// call. The queue keeps at most 256 messages so a script that echoes on
    /// every tic cannot grow VM memory without bound.
    pub fn take_console_output(&mut self) -> Vec<String> {
        std::mem::take(&mut self.console_output)
            .into_iter()
            .collect()
    }

    /// Resize an array, zero-filling growth, within the global memory budget.
    pub fn resize_array(&mut self, name: &str, new_len: usize) -> Result<(), EdukeVmError> {
        let index = self.array_id(name)?;
        let old_len = self.arrays[index].values.len();
        let array_cells = self
            .arrays
            .iter()
            .map(|array| array.values.len())
            .fold(0usize, usize::saturating_add)
            .saturating_sub(old_len);
        let variable_cells = self
            .variables
            .iter()
            .map(|cell| match &cell.storage {
                EdVariableStorage::Global(_) => 1,
                EdVariableStorage::Player(values) | EdVariableStorage::Actor(values) => {
                    values.len()
                }
            })
            .fold(0usize, usize::saturating_add);
        let other_cells = array_cells.saturating_add(variable_cells);
        if other_cells.saturating_add(new_len) > MAX_EDUKE_VALUE_CELLS {
            return Err(EdukeVmError::AllocationLimit {
                name: self.arrays[index].name.clone(),
                cells: other_cells.saturating_add(new_len),
            });
        }
        let array_name = self.arrays[index].name.clone();
        let values = &mut self.arrays[index].values;
        if new_len > values.len() {
            values
                .try_reserve_exact(new_len - values.len())
                .map_err(|_| EdukeVmError::AllocationFailed {
                    kind: "array",
                    name: array_name,
                })?;
        }
        values.resize(new_len, 0);
        Ok(())
    }

    /// Number of registered gamevars.
    #[must_use]
    pub fn variable_count(&self) -> usize {
        self.variables.len()
    }

    /// Number of registered arrays.
    #[must_use]
    pub fn array_count(&self) -> usize {
        self.arrays.len()
    }

    /// Restore actor-scoped gamevars to their CON declaration defaults at
    /// level start while preserving global and player-scoped values.
    pub(crate) fn reset_actor_variables(
        &mut self,
        image: &ScriptImage,
    ) -> Result<(), EdukeVmError> {
        for definition in &image.eduke_variables {
            let index = self.variable_id(&definition.name)?;
            if let EdVariableStorage::Actor(values) = &mut self.variables[index].storage {
                let initial = resolve_token(image, &definition.name, &definition.initial_value)?;
                values.fill(initial);
            }
        }
        Ok(())
    }

    fn execute_from(
        &mut self,
        program: &EdukeProgram,
        start: usize,
        player: usize,
        mut actor: usize,
        instruction_budget: usize,
        state_depth_limit: usize,
        image: Option<&ScriptImage>,
        sound_event_active: bool,
        spawn_event_depth: usize,
        mut access: Option<&mut dyn EdukeStructAccess>,
    ) -> Result<EdExecution, EdukeVmError> {
        if start > program.instructions.len() {
            return Err(EdukeVmError::InvalidProgramCounter {
                address: start,
                length: program.instructions.len(),
            });
        }
        let mut pc = start;
        let mut calls = Vec::new();
        let mut sprite_loops: Vec<EdSpriteLoopFrame> = Vec::new();
        let mut steps = 0usize;
        while pc < program.instructions.len() {
            while sprite_loops.last().is_some_and(|frame| frame.after <= pc) {
                sprite_loops.pop();
            }
            if steps >= instruction_budget {
                return Err(EdukeVmError::InstructionBudgetExceeded {
                    budget: instruction_budget,
                });
            }
            let instruction = &program.instructions[pc];
            steps += 1;
            let mut pending_spawn_return = None;
            match instruction {
                EdInstruction::NoOp => pc += 1,
                EdInstruction::SetHudUniqueId(id) => {
                    let id = self.read_operand(id, player, actor, &mut access)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.set_hud_unique_id(id);
                    }
                    pc += 1;
                }
                EdInstruction::RotateSprite(operands) => {
                    if operands.len() != 12 {
                        return Err(EdukeVmError::InvalidInstructionArity {
                            instruction: "rotatesprite",
                            expected: 12,
                            actual: operands.len(),
                        });
                    }
                    let mut values = [0_i32; 12];
                    for (slot, operand) in values.iter_mut().zip(operands) {
                        *slot = self.read_operand(operand, player, actor, &mut access)?;
                    }
                    if let Some(host) = access.as_deref_mut() {
                        let unique_id = host.current_hud_unique_id();
                        host.draw_hud_sprite(HudSpriteDraw {
                            x: values[0],
                            y: values[1],
                            zoom: values[2],
                            angle: values[3],
                            tile: values[4],
                            shade: values[5],
                            palette: values[6],
                            orientation: values[7],
                            clip: [values[8], values[9], values[10], values[11]],
                            unique_id,
                        })?;
                    }
                    pc += 1;
                }
                EdInstruction::Minitext {
                    x,
                    y,
                    quote,
                    shade,
                    palette,
                } => {
                    let x = self.read_operand(x, player, actor, &mut access)?;
                    let y = self.read_operand(y, player, actor, &mut access)?;
                    let quote = self.read_operand(quote, player, actor, &mut access)?;
                    let quote_index = usize::try_from(quote).map_err(|_| {
                        EdukeVmError::QuoteIndexOutOfBounds {
                            index: usize::MAX,
                            extent: EDUKE_MAX_QUOTES,
                        }
                    })?;
                    let text = self.quote(quote_index)?.to_owned();
                    let shade = self.read_operand(shade, player, actor, &mut access)?;
                    let palette = self.read_operand(palette, player, actor, &mut access)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.draw_hud_text(HudTextDraw {
                            x,
                            y,
                            text,
                            shade,
                            palette,
                        })?;
                    }
                    pc += 1;
                }
                EdInstruction::GameText(operands) => {
                    if operands.len() != 11 && operands.len() != 12 {
                        return Err(EdukeVmError::InvalidInstructionArity {
                            instruction: "gametext",
                            expected: 11,
                            actual: operands.len(),
                        });
                    }
                    let mut values = [0_i32; 12];
                    for (slot, operand) in values.iter_mut().zip(operands) {
                        *slot = self.read_operand(operand, player, actor, &mut access)?;
                    }
                    let quote_index = usize::try_from(values[3]).map_err(|_| {
                        EdukeVmError::QuoteIndexOutOfBounds {
                            index: usize::MAX,
                            extent: EDUKE_MAX_QUOTES,
                        }
                    })?;
                    let text = self.quote(quote_index)?.to_owned();
                    if let Some(host) = access.as_deref_mut() {
                        host.draw_hud_gametext(HudGameTextDraw {
                            tile: values[0],
                            x: values[1],
                            y: values[2],
                            text,
                            shade: values[4],
                            palette: values[5],
                            orientation: values[6],
                            clip: [values[7], values[8], values[9], values[10]],
                            zoom: if operands.len() == 12 {
                                values[11]
                            } else {
                                65536
                            },
                        })?;
                    }
                    pc += 1;
                }
                EdInstruction::DigitalNumber(operands) => {
                    if operands.len() != 11 && operands.len() != 12 {
                        return Err(EdukeVmError::InvalidInstructionArity {
                            instruction: "digitalnumber",
                            expected: 11,
                            actual: operands.len(),
                        });
                    }
                    let mut values = [0_i32; 12];
                    for (slot, operand) in values.iter_mut().zip(operands) {
                        *slot = self.read_operand(operand, player, actor, &mut access)?;
                    }
                    if let Some(host) = access.as_deref_mut() {
                        host.draw_hud_number(HudNumberDraw {
                            tile: values[0],
                            x: values[1],
                            y: values[2],
                            number: values[3],
                            shade: values[4],
                            palette: values[5],
                            orientation: values[6],
                            clip: [values[7], values[8], values[9], values[10]],
                            zoom: if operands.len() == 12 {
                                values[11]
                            } else {
                                65536
                            },
                        })?;
                    }
                    pc += 1;
                }
                EdInstruction::KillIt => {
                    let kill_actor_id = if actor == NO_EVENT_CONTEXT {
                        None
                    } else {
                        Some(
                            i32::try_from(actor).map_err(|_| EdukeVmError::IndexOutOfBounds {
                                kind: "actor",
                                name: "THISACTOR".to_owned(),
                                index: actor,
                                extent: MAXSPRITES,
                            })?,
                        )
                    };
                    return Ok(EdExecution {
                        steps,
                        returned: false,
                        killed_actor: true,
                        kill_actor_id,
                        restart_player: false,
                    });
                }
                EdInstruction::PlaySound {
                    sound,
                    mode,
                    actor: source_actor,
                } => {
                    let mut sound = self.read_operand(sound, player, actor, &mut access)?;
                    let mut actor_id = if let Some(source_actor) = source_actor {
                        self.read_operand(source_actor, player, actor, &mut access)?
                    } else {
                        event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?
                    };
                    if let Some(image) = image.filter(|image| {
                        image.dialect == ConDialect::EDuke32
                            && !sound_event_active
                            && image
                                .eduke_event_entries
                                .iter()
                                .any(|(name, _, _)| name.eq_ignore_ascii_case("EVENT_SOUND"))
                    }) && let Some(host) = access.as_deref_mut()
                    {
                        // The room currently presents seat zero, matching
                        // EDuke32's screenpeek player context for this hook.
                        let sound_player = 0;
                        if *mode == EdSoundMode::Global {
                            actor_id = host.get_member(EdStructKind::Player, 0, "i", None)?;
                        }
                        let sound_actor = usize::try_from(actor_id).map_err(|_| {
                            EdukeVmError::IndexOutOfBounds {
                                kind: "actor",
                                name: "EVENT_SOUND THISACTOR".to_owned(),
                                index: usize::MAX,
                                extent: MAXSPRITES,
                            }
                        })?;
                        self.set_variable("RETURN", sound_player, sound_actor, sound)?;
                        let hook_executions = self.execute_registered_event_hooks_inner(
                            image,
                            "EVENT_SOUND",
                            sound_player,
                            sound_actor,
                            instruction_budget.saturating_sub(steps),
                            state_depth_limit,
                            true,
                            spawn_event_depth,
                            Some(host),
                        )?;
                        for hook_execution in hook_executions {
                            steps = steps.saturating_add(hook_execution.steps);
                        }
                        sound = self.get_variable("RETURN", sound_player, sound_actor)?;
                    }
                    if sound != -1
                        && let Some(host) = access.as_deref_mut()
                    {
                        host.play_sound(actor_id, sound, *mode)?;
                    }
                    pc += 1;
                }
                EdInstruction::SetActorSoundPitch {
                    actor: source_actor,
                    sound,
                    pitch_offset,
                } => {
                    let source_actor =
                        self.read_operand(source_actor, player, actor, &mut access)?;
                    let sound = self.read_operand(sound, player, actor, &mut access)?;
                    let pitch_offset =
                        self.read_operand(pitch_offset, player, actor, &mut access)?;
                    if let Some(host) = access.as_deref_mut() {
                        let context_actor =
                            event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                        host.set_actor_sound_pitch(
                            context_actor,
                            source_actor,
                            sound,
                            pitch_offset,
                        )?;
                    }
                    pc += 1;
                }
                EdInstruction::StopSound {
                    sound,
                    actor: source_actor,
                } => {
                    let sound = self.read_operand(sound, player, actor, &mut access)?;
                    let actor_specific = source_actor.is_some();
                    let actor_id = if let Some(source_actor) = source_actor {
                        self.read_operand(source_actor, player, actor, &mut access)?
                    } else {
                        event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?
                    };
                    if let Some(host) = access.as_deref_mut() {
                        if actor_specific {
                            host.stop_actor_sound(actor_id, sound)?;
                        } else {
                            host.stop_sound(actor_id, sound)?;
                        }
                    }
                    pc += 1;
                }
                EdInstruction::AddPlayerHealth { amount } => {
                    let amount = self.read_operand(amount, player, actor, &mut access)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.add_player_health(player, actor, amount)?;
                    }
                    pc += 1;
                }
                EdInstruction::PlayerWeapon {
                    add_weapon,
                    weapon,
                    ammo,
                } => {
                    let weapon = self.read_operand(weapon, player, actor, &mut access)?;
                    let ammo = self.read_operand(ammo, player, actor, &mut access)?;
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    if let Some(host) = access.as_deref_mut()
                        && !host.player_weapon_op(player, actor_id, *add_weapon, weapon, ammo)?
                    {
                        return Ok(EdExecution {
                            steps,
                            returned: true,
                            killed_actor: false,
                            kill_actor_id: None,
                            restart_player: false,
                        });
                    }
                    pc += 1;
                }
                EdInstruction::PlayerInventory { item, amount } => {
                    let item = self.read_operand(item, player, actor, &mut access)?;
                    let amount = self.read_operand(amount, player, actor, &mut access)?;
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.add_player_inventory(player, actor_id, item, amount)?;
                    }
                    pc += 1;
                }
                EdInstruction::Debris { tile, count } => {
                    let tile = self.read_operand(tile, player, actor, &mut access)?;
                    let count = self.read_operand(count, player, actor, &mut access)?;
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.spawn_debris(actor_id, tile, count)?;
                        if let Some(seed) = host.game_random_seed() {
                            self.game_random_seed = seed;
                        }
                    }
                    pc += 1;
                }
                EdInstruction::Guts { tile, count } => {
                    let tile = self.read_operand(tile, player, actor, &mut access)?;
                    let count = self.read_operand(count, player, actor, &mut access)?;
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.spawn_guts(player, actor_id, tile, count)?;
                        if let Some(seed) = host.game_random_seed() {
                            self.game_random_seed = seed;
                        }
                    }
                    pc += 1;
                }
                EdInstruction::LotsOfGlass { count } => {
                    let count = self.read_operand(count, player, actor, &mut access)?;
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.spawn_lots_of_glass(actor_id, count)?;
                        if let Some(seed) = host.game_random_seed() {
                            self.game_random_seed = seed;
                        }
                    }
                    pc += 1;
                }
                EdInstruction::SpawnCeilingGlass {
                    sprite,
                    sector,
                    count,
                } => {
                    let sprite = self.read_operand(sprite, player, actor, &mut access)?;
                    let sector = self.read_operand(sector, player, actor, &mut access)?;
                    let count = self.read_operand(count, player, actor, &mut access)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.spawn_ceiling_glass(sprite, sector, count)?;
                        if let Some(seed) = host.game_random_seed() {
                            self.game_random_seed = seed;
                        }
                    }
                    pc += 1;
                }
                EdInstruction::ItemSpray { kind, count } => {
                    let count = self.read_operand(count, player, actor, &mut access)?;
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.spawn_item_spray(actor_id, *kind, count)?;
                        if let Some(seed) = host.game_random_seed() {
                            self.game_random_seed = seed;
                        }
                    }
                    pc += 1;
                }
                EdInstruction::Operate => {
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.request_actor_operate(actor_id)?;
                    }
                    pc += 1;
                }
                EdInstruction::OperateMasterSwitches { lotag } => {
                    let lotag = self.read_operand(lotag, player, actor, &mut access)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.operate_master_switches(lotag)?;
                    }
                    pc += 1;
                }
                EdInstruction::RespawnHitag => {
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.respawn_hitag(actor_id)?;
                    }
                    pc += 1;
                }
                EdInstruction::ResetPlayer => {
                    let restart = if let Some(host) = access.as_deref_mut() {
                        host.request_player_restart(player)?
                    } else {
                        false
                    };
                    if restart {
                        return Ok(EdExecution {
                            steps,
                            returned: true,
                            killed_actor: false,
                            kill_actor_id: None,
                            restart_player: true,
                        });
                    }
                    pc += 1;
                }
                EdInstruction::GetTicks { target } => {
                    // EDuke32's command reads an unsynchronized wall clock.
                    // Room keeps script execution deterministic, so the live
                    // host provides elapsed time derived from its simulation clock.
                    let ticks = access
                        .as_deref_mut()
                        .and_then(|host| host.predefined_variable("getticks"))
                        .unwrap_or(0);
                    self.write_lvalue(target, player, actor, ticks, &mut access)?;
                    pc += 1;
                }
                EdInstruction::SetGamePalette { palette } => {
                    let palette = self.read_operand(palette, player, actor, &mut access)?;
                    access
                        .as_deref_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?
                        .set_game_palette(player, palette)?;
                    pc += 1;
                }
                EdInstruction::SetTimerRate { rate } => {
                    let rate = self.read_operand(rate, player, actor, &mut access)?;
                    access
                        .as_deref_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?
                        .set_timer_rate(rate)?;
                    pc += 1;
                }
                EdInstruction::EndOfGame { delay } => {
                    let delay = self.read_operand(delay, player, actor, &mut access)?;
                    access
                        .as_deref_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?
                        .begin_end_of_game(player, delay)?;
                    pc += 1;
                }
                EdInstruction::PlayerTip => {
                    access
                        .as_deref_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?
                        .show_tip(player)?;
                    pc += 1;
                }
                EdInstruction::HitRadius { radius, damage } => {
                    let radius = self.read_operand(radius, player, actor, &mut access)?;
                    let mut damage_values = [0; 4];
                    for (index, operand) in damage.iter().enumerate() {
                        damage_values[index] =
                            self.read_operand(operand, player, actor, &mut access)?;
                    }
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    let host = access
                        .as_deref_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?;
                    host.apply_hitradius(actor_id, radius, damage_values)?;
                    if let Some(seed) = host.game_random_seed() {
                        self.game_random_seed = seed;
                    }
                    pc += 1;
                }
                EdInstruction::PlayerQuickKick => {
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    let host = access
                        .as_deref_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?;
                    host.request_player_quick_kick(actor_id)?;
                    pc += 1;
                }
                EdInstruction::PlayerStomp => {
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    let host = access
                        .as_deref_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?;
                    host.request_player_stomp(player, actor_id)?;
                    pc += 1;
                }
                EdInstruction::PlayerWack => {
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    let angle = (128 - (self.next_game_random(&mut access) & 255)) >> 1;
                    let host = access
                        .as_deref_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?;
                    host.request_player_wack(player, actor_id, angle)?;
                    pc += 1;
                }
                EdInstruction::LineIntersect {
                    origin,
                    destination,
                    line,
                    outputs,
                } => {
                    let origin = [
                        self.read_operand(&origin[0], player, actor, &mut access)?,
                        self.read_operand(&origin[1], player, actor, &mut access)?,
                        self.read_operand(&origin[2], player, actor, &mut access)?,
                    ];
                    let destination = [
                        self.read_operand(&destination[0], player, actor, &mut access)?,
                        self.read_operand(&destination[1], player, actor, &mut access)?,
                        self.read_operand(&destination[2], player, actor, &mut access)?,
                    ];
                    let line = [
                        self.read_operand(&line[0], player, actor, &mut access)?,
                        self.read_operand(&line[1], player, actor, &mut access)?,
                        self.read_operand(&line[2], player, actor, &mut access)?,
                        self.read_operand(&line[3], player, actor, &mut access)?,
                    ];
                    let intersection = line_intersect_3d(origin, destination, line);
                    if let Some([x, y, z]) = intersection {
                        self.write_lvalue(&outputs[0], player, actor, x, &mut access)?;
                        self.write_lvalue(&outputs[1], player, actor, y, &mut access)?;
                        self.write_lvalue(&outputs[2], player, actor, z, &mut access)?;
                    }
                    self.write_lvalue(
                        &outputs[3],
                        player,
                        actor,
                        i32::from(intersection.is_some()),
                        &mut access,
                    )?;
                    pc += 1;
                }
                EdInstruction::TossWeapon => {
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    let host = access
                        .as_deref_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?;
                    host.toss_player_weapon(player, actor_id)?;
                    if let Some(seed) = host.game_random_seed() {
                        self.game_random_seed = seed;
                    }
                    pc += 1;
                }
                EdInstruction::SetActorAction { action } => {
                    let action = self.read_operand(action, player, actor, &mut access)?;
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.set_actor_action(actor_id, action)?;
                    }
                    pc += 1;
                }
                EdInstruction::SetActorMove { movement, flags } => {
                    let movement = self.read_operand(movement, player, actor, &mut access)?;
                    let flags = self.read_operand(flags, player, actor, &mut access)?;
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.set_actor_move(actor_id, movement, flags)?;
                    }
                    pc += 1;
                }
                EdInstruction::SetActorAi { ai } => {
                    let ai = self.read_operand(ai, player, actor, &mut access)?;
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.set_actor_ai(actor_id, ai)?;
                    }
                    pc += 1;
                }
                EdInstruction::SetActorMember { member, value } => {
                    let value = self.read_operand(value, player, actor, &mut access)?;
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.set_actor_member(actor_id, *member, value)?;
                    }
                    pc += 1;
                }
                EdInstruction::AddActorKills { amount } => {
                    let amount = self.read_operand(amount, player, actor, &mut access)?;
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    let player_id =
                        event_context_value("player", "THISPLAYER", player, EDUKE_MAXPLAYERS)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.add_actor_kills(player_id, actor_id, amount)?;
                    }
                    pc += 1;
                }
                EdInstruction::RestoreActorPalette => {
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.restore_actor_palette(actor_id)?;
                    }
                    pc += 1;
                }
                EdInstruction::SetActorSizeTo { x, y } => {
                    let x = self.read_operand(x, player, actor, &mut access)?;
                    let y = self.read_operand(y, player, actor, &mut access)?;
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.set_actor_size_to(actor_id, x, y)?;
                    }
                    pc += 1;
                }
                EdInstruction::FallActor => {
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.fall_actor(actor_id)?;
                    }
                    pc += 1;
                }
                EdInstruction::ResetActorCounter { action } => {
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.reset_actor_counter(actor_id, *action)?;
                    }
                    pc += 1;
                }
                EdInstruction::SetActorSize { x, y } => {
                    let x = self.read_operand(x, player, actor, &mut access)?;
                    let y = self.read_operand(y, player, actor, &mut access)?;
                    let actor_id = event_context_value("actor", "THISACTOR", actor, MAXSPRITES)?;
                    if let Some(host) = access.as_deref_mut() {
                        host.set_actor_size(actor_id, x, y)?;
                    }
                    pc += 1;
                }
                EdInstruction::Assign { target, value } => {
                    let value = self.read_operand(value, player, actor, &mut access)?;
                    self.write_lvalue(target, player, actor, value, &mut access)?;
                    pc += 1;
                }
                EdInstruction::ResizeArray { name, length } => {
                    let length = self.read_operand(length, player, actor, &mut access)?;
                    let length =
                        usize::try_from(length).map_err(|_| EdukeVmError::NegativeArraySize {
                            name: name.clone(),
                            length,
                        })?;
                    self.resize_array(name, length)?;
                    pc += 1;
                }
                EdInstruction::Binary {
                    target,
                    operator,
                    right,
                } => {
                    let left = self.read_lvalue(target, player, actor, &mut access)?;
                    let right = self.read_operand(right, player, actor, &mut access)?;
                    let value = evaluate_binary("instruction", *operator, left, right)?;
                    self.write_lvalue(target, player, actor, value, &mut access)?;
                    pc += 1;
                }
                EdInstruction::JumpIf {
                    left,
                    comparison,
                    right,
                    target,
                } => {
                    let left = self.read_operand(left, player, actor, &mut access)?;
                    let right = self.read_operand(right, player, actor, &mut access)?;
                    pc = if compare_values(*comparison, left, right) {
                        *target
                    } else {
                        pc + 1
                    };
                }
                EdInstruction::IfRandom { threshold, target } => {
                    let threshold = self.read_operand(threshold, player, actor, &mut access)?;
                    let roll = self.next_game_random(&mut access) & 0xff;
                    pc = if roll <= threshold { pc + 1 } else { *target };
                }
                EdInstruction::Jump { target } => pc = *target,
                EdInstruction::Switch {
                    value,
                    cases,
                    default_target,
                } => {
                    let value = self.read_operand(value, player, actor, &mut access)?;
                    pc = cases
                        .iter()
                        .find(|(case, _)| *case == value)
                        .map_or(*default_target, |(_, target)| *target);
                }
                EdInstruction::CallState { name } => {
                    let entry = program
                        .states
                        .get(name)
                        .copied()
                        .ok_or_else(|| EdukeVmError::UnknownState { name: name.clone() })?;
                    if calls.len() >= state_depth_limit {
                        return Err(EdukeVmError::StateCallDepthExceeded {
                            limit: state_depth_limit,
                        });
                    }
                    calls.push(pc + 1);
                    pc = entry;
                }
                EdInstruction::Return => match calls.pop() {
                    Some(return_pc) => pc = return_pc,
                    None => {
                        return Ok(EdExecution {
                            steps,
                            returned: true,
                            killed_actor: false,
                            kill_actor_id: None,
                            restart_player: false,
                        });
                    }
                },
                EdInstruction::EndEvent => {
                    return Ok(EdExecution {
                        steps,
                        returned: false,
                        killed_actor: false,
                        kill_actor_id: None,
                        restart_player: false,
                    });
                }
                EdInstruction::SetQuote { quote, text } => {
                    self.set_quote(*quote, text)?;
                    pc += 1;
                }
                EdInstruction::Qsprintf {
                    output_quote,
                    format_quote,
                    arguments,
                } => {
                    let mut values = Vec::new();
                    values.try_reserve_exact(arguments.len()).map_err(|_| {
                        EdukeVmError::AllocationFailed {
                            kind: "qsprintf arguments",
                            name: format_quote.to_string(),
                        }
                    })?;
                    for argument in arguments {
                        values.push(self.read_operand(argument, player, actor, &mut access)?);
                    }
                    let output = format_eduke_quote(self.quote(*format_quote)?, &values)?;
                    self.set_quote(*output_quote, &output)?;
                    pc += 1;
                }
                EdInstruction::GetStructMember {
                    kind,
                    id,
                    member,
                    member_index,
                    target,
                } => {
                    let id = self.read_operand(id, player, actor, &mut access)?;
                    let member_index = member_index
                        .as_ref()
                        .map(|index| self.read_operand(index, player, actor, &mut access))
                        .transpose()?;
                    let host = access
                        .as_deref_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?;
                    let value = if *kind == EdStructKind::UserDef
                        && member.eq_ignore_ascii_case("return")
                        && member_index.unwrap_or(0) == 0
                    {
                        self.get_variable("RETURN", player, actor)?
                    } else {
                        host.get_member(*kind, id, member, member_index)?
                    };
                    self.write_lvalue(target, player, actor, value, &mut access)?;
                    pc += 1;
                }
                EdInstruction::SetStructMember {
                    kind,
                    id,
                    member,
                    member_index,
                    value,
                } => {
                    let id = self.read_operand(id, player, actor, &mut access)?;
                    let member_index = member_index
                        .as_ref()
                        .map(|index| self.read_operand(index, player, actor, &mut access))
                        .transpose()?;
                    let value = self.read_operand(value, player, actor, &mut access)?;
                    let host = access
                        .as_deref_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?;
                    if *kind == EdStructKind::UserDef
                        && member.eq_ignore_ascii_case("return")
                        && member_index.unwrap_or(0) == 0
                    {
                        self.set_variable("RETURN", player, actor, value)?;
                    } else {
                        host.set_member(*kind, id, member, member_index, value)?;
                    }
                    if *kind == EdStructKind::UserDef && member.eq_ignore_ascii_case("vm_sprite") {
                        if let Some(current_actor) = host.current_actor() {
                            actor = event_actor_context(current_actor)?;
                        }
                    }
                    pc += 1;
                }
                EdInstruction::GetScopedVariable {
                    scope,
                    name,
                    id,
                    target,
                } => {
                    let raw_index = self.read_operand(id, player, actor, &mut access)?;
                    let index = scoped_index(raw_index, *scope, name)?;
                    let value = self.get_scoped_variable(name, *scope, index)?;
                    self.write_lvalue(target, player, actor, value, &mut access)?;
                    pc += 1;
                }
                EdInstruction::SetScopedVariable {
                    scope,
                    name,
                    id,
                    value,
                } => {
                    let raw_index = self.read_operand(id, player, actor, &mut access)?;
                    let index = scoped_index(raw_index, *scope, name)?;
                    let value = self.read_operand(value, player, actor, &mut access)?;
                    self.set_scoped_variable(name, *scope, index, value)?;
                    pc += 1;
                }
                EdInstruction::RandomRange {
                    target,
                    maximum,
                    display,
                } => {
                    let maximum = match maximum {
                        Some(maximum) => self.read_operand(maximum, player, actor, &mut access)?,
                        None => 32767,
                    };
                    let value = if maximum < 0 {
                        0
                    } else if *display {
                        self.random_range(maximum, true)
                    } else {
                        let random = self.next_game_random(&mut access) & 0x7fff;
                        random_range_from_bits(maximum, random)
                    };
                    self.write_lvalue(target, player, actor, value, &mut access)?;
                    pc += 1;
                }
                EdInstruction::RandomBits {
                    target,
                    mask,
                    display,
                } => {
                    let mask = self.read_operand(mask, player, actor, &mut access)?;
                    let value = if *display {
                        self.random_bits(mask, true)
                    } else {
                        self.next_game_random(&mut access) & mask
                    };
                    self.write_lvalue(target, player, actor, value, &mut access)?;
                    pc += 1;
                }
                EdInstruction::MulScale {
                    target,
                    left,
                    right,
                    shift,
                } => {
                    let left = self.read_operand(left, player, actor, &mut access)?;
                    let right = self.read_operand(right, player, actor, &mut access)?;
                    self.write_lvalue(
                        target,
                        player,
                        actor,
                        build_mulscale(left, right, *shift),
                        &mut access,
                    )?;
                    pc += 1;
                }
                EdInstruction::DivScale {
                    target,
                    left,
                    right,
                    shift,
                } => {
                    let left = self.read_operand(left, player, actor, &mut access)?;
                    let right = self.read_operand(right, player, actor, &mut access)?;
                    self.write_lvalue(
                        target,
                        player,
                        actor,
                        build_divscale(left, right, *shift),
                        &mut access,
                    )?;
                    pc += 1;
                }
                EdInstruction::ScaleVar {
                    target,
                    value,
                    numerator,
                    denominator,
                } => {
                    let value = self.read_operand(value, player, actor, &mut access)?;
                    let numerator = self.read_operand(numerator, player, actor, &mut access)?;
                    let denominator = self.read_operand(denominator, player, actor, &mut access)?;
                    let result = if denominator == 0 {
                        0
                    } else {
                        (i64::from(value) * i64::from(numerator) / i64::from(denominator)) as i32
                    };
                    self.write_lvalue(target, player, actor, result, &mut access)?;
                    pc += 1;
                }
                EdInstruction::BuildSin { target, angle } => {
                    let angle = self.read_operand(angle, player, actor, &mut access)?;
                    self.write_lvalue(target, player, actor, build_sin_q14(angle), &mut access)?;
                    pc += 1;
                }
                EdInstruction::BuildCos { target, angle } => {
                    let angle = self.read_operand(angle, player, actor, &mut access)?;
                    self.write_lvalue(target, player, actor, build_cos_q14(angle), &mut access)?;
                    pc += 1;
                }
                EdInstruction::BuildGetAngle { target, x, y } => {
                    let x = self.read_operand(x, player, actor, &mut access)?;
                    let y = self.read_operand(y, player, actor, &mut access)?;
                    self.write_lvalue(target, player, actor, build_getangle(x, y), &mut access)?;
                    pc += 1;
                }
                EdInstruction::GetSlopeZ {
                    target,
                    sector,
                    x,
                    y,
                    ceiling,
                } => {
                    let sector = self.read_operand(sector, player, actor, &mut access)?;
                    let x = self.read_operand(x, player, actor, &mut access)?;
                    let y = self.read_operand(y, player, actor, &mut access)?;
                    let host = access
                        .as_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?;
                    let (floor_z, ceiling_z) = host.get_slope_z(sector, x, y)?;
                    self.write_lvalue(
                        target,
                        player,
                        actor,
                        if *ceiling { ceiling_z } else { floor_z },
                        &mut access,
                    )?;
                    pc += 1;
                }
                EdInstruction::SectorOfWall { target, wall } => {
                    let wall = self.read_operand(wall, player, actor, &mut access)?;
                    let sector = access
                        .as_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?
                        .sector_of_wall(wall)?;
                    self.write_lvalue(target, player, actor, sector, &mut access)?;
                    pc += 1;
                }
                EdInstruction::CanSee { target, start, end } => {
                    let mut start_values = [0; 4];
                    let mut end_values = [0; 4];
                    for (output, operand) in start_values.iter_mut().zip(start) {
                        *output = self.read_operand(operand, player, actor, &mut access)?;
                    }
                    for (output, operand) in end_values.iter_mut().zip(end) {
                        *output = self.read_operand(operand, player, actor, &mut access)?;
                    }
                    let host = access
                        .as_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?;
                    let visible = host.can_see(start_values, end_values)?;
                    self.write_lvalue(target, player, actor, i32::from(visible), &mut access)?;
                    pc += 1;
                }
                EdInstruction::HitScan {
                    ray,
                    outputs,
                    clipmask,
                } => {
                    let mut values = [0; 7];
                    for (value, operand) in values.iter_mut().zip(ray) {
                        *value = self.read_operand(operand, player, actor, &mut access)?;
                    }
                    let clipmask = self.read_operand(clipmask, player, actor, &mut access)? as u32;
                    let host = access
                        .as_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?;
                    let result = host.hit_scan(
                        [values[0], values[1], values[2], values[3]],
                        [values[4], values[5], values[6]],
                        clipmask,
                    )?;
                    for (target, value) in outputs.iter().zip(result) {
                        self.write_lvalue(target, player, actor, value, &mut access)?;
                    }
                    pc += 1;
                }
                EdInstruction::GetZRange {
                    point,
                    outputs,
                    wall_distance,
                    clipmask,
                } => {
                    let mut point_values = [0; 4];
                    for (value, operand) in point_values.iter_mut().zip(point) {
                        *value = self.read_operand(operand, player, actor, &mut access)?;
                    }
                    let wall_distance =
                        self.read_operand(wall_distance, player, actor, &mut access)?;
                    let clipmask = self.read_operand(clipmask, player, actor, &mut access)? as u32;
                    let result = access
                        .as_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?
                        .get_z_range(point_values, wall_distance, clipmask)?;
                    for (target, value) in outputs.iter().zip(result) {
                        self.write_lvalue(target, player, actor, value, &mut access)?;
                    }
                    pc += 1;
                }
                EdInstruction::ForAllSpritesBegin { target, end } => {
                    let host = access
                        .as_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?;
                    let sprite_ids = host.active_sprite_ids()?;
                    if let Some(first) = sprite_ids.first().copied() {
                        self.write_lvalue(target, player, actor, first, &mut access)?;
                        sprite_loops.push(EdSpriteLoopFrame {
                            begin: pc,
                            after: *end,
                            sprite_ids,
                            cursor: 1,
                        });
                        pc += 1;
                    } else {
                        pc = *end;
                    }
                }
                EdInstruction::ForAllSpritesNext { target, begin } => {
                    let Some(frame) = sprite_loops
                        .last_mut()
                        .filter(|frame| frame.begin == *begin)
                    else {
                        return Err(EdukeVmError::InvalidProgramCounter {
                            address: pc,
                            length: program.instructions.len(),
                        });
                    };
                    if let Some(sprite) = frame.sprite_ids.get(frame.cursor).copied() {
                        frame.cursor += 1;
                        self.write_lvalue(target, player, actor, sprite, &mut access)?;
                        pc = frame.begin + 1;
                    } else {
                        sprite_loops.pop();
                        pc += 1;
                    }
                }
                EdInstruction::UpdateSector { target, x, y, z } => {
                    let hint = self.read_lvalue(target, player, actor, &mut access)?;
                    let x = self.read_operand(x, player, actor, &mut access)?;
                    let y = self.read_operand(y, player, actor, &mut access)?;
                    let z = z
                        .as_ref()
                        .map(|z| self.read_operand(z, player, actor, &mut access))
                        .transpose()?;
                    let host = access
                        .as_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?;
                    let sector = host.update_sector(hint, x, y, z)?;
                    self.write_lvalue(target, player, actor, sector, &mut access)?;
                    pc += 1;
                }
                EdInstruction::SpriteDistance {
                    target,
                    first,
                    second,
                    three_dimensional,
                } => {
                    let first = self.read_operand(first, player, actor, &mut access)?;
                    let second = self.read_operand(second, player, actor, &mut access)?;
                    let host = access
                        .as_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?;
                    let distance = host.sprite_distance(first, second, *three_dimensional)?;
                    self.write_lvalue(target, player, actor, distance, &mut access)?;
                    pc += 1;
                }
                EdInstruction::FindNearActor3D {
                    tile,
                    distance,
                    target,
                } => {
                    let tile = self.read_operand(tile, player, actor, &mut access)?;
                    let distance = self.read_operand(distance, player, actor, &mut access)?;
                    let source_actor =
                        i32::try_from(actor).map_err(|_| EdukeVmError::IndexOutOfBounds {
                            kind: "actor",
                            name: "THISACTOR".to_owned(),
                            index: actor,
                            extent: MAXSPRITES,
                        })?;
                    let found = access
                        .as_deref_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?
                        .find_near_actor_3d(source_actor, tile, distance)?;
                    self.write_lvalue(target, player, actor, found, &mut access)?;
                    pc += 1;
                }
                EdInstruction::SpriteListRead {
                    target,
                    kind,
                    index,
                } => {
                    let index = self.read_operand(index, player, actor, &mut access)?;
                    let host = access
                        .as_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?;
                    let sprite = host.read_sprite_list(*kind, index)?;
                    self.write_lvalue(target, player, actor, sprite, &mut access)?;
                    pc += 1;
                }
                EdInstruction::SpriteListChange {
                    sprite,
                    index,
                    status,
                } => {
                    let sprite = self.read_operand(sprite, player, actor, &mut access)?;
                    let index = self.read_operand(index, player, actor, &mut access)?;
                    let host = access
                        .as_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?;
                    host.change_sprite_list(sprite, index, *status)?;
                    pc += 1;
                }
                EdInstruction::SetSpritePosition { sprite, x, y, z } => {
                    let sprite = self.read_operand(sprite, player, actor, &mut access)?;
                    let x = self.read_operand(x, player, actor, &mut access)?;
                    let y = self.read_operand(y, player, actor, &mut access)?;
                    let z = self.read_operand(z, player, actor, &mut access)?;
                    let host = access
                        .as_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?;
                    host.set_sprite_position(sprite, x, y, z)?;
                    pc += 1;
                }
                EdInstruction::SpawnActor {
                    parent,
                    tile,
                    return_sprite,
                } => {
                    let parent = self.read_operand(parent, player, actor, &mut access)?;
                    let tile = self.read_operand(tile, player, actor, &mut access)?;
                    let host = access
                        .as_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?;
                    let sprite = host.spawn_actor(parent, tile)?;
                    if *return_sprite {
                        pending_spawn_return = Some(sprite);
                    }
                    pc += 1;
                }
                EdInstruction::ShootActor {
                    parent,
                    tile,
                    vertical_velocity,
                    return_sprite,
                } => {
                    let parent = self.read_operand(parent, player, actor, &mut access)?;
                    let tile = self.read_operand(tile, player, actor, &mut access)?;
                    let vertical_velocity = vertical_velocity
                        .as_ref()
                        .map(|value| self.read_operand(value, player, actor, &mut access))
                        .transpose()?;
                    let host = access
                        .as_deref_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?;
                    let sprite = host.shoot_actor(parent, tile, vertical_velocity)?;
                    if *return_sprite {
                        pending_spawn_return = Some(sprite);
                    }
                    pc += 1;
                }
                EdInstruction::Abs { target } => {
                    let value = self
                        .read_lvalue(target, player, actor, &mut access)?
                        .wrapping_abs();
                    self.write_lvalue(target, player, actor, value, &mut access)?;
                    pc += 1;
                }
                EdInstruction::Clamp {
                    target,
                    minimum,
                    maximum,
                } => {
                    let value = self.read_lvalue(target, player, actor, &mut access)?;
                    let minimum = self.read_operand(minimum, player, actor, &mut access)?;
                    let maximum = self.read_operand(maximum, player, actor, &mut access)?;
                    if minimum > maximum {
                        return Err(EdukeVmError::InvalidClampRange { minimum, maximum });
                    }
                    let clamped = value.max(minimum).min(maximum);
                    self.write_lvalue(target, player, actor, clamped, &mut access)?;
                    pc += 1;
                }
                EdInstruction::CalculateHypotenuse { target, x, y } => {
                    let x = i128::from(self.read_operand(x, player, actor, &mut access)?);
                    let y = i128::from(self.read_operand(y, player, actor, &mut access)?);
                    let squared = (x * x + y * y) as u128;
                    let length = integer_sqrt(squared).min(i32::MAX as u128) as i32;
                    self.write_lvalue(target, player, actor, length, &mut access)?;
                    pc += 1;
                }
                EdInstruction::Sqrt { input, target } => {
                    let input = self.read_operand(input, player, actor, &mut access)?;
                    let value = if input <= 0 {
                        0
                    } else {
                        integer_sqrt(input as u128).min(i32::MAX as u128) as i32
                    };
                    self.write_lvalue(target, player, actor, value, &mut access)?;
                    pc += 1;
                }
                EdInstruction::EchoQuote { quote } => {
                    let message = self.quote(*quote)?.to_owned();
                    if self.console_output.len() == MAX_EDUKE_CONSOLE_MESSAGES {
                        self.console_output.pop_front();
                    }
                    self.console_output.push_back(message);
                    pc += 1;
                }
                EdInstruction::DisplayQuote { quote } => {
                    let message = self.quote(*quote)?.to_owned();
                    if let Some(host) = access.as_deref_mut() {
                        host.display_quote(player, *quote)?;
                    }
                    if self.last_displayed_quote != Some(*quote) {
                        if self.console_output.len() == MAX_EDUKE_CONSOLE_MESSAGES {
                            self.console_output.pop_front();
                        }
                        self.console_output.push_back(message);
                        self.last_displayed_quote = Some(*quote);
                    }
                    pc += 1;
                }
                EdInstruction::PaletteFrom { values } => {
                    let mut resolved = [0; 4];
                    for (index, operand) in values.iter().enumerate() {
                        resolved[index] = self.read_operand(operand, player, actor, &mut access)?;
                    }
                    if let Some(host) = access.as_deref_mut() {
                        host.palette_from(player, resolved)?;
                    }
                    pc += 1;
                }
            }
            if image.is_some_and(|image| image.dialect == ConDialect::EDuke32)
                && let Some(host) = access.as_deref_mut()
            {
                let spawned = host.take_spawned_actor_ids();
                if !spawned.is_empty() {
                    let mut nested_budget = instruction_budget.saturating_sub(steps);
                    for spawned_id in spawned {
                        let has_spawn_hooks = image.is_some_and(|image| {
                            image.eduke_event_entries.iter().any(|(name, _, _)| {
                                name.eq_ignore_ascii_case("EVENT_EGS")
                                    || name.eq_ignore_ascii_case("EVENT_SPAWN")
                            })
                        });
                        if !has_spawn_hooks {
                            continue;
                        }
                        if spawn_event_depth >= EDUKE_SPAWN_EVENT_DEPTH_LIMIT {
                            return Err(EdukeVmError::SpawnEventDepthExceeded {
                                limit: EDUKE_SPAWN_EVENT_DEPTH_LIMIT,
                            });
                        }
                        let spawned_actor = usize::try_from(spawned_id).map_err(|_| {
                            EdukeVmError::IndexOutOfBounds {
                                kind: "actor",
                                name: "spawn event THISACTOR".to_owned(),
                                index: usize::MAX,
                                extent: MAXSPRITES,
                            }
                        })?;
                        let spawn_player = host.closest_player_for_actor(spawned_id, player);
                        let previous_actor = host.current_actor();
                        let dispatch_result = (|| {
                            for event in ["EVENT_EGS", "EVENT_SPAWN"] {
                                let Some(image) = image.filter(|image| {
                                    image
                                        .eduke_event_entries
                                        .iter()
                                        .any(|(name, _, _)| name.eq_ignore_ascii_case(event))
                                }) else {
                                    continue;
                                };
                                if nested_budget == 0 {
                                    return Err(EdukeVmError::InstructionBudgetExceeded {
                                        budget: instruction_budget,
                                    });
                                }
                                host.set_current_actor(spawned_id);
                                self.set_variable("RETURN", spawn_player, spawned_actor, 0)?;
                                let executions = self.execute_registered_event_hooks_inner(
                                    image,
                                    event,
                                    spawn_player,
                                    spawned_actor,
                                    nested_budget,
                                    state_depth_limit,
                                    sound_event_active,
                                    spawn_event_depth + 1,
                                    Some(&mut *host),
                                )?;
                                let used = executions.iter().fold(0_usize, |total, execution| {
                                    total.saturating_add(execution.steps)
                                });
                                steps = steps.saturating_add(used);
                                nested_budget = nested_budget.saturating_sub(used);
                            }
                            Ok(())
                        })();
                        if let Some(previous_actor) = previous_actor {
                            host.set_current_actor(previous_actor);
                        }
                        dispatch_result?;
                    }
                }
            }
            if let Some(sprite) = pending_spawn_return {
                self.write_lvalue(
                    &EdLValue::Variable("RETURN".to_owned()),
                    player,
                    actor,
                    sprite,
                    &mut access,
                )?;
            }
            if pc > program.instructions.len() {
                return Err(EdukeVmError::InvalidProgramCounter {
                    address: pc,
                    length: program.instructions.len(),
                });
            }
        }
        Ok(EdExecution {
            steps,
            returned: false,
            killed_actor: false,
            kill_actor_id: None,
            restart_player: false,
        })
    }

    fn read_operand(
        &mut self,
        operand: &EdOperand,
        player: usize,
        actor: usize,
        access: &mut Option<&mut dyn EdukeStructAccess>,
    ) -> Result<i32, EdukeVmError> {
        match operand {
            EdOperand::Immediate(value) => Ok(*value),
            EdOperand::CurrentActor => event_context_value("actor", "THISACTOR", actor, MAXSPRITES),
            EdOperand::CurrentPlayer => {
                event_context_value("player", "THISPLAYER", player, EDUKE_MAXPLAYERS)
            }
            EdOperand::PlayerDistance => {
                let actor = i32::try_from(actor).map_err(|_| EdukeVmError::IndexOutOfBounds {
                    kind: "actor",
                    name: "THISACTOR".to_owned(),
                    index: actor,
                    extent: MAXSPRITES,
                })?;
                access
                    .as_deref_mut()
                    .ok_or(EdukeVmError::StructAccessUnavailable)?
                    .player_distance(actor)
            }
            EdOperand::ActorPlayerAngleDifference => {
                let access = access
                    .as_deref_mut()
                    .ok_or(EdukeVmError::StructAccessUnavailable)?;
                let actor_id =
                    i32::try_from(actor).map_err(|_| EdukeVmError::IndexOutOfBounds {
                        kind: "actor",
                        name: "THISACTOR".to_owned(),
                        index: actor,
                        extent: MAXSPRITES,
                    })?;
                let player_id =
                    i32::try_from(player).map_err(|_| EdukeVmError::IndexOutOfBounds {
                        kind: "player",
                        name: "THISPLAYER".to_owned(),
                        index: player,
                        extent: EDUKE_MAXPLAYERS,
                    })?;
                let player_angle =
                    access.get_member(EdStructKind::Player, player_id, "ang", None)?;
                let actor_angle = access.get_member(EdStructKind::Actor, actor_id, "ang", None)?;
                Ok(i32::from(eduke_getincangle(player_angle as i16, actor_angle as i16)).abs())
            }
            EdOperand::ActorSectorGap => {
                let actor_id =
                    i32::try_from(actor).map_err(|_| EdukeVmError::IndexOutOfBounds {
                        kind: "actor",
                        name: "THISACTOR".to_owned(),
                        index: actor,
                        extent: MAXSPRITES,
                    })?;
                let access = access
                    .as_deref_mut()
                    .ok_or(EdukeVmError::StructAccessUnavailable)?;
                let x = access.get_member(EdStructKind::Actor, actor_id, "x", None)?;
                let y = access.get_member(EdStructKind::Actor, actor_id, "y", None)?;
                let sector = access.get_member(EdStructKind::Actor, actor_id, "sectnum", None)?;
                let (floor_z, ceiling_z) = access.get_slope_z(sector, x, y)?;
                Ok(floor_z.wrapping_sub(ceiling_z) >> 8)
            }
            EdOperand::ActorFloorDistance | EdOperand::ActorCeilingDistance => {
                let access = access
                    .as_deref_mut()
                    .ok_or(EdukeVmError::StructAccessUnavailable)?;
                actor_vertical_distance(
                    access,
                    actor,
                    matches!(operand, EdOperand::ActorFloorDistance),
                )
            }
            EdOperand::PlayerInventoryDiffers { item, amount } => {
                let item = self.read_operand(item, player, actor, access)?;
                let amount = self.read_operand(amount, player, actor, access)?;
                access
                    .as_deref_mut()
                    .ok_or(EdukeVmError::StructAccessUnavailable)?
                    .player_inventory_condition(item, amount, player, actor)
                    .map(i32::from)
            }
            EdOperand::PlayerFlags(flags) => Ok(i32::from(
                access
                    .as_deref_mut()
                    .ok_or(EdukeVmError::StructAccessUnavailable)?
                    .if_player(*flags, player, actor)?,
            )),
            EdOperand::PlayerInputBits(mask) => Ok(i32::from(
                access
                    .as_deref_mut()
                    .ok_or(EdukeVmError::StructAccessUnavailable)?
                    .player_input_bits(player)? as i32
                    & mask
                    != 0,
            )),
            EdOperand::ActorDead => {
                let actor_id =
                    i32::try_from(actor).map_err(|_| EdukeVmError::IndexOutOfBounds {
                        kind: "actor",
                        name: "THISACTOR".to_owned(),
                        index: actor,
                        extent: MAXSPRITES,
                    })?;
                let access = access
                    .as_deref_mut()
                    .ok_or(EdukeVmError::StructAccessUnavailable)?;
                let extra = access.get_member(EdStructKind::Actor, actor_id, "extra", None)?;
                let picnum = access.get_member(EdStructKind::Actor, actor_id, "picnum", None)?;
                Ok(i32::from(
                    extra - i32::from(picnum == i32::from(crate::spawn::APLAYER)) < 0,
                ))
            }
            EdOperand::ActorSquished => Ok(i32::from(
                access
                    .as_deref_mut()
                    .ok_or(EdukeVmError::StructAccessUnavailable)?
                    .actor_squished(actor)?,
            )),
            EdOperand::ActorHitByWeapon => Ok(i32::from(
                access
                    .as_deref_mut()
                    .ok_or(EdukeVmError::StructAccessUnavailable)?
                    .actor_hit_by_weapon(actor)?,
            )),
            EdOperand::ActorCanSeePlayer => {
                let actor_id =
                    i32::try_from(actor).map_err(|_| EdukeVmError::IndexOutOfBounds {
                        kind: "actor",
                        name: "THISACTOR".to_owned(),
                        index: actor,
                        extent: MAXSPRITES,
                    })?;
                let player_id =
                    i32::try_from(player).map_err(|_| EdukeVmError::IndexOutOfBounds {
                        kind: "player",
                        name: "THISPLAYER".to_owned(),
                        index: player,
                        extent: EDUKE_MAXPLAYERS,
                    })?;
                let access = access
                    .as_deref_mut()
                    .ok_or(EdukeVmError::StructAccessUnavailable)?;
                let actor_position = [
                    access.get_member(EdStructKind::Actor, actor_id, "x", None)?,
                    access.get_member(EdStructKind::Actor, actor_id, "y", None)?,
                    access.get_member(EdStructKind::Actor, actor_id, "z", None)?,
                    access.get_member(EdStructKind::Actor, actor_id, "sectnum", None)?,
                ];
                let player_position = [
                    access.get_member(EdStructKind::Player, player_id, "posx", None)?,
                    access.get_member(EdStructKind::Player, player_id, "posy", None)?,
                    access.get_member(EdStructKind::Player, player_id, "posz", None)?,
                    access.get_member(EdStructKind::Player, player_id, "cursectnum", None)?,
                ];
                let picnum = access
                    .get_member(EdStructKind::Actor, actor_id, "picnum", None)
                    .unwrap_or(0);
                let optional_actor_member = |access: &mut dyn EdukeStructAccess, member| {
                    access
                        .get_member(EdStructKind::Actor, actor_id, member, None)
                        .unwrap_or(0)
                };
                let actor_geometry = [
                    optional_actor_member(access, "cstat"),
                    optional_actor_member(access, "ang"),
                    optional_actor_member(access, "xrepeat"),
                    optional_actor_member(access, "yrepeat"),
                    optional_actor_member(access, "xoffset"),
                    optional_actor_member(access, "yoffset"),
                ];
                let optional_tile_member = |access: &mut dyn EdukeStructAccess, member| {
                    access
                        .get_member(EdStructKind::TileData, picnum, member, None)
                        .unwrap_or(0)
                };
                let tile_geometry = [
                    optional_tile_member(access, "xsize"),
                    optional_tile_member(access, "ysize"),
                    optional_tile_member(access, "xoffset"),
                    optional_tile_member(access, "yoffset"),
                ];
                let probes = actor_tile_visibility_probes(
                    actor_position,
                    player_position,
                    actor_geometry,
                    tile_geometry,
                );
                for (probe_index, probe) in probes.into_iter().enumerate() {
                    if probes[..probe_index].contains(&probe) {
                        continue;
                    }
                    if access.can_see(probe, player_position)? {
                        access.actor_seen_by_player(actor)?;
                        return Ok(1);
                    }
                }
                Ok(0)
            }
            EdOperand::ActorCanSeeTarget => {
                let actor_id =
                    i32::try_from(actor).map_err(|_| EdukeVmError::IndexOutOfBounds {
                        kind: "actor",
                        name: "THISACTOR".to_owned(),
                        index: actor,
                        extent: MAXSPRITES,
                    })?;
                let player_id =
                    i32::try_from(player).map_err(|_| EdukeVmError::IndexOutOfBounds {
                        kind: "player",
                        name: "THISPLAYER".to_owned(),
                        index: player,
                        extent: EDUKE_MAXPLAYERS,
                    })?;
                let (actor_x, actor_y, actor_z, actor_sect, player_position) = {
                    let host = access
                        .as_deref_mut()
                        .ok_or(EdukeVmError::StructAccessUnavailable)?;
                    (
                        host.get_member(EdStructKind::Actor, actor_id, "x", None)?,
                        host.get_member(EdStructKind::Actor, actor_id, "y", None)?,
                        host.get_member(EdStructKind::Actor, actor_id, "z", None)?,
                        host.get_member(EdStructKind::Actor, actor_id, "sectnum", None)?,
                        [
                            host.get_member(EdStructKind::Player, player_id, "posx", None)?,
                            host.get_member(EdStructKind::Player, player_id, "posy", None)?,
                            host.get_member(EdStructKind::Player, player_id, "posz", None)?,
                            host.get_member(EdStructKind::Player, player_id, "cursectnum", None)?,
                        ],
                    )
                };
                // Chocolate's target-visibility check varies the actor ray
                // height with one gameplay TRAND before calling Build cansee.
                let z_jitter = (self.next_game_random(access) & 41).wrapping_shl(8);
                let start = [actor_x, actor_y, actor_z.wrapping_sub(z_jitter), actor_sect];
                let host = access
                    .as_deref_mut()
                    .ok_or(EdukeVmError::StructAccessUnavailable)?;
                let visible = host.can_see(start, player_position)?;
                if visible {
                    host.actor_seen_by_player(actor)?;
                }
                Ok(i32::from(visible))
            }
            EdOperand::ActorCanShootTarget => access
                .as_deref_mut()
                .ok_or(EdukeVmError::StructAccessUnavailable)?
                .actor_can_shoot_target(player, actor)
                .map(i32::from),
            EdOperand::ActorBulletNear => access
                .as_deref_mut()
                .ok_or(EdukeVmError::StructAccessUnavailable)?
                .actor_bullet_near(actor)
                .map(i32::from),
            EdOperand::ActorRespawnEnabled => access
                .as_deref_mut()
                .ok_or(EdukeVmError::StructAccessUnavailable)?
                .actor_respawn_enabled(actor)
                .map(i32::from),
            EdOperand::ActorNotStayPut => access
                .as_deref_mut()
                .ok_or(EdukeVmError::StructAccessUnavailable)?
                .actor_not_stayput(actor)
                .map(i32::from),
            EdOperand::ActorOnWater => {
                let actor_id =
                    i32::try_from(actor).map_err(|_| EdukeVmError::IndexOutOfBounds {
                        kind: "actor",
                        name: "THISACTOR".to_owned(),
                        index: actor,
                        extent: MAXSPRITES,
                    })?;
                let access = access
                    .as_deref_mut()
                    .ok_or(EdukeVmError::StructAccessUnavailable)?;
                let sector = access.get_member(EdStructKind::Actor, actor_id, "sectnum", None)?;
                if access.get_member(EdStructKind::Sector, sector, "lotag", None)? != 1 {
                    return Ok(0);
                }
                let actor_z = access.get_member(EdStructKind::Actor, actor_id, "z", None)?;
                let floor_z = access.get_member(EdStructKind::Sector, sector, "floorz", None)?;
                Ok(i32::from(actor_z.abs_diff(floor_z) < (32 << 8) as u32))
            }
            EdOperand::ActorAwayFromWall => access
                .as_deref_mut()
                .ok_or(EdukeVmError::StructAccessUnavailable)?
                .actor_away_from_wall(actor)
                .map(i32::from),
            EdOperand::ActorInOuterSpace => {
                let actor_id =
                    i32::try_from(actor).map_err(|_| EdukeVmError::IndexOutOfBounds {
                        kind: "actor",
                        name: "THISACTOR".to_owned(),
                        index: actor,
                        extent: MAXSPRITES,
                    })?;
                let access = access
                    .as_deref_mut()
                    .ok_or(EdukeVmError::StructAccessUnavailable)?;
                let sector = access.get_member(EdStructKind::Actor, actor_id, "sectnum", None)?;
                let floor_stat =
                    access.get_member(EdStructKind::Sector, sector, "floorstat", None)?;
                let ceiling_pal =
                    access.get_member(EdStructKind::Sector, sector, "ceilingpal", None)?;
                let floor_pic =
                    access.get_member(EdStructKind::Sector, sector, "floorpicnum", None)?;
                Ok(i32::from(
                    floor_stat & 1 != 0 && ceiling_pal == 0 && matches!(floor_pic, 80 | 84),
                ))
            }
            EdOperand::ActorHasActiveSound => access
                .as_deref_mut()
                .ok_or(EdukeVmError::StructAccessUnavailable)?
                .actor_has_active_sound(actor)
                .map(i32::from),
            EdOperand::ActorHasSound {
                actor: source_actor,
                sound,
            } => {
                let source_actor = self.read_operand(source_actor, player, actor, access)?;
                let sound = self.read_operand(sound, player, actor, access)?;
                access
                    .as_deref_mut()
                    .ok_or(EdukeVmError::StructAccessUnavailable)?
                    .actor_has_sound(source_actor, sound)
                    .map(i32::from)
            }
            EdOperand::ActorWeaponReservation { register } => {
                let register = self.read_operand(register, player, actor, access)? != 0;
                access
                    .as_deref_mut()
                    .ok_or(EdukeVmError::StructAccessUnavailable)?
                    .actor_weapon_reservation(player, actor, register)
                    .map(i32::from)
            }
            EdOperand::Variable(name) => {
                if matches!(
                    name.as_str(),
                    "numsectors"
                        | "NUMSECTORS"
                        | "NUMWALLS"
                        | "Numsprites"
                        | "totalclock"
                        | "RESPAWN_MONSTERS"
                        | "RESPAWN_ITEMS"
                        | "RESPAWN_INVENTORY"
                ) {
                    return Ok(access
                        .as_deref_mut()
                        .and_then(|host| host.predefined_variable(name))
                        .unwrap_or(0));
                }
                if name == "randomseed"
                    && let Some(seed) = access
                        .as_deref_mut()
                        .and_then(|host| host.game_random_seed())
                {
                    return Ok(seed);
                }
                self.get_variable(name, player, actor)
            }
            EdOperand::ArrayElement { name, index } => {
                let index = self.read_operand(index, player, actor, access)?;
                let extent = self.array(name)?.values.len();
                let index = usize::try_from(index).map_err(|_| EdukeVmError::IndexOutOfBounds {
                    kind: "array",
                    name: name.clone(),
                    index: usize::MAX,
                    extent,
                })?;
                self.get_array(name, index)
            }
            EdOperand::StructMember {
                kind,
                id,
                member,
                member_index,
            } => {
                let id = self.read_operand(id, player, actor, access)?;
                let member_index = member_index
                    .as_ref()
                    .map(|index| self.read_operand(index, player, actor, access))
                    .transpose()?;
                access
                    .as_deref_mut()
                    .ok_or(EdukeVmError::StructAccessUnavailable)?
                    .get_member(*kind, id, member, member_index)
            }
        }
    }

    fn read_lvalue(
        &mut self,
        target: &EdLValue,
        player: usize,
        actor: usize,
        access: &mut Option<&mut dyn EdukeStructAccess>,
    ) -> Result<i32, EdukeVmError> {
        match target {
            EdLValue::Variable(name) => {
                self.read_operand(&EdOperand::Variable(name.clone()), player, actor, access)
            }
            EdLValue::ArrayElement { name, index } => {
                let index = self.read_operand(index, player, actor, access)?;
                let extent = self.array(name)?.values.len();
                let index = usize::try_from(index).map_err(|_| EdukeVmError::IndexOutOfBounds {
                    kind: "array",
                    name: name.clone(),
                    index: usize::MAX,
                    extent,
                })?;
                self.get_array(name, index)
            }
        }
    }

    fn write_lvalue(
        &mut self,
        target: &EdLValue,
        player: usize,
        actor: usize,
        value: i32,
        access: &mut Option<&mut dyn EdukeStructAccess>,
    ) -> Result<(), EdukeVmError> {
        match target {
            EdLValue::Variable(name) => {
                if name == "randomseed"
                    && access
                        .as_deref_mut()
                        .is_some_and(|host| host.set_game_random_seed(value))
                {
                    self.game_random_seed = value;
                    return Ok(());
                }
                if matches!(
                    name.as_str(),
                    "RESPAWN_MONSTERS" | "RESPAWN_ITEMS" | "RESPAWN_INVENTORY"
                ) && access
                    .as_deref_mut()
                    .is_some_and(|host| host.set_predefined_variable(name, value))
                {
                    return Ok(());
                }
                self.set_variable(name, player, actor, value)
            }
            EdLValue::ArrayElement { name, index } => {
                let index = self.read_operand(index, player, actor, access)?;
                let extent = self.array(name)?.values.len();
                let index = usize::try_from(index).map_err(|_| EdukeVmError::IndexOutOfBounds {
                    kind: "array",
                    name: name.clone(),
                    index: usize::MAX,
                    extent,
                })?;
                self.set_array(name, index, value)
            }
        }
    }

    fn push_variable(
        &mut self,
        image: &ScriptImage,
        definition: &EdVariableDefinition,
        allocated: &mut usize,
    ) -> Result<(), EdukeVmError> {
        let key = definition.name.clone();
        if self.variable_ids.contains_key(&key) {
            return Err(EdukeVmError::DuplicateName {
                kind: "variable",
                name: definition.name.clone(),
            });
        }
        let initial = resolve_token(image, &definition.name, &definition.initial_value)?;
        let flags = definition.flags.iter().try_fold(0i32, |bits, flag| {
            resolve_token(image, &definition.name, flag).map(|value| bits | value)
        })?;
        let cell_count = if flags & 2 != 0 {
            MAXSPRITES
        } else if flags & 1 != 0 {
            EDUKE_MAXPLAYERS
        } else {
            1
        };
        *allocated = allocated.saturating_add(cell_count);
        check_limit(&definition.name, *allocated)?;
        let storage = if flags & 2 != 0 {
            EdVariableStorage::Actor(try_filled(
                MAXSPRITES,
                initial,
                "actor variable",
                &definition.name,
            )?)
        } else if flags & 1 != 0 {
            EdVariableStorage::Player(try_filled(
                EDUKE_MAXPLAYERS,
                initial,
                "player variable",
                &definition.name,
            )?)
        } else {
            EdVariableStorage::Global(initial)
        };
        let id = self.variables.len();
        self.variables.push(EdVariableCell {
            name: definition.name.clone(),
            storage,
        });
        self.variable_ids.insert(key, id);
        Ok(())
    }

    fn push_array(
        &mut self,
        image: &ScriptImage,
        definition: &EdArrayDefinition,
        allocated: &mut usize,
    ) -> Result<(), EdukeVmError> {
        let key = definition.name.clone();
        if self.array_ids.contains_key(&key) {
            return Err(EdukeVmError::DuplicateName {
                kind: "array",
                name: definition.name.clone(),
            });
        }
        let length = resolve_token(image, &definition.name, &definition.length)?;
        let length = usize::try_from(length).map_err(|_| EdukeVmError::UnresolvedValue {
            declaration: definition.name.clone(),
            value: definition.length.clone(),
        })?;
        *allocated = allocated.saturating_add(length);
        check_limit(&definition.name, *allocated)?;
        let values = try_filled(length, 0, "array", &definition.name)?;
        let id = self.arrays.len();
        self.arrays.push(EdArrayCell {
            name: definition.name.clone(),
            values,
        });
        self.array_ids.insert(key, id);
        Ok(())
    }

    fn variable(&self, name: &str) -> Result<&EdVariableCell, EdukeVmError> {
        let index = self.variable_id(name)?;
        Ok(&self.variables[index])
    }

    fn variable_id(&self, name: &str) -> Result<usize, EdukeVmError> {
        self.variable_ids
            .get(name)
            .copied()
            .ok_or_else(|| EdukeVmError::UnknownName {
                kind: "variable",
                name: name.to_owned(),
            })
    }

    fn array(&self, name: &str) -> Result<&EdArrayCell, EdukeVmError> {
        let index = self.array_id(name)?;
        Ok(&self.arrays[index])
    }

    fn array_id(&self, name: &str) -> Result<usize, EdukeVmError> {
        self.array_ids
            .get(name)
            .copied()
            .ok_or_else(|| EdukeVmError::UnknownName {
                kind: "array",
                name: name.to_owned(),
            })
    }
}

fn integer_sqrt(value: u128) -> u128 {
    if value < 2 {
        return value;
    }
    let mut estimate = value / 2 + 1;
    let mut next = (estimate + value / estimate) / 2;
    while next < estimate {
        estimate = next;
        next = (estimate + value / estimate) / 2;
    }
    estimate
}

fn copy_quotes(destination: &mut Vec<String>, source: &[String]) -> Result<(), EdukeVmError> {
    destination
        .try_reserve_exact(source.len())
        .map_err(|_| EdukeVmError::AllocationFailed {
            kind: "quote table",
            name: "quotes".to_owned(),
        })?;
    for (index, value) in source.iter().enumerate() {
        let mut quote = String::new();
        quote
            .try_reserve_exact(value.len())
            .map_err(|_| EdukeVmError::AllocationFailed {
                kind: "quote",
                name: index.to_string(),
            })?;
        quote.push_str(value);
        destination.push(quote);
    }
    Ok(())
}

fn format_eduke_quote(format: &str, arguments: &[i32]) -> Result<String, EdukeVmError> {
    let mut output = String::new();
    output
        .try_reserve_exact(EDUKE_MAX_QUOTE_BYTES)
        .map_err(|_| EdukeVmError::AllocationFailed {
            kind: "formatted quote",
            name: "qsprintf".to_owned(),
        })?;
    let mut values = arguments.iter();
    let mut chars = format.chars();
    while let Some(character) = chars.next() {
        if character != '%' {
            if output.len().saturating_add(character.len_utf8()) > EDUKE_MAX_QUOTE_BYTES {
                return Err(EdukeVmError::QuoteTooLong {
                    limit: EDUKE_MAX_QUOTE_BYTES,
                });
            }
            output.push(character);
            continue;
        }
        match chars.next() {
            Some('%') => {
                if output.len() == EDUKE_MAX_QUOTE_BYTES {
                    return Err(EdukeVmError::QuoteTooLong {
                        limit: EDUKE_MAX_QUOTE_BYTES,
                    });
                }
                output.push('%');
            }
            Some('d') => {
                let value = values.next().ok_or_else(|| {
                    EdukeVmError::InvalidQuoteFormat(
                        "more %d placeholders than supplied values".to_owned(),
                    )
                })?;
                let argument_length = decimal_length(*value);
                if output.len().saturating_add(argument_length) > EDUKE_MAX_QUOTE_BYTES {
                    return Err(EdukeVmError::QuoteTooLong {
                        limit: EDUKE_MAX_QUOTE_BYTES,
                    });
                }
                write!(&mut output, "{value}").map_err(|_| {
                    EdukeVmError::InvalidQuoteFormat("format write failed".to_owned())
                })?;
            }
            Some(other) => {
                return Err(EdukeVmError::InvalidQuoteFormat(format!(
                    "unsupported `%{other}` placeholder"
                )));
            }
            None => {
                return Err(EdukeVmError::InvalidQuoteFormat("trailing `%`".to_owned()));
            }
        }
    }
    if values.next().is_some() {
        return Err(EdukeVmError::InvalidQuoteFormat(
            "more values than %d placeholders".to_owned(),
        ));
    }
    Ok(output)
}

fn decimal_length(value: i32) -> usize {
    let mut magnitude = value.unsigned_abs();
    let mut digits = 1usize;
    while magnitude >= 10 {
        magnitude /= 10;
        digits += 1;
    }
    digits + usize::from(value < 0)
}

/// Evaluate an EDuke32 integer operation without mutating VM state.
pub fn evaluate_binary(
    name: &str,
    operator: EdBinaryOperator,
    left: i32,
    right: i32,
) -> Result<i32, EdukeVmError> {
    let invalid = || EdukeVmError::InvalidArithmetic {
        name: name.to_owned(),
        operator: match operator {
            EdBinaryOperator::Divide => "div",
            EdBinaryOperator::Modulo => "mod",
            _ => "arithmetic",
        },
        right,
    };
    match operator {
        EdBinaryOperator::Add => Ok(left.wrapping_add(right)),
        EdBinaryOperator::Subtract => Ok(left.wrapping_sub(right)),
        EdBinaryOperator::Multiply => Ok(left.wrapping_mul(right)),
        EdBinaryOperator::Divide => left.checked_div(right).ok_or_else(invalid),
        EdBinaryOperator::Modulo => left.checked_rem(right).ok_or_else(invalid),
        EdBinaryOperator::And => Ok(left & right),
        EdBinaryOperator::Or => Ok(left | right),
        EdBinaryOperator::Xor => Ok(left ^ right),
        EdBinaryOperator::ShiftLeft => Ok(left.wrapping_shl((right as u32) & 31)),
        EdBinaryOperator::ShiftRight => Ok(left >> ((right as u32) & 31)),
    }
}

/// Build sine in signed Q14 at a 0–2047 Build angle.
#[must_use]
pub fn build_sin_q14(angle: i32) -> i32 {
    sincos_q14(angle).0
}

/// Build cosine in signed Q14 at a 0–2047 Build angle.
#[must_use]
pub fn build_cos_q14(angle: i32) -> i32 {
    sincos_q14(angle).1
}

/// Build angle in `0..2047` for the supplied vector.
#[must_use]
pub fn build_getangle(x: i32, y: i32) -> i32 {
    getangle(x, y)
}

/// Build `(a * b) >> shift` with wrapping 32-bit result.
#[must_use]
pub fn build_mulscale(a: i32, b: i32, shift: u32) -> i32 {
    mulscale(a, b, shift)
}

/// Build `(a << shift) / b`; zero divisor follows Build's current zero result.
#[must_use]
pub fn build_divscale(a: i32, b: i32, shift: u32) -> i32 {
    divscale(a, b, shift)
}

fn compare_values(comparison: EdComparison, left: i32, right: i32) -> bool {
    match comparison {
        EdComparison::Equal => left == right,
        EdComparison::NotEqual => left != right,
        EdComparison::Less => left < right,
        EdComparison::LessEqual => left <= right,
        EdComparison::Greater => left > right,
        EdComparison::GreaterEqual => left >= right,
        EdComparison::BitAndNonZero => left & right != 0,
        EdComparison::BitAndZero => left & right == 0,
        EdComparison::EitherNonZero => left != 0 || right != 0,
        EdComparison::BothZero => left == 0 && right == 0,
        EdComparison::BitXorNonZero => (left ^ right) != 0,
        EdComparison::BitXorZero => (left ^ right) == 0,
        EdComparison::BothNonZero => left != 0 && right != 0,
        EdComparison::EitherZero => left == 0 || right == 0,
        EdComparison::UnsignedGreater => (left as u32) > (right as u32),
        EdComparison::UnsignedGreaterEqual => (left as u32) >= (right as u32),
        EdComparison::UnsignedLess => (left as u32) < (right as u32),
        EdComparison::UnsignedLessEqual => (left as u32) <= (right as u32),
    }
}

fn actor_vertical_distance(
    access: &mut dyn EdukeStructAccess,
    actor: usize,
    to_floor: bool,
) -> Result<i32, EdukeVmError> {
    let actor_id = i32::try_from(actor).map_err(|_| EdukeVmError::IndexOutOfBounds {
        kind: "actor",
        name: "THISACTOR".to_owned(),
        index: actor,
        extent: MAXSPRITES,
    })?;
    let x = access.get_member(EdStructKind::Actor, actor_id, "x", None)?;
    let y = access.get_member(EdStructKind::Actor, actor_id, "y", None)?;
    let z = access.get_member(EdStructKind::Actor, actor_id, "z", None)?;
    let sector = access.get_member(EdStructKind::Actor, actor_id, "sectnum", None)?;
    let cstat = access.get_member(EdStructKind::Actor, actor_id, "cstat", None)? as u16;
    let yrepeat = access
        .get_member(EdStructKind::Actor, actor_id, "yrepeat", None)
        .unwrap_or(0)
        .max(0);
    let sprite_yoffset = access
        .get_member(EdStructKind::Actor, actor_id, "yoffset", None)
        .unwrap_or(0);
    let picnum = access
        .get_member(EdStructKind::Actor, actor_id, "picnum", None)
        .unwrap_or(0);
    let tile_height = access
        .get_member(EdStructKind::TileData, picnum, "ysize", None)
        .unwrap_or(0)
        .max(0);
    let tile_yoffset = access
        .get_member(EdStructKind::TileData, picnum, "yoffset", None)
        .unwrap_or(0);
    let mut yoffset = tile_yoffset.wrapping_add(sprite_yoffset);
    if cstat & 8 != 0 {
        yoffset = yoffset.wrapping_neg();
    }
    let height = tile_height.wrapping_mul(yrepeat).wrapping_shl(2);
    let center_z = z
        .wrapping_sub(yoffset.wrapping_mul(yrepeat).wrapping_mul(4))
        .wrapping_sub(if cstat & 128 == 0 { height / 2 } else { 0 });
    let (floor_z, ceiling_z) = access.get_slope_z(sector, x, y)?;
    if to_floor {
        Ok(floor_z.wrapping_sub(center_z) >> 8)
    } else {
        Ok(center_z.wrapping_sub(ceiling_z) >> 8)
    }
}

/// Build's signed shortest difference between two 0–2047 angles.
fn eduke_getincangle(a: i16, na: i16) -> i16 {
    let mut a = a & 2047;
    let mut na = na & 2047;
    if (i32::from(a) - i32::from(na)).abs() < 1024 {
        na.wrapping_sub(a)
    } else {
        if na > 1024 {
            na = na.wrapping_sub(2048);
        }
        if a > 1024 {
            a = a.wrapping_sub(2048);
        }
        na = na.wrapping_sub(2048);
        a = a.wrapping_sub(2048);
        na.wrapping_sub(a)
    }
}

/// Probe the actor origin and a deterministic grid over the Build tile extent.
/// `actor_geometry` is `[cstat, ang, xrepeat, yrepeat, xoffset, yoffset]` and
/// `tile_geometry` is `[xsize, ysize, tile_xoffset, tile_yoffset]`.
fn actor_tile_visibility_probes(
    actor: [i32; 4],
    player: [i32; 4],
    actor_geometry: [i32; 6],
    tile_geometry: [i32; 4],
) -> [[i32; 4]; 10] {
    let [
        cstat,
        angle,
        xrepeat,
        yrepeat,
        sprite_xoffset,
        sprite_yoffset,
    ] = actor_geometry;
    let [tile_width, tile_height, tile_xoffset, tile_yoffset] = tile_geometry;
    let cstat = cstat as u16;
    let xrepeat = xrepeat.max(0);
    let yrepeat = yrepeat.max(0);
    let width = tile_width.max(0).wrapping_mul(xrepeat);
    let height = tile_height.max(0).wrapping_mul(yrepeat);
    let mut xoffset = tile_xoffset.wrapping_add(sprite_xoffset);
    let mut yoffset = tile_yoffset.wrapping_add(sprite_yoffset);
    if cstat & 4 != 0 {
        xoffset = xoffset.wrapping_neg();
    }
    if cstat & 8 != 0 {
        yoffset = yoffset.wrapping_neg();
    }

    let orientation = cstat & 48;
    let (right_x, right_y, up_x, up_y) = match orientation {
        32 => {
            let (sin, cos) = sincos_q14(angle);
            (sin, -cos, cos, sin)
        }
        16 => {
            let (sin, cos) = sincos_q14(angle);
            (sin, cos, 0, 0)
        }
        _ => {
            let view_angle = getangle(
                player[0].wrapping_sub(actor[0]),
                player[1].wrapping_sub(actor[1]),
            );
            let (sin, cos) = sincos_q14(view_angle);
            (-sin, cos, 0, 0)
        }
    };
    let q14_offset = |axis: i32, distance: i32| axis.wrapping_mul(distance) >> 14;
    let center_x = actor[0]
        .wrapping_add(q14_offset(right_x, xoffset.wrapping_mul(xrepeat)))
        .wrapping_add(q14_offset(up_x, yoffset.wrapping_mul(yrepeat)));
    let center_y = actor[1]
        .wrapping_add(q14_offset(right_y, xoffset.wrapping_mul(xrepeat)))
        .wrapping_add(q14_offset(up_y, yoffset.wrapping_mul(yrepeat)));
    let half_width = width / 2;
    let half_height = height / 2;
    let mut probes = [[actor[0], actor[1], actor[2], actor[3]]; 10];

    if orientation == 32 {
        let x_samples = [-half_width, 0, half_width];
        let y_samples = [-half_height, 0, half_height];
        let mut cursor = 1;
        for dy in y_samples {
            for dx in x_samples {
                probes[cursor] = [
                    center_x
                        .wrapping_add(q14_offset(right_x, dx))
                        .wrapping_add(q14_offset(up_x, dy)),
                    center_y
                        .wrapping_add(q14_offset(right_y, dx))
                        .wrapping_add(q14_offset(up_y, dy)),
                    actor[2],
                    actor[3],
                ];
                cursor += 1;
            }
        }
        return probes;
    }

    let z_height = height.wrapping_shl(2);
    let z_shift = yoffset.wrapping_mul(yrepeat).wrapping_mul(4);
    let top = actor[2]
        .wrapping_sub(z_shift)
        .wrapping_add(if cstat & 128 != 0 { z_height / 2 } else { 0 });
    let middle = top.wrapping_sub(z_height / 2);
    let bottom = top.wrapping_sub(z_height);
    let horizontal = [-half_width, 0, half_width];
    let vertical = [bottom, middle, top];
    let sample_order = [
        (0usize, 1usize),
        (1, 0),
        (1, 2),
        (0, 0),
        (2, 0),
        (0, 2),
        (2, 2),
        (0, 1),
        (2, 1),
    ];
    for (probe_index, (x_index, z_index)) in sample_order.into_iter().enumerate() {
        probes[probe_index + 1] = [
            center_x.wrapping_add(q14_offset(right_x, horizontal[x_index])),
            center_y.wrapping_add(q14_offset(right_y, horizontal[x_index])),
            vertical[z_index],
            actor[3],
        ];
    }
    probes
}

fn normalize(name: &str) -> String {
    name.to_ascii_uppercase()
}

fn event_actor_context(actor: i32) -> Result<usize, EdukeVmError> {
    if actor == -1 {
        Ok(NO_EVENT_CONTEXT)
    } else {
        usize::try_from(actor).map_err(|_| EdukeVmError::InvalidStructId {
            kind: EdStructKind::Actor,
            id: actor,
        })
    }
}

fn event_context_value(
    kind: &'static str,
    name: &'static str,
    context: usize,
    extent: usize,
) -> Result<i32, EdukeVmError> {
    if context == NO_EVENT_CONTEXT {
        return Ok(-1);
    }
    i32::try_from(context).map_err(|_| EdukeVmError::IndexOutOfBounds {
        kind,
        name: name.to_owned(),
        index: context,
        extent,
    })
}

fn legacy_case_folded_ids_match(
    saved: &BTreeMap<String, usize>,
    expected: &BTreeMap<String, usize>,
) -> bool {
    saved.len() == expected.len()
        && expected
            .iter()
            .all(|(name, id)| saved.get(&name.to_ascii_uppercase()).copied() == Some(*id))
}

fn scoped_index(value: i32, scope: EdVariableScope, name: &str) -> Result<usize, EdukeVmError> {
    let (kind, extent) = match scope {
        EdVariableScope::Global => return Ok(0),
        EdVariableScope::Player => ("player", EDUKE_MAXPLAYERS),
        EdVariableScope::Actor => ("actor", MAXSPRITES),
    };
    let index = usize::try_from(value).map_err(|_| EdukeVmError::IndexOutOfBounds {
        kind,
        name: name.to_owned(),
        index: usize::MAX,
        extent,
    })?;
    if index >= extent {
        return Err(EdukeVmError::IndexOutOfBounds {
            kind,
            name: name.to_owned(),
            index,
            extent,
        });
    }
    Ok(index)
}

fn resolve_token(image: &ScriptImage, declaration: &str, token: &str) -> Result<i32, EdukeVmError> {
    token
        .parse::<i32>()
        .ok()
        .or_else(|| {
            image
                .labels
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(token))
                .map(|(_, label)| label.value)
        })
        .or_else(|| match token.to_ascii_uppercase().as_str() {
            "TRUE" => Some(1),
            "FALSE" => Some(0),
            "MAXSPRITES" => Some(MAXSPRITES as i32),
            "MAXPLAYERS" => Some(EDUKE_MAXPLAYERS as i32),
            "MAXSECTORS" => Some(MAXSECTORS as i32),
            "MAXWALLS" => Some(MAXWALLS as i32),
            _ => None,
        })
        .ok_or_else(|| EdukeVmError::UnresolvedValue {
            declaration: declaration.to_owned(),
            value: token.to_owned(),
        })
}

fn check_limit(name: &str, cells: usize) -> Result<(), EdukeVmError> {
    if cells > MAX_EDUKE_VALUE_CELLS {
        return Err(EdukeVmError::AllocationLimit {
            name: name.to_owned(),
            cells,
        });
    }
    Ok(())
}

fn try_filled(
    length: usize,
    value: i32,
    kind: &'static str,
    name: &str,
) -> Result<Vec<i32>, EdukeVmError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(length)
        .map_err(|_| EdukeVmError::AllocationFailed {
            kind,
            name: name.to_owned(),
        })?;
    values.resize(length, value);
    Ok(values)
}

fn scoped_read(
    kind: &'static str,
    name: &str,
    values: &[i32],
    index: usize,
) -> Result<i32, EdukeVmError> {
    values
        .get(index)
        .copied()
        .ok_or_else(|| EdukeVmError::IndexOutOfBounds {
            kind,
            name: name.to_owned(),
            index,
            extent: values.len(),
        })
}

fn scoped_write(
    kind: &'static str,
    name: &str,
    values: &mut [i32],
    index: usize,
    value: i32,
) -> Result<(), EdukeVmError> {
    let extent = values.len();
    let Some(slot) = values.get_mut(index) else {
        return Err(EdukeVmError::IndexOutOfBounds {
            kind,
            name: name.to_owned(),
            index,
            extent,
        });
    };
    *slot = value;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::con::{ConCompileOptions, NullIncludes, compile_with_options};

    #[derive(Default)]
    struct TestStructAccess {
        actor_xvel: i32,
        actor_position: [i32; 3],
        actor_cstat: i32,
        actor_angle: i32,
        player_angle: i32,
        actor_hitag: i32,
        actor_yvel: i32,
        actor_repeats: [i32; 2],
        actor_offsets: [i32; 2],
        actor_temp_data: [i32; 6],
        actor_picnum: i32,
        actor_extra: i32,
        actor_movflag: i32,
        actor_pal: i32,
        actor_hit_picnum: i32,
        actor_sectnum: i32,
        sector_lotag: i32,
        sector_floorstat: i32,
        sector_floorpicnum: i32,
        sector_ceilingpal: i32,
        sector_ceilingstat: i32,
        sector_floorz: i32,
        sector_ceilingz: i32,
        player_extra: i32,
        player_position: [i32; 3],
        player_cursectnum: i32,
        actor_tile_geometry: [i32; 4],
        nearest_player_distance: i32,
        sound_pitch_calls: Vec<(i32, i32, i32, i32)>,
        ifp_result: bool,
        player_input_mask: u32,
        actor_squished: bool,
        actor_hit_by_weapon: bool,
        last_ifp_flags: i32,
        player_health_delta: Option<(usize, usize, i32)>,
        player_weapon_ops: Vec<(usize, i32, bool, i32, i32)>,
        player_weapon_result: bool,
        player_inventory_ops: Vec<(usize, i32, i32, i32)>,
        weapon_toss_calls: Vec<(usize, i32)>,
        debris_calls: Vec<(i32, i32, i32)>,
        guts_calls: Vec<(usize, i32, i32, i32)>,
        glass_calls: Vec<(i32, i32)>,
        ceiling_glass_calls: Vec<(i32, i32, i32)>,
        item_spray_calls: Vec<(i32, EdItemSprayKind, i32)>,
        operate_calls: Vec<i32>,
        respawn_hitag_calls: Vec<i32>,
        player_restart_requests: Vec<usize>,
        master_switch_lotags: Vec<i32>,
        game_palette_calls: Vec<(usize, i32)>,
        timer_rate_calls: Vec<i32>,
        end_of_game_calls: Vec<(usize, i32)>,
        tip_calls: Vec<usize>,
        hitradius_calls: Vec<(i32, i32, [i32; 4])>,
        quick_kick_calls: Vec<i32>,
        stomp_calls: Vec<(usize, i32)>,
        wack_calls: Vec<(usize, i32, i32)>,
        quote_calls: Vec<(usize, usize)>,
        palette_calls: Vec<(usize, [i32; 4])>,
        vm_sprite: i32,
        userdef_return: [i32; 16],
        hit_scan_args: Option<([i32; 4], [i32; 3], u32)>,
        can_see_result: bool,
        z_range_query: Option<([i32; 4], i32, u32)>,
        z_range_result: [i32; 4],
        can_see_calls: usize,
        visible_on_call: usize,
        last_can_see: Option<([i32; 4], [i32; 4])>,
        seen_actor: Option<usize>,
        can_shoot_target: bool,
        can_shoot_target_query: Option<(usize, usize)>,
        bullet_near: bool,
        bullet_near_actor: Option<usize>,
        actor_has_active_sound: bool,
        weapon_reservation: bool,
        weapon_reservation_query: Option<(usize, usize, bool)>,
        played_sounds: Vec<(i32, i32, EdSoundMode)>,
        stopped_sounds: Vec<(i32, i32)>,
        stopped_actor_sounds: Vec<(i32, i32)>,
        actor_script_ops: Vec<(String, i32, i32, i32)>,
        player_inventory_condition_result: bool,
        last_player_inventory_query: Option<(i32, i32, usize, usize)>,
        active_sprites: Vec<i32>,
        game_random_seed: Option<i32>,
        respawn_enabled: bool,
        actor_not_stayput_result: bool,
        actor_away_from_wall_result: bool,
        actor_sound_query: Option<(i32, i32)>,
        actor_sound_result: bool,
        near_actor_query: Option<(i32, i32, i32)>,
        near_actor_result: i32,
        predefined_values: BTreeMap<String, i32>,
    }

    impl EdukeStructAccess for TestStructAccess {
        fn get_member(
            &mut self,
            kind: EdStructKind,
            id: i32,
            member: &str,
            member_index: Option<i32>,
        ) -> Result<i32, EdukeVmError> {
            match (kind, member) {
                (EdStructKind::Actor, "x") if id == 7 => Ok(self.actor_position[0]),
                (EdStructKind::Actor, "y") if id == 7 => Ok(self.actor_position[1]),
                (EdStructKind::Actor, "z") if id == 7 => Ok(self.actor_position[2]),
                (EdStructKind::Actor, "cstat") if id == 7 => Ok(self.actor_cstat),
                (EdStructKind::Actor, "ang") if id == 7 => Ok(self.actor_angle),
                (EdStructKind::Actor, "hitag") if id == 7 => Ok(self.actor_hitag),
                (EdStructKind::Actor, "yvel") if id == 7 => Ok(self.actor_yvel),
                (EdStructKind::Actor, "xrepeat") if id == 7 => Ok(self.actor_repeats[0]),
                (EdStructKind::Actor, "yrepeat") if id == 7 => Ok(self.actor_repeats[1]),
                (EdStructKind::Actor, "xoffset") if id == 7 => Ok(self.actor_offsets[0]),
                (EdStructKind::Actor, "yoffset") if id == 7 => Ok(self.actor_offsets[1]),
                (EdStructKind::Actor, "xvel") if id == 7 && member_index.is_none() => {
                    Ok(self.actor_xvel)
                }
                (EdStructKind::Actor, "htg_t" | "t_data" | "g_t") if id == 7 => member_index
                    .and_then(|index| usize::try_from(index).ok())
                    .and_then(|index| self.actor_temp_data.get(index).copied())
                    .ok_or(EdukeVmError::StructAccessUnavailable),
                (EdStructKind::Actor, "picnum") if id == 7 => Ok(self.actor_picnum),
                (EdStructKind::Actor, "extra") if id == 7 => Ok(self.actor_extra),
                (EdStructKind::Actor, "pal") if id == 7 => Ok(self.actor_pal),
                (EdStructKind::Actor, "htpicnum") if id == 7 => Ok(self.actor_hit_picnum),
                (EdStructKind::Actor, "htmovflag") if id == 7 => Ok(self.actor_movflag),
                (EdStructKind::Actor, "sectnum") if id == 7 => Ok(self.actor_sectnum),
                (EdStructKind::Sector, "lotag") if id == self.actor_sectnum => {
                    Ok(self.sector_lotag)
                }
                (EdStructKind::Sector, "floorz") if id == self.actor_sectnum => {
                    Ok(self.sector_floorz)
                }
                (EdStructKind::Sector, "floorstat") if id == self.actor_sectnum => {
                    Ok(self.sector_floorstat)
                }
                (EdStructKind::Sector, "floorpicnum") if id == self.actor_sectnum => {
                    Ok(self.sector_floorpicnum)
                }
                (EdStructKind::Sector, "ceilingpal") if id == self.actor_sectnum => {
                    Ok(self.sector_ceilingpal)
                }
                (EdStructKind::Sector, "ceilingstat") if id == self.actor_sectnum => {
                    Ok(self.sector_ceilingstat)
                }
                (EdStructKind::Player, "extra") if id == 0 => Ok(self.player_extra),
                (EdStructKind::Player, "ang") if id == 0 => Ok(self.player_angle),
                (EdStructKind::Player, "i") if id == 0 => Ok(19),
                (EdStructKind::Player, "posx") if id == 0 => Ok(self.player_position[0]),
                (EdStructKind::Player, "posy") if id == 0 => Ok(self.player_position[1]),
                (EdStructKind::Player, "posz") if id == 0 => Ok(self.player_position[2]),
                (EdStructKind::Player, "cursectnum") if id == 0 => Ok(self.player_cursectnum),
                (EdStructKind::TileData, "xsize") if id == self.actor_picnum => {
                    Ok(self.actor_tile_geometry[0])
                }
                (EdStructKind::TileData, "ysize") if id == self.actor_picnum => {
                    Ok(self.actor_tile_geometry[1])
                }
                (EdStructKind::TileData, "xoffset") if id == self.actor_picnum => {
                    Ok(self.actor_tile_geometry[2])
                }
                (EdStructKind::TileData, "yoffset") if id == self.actor_picnum => {
                    Ok(self.actor_tile_geometry[3])
                }
                (EdStructKind::Player, "ammo_amount") if member_index == Some(2) => Ok(31),
                (EdStructKind::UserDef, "vm_sprite") => Ok(self.vm_sprite),
                (EdStructKind::UserDef, "return") => self
                    .userdef_return
                    .get(member_index.unwrap_or(0) as usize)
                    .copied()
                    .ok_or(EdukeVmError::StructAccessUnavailable),
                _ => Err(EdukeVmError::StructAccessUnavailable),
            }
        }

        fn set_member(
            &mut self,
            kind: EdStructKind,
            id: i32,
            member: &str,
            member_index: Option<i32>,
            value: i32,
        ) -> Result<(), EdukeVmError> {
            match (kind, member) {
                (EdStructKind::Actor, "xvel") if id == 7 && member_index.is_none() => {
                    self.actor_xvel = value;
                    Ok(())
                }
                (EdStructKind::UserDef, "vm_sprite") => {
                    self.vm_sprite = value;
                    Ok(())
                }
                (EdStructKind::UserDef, "return") => {
                    let Some(slot) = self
                        .userdef_return
                        .get_mut(member_index.unwrap_or(0) as usize)
                    else {
                        return Err(EdukeVmError::StructAccessUnavailable);
                    };
                    *slot = value;
                    Ok(())
                }
                _ => Err(EdukeVmError::StructAccessUnavailable),
            }
        }

        fn play_sound(
            &mut self,
            actor: i32,
            sound: i32,
            mode: EdSoundMode,
        ) -> Result<(), EdukeVmError> {
            self.played_sounds.push((actor, sound, mode));
            Ok(())
        }

        fn set_actor_sound_pitch(
            &mut self,
            context_actor: i32,
            actor: i32,
            sound: i32,
            pitch_offset: i32,
        ) -> Result<(), EdukeVmError> {
            self.sound_pitch_calls
                .push((context_actor, actor, sound, pitch_offset));
            Ok(())
        }

        fn stop_sound(&mut self, actor: i32, sound: i32) -> Result<(), EdukeVmError> {
            self.stopped_sounds.push((actor, sound));
            Ok(())
        }

        fn stop_actor_sound(&mut self, actor: i32, sound: i32) -> Result<(), EdukeVmError> {
            self.stopped_actor_sounds.push((actor, sound));
            Ok(())
        }

        fn toss_player_weapon(&mut self, player: usize, actor: i32) -> Result<(), EdukeVmError> {
            self.weapon_toss_calls.push((player, actor));
            Ok(())
        }

        fn display_quote(&mut self, player: usize, quote: usize) -> Result<(), EdukeVmError> {
            self.quote_calls.push((player, quote));
            Ok(())
        }

        fn palette_from(&mut self, player: usize, values: [i32; 4]) -> Result<(), EdukeVmError> {
            self.palette_calls.push((player, values));
            Ok(())
        }

        fn set_actor_action(&mut self, actor: i32, action: i32) -> Result<(), EdukeVmError> {
            self.actor_script_ops
                .push(("action".to_owned(), actor, action, 0));
            Ok(())
        }

        fn set_actor_move(
            &mut self,
            actor: i32,
            movement: i32,
            flags: i32,
        ) -> Result<(), EdukeVmError> {
            self.actor_script_ops
                .push(("move".to_owned(), actor, movement, flags));
            Ok(())
        }

        fn set_actor_ai(&mut self, actor: i32, ai: i32) -> Result<(), EdukeVmError> {
            self.actor_script_ops.push(("ai".to_owned(), actor, ai, 0));
            Ok(())
        }

        fn player_distance(&mut self, actor: i32) -> Result<i32, EdukeVmError> {
            if actor == 7 {
                Ok(self.nearest_player_distance)
            } else {
                Err(EdukeVmError::StructAccessUnavailable)
            }
        }

        fn if_player(
            &mut self,
            flags: i32,
            player: usize,
            actor: usize,
        ) -> Result<bool, EdukeVmError> {
            if player == 0 && actor == 7 {
                self.last_ifp_flags = flags;
                Ok(self.ifp_result)
            } else {
                Err(EdukeVmError::StructAccessUnavailable)
            }
        }

        fn player_input_bits(&mut self, player: usize) -> Result<u32, EdukeVmError> {
            if player == 0 {
                Ok(self.player_input_mask)
            } else {
                Err(EdukeVmError::StructAccessUnavailable)
            }
        }

        fn actor_squished(&mut self, actor: usize) -> Result<bool, EdukeVmError> {
            if actor == 7 {
                Ok(self.actor_squished)
            } else {
                Err(EdukeVmError::StructAccessUnavailable)
            }
        }

        fn actor_hit_by_weapon(&mut self, actor: usize) -> Result<bool, EdukeVmError> {
            if actor == 7 {
                Ok(self.actor_hit_by_weapon)
            } else {
                Err(EdukeVmError::StructAccessUnavailable)
            }
        }

        fn current_actor(&mut self) -> Option<i32> {
            Some(self.vm_sprite)
        }

        fn game_random_seed(&mut self) -> Option<i32> {
            self.game_random_seed
        }

        fn set_game_random_seed(&mut self, seed: i32) -> bool {
            self.game_random_seed = Some(seed);
            true
        }

        fn next_game_random(&mut self) -> Option<i32> {
            self.game_random_seed.as_mut().map(krand_step)
        }

        fn hit_scan(
            &mut self,
            origin: [i32; 4],
            vector: [i32; 3],
            clipmask: u32,
        ) -> Result<[i32; 6], EdukeVmError> {
            self.hit_scan_args = Some((origin, vector, clipmask));
            Ok([2, -1, 9, 100, 200, 300])
        }

        fn can_see(&mut self, start: [i32; 4], end: [i32; 4]) -> Result<bool, EdukeVmError> {
            self.can_see_calls += 1;
            self.last_can_see = Some((start, end));
            Ok(self.can_see_result || self.can_see_calls == self.visible_on_call)
        }

        fn get_z_range(
            &mut self,
            point: [i32; 4],
            wall_distance: i32,
            clipmask: u32,
        ) -> Result<[i32; 4], EdukeVmError> {
            self.z_range_query = Some((point, wall_distance, clipmask));
            Ok(self.z_range_result)
        }

        fn actor_seen_by_player(&mut self, actor: usize) -> Result<(), EdukeVmError> {
            self.seen_actor = Some(actor);
            Ok(())
        }

        fn actor_can_shoot_target(
            &mut self,
            player: usize,
            actor: usize,
        ) -> Result<bool, EdukeVmError> {
            self.can_shoot_target_query = Some((player, actor));
            Ok(self.can_shoot_target)
        }

        fn actor_bullet_near(&mut self, actor: usize) -> Result<bool, EdukeVmError> {
            self.bullet_near_actor = Some(actor);
            Ok(self.bullet_near)
        }

        fn actor_weapon_reservation(
            &mut self,
            player: usize,
            actor: usize,
            register: bool,
        ) -> Result<bool, EdukeVmError> {
            self.weapon_reservation_query = Some((player, actor, register));
            Ok(self.weapon_reservation)
        }

        fn get_slope_z(
            &mut self,
            sector: i32,
            _x: i32,
            _y: i32,
        ) -> Result<(i32, i32), EdukeVmError> {
            if sector == self.actor_sectnum {
                Ok((self.sector_floorz, self.sector_ceilingz))
            } else {
                Err(EdukeVmError::StructAccessUnavailable)
            }
        }

        fn sector_of_wall(&mut self, wall: i32) -> Result<i32, EdukeVmError> {
            Ok(if (10..14).contains(&wall) { 3 } else { -1 })
        }

        fn add_player_health(
            &mut self,
            player: usize,
            actor: usize,
            amount: i32,
        ) -> Result<(), EdukeVmError> {
            self.player_health_delta = Some((player, actor, amount));
            Ok(())
        }

        fn player_weapon_op(
            &mut self,
            player: usize,
            actor: i32,
            add_weapon: bool,
            weapon: i32,
            ammo: i32,
        ) -> Result<bool, EdukeVmError> {
            self.player_weapon_ops
                .push((player, actor, add_weapon, weapon, ammo));
            Ok(self.player_weapon_result)
        }

        fn add_player_inventory(
            &mut self,
            player: usize,
            actor: i32,
            item: i32,
            amount: i32,
        ) -> Result<(), EdukeVmError> {
            self.player_inventory_ops
                .push((player, actor, item, amount));
            Ok(())
        }

        fn spawn_debris(&mut self, actor: i32, tile: i32, count: i32) -> Result<(), EdukeVmError> {
            self.debris_calls.push((actor, tile, count));
            Ok(())
        }

        fn spawn_guts(
            &mut self,
            player: usize,
            actor: i32,
            tile: i32,
            count: i32,
        ) -> Result<(), EdukeVmError> {
            self.guts_calls.push((player, actor, tile, count));
            Ok(())
        }

        fn spawn_lots_of_glass(&mut self, actor: i32, count: i32) -> Result<(), EdukeVmError> {
            self.glass_calls.push((actor, count));
            Ok(())
        }

        fn spawn_ceiling_glass(
            &mut self,
            sprite: i32,
            sector: i32,
            count: i32,
        ) -> Result<(), EdukeVmError> {
            self.ceiling_glass_calls.push((sprite, sector, count));
            Ok(())
        }

        fn spawn_item_spray(
            &mut self,
            actor: i32,
            kind: EdItemSprayKind,
            count: i32,
        ) -> Result<(), EdukeVmError> {
            self.item_spray_calls.push((actor, kind, count));
            Ok(())
        }

        fn request_actor_operate(&mut self, actor: i32) -> Result<(), EdukeVmError> {
            self.operate_calls.push(actor);
            Ok(())
        }

        fn operate_master_switches(&mut self, lotag: i32) -> Result<(), EdukeVmError> {
            self.master_switch_lotags.push(lotag);
            Ok(())
        }

        fn respawn_hitag(&mut self, actor: i32) -> Result<(), EdukeVmError> {
            self.respawn_hitag_calls.push(actor);
            Ok(())
        }

        fn request_player_restart(&mut self, player: usize) -> Result<bool, EdukeVmError> {
            self.player_restart_requests.push(player);
            Ok(true)
        }

        fn set_game_palette(&mut self, player: usize, palette: i32) -> Result<(), EdukeVmError> {
            self.game_palette_calls.push((player, palette));
            Ok(())
        }

        fn set_timer_rate(&mut self, rate: i32) -> Result<(), EdukeVmError> {
            self.timer_rate_calls.push(rate);
            Ok(())
        }

        fn begin_end_of_game(&mut self, player: usize, delay: i32) -> Result<(), EdukeVmError> {
            self.end_of_game_calls.push((player, delay));
            Ok(())
        }

        fn show_tip(&mut self, player: usize) -> Result<(), EdukeVmError> {
            self.tip_calls.push(player);
            Ok(())
        }

        fn apply_hitradius(
            &mut self,
            actor: i32,
            radius: i32,
            damage: [i32; 4],
        ) -> Result<(), EdukeVmError> {
            self.hitradius_calls.push((actor, radius, damage));
            Ok(())
        }

        fn request_player_quick_kick(&mut self, actor: i32) -> Result<(), EdukeVmError> {
            self.quick_kick_calls.push(actor);
            Ok(())
        }

        fn request_player_stomp(&mut self, player: usize, actor: i32) -> Result<(), EdukeVmError> {
            self.stomp_calls.push((player, actor));
            Ok(())
        }

        fn request_player_wack(
            &mut self,
            player: usize,
            actor: i32,
            angle: i32,
        ) -> Result<(), EdukeVmError> {
            self.wack_calls.push((player, actor, angle));
            Ok(())
        }

        fn player_inventory_condition(
            &mut self,
            item: i32,
            amount: i32,
            player: usize,
            actor: usize,
        ) -> Result<bool, EdukeVmError> {
            self.last_player_inventory_query = Some((item, amount, player, actor));
            Ok(self.player_inventory_condition_result)
        }

        fn actor_respawn_enabled(&mut self, actor: usize) -> Result<bool, EdukeVmError> {
            if actor == 7 {
                Ok(self.respawn_enabled)
            } else {
                Err(EdukeVmError::StructAccessUnavailable)
            }
        }

        fn actor_not_stayput(&mut self, actor: usize) -> Result<bool, EdukeVmError> {
            if actor == 7 {
                Ok(self.actor_not_stayput_result)
            } else {
                Err(EdukeVmError::StructAccessUnavailable)
            }
        }

        fn actor_away_from_wall(&mut self, actor: usize) -> Result<bool, EdukeVmError> {
            if actor == 7 {
                Ok(self.actor_away_from_wall_result)
            } else {
                Err(EdukeVmError::StructAccessUnavailable)
            }
        }

        fn actor_has_active_sound(&mut self, actor: usize) -> Result<bool, EdukeVmError> {
            if actor == 7 {
                Ok(self.actor_has_active_sound)
            } else {
                Err(EdukeVmError::StructAccessUnavailable)
            }
        }

        fn actor_has_sound(&mut self, actor: i32, sound: i32) -> Result<bool, EdukeVmError> {
            self.actor_sound_query = Some((actor, sound));
            Ok(self.actor_sound_result)
        }

        fn find_near_actor_3d(
            &mut self,
            source_actor: i32,
            tile: i32,
            distance: i32,
        ) -> Result<i32, EdukeVmError> {
            self.near_actor_query = Some((source_actor, tile, distance));
            Ok(self.near_actor_result)
        }

        fn predefined_variable(&mut self, name: &str) -> Option<i32> {
            self.predefined_values.get(name).copied()
        }

        fn set_predefined_variable(&mut self, name: &str, value: i32) -> bool {
            if !matches!(
                name,
                "RESPAWN_MONSTERS" | "RESPAWN_ITEMS" | "RESPAWN_INVENTORY"
            ) {
                return false;
            }
            self.predefined_values.insert(name.to_owned(), value);
            true
        }

        fn active_sprite_ids(&mut self) -> Result<Vec<i32>, EdukeVmError> {
            Ok(self.active_sprites.clone())
        }
    }

    fn image() -> ScriptImage {
        compile_with_options(
            b"define MAXSPRITES 4\nvar shared 7\nvar by_player 3 1\nvar by_actor -2 2\ngamearray values 3\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        )
    }

    #[test]
    fn global_player_actor_and_array_values_have_fixed_scopes() {
        let mut state = EdukeVmState::from_image(&image()).expect("valid declarations");
        assert_eq!(state.get_variable("shared", 99, 99), Ok(7));
        assert_eq!(state.get_variable("by_player", 2, 0), Ok(3));
        assert_eq!(state.get_variable("by_actor", 0, 3), Ok(-2));
        assert_eq!(state.variable_scope("shared"), Ok(EdVariableScope::Global));
        assert_eq!(
            state.variable_scope("by_player"),
            Ok(EdVariableScope::Player)
        );
        assert_eq!(state.variable_scope("by_actor"), Ok(EdVariableScope::Actor));
        state
            .set_variable("by_player", 2, 0, 9)
            .expect("player slot");
        state
            .set_variable("by_actor", 0, 3, 11)
            .expect("actor slot");
        state.set_array("values", 2, 17).expect("array element");
        assert_eq!(state.get_variable("by_player", 2, 0), Ok(9));
        assert_eq!(state.get_variable("by_actor", 0, 3), Ok(11));
        assert_eq!(state.get_array("values", 2), Ok(17));
        assert!(matches!(
            state.get_variable("SHARED", 0, 0),
            Err(EdukeVmError::UnknownName {
                kind: "variable",
                ..
            })
        ));
        assert!(matches!(
            state.get_array("VALUES", 2),
            Err(EdukeVmError::UnknownName { kind: "array", .. })
        ));
        assert_eq!(state.variable_count(), 4); // includes implicit RETURN.
        assert_eq!(state.array_count(), 1);
    }

    #[test]
    fn scoped_variable_and_array_instructions_round_trip_selected_slots() {
        let image = compile_with_options(
            b"gamevar actor_value 0 2\ngamevar player_value 0 1\nvar actor_copy 0\nvar player_copy 0\nvar array_copy 0\ngamearray values 4 0\ndefstate scoped_values\nsetactorvar[3].actor_value 41\ngetactorvar[3].actor_value actor_copy\nsetplayervar[2].player_value 37\ngetplayervar[2].player_value player_copy\nsetarray values[1] 53\ngetarray array_copy values 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["scoped_values"];
        state
            .execute_at(&image.eduke_program, entry, 0, 0, 64, 8)
            .expect("scoped instructions execute");

        assert_eq!(state.get_variable("actor_value", 0, 3), Ok(41));
        assert_eq!(state.get_variable("actor_value", 0, 2), Ok(0));
        assert_eq!(state.get_variable("player_value", 2, 0), Ok(37));
        assert_eq!(state.get_variable("player_value", 1, 0), Ok(0));
        assert_eq!(state.get_variable("actor_copy", 0, 0), Ok(41));
        assert_eq!(state.get_variable("player_copy", 0, 0), Ok(37));
        assert_eq!(state.get_array("values", 1), Ok(53));
        assert_eq!(state.get_variable("array_copy", 0, 0), Ok(53));
    }

    #[test]
    fn eduke_labels_gamevars_and_arrays_preserve_case() {
        let image = compile_with_options(
            b"define UpperValue 7\ndefine uppervalue 9\nvar Score 0\nvar score 0\nvar StateResult 0\ngamearray Cache 1\ngamearray cache 1\ndefstate Foo\nadd StateResult 1\nends\ndefstate foo\nadd StateResult 10\nends\nappendevent EVENT_GAME\nset Score UpperValue\nset score uppervalue\nsetarray Cache 0 Score\nsetarray cache 0 score\nstate Foo\nstate foo\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(
            image.labels.get("UpperValue").map(|label| label.value),
            Some(7)
        );
        assert_eq!(
            image.labels.get("uppervalue").map(|label| label.value),
            Some(9)
        );
        assert!(image.eduke_program.states.contains_key("Foo"));
        assert!(image.eduke_program.states.contains_key("foo"));
        assert!(!image.eduke_program.states.contains_key("FOO"));

        let mut state = EdukeVmState::from_image(&image).expect("case-distinct symbols");
        state
            .execute_registered_event_hooks(&image, "EVENT_GAME", 0, 0, 64, 8)
            .expect("hook resolves exact symbol spelling");
        assert_eq!(state.get_variable("Score", 0, 0), Ok(7));
        assert_eq!(state.get_variable("score", 0, 0), Ok(9));
        assert_eq!(state.get_array("Cache", 0), Ok(7));
        assert_eq!(state.get_array("cache", 0), Ok(9));
        assert_eq!(state.get_variable("StateResult", 0, 0), Ok(11));
        assert!(matches!(
            state.execute_state(&image.eduke_program, "FOO", 0, 0, 8, 2),
            Err(EdukeVmError::UnknownState { .. })
        ));
        let wrong_case_call = compile_with_options(
            b"defstate Foo\nnullop\nends\nappendevent EVENT_GAME\nstate FOO\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(wrong_case_call.errors, 0);
        let mut wrong_case_state =
            EdukeVmState::from_image(&wrong_case_call).expect("valid declarations");
        assert!(matches!(
            wrong_case_state.execute_registered_event_hooks(
                &wrong_case_call,
                "EVENT_GAME",
                0,
                0,
                8,
                2
            ),
            Err(EdukeVmError::UnknownState { .. })
        ));
        assert!(matches!(
            state.get_variable("SCORE", 0, 0),
            Err(EdukeVmError::UnknownName {
                kind: "variable",
                ..
            })
        ));
        assert!(matches!(
            state.get_array("CACHE", 0),
            Err(EdukeVmError::UnknownName { kind: "array", .. })
        ));
    }

    #[test]
    fn integer_arithmetic_updates_values_with_fixed_width_rules() {
        let mut state = EdukeVmState::from_image(&image()).expect("valid declarations");
        assert_eq!(
            state.apply_binary("shared", 0, 0, EdBinaryOperator::Add, i32::MAX),
            Ok(i32::MIN.wrapping_add(6))
        );
        assert_eq!(
            evaluate_binary("x", EdBinaryOperator::Xor, 0b1010, 0b1100),
            Ok(0b0110)
        );
        assert_eq!(
            evaluate_binary("x", EdBinaryOperator::ShiftRight, -8, 1),
            Ok(-4)
        );
        assert!(matches!(
            evaluate_binary("x", EdBinaryOperator::Divide, 7, 0),
            Err(EdukeVmError::InvalidArithmetic { .. })
        ));
        assert_eq!(build_mulscale(256, 256, 8), 256);
        assert_eq!(build_sin_q14(0), 0);
        assert_eq!(build_cos_q14(0), 16_384);
        assert_eq!(build_getangle(0, 1), 512);
    }

    #[test]
    fn sqrt_compiles_input_and_destination_and_uses_integer_floor() {
        let image = compile_with_options(
            b"gamevar input 0 0\ngamevar output 0 0\ndefstate square_root\nsqrt input output\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        let entry = image.eduke_program.states["square_root"];
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        for (input, expected) in [(81, 9), (2, 1), (0, 0), (-4, 0)] {
            state
                .set_variable("input", 0, 0, input)
                .expect("input gamevar");
            state
                .execute_at(&image.eduke_program, entry, 0, 0, 8, 2)
                .expect("sqrt executes");
            assert_eq!(state.get_variable("output", 0, 0), Ok(expected));
        }
    }

    #[test]
    fn storage_access_checks_indices_and_array_resize_budget() {
        let mut state = EdukeVmState::from_image(&image()).expect("valid declarations");
        assert!(matches!(
            state.get_variable("by_player", EDUKE_MAXPLAYERS, 0),
            Err(EdukeVmError::IndexOutOfBounds { kind: "player", .. })
        ));
        assert!(matches!(
            state.get_array("values", 3),
            Err(EdukeVmError::IndexOutOfBounds { kind: "array", .. })
        ));
        state.resize_array("values", 5).expect("small resize");
        assert_eq!(state.get_array("values", 4), Ok(0));
    }

    #[test]
    fn runtime_requires_eduke_dialect_and_resolves_constants() {
        let mut atomic = ScriptImage::new();
        assert_eq!(
            EdukeVmState::from_image(&atomic),
            Err(EdukeVmError::WrongDialect)
        );
        atomic.dialect = ConDialect::EDuke32;
        atomic.eduke_arrays.push(EdArrayDefinition {
            name: "actors".into(),
            length: "MAXSPRITES".into(),
            flags: Vec::new(),
        });
        let state = EdukeVmState::from_image(&atomic).expect("constant should resolve");
        assert_eq!(state.get_array("actors", MAXSPRITES - 1), Ok(0));
    }

    #[test]
    fn oversized_untrusted_array_is_rejected_before_allocation() {
        let mut image = ScriptImage::new();
        image.dialect = ConDialect::EDuke32;
        image.eduke_arrays.push(EdArrayDefinition {
            name: "too_large".into(),
            length: (MAX_EDUKE_VALUE_CELLS + 1).to_string(),
            flags: Vec::new(),
        });
        assert!(matches!(
            EdukeVmState::from_image(&image),
            Err(EdukeVmError::AllocationLimit { .. })
        ));
    }

    #[test]
    fn variable_and_array_storage_round_trips_through_serde() {
        let mut state = EdukeVmState::from_image(&image()).expect("valid declarations");
        state
            .set_variable("by_actor", 0, 2, 91)
            .expect("actor variable");
        state.set_array("values", 1, -7).expect("array element");
        let encoded = serde_json::to_vec(&state).expect("encode ED state");
        let decoded: EdukeVmState = serde_json::from_slice(&encoded).expect("decode ED state");
        assert_eq!(decoded, state);
        assert_eq!(decoded.get_variable("by_actor", 0, 2), Ok(91));
        assert_eq!(decoded.get_array("values", 1), Ok(-7));
    }

    #[test]
    fn console_echo_output_is_ephemeral_across_state_serialization() {
        let mut state = EdukeVmState::from_image(&image()).expect("valid declarations");
        state
            .console_output
            .push_back("transient debug line".to_owned());
        let encoded = serde_json::to_vec(&state).expect("encode ED state");
        let mut decoded: EdukeVmState = serde_json::from_slice(&encoded).expect("decode ED state");
        assert!(decoded.take_console_output().is_empty());
        assert_eq!(decoded.get_variable("shared", 0, 0), Ok(7));
    }

    #[test]
    fn validation_migrates_legacy_case_folded_index_maps() {
        let image = image();
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        state
            .set_variable("by_actor", 0, 2, 91)
            .expect("actor variable");
        state.set_array("values", 1, -7).expect("array element");
        state.variable_ids = state
            .variable_ids
            .iter()
            .map(|(name, id)| (name.to_ascii_uppercase(), *id))
            .collect();
        state.array_ids = state
            .array_ids
            .iter()
            .map(|(name, id)| (name.to_ascii_uppercase(), *id))
            .collect();

        state
            .validate_for_image(&image)
            .expect("old version-2 index map is migrated");
        assert_eq!(state.get_variable("by_actor", 0, 2), Ok(91));
        assert_eq!(state.get_array("values", 1), Ok(-7));
        assert!(matches!(
            state.get_variable("BY_ACTOR", 0, 2),
            Err(EdukeVmError::UnknownName {
                kind: "variable",
                ..
            })
        ));
    }

    #[test]
    fn instruction_stream_runs_state_calls_comparisons_and_switches() {
        let mut state = EdukeVmState::from_image(&image()).expect("valid declarations");
        let program = EdukeProgram {
            instructions: vec![
                EdInstruction::Assign {
                    target: EdLValue::Variable("shared".into()),
                    value: EdOperand::Immediate(1),
                },
                EdInstruction::CallState {
                    name: "increment".into(),
                },
                EdInstruction::Switch {
                    value: EdOperand::Variable("shared".into()),
                    cases: vec![(2, 5)],
                    default_target: 4,
                },
                EdInstruction::Assign {
                    target: EdLValue::Variable("shared".into()),
                    value: EdOperand::Immediate(-1),
                },
                EdInstruction::Return,
                EdInstruction::Assign {
                    target: EdLValue::Variable("shared".into()),
                    value: EdOperand::Immediate(9),
                },
                EdInstruction::Return,
                EdInstruction::Binary {
                    target: EdLValue::Variable("shared".into()),
                    operator: EdBinaryOperator::Add,
                    right: EdOperand::Immediate(1),
                },
                EdInstruction::Return,
            ],
            states: BTreeMap::from([("increment".into(), 7)]),
        };
        let result = state
            .execute(&program, 0, 0, 16, 4)
            .expect("program should execute");
        assert!(result.returned);
        assert_eq!(state.get_variable("shared", 0, 0), Ok(9));
        assert!(compare_values(EdComparison::BitAndNonZero, 0b1100, 0b0100));
        assert!(!compare_values(EdComparison::BitAndNonZero, 0b1100, 0b0010));
        assert!(compare_values(EdComparison::BitXorNonZero, 1, 2));
        assert!(compare_values(EdComparison::BothNonZero, -1, 1));
        assert!(compare_values(EdComparison::UnsignedGreater, -1, 1));
        assert!(!compare_values(EdComparison::UnsignedLess, -1, 1));
    }

    #[test]
    fn every_vm_comparison_uses_its_signed_bitwise_or_unsigned_rule() {
        let cases = [
            (EdComparison::Equal, 4, 4, true),
            (EdComparison::NotEqual, 4, 5, true),
            (EdComparison::Less, -2, 1, true),
            (EdComparison::LessEqual, 4, 4, true),
            (EdComparison::Greater, 5, 4, true),
            (EdComparison::GreaterEqual, 4, 4, true),
            (EdComparison::BitAndNonZero, 0b1100, 0b0100, true),
            (EdComparison::BitAndZero, 0b1100, 0b0010, true),
            (EdComparison::EitherNonZero, 0, 1, true),
            (EdComparison::BothZero, 0, 0, true),
            (EdComparison::BitXorNonZero, 1, 2, true),
            (EdComparison::BitXorZero, 3, 3, true),
            (EdComparison::BothNonZero, -1, 1, true),
            (EdComparison::EitherZero, 0, 2, true),
            (EdComparison::UnsignedGreater, -1, 1, true),
            (EdComparison::UnsignedGreaterEqual, -1, -1, true),
            (EdComparison::UnsignedLess, 1, -1, true),
            (EdComparison::UnsignedLessEqual, 1, 1, true),
        ];
        for (comparison, left, right, expected) in cases {
            assert_eq!(
                compare_values(comparison, left, right),
                expected,
                "{comparison:?}({left}, {right})"
            );
        }
    }

    #[test]
    fn hitscan_reads_ray_and_writes_six_result_variables() {
        let source = b"gamevar hs -1 0\ngamevar hw -1 0\ngamevar hsp -1 0\ngamevar hx 0 0\ngamevar hy 0 0\ngamevar hz 0 0\ndefstate cast\nhitscan 10 20 30 1 16384 0 -256 hs hw hsp hx hy hz 4294901808\nends\n";
        let image = compile_with_options(
            source,
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={}", image.errors);
        let entry = image.eduke_program.states["cast"];
        let mut state = EdukeVmState::from_image(&image).expect("declarations");
        let mut host = TestStructAccess::default();
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 0, 8, 4, &mut host)
            .expect("hitscan instruction");
        assert_eq!(
            host.hit_scan_args,
            Some(([10, 20, 30, 1], [16384, 0, -256], 4_294_901_808))
        );
        assert_eq!(state.get_variable("hs", 0, 0), Ok(2));
        assert_eq!(state.get_variable("hw", 0, 0), Ok(-1));
        assert_eq!(state.get_variable("hsp", 0, 0), Ok(9));
        assert_eq!(state.get_variable("hx", 0, 0), Ok(100));
        assert_eq!(state.get_variable("hy", 0, 0), Ok(200));
        assert_eq!(state.get_variable("hz", 0, 0), Ok(300));
    }

    #[test]
    fn hitradius_evaluates_damage_bands_and_calls_the_live_world_host() {
        let image = compile_with_options(
            b"gamevar blast_radius 700 0\ngamevar inner_damage 23 0\ndefstate blast\nhitradius blast_radius 3 7 13 inner_damage\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("hitradius"), None);

        let entry = image.eduke_program.states["blast"];
        let mut state = EdukeVmState::from_image(&image).expect("declarations");
        let mut host = TestStructAccess::default();
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("hitradius instruction");
        assert_eq!(host.hitradius_calls, [(7, 700, [3, 7, 13, 23])]);
    }

    #[test]
    fn pkick_dispatches_to_the_live_player_action_host() {
        let image = compile_with_options(
            b"defstate kick\npkick\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("pkick"), None);

        let entry = image.eduke_program.states["kick"];
        let mut state = EdukeVmState::from_image(&image).expect("declarations");
        let mut host = TestStructAccess::default();
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("pkick instruction");
        assert_eq!(host.quick_kick_calls, [7]);
    }

    #[test]
    fn allsprites_for_loop_uses_host_snapshot_in_order() {
        let source = b"gamevar iterator 0 0\ngamevar total 0 0\ndefstate loop_sprites\nfor iterator allsprites\n{\nadd total iterator\n}\nends\n";
        let image = compile_with_options(
            source,
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={}", image.errors);
        let entry = image.eduke_program.states["loop_sprites"];
        let mut state = EdukeVmState::from_image(&image).expect("declarations");
        let mut host = TestStructAccess {
            active_sprites: vec![2, 5, 9],
            ..TestStructAccess::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 0, 16, 4, &mut host)
            .expect("sprite iteration");
        assert_eq!(state.get_variable("total", 0, 0), Ok(16));
        assert_eq!(state.get_variable("iterator", 0, 0), Ok(9));
    }

    #[test]
    fn instruction_budget_stops_unbounded_branches() {
        let mut state = EdukeVmState::from_image(&image()).expect("valid declarations");
        let program = EdukeProgram {
            instructions: vec![EdInstruction::Jump { target: 0 }],
            states: BTreeMap::new(),
        };
        assert_eq!(
            state.execute(&program, 0, 0, 7, 4),
            Err(EdukeVmError::InstructionBudgetExceeded { budget: 7 })
        );
    }

    #[test]
    fn instruction_stream_executes_bounded_condition_loop_and_array_access() {
        let mut state = EdukeVmState::from_image(&image()).expect("valid declarations");
        let program = EdukeProgram {
            instructions: vec![
                EdInstruction::Assign {
                    target: EdLValue::Variable("shared".into()),
                    value: EdOperand::Immediate(0),
                },
                EdInstruction::JumpIf {
                    left: EdOperand::Variable("shared".into()),
                    comparison: EdComparison::GreaterEqual,
                    right: EdOperand::Immediate(2),
                    target: 4,
                },
                EdInstruction::Binary {
                    target: EdLValue::Variable("shared".into()),
                    operator: EdBinaryOperator::Add,
                    right: EdOperand::Immediate(1),
                },
                EdInstruction::Jump { target: 1 },
                EdInstruction::Assign {
                    target: EdLValue::ArrayElement {
                        name: "values".into(),
                        index: Box::new(EdOperand::Variable("shared".into())),
                    },
                    value: EdOperand::Immediate(23),
                },
                EdInstruction::Return,
            ],
            states: BTreeMap::new(),
        };
        let result = state
            .execute(&program, 0, 0, 16, 4)
            .expect("bounded loop should terminate");
        assert_eq!(state.get_variable("shared", 0, 0), Ok(2));
        assert_eq!(state.get_array("values", 2), Ok(23));
        assert_eq!(result.steps, 10);
    }

    #[test]
    fn script_random_streams_are_independent() {
        let mut state_a = EdukeVmState::from_image(&image()).expect("valid declarations");
        let mut state_b = state_a.clone();
        state_a.seed_random_streams(123, 456);
        state_b.seed_random_streams(123, 456);
        let _gameplay = state_a.random_bits(0x7fff, false);
        assert_eq!(
            state_a.random_bits(0x7fff, true),
            state_b.random_bits(0x7fff, true)
        );
        assert_ne!(
            state_a.random_bits(0x7fff, false),
            state_b.random_bits(0x7fff, false)
        );
    }

    #[test]
    fn randomseed_is_the_read_write_game_random_state() {
        let mut state = EdukeVmState::from_image(&image()).expect("valid declarations");
        assert_eq!(state.get_variable("randomseed", 0, 0), Ok(1996));
        assert_eq!(
            state.variable_scope("randomseed"),
            Ok(EdVariableScope::Global)
        );
        state
            .set_variable("randomseed", 0, 0, 123)
            .expect("built-in seed is writable");
        let mut expected_seed = 123;
        let expected_bits = krand_step(&mut expected_seed) & 0x7fff;
        assert_eq!(state.random_bits(0x7fff, false), expected_bits);
        assert_eq!(state.get_variable("randomseed", 0, 0), Ok(expected_seed));

        let encoded = serde_json::to_vec(&state).expect("serialize random state");
        let mut restored: EdukeVmState =
            serde_json::from_slice(&encoded).expect("restore random state");
        let expected_next_bits = krand_step(&mut expected_seed) & 0x7fff;
        assert_eq!(restored.random_bits(0x7fff, false), expected_next_bits);
        assert_eq!(restored.get_variable("randomseed", 0, 0), Ok(expected_seed));
    }

    #[test]
    fn ifrnd_uses_inclusive_probability_and_one_gameplay_draw() {
        let image = compile_with_options(
            b"gamevar always 0 0\ngamevar never 0 0\ngamevar fallback 0 0\ndefstate random_branches\nifrnd 255\nadd always 1\nifrnd -1\nadd never 1\nelse\nadd fallback 1\nife 1 1 ifrnd 255 add always 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert!(
            image
                .eduke_program
                .instructions
                .iter()
                .any(|instruction| { matches!(instruction, EdInstruction::IfRandom { .. }) })
        );

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        state.seed_random_streams(123, 456);
        let mut expected_seed = 123;
        let _ = krand_step(&mut expected_seed);
        let _ = krand_step(&mut expected_seed);
        let _ = krand_step(&mut expected_seed);
        state
            .execute_state(&image.eduke_program, "random_branches", 0, 0, 32, 4)
            .expect("ifrnd branches terminate");

        assert_eq!(state.get_variable("always", 0, 0), Ok(2));
        assert_eq!(state.get_variable("never", 0, 0), Ok(0));
        assert_eq!(state.get_variable("fallback", 0, 0), Ok(1));
        assert_eq!(state.get_variable("randomseed", 0, 0), Ok(expected_seed));
    }

    #[test]
    fn ifrnd_else_ifrnd_consumes_nested_brace_bodies() {
        let image = compile_with_options(
            b"var selected 0\ndefstate random_chain\nifrnd -1\n{\nadd selected 1\n}\nelse ifrnd -1\n{\nadd selected 2\n}\nelse\n{\nadd selected 4\n}\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_samples);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        state
            .execute_state(&image.eduke_program, "random_chain", 0, 0, 64, 8)
            .expect("nested else-if random branches terminate");
        assert_eq!(state.get_variable("selected", 0, 0), Ok(4));
    }

    #[test]
    fn ifrnd_threshold_is_inclusive_at_the_random_byte_boundary() {
        let image = compile_with_options(
            b"var threshold 0 0\nvar matched 0 0\ndefstate boundary\nifrnd threshold add matched 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);

        let mut expected_seed = 123;
        let roll = krand_step(&mut expected_seed) & 0xff;
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        state.seed_random_streams(123, 456);
        state
            .set_variable("threshold", 0, 0, roll)
            .expect("threshold gamevar");
        state
            .execute_state(&image.eduke_program, "boundary", 0, 0, 16, 4)
            .expect("equal random threshold passes");
        assert_eq!(state.get_variable("matched", 0, 0), Ok(1));

        state.seed_random_streams(123, 456);
        state
            .set_variable("threshold", 0, 0, roll - 1)
            .expect("threshold just below roll");
        state
            .set_variable("matched", 0, 0, 0)
            .expect("clear result");
        state
            .execute_state(&image.eduke_program, "boundary", 0, 0, 16, 4)
            .expect("lower random threshold fails");
        assert_eq!(state.get_variable("matched", 0, 0), Ok(0));
    }

    #[test]
    fn live_host_randomseed_reads_writes_and_draws_share_the_host_stream() {
        let image = compile_with_options(
            b"var observed 0 0\nvar negative_random 0 0\nvar random_value 0 0\nvar seed_after_draw 0 0\ndefstate shared_rng\nset observed randomseed\nrand negative_random -1\nrand random_value 100\nset seed_after_draw randomseed\nset randomseed 900\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);

        let mut expected_seed = 123;
        let random = krand_step(&mut expected_seed) & 0x7fff;
        let expected_random_value = random % 101;
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let mut host = TestStructAccess {
            game_random_seed: Some(123),
            ..Default::default()
        };
        state
            .execute_at_with_access(
                &image.eduke_program,
                image.eduke_program.states["shared_rng"],
                0,
                0,
                16,
                4,
                &mut host,
            )
            .expect("host-backed RNG executes");

        assert_eq!(state.get_variable("observed", 0, 0), Ok(123));
        assert_eq!(state.get_variable("negative_random", 0, 0), Ok(0));
        assert_eq!(
            state.get_variable("random_value", 0, 0),
            Ok(expected_random_value)
        );
        assert_eq!(
            state.get_variable("seed_after_draw", 0, 0),
            Ok(expected_seed)
        );
        assert_eq!(host.game_random_seed, Some(900));
        assert_eq!(state.get_variable("randomseed", 0, 0), Ok(900));
    }

    #[test]
    fn numplayers_is_a_live_global_builtin() {
        let image = compile_with_options(
            b"var observed_players 0\nvar observed_multimode 0\nappendevent EVENT_GAME\nset observed_players numplayers\nset observed_multimode MULTIMODE\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0);
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        assert_eq!(state.get_variable("numplayers", 0, 0), Ok(1));
        assert_eq!(
            state.variable_scope("numplayers"),
            Ok(EdVariableScope::Global)
        );
        state.set_num_players(2);
        state.set_multimode(3);
        state
            .execute_registered_event_hooks(&image, "EVENT_GAME", 0, 0, 8, 4)
            .expect("event observes the live player count");
        assert_eq!(state.get_variable("observed_players", 0, 0), Ok(2));
        assert_eq!(state.get_variable("observed_multimode", 0, 0), Ok(3));
    }

    #[test]
    fn source_compiler_emits_executable_event_and_state_subset() {
        let image = compile_with_options(
            b"var amount 0\ndefine INITIAL_AMOUNT 5\ndefstate increment\nadd amount 1\nends\nappendevent EVENT_GAME\nset amount INITIAL_AMOUNT\nstate increment\nife amount 6 add amount 3\nifn amount 9 set amount 99\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={}", image.errors);
        let event_entry = image.eduke_event_entries[0].1;
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        state
            .execute_at(&image.eduke_program, event_entry, 0, 0, 32, 8)
            .expect("event bytecode should run");
        assert_eq!(state.get_variable("amount", 0, 0), Ok(9));
    }

    #[test]
    fn actor_state_conditionals_read_live_temps_and_sprite_fields() {
        let image = compile_with_options(
            b"define ACTION_RUN 7\nvar matched 0\nvar misses 0\ndefstate check_conditions\nifcount 3 add matched 1\nifcount 4 add misses 1 else add matched 2\nifaction ACTION_RUN add matched 4\nifactioncount 2 add matched 8\nifai 11 add matched 16\nifmove 9 add matched 32\nifactor 42 add matched 64\nifstrength 10 add matched 128\nifwasweapon 99 add matched 256\nifspawnedby 99 add matched 512\nifspritepal 4 add matched 1024\nifmultiplayer add matched 2048\nife 1 1 ifcount 3 add matched 4096\nifpdistl 100 add matched 8192\nifpdistg 40 add matched 16384\nifinwater add matched 32768\nifoutside add matched 65536\nifphealthl 20 add matched 131072\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifcount"), None);
        assert_eq!(image.eduke_noop_heads.get("ifmultiplayer"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        state.set_multimode(2);
        let mut host = TestStructAccess {
            actor_temp_data: [3, 9, 2, 0, 7, 11],
            actor_picnum: 42,
            actor_extra: 10,
            actor_pal: 4,
            actor_hit_picnum: 99,
            actor_sectnum: 5,
            sector_lotag: 2,
            sector_ceilingstat: 1,
            player_extra: 10,
            nearest_player_distance: 50,
            ..Default::default()
        };
        let entry = image.eduke_program.states["check_conditions"];
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 128, 8, &mut host)
            .expect("actor-state conditions execute against live fields");

        assert_eq!(state.get_variable("matched", 0, 7), Ok(262_143));
        assert_eq!(state.get_variable("misses", 0, 7), Ok(0));
    }

    #[test]
    fn ifangdiffl_uses_shortest_wrapped_player_actor_angle() {
        let image = compile_with_options(
            b"var inclusive 0\nvar too_narrow 0\ndefstate check_angle\nifangdiffl 16 add inclusive 1 else add too_narrow 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifangdiffl"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_angle"];
        let mut host = TestStructAccess {
            player_angle: 2040,
            actor_angle: 8,
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("wrapped angle condition executes");
        assert_eq!(state.get_variable("inclusive", 0, 7), Ok(1));
        assert_eq!(state.get_variable("too_narrow", 0, 7), Ok(0));

        state
            .set_variable("inclusive", 0, 7, 0)
            .expect("reset result");
        state
            .set_variable("too_narrow", 0, 7, 0)
            .expect("reset result");
        host.actor_angle = 9;
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("angle just outside the limit executes");
        assert_eq!(state.get_variable("inclusive", 0, 7), Ok(0));
        assert_eq!(state.get_variable("too_narrow", 0, 7), Ok(1));
    }

    #[test]
    fn ifawayfromwall_uses_live_actor_geometry() {
        let image = compile_with_options(
            b"var away 0\nvar close 0\ndefstate check_wall_clearance\nifawayfromwall add away 1 else add close 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifawayfromwall"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_wall_clearance"];
        let mut host = TestStructAccess {
            actor_away_from_wall_result: true,
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("clearance condition executes");
        assert_eq!(state.get_variable("away", 0, 7), Ok(1));
        assert_eq!(state.get_variable("close", 0, 7), Ok(0));

        state.set_variable("away", 0, 7, 0).expect("reset result");
        state.set_variable("close", 0, 7, 0).expect("reset result");
        host.actor_away_from_wall_result = false;
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("wall proximity condition executes");
        assert_eq!(state.get_variable("away", 0, 7), Ok(0));
        assert_eq!(state.get_variable("close", 0, 7), Ok(1));
    }

    #[test]
    fn ifactorsound_queries_the_selected_sprite_and_sound() {
        let image = compile_with_options(
            b"var audible 0\nvar silent 0\ndefstate check_sound\nifactorsound 17 42 add audible 1 else add silent 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifactorsound"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_sound"];
        let mut host = TestStructAccess {
            actor_sound_result: true,
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("sound condition executes");
        assert_eq!(host.actor_sound_query, Some((17, 42)));
        assert_eq!(state.get_variable("audible", 0, 7), Ok(1));
        assert_eq!(state.get_variable("silent", 0, 7), Ok(0));

        state
            .set_variable("audible", 0, 7, 0)
            .expect("reset result");
        state.set_variable("silent", 0, 7, 0).expect("reset result");
        host.actor_sound_result = false;
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("silent condition executes");
        assert_eq!(state.get_variable("audible", 0, 7), Ok(0));
        assert_eq!(state.get_variable("silent", 0, 7), Ok(1));
    }

    #[test]
    fn mikesnd_plays_the_actor_yvel_sound() {
        let image = compile_with_options(
            b"defstate play_microphone_sound\nmikesnd\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("mikesnd"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["play_microphone_sound"];
        let mut host = TestStructAccess {
            actor_hitag: 73,
            actor_yvel: 74,
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 8, 4, &mut host)
            .expect("mikesnd executes against the current actor");

        assert_eq!(host.played_sounds, vec![(7, 74, EdSoundMode::Actor)]);
    }

    #[test]
    fn setactorsoundpitch_forwards_actor_sound_and_cent_offset() {
        let image = compile_with_options(
            b"defstate retune_sound\nsetactorsoundpitch -1 18 -750\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("setactorsoundpitch"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["retune_sound"];
        let mut host = TestStructAccess::default();
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("sound pitch command executes");

        assert_eq!(host.sound_pitch_calls, [(7, -1, 18, -750)]);
    }

    #[test]
    fn findnearactor3d_writes_the_host_search_result() {
        let image = compile_with_options(
            b"var found -2\ndefstate search_actor\nfindnearactor3d 9001 200 found\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("findnearactor3d"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["search_actor"];
        let mut host = TestStructAccess {
            near_actor_result: 23,
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("near-actor lookup executes");

        assert_eq!(host.near_actor_query, Some((7, 9001, 200)));
        assert_eq!(state.get_variable("found", 0, 7), Ok(23));
    }

    #[test]
    fn ifonwater_checks_sector_lotag_and_floor_distance() {
        let image = compile_with_options(
            b"var matched 0\ndefstate check_onwater\nifonwater add matched 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifonwater"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_onwater"];
        let mut host = TestStructAccess {
            actor_sectnum: 5,
            sector_lotag: 1,
            sector_floorz: 10 << 8,
            actor_position: [0, 0, (10 << 8) + (32 << 8) - 1],
            ..Default::default()
        };

        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 32, 8, &mut host)
            .expect("near-water actor condition executes");
        assert_eq!(state.get_variable("matched", 0, 7), Ok(1));

        state
            .set_variable("matched", 0, 7, 0)
            .expect("reset matched result");
        host.actor_position[2] = (10 << 8) + (32 << 8);
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 32, 8, &mut host)
            .expect("water-distance boundary executes");
        assert_eq!(state.get_variable("matched", 0, 7), Ok(0));

        host.sector_lotag = 2;
        host.actor_position[2] = 10 << 8;
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 32, 8, &mut host)
            .expect("underwater actor condition executes");
        assert_eq!(state.get_variable("matched", 0, 7), Ok(0));
    }

    #[test]
    fn ifinouterspace_requires_parallax_space_floor_without_ceiling_tint() {
        let image = compile_with_options(
            b"var matched 0\ndefstate check_space\nifinouterspace add matched 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifinouterspace"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_space"];
        let mut host = TestStructAccess {
            actor_sectnum: 5,
            sector_floorstat: 1,
            sector_floorpicnum: 80,
            ..Default::default()
        };

        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 32, 8, &mut host)
            .expect("outer-space floor condition executes");
        assert_eq!(state.get_variable("matched", 0, 7), Ok(1));

        state
            .set_variable("matched", 0, 7, 0)
            .expect("reset matched result");
        host.sector_ceilingpal = 1;
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 32, 8, &mut host)
            .expect("tinted ceiling is not outer space");
        assert_eq!(state.get_variable("matched", 0, 7), Ok(0));

        host.sector_ceilingpal = 0;
        host.sector_floorpicnum = 81;
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 32, 8, &mut host)
            .expect("non-space floor is not outer space");
        assert_eq!(state.get_variable("matched", 0, 7), Ok(0));
    }

    #[test]
    fn ifnosounds_checks_the_active_actor_voice_owner() {
        let image = compile_with_options(
            b"var matched 0\nvar sounds_active 0\ndefstate check_sounds\nifnosounds add matched 1 else add sounds_active 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifnosounds"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_sounds"];
        let mut host = TestStructAccess {
            actor_has_active_sound: true,
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 32, 8, &mut host)
            .expect("active voice condition executes");
        assert_eq!(state.get_variable("matched", 0, 7), Ok(0));
        assert_eq!(state.get_variable("sounds_active", 0, 7), Ok(1));

        host.actor_has_active_sound = false;
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 32, 8, &mut host)
            .expect("quiet actor condition executes");
        assert_eq!(state.get_variable("matched", 0, 7), Ok(1));
        assert_eq!(state.get_variable("sounds_active", 0, 7), Ok(1));
    }

    #[test]
    fn tip_command_dispatches_to_the_active_player() {
        let image = compile_with_options(
            b"defstate show_tip\ntip\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("tip"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["show_tip"];
        let mut host = TestStructAccess::default();
        state
            .execute_at_with_access(&image.eduke_program, entry, 1, 7, 32, 8, &mut host)
            .expect("TIP command dispatches");
        assert_eq!(host.tip_calls, [1]);
    }

    #[test]
    fn ifp_compiles_or_combined_flags_and_branches_through_the_live_host() {
        let image = compile_with_options(
            b"define pstanding 1\ndefine ponground 8192\ndefine pfacing 65536\nvar matched 0\nvar missed 0\ndefstate check_ifp\nifp pstanding ponground add matched 3 else add missed 1\nife 1 1 ifp pfacing add matched 4\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifp"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_ifp"];
        let mut host = TestStructAccess {
            ifp_result: true,
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 32, 8, &mut host)
            .expect("ifp true branches execute");
        assert_eq!(host.last_ifp_flags, 65_536);
        assert_eq!(state.get_variable("matched", 0, 7), Ok(7));
        assert_eq!(state.get_variable("missed", 0, 7), Ok(0));

        state
            .set_variable("matched", 0, 7, 0)
            .expect("reset matched result");
        state
            .set_variable("missed", 0, 7, 0)
            .expect("reset missed result");
        host.ifp_result = false;
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 32, 8, &mut host)
            .expect("ifp else branch executes");
        assert_eq!(host.last_ifp_flags, 65_536);
        assert_eq!(state.get_variable("matched", 0, 7), Ok(0));
        assert_eq!(state.get_variable("missed", 0, 7), Ok(1));
    }

    #[test]
    fn ifdead_uses_the_actor_health_and_player_tile_adjustment() {
        let image = compile_with_options(
            b"var dead_count 0\nvar still_count 0\ndefstate check_dead\nifdead add dead_count 1\nifnotmoving add still_count 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifdead"), None);
        assert_eq!(image.eduke_noop_heads.get("ifnotmoving"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_dead"];
        let mut host = TestStructAccess {
            actor_extra: 0,
            actor_picnum: 42,
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("zero-health non-player actors are not below zero");
        assert_eq!(state.get_variable("dead_count", 0, 7), Ok(0));

        host.actor_extra = -1;
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("negative-health actor is dead");
        assert_eq!(state.get_variable("dead_count", 0, 7), Ok(1));

        host.actor_extra = 0;
        host.actor_picnum = i32::from(crate::spawn::APLAYER);
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("player tile uses the historical one-point adjustment");
        assert_eq!(state.get_variable("dead_count", 0, 7), Ok(2));
        assert_eq!(state.get_variable("still_count", 0, 7), Ok(3));
    }

    #[test]
    fn ifhitspace_reads_the_active_players_sync_bits() {
        let image = compile_with_options(
            b"var pressed 0\ndefstate check_space\nifhitspace add pressed 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifhitspace"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_space"];
        let mut host = TestStructAccess {
            player_input_mask: 1 << 29,
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("active player space input branches");
        assert_eq!(state.get_variable("pressed", 0, 7), Ok(1));
    }

    #[test]
    fn ifsquished_branches_through_the_live_world_host() {
        let image = compile_with_options(
            b"var squished_count 0\ndefstate check_squished\nifsquished add squished_count 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifsquished"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_squished"];
        let mut host = TestStructAccess {
            actor_squished: true,
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("squished actor branch executes");
        assert_eq!(state.get_variable("squished_count", 0, 7), Ok(1));

        host.actor_squished = false;
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("unsquished actor branch skips");
        assert_eq!(state.get_variable("squished_count", 0, 7), Ok(1));
    }

    #[test]
    fn ifhitweapon_consumes_the_pending_hit_through_the_live_host() {
        let image = compile_with_options(
            b"var hit_count 0\ndefstate check_hit\nifhitweapon add hit_count 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifhitweapon"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_hit"];
        let mut host = TestStructAccess {
            actor_hit_by_weapon: true,
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("pending hit branches");
        assert_eq!(state.get_variable("hit_count", 0, 7), Ok(1));

        host.actor_hit_by_weapon = false;
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("consumed hit skips the branch");
        assert_eq!(state.get_variable("hit_count", 0, 7), Ok(1));
    }

    #[test]
    fn sectorofwall_writes_the_owning_sector_or_minus_one() {
        let image = compile_with_options(
            b"var wall_id 11\nvar sector_id -9\ndefstate find_wall_sector\nsectorofwall sector_id wall_id\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("sectorofwall"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["find_wall_sector"];
        let mut host = TestStructAccess::default();
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("wall lookup executes");
        assert_eq!(state.get_variable("sector_id", 0, 7), Ok(3));

        state
            .set_variable("wall_id", 0, 7, 99)
            .expect("wall id exists");
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("missing wall lookup executes");
        assert_eq!(state.get_variable("sector_id", 0, 7), Ok(-1));
    }

    #[test]
    fn addphealth_applies_the_amount_to_the_active_player_context() {
        let image = compile_with_options(
            b"defstate add_health\naddphealth 25\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("addphealth"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["add_health"];
        let mut host = TestStructAccess::default();
        state
            .execute_at_with_access(&image.eduke_program, entry, 2, 7, 16, 4, &mut host)
            .expect("player health command executes");
        assert_eq!(host.player_health_delta, Some((2, 7, 25)));
    }

    #[test]
    fn weapon_and_inventory_commands_dispatch_and_full_ammo_returns_from_event() {
        let image = compile_with_options(
            b"define PISTOL_WEAPON 1\ndefine SHOTGUN_WEAPON 2\ndefine GET_JETPACK 4\nvar shared 0\nappendevent EVENT_GAME\naddammo PISTOL_WEAPON 10\naddammovar PISTOL_WEAPON 12\naddweapon SHOTGUN_WEAPON 8\naddweaponvar SHOTGUN_WEAPON 9\naddinventory GET_JETPACK 1600\naddinventoryvar GET_JETPACK 1500\nadd shared 1\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("addammovar"), None);
        assert_eq!(image.eduke_noop_heads.get("addweapon"), None);
        assert_eq!(image.eduke_noop_heads.get("addinventory"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image
            .eduke_event_entries
            .iter()
            .find(|(event, _, _)| event == "EVENT_GAME")
            .expect("game event")
            .1;
        let mut host = TestStructAccess {
            player_weapon_result: true,
            ..TestStructAccess::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 2, 7, 16, 4, &mut host)
            .expect("inventory commands execute");
        assert_eq!(
            host.player_weapon_ops,
            [
                (2, 7, false, 1, 10),
                (2, 7, false, 1, 12),
                (2, 7, true, 2, 8),
                (2, 7, true, 2, 9),
            ]
        );
        assert_eq!(
            host.player_inventory_ops,
            [(2, 7, 4, 1600), (2, 7, 4, 1500)]
        );
        assert_eq!(state.get_variable("shared", 0, 0), Ok(1));

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let mut host = TestStructAccess::default();
        let execution = state
            .execute_at_with_access(&image.eduke_program, entry, 2, 7, 16, 4, &mut host)
            .expect("full ammo aborts the event without faulting");
        assert!(execution.returned);
        assert_eq!(host.player_weapon_ops, [(2, 7, false, 1, 10)]);
        assert!(host.player_inventory_ops.is_empty());
        assert_eq!(state.get_variable("shared", 0, 0), Ok(0));
    }

    #[test]
    fn debris_compiles_and_dispatches_tile_and_fragment_count() {
        let image = compile_with_options(
            b"defstate debris_burst\ndebris 9000 3\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("debris"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["debris_burst"];
        let mut host = TestStructAccess::default();
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("debris command executes");
        assert_eq!(host.debris_calls, vec![(7, 9000, 3)]);
    }

    #[test]
    fn guts_compiles_and_dispatches_the_active_player_and_actor() {
        let image = compile_with_options(
            b"defstate gore_burst\nguts 9000 2\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("guts"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["gore_burst"];
        let mut host = TestStructAccess::default();
        state
            .execute_at_with_access(&image.eduke_program, entry, 2, 7, 16, 4, &mut host)
            .expect("guts command executes");
        assert_eq!(host.guts_calls, vec![(2, 7, 9000, 2)]);
    }

    #[test]
    fn lotsofglass_compiles_and_dispatches_fragment_count() {
        let image = compile_with_options(
            b"defstate glass_burst\nlotsofglass 4\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("lotsofglass"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["glass_burst"];
        let mut host = TestStructAccess::default();
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("glass command executes");
        assert_eq!(host.glass_calls, vec![(7, 4)]);
    }

    #[test]
    fn spawnceilingglass_compiles_and_dispatches_sprite_sector_and_count() {
        let image = compile_with_options(
            b"defstate ceiling_glass\nspawnceilingglass 12 3 4\nspawnceilingglass 5 6\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("spawnceilingglass"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["ceiling_glass"];
        let mut host = TestStructAccess::default();
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("ceiling glass command executes");
        assert_eq!(host.ceiling_glass_calls, vec![(12, 3, 4), (7, 5, 6)]);
    }

    #[test]
    fn item_spray_commands_compile_and_dispatch_their_count() {
        let image = compile_with_options(
            b"defstate item_sprays\nmoney 5\nmail 2\npaper 3\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("money"), None);
        assert_eq!(image.eduke_noop_heads.get("mail"), None);
        assert_eq!(image.eduke_noop_heads.get("paper"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["item_sprays"];
        let mut host = TestStructAccess::default();
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("item spray commands execute");
        assert_eq!(
            host.item_spray_calls,
            vec![
                (7, EdItemSprayKind::Money, 5),
                (7, EdItemSprayKind::Mail, 2),
                (7, EdItemSprayKind::Paper, 3),
            ]
        );
    }

    #[test]
    fn getticks_writes_host_clock_to_gamevar() {
        let image = compile_with_options(
            b"var elapsed 0\ndefstate read_clock\ngetticks elapsed\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("getticks"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let mut host = TestStructAccess::default();
        host.predefined_values.insert("getticks".into(), 12_345);
        let entry = image.eduke_program.states["read_clock"];
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 0, 8, 2, &mut host)
            .expect("getticks executes");
        assert_eq!(state.get_variable("elapsed", 0, 0), Ok(12_345));
    }

    #[test]
    fn setgamepalette_dispatches_the_player_and_palette_id() {
        let image = compile_with_options(
            b"defstate select_palette\nsetgamepalette 240\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("setgamepalette"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["select_palette"];
        let mut host = TestStructAccess::default();
        state
            .execute_at_with_access(&image.eduke_program, entry, 2, 7, 8, 2, &mut host)
            .expect("setgamepalette executes");
        assert_eq!(host.game_palette_calls, vec![(2, 240)]);
    }

    #[test]
    fn inittimer_dispatches_the_requested_clock_rate() {
        let image = compile_with_options(
            b"defstate slow_clock\ninittimer 60\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("inittimer"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["slow_clock"];
        let mut host = TestStructAccess::default();
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 0, 8, 2, &mut host)
            .expect("inittimer executes");
        assert_eq!(host.timer_rate_calls, vec![60]);
    }

    #[test]
    fn endofgame_dispatches_the_countdown_with_its_default() {
        let image = compile_with_options(
            b"defstate exit_episode\nendofgame\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("endofgame"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["exit_episode"];
        let mut host = TestStructAccess::default();
        state
            .execute_at_with_access(&image.eduke_program, entry, 1, 0, 8, 2, &mut host)
            .expect("endofgame executes");
        assert_eq!(host.end_of_game_calls, vec![(1, 52)]);
    }

    #[test]
    fn operate_dispatches_to_the_session_interaction_queue() {
        let image = compile_with_options(
            b"defstate use_nearby_sector\noperate\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("operate"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["use_nearby_sector"];
        let mut host = TestStructAccess::default();
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("operate command executes");
        assert_eq!(host.operate_calls, [7]);
    }

    #[test]
    fn pstomp_and_wackplayer_dispatch_with_deterministic_angle() {
        let image = compile_with_options(
            b"defstate player_commands\npstomp\nwackplayer\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("pstomp"), None);
        assert_eq!(image.eduke_noop_heads.get("wackplayer"), None);

        let mut expected_seed = 1996;
        let expected_angle = (128 - (krand_step(&mut expected_seed) & 255)) >> 1;
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["player_commands"];
        let mut host = TestStructAccess::default();
        state
            .execute_at_with_access(&image.eduke_program, entry, 1, 7, 8, 2, &mut host)
            .expect("player CON commands execute");

        assert_eq!(host.stomp_calls, [(1, 7)]);
        assert_eq!(host.wack_calls, [(1, 7, expected_angle)]);
        assert_eq!(state.get_variable("randomseed", 1, 7), Ok(expected_seed));
    }

    #[test]
    fn operatemasterswitches_dispatches_its_lotag() {
        let image = compile_with_options(
            b"defstate arm_masterswitches\noperatemasterswitches 42\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("operatemasterswitches"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["arm_masterswitches"];
        let mut host = TestStructAccess::default();
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("masterswitch command executes");
        assert_eq!(host.master_switch_lotags, [42]);
    }

    #[test]
    fn lineintersect_updates_outputs_only_when_segments_cross() {
        let image = compile_with_options(
            b"var ix 0 0\nvar iy 0 0\nvar iz 0 0\nvar hit 0 0\ndefstate intersect\nlineintersect 0 0 0 10 10 20 0 10 10 0 ix iy iz hit\nlineintersect 0 0 0 10 0 20 0 10 10 10 ix iy iz hit\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("lineintersect"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        state
            .execute_state(&image.eduke_program, "intersect", 0, 0, 16, 2)
            .expect("run line intersection commands");
        assert_eq!(state.get_variable("ix", 0, 0), Ok(5));
        assert_eq!(state.get_variable("iy", 0, 0), Ok(5));
        assert_eq!(state.get_variable("iz", 0, 0), Ok(10));
        assert_eq!(state.get_variable("hit", 0, 0), Ok(0));
    }

    #[test]
    fn respawnhitag_dispatches_the_current_actor_to_the_session() {
        let image = compile_with_options(
            b"defstate activate_respawns\nrespawnhitag\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("respawnhitag"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["activate_respawns"];
        let mut host = TestStructAccess::default();
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 13, 16, 4, &mut host)
            .expect("respawnhitag executes");
        assert_eq!(host.respawn_hitag_calls, [13]);
    }

    #[test]
    fn resetplayer_requests_a_restart_and_stops_the_event_chain() {
        let image = compile_with_options(
            b"var after_reset 0\nappendevent EVENT_GAME\nresetplayer\nset after_reset 1\nendevent\nappendevent EVENT_GAME\nset after_reset 2\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("resetplayer"), None);
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let mut host = TestStructAccess::default();
        let executions = state
            .execute_registered_event_hooks_with_access(
                &image,
                "EVENT_GAME",
                2,
                7,
                16,
                4,
                &mut host,
            )
            .expect("resetplayer requests the host restart");

        assert_eq!(host.player_restart_requests, [2]);
        assert_eq!(executions.len(), 1);
        assert!(executions[0].returned);
        assert!(executions[0].restart_player);
        assert_eq!(state.get_variable("after_reset", 0, 0), Ok(0));
    }

    #[test]
    fn getzrange_writes_live_floor_ceiling_and_hit_outputs() {
        let image = compile_with_options(
            b"var ceil_z 0\nvar ceil_hit 0\nvar floor_z 0\nvar floor_hit 0\ndefstate query_range\ngetzrange 10 20 30 4 ceil_z ceil_hit floor_z floor_hit 128 65537\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("getzrange"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["query_range"];
        let mut host = TestStructAccess {
            z_range_result: [-4096, 16388, 4096, 49161],
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("Build range query executes");

        assert_eq!(host.z_range_query, Some(([10, 20, 30, 4], 128, 65_537)));
        assert_eq!(state.get_variable("ceil_z", 0, 7), Ok(-4096));
        assert_eq!(state.get_variable("ceil_hit", 0, 7), Ok(16388));
        assert_eq!(state.get_variable("floor_z", 0, 7), Ok(4096));
        assert_eq!(state.get_variable("floor_hit", 0, 7), Ok(49161));
    }

    #[test]
    fn ifcansee_queries_actor_to_player_geometry_and_branches() {
        let image = compile_with_options(
            b"var visible 0\nvar blocked 0\ndefstate check_visibility\nifcansee add visible 1 else add blocked 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifcansee"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_visibility"];
        let mut host = TestStructAccess {
            actor_position: [100, 200, -300],
            actor_sectnum: 4,
            player_position: [900, 1200, -1500],
            player_cursectnum: 9,
            can_see_result: true,
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("visible actor/player line-of-sight query executes");
        assert_eq!(
            host.last_can_see,
            Some(([100, 200, -300, 4], [900, 1200, -1500, 9]))
        );
        assert_eq!(state.get_variable("visible", 0, 7), Ok(1));
        assert_eq!(state.get_variable("blocked", 0, 7), Ok(0));

        state
            .set_variable("visible", 0, 7, 0)
            .expect("reset visible");
        host.can_see_result = false;
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("blocked actor/player line-of-sight query executes");
        assert_eq!(state.get_variable("visible", 0, 7), Ok(0));
        assert_eq!(state.get_variable("blocked", 0, 7), Ok(1));
    }

    #[test]
    fn ifcanseetarget_uses_one_gameplay_draw_and_traces_actor_to_player() {
        let image = compile_with_options(
            b"var visible 0\nvar blocked 0\ndefstate check_target_visibility\nifcanseetarget add visible 1 else add blocked 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifcanseetarget"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_target_visibility"];
        let mut expected_seed = 1996;
        let random = krand_step(&mut expected_seed);
        let z_jitter = (random & 41).wrapping_shl(8);
        let mut host = TestStructAccess {
            actor_position: [100, 200, -300],
            actor_sectnum: 4,
            player_position: [900, 1200, -1500],
            player_cursectnum: 9,
            can_see_result: true,
            game_random_seed: Some(1996),
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("target visibility query executes");

        assert_eq!(
            host.last_can_see,
            Some((
                [100, 200, (-300i32).wrapping_sub(z_jitter), 4],
                [900, 1200, -1500, 9],
            ))
        );
        assert_eq!(host.can_see_calls, 1);
        assert_eq!(host.game_random_seed, Some(expected_seed));
        assert_eq!(host.seen_actor, Some(7));
        assert_eq!(state.get_variable("visible", 0, 7), Ok(1));
        assert_eq!(state.get_variable("blocked", 0, 7), Ok(0));
    }

    #[test]
    fn ifcanshoottarget_uses_the_live_actor_target_query() {
        let image = compile_with_options(
            b"var can_shoot 0\nvar blocked 0\ndefstate check_shoot_target\nifcanshoottarget add can_shoot 1 else add blocked 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifcanshoottarget"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_shoot_target"];
        let mut host = TestStructAccess {
            can_shoot_target: true,
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 3, 7, 16, 4, &mut host)
            .expect("firing-corridor query executes");
        assert_eq!(host.can_shoot_target_query, Some((3, 7)));
        assert_eq!(state.get_variable("can_shoot", 3, 7), Ok(1));
        assert_eq!(state.get_variable("blocked", 3, 7), Ok(0));

        host.can_shoot_target = false;
        state
            .execute_at_with_access(&image.eduke_program, entry, 3, 7, 16, 4, &mut host)
            .expect("blocked firing-corridor query executes");
        assert_eq!(state.get_variable("can_shoot", 3, 7), Ok(1));
        assert_eq!(state.get_variable("blocked", 3, 7), Ok(1));
    }

    #[test]
    fn ifbulletnear_uses_the_live_projectile_threat_query() {
        let image = compile_with_options(
            b"var threatened 0\nvar safe 0\ndefstate check_projectile\nifbulletnear add threatened 1 else add safe 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifbulletnear"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_projectile"];
        let mut host = TestStructAccess {
            bullet_near: true,
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("incoming-projectile query executes");
        assert_eq!(host.bullet_near_actor, Some(7));
        assert_eq!(state.get_variable("threatened", 0, 7), Ok(1));
        assert_eq!(state.get_variable("safe", 0, 7), Ok(0));

        state
            .set_variable("threatened", 0, 7, 0)
            .expect("reset threat counter");
        host.bullet_near = false;
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("clear projectile query executes");
        assert_eq!(state.get_variable("threatened", 0, 7), Ok(0));
        assert_eq!(state.get_variable("safe", 0, 7), Ok(1));
    }

    #[test]
    fn ifgotweaponce_passes_the_registration_mode_to_the_live_host() {
        let image = compile_with_options(
            b"var reserved 0\nvar unreserved 0\ndefstate check_reservation\nifgotweaponce 1 add reserved 1 else add unreserved 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifgotweaponce"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_reservation"];
        let mut host = TestStructAccess {
            weapon_reservation: true,
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 2, 7, 16, 4, &mut host)
            .expect("co-op reservation condition executes");
        assert_eq!(host.weapon_reservation_query, Some((2, 7, true)));
        assert_eq!(state.get_variable("reserved", 2, 7), Ok(1));
        assert_eq!(state.get_variable("unreserved", 2, 7), Ok(0));
    }

    #[test]
    fn ifcansee_accepts_visible_part_of_actor_tile_when_origin_is_blocked() {
        let image = compile_with_options(
            b"var visible 0\nvar blocked 0\ndefstate check_visibility\nifcansee add visible 1 else add blocked 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_visibility"];
        let mut host = TestStructAccess {
            actor_position: [100, 200, -300],
            actor_picnum: 42,
            actor_repeats: [8, 8],
            player_position: [900, 1200, -1500],
            actor_sectnum: 4,
            player_cursectnum: 9,
            actor_tile_geometry: [32, 64, 0, 0],
            visible_on_call: 5,
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("tile extent visibility probes execute");

        assert_eq!(host.can_see_calls, 5);
        let (visible_probe, player_position) = host.last_can_see.expect("visibility probe");
        assert_ne!(visible_probe, [100, 200, -300, 4]);
        assert_eq!(player_position, [900, 1200, -1500, 9]);
        assert_eq!(state.get_variable("visible", 0, 7), Ok(1));
        assert_eq!(state.get_variable("blocked", 0, 7), Ok(0));
    }

    #[test]
    fn ifcansee_wall_samples_follow_the_build_sprite_angle() {
        let probes = actor_tile_visibility_probes(
            [0, 0, 0, 2],
            [100, 100, 0, 3],
            [16, 256, 8, 8, 0, 0],
            [4, 4, 0, 0],
        );
        assert!(probes[5][0] > 0 && probes[5][1] > 0);
        assert_eq!(probes[5][3], 2);
    }

    #[test]
    fn ifpinventory_uses_the_player_inventory_condition_and_else_branch() {
        let image = compile_with_options(
            b"var matched 0\nvar not_matched 0\ndefstate check_inventory\nifpinventory 6 0 add matched 1 else add not_matched 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifpinventory"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_inventory"];
        let mut host = TestStructAccess {
            player_inventory_condition_result: true,
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 2, 7, 16, 4, &mut host)
            .expect("inventory test executes");

        assert_eq!(host.last_player_inventory_query, Some((6, 0, 2, 7)));
        assert_eq!(state.get_variable("matched", 2, 7), Ok(1));
        assert_eq!(state.get_variable("not_matched", 2, 7), Ok(0));
    }

    #[test]
    fn ifrespawn_and_respawn_predefined_variables_use_the_live_host() {
        let image = compile_with_options(
            b"var matched 0\nvar not_matched 0\nvar respawn_setting 0\nappendevent EVENT_GAME\nset RESPAWN_ITEMS 1\nset respawn_setting RESPAWN_ITEMS\nifrespawn add matched 1 else add not_matched 1\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifrespawn"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let event = image.eduke_event_entries[0].1;
        let mut host = TestStructAccess {
            respawn_enabled: true,
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, event, 0, 7, 16, 4, &mut host)
            .expect("respawn mode condition executes");

        assert_eq!(host.predefined_values.get("RESPAWN_ITEMS"), Some(&1));
        assert_eq!(state.get_variable("respawn_setting", 0, 7), Ok(1));
        assert_eq!(state.get_variable("matched", 0, 7), Ok(1));
        assert_eq!(state.get_variable("not_matched", 0, 7), Ok(0));

        state
            .set_variable("matched", 0, 7, 0)
            .expect("reset matched count");
        host.respawn_enabled = false;
        state
            .execute_at_with_access(&image.eduke_program, event, 0, 7, 16, 4, &mut host)
            .expect("disabled respawn option executes");
        assert_eq!(state.get_variable("matched", 0, 7), Ok(0));
        assert_eq!(state.get_variable("not_matched", 0, 7), Ok(1));
    }

    #[test]
    fn ifactornotstayput_branches_on_the_live_actor_temp() {
        let image = compile_with_options(
            b"var active 0\nvar restricted 0\ndefstate check_stayput\nifactornotstayput add active 1 else add restricted 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifactornotstayput"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_stayput"];
        let mut host = TestStructAccess {
            actor_not_stayput_result: true,
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("not-stayput condition executes");
        assert_eq!(state.get_variable("active", 0, 7), Ok(1));
        assert_eq!(state.get_variable("restricted", 0, 7), Ok(0));

        state
            .set_variable("active", 0, 7, 0)
            .expect("reset counter");
        host.actor_not_stayput_result = false;
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("stayput actor condition executes");
        assert_eq!(state.get_variable("active", 0, 7), Ok(0));
        assert_eq!(state.get_variable("restricted", 0, 7), Ok(1));
    }

    #[test]
    fn ifgapzl_uses_slope_adjusted_actor_sector_gap_in_build_units() {
        let image = compile_with_options(
            b"var narrow 0\nvar open 0\ndefstate check_gap\nifgapzl 16 add narrow 1 else add open 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("ifgapzl"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_gap"];
        let mut host = TestStructAccess {
            actor_position: [100, 200, -300],
            actor_sectnum: 4,
            sector_floorz: 15 << 8,
            sector_ceilingz: 0,
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("narrow sector condition executes");
        assert_eq!(state.get_variable("narrow", 0, 7), Ok(1));
        assert_eq!(state.get_variable("open", 0, 7), Ok(0));

        state.set_variable("narrow", 0, 7, 0).expect("reset narrow");
        host.sector_floorz = 20 << 8;
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("wide sector condition executes");
        assert_eq!(state.get_variable("narrow", 0, 7), Ok(0));
        assert_eq!(state.get_variable("open", 0, 7), Ok(1));
    }

    #[test]
    fn floor_and_ceiling_distance_conditions_measure_actor_center() {
        let image = compile_with_options(
            b"var floor_limit 69\nvar ceiling_limit 33\nvar floor_close 0\nvar floor_far 0\nvar ceiling_close 0\nvar ceiling_far 0\ndefstate check_distances\niffloordistl floor_limit add floor_close 1 else add floor_far 1\nifceilingdistl ceiling_limit add ceiling_close 1 else add ceiling_far 1\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("iffloordistl"), None);
        assert_eq!(image.eduke_noop_heads.get("ifceilingdistl"), None);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let entry = image.eduke_program.states["check_distances"];
        let mut host = TestStructAccess {
            actor_position: [100, 200, 40 << 8],
            actor_picnum: 42,
            actor_repeats: [32, 32],
            actor_sectnum: 4,
            sector_floorz: 100 << 8,
            sector_ceilingz: 0,
            actor_tile_geometry: [0, 32, 0, 0],
            ..Default::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("vertical distance conditions execute");
        assert_eq!(state.get_variable("floor_close", 0, 7), Ok(1));
        assert_eq!(state.get_variable("ceiling_close", 0, 7), Ok(1));

        for name in ["floor_close", "floor_far", "ceiling_close", "ceiling_far"] {
            state.set_variable(name, 0, 7, 0).expect("reset result");
        }
        state
            .set_variable("floor_limit", 0, 7, 68)
            .expect("set strict floor boundary");
        state
            .set_variable("ceiling_limit", 0, 7, 32)
            .expect("set strict ceiling boundary");
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 16, 4, &mut host)
            .expect("equal boundary skips strict less-than conditions");
        assert_eq!(state.get_variable("floor_close", 0, 7), Ok(0));
        assert_eq!(state.get_variable("floor_far", 0, 7), Ok(1));
        assert_eq!(state.get_variable("ceiling_close", 0, 7), Ok(0));
        assert_eq!(state.get_variable("ceiling_far", 0, 7), Ok(1));
    }

    #[test]
    fn registered_event_hooks_execute_in_source_order() {
        let image = compile_with_options(
            b"var order 0\nappendevent EVENT_GAME\nadd order 1\nendevent\nappendevent event_game\nmul order 10\nendevent\nappendevent EVENT_GAME\nadd order 2\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let executions = state
            .execute_registered_event_hooks(&image, "event_game", 0, 0, 16, 4)
            .expect("registered hooks run in sequence");

        assert_eq!(executions.len(), 3);
        assert_eq!(
            executions
                .iter()
                .map(|execution| execution.steps)
                .sum::<usize>(),
            6
        );
        assert_eq!(state.get_variable("order", 0, 0), Ok(12));

        state
            .set_variable("order", 0, 0, 0)
            .expect("reset hook order value");
        assert_eq!(
            state.execute_registered_event_hooks(&image, "EVENT_GAME", 0, 0, 5, 4),
            Err(EdukeVmError::InstructionBudgetExceeded { budget: 1 })
        );
        assert_eq!(
            state.get_variable("order", 0, 0),
            Ok(12),
            "the one remaining instruction is shared with the third hook"
        );
    }

    #[test]
    fn source_compiler_emits_bounded_while_and_switch_control_flow() {
        let image = compile_with_options(
            b"var amount 0\ndefstate update\nadd amount 1\nwhilel amount 3\n{\nadd amount 1\n}\nswitch amount\ncase 3\nadd amount 10\nbreak\ndefault\nset amount 99\nendswitch\nends\nappendevent EVENT_GAME\nstate update\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={}", image.errors);
        let event_entry = image.eduke_event_entries[0].1;
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        state
            .execute_at(&image.eduke_program, event_entry, 0, 0, 64, 8)
            .expect("loop and switch should run");
        assert_eq!(state.get_variable("amount", 0, 0), Ok(13));
    }

    #[test]
    fn killit_stops_actor_script_and_later_event_hooks() {
        let image = compile_with_options(
            b"var after 0\nonevent EVENT_GAME\nkillit\nset after 1\nendevent\nappendevent EVENT_GAME\nset after 2\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let mut host = TestStructAccess::default();

        let executions = state
            .execute_registered_event_hooks_with_access(
                &image,
                "EVENT_GAME",
                0,
                7,
                16,
                4,
                &mut host,
            )
            .expect("killit should return through the live host");

        assert_eq!(executions.len(), 1, "later hooks must be skipped");
        assert!(executions[0].killed_actor);
        assert_eq!(executions[0].kill_actor_id, Some(7));
        assert_eq!(state.get_variable("after", 0, 7), Ok(0));
    }

    #[test]
    fn event_return_stops_the_remaining_hook_chain() {
        let image = compile_with_options(
            b"var phase 0\nappendevent EVENT_GAME\nset phase 1\nreturn\nset phase 2\nendevent\nappendevent EVENT_GAME\nset phase 3\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");

        let executions = state
            .execute_registered_event_hooks(&image, "EVENT_GAME", 0, 7, 16, 4)
            .expect("return should stop this event chain");

        assert_eq!(executions.len(), 1);
        assert!(executions[0].returned);
        assert_eq!(state.get_variable("phase", 0, 7), Ok(1));
    }

    #[test]
    fn source_compiler_dispatches_actor_and_global_sound_commands() {
        let image = compile_with_options(
            b"var actor_sound_id 16\nvar global_sound_id 17\nappendevent EVENT_GAME\nsound 10\nsoundonce 11\nglobalsound 12\nstopsound 13\nactorsound 23 14\nstopactorsound 24 15\nsoundvar actor_sound_id\nglobalsoundvar global_sound_id\ntossweapon\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let mut host = TestStructAccess::default();

        state
            .execute_registered_event_hooks_with_access(
                &image,
                "EVENT_GAME",
                0,
                7,
                16,
                4,
                &mut host,
            )
            .expect("sound commands should use the live host bridge");

        assert_eq!(
            host.played_sounds,
            [
                (7, 10, EdSoundMode::Actor),
                (7, 11, EdSoundMode::ActorOnce),
                (7, 12, EdSoundMode::Global),
                (23, 14, EdSoundMode::Actor),
                (7, 16, EdSoundMode::Actor),
                (7, 17, EdSoundMode::Global),
            ]
        );
        assert_eq!(host.stopped_sounds, [(7, 13)]);
        assert_eq!(host.stopped_actor_sounds, [(24, 15)]);
        assert_eq!(host.weapon_toss_calls, [(0, 7)]);
    }

    #[test]
    fn event_sound_can_remap_sound_and_does_not_recurse() {
        let image = compile_with_options(
            b"var sound_source 0 0\nappendevent EVENT_SOUND\nset sound_source THISACTOR\nsound 11\nset RETURN 20\nendevent\nappendevent EVENT_GAME\nsound 10\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        let entry = image
            .eduke_event_entries
            .iter()
            .find(|(name, _, _)| name == "EVENT_GAME")
            .map(|(_, entry, _)| *entry)
            .expect("game event entry");
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let mut host = TestStructAccess::default();

        state
            .execute_at_with_access_and_events(&image, entry, 0, 7, 32, 4, &mut host)
            .expect("sound event should remap the sound and return");

        assert_eq!(
            host.played_sounds,
            [(7, 11, EdSoundMode::Actor), (7, 20, EdSoundMode::Actor)]
        );
        assert_eq!(state.get_variable("sound_source", 0, 7), Ok(7));
    }

    #[test]
    fn event_sound_global_uses_the_screenpeek_player_sprite() {
        let image = compile_with_options(
            b"var sound_source 0 0\nappendevent EVENT_SOUND\nset sound_source THISACTOR\nset RETURN 20\nendevent\nappendevent EVENT_GAME\nglobalsound 10\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        let entry = image
            .eduke_event_entries
            .iter()
            .find(|(name, _, _)| name == "EVENT_GAME")
            .map(|(_, entry, _)| *entry)
            .expect("game event entry");
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let mut host = TestStructAccess::default();

        state
            .execute_at_with_access_and_events(&image, entry, 0, 7, 32, 4, &mut host)
            .expect("global sound event should use local player context");

        assert_eq!(host.played_sounds, [(19, 20, EdSoundMode::Global)]);
        assert_eq!(state.get_variable("sound_source", 0, 19), Ok(19));
    }

    #[test]
    fn event_sound_return_minus_one_cancels_playback() {
        let image = compile_with_options(
            b"appendevent EVENT_SOUND\nset RETURN -1\nendevent\nappendevent EVENT_GAME\nsound 10\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        let entry = image
            .eduke_event_entries
            .iter()
            .find(|(name, _, _)| name == "EVENT_GAME")
            .map(|(_, entry, _)| *entry)
            .expect("game event entry");
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let mut host = TestStructAccess::default();

        state
            .execute_at_with_access_and_events(&image, entry, 0, 7, 32, 4, &mut host)
            .expect("sound event should cancel playback");

        assert!(host.played_sounds.is_empty());
    }

    #[test]
    fn source_compiler_dispatches_action_move_and_ai_labels() {
        let image = compile_with_options(
            b"action ACT_IDLE 0 4 1 1 10\nmove MOVE_RUN 32 0\nai AI_RUN ACT_IDLE MOVE_RUN 8\nappendevent EVENT_GAME\naction ACT_IDLE\nmove MOVE_RUN 8\nai AI_RUN\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        let action = image.labels.get("ACT_IDLE").expect("action label").value;
        let movement = image.labels.get("MOVE_RUN").expect("move label").value;
        let ai = image.labels.get("AI_RUN").expect("AI label").value;
        assert!(matches!(
            image.eduke_program.instructions.as_slice(),
            [
                EdInstruction::SetActorAction { action: EdOperand::Immediate(a) },
                EdInstruction::SetActorMove {
                    movement: EdOperand::Immediate(m),
                    flags: EdOperand::Immediate(8)
                },
                EdInstruction::SetActorAi { ai: EdOperand::Immediate(i) },
                EdInstruction::EndEvent
            ] if *a == action && *m == movement && *i == ai
        ));

        let mut state = EdukeVmState::from_image(&image).expect("valid EDuke declarations");
        let mut host = TestStructAccess::default();
        state
            .execute_registered_event_hooks_with_access(
                &image,
                "EVENT_GAME",
                0,
                7,
                16,
                4,
                &mut host,
            )
            .expect("dispatch actor script pointer commands");
        assert_eq!(
            host.actor_script_ops,
            vec![
                ("action".to_owned(), 7, action, 0),
                ("move".to_owned(), 7, movement, 8),
                ("ai".to_owned(), 7, ai, 0),
            ]
        );
    }

    #[test]
    fn source_compiler_reads_live_struct_fields_in_conditions() {
        let image = compile_with_options(
            b"var result 0\ndefstate update\nifg sprite[THISACTOR].xvel 9 set result 1\nifg player[THISACTOR].ammo_amount 2 30 set result 2\nswitch result\ncase 2: set result 3\ncase 3 case 4\nset result 4\nendswitch\nends\nappendevent EVENT_GAME\nstate update\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        let entry = image.eduke_event_entries[0].1;
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let mut access = TestStructAccess {
            actor_xvel: 10,
            ..TestStructAccess::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 64, 8, &mut access)
            .expect("live condition operands should execute");
        assert_eq!(state.get_variable("result", 0, 7), Ok(4));
    }

    #[test]
    fn source_compiler_runs_quote_redefinition_and_qsprintf() {
        let image = compile_with_options(
            b"var amount 0\ndefine OUT_QUOTE 100\ndefine FORMAT_QUOTE 101\ndefinequote OUT_QUOTE \"\"\ndefinequote FORMAT_QUOTE \"%d / %d\"\ndefstate update\nredefinequote FORMAT_QUOTE \"value %d\"\nqsprintf OUT_QUOTE FORMAT_QUOTE amount\necho OUT_QUOTE\nends\nappendevent EVENT_GAME\nset amount 42\nstate update\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={}", image.errors);
        let entry = image.eduke_event_entries[0].1;
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        state
            .execute_at(&image.eduke_program, entry, 0, 0, 32, 8)
            .expect("quote instructions should execute");
        assert_eq!(state.quote(100), Ok("value 42"));
        assert_eq!(state.quote(101), Ok("value %d"));
        assert_eq!(state.take_console_output(), ["value 42"]);
        assert_eq!(image.eduke_noop_heads.get("echo"), None);
    }

    #[test]
    fn source_compiler_displays_quotes_for_player_and_suppresses_log_repeats() {
        let image = compile_with_options(
            b"definequote 100 \"first\"\ndefinequote 101 \"second\"\nappendevent EVENT_GAME\nquote 100\nquote 100\nquote 101\npalfrom 48 1 2 3\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("quote"), None);
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let mut host = TestStructAccess::default();
        state
            .execute_registered_event_hooks_with_access(
                &image,
                "EVENT_GAME",
                2,
                7,
                16,
                4,
                &mut host,
            )
            .expect("quote commands should execute");
        assert_eq!(host.quote_calls, [(2, 100), (2, 100), (2, 101)]);
        assert_eq!(host.palette_calls, [(2, [48, 1, 2, 3])]);
        assert_eq!(state.take_console_output(), ["first", "second"]);
    }

    #[test]
    fn console_echo_queue_retains_only_the_latest_bounded_messages() {
        let mut source = String::from("definequote 100 echo\nappendevent EVENT_GAME\n");
        for _ in 0..(MAX_EDUKE_CONSOLE_MESSAGES + 17) {
            source.push_str("echo 100\n");
        }
        source.push_str("endevent\n");
        let image = compile_with_options(
            source.as_bytes(),
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);

        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        state
            .execute_registered_event_hooks(&image, "EVENT_GAME", 0, 0, 1024, 8)
            .expect("echo hook executes");
        let output = state.take_console_output();
        assert_eq!(output.len(), MAX_EDUKE_CONSOLE_MESSAGES);
        assert!(output.iter().all(|line| line == "echo"));
        assert!(state.take_console_output().is_empty());
    }

    #[test]
    fn source_compiler_runs_bounded_game_and_display_random_ranges() {
        let image = compile_with_options(
            b"var roll 0\ndefine MAX_ROLL 6\nappendevent EVENT_GAME\nrand roll MAX_ROLL\nendevent\nappendevent EVENT_DISPLAYREST\ndisplayrand roll\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={}", image.errors);
        let game_entry = image.eduke_event_entries[0].1;
        let display_entry = image.eduke_event_entries[1].1;
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        state.seed_random_streams(123, 456);
        state
            .execute_at(&image.eduke_program, game_entry, 0, 0, 8, 4)
            .expect("rand instruction should execute");
        let game_roll = state.get_variable("roll", 0, 0).expect("declared variable");
        assert!((0..=6).contains(&game_roll));
        state
            .execute_at(&image.eduke_program, display_entry, 0, 0, 8, 4)
            .expect("displayrand instruction should execute");
        let display_roll = state.get_variable("roll", 0, 0).expect("declared variable");
        assert!((0..=32767).contains(&display_roll));
        assert_eq!(state.get_variable("roll", 0, 0), Ok(display_roll));
    }

    #[test]
    fn source_compiler_routes_struct_and_explicit_actor_var_access() {
        let image = compile_with_options(
            b"var temporary 0\nvar actor_age 0 2\ndefstate update\ngeta[THISACTOR].xvel temporary\nadd temporary 2\nseta[THISACTOR].xvel temporary\nsetactorvar[THISACTOR].actor_age 5\nends\nappendevent EVENT_GAME\nstate update\ngetactorvar[THISACTOR].actor_age temporary\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={}", image.errors);
        let entry = image.eduke_event_entries[0].1;
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let mut access = TestStructAccess {
            actor_xvel: 10,
            ..TestStructAccess::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 7, 32, 8, &mut access)
            .expect("host-backed instructions should execute");
        assert_eq!(access.actor_xvel, 12);
        assert_eq!(
            state.get_scoped_variable("actor_age", EdVariableScope::Actor, 7),
            Ok(5)
        );
        assert_eq!(state.get_variable("temporary", 0, 7), Ok(5));
    }

    #[test]
    fn userdef_vm_sprite_changes_thisactor_and_return_slot_zero_aliases_return() {
        let image = compile_with_options(
            b"var actor_age 0 2\nvar initial 0\nvar active 0\nvar current 0\nvar copied 0\ndefstate update\ngetuserdef[].vm_sprite initial\nsetuserdef[].vm_sprite 7\nsetactorvar[THISACTOR].actor_age 11\nset active THISACTOR\nsetuserdef[].return 1 77\ngetuserdef[].return 1 copied\ngetuserdef[].return 0 current\nsetuserdef[].return 0 42\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={}", image.errors);
        let entry = image.eduke_program.states["update"];
        let mut state = EdukeVmState::from_image(&image).expect("valid declarations");
        let mut host = TestStructAccess {
            vm_sprite: 2,
            ..TestStructAccess::default()
        };
        state
            .execute_at_with_access(&image.eduke_program, entry, 0, 2, 32, 4, &mut host)
            .expect("userdef accesses");

        assert_eq!(state.get_variable("initial", 0, 2), Ok(2));
        assert_eq!(state.get_variable("active", 0, 2), Ok(7));
        assert_eq!(state.get_variable("current", 0, 2), Ok(0));
        assert_eq!(
            state.get_scoped_variable("actor_age", EdVariableScope::Actor, 7),
            Ok(11)
        );
        assert_eq!(
            state.get_scoped_variable("actor_age", EdVariableScope::Actor, 2),
            Ok(0)
        );
        assert_eq!(state.get_variable("copied", 0, 2), Ok(77));
        assert_eq!(state.get_variable("RETURN", 0, 2), Ok(42));
        assert_eq!(host.vm_sprite, 7);
    }
}
