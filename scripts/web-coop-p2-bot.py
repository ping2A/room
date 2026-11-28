#!/usr/bin/env python3
"""Minimal seat-1 lockstep peer for web coop debugging (idle cmds)."""
from __future__ import annotations

import argparse
import struct
import sys
import time

try:
    from websockets.sync.client import connect
except ImportError:
    print("pip install websockets", file=sys.stderr)
    sys.exit(1)

FTRL = b"FTRL"
FNHS = b"FNHS"
MSG_HELLO, MSG_WELCOME, MSG_DATA = 1, 2, 3
ENGINE = b"0.1.0"
ABI = 1


def pstr(s: bytes) -> bytes:
    return struct.pack("<H", len(s)) + s


def pack_relay_hello(name: str) -> bytes:
    b = name.encode()[:64]
    return FTRL + bytes([MSG_HELLO]) + struct.pack("<H", len(b)) + b


def pack_data(payload: bytes) -> bytes:
    return FTRL + bytes([MSG_DATA]) + payload


def lobby_hello(fingerprint: int, name: str) -> bytes:
    return (
        FNHS
        + bytes([1])
        + pstr(ENGINE)
        + struct.pack("<i", ABI)
        + struct.pack("<Q", fingerprint & 0xFFFFFFFFFFFFFFFF)
        + pstr(name.encode())
    )


def lobby_ready(peer: int) -> bytes:
    return FNHS + bytes([5, peer & 0xFF])


def timed_cmd(tic: int, peer: int, forward: int = 0) -> bytes:
    # ver u8, peer u8, tic u32, buttons u32, pitch/yaw/roll/fwd/side/up i16, weapon u8
    return (
        bytes([1, peer & 0xFF])
        + struct.pack("<I", tic)
        + struct.pack("<I", 0)
        + struct.pack("<hhhhhh", 0, 0, 0, forward, 0, 0)
        + bytes([0])
    )


def parse_relay(data: bytes):
    if len(data) < 5 or data[:4] != FTRL:
        return None, b""
    return data[4], data[5:]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--url", default="ws://127.0.0.1:8765")
    ap.add_argument("--name", default="p2bot")
    ap.add_argument("--fingerprint", type=lambda x: int(x, 0), required=True)
    ap.add_argument("--seconds", type=float, default=120)
    args = ap.parse_args()

    print(f"bot: connect {args.url} fp={args.fingerprint:016x}", flush=True)
    with connect(args.url, max_size=2**22) as ws:
        ws.send(pack_relay_hello(args.name))
        seat = None
        started = False
        peers_ready = False
        next_send = 0
        last_ready = 0.0
        end = time.time() + args.seconds
        while time.time() < end:
            try:
                msg = ws.recv(timeout=0.05)
            except TimeoutError:
                if started and seat is not None and not peers_ready:
                    if time.time() - last_ready >= 0.25:
                        ws.send(pack_data(lobby_ready(seat)))
                        last_ready = time.time()
                elif peers_ready and seat is not None:
                    ws.send(pack_data(timed_cmd(next_send, seat)))
                continue
            if isinstance(msg, str):
                continue
            mtype, rest = parse_relay(msg)
            if mtype == MSG_WELCOME and len(rest) >= 2:
                seat = rest[0]
                print(f"bot: relay seat {seat}", flush=True)
                ws.send(pack_data(lobby_hello(args.fingerprint, args.name)))
            elif mtype == MSG_DATA and rest:
                payload = rest[1:]
                if payload.startswith(FNHS) and len(payload) >= 5:
                    kind = payload[4]
                    if kind == 2:
                        print("bot: lobby Welcome", flush=True)
                    elif kind == 3:
                        # Reject — read reason
                        if len(payload) >= 7:
                            n = struct.unpack_from("<H", payload, 5)[0]
                            reason = payload[7 : 7 + n].decode("utf-8", "replace")
                        else:
                            reason = "?"
                        print(f"bot: REJECT {reason}", flush=True)
                        return 1
                    elif kind == 4:
                        started = True
                        next_send = 0
                        print("bot: Start — announcing Ready", flush=True)
                        ws.send(pack_data(lobby_ready(seat or 1)))
                        last_ready = time.time()
                    elif kind == 5:
                        # Host Ready — we already sent ours; begin lockstep.
                        if started and not peers_ready:
                            peers_ready = True
                            print("bot: peers Ready — sending idle lockstep", flush=True)
                            ws.send(pack_data(timed_cmd(0, seat or 1)))
                            next_send = 1
                elif peers_ready and len(payload) >= 23 and payload[0] == 1:
                    peer = payload[1]
                    tic = struct.unpack_from("<I", payload, 2)[0]
                    if peer != (seat or 1):
                        while next_send <= tic + 1:
                            ws.send(pack_data(timed_cmd(next_send, seat or 1)))
                            next_send += 1
        print("bot: done", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
