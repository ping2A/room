// Room ZScript stdlib stubs (docs/34 §5.2) — BD subset.
// Native members bind in room-zvm; bodies may be native in Room.

version "4.12.0"

// Global play-scope names (compiler also maps these to natives).
const MAXPLAYERS = 64;
const consoleplayer = 0;
const skill = 0;

class Object native
{
    native static Object New(class<Object> type);
    native void Destroy();
}

class Thinker : Object native
{
    virtual void Tick() {}
    virtual void PostBeginPlay() {}
    virtual void BeginPlay() {}
}

class Actor : Thinker native
{
    native int health;
    native bool bISMONSTER;
    native void GiveInventory(class<Inventory> item, int amount = 1);
    native void TakeInventory(class<Inventory> item, int amount = 1);
    native string GetClassName();
    native void ChangeTID(int newtid);
    native string SoundClass;
    native int TID;
}

class Inventory : Actor native
{
    // GZDoom Inventory.ModifyDamage — BD `Damage_Control` overrides this.
    virtual void ModifyDamage(
        int damage,
        Name damageType,
        out int newDamage,
        bool passive,
        Actor inflictor = null,
        Actor source = null,
        int flags = 0) {}
}

class PlayerPawn : Actor native
{
}

class EventHandler : Object native
{
    native static EventHandler Find(class<EventHandler> type);
    virtual void OnRegister() {}
    virtual void PlayerEntered(PlayerEvent e) {}
    virtual void WorldThingSpawned(WorldEvent e) {}
    virtual void WorldLoaded(WorldEvent e) {}
    virtual void WorldTick() {}
}

class StaticEventHandler : EventHandler native
{
}

struct PlayerEvent
{
    int PlayerNumber;
}

struct WorldEvent
{
    Actor Thing;
}

struct PlayerInfo
{
    Actor mo;
    native int GetGender();
    native string GetUserName(int maxlen = 0);
}

class CVar native
{
    native static CVar GetCVar(string name, PlayerInfo player = null);
    native static CVar FindCVar(string name);
    native int GetInt();
    native double GetFloat();
    native string GetString();
}

// UZDoom post-process shader control (GLDEFS `HardwareShader PostProcess`);
// render-only, applied by the shell (docs/29 SH4). A struct in GZDoom; a
// static-only stub here.
class PPShader native
{
    native static void SetEnabled(string shaderName, bool enable);
    native static void SetUniform1f(string shaderName, string uniformName, double value);
    native static void SetUniform1i(string shaderName, string uniformName, int value);
}

class Wads native
{
    const int GLOBALNAMESPACE = 0;
    native static int CheckNumForFullName(string name);
    native static int CheckNumForName(string name, int ns = 0);
    native static int FindLump(string name, int start = 0);
    native static string ReadLump(int lump);
}

class Console native
{
    native static void Printf(string fmt, ...);
}

// Minimal LevelLocals stand-in (BD / GZDoom `level` global binds in the compiler).
class LevelLocals native
{
    native int time;
    native int maptime;
    native int totaltime;
    native string GetChecksum();
    // UZDoom 5.0 sky / fog (docs/36 UZ6).
    native void SetSkyFog(int fogdensity);
    native void SetThickFog(double distance, double multiplier);
    native void ChangeSkyMist(TextureID skymist, bool usemist = true, double yscale = 1.0);
}

class TexMan native
{
    const Type_Any = 0;
    native static TextureID CheckForTexture(string name, int usetype = Type_Any, int flags = 0);
}
