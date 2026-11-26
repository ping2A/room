#!/usr/bin/env bash
# Start Room on the connected Android device and save what it logged
# (docs/41). Use it when the app closes at once or misbehaves:
#
#   scripts/android-logs.sh            # restart the app, collect for 15 s
#   scripts/android-logs.sh 60         # …for 60 s (play a bit meanwhile)
#   scripts/android-logs.sh --no-start # just collect what is already logged
#                                      # (after starting it on the phone)
#
# Writes mobile/android/last-run.log (Room's lines, Android's lines about
# the app, any crash and native-crash dump) and prints the important part.
# The full device log goes to mobile/android/last-run-full.log.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PKG=io.github.roomengine.room
OUT="$ROOT/mobile/android/last-run.log"
FULL="$ROOT/mobile/android/last-run-full.log"

seconds=15
start=1
for arg in "$@"; do
  case "$arg" in
    --no-start) start=0 ;;
    ''|*[!0-9]*) echo "usage: $0 [seconds] [--no-start]" >&2; exit 2 ;;
    *) seconds="$arg" ;;
  esac
done
command -v adb >/dev/null || { echo "android-logs: adb not found (Android SDK platform-tools)" >&2; exit 1; }
adb get-state >/dev/null 2>&1 || { echo "android-logs: no device (adb devices)" >&2; exit 1; }

if [[ "$start" == 1 ]]; then
  adb shell am force-stop "$PKG" || true
  adb logcat -c || true
  adb logcat -b crash -c 2>/dev/null || true
  echo "android-logs: starting $PKG, collecting for ${seconds}s"
  adb shell am start -n "$PKG/android.app.NativeActivity" >/dev/null
  for ((i = 0; i < seconds; i++)); do
    sleep 1
    if [[ $i -ge 3 ]] && ! adb shell pidof "$PKG" >/dev/null 2>&1; then
      echo "android-logs: the app is no longer running after $((i + 1))s"
      sleep 1 # let the crash dump land
      break
    fi
  done
fi

adb logcat -d -v threadtime >"$FULL" || true
{
  echo "# Room Android log, $(date)"
  echo "# device: $(adb shell getprop ro.product.manufacturer | tr -d '\r') $(adb shell getprop ro.product.model | tr -d '\r'), Android $(adb shell getprop ro.build.version.release | tr -d '\r') (API $(adb shell getprop ro.build.version.sdk | tr -d '\r')), ABI $(adb shell getprop ro.product.cpu.abi | tr -d '\r')"
  echo "# still running: $(adb shell pidof "$PKG" 2>/dev/null | tr -d '\r' || true)"
  echo
  echo "## Room (tag room) and the app's own lines"
  grep -E " (room|RustStdoutStderr|threaded_app|NativeActivity|android_activity|wgpu[a-z_]*|vulkan|AdrenoVK[a-zA-Z-]*|mali[a-zA-Z_]*)\s*:" "$FULL" || true
  echo
  echo "## Android about the app (start, death, linker, Java crashes)"
  { grep -E "$PKG|AndroidRuntime|linker|UnsatisfiedLinkError|dlopen" "$FULL" \
      | grep -vE " (room|RustStdoutStderr)\s*:" \
      | grep -E "ActivityManager|ActivityTaskManager|AndroidRuntime|linker|libc|Zygote|UnsatisfiedLinkError|dlopen|WindowManager: .*(died|Exception)|DEBUG" || true
    # Native crashes name the process only by its last 15 characters.
    grep -E "Fatal signal .*roomengine|F DEBUG +:|F libc +:.*roomengine" "$FULL" || true
  } | sort -s -k1,2
  echo
  echo "## Crash buffer (native crashes, tombstone summary)"
  adb logcat -d -b crash -v threadtime 2>/dev/null || true
  # The app keeps its own log on the phone (every line, even when it dies
  # before logcat is read): this run and the one before.
  for name in room-log.txt room-log-previous.txt; do
    echo
    echo "## $name (the app's own log file on the phone)"
    adb shell cat "/sdcard/Android/data/$PKG/files/$name" 2>/dev/null \
      || adb shell run-as "$PKG" cat "files/$name" 2>/dev/null \
      || echo "(not found)"
  done
} >"$OUT"

echo "android-logs: wrote $OUT (full device log: $FULL)"
echo "----"
# The interesting part: Room's last lines, errors and any crash.
grep -E "E room|F room|PANIC|FATAL|room: (stopped|start|step|android_main|libroom|gpu|adapter|surface|window|previous)|F DEBUG|Fatal signal|AndroidRuntime|UnsatisfiedLinkError|Process $PKG.*(died|has died)|Abort message" "$OUT" | tail -80 || true
echo "----"
echo "android-logs: send $OUT if the cause is not clear from the lines above"
