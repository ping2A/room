// Room game API for AssemblyScript mods (guide + reference: docs/12-wasm-scripting.md §16;
// full example: mods/doom1-bounty/bounty.ts).
//
//   import { RoomMod, Actor, register, print } from "./room";
//   class Hello extends RoomMod {
//     onLoad(maptime: i32): void { print("Hello!"); }
//   }
//   register(new Hello("Hello"));
//
// The editor's Scripts tab compiles this live. Offline (room.ts + room_abi.ts
// next to mod.ts, which ends with `export * from "./room_abi";`):
//   npx asc mod.ts --runtime incremental --use abort=room/roomAbort -O2 -o mod.wasm
// or: node scripts/asc-compile.mjs mod.ts mod.wasm
// Call the game only from RoomMod callbacks (the world is attached there).

// ------------------------------------------------------------------ imports

@external("room", "log") declare function _log(level: i32, ptr: usize, len: i32): void;
@external("room", "level_time") declare function _level_time(): i32;
@external("room", "player_count") declare function _player_count(): i32;
@external("room", "actor_exists") declare function _actor_exists(a: u64): i32;
@external("room", "class_find") declare function _class_find(ptr: usize, len: i32): i32;
@external("room", "reg_class_begin") declare function _reg_class_begin(ptr: usize, len: i32, parent: i32, replaces: i32, ednum: i32, flags: i32): i32;
@external("room", "reg_class_property_i32") declare function _reg_class_property_i32(prop: i32, v: i32): void;
@external("room", "reg_class_property_str") declare function _reg_class_property_str(prop: i32, ptr: usize, len: i32): void;
@external("room", "reg_class_flag") declare function _reg_class_flag(flag: i32, on: i32): void;
@external("room", "reg_class_end") declare function _reg_class_end(): void;
@external("room", "reg_event_handler") declare function _reg_event_handler(ptr: usize, len: i32, order: i32, flags: i32, lo: i32, hi: i32, base: i32): i32;
@external("room", "acs_execute") declare function _acs_execute(script: i32, map: i32, args: usize, n: i32): i32;
@external("room", "game_skill") declare function _game_skill(): i32;
@external("room", "level_mapname") declare function _level_mapname(buf: usize, cap: i32): i32;
@external("room", "level_stat") declare function _level_stat(stat: i32): i32;
@external("room", "math_sin") declare function _math_sin(r: f64): f64;
@external("room", "math_cos") declare function _math_cos(r: f64): f64;
@external("room", "math_atan2") declare function _math_atan2(y: f64, x: f64): f64;
@external("room", "math_sqrt") declare function _math_sqrt(x: f64): f64;
@external("room", "rng_range") declare function _rng_range(lo: i32, hi: i32): i32;
@external("room", "player_actor") declare function _player_actor(p: i32): u64;
@external("room", "player_get_i32") declare function _player_get_i32(p: i32, prop: i32): i32;
@external("room", "player_print") declare function _player_print(p: i32, ptr: usize, len: i32): void;
@external("room", "actor_class") declare function _actor_class(a: u64): i32;
@external("room", "class_name") declare function _class_name(c: i32, buf: usize, cap: i32): i32;
@external("room", "class_is_descendant") declare function _class_is_descendant(c: i32, parent: i32): i32;
@external("room", "actor_pos") declare function _actor_pos(a: u64, out: usize): void;
@external("room", "actor_vel") declare function _actor_vel(a: u64, out: usize): void;
@external("room", "actor_angle") declare function _actor_angle(a: u64): f64;
@external("room", "actor_get_i32") declare function _actor_get_i32(a: u64, prop: i32): i32;
@external("room", "actor_get_f64") declare function _actor_get_f64(a: u64, prop: i32): f64;
@external("room", "actor_get_ref") declare function _actor_get_ref(a: u64, ptr: i32): u64;
@external("room", "actor_iter_class") declare function _actor_iter_class(c: i32, sub: i32, out: usize, cap: i32): i32;
@external("room", "actor_iter_radius") declare function _actor_iter_radius(x: f64, y: f64, r: f64, out: usize, cap: i32): i32;
@external("room", "actor_check_sight") declare function _actor_check_sight(a: u64, b: u64): i32;
@external("room", "actor_distance") declare function _actor_distance(a: u64, b: u64): f64;
@external("room", "actor_count_inv") declare function _actor_count_inv(a: u64, c: i32): i32;
@external("room", "point_floor_z") declare function _point_floor_z(x: f64, y: f64): f64;
@external("room", "point_ceiling_z") declare function _point_ceiling_z(x: f64, y: f64): f64;
@external("room", "actor_spawn") declare function _actor_spawn(c: i32, x: f64, y: f64, z: f64, angle: f64, flags: i32): u64;
@external("room", "actor_destroy") declare function _actor_destroy(a: u64): void;
@external("room", "actor_set_i32") declare function _actor_set_i32(a: u64, prop: i32, v: i32): void;
@external("room", "actor_set_f64") declare function _actor_set_f64(a: u64, prop: i32, v: f64): void;
@external("room", "actor_set_ref") declare function _actor_set_ref(a: u64, ptr: i32, v: u64): void;
@external("room", "actor_set_pos") declare function _actor_set_pos(a: u64, x: f64, y: f64, z: f64): i32;
@external("room", "actor_set_vel") declare function _actor_set_vel(a: u64, x: f64, y: f64, z: f64): void;
@external("room", "actor_set_angle") declare function _actor_set_angle(a: u64, deg: f64): void;
@external("room", "actor_damage") declare function _actor_damage(t: u64, inflictor: u64, source: u64, amount: i32): i32;
@external("room", "actor_give_inventory") declare function _actor_give_inventory(a: u64, c: i32, n: i32): i32;
@external("room", "actor_take_inventory") declare function _actor_take_inventory(a: u64, c: i32, n: i32): i32;
@external("room", "actor_set_state") declare function _actor_set_state(a: u64, ptr: usize, len: i32): i32;
@external("room", "actor_start_sound") declare function _actor_start_sound(a: u64, ptr: usize, len: i32, vol: f64): void;
@external("room", "level_exit") declare function _level_exit(secret: i32): void;
@external("room", "level_set_music") declare function _level_set_music(ptr: usize, len: i32): void;
@external("room", "game_get_rule") declare function _game_get_rule(rule: i32): i32;
@external("room", "game_set_rule") declare function _game_set_rule(rule: i32, value: i32): void;
@external("room", "game_end_match") declare function _game_end_match(winner: i32): void;

