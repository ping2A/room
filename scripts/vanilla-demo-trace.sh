#!/usr/bin/env bash
# Per-tic reference trace of a vanilla demo, for finding where Room's demo
# playback first drifts (freedoom_smoke `freedoom2_demo1_vanilla_full_playback`).
#
# Builds doomgeneric (a headless port of the vanilla / Chocolate Doom engine)
# with a dump hook in P_Ticker, then runs `-timedemo` on the IWAD's DEMO1.
# Output: one line per tic — `leveltime x y angle prndindex health momx momy`
# (x / y in map units, the rest raw). Set DG_MOBJS=<tic> to also dump every
# mobj for 40 tics from <tic>.
#
# Usage: ./scripts/vanilla-demo-trace.sh [IWAD] [OUT]
#   IWAD defaults to testdata/freedoom/freedoom2.wad, OUT to target/vanilla-demo1.txt
# Needs: git, a C compiler, python3. No SDL.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IWAD="${1:-$ROOT/testdata/freedoom/freedoom2.wad}"
OUT="${2:-$ROOT/target/vanilla-demo1.txt}"
WORK="$ROOT/target/doomgeneric-trace"
mkdir -p "$WORK" "$(dirname "$OUT")"
if [[ ! -d "$WORK/doomgeneric" ]]; then
  git clone --depth 1 https://github.com/ozkl/doomgeneric.git "$WORK/doomgeneric"
fi
SRC="$WORK/doomgeneric/doomgeneric"
cat > "$SRC/doomgeneric_dump.c" <<'C'
#include "doomgeneric.h"
#include <stdint.h>
static uint32_t fake_ms = 0;
void DG_Init(void) {}
void DG_DrawFrame(void) {}
void DG_SleepMs(uint32_t ms) { fake_ms += ms; }
uint32_t DG_GetTicksMs(void) { fake_ms += 1; return fake_ms; }
int DG_GetKey(int *pressed, unsigned char *key) { (void)pressed; (void)key; return 0; }
void DG_SetWindowTitle(const char *title) { (void)title; }
int main(int argc, char **argv) {
    doomgeneric_Create(argc, argv);
    for (;;) doomgeneric_Tick();
    return 0;
}
C
python3 - "$SRC/p_tick.c" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
if "DG_DUMP" not in s:
    s = s.replace("""    // for par times
    leveltime++;""", """    // for par times
    leveltime++;
    {
        extern int prndindex;
        static FILE *dump;
        const char *path = getenv("DG_DUMP");
        const char *mobjs = getenv("DG_MOBJS");
        mobj_t *mo = players[0].mo;
        if (!dump) dump = fopen(path ? path : "dg_dump.txt", "w");
        if (dump && mo) {
            fprintf(dump, "%d %.4f %.4f %u %d %d %d %d\\n", leveltime,
                    mo->x / 65536.0, mo->y / 65536.0, (unsigned)mo->angle,
                    prndindex, mo->health, mo->momx, mo->momy);
            if (mobjs && leveltime >= atoi(mobjs) && leveltime <= atoi(mobjs) + 40) {
                thinker_t *th;
                for (th = thinkercap.next; th != &thinkercap; th = th->next) {
                    mobj_t *m;
                    if (th->function.acp1 != (actionf_p1) P_MobjThinker) continue;
                    m = (mobj_t *) th;
                    fprintf(dump, "  M %d type=%d x=%.4f y=%.4f z=%.4f st=%d tics=%d hp=%d\\n",
                            leveltime, m->type, m->x / 65536.0, m->y / 65536.0,
                            m->z / 65536.0, (int)(m->state - states), m->tics, m->health);
                }
            }
            fflush(dump);
        }
    }""")
    s = "#include <stdlib.h>\n#include <stdio.h>\n" + s
    open(p, "w").write(s)
PY
SRCS=$(grep "^SRC_DOOM" "$SRC/Makefile" | sed 's/SRC_DOOM = //; s/doomgeneric_xlib.o/doomgeneric_dump.o/; s/\.o/.c/g')
(cd "$SRC" && ${CC:-cc} -O1 -w -DNORMALUNIX -DLINUX -D_DEFAULT_SOURCE $SRCS -o "$WORK/dgtrace" -lm)
python3 - "$IWAD" "$WORK/demo1.lmp" <<'PY'
import struct, sys
data = open(sys.argv[1], "rb").read()
_, n, off = struct.unpack_from("<4sII", data, 0)
for i in range(n):
    o, size, name = struct.unpack_from("<II8s", data, off + i * 16)
    if name.split(b"\0")[0].upper() == b"DEMO1":
        open(sys.argv[2], "wb").write(data[o:o + size])
        break
else:
    raise SystemExit("DEMO1 not found")
PY
(cd "$WORK" && DG_DUMP="$OUT" "$WORK/dgtrace" -iwad "$IWAD" -timedemo demo1 >/dev/null 2>&1 || true)
echo "wrote $OUT ($(wc -l < "$OUT") tics)"
