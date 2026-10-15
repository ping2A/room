# 0002 — room-server stack (tokio / axum / FTR2)

## Context

`docs/22-room-server.md` specifies a multi-room production host to replace the
single-game `room-ws` relay. We need a pure-Rust async stack that does not pull
C/C++ (Appendix A) while remaining wire-compatible with today's `room-web`
clients during migration.

## Decision

1. **Runtime:** `tokio` multi-thread + `axum` (HTTP/1 + WebSocket only).
   **TLS (M2):** terminate at a reverse proxy (`deploy/Caddyfile`); the process
   speaks plain HTTP/WS on loopback and sets security headers itself. In-process
   rustls remains optional later if we need a single binary without a proxy.
2. **Protocol crate:** `room-proto` owns `FTR2` binary codecs and JSON control
   types; no I/O; shared by native server and (later) `room-web` wasm.
3. **Legacy path:** feature `legacy-ftrl` (default on) speaks today's `FTRL`
   framing on `/ws/{room_id}`, stamps the connection seat into `TimedCmd`, and
   fans out `PEER_JOIN` / `PEER_LEAVE`. Browser arbitration stays in the client
   until M2 tickets / server Start.
4. **v2 path:** `/v2/rooms/{id}/ws` with server-authoritative `TIC_CMD` →
   `TIC_FRAME` (seat from connection, never from payload).
5. **Room model:** one tokio task per room, bounded `mpsc` command channel,
   `DashMap` registry. No Redis in phase 1.

## Consequences

- `./scripts/play-web-coop.sh --server` creates a room via `POST /api/rooms`
  and points browsers at `/ws/{id}` (CLI flags, not env vars).
- `room-ws` remains for the simplest local two-player loop until legacy is
  removed.
- Join tickets use HMAC-SHA256 (`sha2`); enable enforcement with
  `--require-tickets`. Origin allowlist via repeated `--origin`.
- Load soak: `cargo run -p room-server --bin room-load -- --rooms 1000`.
- New workspace deps (`tokio`, `axum`, `dashmap`, `sha2`, …) are listed in
  Appendix A.
