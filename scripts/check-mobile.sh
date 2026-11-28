#!/usr/bin/env bash
# Check that the phone entry crate (crates/room-mobile) compiles for Android
# and/or iOS (docs/41 M1). Usage:
#
#   scripts/check-mobile.sh android   # needs ANDROID_NDK_HOME and cargo-ndk
#   scripts/check-mobile.sh ios       # needs macOS with Xcode
#   scripts/check-mobile.sh           # both, skipping what this machine can't do
#
# `room-mobile` pulls `room-app` without default features, so this also
# proves the app builds without clap, the Cranelift JIT and gilrs.
# Each step ends in `|| return 1` so a failure is reported even when a
# function runs as an `if` condition (where `set -e` is off).
set -euo pipefail
cd "$(dirname "$0")/.."

want="${1:-all}"
ran=0

android() {
  if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
    echo "check-mobile: ANDROID_NDK_HOME is not set (install the NDK, e.g. r27c)" >&2
    return 1
  fi
  if ! command -v cargo-ndk >/dev/null; then
    echo "check-mobile: cargo-ndk missing (cargo install cargo-ndk --locked)" >&2
    return 1
  fi
  rustup target add aarch64-linux-android x86_64-linux-android >/dev/null || return 1
  # API 24 (Android 7): AAudio needs 26, cpal falls back to OpenSL ES below it.
  cargo ndk --platform 24 -t arm64-v8a -t x86_64 check -p room-mobile || return 1
  cargo ndk --platform 24 -t arm64-v8a clippy -p room-mobile --no-deps -- -D warnings || return 1
}

ios() {
  if [[ "$(uname -s)" != "Darwin" ]] || ! xcrun --sdk iphoneos --show-sdk-path >/dev/null 2>&1; then
    echo "check-mobile: iOS needs macOS with Xcode" >&2
    return 1
  fi
  rustup target add aarch64-apple-ios aarch64-apple-ios-sim >/dev/null || return 1
  cargo check -p room-mobile --features ios-app --target aarch64-apple-ios || return 1
  cargo check -p room-mobile --features ios-app --target aarch64-apple-ios-sim || return 1
  cargo clippy -p room-mobile --features ios-app --no-deps --target aarch64-apple-ios -- -D warnings || return 1
}

case "$want" in
  android) android; ran=1 ;;
  ios) ios; ran=1 ;;
  all)
    if android; then ran=1; else echo "check-mobile: skipped Android" >&2; fi
    if ios; then ran=1; else echo "check-mobile: skipped iOS" >&2; fi
    ;;
  *) echo "usage: $0 [android|ios]" >&2; exit 2 ;;
esac

if [[ "$ran" == 0 ]]; then
  echo "check-mobile: nothing checked" >&2
  exit 1
fi
echo "check-mobile: ok"
