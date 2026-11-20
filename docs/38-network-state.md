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
| Native UDP | `room-app`, `room-net::UdpTransport` | peer-to-peer mesh | 2–4 | picked by the host from measured ping (`delay_for_rtts`), sent in Start; `ROOM_NET_DELAY` (1–8) forces it | blocks; resends the recent command window every 50 ms; error after 3 s | `cl_predict` (on by default) | world checksum every 35 tics (§8) |
| Native TCP mesh | `room-net::TcpTransport` | peer-to-peer mesh | 2–4 (app clamp; transport allows 8) | same | same | same | same |
| Native TCP relay | `room-net::{TcpRelayServer, TcpRelayClient}` | host relays (packet server) | 2–64 | same | same | same | same |
| Native Duke | `room-app` + `room-duke` | any of the above | 2–8 | 1 | same | no | yes (`CS` extension, every peer) |
| Web, legacy `FTRL` | `room-web`, `room-ws` | WebSocket relay | 2–8 | 0 | rebroadcast every 2 frames; fill after ~30 s, or 45 frames once a peer has left | on (`?predict=0` off) | none |
| Web, protocol v2/v3 | `room-web`, `room-server`, `room-proto` | server-authoritative 35 Hz clock | 2–64 (Doom), 2–8 (Duke) | streamed `ceil((RTT+15 ms)/tic)` ahead, 1–10 tics | server repeats a late seat's command 3 tics, then neutral | on (`?predict=0` off) | clients send CHECKSUM every 35 tics; the server compares seats (and the verifier) (§8) |
| Spectators (v2) | `room-server::room::spectate` | server streams the frame log | ≤16 per room | — | catch-up fast-forward when >35 tics behind | — | — |

Optional encryption: native `--noise` (Noise_XX, all native transports).

## 2. Seats and the playsim

- `room_sim::MAX_PLAYERS` = **64** (UZDoom `MAXPLAYERS`). `World::players` is a
  64-slot `Vec`; frags, bot seats and `PowerTimeFreezer` masks cover 64 seats.
- Spawn spots: Player 1–4 starts, then ZDoom Player 5–8 starts (4001–4004), then
  deathmatch starts, then free floor on rings around Player 1's start (walkable
  from it, clear of walls and pawns). Seats past 8 respawn on the free player
  starts in turn (seat 8 on the first free one, seat 9 on the next, …) instead
  of all on Player 1's (since 2026-11-18, `World::respawn_spot`).
- **Hash compatibility:** `World::checksum` always covers seats 0–7 and covers a
  seat ≥ 8 only while it is in the game, so games of ≤ 8 players hash exactly as
  before UZ12 (committed demo and checksum tests are unchanged).
- **Seat colours (since 2026-11-18).** Each seat's pawn is drawn in its colour:
  green, indigo, brown, red, blue, orange, tan, gray, then round again for seats
  past 8 (`room_render::player_colors`, Doom's 112–127 ramp moved onto another
  ramp; textures are named `<lump>@P<n>`). Desktop and browser both draw them.
- **Deathmatch and bots on the desktop (since 2026-11-18).** The host passes
  `--deathmatch` and/or `--bots N`; the lobby's `Welcome` carries both (two
  trailing bytes, read as 0 from older hosts), so joiners start the same game
  without passing them. Bots take the seats after the humans.

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
| Native (`room-app`) | `LockstepSession::pending_local` (`NetPlay::pending_local_cmds`) | on (since 2026-11-18) | console `cl_predict 0` |
| Web v2 | commands sent but not yet in a frame (`NetPlay::unapplied_cmds`) | on | URL `?predict=0` |
| Web legacy | the one outstanding command (`pending_local`) | on | URL `?predict=0` |

On the web the predicted offset is applied on top of the interpolated confirmed
eye, plus mouse movement not yet sent. When a confirmed frame shows a guess was
wrong (the predicted eye jumps more than 12 units), the eye stays where it was
drawn and the difference eases out with a 35 ms half-life
(`play.rs` `PredictSmoothing`); jumps over 128 units (teleports) snap. With prediction off, the web client still
previews unsent look (yaw / pitch), as before.

