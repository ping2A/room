# Red Noir

The world in black, white and grey; only blood stays red. The weapon turns
grey with the world (as in GZDoom); the status bar and menus keep their
colours.

A post-process shader cannot know what a pixel shows, so blood is told by
its colour: Doom's blood uses the palette's pure red ramp (red high, green
and blue near zero). Red bricks, flesh and fire are browner, pinker or more
orange and turn grey. Other pure reds stay red too (the red cross on medikits,
red keys), and a red pain flash still tints the screen.

## Settings

| CVar | Values | Default |
|---|---|---|
| `noir_mode` | 0 normal colours, 1 red blood only, 2 every strong red (blood, red keys, red lights), 3 black and white only | 1 |
| `noir_contrast` | contrast of the greys, 50–200 % | 130 |
| `noir_grain` | film grain, 0–100 % | 20 |
| `noir_vignette` | darkened corners, 0–100 % | 35 |

All are user CVars, saved with your settings, and in **Options → Red Noir**.

## Use it

```sh
./scripts/play-doom.sh --file testdata/brutal22test6.pk3 --noir        # builds the pk3 if needed
./scripts/play-doom.sh --noir reds --set noir_grain=0
mods/rednoir/pack.sh                                                   # → mods/rednoir/rednoir.pk3
room --iwad freedoom2.wad --file mods/rednoir/rednoir.pk3
```

It also loads in GZDoom 4.x with OpenGL (`gzdoom -file rednoir.pk3`) and
stacks with other mods such as Brutal Doom.

## In the browser (room server)

`./room start` (or `./scripts/room-server.sh start`) packs `pk3/` into
`mods/rednoir/rednoir.wad` (with `room.toml` as its manifest, via
`scripts/pack-content-mod.py`) and adds it to the server's mod catalog. It is
allowed the first time the server sees it, so it is listed in the lobby's
**Play solo** (pick **Red Noir** under Mod) and can be chosen for new rooms;
an admin can turn it off in the admin page (Settings → Mods). The browser runs
the same shader and script, and its settings are under Options → Red Noir.

`scripts/package-web.sh` packs it into the static site too (`mods/rednoir.wad`):
choose it in the game bar's Mod list, or open the play page with
`?mod=rednoir`, or `?pack=rednoir` to stack it on a gameplay mod
(`?mod=survival&pack=rednoir`).
