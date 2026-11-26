// testdata/zscript/conformance/probe.zs
class Probe
{
    static int Ret()     { return 5; }
    static int Add()     { int t = 2; t += 3; return t; }                          // 5
    static int Loop()    { int t = 0; for (int i = 0; i < 10; i++) t += i; return t; } // 45
    static int Wh()      { int i = 0; while (i < 7) i++; return i; }                // 7
    static int DoW()     { int i = 0; do { i += 2; } while (i < 7); return i; }     // 8
    static int Sw(int x) { switch (x) { case 1: return 10; case 2: return 20; default: return 99; } } // 20 / 99
    static int SwFall(int x) { int r = 0; switch (x) { case 1: r += 1; case 2: r += 2; break; case 3: r = 30; } return r; } // x=1 → 3
    static int Tern(int x) { return (x > 3) ? 1 : -1; }                             // 1 / -1
    static int If1(int x)  { if (x > 3) return 1; return -1; }                      // -1 for x=1
    static int Call()    { return Probe.Ret() + 1; }                                // 6
    static int Brk()     { int i; for (i = 0; i < 100; i++) { if (i == 4) break; } return i; } // 4
    static int Cont()    { int t = 0; for (int i = 0; i < 5; i++) { if (i == 2) continue; t += i; } return t; } // 8
    static int Fmt()     { String s = String.Format("%i", 42); return s.ToInt(10); } // 42
    static int Chr()     { String b = "AB"; return b.CharCodeAt(1); }               // 66
    static int Mid()     { String b = "hello"; return b.Mid(1, 3).Length(); }       // 3
    static int Arr()     { Array<int> a; a.Push(3); a.Push(4); a.Insert(0, 9); return a.Size() * 100 + a[0]; } // 309
    static int ArrDel()  { Array<int> a; a.Push(3); a.Push(4); a.Delete(0); return a[0]; } // 4
    static int ArrS()    { Array<string> a; a.Push("x"); a.Insert(0, "yz"); return a[0].Length() + a.Size() * 10; } // 22
    static int MapLen()  { ZC_IntStrMap m = new("ZC_IntStrMap"); m.insert(5, "abc"); return m.get(5).Length(); } // 3
    static int MapInt()  { ZC_IntStrMap m = new("ZC_IntStrMap"); m.insertInt(7, 9); m.insertInt(3, 1); return m.getInt(7) * 10 + m.getInt(3); } // 91
    static int MapDel()  { ZC_IntStrMap m = new("ZC_IntStrMap"); m.insert(1, "a"); m.remove(1); return m.checkKey(1); } // 0
    static int Bits()    { int v = 0x1234; return ((v >> 4) & 0xff) | (1 << 12); }   // 0x1123
    static int Neg()     { int temp = 200; return (temp > 127) ? (-temp) : temp; }   // -200
    static bool NotNot() { Actor a = null; return !!a; }                            // false
    static int Cast()    { Object o = new("ZC_IntStrMap"); ZC_IntStrMap m = ZC_IntStrMap(o); return m != null; } // 1
    static int CastBad() { Object o = new("ZC_IntStrMap"); EventHandler e = EventHandler(o); return e == null; } // 1
    static double Fl()   { double d = 1.5; return d * 2; }                          // 3.0
    static int Out()     { int r; Probe.SetOut(r); return r; }                      // 77
    static void SetOut(out int r) { r = 77; }
}
