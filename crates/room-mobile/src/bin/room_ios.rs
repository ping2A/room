//! The iPhone / iPad app executable (docs/41 M5). `scripts/build-ios.sh`
//! puts it in `Room.app` with `mobile/ios/Info.plist` and the game files.

fn main() {
    #[cfg(target_os = "ios")]
    room_mobile::room_ios_main();
    #[cfg(not(target_os = "ios"))]
    {
        eprintln!("room-ios: this is the iOS app; build it with scripts/build-ios.sh");
        std::process::exit(2);
    }
}
