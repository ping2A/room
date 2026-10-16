# room-server

Production multi-room WebSocket server for Room web coop (`docs/22-room-server.md`).
Config is CLI flags; each flag also reads a matching `ROOM_*` environment variable.

## Quick start

```bash
# Game API + lobby + /play (room-web) on :8765, admin on :8766
./scripts/room-server.sh start
./scripts/room-server.sh status

open http://127.0.0.1:8765/        # public lobby
open http://127.0.0.1:8765/play/   # game client (same origin)
open http://127.0.0.1:8766/admin   # operator console
```

Lobby **Callsign** is required before Play / create — it becomes the in-game pseudo
(sanitized server-side, max 24 chars). **Play** (this tab) mints a join ticket and
navigates to `/play/?ws=…&ticket=…&name=…` on the same host. **New tab** opens another
client for a second seat. Lockstep starts only when every seat has joined and sent
Ready. Joining a pit someone else opened always asks for (or confirms) the callsign,
and duplicate names in one room become `name 2`, `name 3`, ….

**Spectate** opens `/play/?ws=…&spectate=1&name=…`: a seat-less client that replays the
match from tic 0 (fast-forward), then shows every player in a split-screen grid.
In game: **G** toggles grid / follow, **1–4** or click picks a player, **Tab** shows the
scoreboard (players also get Tab + the bottom-left kill feed).

### Production checklist

| Flag / setting | Why |
|----------------|-----|
| `--admin-token <long secret>` | Admin API/UI is open if empty |
| `--admin-bind` on a private interface | Keep ops off the public game bind |
| `--ticket-secret` ≥ 16 random bytes | Default is a known dev string |
| `--require-tickets` | Stops display-name spoofing without a mint |
| `--origin https://your.cdn` (repeatable) | Browser Origin allowlist |
| `--data-dir /var/lib/room` | Persist `bans.json` + `audit.jsonl` |
| TLS reverse proxy | See `deploy/Caddyfile` |

Bans (`POST /admin/api/v1/bans`, seat **Ban** in the console) are enforced on join mint,
WebSocket upgrade (IP), and v2 hello/resume. Scopes: `ip`, `name`, `name_regex` (`*`/`?`),
`fingerprint`, `ticket_id`, `turn_user`, `auth_key`, `resume_token`.

## Run (manual)

```bash
cargo run -p room-server --release --bin room-server -- \
  --bind 127.0.0.1:8765 \
  --admin-bind 127.0.0.1:8766 \
  --play-base http://127.0.0.1:8080 \
  --ticket-secret 'dev-secret' \
  --origin http://127.0.0.1:8080

# Public room directory
curl -s http://127.0.0.1:8765/api/rooms | jq .

# Create a room
curl -s -X POST http://127.0.0.1:8765/api/rooms \
  -H 'content-type: application/json' \
  -d '{"map":"E1M1","skill":3,"max_players":2,"visibility":"public"}'

# Mint a join ticket (v2)
curl -s -X POST http://127.0.0.1:8765/api/rooms/<id>/join \
  -H 'content-type: application/json' \
  -d '{"name":"alice"}'

# Daemon helper
./scripts/room-server.sh restart --admin-bind 127.0.0.1:8766 -- \
  --admin-token secret --play-base http://127.0.0.1:8080
./scripts/room-server.sh stop

# Browser coop helpers
./scripts/play-web-coop.sh --server --no-open
./scripts/play-web-coop.sh --v2 --no-open
./scripts/smoke-room-server.sh
```

### Public surfaces (game bind)

| Path | Purpose |
|------|---------|
| `/` or `/lobby` | Public lobby UI |
| `/api/lobby` | `{ play_base, node_id, draining }` |
| `/api/rooms` | `GET` public directory · `POST` create |
| `/api/rooms/{id}/join` | Mint join ticket |
| `/api/mods` | Allowed mods `[{name, title, fingerprint, size, url}]` |
| `/api/mods/{name}.wad` | Mod WAD bytes (CORS `*`, `ETag` = fingerprint) |
| `/v2/rooms/{id}/ws` | Protocol v2 WebSocket |
| `/ws/{id}` | Legacy FTRL gateway |
| `/healthz` `/readyz` `/metrics` | Ops |

