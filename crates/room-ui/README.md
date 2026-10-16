# room-ui

2D draw lists, fonts, menus, status bars, HUD, automap, intermission, finale.

## Public API (M5 bootstrap)

- `UiDrawList` / `DrawCmd` — GPU-free draw commands (fill / text / patch / line)
- `ListMenu` / `MenuStack` — Doom main → skill/episode / pause / quit confirm; patch items + skull selector; mouse hit-test (`MenuCommand`)
- `draw_title` — `TITLEPIC` or branded fallback
- `SoftFont` / `FontRegistry` — STCFN/FON1/FON2 metrics + `paint_text` glyph fills
- `HudState` — numbers, mugshot face, `STARMS` bits, ammo table, flash palette
- `DoomStatusBar` — classic 320×32 `STBAR` layout (ARMS, ammo table, vanilla key boxes)
- `screen_blend` — PLAYPAL pain / pickup / radsuit flash as an RGBA overlay
- `draw_automap` — top-down linedef overlay + player arrow
- `draw_intermission` / `draw_finale` — SoftFont end-of-level / episode screens
- `blit_draw_list_with_fonts` — scale virtual UI; soft-font text when registry provided
