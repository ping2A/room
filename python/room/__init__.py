"""Educational Room Python package.

Write a :class:`room.sdk.ModBase` / ``RoomMod`` the same way you would author a
WASM play module, then drive it with :func:`room.host.run_mod`.

Production netplay mods still ship as ``.wasm`` inside a PWAD — see
``docs/12-wasm-scripting.md`` and ``mods/doom1-bounty``.
"""

from __future__ import annotations

from room.sdk import Actor, ModBase, RoomMod, TICRATE, event_mask

__all__ = [
    "Actor",
    "ModBase",
    "RoomMod",
    "Session",
    "TICRATE",
    "event_mask",
    "iwad_exists",
    "run_mod",
]


def _ext():
    try:
        from room import _native as native_mod
    except ImportError as exc:  # pragma: no cover
        raise ImportError(
            "room._native is not built. From the repo root run:\n"
            "  ./scripts/setup-python-edu.sh\n"
            f"(underlying error: {exc})"
        ) from exc
    return native_mod


def __getattr__(name: str):
    if name == "Session":
        return _ext().Session
    if name == "iwad_exists":
        return _ext().iwad_exists
    if name == "run_mod":
        from room.host import run_mod

        return run_mod
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


def __dir__():
    return sorted(__all__)