### Admin surfaces (`--admin-bind`, or same bind if unset)

| Path | Purpose |
|------|---------|
| `/admin` | Operator console |
| `/admin/api/v1/*` | Summary, rooms, kick/mute/notice, ban-from-seat, bans CRUD, mods allow/disallow, audit, drain |

### CLI flags

| Flag | Default | Meaning |
|------|---------|---------|
| `--bind` | `127.0.0.1:8765` | Game/API/lobby listen address |
| `--admin-bind` | _(empty = same as `--bind`)_ | Admin UI/API listen address |
| `--play-base` | `http://127.0.0.1:8080` (or `http://<bind>/play` when `--web-root` is set) | Browser client origin for lobby Play links |
| `--web-root` | _(empty)_ | Serve built `room-web` at `/play/` |
| `--node-id` | `local` | Multi-node `room_location` id |
| `--max-rooms` | `5000` | Node capacity |
| `--ticket-secret` | `room-dev-ticket-secret` | HMAC key material |
| `--origin` | _(none)_ | Allowed browser Origin (repeatable); empty = allow any |
| `--require-tickets` | off | Reject v2 hello without a minted ticket |
| `--stall-fill-ms` | `30` | Jitter grace past a tic's 35 Hz slot before a missing seat is filled (last cmd repeated 3 tics, then neutral) |
| `--max-conn-per-ip` | `16` | Concurrent WS per client IP (`0` = unlimited) |
| `--admin-token` | _(empty)_ | Bearer token for `/admin/*` (empty = open/dev) |
| `--data-dir` | _(empty)_ | Persist `audit.jsonl` + `bans.json` |
| `--stun-url` | _(auto)_ | WebRTC STUN in `welcome.media` |
| `--stun-bind` | _(off)_ | Built-in Lightcall STUN (e.g. `0.0.0.0:3478`) |
| `--turn-bind` / `--turn-public-ip` | _(off)_ | Built-in Lightcall TURN + per-seat long-term creds |
| `--turn-host` / `--turn-secret` | _(off)_ | External coturn REST short-lived creds |
| `--sfu-udp` | _(off)_ | In-process SFU UDP bind (`--features rtp`) |
| `--enable-verifier` | off | Allow `enable_verifier` on create |
| `--max-verified-rooms` | `200` | Cap verified rooms |
| `--iwad` | _(empty)_ | IWAD path for Headless verifier (`--features verifier`) |
| `--mods-dir` | _(empty)_ | Directory of mod WADs (`ROOMMOD` lump) offered to rooms; `POST /api/rooms` with `"mod": "<file stem>"` |
| `--allow-mods` | _(none)_ | First-run allow list (`a,b` or `*`) before `<data-dir>/mods.json` exists; afterwards admins allow mods in Settings → Mods |
| `--drain-grace-secs` | `60` | Delay after drain notice before force-close |

TLS: put Caddy (or nginx) in front — see `deploy/Caddyfile`.

Features: `legacy-ftrl` (default), `rtp` (str0m SFU), `verifier` (Headless).

## Load / soak / fuzz

```bash
cargo run -p room-server --release --bin room-load -- \
  --rooms 5000 --players 2 --tics 5 --concurrency 128 --max-rooms 5500 --sample-rss

./scripts/soak-room-server.sh --hours 1 --rooms 32
./scripts/fuzz-room-proto.sh --seconds 3600
```

Drain: `POST /admin/api/v1/maintenance` with `{"enabled":true,"message":"…"}`.

## Status

- **S1–S5 done** (see `docs/21-milestone-progress.md`)
- Public lobby UI + `GET /api/rooms` directory
- Admin operator console on `--admin-bind`
