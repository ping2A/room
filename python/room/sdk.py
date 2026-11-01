"""Python mirror of the WASM guest SDK (`docs/12` / AssemblyScript ``room.ts``).

This is the **educational** surface: same events and vocabulary as a real
RoomMod, but callbacks run in CPython against a headless :class:`Session`.
Rewrite the same logic in Rust (``room-script-sdk``) or AssemblyScript for a
netplay-safe ``.wasm`` PWAD.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Optional, Protocol

# Doom tic rate (same constant as the WASM SDK).
TICRATE = 35


class event_mask:
    """Bit names matching ``room_script_sdk::game::event_mask`` (documentation)."""

    WORLD_LOADED = 1 << 0
    WORLD_TICK = 1 << 1
    WORLD_THING_DIED = 1 << 2
    WORLD_THING_DAMAGED = 1 << 3
    WORLD_UNLOADED = 1 << 4


@dataclass
class Actor:
    """Lightweight actor handle (snapshot fields from the native session)."""

    id: int
    class_name: str
    x: float
    y: float
    z: float
    angle: float
    health: int
    is_player: bool
    is_monster: bool

    @classmethod
    def from_view(cls, view: Any) -> "Actor":
        return cls(
            id=int(view.id),
            class_name=str(view.class_name),
            x=float(view.x),
            y=float(view.y),
            z=float(view.z),
            angle=float(view.angle),
            health=int(view.health),
            is_player=bool(view.is_player),
            is_monster=bool(view.is_monster),
        )


class RoomMod(Protocol):
    """Override the hooks you need — same names as AssemblyScript ``RoomMod``."""

    name: str

    def register(self) -> None:
        """Declare mod classes (educational host: no-op unless you extend it)."""

    def on_load(self, maptime: int) -> None: ...

    def on_tick(self, t: int) -> None: ...

    def on_thing_died(self, thing: Actor, killer: Optional[Actor]) -> None: ...

    def on_unload(self) -> None: ...


class ModBase:
    """Convenient base class (optional — Protocol alone is enough for typing)."""

    name: str = "PythonMod"

    def __init__(self, name: str | None = None) -> None:
        if name:
            self.name = name
        self._session: Any = None

    def bind(self, session: Any) -> None:
        self._session = session

    def print(self, text: str) -> None:
        if self._session is not None:
            self._session.mod_print(text)

    def skill(self) -> int:
        # Skill is fixed at session open; expose via attribute if set by host.
        return int(getattr(self, "_skill", 3))

    def player(self, seat: int = 0) -> Optional[Actor]:
        if self._session is None or seat != 0:
            return None
        pose = self._session.player_pose()
        hp = self._session.player_health()
        if pose is None or hp is None:
            return None
        x, y, z, ang = pose
        return Actor(
            id=0,
            class_name="Player",
            x=x,
            y=y,
            z=z,
            angle=ang,
            health=hp,
            is_player=True,
            is_monster=False,
        )

    def spawn(
        self,
        class_name: str,
        x: float | None = None,
        y: float | None = None,
        angle: float = 180.0,
    ) -> Optional[int]:
        if self._session is None:
            return None
        return self._session.spawn(class_name, x, y, angle)

    def register(self) -> None:
        return None

    def on_load(self, maptime: int) -> None:
        return None

    def on_tick(self, t: int) -> None:
        return None

    def on_thing_died(self, thing: Actor, killer: Optional[Actor]) -> None:
        return None

    def on_unload(self) -> None:
        return None
