"""Desktop / educational Python RoomMod (mirrors testdata/mods/hello_as.ts).

Desktop: packed as HELLO lump + ROOMMOD with language = \"python\", loaded by
`room-app --features python-mods`.

Educational REPL (no WAD):

    ./scripts/setup-python-edu.sh
    python -m room mods/doom1-python-hello/hello_mod.py --skill 4
"""

from __future__ import annotations

from typing import Optional

from room.sdk import Actor, ModBase, TICRATE


class Hello(ModBase):
    """Greet on load, spawn an Imp at 3 s, heal the player on monster kills."""

    def __init__(self) -> None:
        super().__init__("HelloPython")
        self.kills = 0
        self._spawned = False

    def register(self) -> None:
        # WASM mods call defineClass("GhostImp", "DoomImp") here.
        return None

    def on_load(self, maptime: int) -> None:
        self.print(f"Hello from Python! skill {self.skill()}")

    def on_tick(self, t: int) -> None:
        if self._spawned or t < 3 * TICRATE:
            return
        self._spawned = True
        p = self.player(0)
        if p is None:
            return
        aid = self.spawn("DoomImp", p.x + 256.0, p.y, angle=180.0)
        if aid is not None:
            self.print(f"Spawned DoomImp id={aid}")

    def on_thing_died(self, thing: Actor, killer: Optional[Actor]) -> None:
        if killer is None or not killer.is_player or not thing.is_monster:
            return
        self.kills += 1
        if self._session is not None:
            hp = self._session.player_health() or 0
            self._session.set_player_health(min(200, hp + 10))
        self.print(f"Kill #{self.kills} ({thing.class_name}): +10 health")


def create() -> Hello:
    return Hello()


MOD = Hello()
