# room-signal

Optional in-memory signalling relay for Room netplay (dev / CI).

```bash
cargo run -p room-signal -- --bind 127.0.0.1:3535
```

- `POST /room/{id}/join` → `{"peer":"p0"}`
- `POST /room/{id}/signal` body `{"from","to","payload"}`
- `GET /room/{id}/poll?peer=p0` → queued messages
- `GET /health`

Used by `room-net::HttpSignalBus` / `SignalMeshTransport`. Optional
`--features webrtc` uses `matchbox_socket` against a WebSocket matchbox server
instead; this binary remains the HTTP room helper for CI and desktop smoke.
