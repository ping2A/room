#!/usr/bin/env bash
# Manage a local room-server daemon (docs/22).
#
# Prefer the repo entry point (same thing):
#   ./room start|stop|restart|status|mods|logs|health
#
#   ./scripts/room-server.sh start
#   ./scripts/room-server.sh stop
#   ./scripts/room-server.sh restart
#   ./scripts/room-server.sh status
#   ./scripts/room-server.sh mods      # only build the gameplay mods (mods/*)
#
# Extra server flags after `--`:
#   ./room start --bind 127.0.0.1:8765 -- \
#       --admin-token secret --origin http://127.0.0.1:8080
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

RUN_DIR="${ROOM_SERVER_RUN_DIR:-$ROOT/target/room-server}"
PID_FILE="$RUN_DIR/room-server.pid"
LOG_FILE="$RUN_DIR/room-server.log"
BIND_FILE="$RUN_DIR/bind"
ADMIN_BIND_FILE="$RUN_DIR/admin_bind"
BIN="$ROOT/target/release/room-server"

BIND="127.0.0.1:8765"
ADMIN_BIND="127.0.0.1:8766"
PLAY_BASE=""
WEB_ROOT="$ROOT/target/room-web"
BUILD=1
BUILD_WEB=1
EXTRA_ARGS=()

usage() {
  cat <<EOF
Usage: $0 <start|stop|restart|status|mods> [options] [-- server-flags...]

Commands:
  start     Build (unless --no-build) and run room-server in the background
  stop      Stop the background process (SIGTERM, then SIGKILL)
  restart   stop then start
  status    Print pid / health / admin UI
  admin-tui Live ratatui ops console (rooms, seats, metrics, bans, audit)
  mods      Build every mods/<dir> (Rust crate or AssemblyScript) into <module>.wad

Mods: start builds them too (unless --no-build) and serves them, but each one
stays disallowed until an admin allows it in the admin UI (Settings → Mods).

Options:
  --bind HOST:PORT        Game/API/lobby listen address (default: ${BIND})
  --admin-bind HOST:PORT  Admin UI/API listen address (default: ${ADMIN_BIND})
  --play-base URL         Browser client origin (default: http://<bind>/play when web-root set)
  --web-root DIR          Serve room-web at /play/ (default: target/room-web)
  --no-web                Do not build/serve room-web
  --no-build              Skip cargo build on start
  --run-dir DIR           Pid/log directory (default: target/room-server)
  -h, --help              Show this help

Environment:
  ROOM_SERVER_RUN_DIR   Same as --run-dir

Lobby:    http://<bind>/
Play:     http://<bind>/play/   (same-origin; needs --web-root)
Admin UI:  http://<admin-bind>/admin
Admin TUI: ./scripts/room-server.sh admin-tui
EOF
}

cmd=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    start|stop|restart|status|mods|admin-tui)
      if [[ -n "$cmd" ]]; then
        echo "room-server.sh: unexpected second command '$1'" >&2
        exit 2
      fi
      cmd="$1"
      shift
      ;;
    --bind) BIND="${2:?}"; shift 2 ;;
    --admin-bind) ADMIN_BIND="${2:?}"; shift 2 ;;
    --play-base) PLAY_BASE="${2:?}"; shift 2 ;;
    --web-root) WEB_ROOT="${2:?}"; shift 2 ;;
    --no-web) BUILD_WEB=0; WEB_ROOT=""; shift ;;
    --no-build) BUILD=0; shift ;;
    --run-dir)
      RUN_DIR="${2:?}"
      PID_FILE="$RUN_DIR/room-server.pid"
      LOG_FILE="$RUN_DIR/room-server.log"
      BIND_FILE="$RUN_DIR/bind"
      ADMIN_BIND_FILE="$RUN_DIR/admin_bind"
      shift 2
      ;;
    -h|--help) usage; exit 0 ;;
    --) shift; EXTRA_ARGS=("$@"); break ;;
    *)
      echo "room-server.sh: unknown arg: $1 (pass server flags after --)" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$cmd" ]]; then
  usage >&2
  exit 2
