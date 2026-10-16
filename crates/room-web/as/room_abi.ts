// Lifecycle exports for AssemblyScript Room mods (see room.ts). The editor
// appends `export * from "./room_abi";` to every module.

import { Actor, actorOrNull, className, currentMod, utf8 } from "./room";

@external("room", "reg_event_handler") declare function _reg_event_handler(ptr: usize, len: i32, order: i32, flags: i32, lo: i32, hi: i32, base: i32): i32;

const CALLBACK_BASE: i32 = 1000;

export function room_abi_version(): i32 { return 1; }
export function room_alloc(size: i32, align: i32): usize { return heap.alloc(<usize>max(size, 1)); }
export function room_free(ptr: usize, size: i32, align: i32): void { if (ptr != 0) heap.free(ptr); }

export function room_register(): i32 {
  const m = currentMod();
  if (m == null) return 0;
  const b = utf8(m.name);
  const code = _reg_event_handler(changetype<usize>(b), b.byteLength, m.order, 0,
    <i32>(m.events & 0xffffffff), <i32>(m.events >> 32), CALLBACK_BASE);
  if (code != 0) return code;
  m.register();
  return 0;
}

export function room_init(kind: i32): i32 { return 0; }

export function room_dispatch(id: i32, p: usize, len: i32): i64 {
  const m = currentMod();
  if (m == null) return 0;
  switch (id - CALLBACK_BASE) {
    case 0: m.onTick(load<i32>(p)); break;
    case 1: m.onThingDied(new Actor(load<u64>(p)), actorOrNull(load<u64>(p + 8))); break;
    case 3: m.onLoad(load<i32>(p + 4)); break;
    case 4: m.onThingSpawned(new Actor(load<u64>(p))); break;
    case 5: m.onThingDamaged(new Actor(load<u64>(p)), actorOrNull(load<u64>(p + 8)), actorOrNull(load<u64>(p + 16)), load<i32>(p + 24), load<i32>(p + 28)); break;
    case 6: m.onPlayerEnter(new Actor(load<u64>(p)), load<i32>(p + 8) - 1); break;
    case 7: m.onPlayerDied(new Actor(load<u64>(p)), load<i32>(p + 8) - 1); break;
    case 8: m.onUnload(); break;
    case 9: m.onItemPickup(new Actor(load<u64>(p)), className(load<i32>(p + 8))); break;
    default: break;
  }
  return 0;
}
