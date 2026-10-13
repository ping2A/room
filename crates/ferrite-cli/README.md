# ferrite-cli

Command-line tools for Ferrite archives, definitions, and maps.

## Commands

```text
ferrite wad ls <path>
ferrite wad info <path>
ferrite wad extract <path> <lump> -o <file>

ferrite defs dump <archives…> --registry mapinfo|sndinfo|gameinfo|language|textures|…

ferrite map info <path> <MAP01> [--rebuild-nodes]
ferrite map render2d <path> <MAP01> -o out.svg [--scale 0.25]
ferrite map nodebuild <path> <MAP01>
```
