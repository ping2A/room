# ferrite-vfs

Virtual filesystem for WAD/PK3/PK7/directory mounts, namespaces, IWAD detection,
and load-order lump lookup (GZDoom `FileSystem` / `FResourceFile`).

ZIP/PK3 compression: stored, deflate, bzip2, and LZMA (pure Rust via `zip` 8.x).

## Public API

- [`Vfs`](crate::Vfs) / [`VfsBuilder`](crate::VfsBuilder) — mount containers, resolve lumps
- [`Namespace`](crate::Namespace), [`LumpInfo`](crate::LumpInfo), [`LumpId`](crate::LumpId)
- [`iwad`](crate::iwad) — IWADINFO parsing and candidate matching
- [`autoload`](crate::autoload) — `[*.Autoload]` section keys + `autoload/` dir discovery
- [`write`](crate::write) — `WadWriter` for tools/tests

See `docs/03-vfs.md`.
