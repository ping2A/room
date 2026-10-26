# room-cli

Command-line tools for Room archives, definitions, and maps.

## Commands

```text
room wad ls <path>
room wad info <path>
room wad extract <path> <lump> -o <file>

room defs dump <archives…> --registry mapinfo|sndinfo|gameinfo|language|textures|…

room map info <path> <MAP01> [--rebuild-nodes]
room map render2d <path> <MAP01> -o out.svg [--scale 0.25]
room map nodebuild <path> <MAP01>

room duke trace-diff <a.trace> <b.trace>
room duke trace-hash <a.trace>
room duke trace-check-manifest [testdata/duke-trace-manifest.toml]
room duke extract-demos [testdata/duke/duke3d.grp] -o testdata/duke/demos
room duke soft-view [--map E1L1.MAP] [-o soft-view.pgm] [--forward N] [--turn A]
```

Duke sync traces: [`docs/duke-trace-format.md`](../../docs/duke-trace-format.md),
[`docs/duke-trace-howto.md`](../../docs/duke-trace-howto.md).
