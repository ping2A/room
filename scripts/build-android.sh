#!/usr/bin/env bash
# Build the Room Android app (docs/41 M2):
#   1. libroom_mobile.so for arm64 phones and the x86_64 emulator (cargo-ndk),
#   2. Freedoom 1 + 2 (and the TimGM6mb SoundFont, if fetched) into the APK assets,
#   3. the APK with Gradle.
#
# Needs: ANDROID_NDK_HOME (NDK r27c or newer), ANDROID_HOME / Android Studio's
# SDK (platform 35, build-tools), a JDK 17+, `cargo install cargo-ndk`.
#
#   scripts/build-android.sh            # debug APK
#   scripts/build-android.sh --release  # optimised native code
#   scripts/build-android.sh --install  # …and adb install -r + launch
#   scripts/build-android.sh --bundle   # an .aab (Play) instead of an .apk
#   scripts/build-android.sh --store    # Play Store build (docs/41 M8):
#                                       # --release --bundle, no Brutal Doom,
#                                       # signed with the keystore below
#
# Release signing (docs/41 M8), from the environment; without it release
# builds are debug-signed (fine to sideload, refused by the Play Store):
#   ROOM_ANDROID_KEYSTORE           path to the .jks / .keystore
#   ROOM_ANDROID_KEYSTORE_PASSWORD  its password
#   ROOM_ANDROID_KEY_ALIAS          the key's alias
#   ROOM_ANDROID_KEY_PASSWORD       the key's password (default: the store's)
# The version comes from the Cargo workspace (versionName = 0.1.0,
# versionCode = major·1000000 + minor·10000 + patch·100 + ROOM_BUILD_NUMBER,
# 0–99, default 0: raise it to upload a second build of one version).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/mobile/android"

profile=debug
install=0
bundle=0
store=0
for arg in "$@"; do
  case "$arg" in
    --release) profile=release ;;
    --install) install=1 ;;
    --bundle) bundle=1 ;;
    --store) store=1 profile=release bundle=1 ;;
    *) echo "usage: $0 [--release] [--install] [--bundle] [--store]" >&2; exit 2 ;;
  esac
done
if [[ "$bundle" == 1 && "$install" == 1 ]]; then
  echo "build-android: --install needs an APK; an .aab goes to the Play Console" >&2
  exit 2
fi
if [[ "$store" == 1 ]]; then
  # Brutal Doom has no licence for the whole mod (docs/41 M8).
  export ROOM_ANDROID_NO_BRUTAL=1
  for v in ROOM_ANDROID_KEYSTORE ROOM_ANDROID_KEYSTORE_PASSWORD ROOM_ANDROID_KEY_ALIAS; do
    [[ -n "${!v:-}" ]] || { echo "build-android: --store needs $v (see the top of this script)" >&2; exit 1; }
  done
  [[ -f "$ROOM_ANDROID_KEYSTORE" ]] || { echo "build-android: no keystore at $ROOM_ANDROID_KEYSTORE" >&2; exit 1; }
fi

# Version from the workspace Cargo.toml.
version="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
IFS=. read -r v_major v_minor v_patch <<<"${version:-0.1.0}"
build_number="${ROOM_BUILD_NUMBER:-0}"
if ! [[ "$build_number" =~ ^[0-9]+$ ]] || (( build_number > 99 )); then
  echo "build-android: ROOM_BUILD_NUMBER must be 0-99" >&2
  exit 2
fi
version_code=$(( ${v_major:-0} * 1000000 + ${v_minor:-0} * 10000 + ${v_patch%%[!0-9]*} * 100 + build_number ))
echo "build-android: version ${version:-0.1.0} (code $version_code)"

if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
  echo "build-android: set ANDROID_NDK_HOME (Android Studio: SDK Manager > SDK Tools > NDK)" >&2
  exit 1
fi
command -v cargo-ndk >/dev/null || { echo "build-android: cargo install cargo-ndk --locked" >&2; exit 1; }

