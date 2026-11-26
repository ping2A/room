#!/usr/bin/env bash
# Build Room for iPhone / iPad (docs/41 M5) as a plain Room.app bundle:
# the `room-ios` executable, mobile/ios/Info.plist, Freedoom 1 + 2 (and the
# TimGM6mb SoundFont if fetched). No Xcode project is needed, only Xcode's
# command-line tools (SDK, simctl, codesign, devicectl).
#
#   scripts/build-ios.sh --sim [--install]      # simulator (no signing)
#   scripts/build-ios.sh --device [--install]   # a real device (signed)
#   add --release for optimised code
#   scripts/build-ios.sh --store                # TestFlight / App Store build
#                                               # (docs/41 M8): --device
#                                               # --release, no Brutal Doom,
#                                               # packaged as target/ios/Room.ipa
#
# Brutal Doom and Room's Rust port of its ZScript ship in development builds
# when found ($ROOM_BRUTALDOOM_PK3, else testdata/brutal*.pk3, as the web and
# Android builds); ROOM_IOS_NO_BRUTAL=1 leaves it out. The version comes from
# the Cargo workspace (CFBundleShortVersionString 0.1.0; CFBundleVersion the
# same number as Android's versionCode, with ROOM_BUILD_NUMBER 0–99).
#
# Device builds need:
#   IOS_SIGN_IDENTITY  e.g. "Apple Development: Jane Doe (ABCDE12345)"
#                      (security find-identity -v -p codesigning)
#   IOS_PROFILE        a .mobileprovision for the bundle id (Xcode makes one
#                      when you create an app id; or the developer portal)
#   IOS_BUNDLE_ID      optional, default io.github.roomengine.room
#   IOS_DEVICE         for --install: device id or name (xcrun devicectl list devices)
# --store needs an "Apple Distribution" IOS_SIGN_IDENTITY and an App Store
# IOS_PROFILE; upload Room.ipa with Apple's Transporter app (or
# `xcrun altool --upload-app -t ios -f target/ios/Room.ipa …`).
# Simulator --install uses the booted simulator (open -a Simulator first).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

kind=""
install=0
profile=debug
store=0
usage="usage: $0 --sim|--device|--store [--install] [--release]"
for arg in "$@"; do
  case "$arg" in
    --sim) kind=sim ;;
    --device) kind=device ;;
    --store) kind=device store=1 profile=release ;;
    --install) install=1 ;;
    --release) profile=release ;;
    *) echo "$usage" >&2; exit 2 ;;
  esac
done
[[ -n "$kind" ]] || { echo "$usage" >&2; exit 2; }
[[ "$(uname -s)" == Darwin ]] || { echo "build-ios: needs macOS with Xcode" >&2; exit 1; }
if [[ "$store" == 1 ]]; then
  # Brutal Doom has no licence for the whole mod (docs/41 M8).
  export ROOM_IOS_NO_BRUTAL=1
  [[ "$install" == 0 ]] || { echo "build-ios: --store makes an .ipa for TestFlight; use --device --install to try it" >&2; exit 2; }
fi

# Version from the workspace Cargo.toml (same numbers as Android).
version="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
version="${version:-0.1.0}"
IFS=. read -r v_major v_minor v_patch <<<"$version"
build_number="${ROOM_BUILD_NUMBER:-0}"
if ! [[ "$build_number" =~ ^[0-9]+$ ]] || (( build_number > 99 )); then
  echo "build-ios: ROOM_BUILD_NUMBER must be 0-99" >&2
  exit 2
fi
version_code=$(( ${v_major:-0} * 1000000 + ${v_minor:-0} * 10000 + ${v_patch%%[!0-9]*} * 100 + build_number ))

if [[ "$kind" == sim ]]; then
  if [[ "$(uname -m)" == arm64 ]]; then target=aarch64-apple-ios-sim; else target=x86_64-apple-ios; fi
else
  target=aarch64-apple-ios
fi
bundle_id="${IOS_BUNDLE_ID:-io.github.roomengine.room}"
export IPHONEOS_DEPLOYMENT_TARGET=15.0

rustup target add "$target" >/dev/null
flags=(-p room-mobile --features ios-app --bin room-ios --target "$target")
[[ "$profile" == release ]] && flags+=(--release)
(cd "$ROOT" && cargo build "${flags[@]}")

# Game files (read in place from the bundle; nothing is unpacked on iOS).
bash "$ROOT/scripts/fetch-freedoom.sh"
app="$ROOT/target/ios/$kind/Room.app"
rm -rf "$app"
mkdir -p "$app/soundfonts"
cp "$ROOT/target/$target/$profile/room-ios" "$app/room-ios"
cp "$ROOT/mobile/ios/Info.plist" "$app/Info.plist"
plutil -replace CFBundleIdentifier -string "$bundle_id" "$app/Info.plist"
plutil -replace CFBundleShortVersionString -string "$version" "$app/Info.plist"
plutil -replace CFBundleVersion -string "$version_code" "$app/Info.plist"
# App icon (scripts/make-android-icon.py writes the asset catalog): actool
# compiles it to Assets.car and says which Info.plist keys point at it.
if [[ -d "$ROOT/mobile/ios/Assets.xcassets" ]]; then
  partial="$(mktemp -t room-icon).plist"
  platform=iphoneos
  [[ "$kind" == sim ]] && platform=iphonesimulator
  xcrun actool "$ROOT/mobile/ios/Assets.xcassets" --compile "$app" \
    --platform "$platform" --minimum-deployment-target 15.0 \
    --app-icon AppIcon --target-device iphone --target-device ipad \
    --output-partial-info-plist "$partial" >/dev/null
  /usr/libexec/PlistBuddy -c "Merge $partial" "$app/Info.plist"
  rm -f "$partial"