Not predicted (the confirmed view is drawn instead): dead or spectating players,
`ChangeCamera` views, frozen / no-clip / flying players, time freeze, pause, the
Duke pawn. Since 2026-11-18 a prediction follows jumps (`JumpZ`, high-jump
powerup, `no_jump`), gravity, falling off ledges and ceilings, out of water;
floors are read where the confirmed world has them, so a moving lift is followed
one confirmed tic late. Still not predicted: swimming height, pushers /
conveyors / scrollers, doors opening, being pushed by monsters or other players,
powerup speed changes.

### 5.1 Rollback (`cl_rollback`, native Doom, since 2026-11-18)

Prediction moves only the local view. Rollback runs the **whole game** ahead:
other players, monsters, projectiles, doors.

- **Two games.** The confirmed game advances only on frames that carry every
  seat's real command, exactly as in lockstep. It is what is checksummed (§8)
  and saved, and what plays sound and effects. Every local tic, a copy of it
  (`World::snapshot`, `Headless::speculate`) runs through the unconfirmed
  frames, `LockstepSession::speculative_frames`. Those frames use our own real
  commands. A peer whose command has not arrived is guessed to repeat its
  newest known one. The view, sprites and HUD-free world drawing come from that
  copy (`Headless::swap_world`, only while drawing).
- **Rebuilt every tic.** The copy is rebuilt from the confirmed game each local
  tic, so a wrong guess shows for at most one tic before the real commands
  replace it. No state from a copy ever flows back into the confirmed game.
- **No waiting.** `NetPlay::rollback_tic` sends this tic's command and returns
  whatever frames are complete; it never blocks. Only when our commands are a
  full window ahead (`cl_rollback N`, 1–16 tics) does it hold back, and it
  resends the recent commands while it does. A late packet no longer freezes
  the game; it shows as one wrong guess.
- **Per machine.** Each machine picks rollback on its own, with no lobby change.
  The wire format and the confirmed game are identical. Checksums are labelled
  with the number of frames applied, the same tic numbers lockstep uses.
- **Cost.** A world copy takes about 0.2 ms and a tic about 0.35 ms (Doom II
  MAP01, 4 seats, 290 actors). A full 8-tic window is about 3 ms per tic.
- **When it's off.** Off by default. It turns itself off when the game can't
  be copied: WASM play modules, WASM or native mods, CON events, or Duke. Then
  it falls back to lockstep with `cl_predict`. ZScript games can be copied
  since 2026-11-18 (the VM's compiled program is shared, `Arc`, so a copy is
  cheap).
- **HUD and own sounds (since 2026-11-18).** The HUD (health, ammo, weapon)
  comes from the copy. Sounds our own seat makes (weapon, pickups, pain) play
  from the newest copied tic (`Headless::speculate_with_own_sounds`), and the
  confirmed game skips them when it reaches that tic
  (`drain_audio_for_predicted`, matched by tic and sound name). Other seats'
  and monsters' sounds still play from the confirmed game.
- **Indicator.** The browser network graph shows a Rollback row
  (`N tics · K ahead`). The desktop has no indicator yet.
- **Limits.** Particles and other effects play from the confirmed game, so
  they lag by the round trip. A sound from a wrong guess has already played.

**Browser (protocol v2, since 2026-11-18).** Settings → Network → Rollback
netcode (off by default), with a look-ahead of 4, 8, 12 or 16 tics; the page
calls `roomSetRollback(n)`. The confirmed game is the server's frames, now run
as soon as they arrive, with no jitter buffer. The copy runs from the next
frame to our newest sent command, capped at the look-ahead. It uses our sent
commands (`NetPlay::rollback_frames`, `LockstepSession::speculative_frames_with`),
the server's frames that are here, and guesses for the other seats. It is
rebuilt whenever a frame arrives or a command goes out, and drawn for that
frame only (`PlaySession::begin_rollback_view` / `end_rollback_view` around
`draw_frame`). The eye glides between copies by the input clock. Prediction
is skipped while rollback draws (it is already ahead). The same page section
has a Prediction switch (`roomSetPredict`). It only overrides `?predict=0`
once the box is changed. Spectators and the legacy `FTRL` relay keep the old
view.

## 6. Intermission

- **Native netplay:** ready-vote (`Headless::intermission_vote`). A seat is ready
  once it presses use or fire (fire still held from the level does not count;
  bots are ready). The vote is read from the lockstep commands, so every peer
  leaves on the same tic. The screen shows `READY n/m`.