// ------------------------------------------------------------------ constants

/** Event bits for `RoomMod.events`. */
export namespace Events {
  export const TICK: u64 = 1 << 0;
  export const THING_DIED: u64 = 1 << 1;
  export const LOADED: u64 = 1 << 3;
  export const THING_SPAWNED: u64 = 1 << 4;
  export const THING_DAMAGED: u64 = 1 << 5;
  export const PLAYER_ENTERED: u64 = 1 << 6;
  export const PLAYER_DIED: u64 = 1 << 7;
  export const UNLOADED: u64 = 1 << 8;
  export const ITEM_PICKUP: u64 = 1 << 9;
  export const ALL: u64 = TICK | THING_DIED | LOADED | THING_SPAWNED | THING_DAMAGED
    | PLAYER_ENTERED | PLAYER_DIED | UNLOADED | ITEM_PICKUP;
}

/** `levelStat` selectors. */
export namespace Stat {
  export const KILLS: i32 = 0;
  export const TOTAL_KILLS: i32 = 1;
  export const ITEMS: i32 = 2;
  export const TOTAL_ITEMS: i32 = 3;
  export const SECRETS: i32 = 4;
  export const TOTAL_SECRETS: i32 = 5;
  export const MAPTIME: i32 = 6;
}

/** `playerStat` selectors. */
export namespace PlayerStat {
  export const IN_GAME: i32 = 0;
  export const HEALTH: i32 = 1;
  export const ARMOR: i32 = 2;
  export const KILLS: i32 = 3;
  export const ITEMS: i32 = 4;
  export const SECRETS: i32 = 5;
}

