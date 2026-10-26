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
Admin UI: http://<admin-bind>/admin
EOF
}

cmd=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    start|stop|restart|status|mods)
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
    crates/room-web/editor.html crates/room-web/editor.js crates/room-web/editor-tools.js crates/room-web/editor-paint.js crates/room-web/editor-scripts.js crates/room-web/editor.css \
    crates/room-web/face-capture.js crates/room-web/face-gallery.js crates/room-web/face-share.js \
    crates/room-web/face-worker.js crates/room-web/settings-faces.js crates/room-web/sound-record.js "$out/"
  cp -R crates/room-web/as "$out/as"
  # Version every module URL so browsers never run a stale wasm / JS mix
  # (the server sends no cache headers).
  local v
  v="$(date +%s)"
  sed -i.bak "s|room_web_bg\.wasm|room_web_bg.wasm?v=${v}|g" "$out/room_web.js"
  sed -i.bak -E "s#\./(room_web|editor-tools|editor-paint|editor-scripts|netgraph|touch|rtc|voice)\.js#./\1.js?v=${v}#g" \
    "$out/editor.js" "$out/index.html"
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
if not modules:
    sys.exit(f"{d}/room.toml: no [[module]]")
cargo = d / "Cargo.toml"
if cargo.is_file():
    crate = tomllib.loads(cargo.read_text())["package"]["name"].replace("-", "_")
    built = pathlib.Path("target/mods/wasm32-unknown-unknown/release") / f"{crate}.wasm"
    (d / f"{modules[0]['name']}.wasm").write_bytes(built.read_bytes())
lumps = [("ROOMMOD", manifest)]
for m in modules:
    wasm, ts = d / f"{m['name']}.wasm", d / f"{m['name']}.ts"
    if not cargo.is_file() and ts.is_file():
        subprocess.run(["node", "scripts/asc-compile.mjs", str(ts), str(wasm)], check=True)
    lumps.append((m["file"], wasm.read_bytes()))
    if m.get("source") and ts.is_file():
        lumps.append((m["source"], ts.read_bytes()))
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

do_stop() {
  if ! is_running; then
    rm -f "$PID_FILE"
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

case "$cmd" in
  start) do_start ;;
  stop) do_stop ;;
  restart) do_stop; do_start ;;
  status) do_status ;;
  mods) build_mods ;;
esac
