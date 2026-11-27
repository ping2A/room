# Room for Android

A NativeActivity APK around `crates/room-mobile` (docs/41, ADR 0009). It
ships Freedoom 1 and 2 and, when the build finds it, Brutal Doom with Room's
Rust port of its ZScript (`$ROOM_BRUTALDOOM_PK3`, else `testdata/brutal*.pk3`,
as the web build; `ROOM_ANDROID_NO_BRUTAL=1` leaves it out). The app opens on
the launcher; shipped mods are ticked by default, so picking Freedoom and
Play starts Freedoom + Brutal Doom. Brutal Doom adds ~160 MB to the APK and
the same again once unpacked on the phone at first start.

## Build

You need Android Studio's SDK (platform 35), the NDK (r27c or newer), a
JDK 17+ and `cargo install cargo-ndk --locked`. The script uses Android
Studio's bundled JDK when `JAVA_HOME` is not set; without Android Studio,
`brew install --cask temurin@17` (macOS) or `apt install openjdk-17-jdk`.

```sh
export ANDROID_HOME=~/Library/Android/sdk          # macOS default
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/<version>
scripts/build-android.sh --install                  # build, install, launch
adb logcat -s room                                   # engine messages
scripts/android-lifecycle-test.sh                    # docs/41 M2 acceptance
```

If the app closes at once or misbehaves, `scripts/android-logs.sh [seconds]`
restarts it and saves its log to `mobile/android/last-run.log` (Room's own
lines, Android's lines about the app, any crash dump); `--install` runs it
for 15 s after installing. Every start-up step logs a `room: start:` or
`room: step:` line; a panic logs `E room: PANIC …` with a backtrace, and a
native crash logs `F room: FATAL <signal> … last room: step: …` before
Android's own crash report. So the last lines show where a start stopped.

The app also writes every line to a file on the phone,
`/sdcard/Android/data/io.github.roomengine.room/files/room-log.txt` (the
run before is `room-log-previous.txt`, and its last lines are repeated in
logcat at the next start). After a crash started from the phone itself,
`scripts/android-logs.sh --no-start` collects both files.

The launcher icon is the Room logo (`docs/media/logo.png`), made by
`python3 scripts/make-android-icon.py` (needs Pillow); the PNGs are
committed, so a normal build does not need it.

`--release` builds optimised native code. Release builds are signed with
your keystore when `ROOM_ANDROID_KEYSTORE`, `ROOM_ANDROID_KEYSTORE_PASSWORD`
and `ROOM_ANDROID_KEY_ALIAS` (and `ROOM_ANDROID_KEY_PASSWORD` if it differs)
are set, and debug-signed otherwise (fine to sideload, refused by the Play
Store). `--bundle` makes an `.aab` instead of an `.apk`; `--store` is the
Play Store build: release, `.aab`, keystore required, Brutal Doom left out
(docs/41 M8). The version comes from the workspace `Cargo.toml`
(`versionCode` = major·1000000 + minor·10000 + patch·100 +
`ROOM_BUILD_NUMBER`, so raise `ROOM_BUILD_NUMBER` to upload a second build
of one version).

```sh
keytool -genkeypair -v -keystore ~/room-release.jks -alias room \
  -keyalg RSA -keysize 4096 -validity 10000      # once; keep it safe
export ROOM_ANDROID_KEYSTORE=~/room-release.jks ROOM_ANDROID_KEY_ALIAS=room
export ROOM_ANDROID_KEYSTORE_PASSWORD=…           # from a password manager
scripts/build-android.sh --store                  # → app-release.aab
```

Opening `mobile/android` in Android Studio works too, after
`scripts/build-android.sh` has filled `app/src/main/jniLibs` and
`app/src/main/assets`.
