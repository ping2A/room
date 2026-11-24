# room-admin-tui

Live terminal admin console for [`room-server`](../room-server). Polls the same
`/admin/api/v1` surface as the web SPA and shows rooms, seats, metrics, bans,
audit, and mods.

```bash
# against the default admin bind from ./scripts/room-server.sh
cargo run -p room-admin-tui --bin room-admin -- \
  --url http://127.0.0.1:8766 \
  --user admin --password admin

# bearer token (when no MFA users exist)
cargo run -p room-admin-tui --bin room-admin -- \
  --url http://127.0.0.1:8766 \
  --token secret

./scripts/room-server.sh admin-tui

# non-interactive snapshot (CI / scripts)
cargo run -p room-admin-tui --bin room-admin -- --url http://127.0.0.1:8766 --once
```

Tabs: **1** Dash · **2** Net (relay histogram, rates, room lockstep) · **3** Rooms ·
**4** Detail (RTT / miss streak / minimap) · **5** Bans · **6** Audit · **7** Mods.

Keys: `Tab` / `1`–`7` · `↑`/`↓` select · `Enter` detail · `c` close · `n` notice ·
`k` kick · `r` refresh · `q` quit.