/** `Actor.getInt` / `setInt` selectors. */
export namespace Prop {
  export const HEALTH: i32 = 0;
  export const SPAWN_HEALTH: i32 = 1;
  export const TID: i32 = 2;
  export const FLAGS: i32 = 3;
  export const IS_MONSTER: i32 = 4;
  export const PLAYER: i32 = 5;
  export const IS_MISSILE: i32 = 6;
  export const IS_DEAD: i32 = 7;
  export const DAMAGE: i32 = 8;
  export const REACTION_TIME: i32 = 9;
  export const FRIENDLY: i32 = 10;
}

/** `Actor.getFloat` / `setFloat` selectors. */
export namespace FProp {
  export const RADIUS: i32 = 0;
  export const HEIGHT: i32 = 1;
  export const SPEED: i32 = 2;
  export const ALPHA: i32 = 3;
  export const SCALE_X: i32 = 4;
  export const SCALE_Y: i32 = 5;
  export const GRAVITY: i32 = 6;
  export const DAMAGE_FACTOR: i32 = 7;
  export const FLOOR_Z: i32 = 8;
}

/** `spawn` flags. */
export namespace Spawn {
  export const TELEFOG: i32 = 1;
  export const CHECK_POSITION: i32 = 2;
  export const FRIENDLY: i32 = 4;
}

/** `getRule` / `setRule` ids; rules reset when a map starts (set them in `onLoad`). */
export namespace Rule {
  /** Players hurt each other, player kills count as frags (0/1). */
  export const DEATHMATCH: i32 = 0;
  /** Dead players respawn with Use (default 1); 0 keeps them down until the next map. */
  export const RESPAWN: i32 = 1;
  /** Read-only: tics left before the room time limit ends the match (0 = no limit). */
  export const TIME_LEFT: i32 = 2;
  /** Read-only: 1 once the match is over. */
  export const MATCH_OVER: i32 = 3;
}

/** `classProperty` ids (percent props are 0..100+). */
export namespace ClassProp {
  export const HEALTH: i32 = 1;
  export const SPEED: i32 = 2;
  export const PAIN_CHANCE: i32 = 3;
  export const RADIUS: i32 = 4;
  export const HEIGHT: i32 = 5;
  export const MASS: i32 = 6;
  export const REACTION_TIME: i32 = 7;
  export const MELEE_RANGE: i32 = 8;
  export const DAMAGE_FACTOR_PERCENT: i32 = 9;
  export const SCALE_PERCENT: i32 = 10;
  export const FAST_SPEED: i32 = 11;
  export const ALPHA_PERCENT: i32 = 12;
  export const TRANSLATION: i32 = 20;
  export const TAG: i32 = 21;
  export const OBITUARY: i32 = 22;
}

/** `classFlag` ids. */
export namespace ClassFlag {
  export const SOLID: i32 = 1;
  export const SHOOTABLE: i32 = 2;
  export const COUNTKILL: i32 = 3;
  export const FRIENDLY: i32 = 4;
  export const NOGRAVITY: i32 = 5;
  export const FLOAT: i32 = 6;
  export const NOBLOOD: i32 = 7;
  export const BOSS: i32 = 8;
  export const NORADIUSDMG: i32 = 9;
  export const INVULNERABLE: i32 = 10;
  export const DONTDRAW: i32 = 11;
}

export const TICRATE: i32 = 35;

// ------------------------------------------------------------------ helpers

