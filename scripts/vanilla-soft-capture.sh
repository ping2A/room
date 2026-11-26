#!/usr/bin/env bash
# Reference frames from Doom's own software renderer for room-render-soft
# (docs/40 SR0–SR2), full screen 320×200, raw palette indices.
#
# Builds doomgeneric (headless vanilla / Chocolate Doom) with a hook that,
# once the map is loaded (monsters included), draws each pose of
# testdata/soft/poses.txt for that IWAD twice with R_RenderPlayerView:
#   <map>-<pose>.idx         walls, floors, ceilings, sky (SR1: masked skipped)
#   <map>-<pose>-things.idx  the whole view: things, masked walls, weapon (SR2)
#   <map>-<pose>-things.txt  what that view drew from: fuzzpos, every mobj by
#                            sector thing list, the weapon layers
# under testdata/soft/<iwad>/, plus .ppm files to look at. room-render-soft's
# `vanilla_captures_match` and `vanilla_things_match` tests compare against
# them.
#
# Usage: ./scripts/vanilla-soft-capture.sh [IWAD ...]
#   default: testdata/freedoom/freedoom1.wad testdata/freedoom/freedoom2.wad
# Needs: git, a C compiler, python3. No SDL.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$ROOT/target/doomgeneric-soft"
POSES="$ROOT/testdata/soft/poses.txt"
mkdir -p "$WORK"
if [[ ! -d "$WORK/doomgeneric" ]]; then
  git clone --depth 1 https://github.com/ozkl/doomgeneric.git "$WORK/doomgeneric"
fi
SRC="$WORK/doomgeneric/doomgeneric"

cat > "$SRC/doomgeneric_soft.c" <<'C'
#include "doomgeneric.h"
#include "doomstat.h"
#include "r_local.h"
#include "r_main.h"
#include "w_wad.h"
#include "z_zone.h"
#include "i_video.h"
#include "info.h"
#include "p_local.h"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int dg_pose_set = 0;
int dg_pose_masked = 0;
fixed_t dg_x, dg_y, dg_z;
angle_t dg_angle;

extern int fuzzpos;

static void write_frame(const char *outdir, const char *mapname, const char *name,
                        const char *suffix) {
    char path[512];
    byte *pal = W_CacheLumpName("PLAYPAL", PU_CACHE);
    FILE *o;
    int k;
    snprintf(path, sizeof path, "%s/%s-%s%s.idx", outdir, mapname, name, suffix);
    o = fopen(path, "wb");
    fwrite(I_VideoBuffer, 1, SCREENWIDTH * SCREENHEIGHT, o);
    fclose(o);
    snprintf(path, sizeof path, "%s/%s-%s%s.ppm", outdir, mapname, name, suffix);
    o = fopen(path, "wb");
    fprintf(o, "P6\n%d %d\n255\n", SCREENWIDTH, SCREENHEIGHT);
    for (k = 0; k < SCREENWIDTH * SCREENHEIGHT; k++)
        fwrite(pal + I_VideoBuffer[k] * 3, 1, 3, o);
    fclose(o);
    printf("wrote %s/%s-%s%s.idx\n", outdir, mapname, name, suffix);
}

/* The inputs of the things view, in the order R_AddSprites reads them. */
static void write_things(const char *outdir, const char *mapname, const char *name) {
    char path[512];
    player_t *pl = &players[consoleplayer];
    FILE *o;
    int s, i;
    snprintf(path, sizeof path, "%s/%s-%s-things.txt", outdir, mapname, name);
    o = fopen(path, "w");
    fprintf(o, "fuzzpos %d\n", fuzzpos);
    fprintf(o, "psprite_fuzz %d\n",
            (pl->powers[pw_invisibility] > 4 * 32 || (pl->powers[pw_invisibility] & 8)) ? 1 : 0);
    for (s = 0; s < numsectors; s++) {
        mobj_t *mo;
        for (mo = sectors[s].thinglist; mo; mo = mo->snext)
            fprintf(o, "thing %d %d %d %u %s %d %d %d\n", mo->x, mo->y, mo->z,
                    (unsigned)mo->angle, sprnames[mo->sprite], mo->frame, mo->flags, s);
    }
    for (i = 0; i < NUMPSPRITES; i++) {
        pspdef_t *psp = &pl->psprites[i];
        if (psp->state)
            fprintf(o, "psprite %s %d %d %d\n", sprnames[psp->state->sprite],
                    psp->state->frame, psp->sx, psp->sy);
    }
    fclose(o);
}

