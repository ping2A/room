# room-ws

WebSocket **packet-server relay** for browser (and cross-play) lockstep coop.

Speaks the same `FTRL` framing as TCP `--transport tcp-server`, one datagram per
WebSocket binary message. All seats are clients; seat `0` runs the lobby
arbitrator in the page.

```bash
# Terminal A — relay
cargo run -p room-ws -- --bind 127.0.0.1:8765 --players 2

# Terminal B — web shell (copies IWAD, opens browser)
./scripts/play-web-coop.sh
# Open a second tab: http://127.0.0.1:8080/?ws=ws://127.0.0.1:8765&players=2&name=p2
```