const SCRATCH: usize = memory.data(256, 8);
const LIST_CAP: i32 = 512;
const LIST: usize = memory.data(LIST_CAP * 8, 8);

export function utf8(s: string): ArrayBuffer {
  return String.UTF8.encode(s);
}

function readName(len: i32): string {
  return String.UTF8.decodeUnsafe(SCRATCH, <usize>min(max(len, 0), 256));
}

export function actorOrNull(bits: u64): Actor | null {
  return bits == 0 ? null : new Actor(bits);
}

function bitsOf(a: Actor | null): u64 {
  return a == null ? 0 : a.ref;
}

/** 3D vector (positions, velocities). */
export class Vec3 {
  constructor(public x: f64 = 0, public y: f64 = 0, public z: f64 = 0) {}
}

function readVec(): Vec3 {
  return new Vec3(load<f64>(SCRATCH), load<f64>(SCRATCH, 8), load<f64>(SCRATCH, 16));
}

function readList(n: i32): Actor[] {
  const count = min(n, LIST_CAP);
  const out = new Array<Actor>(count);
  for (let i = 0; i < count; i++) out[i] = new Actor(load<u64>(LIST + <usize>i * 8));
  return out;
}

// ------------------------------------------------------------------ actors

/** Live actor handle (stale handles read as 0 and ignore writes). */
export class Actor {
  constructor(public ref: u64) {}

  get exists(): bool { return _actor_exists(this.ref) != 0; }
  get className(): string { return className(_actor_class(this.ref)); }
  /** Instance of `name` or a subclass of it. */
  is(name: string): bool { return _class_is_descendant(_actor_class(this.ref), classOf(name)) != 0; }

  get pos(): Vec3 { _actor_pos(this.ref, SCRATCH); return readVec(); }
  get vel(): Vec3 { _actor_vel(this.ref, SCRATCH); return readVec(); }
  /** Teleport (no collision check). */
  setPos(x: f64, y: f64, z: f64): bool { return _actor_set_pos(this.ref, x, y, z) != 0; }
  setVel(x: f64, y: f64, z: f64): void { _actor_set_vel(this.ref, x, y, z); }
  /** Facing in degrees. */
  get angle(): f64 { return _actor_angle(this.ref); }
  set angle(deg: f64) { _actor_set_angle(this.ref, deg); }

  getInt(prop: i32): i32 { return _actor_get_i32(this.ref, prop); }
  setInt(prop: i32, v: i32): void { _actor_set_i32(this.ref, prop, v); }
  getFloat(prop: i32): f64 { return _actor_get_f64(this.ref, prop); }
  setFloat(prop: i32, v: f64): void { _actor_set_f64(this.ref, prop, v); }

  get health(): i32 { return this.getInt(Prop.HEALTH); }
  set health(v: i32) { this.setInt(Prop.HEALTH, v); }
  get isMonster(): bool { return this.getInt(Prop.IS_MONSTER) != 0; }
  get isDead(): bool { return this.getInt(Prop.IS_DEAD) != 0; }
  get isPlayer(): bool { return this.getInt(Prop.PLAYER) > 0; }
  /** Player slot (0-based), or -1. */
  get playerSlot(): i32 { return this.getInt(Prop.PLAYER) - 1; }
  get speed(): f64 { return this.getFloat(FProp.SPEED); }
  set speed(v: f64) { this.setFloat(FProp.SPEED, v); }
  get alpha(): f64 { return this.getFloat(FProp.ALPHA); }
  set alpha(v: f64) { this.setFloat(FProp.ALPHA, v); }

  get target(): Actor | null { return actorOrNull(_actor_get_ref(this.ref, 0)); }
  set target(a: Actor | null) { _actor_set_ref(this.ref, 0, bitsOf(a)); }
  get master(): Actor | null { return actorOrNull(_actor_get_ref(this.ref, 1)); }
  set master(a: Actor | null) { _actor_set_ref(this.ref, 1, bitsOf(a)); }
  get tracer(): Actor | null { return actorOrNull(_actor_get_ref(this.ref, 2)); }
  set tracer(a: Actor | null) { _actor_set_ref(this.ref, 2, bitsOf(a)); }

