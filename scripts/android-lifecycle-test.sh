#!/usr/bin/env bash
# docs/41 M2 acceptance on a phone or emulator: send Room to the background
# and back 20 times, rotate it 10 times, and check that the same process kept
# running the same game (no crash, no map reload).
#
#   scripts/build-android.sh --install && scripts/android-lifecycle-test.sh
#
# Needs one device in `adb devices`. Rotation uses the accelerometer setting,
# which is restored at the end.
set -euo pipefail
PKG=io.github.roomengine.room
ACT="$PKG/android.app.NativeActivity"

adb logcat -c
adb shell am start -W -n "$ACT" >/dev/null
sleep 8 # first run unpacks Freedoom
# The app opens on the launcher (docs/41 M4): Enter starts the first game.
adb shell input keyevent KEYCODE_ENTER
sleep 5
pid="$(adb shell pidof "$PKG" | tr -d '\r')"
[[ -n "$pid" ]] || { echo "lifecycle: app is not running" >&2; exit 1; }
echo "lifecycle: running as pid $pid"

for i in $(seq 1 20); do
  adb shell input keyevent KEYCODE_HOME
  sleep 1
  adb shell am start -n "$ACT" >/dev/null
  sleep 2
done

old_auto="$(adb shell settings get system accelerometer_rotation | tr -d '\r')"
old_rot="$(adb shell settings get system user_rotation | tr -d '\r')"
adb shell settings put system accelerometer_rotation 0
for i in $(seq 1 10); do
  adb shell settings put system user_rotation $(( (i % 2) * 2 + 1 )) # 1 and 3: both landscapes
  sleep 2
done
adb shell settings put system accelerometer_rotation "${old_auto:-1}"
adb shell settings put system user_rotation "${old_rot:-0}"

now="$(adb shell pidof "$PKG" | tr -d '\r')"
log="$(adb logcat -d -s room)"
resumed="$(grep -c 'room: resumed' <<<"$log" || true)"
entered="$(grep -c 'room: entered' <<<"$log" || true)"
crashed="$(grep -c 'crash report' <<<"$log" || true)"
echo "lifecycle: pid $pid -> ${now:-gone}; resumed $resumed time(s); map entered $entered time(s)"
grep 'room: resumed' <<<"$log" | tail -3 || true

fail=0
[[ "$now" == "$pid" ]] || { echo "lifecycle: FAIL the process changed or died" >&2; fail=1; }
(( resumed >= 20 )) || { echo "lifecycle: FAIL expected >= 20 returns from the background" >&2; fail=1; }
(( entered == 1 )) || { echo "lifecycle: FAIL the map was entered $entered times (reloaded?)" >&2; fail=1; }
(( crashed == 0 )) || { echo "lifecycle: FAIL a crash report was written" >&2; fail=1; }
[[ "$fail" == 0 ]] && echo "lifecycle: ok"
exit "$fail"