fi

mkdir -p "$RUN_DIR"

build_room_web() {
  local out="$1"
  echo "room-server: building room-web → ${out}"
  rustup target add wasm32-unknown-unknown >/dev/null
  if ! command -v wasm-bindgen >/dev/null 2>&1; then
    cargo install wasm-bindgen-cli --locked --version 0.2.129
  fi
  cargo build -p room-web --release --target wasm32-unknown-unknown --features browser
  local wasm="target/wasm32-unknown-unknown/release/room_web.wasm"
  rm -rf "$out"
  mkdir -p "$out"
  wasm-bindgen "$wasm" --out-dir "$out" --target web --no-typescript
  cp crates/room-web/index.html crates/room-web/voice.js crates/room-web/rtc.js \
    crates/room-web/netgraph.js crates/room-web/touch.js crates/room-web/settings.js crates/room-web/mixer-worklet.js \
    crates/room-web/editor.html crates/room-web/editor.js crates/room-web/editor-tools.js crates/room-web/editor-paint.js crates/room-web/editor-scripts.js crates/room-web/editor-extras.js crates/room-web/editor-geom.js crates/room-web/editor.css \
    crates/room-web/face-capture.js crates/room-web/face-gallery.js crates/room-web/face-share.js \
    crates/room-web/face-worker.js crates/room-web/settings-faces.js crates/room-web/sound-record.js \
    crates/room-web/favicon.png crates/room-web/apple-touch-icon.png "$out/"
  cp -R crates/room-web/as "$out/as"
  # Version every module URL so browsers never run a stale wasm / JS mix
  # (the server sends no cache headers).
  local v
  v="$(date +%s)"
  sed -i.bak "s|room_web_bg\.wasm|room_web_bg.wasm?v=${v}|g" "$out/room_web.js"
  sed -i.bak -E "s#\./(room_web|editor-tools|editor-paint|editor-scripts|editor-extras|editor-geom|netgraph|touch|rtc|voice)\.js#./\1.js?v=${v}#g" \
    "$out/editor.js" "$out/editor-extras.js" "$out/index.html"
  sed -i.bak -E "s#\./editor\.(js|css)#./editor.\1?v=${v}#g" "$out/editor.html"
  sed -i.bak "s|./face-capture.js|./face-capture.js?v=${v}|g" "$out/editor-paint.js"
  sed -i.bak "s|./sound-record.js|./sound-record.js?v=${v}|g" "$out/editor.js" "$out/editor-paint.js"
  rm -f "$out"/*.bak
  # GM SoundFont for web music (same search order as the desktop shell).
  for sf in soundfonts/room.sf2 soundfonts/TimGM6mb.sf2 soundfonts/default.sf2 soundfonts/gzdoom.sf2 soundfonts/FluidR3_GM.sf2; do
    if [[ -f "$sf" ]]; then
      cp "$sf" "$out/soundfont.sf2"
      break
    fi
  done
  mkdir -p "$out/mods"
  if [[ -f testdata/voxel/cheello_voxels.zip ]]; then
    cp testdata/voxel/cheello_voxels.zip "$out/mods/voxel.pk3"
    cp testdata/voxel/cheello_voxels.zip "$out/mods/cheello_voxels.zip"
  fi
  if [[ -f testdata/doom/doom1.wad ]]; then
    cp testdata/doom/doom1.wad "$out/doom1.wad"
  elif [[ -f testdata/freedoom/freedoom1.wad ]]; then
    cp testdata/freedoom/freedoom1.wad "$out/freedoom1.wad"
  elif [[ -f testdata/freedoom/freedoom2.wad ]]; then
    cp testdata/freedoom/freedoom2.wad "$out/freedoom1.wad"
  fi
  # Duke Nukem 3D rooms (lobby "Game" choice) load /play/duke3d.grp.
  if [[ -f testdata/duke/duke3d.grp ]]; then
    cp testdata/duke/duke3d.grp "$out/duke3d.grp"
  fi
  # The built-in solo preset is opt-in at the deployment boundary: honor
  # explicit paths first, then use local testdata when present. Commercial
  # Duke data stays in the local server's output and is never committed.
  local duke_grp="${ROOM_DUKE3D_GRP:-}"
  local duke_mod="${ROOM_BRUTAL_DUKE_DIR:-}"
  if [[ -z "$duke_grp" && -f testdata/duke/duke3d.grp ]]; then
    duke_grp="testdata/duke/duke3d.grp"
  fi
  if [[ -z "$duke_mod" && -f testdata/brutal-duke-v0.2.0/brutal/eduke.con ]]; then
    duke_mod="testdata/brutal-duke-v0.2.0/brutal"
  fi
  # Default "Play solo" content: Freedoom + Brutal Doom + optional Duke.
  ROOM_DUKE3D_GRP="$duke_grp" ROOM_BRUTAL_DUKE_DIR="$duke_mod" \
    ./scripts/bundle-solo-content.sh "$out" || echo "room-server: bundled solo content skipped" >&2
}

# Build each mods/<dir> with a room.toml manifest and pack it as a PWAD:
# ROOMMOD (room.toml) + one lump per [[module]] (`file`) from <name>.wasm,
# plus the `source` lump from <name>.ts when the manifest names one.
# Cargo.toml → Rust crate (one module); <name>.ts → AssemblyScript.
build_mods() {
  local dir
  for dir in mods/*/; do
    dir="${dir%/}"
    [[ -f "$dir/room.toml" ]] || continue
    if [[ -f "$dir/Cargo.toml" ]]; then
      rustup target add wasm32-unknown-unknown >/dev/null
      cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "$dir/Cargo.toml" --target-dir target/mods
    elif compgen -G "$dir/*.ts" >/dev/null; then
      if ! command -v node >/dev/null 2>&1; then
        echo "room-server: skip ${dir} (AssemblyScript needs node)" >&2
        continue
      fi
      [[ -f target/asc-npm/node_modules/assemblyscript/dist/asc.js ]] || ./scripts/fetch-assemblyscript.sh
    fi
    python3 - "$dir" <<'EOF'
import pathlib, struct, subprocess, sys, tomllib
d = pathlib.Path(sys.argv[1])
manifest = (d / "room.toml").read_bytes()
modules = tomllib.loads(manifest.decode()).get("module", [])
pk3 = d / "pk3"
if not modules and pk3.is_dir():
    # Content mod (GLDEFS post shader, ZSCRIPT, CVARINFO, MENUDEF …).
    subprocess.run([sys.executable, "scripts/pack-content-mod.py", str(d)], check=True)
    raise SystemExit(0)
if not modules:
    sys.exit(f"{d}/room.toml: no [[module]]")
# Host Python mods: pack .py lumps (desktop `python-mods` feature).
if all(
    str(m.get("language", m.get("lang", "wasm"))).lower() in ("python", "py")
    for m in modules
):
    subprocess.run(["./scripts/pack-python-mod.sh", str(d)], check=True)
    raise SystemExit(0)
cargo = d / "Cargo.toml"
if cargo.is_file():
    crate = tomllib.loads(cargo.read_text())["package"]["name"].replace("-", "_")
    built = pathlib.Path("target/mods/wasm32-unknown-unknown/release") / f"{crate}.wasm"
    (d / f"{modules[0]['name']}.wasm").write_bytes(built.read_bytes())
lumps = [("ROOMMOD", manifest)]
for m in modules:
    lang = str(m.get("language", m.get("lang", "wasm"))).lower()
    if lang in ("python", "py"):
        continue  # mixed manifests: python lumps packed separately
    wasm, ts = d / f"{m['name']}.wasm", d / f"{m['name']}.ts"
    if not cargo.is_file() and ts.is_file():
        subprocess.run(["node", "scripts/asc-compile.mjs", str(ts), str(wasm)], check=True)
    lumps.append((m["file"], wasm.read_bytes()))
    if m.get("source") and ts.is_file():
        lumps.append((m["source"], ts.read_bytes()))
# Track B: content under lumps/ (stem → lump name) and sprites/ (SS_START…SS_END).
lump_dir = d / "lumps"
if lump_dir.is_dir():
    for path in sorted(lump_dir.iterdir()):
        if not path.is_file() or path.name.startswith("."):
            continue
        name = path.stem.upper()[:8]
        lumps.append((name, path.read_bytes()))
sprite_dir = d / "sprites"
if sprite_dir.is_dir():
    frames = sorted(p for p in sprite_dir.iterdir() if p.is_file() and not p.name.startswith("."))
    if frames:
        lumps.append(("SS_START", b""))
        for path in frames:
            name = path.stem.upper()[:8]
            lumps.append((name, path.read_bytes()))
        lumps.append(("SS_END", b""))
maps_dir = d / "maps"
if maps_dir.is_dir():
    def _read_wad_lumps(data: bytes):
        if data[:4] not in (b"IWAD", b"PWAD"):
            sys.exit(f"{d}: not a WAD under maps/")
        num, off = struct.unpack_from("<ii", data, 4)
        out = []
        for i in range(num):
            fp, sz, nm = struct.unpack_from("<ii8s", data, off + i * 16)
            nm = nm.split(b"\0")[0].decode("ascii", "replace")
            out.append((nm, data[fp : fp + sz]))
        return out

    for path in sorted(maps_dir.glob("*.wad")):
        for name, body in _read_wad_lumps(path.read_bytes()):
            lumps.append((name, body))
data, dir_, off = b"", b"", 12
for name, body in lumps:
    dir_ += struct.pack("<ii8s", off, len(body), name.encode())
    data += body
    off += len(body)
out = d / f"{modules[0]['name']}.wad"
out.write_bytes(b"PWAD" + struct.pack("<ii", len(lumps), off) + data + dir_)
print(f"room-server: mod {out} ({len(data) + len(dir_) + 12} bytes)")
EOF
  done
}