  distanceTo(other: Actor): f64 { return _actor_distance(this.ref, other.ref); }
  canSee(other: Actor): bool { return _actor_check_sight(this.ref, other.ref) != 0; }

  countInv(item: string): i32 { return _actor_count_inv(this.ref, classOf(item)); }
  /** Give weapons, ammo, armor, health, keys or powerups. */
  give(item: string, amount: i32 = 1): bool { return _actor_give_inventory(this.ref, classOf(item), amount) != 0; }
  take(item: string, amount: i32 = 1): i32 { return _actor_take_inventory(this.ref, classOf(item), amount); }

  /** Damage through the normal damage path; returns damage dealt. */
  damage(amount: i32, source: Actor | null = null, inflictor: Actor | null = null): i32 {
    return _actor_damage(this.ref, bitsOf(inflictor), bitsOf(source), amount);
  }
  destroy(): void { _actor_destroy(this.ref); }
  /** Jump to a state label ("See", "Pain", "Death", …). */
  setState(label: string): bool { const b = utf8(label); return _actor_set_state(this.ref, changetype<usize>(b), b.byteLength) != 0; }
  sound(name: string, volume: f64 = 1.0): void { const b = utf8(name); _actor_start_sound(this.ref, changetype<usize>(b), b.byteLength, volume); }
}

// ------------------------------------------------------------------ world

/** Class handle by name (0 when unknown). */
export function classOf(name: string): i32 {
  const b = utf8(name);
  return _class_find(changetype<usize>(b), b.byteLength);
}

export function className(cls: i32): string { return readName(_class_name(cls, SCRATCH, 256)); }

/** Spawn `cls` at (x, y, z); z = NaN puts it on the floor. Null when blocked / unknown. */
export function spawn(cls: string, x: f64, y: f64, z: f64 = NaN, angle: f64 = 0, flags: i32 = 0): Actor | null {
  const zz = isNaN(z) ? _point_floor_z(x, y) : z;
  return actorOrNull(_actor_spawn(classOf(cls), x, y, zz, angle, flags));
}

/** Actors of `cls` ("" = every actor), up to 512. */
export function actorsOfClass(cls: string, subclasses: bool = true): Actor[] {
  const c = cls.length == 0 ? 0 : classOf(cls);
  return readList(_actor_iter_class(c, subclasses ? 1 : 0, LIST, LIST_CAP));
}

/** Actors whose centre is within `radius` of (x, y), up to 512. */
export function actorsInRadius(x: f64, y: f64, radius: f64): Actor[] {
  return readList(_actor_iter_radius(x, y, radius, LIST, LIST_CAP));
}

/** Player body for slot (0-based). */
export function player(slot: i32 = 0): Actor | null { return actorOrNull(_player_actor(slot + 1)); }
export function playerStat(slot: i32, stat: i32): i32 { return _player_get_i32(slot + 1, stat); }
export function playerCount(): i32 { return _player_count(); }

/** HUD message to everyone (slot -1) or one player. */
export function print(msg: string, slot: i32 = -1): void {
  const b = utf8(msg);
  _player_print(slot + 1, changetype<usize>(b), b.byteLength);
}

/** Editor log / console line. */
export function log(msg: string): void {
  const b = utf8(msg);
  _log(2, changetype<usize>(b), b.byteLength);
}

