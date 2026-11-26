// Excerpt of GZDoom `wadsrc/static/zscript/events.zs` shapes (ZS0 corpus).
struct NetworkCommand native play version("4.12")
{
    native readonly int Player;
    native readonly Name Command;
    native int ReadInt();
    native void ReadIntArray(out Array<int> values, ENetCmd intSize = NET_INT);
}

enum ENetCmd
{
    NET_INT8 = 1,
    NET_INT,
}

class StaticEventHandler : Object native play version("2.4")
{
    clearscope static native StaticEventHandler Find(class<StaticEventHandler> type);
    version("4.12") virtual void NetworkCommandProcess(NetworkCommand cmd) {}
    version("4.12") clearscope static native vararg bool SendNetworkCommand(Name cmd, ...);
    virtual ui bool UiProcess(UiEvent e) { return false; }
    native readonly int Order;
    native readonly int Args[3];
}

class EventHandler : StaticEventHandler native version("2.4")
{
    clearscope static native void SendNetworkEvent(String name, int arg1 = 0, int arg2 = 0, int arg3 = 0);
}
