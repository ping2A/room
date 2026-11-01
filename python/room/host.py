"""Run a Python ``RoomMod`` against a headless :class:`Session`."""

from __future__ import annotations

from typing import Any, Callable, Optional

from room.sdk import Actor, ModBase


def run_mod(
    session: Any,
    mod: ModBase,
    *,
    tics: int = 35 * 10,
    skill: int = 3,
    on_message: Optional[Callable[[str], None]] = None,
    tick_cmd: Optional[Callable[[int], tuple[float, float, bool]]] = None,
) -> list[str]:
    """Dispatch ``on_load`` / ``on_tick`` / ``on_thing_died`` like the WASM host.

    Returns every drained print/HUD line (including ``mod.print``).
    """
    mod.bind(session)
    mod._skill = skill  # noqa: SLF001 — educational mirror of skill()
    mod.register()
    mod.on_load(int(session.maptime))

    messages: list[str] = []

    def _emit(lines: list[str]) -> None:
        for line in lines:
            messages.append(line)
            if on_message is not None:
                on_message(line)

    _emit(session.drain_messages())

    for _ in range(max(0, tics)):
        fwd, side, attack = (0.0, 0.0, False)
        if tick_cmd is not None:
            fwd, side, attack = tick_cmd(int(session.maptime))
        session.tick(fwd, side, attack)
        t = int(session.maptime)
        mod.on_tick(t)
        killer = mod.player(0)
        for view in session.poll_deaths():
            thing = Actor.from_view(view)
            mod.on_thing_died(thing, killer)
        _emit(session.drain_messages())

    mod.on_unload()
    _emit(session.drain_messages())
    return messages
