"""CLI: ``python -m room`` or ``room-edu`` after install."""

from __future__ import annotations

import argparse
import importlib.util
import sys
from pathlib import Path


def _load_mod(path: Path):
    spec = importlib.util.spec_from_file_location("room_user_mod", path)
    if spec is None or spec.loader is None:
        raise SystemExit(f"cannot import {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    if hasattr(module, "create"):
        return module.create()
    if hasattr(module, "MOD"):
        return module.MOD
    raise SystemExit(f"{path} must define create() or MOD")


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description="Run an educational Room Python mod")
    p.add_argument(
        "mod",
        nargs="?",
        default=None,
        help="Path to hello_mod.py (default: mods/doom1-python-hello/hello_mod.py)",
    )
    p.add_argument("--skill", type=int, default=4)
    p.add_argument("--tics", type=int, default=35 * 5)
    p.add_argument("--iwad", default=None, help="Optional IWAD path + --map")
    p.add_argument("--map", default="E1M1")
    args = p.parse_args(argv)

    root = Path(__file__).resolve().parents[2]
    mod_path = Path(args.mod) if args.mod else root / "mods/doom1-python-hello/hello_mod.py"
    if not mod_path.is_file():
        print(f"mod not found: {mod_path}", file=sys.stderr)
        return 2

    from room import Session
    from room.host import run_mod

    if args.iwad:
        session = Session.from_iwad(args.iwad, args.map, args.skill)
    else:
        session = Session.arena(args.skill)

    mod = _load_mod(mod_path)
    print(f"room-edu: running {mod_path.name} for {args.tics} tics (skill {args.skill})")
    msgs = run_mod(session, mod, tics=args.tics, skill=args.skill, on_message=print)
    print(f"room-edu: done ({len(msgs)} messages, maptime={session.maptime})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
