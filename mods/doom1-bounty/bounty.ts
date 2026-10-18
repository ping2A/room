// Bounty Hunter — example AssemblyScript mod for doom1.wad (works on any map).
//
// * One monster on the map is the bounty: tougher, bigger, faster. Every few
//   seconds a hint tells you where it is ("Imp, 900 units north-east").
// * Each claimed bounty pays out the next weapon (shotgun → chaingun → rocket
//   launcher), ammo, health and score. Skill scales toughness and pay.
// * Fast kills chain into combos for extra score.
// * When no monsters are left, "The Warden" (a class this mod defines) spawns
//   at the far end of the map as the final bounty.
//
// Build: ./scripts/room-server.sh mods · Edit live: editor Scripts tab.
import {
  RoomMod, Actor, register, print, player, spawn, skill, random, actorsOfClass,
  defineClass, classProperty, classPropertyString, endClass, ClassProp, FProp, Spawn, TICRATE,
  atan2, sqrt, log, levelTime,
} from "./room";

const FIRST_BOUNTY = 5 * TICRATE;
const NEXT_BOUNTY = 4 * TICRATE;
const COMBO_WINDOW = 3 * TICRATE;
const MAX_SPOTS = 256;

// Payouts per claimed bounty (the last entry repeats).
const WEAPONS: string[] = ["Shotgun", "Chaingun", "RocketLauncher", ""];
const AMMO: string[] = ["Shell", "ClipBox", "RocketAmmo", "Shell"];
const AMMO_AMOUNT: i32[] = [8, 1, 4, 12];

// Per skill 1..5: bounty health multiplier (%), hint interval (s), score per bounty.
const TOUGHNESS: i32[] = [150, 200, 250, 300, 400];
const HINT_EVERY: i32[] = [6, 8, 10, 12, 15];
const PAY: i32[] = [50, 75, 100, 150, 250];

const COMPASS: string[] = ["east", "north-east", "north", "north-west", "west", "south-west", "south", "south-east"];

class Spot {
  constructor(public x: f64, public y: f64) {}
}

class BountyHunter extends RoomMod {
  bounty: Actor | null = null;
  nextPick: i32 = FIRST_BOUNTY;
  nextHint: i32 = 0;
  lastDamageMsg: i32 = 0;
  claimed: i32 = 0;
  score: i32 = 0;
  combo: i32 = 0;
  lastKill: i32 = -1000;
  wardenSpawned: bool = false;
  spots: Spot[] = [];

  register(): void {
    // The final bounty: a bigger, tougher Baron.
    defineClass("Warden", "BaronOfHell");
    classProperty(ClassProp.HEALTH, 1500);
    classProperty(ClassProp.SCALE_PERCENT, 130);
    classProperty(ClassProp.SPEED, 10);
    classPropertyString(ClassProp.OBITUARY, "%o was judged by the Warden.");
    endClass();
  }

  onLoad(maptime: i32): void {
    this.bounty = null;
    this.nextPick = maptime + FIRST_BOUNTY;
    this.wardenSpawned = false;
    this.combo = 0;
    this.collectSpots();
    print("Bounty Hunter: skill " + skill().toString() + " - first bounty in 5s");
  }

  onTick(t: i32): void {
    const b = this.bounty;
    if (b == null) {
      if (t >= this.nextPick) this.pickBounty(t);
    } else if (!b.exists || b.isDead) {
      // Killed by something else (infighting, crusher, barrel): no payout.
      print("The bounty was lost - a new one is coming");
      this.bounty = null;
      this.nextPick = t + NEXT_BOUNTY;
    } else if (t >= this.nextHint) {
      this.hint(b);
      this.nextHint = t + HINT_EVERY[this.skillIndex()] * TICRATE;
    }
    if (this.combo >= 2 && t - this.lastKill > COMBO_WINDOW) {
      print("Combo x" + this.combo.toString() + " ended");
      this.combo = 0;
    }
  }

  onThingDamaged(thing: Actor, source: Actor | null, inflictor: Actor | null, damage: i32, health: i32): void {
    const b = this.bounty;
    if (b == null || thing.ref != b.ref || health <= 0) return;
    const now = this.time();
    if (now - this.lastDamageMsg < 2 * TICRATE) return;
    this.lastDamageMsg = now;
    print("Bounty hit - " + health.toString() + " HP left");
  }

  onThingDied(thing: Actor, killer: Actor | null): void {
    if (killer == null || !killer.isPlayer || !thing.isMonster) return;
    const now = this.time();
    this.combo = now - this.lastKill <= COMBO_WINDOW ? this.combo + 1 : 1;
    this.lastKill = now;
    this.score += 10 * this.combo;
    if (this.combo >= 3) print("Combo x" + this.combo.toString() + "!");

    const b = this.bounty;
    if (b != null && thing.ref == b.ref) this.claim(killer, thing, now);
  }

