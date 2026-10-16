import { RoomMod, Actor, register, print, player, skill, spawn, random, Events, Spawn, defineClass, classProperty, endClass, ClassProp, TICRATE } from "./room";

class Hello extends RoomMod {
  kills: i32 = 0;
  register(): void {
    defineClass("GhostImp", "DoomImp");
    classProperty(ClassProp.HEALTH, 30);
    classProperty(ClassProp.ALPHA_PERCENT, 50);
    endClass();
  }
  onLoad(maptime: i32): void {
    print("Hello from AssemblyScript! skill " + skill().toString());
  }
  onTick(t: i32): void {
    if (t == 3 * TICRATE) {
      const p = player(0);
      if (p != null) {
        const pos = p.pos;
        spawn("GhostImp", pos.x + 256, pos.y, NaN, 180, Spawn.TELEFOG | Spawn.CHECK_POSITION);
      }
    }
  }
  onThingDied(thing: Actor, killer: Actor | null): void {
    if (killer != null && killer.isPlayer && thing.isMonster) {
      this.kills++;
      killer.health = killer.health + 10;
      print("Kill #" + this.kills.toString() + " (" + thing.className + "): +10 health");
    }
  }
}
register(new Hello("HelloAS"));
export * from "./room_abi";