- **Web (since 2026-11-18):** the same vote. Lockstep frames keep flowing during
  the intermission and go through `Headless::tick`; the status line shows
  `ready n/m · next map in N s` and asks for E or a click. Single player still
  moves on after 1.5 s.
- **Time-out (both):** a vote ends by itself after 30 s of lockstep tics
  (`INTERMISSION_VOTE_TIMEOUT_TICS`), so a player away from the keyboard, or a
  seat that dropped and now sends neutral commands, can't hold everyone. The
  desktop screen shows the seconds left.

## 7. Saves

- **Net saves (native):** only the host can `save` / `load`; other seats get "Only
  the host can save a net game". The request rides in the host's next command
  (`Buttons::NETSAVE` / `NETLOAD`, slot in `weapon_select`, stripped before the
  playsim) and every peer saves or loads its own `net<N>` copy on that tic.
  A peer without the host's `net<N>` file desyncs on load.
- Saves carry every seat (`players.bin`); single-player saves are unchanged.
- **Web net saves (since 2026-11-18):** the host presses F6 (save) or F9 (load).
  The request rides in the host's command exactly as on desktop; on that tic
  every browser saves the match into memory and `localStorage`
  (`room/netsave/<seed>/<slot>`, base64) or loads it, changing map first if
  needed. Other seats get "Only the host can save or load a net game". A browser
  without the copy (cleared storage, a different machine) can't load; the
  checksum then reports the desync.

## 8. Desync detection