static uint32_t fake_ms = 0;
void DG_Init(void) {}
void DG_DrawFrame(void) {}
void DG_SleepMs(uint32_t ms) { fake_ms += ms; }
uint32_t DG_GetTicksMs(void) { fake_ms += 1; return fake_ms; }
int DG_GetKey(int *pressed, unsigned char *key) { (void)pressed; (void)key; return 0; }
void DG_SetWindowTitle(const char *title) { (void)title; }

int main(int argc, char **argv) {
    const char *poses = getenv("DG_POSES");   /* "name x y z angle" lines */
    const char *outdir = getenv("DG_OUT");
    const char *mapname = getenv("DG_MAP");
    char line[256];
    FILE *f;
    int i;
    doomgeneric_Create(argc, argv);
    for (i = 0; i < 400 && !(gamestate == GS_LEVEL && players[consoleplayer].mo); i++)
        doomgeneric_Tick();
    if (gamestate != GS_LEVEL) { fprintf(stderr, "level did not load\n"); return 1; }
    R_SetViewSize(11, 0);
    R_ExecuteSetViewSize();
    f = fopen(poses, "r");
    if (!f) { perror(poses); return 1; }
    while (fgets(line, sizeof line, f)) {
        char name[64], zs[32];
        double x, y, ang;
        if (sscanf(line, "%63s %lf %lf %31s %lf", name, &x, &y, zs, &ang) != 5) continue;
        dg_x = (fixed_t)(x * 65536.0);
        dg_y = (fixed_t)(y * 65536.0);
        if (strcmp(zs, "auto") == 0)
            dg_z = R_PointInSubsector(dg_x, dg_y)->sector->floorheight + 41 * FRACUNIT;
        else
            dg_z = (fixed_t)(atof(zs) * 65536.0);
        dg_angle = (angle_t)(unsigned long long)(ang / 360.0 * 4294967296.0);
        dg_pose_set = 1;
        dg_pose_masked = 0;
        R_RenderPlayerView(&players[consoleplayer]);
        write_frame(outdir, mapname, name, "");
        write_things(outdir, mapname, name);
        dg_pose_masked = 1;
        R_RenderPlayerView(&players[consoleplayer]);
        write_frame(outdir, mapname, name, "-things");
    }
    fclose(f);
    return 0;
}
C

# Start from the pristine r_main.c so an older hook is replaced.
git -C "$WORK/doomgeneric" checkout -- doomgeneric/r_main.c
python3 - "$SRC/r_main.c" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
if "dg_pose_set" not in s:
    s = s.replace("""    viewz = player->viewz;""", """    viewz = player->viewz;
    {
        extern int dg_pose_set;
        extern fixed_t dg_x, dg_y, dg_z;
        extern angle_t dg_angle;
        if (dg_pose_set) {
            viewx = dg_x; viewy = dg_y; viewz = dg_z; viewangle = dg_angle;
            extralight = 0;
        }
    }""", 1)
    s = s.replace("""    R_DrawMasked ();""", """    {
        extern int dg_pose_set, dg_pose_masked;
        if (!dg_pose_set || dg_pose_masked) R_DrawMasked ();
    }""", 1)
    open(p, "w").write(s)
PY

SRCS=$(grep "^SRC_DOOM" "$SRC/Makefile" | sed 's/SRC_DOOM = //; s/doomgeneric_xlib.o/doomgeneric_soft.o/; s/\.o/.c/g')
(cd "$SRC" && ${CC:-cc} -O1 -w -DNORMALUNIX -DLINUX -D_DEFAULT_SOURCE $SRCS -o "$WORK/dgsoft" -lm)

if [[ $# -eq 0 ]]; then
  set -- "$ROOT/testdata/freedoom/freedoom1.wad" "$ROOT/testdata/freedoom/freedoom2.wad"
fi
for IWAD in "$@"; do
  BASE="$(basename "$IWAD")"
  OUT="$ROOT/testdata/soft/$BASE"
  mkdir -p "$OUT"
  for MAP in $(grep -v '^#' "$POSES" | awk -v b="$BASE" '$1 == b {print $2}' | sort -u); do
    grep -v '^#' "$POSES" | awk -v b="$BASE" -v m="$MAP" '$1 == b && $2 == m {print $3, $4, $5, $6, $7}' > "$WORK/poses-$MAP.txt"
    if [[ "$MAP" =~ ^E([0-9])M([0-9])$ ]]; then
      WARP=(-warp "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}")
    else
      WARP=(-warp "${MAP#MAP}")
    fi
    (cd "$WORK" && DG_POSES="$WORK/poses-$MAP.txt" DG_OUT="$OUT" DG_MAP="$MAP" \
      ./dgsoft -iwad "$IWAD" "${WARP[@]}" -skill 3 -nosound -nomusic)
  done
done
