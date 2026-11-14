#!/bin/bash
# Build the Chocolate Duke3D reference oracle used for DEMO sync work.
# Needs: git, gcc with -m32 (gcc-multilib), 32-bit SDL 1.2 + SDL_mixer
# (libsdl1.2-dev:i386, libsdl-mixer1.2-dev:i386 or the .so.0 runtime libs).
# Output: scripts/duke-oracle/build/chocolate-duke3d (non-PIE, debug info,
# so `addr2line` maps krand call sites).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
src=$here/build/src; out=$here/build
mkdir -p "$out"
if [ ! -d "$src" ]; then
  git clone https://github.com/fabiensanglard/chocolate_duke3D.git "$src"
  git -C "$src" checkout 1cfa1909aa1f5a5ab159301044ca499673bc2283
  git -C "$src" apply "$here/chocolate.patch"
  cp "$here/room_trace.c" "$here/room_trace.h" "$src/Game/src/"
fi
F="-m32 -mtune=generic -march=i686 -g -O1 -fcommon -fno-strict-aliasing -fno-aggressive-loop-optimizations"
I="-I$src/Engine/src -I$src/Game/src -I/usr/include/SDL -D_GNU_SOURCE=1 -D_REENTRANT -DPLATFORM_UNIX"
SRCS="Engine/src/cache.c Engine/src/display.c Engine/src/draw.c Engine/src/dummy_multi.c Engine/src/engine.c
Engine/src/filesystem.c Engine/src/fixedPoint_math.c Engine/src/network.c Engine/src/tiles.c
Game/src/actors.c Game/src/animlib.c Game/src/audiolib/dsl.c Game/src/audiolib/fx_man.c
Game/src/audiolib/ll_man.c Game/src/audiolib/multivoc.c Game/src/audiolib/mv_mix.c Game/src/audiolib/mvreverb.c
Game/src/audiolib/nodpmi.c Game/src/audiolib/pitch.c Game/src/audiolib/user.c Game/src/config.c
Game/src/console.c Game/src/control.c Game/src/cvar_defs.c Game/src/cvars.c Game/src/dummy_audiolib.c
Game/src/game.c Game/src/gamedef.c Game/src/global.c Game/src/keyboard.c Game/src/menues.c
Game/src/midi/sdl_midi.c Game/src/player.c Game/src/room_trace.c Game/src/rts.c Game/src/scriplib.c
Game/src/sector.c Game/src/sounds.c Game/src/premap.c"
objs=()
for f in $SRCS; do
  o=$out/$(echo "$f" | tr / _).o
  gcc $F $I -w -c "$src/$f" -o "$o"
  objs+=("$o")
done
SDL=$(ls /usr/lib/i386-linux-gnu/libSDL-1.2.so* /lib/i386-linux-gnu/libSDL-1.2.so* 2>/dev/null | head -1)
MIX=$(ls /usr/lib/i386-linux-gnu/libSDL_mixer-1.2.so* /lib/i386-linux-gnu/libSDL_mixer-1.2.so* 2>/dev/null | head -1)
gcc -m32 -no-pie -o "$out/chocolate-duke3d" "${objs[@]}" -Wl,--allow-multiple-definition "$SDL" "$MIX" -lm
echo "built $out/chocolate-duke3d"
