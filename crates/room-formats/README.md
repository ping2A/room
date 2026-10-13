# room-formats

Pure binary parsers (`&[u8] → Result<T>`). No VFS. See `docs/04-binary-formats.md`.

## Implemented (M2 bootstrap)

- Doom picture (patch) + tall-patch posts
- PLAYPAL / COLORMAP / TRANMAP (Boom 256×256 blend; also TINTTAB / XLATAB)
- Flat size detection helpers
