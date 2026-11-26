// String methods Brutal Doom and the §14.6 probe call.
version "4.12.0"

class String native
{
    native static String Format(string fmt, ...);
    native int Length();
    native int CharCodeAt(int pos);
    native int ByteAt(int pos);
    native String Mid(int pos, int len = -1);
    native String Left(int len);
    native String Right(int len);
    native int ToInt(int base = 10);
    native int IndexOf(string inner, int start = 0);
}