/** Skill 1 (ITYTD) ..= 5 (Nightmare). */
export function skill(): i32 { return _game_skill(); }
export function levelStat(stat: i32): i32 { return _level_stat(stat); }
/** Tics since the map started. */
export function levelTime(): i32 { return _level_time(); }
export function mapName(): string { return readName(_level_mapname(SCRATCH, 256)); }
export function floorZ(x: f64, y: f64): f64 { return _point_floor_z(x, y); }
export function ceilingZ(x: f64, y: f64): f64 { return _point_ceiling_z(x, y); }
export function exitLevel(secret: bool = false): void { _level_exit(secret ? 1 : 0); }
/** Change music ("D_E1M8"; "" stops). */
export function setMusic(name: string): void { const b = utf8(name); _level_set_music(changetype<usize>(b), b.byteLength); }
export function getRule(rule: i32): i32 { return _game_get_rule(rule); }
export function setRule(rule: i32, value: i32): void { _game_set_rule(rule, value); }
/** End the match: `winner` slot (0-based) or -1 for none. Shows the final standings. */
export function endMatch(winner: i32 = -1): void { _game_end_match(winner + 1); }
/** Run an ACS script on the current map. */
export function acs(script: i32, a0: i32 = 0, a1: i32 = 0, a2: i32 = 0): i32 {
  store<i32>(SCRATCH, a0); store<i32>(SCRATCH, a1, 4); store<i32>(SCRATCH, a2, 8);
  return _acs_execute(script, 0, SCRATCH, 3);
}

/** Inclusive random integer from the game RNG (deterministic; use instead of Math.random). */
export function random(lo: i32, hi: i32): i32 { return _rng_range(lo, hi); }
export function sin(rad: f64): f64 { return _math_sin(rad); }
export function cos(rad: f64): f64 { return _math_cos(rad); }
export function atan2(y: f64, x: f64): f64 { return _math_atan2(y, x); }
export function sqrt(x: f64): f64 { return _math_sqrt(x); }

// ------------------------------------------------------------------ classes

/** Declare an actor class (call from `RoomMod.register`). `editorNum` = DoomEdNum or -1. */
export function defineClass(name: string, parent: string, replaces: string = "", editorNum: i32 = -1): bool {
  const b = utf8(name);
  const p = classOf(parent);
  const r = replaces.length == 0 ? 0 : classOf(replaces);
  return _reg_class_begin(changetype<usize>(b), b.byteLength, p, r, editorNum, 0) != 0;
}
export function classProperty(prop: i32, value: i32): void { _reg_class_property_i32(prop, value); }
export function classPropertyString(prop: i32, value: string): void {
  const b = utf8(value);
  _reg_class_property_str(prop, changetype<usize>(b), b.byteLength);
}
export function classFlag(flag: i32, on: bool = true): void { _reg_class_flag(flag, on ? 1 : 0); }
export function endClass(): void { _reg_class_end(); }

// ------------------------------------------------------------------ mods

/** Override the callbacks you need; `events` limits which ones are sent. */
export class RoomMod {
  events: u64 = Events.ALL;
  order: i32 = 0;
  constructor(public name: string) {}
  /** Declare classes (`defineClass` … `endClass`). */
  register(): void {}
  onLoad(maptime: i32): void {}
  onTick(maptime: i32): void {}
  onUnload(): void {}
  onPlayerEnter(player: Actor, slot: i32): void {}
  onPlayerDied(player: Actor, slot: i32): void {}
  onThingSpawned(thing: Actor): void {}
  onThingDamaged(thing: Actor, source: Actor | null, inflictor: Actor | null, damage: i32, health: i32): void {}
  onThingDied(thing: Actor, killer: Actor | null): void {}
  onItemPickup(toucher: Actor, item: string): void {}
}

let current: RoomMod | null = null;

/** Install the mod (call once at top level). */
export function register(m: RoomMod): void { current = m; }

/** The registered mod. */
export function currentMod(): RoomMod | null { return current; }


export function roomAbort(msg: string | null, file: string | null, line: u32, col: u32): void {
  log("abort: " + (msg == null ? "" : msg) + " at " + (file == null ? "?" : file) + ":" + line.toString());
  unreachable();
}