# Gradle needs a JDK 17+. macOS ships only a stub `java` ("Unable to locate a
# Java Runtime"), so look for a real one: JAVA_HOME, Android Studio's bundled
# JBR, then any JDK the system knows. Checked before the Rust build so a
# missing JDK fails in a second, not after it.
java_major() { # prints the major version of "$1/bin/java", or nothing
  "$1/bin/java" -version 2>&1 | sed -nE '1s/.*version "(1\.)?([0-9]+).*/\2/p'
}
find_jdk() {
  local c
  for c in \
    "${JAVA_HOME:-}" \
    "/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
    "$HOME/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
    "/opt/android-studio/jbr" \
    "$HOME/android-studio/jbr" \
    "$( [[ -x /usr/libexec/java_home ]] && /usr/libexec/java_home -v 17+ 2>/dev/null )"; do
    [[ -n "$c" && -x "$c/bin/java" ]] || continue
    local v; v="$(java_major "$c")"
    [[ -n "$v" && "$v" -ge 17 ]] && { echo "$c"; return 0; }
  done
  return 1
}
if jdk="$(find_jdk)"; then
  export JAVA_HOME="$jdk"
  echo "build-android: JDK $(java_major "$JAVA_HOME") at $JAVA_HOME"
else
  cat >&2 <<'MSG'
build-android: no JDK 17 or newer found (Gradle needs one to build the APK).
  - With Android Studio installed, its JDK is used automatically; otherwise
  - macOS:  brew install --cask temurin@17   (or: brew install openjdk@17)
  - Linux:  sudo apt install openjdk-17-jdk
  or point JAVA_HOME at an existing JDK 17+.
MSG
  exit 1
fi

# Gradle finds the SDK through ANDROID_HOME (or app/local.properties).
if [[ -z "${ANDROID_HOME:-}" ]]; then
  for c in "$HOME/Library/Android/sdk" "$HOME/Android/Sdk"; do
    [[ -d "$c/platforms" ]] && { export ANDROID_HOME="$c"; break; }
  done
fi
if [[ -z "${ANDROID_HOME:-}" && ! -f "$APP/local.properties" ]]; then
  echo "build-android: set ANDROID_HOME to the Android SDK (Android Studio: Settings > Android SDK)" >&2
  exit 1
fi
rustup target add aarch64-linux-android x86_64-linux-android >/dev/null

# 1. Native library -> app/src/main/jniLibs/<abi>/libroom_mobile.so
# Start from an empty jniLibs: cargo-ndk also copies stray .so files that
# dependencies leave in the target folder (libsevenz_rust2-*.so); the app
# loads only libroom_mobile.so, which needs nothing but system libraries.
rm -rf "$APP/app/src/main/jniLibs"
cargo_profile=()
[[ "$profile" == release ]] && cargo_profile=(--release)
(cd "$ROOT" && cargo ndk --platform 26 -t arm64-v8a -t x86_64 \
  -o "$APP/app/src/main/jniLibs" build -p room-mobile ${cargo_profile[@]+"${cargo_profile[@]}"})
find "$APP/app/src/main/jniLibs" -name '*.so' ! -name libroom_mobile.so -delete

# A shared library may link with symbols left undefined; Android then refuses
# to load it ("dlopen failed: cannot locate symbol …") and the app closes at
# once. Check here instead: every undefined symbol must come from a system
# library. C++ runtime symbols (`_Z…`, `__cxa_…`, `__gxx_…`) never do.
readelf_bin="$(ls "$ANDROID_NDK_HOME"/toolchains/llvm/prebuilt/*/bin/llvm-readelf 2>/dev/null | head -1)"
[[ -x "$readelf_bin" ]] || readelf_bin="$(command -v llvm-readelf || command -v readelf || true)"
if [[ -n "$readelf_bin" ]]; then
  for so in "$APP"/app/src/main/jniLibs/*/libroom_mobile.so; do
    missing="$("$readelf_bin" -W --dyn-syms "$so" | awk '$7 == "UND" && $5 != "WEAK" { print $8 }' \
      | grep -E '^(_Z|__cxa_|__gxx_|__cxx)' | grep -v '@' || true)"
    if [[ -n "$missing" ]]; then
      echo "build-android: $so has C++ runtime symbols no system library provides;" >&2
      echo "  Android would refuse to load it. Missing:" >&2
      echo "$missing" | head -20 | sed 's/^/    /' >&2
      exit 1
    fi
  done
  echo "build-android: libroom_mobile.so: no missing C++ runtime symbols"
else
  echo "build-android: no readelf found, skipping the undefined-symbol check"
fi

# 2. Game files shipped inside the APK (copied to app storage on first run).
bash "$ROOT/scripts/fetch-freedoom.sh"
assets="$APP/app/src/main/assets"
mkdir -p "$assets/soundfonts"
cp "$ROOT/testdata/freedoom/freedoom1.wad" "$ROOT/testdata/freedoom/freedoom2.wad" "$assets/"
# Freedoom is BSD-3-Clause: its licence travels with the WADs.
cp "$ROOT/testdata/freedoom/COPYING.txt" "$assets/freedoom-COPYING.txt"
# Brutal Doom + Room's Rust port of its ZScript, found the way the web
# build's scripts/bundle-solo-content.sh finds them: $ROOM_BRUTALDOOM_PK3,
# else testdata/brutal*.pk3. The launcher ticks it for Freedoom by default.
# ROOM_ANDROID_NO_BRUTAL=1 leaves it out (an APK ~160 MB smaller).
rm -f "$assets/brutal.pk3" "$assets/brutal-zscript.wad" "$assets/BRUTALDOOM-LICENSE.txt"
brutal="${ROOM_BRUTALDOOM_PK3:-}"
if [[ -z "$brutal" ]]; then
  for candidate in "$ROOT"/testdata/brutal*.pk3 "$ROOT"/testdata/*/brutal*.pk3; do
    [[ -f "$candidate" ]] && { brutal="$candidate"; break; }
  done
