# room-faceswap

Pure-Rust face alignment and sprite-head compositing for Room
([`docs/26-face-swap.md`](../../docs/26-face-swap.md)).

No filesystem, GPU, network, or window. Web editor, desktop, `room-cli`, and
the renderer share this crate.

## API surface

| Module | Role |
|--------|------|
| `align` | Manual eye/chin → canonical 128×128 `FaceProfile` |
| `parse` | `HEADANCHORS` text lump (`room-sc`) |
| `estimate` | Silhouette auto-anchor |
| `composite` | Front paste (Oklab transfer + PLAYPAL quantize) |
| `bake` | Class / prefix bake |
| `assign` | Per-class / per-instance rules (non-sim hash) |
| `live` | Render-only override table |
| `share` | MP share policy + 128×128 blob caps |
| `duke` | Build-palette skip helpers |
| `detect` | Stub (empty); manual points always work |

## CLI

```bash
room faceswap bake --iwad testdata/freedoom/freedoom2.wad \
  --face me.png --eyes 40,50,90,50 --chin 65,120 \
  --prefix TROO --anchors wadsrc/room/HEADANCHORS -o faces.pk3

room faceswap anchors-estimate --iwad … --prefix POSS
```

## Determinism

Compositing uses `libm` only. Same inputs → same bytes on native and wasm32.