fi
if [[ "$store" == 1 ]]; then
  # Keys Xcode adds to every App Store build; App Store Connect checks them.
  sdk_version="$(xcrun --sdk iphoneos --show-sdk-version)"
  xcode_version="$(xcodebuild -version | sed -n '1s/Xcode //p' | awk -F. '{ printf "%d%d%d", $1, $2, $3 }')"
  plutil -replace DTPlatformName -string iphoneos "$app/Info.plist"
  plutil -replace DTPlatformVersion -string "$sdk_version" "$app/Info.plist"
  plutil -replace DTSDKName -string "iphoneos$sdk_version" "$app/Info.plist"
  plutil -replace DTXcode -string "$xcode_version" "$app/Info.plist"
  plutil -replace DTXcodeBuild -string "$(xcodebuild -version | sed -n '2s/Build version //p')" "$app/Info.plist"
  plutil -replace DTCompiler -string com.apple.compilers.llvm.clang.1_0 "$app/Info.plist"
  plutil -replace BuildMachineOSBuild -string "$(sw_vers -buildVersion)" "$app/Info.plist"
  plutil -replace CFBundleSupportedPlatforms -json '["iPhoneOS"]' "$app/Info.plist"
fi
cp "$ROOT/testdata/freedoom/freedoom1.wad" "$ROOT/testdata/freedoom/freedoom2.wad" "$app/"
cp "$ROOT/testdata/freedoom/COPYING.txt" "$app/freedoom-COPYING.txt"
if [[ -f "$ROOT/soundfonts/TimGM6mb.sf2" ]]; then
  cp "$ROOT/soundfonts/TimGM6mb.sf2" "$app/soundfonts/"
else
  echo "build-ios: no soundfonts/TimGM6mb.sf2 (scripts/fetch-soundfont.sh); music falls back to OPL"
fi
# Brutal Doom + Room's Rust port of its ZScript (development builds); the
# launcher lists files in the app bundle as shipped and ticks them.
brutal="${ROOM_BRUTALDOOM_PK3:-}"
if [[ -z "$brutal" ]]; then
  for candidate in "$ROOT"/testdata/brutal*.pk3 "$ROOT"/testdata/*/brutal*.pk3; do
    [[ -f "$candidate" ]] && { brutal="$candidate"; break; }
  done
fi
if [[ -n "${ROOM_IOS_NO_BRUTAL:-}" ]]; then
  echo "build-ios: Brutal Doom left out"
elif [[ -n "$brutal" && -f "$brutal" ]]; then
  cp "$brutal" "$app/brutal.pk3"
  unzip -p "$brutal" DDZLICENSE >"$app/BRUTALDOOM-LICENSE.txt" 2>/dev/null || rm -f "$app/BRUTALDOOM-LICENSE.txt"
  zs="$ROOT/mods/brutal-zscript-rs/brutal-zscript.wad"
  [[ -f "$zs" ]] && cp "$zs" "$app/brutal-zscript.wad"
  echo "build-ios: shipping Brutal Doom from $brutal"
else
  echo "build-ios: no Brutal Doom pk3 (ROOM_BRUTALDOOM_PK3 or testdata/brutal*.pk3); Freedoom only"
fi

if [[ "$kind" == device ]]; then
  : "${IOS_SIGN_IDENTITY:?set IOS_SIGN_IDENTITY (security find-identity -v -p codesigning)}"
  : "${IOS_PROFILE:?set IOS_PROFILE to a .mobileprovision for $bundle_id}"
  cp "$IOS_PROFILE" "$app/embedded.mobileprovision"
  ent="$(mktemp -t room-ent).plist"
  security cms -D -i "$IOS_PROFILE" > "$ent.profile"
  /usr/libexec/PlistBuddy -x -c 'Print :Entitlements' "$ent.profile" > "$ent"
  codesign --force --timestamp=none --sign "$IOS_SIGN_IDENTITY" --entitlements "$ent" "$app"
  rm -f "$ent" "$ent.profile"
else
  # The simulator accepts an ad-hoc signature.
  codesign --force --sign - "$app"
fi
echo "build-ios: $app (version $version, build $version_code)"

if [[ "$store" == 1 ]]; then
  ipa="$ROOT/target/ios/Room.ipa"
  stage="$(mktemp -d -t room-ipa)"
  mkdir -p "$stage/Payload"
  cp -R "$app" "$stage/Payload/"
  rm -f "$ipa"
  (cd "$stage" && zip -qry "$ipa" Payload)
  rm -rf "$stage"
  echo "build-ios: $ipa — upload it with Transporter for TestFlight"
fi

if [[ "$install" == 1 ]]; then
  if [[ "$kind" == sim ]]; then
    xcrun simctl install booted "$app"
    xcrun simctl launch --terminate-running-process booted "$bundle_id"
    echo "build-ios: logs with: xcrun simctl spawn booted log stream --predicate 'process == \"room-ios\"'"
  else
    : "${IOS_DEVICE:?set IOS_DEVICE (xcrun devicectl list devices)}"
    xcrun devicectl device install app --device "$IOS_DEVICE" "$app"
    xcrun devicectl device process launch --device "$IOS_DEVICE" "$bundle_id"
  fi
fi
