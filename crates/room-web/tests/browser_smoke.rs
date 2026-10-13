//! Browser smoke via `wasm-bindgen-test` (Chrome/Firefox headless).
//!
//! ```text
//! WASM_BINDGEN_TEST_TIMEOUT=120 cargo test -p room-web --test browser_smoke \
//!   --target wasm32-unknown-unknown --features browser
//! ```

#![allow(clippy::unwrap_used)]

#[cfg(all(target_arch = "wasm32", feature = "browser"))]
mod wasm {
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn version_exposed() {
        assert!(!room_web::version().is_empty());
    }

    #[wasm_bindgen_test]
    fn embedded_demo_map_in_browser() {
        let map = room_web::load_embedded_demo_map().expect("demo map");
        assert_eq!(map.sectors.len(), 1);
        assert!(!map.lines.is_empty());
    }
}

#[cfg(not(all(target_arch = "wasm32", feature = "browser")))]
#[test]
fn browser_smoke_host_placeholder() {
    // Real checks run under wasm32 + browser + wasm-bindgen-test-runner.
}
