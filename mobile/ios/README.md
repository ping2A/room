# Room for iPhone and iPad

`Room.app` is assembled by `scripts/build-ios.sh` around the `room-ios`
executable (`crates/room-mobile`, feature `ios-app`); there is no Xcode
project (docs/41 M5). You need Xcode's command-line tools.

```sh
open -a Simulator                              # boot an iPhone simulator
scripts/build-ios.sh --sim --install           # build, install, launch
scripts/ios-sim-lifecycle-test.sh              # background/foreground ×20

# A real device: you need a signing identity and a provisioning profile for
# the bundle id. Xcode makes both when it builds any app with that id for
# your device (profiles end up in ~/Library/Developer/Xcode/UserData/
# Provisioning Profiles); a paid account can also download them.
export IOS_SIGN_IDENTITY="Apple Development: …"   # security find-identity -v -p codesigning
export IOS_PROFILE=~/Downloads/Room.mobileprovision
export IOS_BUNDLE_ID=io.github.roomengine.room    # must match the profile
export IOS_DEVICE=<id or name>                    # xcrun devicectl list devices
scripts/build-ios.sh --device --install --release
```

Games and mods: copy WAD, PK3 or GRP files into Room's folder in the Files
app (On My iPhone › Room); the launcher lists them on the next start.
Development builds also ship Brutal Doom when the build finds it
(`$ROOM_BRUTALDOOM_PK3`, else `testdata/brutal*.pk3`; `ROOM_IOS_NO_BRUTAL=1`
leaves it out), ticked by default for Freedoom.

TestFlight (docs/41 M8): with an "Apple Distribution" identity and an App
Store provisioning profile in `IOS_SIGN_IDENTITY` / `IOS_PROFILE`,
`scripts/build-ios.sh --store` builds a release without Brutal Doom, adds the
keys App Store Connect checks, and packages `target/ios/Room.ipa`; upload it
with Apple's Transporter app. The version comes from the workspace
`Cargo.toml` (raise `ROOM_BUILD_NUMBER` for a second upload of one
version). The app icon is the asset catalog `mobile/ios/Assets.xcassets`
(Freedoom's dead face, from `scripts/make-android-icon.py`), compiled by
`actool` during the build.