  onItemPickup(toucher: Actor, item: string): void {
    // Secret-ish bonus: backpacks pay score.
    if (toucher.isPlayer && item == "Backpack") {
      this.score += 100;
      print("Backpack: +100 score");
    }
  }

  onPlayerDied(p: Actor, slot: i32): void {
    print("Hunt over - score " + this.score.toString() + ", bounties " + this.claimed.toString());
  }

  onUnload(): void {
    log("map done: score " + this.score.toString() + ", bounties " + this.claimed.toString());
  }

  // ---------------------------------------------------------------- helpers

  time(): i32 { return levelTime(); }

  skillIndex(): i32 { return min(max(skill(), 1), 5) - 1; }

  collectSpots(): void {
    this.spots = [];
    const all = actorsOfClass("", false);
    for (let i = 0; i < all.length && this.spots.length < MAX_SPOTS; i++) {
      const p = all[i].pos;
      this.spots.push(new Spot(p.x, p.y));
    }
  }

  pickBounty(t: i32): void {
    const all = actorsOfClass("", false);
    const live: Actor[] = [];
    for (let i = 0; i < all.length; i++) {
      const a = all[i];
      if (a.isMonster && !a.isDead && !a.isPlayer) live.push(a);
    }
    if (live.length == 0) {
      if (!this.wardenSpawned) this.spawnWarden(t);
      else this.nextPick = t + NEXT_BOUNTY;
      return;
    }
    const b = live[random(0, live.length - 1)];
    b.health = b.health * TOUGHNESS[this.skillIndex()] / 100;
    b.setFloat(FProp.SCALE_X, 1.35);
    b.setFloat(FProp.SCALE_Y, 1.35);
    b.speed = b.speed * 1.3;
    this.bounty = b;
    this.nextHint = t + 2 * TICRATE;
    print("New bounty: " + b.className + " (" + b.health.toString() + " HP)");
  }

  spawnWarden(t: i32): void {
    const p = player(0);
    if (p == null || this.spots.length == 0) return;
    const pp = p.pos;
    // Try the 16 farthest spots (no closures in AssemblyScript: plain insertion sort).
    const order: Spot[] = [];
    const dist: f64[] = [];
    for (let i = 0; i < this.spots.length; i++) {
      const s = this.spots[i];
      const d = (s.x - pp.x) * (s.x - pp.x) + (s.y - pp.y) * (s.y - pp.y);
      order.push(s);
      dist.push(d);
      for (let j = dist.length - 1; j > 0 && dist[j - 1] < dist[j]; j--) {
        const td = dist[j]; dist[j] = dist[j - 1]; dist[j - 1] = td;
        const ts = order[j]; order[j] = order[j - 1]; order[j - 1] = ts;
      }
    }
    for (let i = 0; i < order.length && i < 16; i++) {
      const s = order[i];
      const w = spawn("Warden", s.x, s.y, NaN, 0, Spawn.TELEFOG | Spawn.CHECK_POSITION);
      if (w != null) {
        w.target = p;
        w.setState("See");
        this.wardenSpawned = true;
        this.bounty = w;
        this.nextHint = t + 2 * TICRATE;
        print("Map cleared! Final bounty: THE WARDEN");
        return;
      }
    }
    this.nextPick = t + NEXT_BOUNTY;
  }

  claim(killer: Actor, thing: Actor, now: i32): void {
    const i = min(this.claimed, WEAPONS.length - 1);
    this.claimed++;
    const pay = PAY[this.skillIndex()] * max(this.combo, 1);
    this.score += pay;
    if (WEAPONS[i].length > 0) killer.give(WEAPONS[i], 1);
    killer.give(AMMO[i], AMMO_AMOUNT[i]);
    killer.health = min(killer.health + 25, 200);
    const reward = WEAPONS[i].length > 0 ? WEAPONS[i] : AMMO[i];
    print("Bounty claimed: " + thing.className + "! +" + pay.toString() + " score, " + reward);
    this.bounty = null;
    if (thing.className == "Warden") {
      print("THE WARDEN FALLS - final score " + this.score.toString());
      this.nextPick = i32.MAX_VALUE;
    } else {
      this.nextPick = now + NEXT_BOUNTY;
    }
  }

  hint(b: Actor): void {
    const p = player(0);
    if (p == null) return;
    const bp = b.pos, pp = p.pos;
    const dx = bp.x - pp.x, dy = bp.y - pp.y;
    const dist = <i32>sqrt(dx * dx + dy * dy);
    const deg = atan2(dy, dx) * 180 / Math.PI;
    const dir = COMPASS[(<i32>Math.floor((deg + 360 + 22.5) / 45)) % 8];
    const seen = p.canSee(b) ? " - in sight!" : "";
    print("Bounty " + b.className + ": " + dist.toString() + " units " + dir + seen);
  }
}

register(new BountyHunter("BountyHunter"));
