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
```