fi
if [[ -n "${ROOM_ANDROID_NO_BRUTAL:-}" ]]; then
  echo "build-android: ROOM_ANDROID_NO_BRUTAL set: Brutal Doom left out"
elif [[ -n "$brutal" && -f "$brutal" ]]; then
  cp "$brutal" "$assets/brutal.pk3"
  unzip -p "$brutal" DDZLICENSE >"$assets/BRUTALDOOM-LICENSE.txt" 2>/dev/null \
    || rm -f "$assets/BRUTALDOOM-LICENSE.txt"
  zs="$ROOT/mods/brutal-zscript-rs/brutal-zscript.wad"
  if [[ -f "$zs" ]]; then
    cp "$zs" "$assets/brutal-zscript.wad"
  else
    echo "build-android: no $zs (mods/brutal-zscript-rs/build.sh); Brutal Doom runs its own ZScript"
  fi
  echo "build-android: shipping Brutal Doom from $brutal"
else
  echo "build-android: no Brutal Doom pk3 (set ROOM_BRUTALDOOM_PK3 or put brutal*.pk3 in testdata/); Freedoom only"
fi
if [[ -f "$ROOT/soundfonts/TimGM6mb.sf2" ]]; then
  cp "$ROOT/soundfonts/TimGM6mb.sf2" "$assets/soundfonts/"
else
  echo "build-android: no soundfonts/TimGM6mb.sf2 (scripts/fetch-soundfont.sh); music falls back to OPL"
fi
# Room's own mods (Survival, Bounty, Royale, Shuriken, Color World, Red
# Noir), as the web build serves them. bundled-mods.txt lists them for the
# app's first-run copy; the launcher shows them unticked.
# ROOM_ANDROID_NO_MODS=1 leaves them out.
if [[ -n "${ROOM_ANDROID_NO_MODS:-}" ]]; then
  if [[ -f "$assets/bundled-mods.txt" ]]; then
    while IFS= read -r old; do
      [[ -n "$old" && "$old" != */* ]] && rm -f "$assets/$old"
    done <"$assets/bundled-mods.txt"
    rm -f "$assets/bundled-mods.txt"
  fi
  echo "build-android: ROOM_ANDROID_NO_MODS set: Room's mods left out"
else
  bash "$ROOT/scripts/stage-mods.sh" "$assets"
fi

# 3. APK (or AAB)
variant=Debug
[[ "$profile" == release ]] && variant=Release
lower="$(tr '[:upper:]' '[:lower:]' <<<"$variant")"
if [[ "$bundle" == 1 ]]; then
  task="bundle$variant"
  apk="$APP/app/build/outputs/bundle/$lower/app-$lower.aab"
else
  task="assemble$variant"
  apk="$APP/app/build/outputs/apk/$lower/app-$lower.apk"
fi
if [[ "$profile" == release && -z "${ROOM_ANDROID_KEYSTORE:-}" ]]; then
  echo "build-android: no ROOM_ANDROID_KEYSTORE: the release build is debug-signed (sideload only)"
fi
(cd "$APP" && ./gradlew --no-daemon \
  -ProomVersionName="${version:-0.1.0}" -ProomVersionCode="$version_code" "$task")
echo "build-android: $apk"

if [[ "$install" == 1 ]]; then
  adb install -r "$apk"
  # Start the app and keep its log (mobile/android/last-run.log): if it
  # closes at once, the reason is printed here.
  bash "$ROOT/scripts/android-logs.sh" 15
  echo "build-android: live logs with: adb logcat -s room"
fi
