# 38 — Network state

What Room's multiplayer does **today**, path by path, and what it does not do yet.
The design is in `17-netplay-demos-saves.md` (lockstep model, demos, saves) and
`22-room-server.md` (web server, protocol v2, security). This file is the status
sheet: when a network change lands, update the matching section here and add a
line to §11 in the same change.

Last updated: 2026-11-09 (UZ12 + web prediction).

## 1. At a glance

| Path | Crates | Topology | Seats | Input delay | Stalls | Prediction | Desync check |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Native UDP | `room-app`, `room-net::UdpTransport` | peer-to-peer mesh | 2–4 | `ROOM_NET_DELAY` (1–8, default 1) | blocks, error after 3 s | `cl_predict 1` (off by default) | none for Doom (§8) |
| Native TCP mesh | `room-net::TcpTransport` | peer-to-peer mesh | 2–4 (app clamp; transport allows 8) | same | same | same | none for Doom |
| Native TCP relay | `room-net::{TcpRelayServer, TcpRelayClient}` | host relays (packet server) | 2–64 | same | same | same | none for Doom |
| Native Duke | `room-app` + `room-duke` | any of the above | 2–8 | 1 | same | no | yes (`CS` extension, every peer) |
| Web, legacy `FTRL` | `room-web`, `room-ws` | WebSocket relay | 2–8 | 0 | rebroadcast every 2 frames; fill after ~30 s, or 45 frames once a peer has left | on (`?predict=0` off) | none |
| Web, protocol v2/v3 | `room-web`, `room-server`, `room-proto` | server-authoritative 35 Hz clock | 2–64 (Doom), 2–8 (Duke) | streamed `ceil((RTT+15 ms)/tic)` ahead, 1–10 tics | server repeats a late seat's command 3 tics, then neutral | on (`?predict=0` off) | server-side only when the verifier runs; clients send no CHECKSUM (§8) |
| Spectators (v2) | `room-server::room::spectate` | server streams the frame log | ≤16 per room | — | catch-up fast-forward when >35 tics behind | — | — |

Optional encryption: native `--noise` (Noise_XX, all native transports).

## 2. Seats and the playsim

- `room_sim::MAX_PLAYERS` = **64** (UZDoom `MAXPLAYERS`). `World::players` is a
  64-slot `Vec`; frags, bot seats and `PowerTimeFreezer` masks cover 64 seats.
- Spawn spots: Player 1–4 starts, then ZDoom Player 5–8 starts (4001–4004), then
  deathmatch starts, then free floor on rings around Player 1's start (walkable
  from it, clear of walls and pawns). Seats past 8 respawn on Player 1's start.
- **Hash compatibility:** `World::checksum` always covers seats 0–7 and covers a
  seat ≥ 8 only while it is in the game, so games of ≤ 8 players hash exactly as
  before UZ12 (committed demo and checksum tests are unchanged).
- Players past 8 have no colour of their own.

## 3. Wire protocols

### 3.1 Native lockstep (`room-net`)

- Lobby `FNHS`: `Hello` (engine version, `ABI_MAJOR`, load-order fingerprint,
  name) → `Welcome` / `Start` (seat, peer count, RNG seed, map, skill).
- In game: `encode_timed_cmd` (tic, peer, 17-byte `NetTicCmd`), optional checksum
  extension, bounded auxiliary packets (`FRAU`, presentation only, e.g. Duke RTS).
- Every peer seeds neutral commands for tics `0..delay`, so all peers **must** use
  the same `ROOM_NET_DELAY`; it is not negotiated in the lobby yet.

### 3.2 Web protocol v2 / v3 (`room-proto`, `FTR2` framing)

`room_proto::PROTOCOL_VERSION` = **3**. The message table is in `22 §4`; v3 adds:

| Type | Name | Dir | Payload |
| --- | --- | --- | --- |
| 0x16 | TIC\_FRAME\_WIDE | S→C | `u32 tic`, `u64 seat_mask`, one `NetTicCmd` per set bit (≤ 1100 bytes) |
| 0x17 | TIC\_BATCH\_WIDE | S→C | `u16 count`, then wide frame bodies |

A frame whose seats all fit in 8 bits is still sent as v2 `TIC_FRAME` (0x11), so
rooms of ≤ 8 seats send exactly the v2 bytes and older clients keep working. A
room never mixes layouts; spectator logs follow the room's layout.

### 3.3 Legacy web `FTRL` (`room-ws`)

`MSG_HELLO / WELCOME / DATA / REJECT / VOICE / PEER_JOIN / PEER_LEAVE`; lockstep
packets ride in `MSG_DATA`. Up to 8 seats. Kept for compatibility; new work goes
to v2/v3.

## 4. Input timing

- **Native:** `NetPlay::advance_tic` submits the local command for
  `local_tic + delay`, rebroadcasts every 50 ms, and blocks until the full frame
  for the next tic arrives (error "lockstep stall" after 3 s).
- **Web v2:** the server releases tic N no earlier than `start + N/35 s` and as soon
  as every seat has sent it; a seat late by more than 30 ms gets its last command
  repeated (weapon change cleared) for 3 tics, then a neutral one. The client
  pings every 500 ms and streams input `ceil((RTT + 15 ms)/28.6 ms)` tics ahead of
  the newest frame (1–10), at most 4 commands per frame.
