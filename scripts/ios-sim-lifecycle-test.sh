#!/usr/bin/env bash
# docs/41 M5 acceptance on the iOS simulator: start Room, start the first game
# from the launcher, send the app to the background and back 20 times, and
# check the same game kept running (no crash, no map reload).
#
#   open -a Simulator            # boot an iPhone simulator first
#   scripts/build-ios.sh --sim --install && scripts/ios-sim-lifecycle-test.sh
#
# The launcher tap goes through the app's ROOM_TEST_TOUCH hook (simctl passes
# SIMCTL_CHILD_* variables to the app); simctl has no rotation command, so
# rotation is checked by hand (Simulator > Device > Rotate).
set -euo pipefail
ID="${IOS_BUNDLE_ID:-io.github.roomengine.room}"
out="$(mktemp -d)"
# Tap the middle of the first launcher row (52 dp rows from 96 dp down) on a
# 393-point-tall landscape iPhone, at frame 60.
export SIMCTL_CHILD_ROOM_TEST_TOUCH="60,1,s,0.5,0.31;62,1,e,0.5,0.31"
xcrun simctl launch --terminate-running-process --stderr="$out/stderr.txt" booted "$ID" >/dev/null
sleep 12
for i in $(seq 1 20); do
  xcrun simctl launch booted com.apple.Preferences >/dev/null
  sleep 1
  xcrun simctl launch --stderr="$out/stderr.txt" booted "$ID" >/dev/null
  sleep 2
done
log="$(cat "$out/stderr.txt")"
resumed="$(grep -c 'room: resumed' <<<"$log" || true)"
entered="$(grep -c 'room: entered' <<<"$log" || true)"
crashed="$(grep -c 'crash report' <<<"$log" || true)"
echo "lifecycle: resumed $resumed time(s); map entered $entered time(s); log in $out/stderr.txt"
fail=0
(( entered == 1 )) || { echo "lifecycle: FAIL the map was entered $entered times (expected once)" >&2; fail=1; }
(( resumed >= 20 )) || { echo "lifecycle: FAIL expected >= 20 returns from the background" >&2; fail=1; }
(( crashed == 0 )) || { echo "lifecycle: FAIL a crash report was written" >&2; fail=1; }
[[ "$fail" == 0 ]] && echo "lifecycle: ok"
exit "$fail"
