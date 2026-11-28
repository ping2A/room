# Color World

A small mod that recolours the 3D view with a full-screen post-process shader.
The weapon is recoloured with the world (as in GZDoom); the status bar and
menus keep their normal colours.

## Modes

| `colorworld_mode` | Look |
|---|---|
| 0 | Normal (shader off) |
| 1 | Rainbow: every surface keeps its shading but its colour follows a moving rainbow across the screen |
| 2 | Rainbow bands: brightness is split into 7 bands, each one a rainbow colour |
| 3 | Hue cycle: the original colours, with their hue slowly rotating |
| 4 | Grayscale |
| 5 | Sepia |
| 6 | Inverted |
| 7 | Thermal (dark is purple, bright is yellow/white) |
| 8 | Game Boy (4 shades of green) |
| 9 | Neon edges: black world with glowing rainbow outlines |

Two more settings:

- `colorworld_speed` (0–400, default 100): animation speed in percent for modes 1, 2, 3 and 9.
- `colorworld_strength` (0–100, default 100): how much of the effect is mixed over the original picture.

All three are user CVars, saved with your settings. They are also in
**Options → Color World**.

## Use it

Build the pk3:

```sh
mods/colorworld/pack.sh
```

With `scripts/play-doom.sh` (builds the pk3 for you, mode by name or number):

```sh
./scripts/play-doom.sh --file testdata/brutal22test6.pk3 --colors rainbow
./scripts/play-doom.sh --colors neon --colors-speed 200 --colors-strength 80
# or any mod option, for any mod:
./scripts/play-doom.sh --file mods/colorworld/colorworld.pk3 --set colorworld_mode=7
```

Mode names for `--colors`: `normal rainbow bands hue gray sepia invert thermal gameboy neon`.

Room (desktop) directly:

```sh
room --iwad freedoom2.wad --file mods/colorworld/colorworld.pk3 --set colorworld_mode=1
```

The `pk3/` folder can be loaded directly too (`--file mods/colorworld/pk3`).
It also loads in GZDoom 4.x with OpenGL (`gzdoom -file colorworld.pk3`),
and stacks with other mods such as Brutal Doom.

## How it works

- `GLDEFS` declares a `HardwareShader PostProcess scene` named `ColorWorld`, disabled by default.
- `ZSCRIPT` has an event handler that, every tic, reads the CVars, turns the shader on or off with
  `PPShader.SetEnabled` and passes the mode, the tic count, the speed and the strength with
  `PPShader.SetUniform1i`.
- `shaders/colorworld.fp` does the colour work. All uniforms are ints.

## In the browser (room server)

`./room start` (or `./scripts/room-server.sh start`) packs `pk3/` into
`mods/colorworld/colorworld.wad` (with `room.toml` as its manifest, via
`scripts/pack-content-mod.py`) and adds it to the server's mod catalog. It is
allowed the first time the server sees it, so it is listed in the lobby's
**Play solo** (pick **Color World** under Mod) and can be chosen for new rooms;
an admin can turn it off in the admin page (Settings → Mods). The browser runs
the same shader and script, and its settings are under Options → Color World.

`scripts/package-web.sh` packs it into the static site too (`mods/colorworld.wad`):
choose it in the game bar's Mod list, or open the play page with
`?mod=colorworld`, or `?pack=colorworld` to stack it on a gameplay mod
(`?mod=survival&pack=colorworld`).