- **Web legacy:** delay 0, one outstanding command, rebroadcast every 2 frames.

## 5. Client prediction

Implemented in `room-sim/src/predict.rs` (`World::predict_player_view`). It takes
the **confirmed** world and the local commands that were sent but not yet
released, walks a copy of the local pawn through them with `move_player`'s
thrust rules (float and vanilla fixed-point modes) and the read-only
`check_position` (walls, solid things, 24-unit steps), and returns the eye
position, yaw and pitch. The world is never modified, so prediction cannot cause
a desync and turning it off is bit-identical. Each frame starts again from the
confirmed pawn, so a wrong guess is corrected by the next confirmed tic.

| Client | Pending commands from | Default | Toggle |
| --- | --- | --- | --- |
| Native (`room-app`) | `LockstepSession::pending_local` (`NetPlay::pending_local_cmds`) | off | console `cl_predict 1` |
| Web v2 | commands sent but not yet in a frame (`NetPlay::unapplied_cmds`) | on | URL `?predict=0` |
| Web legacy | the one outstanding command (`pending_local`) | on | URL `?predict=0` |

On the web the predicted offset is applied on top of the interpolated confirmed
eye, plus mouse movement not yet sent. With prediction off, the web client still
previews unsent look (yaw / pitch), as before.

Not predicted (the confirmed view is drawn instead): dead or spectating players,
`ChangeCamera` views, frozen / no-clip / flying players, time freeze, pause, the
Duke pawn. Within a prediction: no jumping, no pushers / conveyors / scrollers, no
doors or lifts moving, no being pushed by monsters or other players, no powerup
speed changes.

## 6. Intermission

- **Native netplay:** ready-vote (`Headless::intermission_vote`). A seat is ready
  once it presses use or fire (fire still held from the level does not count;
  bots are ready). The vote is read from the lockstep commands, so every peer
  leaves on the same tic. The screen shows `READY n/m`.
- **Web:** leaves after a fixed 1.5 s of local frames. Lockstep tics are not
  consumed during the intermission, so peers stay in sync, but there is no vote.

## 7. Saves

- **Net saves (native):** only the host can `save` / `load`; other seats get "Only
  the host can save a net game". The request rides in the host's next command
  (`Buttons::NETSAVE` / `NETLOAD`, slot in `weapon_select`, stripped before the
  playsim) and every peer saves or loads its own `net<N>` copy on that tic.
  A peer without the host's `net<N>` file desyncs on load.
- Saves carry every seat (`players.bin`); single-player saves are unchanged.
- Web netplay has no save / load.

## 8. Desync detection

| Path | State |
| --- | --- |
| Native Doom | `LockstepSession` can compare checksums, but Doom netplay sends none (`advance_tic` passes `None`): a desync goes unnoticed. |
| Native Duke | Full `DukeSession` checksum in every input packet; compared once every peer reported the tic. |
| Web v2 | The server compares client CHECKSUM messages (and the verifier's) and broadcasts `desync`, but `room-web` never sends CHECKSUM, so only the verifier's own run is checked. |
| Web legacy | None. |

## 9. Known gaps / next steps

1. Doom clients (native and web) should send a world checksum every 35 tics.
2. Negotiate the native input delay in the lobby (`Start`) instead of `ROOM_NET_DELAY`.
3. Web intermission ready-vote and web net saves.
4. Prediction of jumps, pushers and moving floors; smoothing of the predicted
   offset between tics on the web.
5. Seat colours past 8; per-seat respawn spots past 8.
6. Legacy `room-ws` and the native mesh transports stay at 8 / 4 seats.
7. Five `room-server` integration test files (`admin_api`, `security`,
   `multi_room_legacy`, `public_lobby`, `drain_and_location`) do not compile:
   they predate `CreateRoomRequest.duke_game_mode` / `duke_friendly_fire`.

## 10. Tests

| Area | Tests |
| --- | --- |
| 64 seats | `room-game/tests/many_players.rs` (16-seat soak, deterministic; 64-seat spawn) |
| Protocol v3 | `room-proto` (`narrow_frames_keep_the_v2_bytes…`, `v3_frames_and_batches_round_trip…`), `room-server` `sixty_four_seats_release_a_full_wide_frame`, `a_64_seat_room_logs_and_replays_wide_frames` |
| Prediction | `room-sim` `predict::tests` (matches the lockstep, stops at walls, world untouched); `room-net` `pending_local_lists_unreleased_local_input_oldest_first` |
| Ready-vote, net saves | `room-game` `intermission_waits_for_every_seat_to_press_use_or_fire`, `net_saves_keep_every_seat_and_the_request_never_reaches_the_playsim` |
| Lockstep | `room-game/tests/coop_netplay.rs`, `room-net` unit tests, `room-server` `checksum_desync`, `spectate` |

## 11. Change log

- 2026-11-09 — Web client prediction (v2 and legacy, on by default); lobby and admin
  "Max players" fields accept 64.
- 2026-11-09 — UZ12: 64 seats, protocol v3 wide frames, native client prediction
  (`cl_predict`), `ROOM_NET_DELAY`, intermission ready-vote, host-only net saves,
  saves carry every seat.
