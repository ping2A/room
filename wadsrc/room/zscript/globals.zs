// Play-scope names (`docs/34` §14.3.2). The resolver also knows these
// builtins so a unit that only includes `base.zs` still resolves them.
version "4.12.0"

class _Globals native
{
    native readonly int GameState;
    native readonly int GS_LEVEL;
    native readonly int Skill;
    native readonly int consoleplayer;
    native readonly int gametic;
    native readonly bool multiplayer;
    native readonly int MAXPLAYERS;
    native readonly LevelLocals level;
    native readonly Array<PlayerInfo> players;
}
