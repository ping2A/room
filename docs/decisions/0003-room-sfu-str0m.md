# 0003 — room-sfu uses str0m (sans-IO WebRTC)

## Context

`docs/22-room-server.md` §8 requires a pure-Rust SFU for Opus voice and VP8
video, with signalling on the existing game WebSocket. LiveKit is the documented
fallback if the chosen stack is insufficient.

## Decision

1. New crate `room-sfu` owns media sessions; `room-server` only routes `rtc`
   JSON and mute commands into it.
2. **RTP/DTLS stack:** [`str0m`](https://crates.io/crates/str0m) behind
   `room-sfu` feature `rtp`, with `default-features = false` + `rust-crypto`
   (avoid aws-lc C). UDP demux by ICE ufrag is sketched in `room-sfu::rtp`.
3. Phase 1: signalling + mute + TURN REST-style credentials (HMAC-SHA256
   username/credential in `welcome.media.ice_servers`). Point coturn at a
   matching auth hook, or replace with HMAC-SHA1 when integrating stock coturn.
4. Browser: `rtc.js` (`window.roomRtc`) for Opus PTT; PCM `voice.js` remains
   fallback.

## Consequences

- PCM `VOICE` / `VOICE_LEGACY` is deprecated once Opus WebRTC works in browsers.
- Operators size SFU egress separately from the lockstep WebSocket path.
