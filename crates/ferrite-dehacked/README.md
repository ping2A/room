# ferrite-dehacked

DeHackEd/BEX/MBF21 patcher for ClassDef IR.

## Public API (M5 bootstrap)

- `parse_dehacked` — `Thing` / `Frame` / `Pointer` / `Weapon` / `Ammo` / `Sound` /
  `Sprite` / `Misc` / `[STRINGS]` / `[CODEPTR]` / `[SOUNDS]` / `[SPRITES]`
- `apply_patch` — mutate `ClassDefIr` using `wadsrc/ferrite/dehacked/` tables
  (things, sprites, sounds, weapons, ammo, codepointers, states); Frame /
  Pointer / `[CODEPTR]` resolve via `states.txt` class/label/offset onto the
  containing DECORATE line
- `thing_name` / `sprite_name` / `sound_name` / `weapon_name` / `ammo_name` /
  `frame_target`