http_base_for() {
  local bind="$1"
  local host port
  host="${bind%:*}"
  port="${bind##*:}"
  case "$host" in
    0.0.0.0|::|"[::]") host="127.0.0.1" ;;
  esac
  echo "http://${host}:${port}"
}

is_running() {
  [[ -f "$PID_FILE" ]] || return 1
  local pid
  pid="$(cat "$PID_FILE" 2>/dev/null || true)"
  [[ -n "${pid}" ]] || return 1
  kill -0 "$pid" 2>/dev/null
}

pid_of() {
  [[ -f "$PID_FILE" ]] && cat "$PID_FILE"
}

# room-server processes listening on our game/admin ports (a server whose
# pid file was lost: started by hand, or by a start that failed half-way).
stray_pids() {
  command -v lsof >/dev/null 2>&1 || return 0
  local port pid
  for port in "${BIND##*:}" "${ADMIN_BIND##*:}"; do
    for pid in $(lsof -nP -t -iTCP:"$port" -sTCP:LISTEN 2>/dev/null); do
      # Only ever stop a room-server, never another program on the port.
      if ps -p "$pid" -o comm= 2>/dev/null | grep -q 'room-serv'; then
        echo "$pid"
      fi
    done
  done | sort -u
}

stop_pid() {
  local pid="$1"
  echo "room-server: stopping pid=${pid}"
  kill "$pid" 2>/dev/null || true
  for _ in $(seq 1 50); do
    if ! kill -0 "$pid" 2>/dev/null; then
      return 0
    fi
    sleep 0.1
  done
  echo "room-server: SIGKILL pid=${pid}"
  kill -9 "$pid" 2>/dev/null || true
}