| Path | State |
| --- | --- |
| Native Doom | `NetPlay::advance_doom_tic` sends `World::checksum` (the world after every released frame) with the command every 35 lockstep tics. `LockstepSession::check_complete_desyncs` compares each tic once every peer reported it, even when a report arrives late; a mismatch prints `Network desync at tic N` on the HUD once and play goes on. |
| Native Duke | Full `DukeSession` checksum in every input packet; compared once every peer reported the tic (same sweep, same HUD warning). |
| Web v2 | `room-web` sends CHECKSUM every 35 tics (`NetPlay::report_world_checksum`, after the frame is simulated; spectators don't). The server compares the seats (and the verifier) and broadcasts `desync`; the client shows `Network desync at tic N: player … out of step` in the event feed once. |
| Web legacy | None. |

## 9. Known gaps / next steps

1. ~~Doom clients send a world checksum every 35 tics~~ (done 2026-11-18, native and web v2).
2. ~~Negotiate the native input delay~~ (done 2026-11-18). Once the lobby is full
   the host pings every seat (`LobbyMsg::Ping` / `Pong`, up to 1.5 s, four answers
   each), takes each seat's best round trip and picks the delay with
   `room_net::delay_for_rtts`: half the two slowest round trips plus 10 ms,
   in tics, 1–8 (LAN 1; two players 100 ms away 4). It goes out in
   `Start.delay` (type 6; type-4 Starts from older hosts still decode). The delay
   is fixed for the match; it does not adapt mid-game.
3. ~~Web intermission ready-vote and web net saves~~ (done 2026-11-18).
4. ~~Jumps~~ and ~~web correction smoothing~~ (done 2026-11-18). Pushers,
   conveyors and doors are still not predicted.
5. ~~Seat colours; respawn spots past 8~~ (done 2026-11-18).
5b. The input delay is fixed for the match; it does not adapt mid-game yet.
6. Legacy `room-ws` and the native mesh transports stay at 8 / 4 seats.
7. A real 30-minute session with four people (desktop and browser) has still not
   been played; the automated soak (§10) is the stand-in.

**Fixed on 2026-11-18:** a single lost UDP packet used to stall native netplay for
good. Only the newest command was ever resent, so a peer waiting on an older one
never got it. `LockstepSession::rebroadcast_recent` now resends the last
`2 × delay + 4` commands (8–64), each with its own checksum, and commands for
tics already released are ignored on receive. Web peer meshes use it too.

## 10. Tests

| Area | Tests |
| --- | --- |
| 64 seats | `room-game/tests/many_players.rs` (16-seat soak, deterministic; 64-seat spawn) |
| Protocol v3 | `room-proto` (`narrow_frames_keep_the_v2_bytes…`, `v3_frames_and_batches_round_trip…`), `room-server` `sixty_four_seats_release_a_full_wide_frame`, `a_64_seat_room_logs_and_replays_wide_frames` |
| Prediction | `room-sim` `predict::tests` (matches the lockstep, stops at walls, world untouched); `room-net` `pending_local_lists_unreleased_local_input_oldest_first` |
| Ready-vote, net saves | `room-game` `intermission_waits_for_every_seat_to_press_use_or_fire`, `an_idle_seat_cannot_hold_the_intermission_forever`, `net_saves_keep_every_seat_and_the_request_never_reaches_the_playsim`; `room-web` `base64_round_trips_every_length` (wasm build only) |
| Input delay | `room-net` `delay_grows_with_the_two_slowest_round_trips`, `start_with_delay_and_ping_pong_round_trip`; `room-app` `udp_lobby_negotiates_the_input_delay`, `tcp_relay_lobby_negotiates_the_input_delay` |
| Lockstep | `room-game/tests/coop_netplay.rs`, `room-net` unit tests (`rebroadcast_recovers_a_dropped_command`, `late_checksums_are_still_compared`), `room-server` `checksum_desync`, `spectate` |
| Soak (M12) | `room-game/tests/netplay_soak.rs`: four `Headless` games, each with its own `LockstepSession` (delay 2), over a simulated network with 0–6 steps of jitter (reordering) and packet loss, random human-like input (run, strafe, turn, fire, use, weapons). Checksums every 35 tics plus a final whole-world compare. Co-op (4 humans) and deathmatch (2 humans + 2 bots). Two minutes by default; `ROOM_SOAK_TICS=63000 ROOM_SOAK_DROP=100` runs 30 minutes at 10 % loss. 2026-11-18 run: 63 000 tics each, 1 800 checksums verified per seat, about 370 000 of 3.7 million packets dropped, no desync. A drill test shifts one peer's world and expects the desync to be reported within two checksum intervals. |
| Rollback | `room-net` `speculative_frames_with_take_our_commands_from_outside_the_buffer` (web v2 frames); `room-game/tests/rollback.rs` (a copy run through 1, 8 and 200 tics lands on exactly the real game's checksum; speculating leaves the game untouched); `netplay_soak.rs` `four_seat_*_rollback_soak*` (non-blocking peers, window 8; every speculation whose guesses all came true is checked against the confirmed game at that tic — about 7 200 per 2-minute run, all exact — and the four confirmed games end identical); `room-net` `speculative_frames_guess_missing_peers_from_their_last_command` |
| Seats, colours, rules | `room-sim` `seats_past_eight_share_the_free_player_starts`; `room-render` `player_colors::tests`; `room-net` `welcome_carries_rules_and_reads_old_hosts`; `room-app` netplay lobby test (joiner gets deathmatch and 2 bots); `room-sim/tests/zscript_play.rs` `a_world_snapshot_carries_the_zscript_vm` |
| Bots | `room-sim` `bot::tests` (co-op bots target monsters, deathmatch bots players; dead bots respawn; co-op bots follow the players) |

## 11. Change log

- 2026-11-18 (late night) — Rollback round 2: HUD and own sounds from the copy,
  ZScript games can be copied, browser rollback indicator. Desktop
  `--deathmatch` / `--bots` sent to joiners in `Welcome`. Seat colours.
  Respawn spots past 8.

- 2026-11-18 (night) — Browser rollback (Settings → Network) and a Prediction
  switch in Settings.
- 2026-11-18 (evening) — Rollback (`cl_rollback`, native Doom): `World: Clone`
  (`World::snapshot`), `Headless::speculate` / `swap_world`,
  `LockstepSession::speculative_frames` / `local_lead` / `set_rollback_window`,
  `NetPlay::rollback_tic`; rollback soaks.

- 2026-11-18 (later) — Ping-picked input delay, web intermission ready-vote,
  30 s vote time-out, web net saves (F6 / F9).

- 2026-11-18 — Doom desync detection (native and web v2), recent-window resend
  (fixes the lost-packet stall), host-negotiated input delay, `cl_predict` on by
  default, jump/fall prediction, web correction smoothing, co-op-aware bots, the
  four-seat soak.

- 2026-11-09 — Web client prediction (v2 and legacy, on by default); lobby and admin
  "Max players" fields accept 64.
- 2026-11-09 — UZ12: 64 seats, protocol v3 wide frames, native client prediction
  (`cl_predict`), `ROOM_NET_DELAY`, intermission ready-vote, host-only net saves,
  saves carry every seat.
