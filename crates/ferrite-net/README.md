# ferrite-net

Lockstep netcode and transports (`docs/17-netplay-demos-saves.md`).

## Status (M12 done)

- `LockstepSession` — delay buffer, advance when all peers present
- `UdpTransport` — non-blocking UDP
- Lobby: `LobbyMsg` / `LobbyHost`
- `DesyncCollector` — checksum compare + text dumps
- In-process `LoopbackMesh` + `LossyTransport`
- `SignalMeshTransport` + `SignalBus` — full-mesh over signalling
  (`InMemorySignalBus` / `SharedSignalBus` / `HttpSignalBus` → `ferrite-signal`)
- Optional `--features webrtc` — `MatchboxTransport` over `matchbox_socket`
- Freedoom MAP01 2-peer lockstep smoke in `ferrite-game`
- Strong coop suite: `ferrite-game` `coop_netplay` (UDP lobby + dual-seat mesh,
  independent movement, FF block, desync detection); UDP lobby→lockstep unit test

```bash
cargo test -p ferrite-net --lib
cargo check -p ferrite-net --features webrtc
cargo test -p ferrite-game --test coop_netplay
cargo test -p ferrite-game --test freedoom_smoke freedoom2_map01_two_peer_lockstep_stays_synced
```