do_stop() {
  if ! is_running; then
    rm -f "$PID_FILE"
    local strays
    strays="$(stray_pids)"
    if [[ -n "$strays" ]]; then
      for pid in $strays; do
        stop_pid "$pid"
      done
      echo "room-server: stopped (it had no pid file)"
      return 0
    fi
    echo "room-server: not running"
    return 0
  fi
  local pid
  pid="$(pid_of)"
  echo "room-server: stopping pid=${pid}"
  kill "$pid" 2>/dev/null || true
  for _ in $(seq 1 50); do
    if ! kill -0 "$pid" 2>/dev/null; then
      break
    fi
    sleep 0.1
  done
  if kill -0 "$pid" 2>/dev/null; then
    echo "room-server: SIGKILL pid=${pid}"
    kill -9 "$pid" 2>/dev/null || true
  fi
  rm -f "$PID_FILE"
  echo "room-server: stopped"
}

do_start() {
  if is_running; then
    echo "room-server: already running pid=$(pid_of) (log: ${LOG_FILE})"
    return 0
  fi
  rm -f "$PID_FILE"

  if [[ "$BUILD" -eq 1 ]]; then
    echo "room-server: building release binary…"
    cargo build -p room-server --release --bin room-server
  fi
  if [[ ! -x "$BIN" ]]; then
    echo "room-server: missing binary ${BIN} (try without --no-build)" >&2
    exit 1
  fi

  local start_args=(--bind "$BIND" --admin-bind "$ADMIN_BIND")
  if [[ -n "$WEB_ROOT" && "$BUILD_WEB" -eq 1 ]]; then
    build_room_web "$WEB_ROOT"
  fi
  if [[ -n "$WEB_ROOT" && -d "$WEB_ROOT" && -f "$WEB_ROOT/index.html" ]]; then
    start_args+=(--web-root "$WEB_ROOT")
  elif [[ -n "$WEB_ROOT" ]]; then
    echo "room-server: warning: web-root ${WEB_ROOT} incomplete; Play needs ./scripts/play-web.sh first" >&2
    WEB_ROOT=""
  fi
  if [[ -n "$PLAY_BASE" ]]; then
    start_args+=(--play-base "$PLAY_BASE")
  elif [[ -n "$WEB_ROOT" ]]; then
    PLAY_BASE="$(http_base_for "$BIND")/play"
    start_args+=(--play-base "$PLAY_BASE")
  else
    PLAY_BASE="http://127.0.0.1:8080"
    start_args+=(--play-base "$PLAY_BASE")
  fi

  # Lightcall STUN/TURN for web WebRTC (skip if caller already set binds).
  local extra_joined="${EXTRA_ARGS[*]-}"
  if [[ "$extra_joined" != *"--stun-bind"* ]]; then
    start_args+=(--stun-bind "0.0.0.0:3478")
  fi
  if [[ "$extra_joined" != *"--turn-bind"* ]]; then
    start_args+=(--turn-bind "0.0.0.0:3479" --turn-public-ip "127.0.0.1")
  fi

  # Gameplay mods rooms can be created with (every player downloads the room's
  # mod); admins allow each one in Settings → Mods.
  if [[ "$extra_joined" != *"--mods-dir"* && -z "${ROOM_MODS_DIR:-}" ]]; then
    if [[ "$BUILD" -eq 1 ]]; then
      build_mods
    fi
    local mods_dir="$RUN_DIR/mods"
    mkdir -p "$mods_dir"
    rm -f "$mods_dir"/*.wad
    local wad
    for wad in mods/*/*.wad; do
      [[ -f "$wad" ]] && cp "$wad" "$mods_dir/"
    done
    start_args+=(--mods-dir "$mods_dir")
    # Every mod built from mods/ is allowed the first time the server sees
    # it (Play solo lists only allowed mods); admins can still disallow one
    # in Settings → Mods, and that choice is kept in data/mods.json.
    if [[ "$extra_joined" != *"--allow-mods"* ]]; then
      export ROOM_ALLOW_MODS="${ROOM_ALLOW_MODS:-*}"
    fi
  fi

  # Persist bans/audit/admins; bootstrap MFA admin on first run.
  local data_dir="$RUN_DIR/data"
  mkdir -p "$data_dir"
  if [[ "$extra_joined" != *"--data-dir"* ]]; then
    start_args+=(--data-dir "$data_dir")
  fi
  if [[ "$extra_joined" != *"--admin-rp-origin"* ]]; then
    start_args+=(--admin-rp-id localhost --admin-rp-origin "$(http_base_for "$ADMIN_BIND")")
  fi
  if [[ "$extra_joined" != *"--admin-bootstrap"* && -z "${ROOM_ADMIN_BOOTSTRAP:-}" && ! -f "$data_dir/admins.json" ]]; then
    start_args+=(--admin-bootstrap "admin:admin")
    printf 'username=admin\npassword=admin\n' >"$data_dir/bootstrap.txt"
    chmod 600 "$data_dir/bootstrap.txt" 2>/dev/null || true
    echo "room-server: first-run admin → user admin, password admin"
  fi

  # Fail fast if something else already owns the game/admin ports
  # (otherwise healthz succeeds against the stale process and we look "up").
  local port game_port admin_port
  game_port="${BIND##*:}"
  admin_port="${ADMIN_BIND##*:}"
  for port in "$game_port" "$admin_port"; do
    if command -v lsof >/dev/null 2>&1; then
      if lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1; then
        echo "room-server: port ${port} already in use — stop the other process first" >&2
        lsof -nP -iTCP:"$port" -sTCP:LISTEN >&2 || true
        if [[ -n "$(stray_pids)" ]]; then
          echo "room-server: it is an older room-server: './room stop' stops it (or './room restart')" >&2
        fi
        exit 1
      fi
    fi
  done

  echo "room-server: starting ${start_args[*]}${EXTRA_ARGS[*]:+ ${EXTRA_ARGS[*]}}"
  printf '%s\n' "$BIND" >"$BIND_FILE"
  printf '%s\n' "$ADMIN_BIND" >"$ADMIN_BIND_FILE"
  nohup "$BIN" "${start_args[@]}" \
    ${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"} >>"$LOG_FILE" 2>&1 &
  local pid=$!
  echo "$pid" >"$PID_FILE"

  local base admin_base
  base="$(http_base_for "$BIND")"
  admin_base="$(http_base_for "$ADMIN_BIND")"
  for _ in $(seq 1 100); do
    if ! kill -0 "$pid" 2>/dev/null; then
      echo "room-server: process exited early; see ${LOG_FILE}" >&2
      rm -f "$PID_FILE"
      exit 1
    fi
    if curl -sf "${base}/healthz" >/dev/null 2>&1 \
      && curl -sf "${admin_base}/healthz" >/dev/null 2>&1; then
      # Confirm our pid is the one listening (not a leftover).
      if command -v lsof >/dev/null 2>&1; then
        local listeners
        listeners="$(lsof -nP -tiTCP:"$game_port" -sTCP:LISTEN 2>/dev/null || true)"
        if [[ -n "$listeners" && "$listeners" != *"$pid"* ]]; then
          echo "room-server: healthz ok but pid ${pid} is not listening on ${BIND} (found ${listeners}); see ${LOG_FILE}" >&2
          kill "$pid" 2>/dev/null || true
          rm -f "$PID_FILE"
          exit 1
        fi
      fi
      echo "room-server: started pid=${pid}"
      echo "  lobby: ${base}/"
      echo "  play:  ${PLAY_BASE}/"
      echo "  admin: ${admin_base}/admin  (Settings → Mods to allow mods)"
      echo "  log:   ${LOG_FILE}"
      return 0
    fi
    sleep 0.05
  done
  echo "room-server: started pid=${pid} but healthz not ready yet (log: ${LOG_FILE})" >&2
}

do_status() {
  if [[ -f "$BIND_FILE" ]]; then
    BIND="$(cat "$BIND_FILE")"
  fi
  if [[ -f "$ADMIN_BIND_FILE" ]]; then
    ADMIN_BIND="$(cat "$ADMIN_BIND_FILE")"
  fi
  local base admin_base pid
  base="$(http_base_for "$BIND")"
  admin_base="$(http_base_for "$ADMIN_BIND")"
  if is_running; then
    pid="$(pid_of)"
    echo "room-server: running pid=${pid}"
    echo "  bind:       ${BIND}"
    echo "  lobby:      ${base}/"
    echo "  admin-bind: ${ADMIN_BIND}"
    echo "  admin UI:   ${admin_base}/admin"
    echo "  admin TUI:  ./scripts/room-server.sh admin-tui"
    echo "  log:        ${LOG_FILE}"
    echo "  pidfile:    ${PID_FILE}"
    if curl -sf "${base}/healthz" >/dev/null 2>&1; then
      echo "  healthz:    ok (${base}/healthz)"
    else
      echo "  healthz:    unreachable (${base}/healthz)"
    fi
    if out="$(curl -sf "${base}/readyz" 2>/dev/null)"; then
      echo "  readyz:     ${out}"
    else
      code="$(curl -s -o /dev/null -w '%{http_code}' "${base}/readyz" 2>/dev/null || echo '?')"
      echo "  readyz:     http ${code}"
    fi
    if curl -sf "${admin_base}/admin" >/dev/null 2>&1; then
      echo "  admin:      ok (${admin_base}/admin)"
    else
      echo "  admin:      unreachable (${admin_base}/admin)"
    fi
    return 0
  fi
  echo "room-server: not running"
  if [[ -f "$LOG_FILE" ]]; then
    echo "  last log: ${LOG_FILE}"
  fi
  return 1
}

do_admin_tui() {
  if [[ -f "$ADMIN_BIND_FILE" ]]; then
    ADMIN_BIND="$(cat "$ADMIN_BIND_FILE")"
  fi
  local admin_base bin target_dir
  admin_base="$(http_base_for "$ADMIN_BIND")"
  target_dir="${CARGO_TARGET_DIR:-$ROOT/target}"
  bin="${target_dir}/debug/room-admin"
  # Build first (show cargo progress), then exec the binary so the alternate-
  # screen TUI owns this terminal. A quiet `cargo run` looked stuck on the
  # one-line "admin TUI → …" message.
  echo "room-server: building room-admin…"
  cargo build -p room-admin-tui --bin room-admin
  if [[ ! -x "$bin" ]]; then
    echo "room-server: missing ${bin}" >&2
    exit 1
  fi
  # Clear the build chatter so the next thing on screen is the full TUI.
  printf '\033[2J\033[H'
  if [[ -z "${ROOM_ADMIN_USER:-}" && -n "${ROOM_ADMIN_BOOTSTRAP:-}" ]]; then
    ROOM_ADMIN_USER="${ROOM_ADMIN_BOOTSTRAP%%:*}"
    ROOM_ADMIN_PASSWORD="${ROOM_ADMIN_BOOTSTRAP#*:}"
    export ROOM_ADMIN_USER ROOM_ADMIN_PASSWORD
  fi
  exec "$bin" \
    --url "${admin_base}" \
    ${ROOM_ADMIN_TOKEN:+--token "$ROOM_ADMIN_TOKEN"} \
    ${ROOM_ADMIN_USER:+--user "$ROOM_ADMIN_USER"} \
    ${ROOM_ADMIN_PASSWORD:+--password "$ROOM_ADMIN_PASSWORD"} \
    ${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"}
}

case "$cmd" in
  start) do_start ;;
  stop) do_stop ;;
  restart) do_stop; do_start ;;
  status) do_status ;;
  mods) build_mods ;;
  admin-tui) do_admin_tui ;;
esac
