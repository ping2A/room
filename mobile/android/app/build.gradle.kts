plugins {
    id("com.android.application")
}

// Version from the Cargo workspace, passed by scripts/build-android.sh
// (-ProomVersionName=0.1.0 -ProomVersionCode=10000); defaults for builds
// started from Android Studio.
val roomVersionName = (findProperty("roomVersionName") as String?) ?: "0.1.0"
val roomVersionCode = ((findProperty("roomVersionCode") as String?) ?: "1").toInt()

// Release signing from the environment (docs/41 M8). Without a keystore the
// release build is debug-signed: fine to sideload, refused by the Play Store.
val releaseKeystore: String? = System.getenv("ROOM_ANDROID_KEYSTORE")

android {
    namespace = "io.github.roomengine.room"
    compileSdk = 35

    defaultConfig {
        applicationId = "io.github.roomengine.room"
        // 26: AAudio and Vulkan are available on every device from here.
        minSdk = 26
        targetSdk = 35
        versionCode = roomVersionCode
        versionName = roomVersionName
        ndk {
            // Phones, and the x86_64 emulator image.
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    signingConfigs {
        if (releaseKeystore != null) {
            create("store") {
                storeFile = file(releaseKeystore)
                storePassword = System.getenv("ROOM_ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("ROOM_ANDROID_KEY_ALIAS")
                keyPassword = System.getenv("ROOM_ANDROID_KEY_PASSWORD")
                    ?: System.getenv("ROOM_ANDROID_KEYSTORE_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = if (releaseKeystore != null) {
                signingConfigs.getByName("store")
            } else {
                signingConfigs.getByName("debug")
            }
        }
    }

    androidResources {
        // Stored, not deflated: the app copies these out on first run, and
        // reading stored assets is much faster.
        noCompress += listOf("wad", "pk3", "sf2", "grp")
    }

    // `libroom_mobile.so` per ABI, written by scripts/build-android.sh.
    sourceSets["main"].jniLibs.srcDir("src/main/jniLibs")
    // Freedoom (and an optional SoundFont), copied by the same script.
    sourceSets["main"].assets.srcDir("src/main/assets")
}
