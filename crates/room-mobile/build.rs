//! Android: link the C++ ABI library next to `libc++_static`.
//!
//! cpal plays sound through oboe, a C++ library; `oboe-sys` asks for
//! `libc++_static`. Since NDK r23 the C++ ABI half of the runtime
//! (`__cxa_*`, `operator new`, exception and type-info classes) is a
//! separate archive, `libc++abi.a`, which `clang++` adds by itself but a
//! Rust link does not. Without it `libroom_mobile.so` still links (a shared
//! library may leave symbols undefined) and then fails to load on the phone:
//! `dlopen failed: cannot locate symbol "__cxa_pure_virtual"`.
//! `scripts/build-android.sh` checks the result for such symbols.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        // Found in the NDK sysroot the linker already searches; there is
        // only a static `libc++abi.a`, so it is linked into the library.
        println!("cargo:rustc-link-lib=c++abi");
    }
}
