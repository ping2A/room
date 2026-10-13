# ferrite-web

Browser shell (`wasm-bindgen` + `web-sys` + wgpu).

```
cargo test -p ferrite-web
cargo build -p ferrite-web --target wasm32-unknown-unknown --features browser
```

With `browser` on `wasm32`, startup finds `#ferrite` and runs a wgpu map viewer
(embedded UDMF room, orbiting camera; WebGPU / WebGL2). See `index.html`.

JS embed API (M13):

- `ferriteVersion()` — package version
- `ferriteUrlParam(name)` — `?map=` / `?iwad=` query helpers
- `ferriteOpfsSet` / `ferriteOpfsGet` — durable key/value for the host page
  (`localStorage` today; OPFS-ready naming)
