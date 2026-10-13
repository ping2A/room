# ferrite-nodebuild

Internal BSP / GL node builder.

## M3 status

- **Recursive BSP** (`build_bsp` / `build_nodes`): ZDBSP-style splitter cost
  (`SplitCost`, imbalance, AA preference); straddling segs are cut at the
  partition and snapped to 16.16.
- **GL nodes** (`game_to_gl` / via `apply_nodes`): closes subsector rings with
  minisegs (`line = None`) and pairs partners on two-sided linedefs.
- **Sector-fan** (`build_nodes_fan`): one subsector per sector (legacy / tests).
- **Point lookup** (`point_in_subsector` / `point_in_sector`) over BSP
  (`NF_SUBSECTOR` / bit-31 child = subsector; classic bit-15 remapped on load).
  Also `point_on_side_vanilla` / `point_in_sector_vanilla` (Doom 16.16) for
  parity tests against IWAD node trees.
- **Validation** (`validate_subsectors`): closed + convex checks for GL-style
  / fan polygons. Classic Doom `SEGS` omit partition minisegs and are not
  expected to pass closure.
