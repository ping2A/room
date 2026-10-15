# room-sfu

Selective Forwarding Unit + ICE helpers for Room web media (`docs/22-room-server.md` §8).

## Features

| Feature | Purpose |
|---|---|
| _(default)_ | Signalling / mute roster; coturn REST credential minting |
| `rtp` | In-process str0m UDP SFU demux |
| `builtin-turn` | Lightcall STUN + TURN relay (`turn` / webrtc-rs) |

## Built-in STUN/TURN (from Lightcall)

```rust
// STUN (always)
room_sfu::run_stun("0.0.0.0:3478".parse()?).await?;

// TURN (feature `builtin-turn`)
room_sfu::run_turn_relay(room_sfu::TurnRelayConfig { … }).await?;
```

`room-server` exposes `--stun-bind` / `--turn-bind` / `--turn-public-ip` and
issues per-seat long-term credentials in `welcome.media.ice_servers`.

## Signalling API

```rust
use room_sfu::{MediaSession, SignalIn, SignalOut};

let mut session = MediaSession::new("room-id");
let outs = session.handle(SignalIn::Offer { seat: 0, sdp: "…" })?;
```
