# room-proto

Wire types and codecs for **room-server** protocol v2 (`docs/22-room-server.md`).

- Binary: `FTR2` + type + payload (`TIC_CMD`, `TIC_FRAME`, `CHECKSUM`, …)
- Text: JSON control messages (`hello`, `welcome`, `ready`, …)

No I/O. Safe for `wasm32`. Every decoder returns `Result` and has unit + property tests.
