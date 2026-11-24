//! Compile ACS, load it with `load_behavior`, run it in the VM and check
//! what the scripts did.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{AcsHost, AcsState, HudMessageArgs, load_behavior, start_script, tick_scripts};

#[derive(Default)]
struct Host {
    prints: Vec<String>,
    specials: Vec<(i32, [i32; 5])>,
    huds: Vec<HudMessageArgs>,
    callfuncs: Vec<(i32, Vec<i32>)>,
    special_result: i32,
}

impl AcsHost for Host {
    fn execute_special(&mut self, special: i32, args: [i32; 5], _activator: u64) -> i32 {
        self.specials.push((special, args));
        self.special_result
    }
    fn random(&mut self, lo: i32, _hi: i32) -> i32 {
        lo
    }
    fn timer(&self) -> i32 {
        0
    }
    fn player_count(&self) -> i32 {
        1
    }
    fn game_skill(&self) -> i32 {
        2
    }
    fn print(&mut self, text: &str, _bold: bool) {
        self.prints.push(text.to_owned());
    }
    fn tag_busy(&self, _tag: i32) -> bool {
        false
    }
    fn poly_busy(&self, _po: i32) -> bool {
        false
    }
    fn call_func(&mut self, func_index: i32, args: &[i32], _activator: u64) -> i32 {
        self.callfuncs.push((func_index, args.to_vec()));
        0
    }
    fn hud_message_ex(&mut self, msg: HudMessageArgs) {
        self.huds.push(msg);
    }
}

fn compile(src: &str) -> Vec<u8> {
    match compile_acs_standalone(src) {
        Ok(b) => b,
        Err(e) => panic!("compile failed: {e}\n{src}"),
    }
}

fn load(src: &str) -> AcsState {
    let module = load_behavior(&compile(src)).expect("compiled module loads");
    let mut state = AcsState::new();
    state.push_module(module);
    state
}

/// Run script `n` (and `ticks` tics after it); return the host.
fn run_n(src: &str, n: i32, ticks: usize) -> (AcsState, Host) {
    let mut state = load(src);
    let mut host = Host::default();
    start_script(&mut state, &mut host, n, &[0; 3], false);
    for _ in 0..ticks {
        tick_scripts(&mut state, &mut host);
    }
    (state, host)
}

fn prints(src: &str) -> Vec<String> {
    run_n(src, 1, 0).1.prints
}

fn err(src: &str) -> CompileError {
    compile_acs_standalone(src).expect_err("should not compile")
}

#[test]
fn hello_world() {
    let src = r#"
        #include "zcommon.acs"
        script 1 (void) { Print(s:"Hello ", d:2 + 3, s:"!"); }
    "#;
    assert_eq!(prints(src), ["Hello 5!"]);
}

#[test]
fn module_shape() {
    let bytes = compile(
        r#"script 7 OPEN NET { } script "Named" ENTER { } function int f(int a) { return a; }"#,
    );
    assert_eq!(&bytes[..4], b"ACSE");
    let m = load_behavior(&bytes).unwrap();
    assert_eq!(m.scripts.len(), 2);
    let s7 = m.find_script(7).unwrap();
    assert_eq!(s7.script_type, crate::ScriptType::Open);
    assert_eq!(s7.flags, crate::SCRIPTF_NET);
    let named = m.find_script(-1).unwrap();
    assert_eq!(named.script_type, crate::ScriptType::Enter);
    assert_eq!(m.script_names, ["Named"]);
    assert_eq!(m.functions.len(), 1);
    assert_eq!((m.functions[0].argc, m.functions[0].has_return), (1, 1));
    assert_eq!(m.function_names, ["f"]);
}

#[test]
fn precedence_and_operators() {
    let src = r#"
        script 1 (void) {
            Print(d:2 + 3 * 4, s:" ", d:(2 + 3) * 4, s:" ", d:10 - 4 - 3, s:" ", d:-7 / 2, s:" ", d:-7 % 3);
            int a = 6, b = 3;
            Print(d:a + b * 2, s:" ", d:a << 2 | 1, s:" ", d:a & b, s:" ", d:a ^ b, s:" ", d:~a, s:" ", d:!a);
            Print(d:a > b, d:a < b, d:a >= 6, d:a <= 5, d:a == 6, d:a != 6, d:a && 0, d:a || 0);
            Print(d:a > b ? 100 : 200, s:" ", d:a < b ? 100 : b == 3 ? 300 : 400);
            Print(d:0x10 + 'A', s:" ", d:1.5, s:" ", f:1.5, s:" ", d:-a);
        }
    "#;
    assert_eq!(
        prints(src),
        [
            "14 20 3 -3 -1",
            "12 25 2 5 -7 0",
            "10101001",
            "100 300",
            "81 98304 1.5 -6"
        ]
    );
}

#[test]
fn variables_and_assignment_operators() {
    let src = r#"
        int m = 5;
        int arr[4] = {1, 2, 3, 4};
        script 1 (void) {
            int x = 10;
            x += 5; x -= 3; x *= 2; x /= 4; x %= 4;
            Print(d:x);
            x = 12; x &= 10; x |= 1; x ^= 3; x <<= 2; x >>= 1;
            Print(d:x);
            m++; ++m; m--; m += 10;
            Print(d:m);
            arr[1] += 40; arr[2]++; arr[3] <<= 3; arr[0] = arr[1] + arr[2];
            Print(d:arr[0], s:",", d:arr[1], s:",", d:arr[2], s:",", d:arr[3]);
            int y = x++ + 1;
            int z = ++x;
            Print(d:y, s:" ", d:z, s:" ", d:x);
            int i = 0;
            int w = arr[i++];
            Print(d:w, s:" ", d:i, s:" ", d:(m = 3) + m);
        }
    "#;
    // x: 10+5=15, 12, 24, 6, 2 ; 12&10=8, |1=9, ^3=10, <<2=40, >>1=20
    assert_eq!(
        prints(src),
        ["2", "20", "16", "46,42,4,32", "21 22 22", "46 1 6"]
    );
}

#[test]
fn loops_and_switch() {
    let src = r#"
        script 1 (void) {
            int s = 0;
            for (int i = 1; i <= 10; i++) s += i;
            Print(d:s);
            int n = 0;
            while (n < 5) n++;
            until (n == 0) n--;
            Print(d:n);
            int k = 0;
            do { k += 2; } while (k < 7);
            do k--; until (k <= 3);
            Print(d:k);
            s = 0;
            for (int j = 0; j < 10; j++) {
                if (j == 2) continue;
                if (j == 6) break;
                s += j;
            }
            Print(d:s);
            for (int v = 0; v < 6; v++) {
                switch (v) {
                    case 0: Print(s:"zero"); break;
                    case 1:
                    case 2: Print(s:"one-two ", d:v); break;
                    case 3: Print(s:"three, falls");
                    case 4: Print(s:"four"); break;
                    default: Print(s:"other ", d:v);
                }
            }
            int depth = 0;
            for (int a = 0; a < 3; a++) {
                switch (a) { case 1: continue; default: depth += 10; }
                depth++;
            }
            Print(d:depth);
        }
    "#;
    assert_eq!(
        prints(src),
        [
            "55",
            "0",
            "3",
            "13",
            "zero",
            "one-two 1",
            "one-two 2",
            "three, falls",
            "four",
            "four",
            "other 5",
            "22"
        ]
    );
}

#[test]
fn if_else_chains() {
    let src = r#"
        function str grade(int v) {
            if (v >= 90) return "A";
            else if (v >= 80) return "B";
            else if (v >= 70) { return "C"; }
            return "F";
        }
        script 1 (void) { Print(s:grade(95), s:grade(85), s:grade(75), s:grade(5)); }
    "#;
    assert_eq!(prints(src), ["ABCF"]);
}

#[test]
fn functions_recursion_and_locals() {
    let src = r#"
        int calls;
        function int fact(int n) { calls++; if (n <= 1) return 1; return n * fact(n - 1); }
        function void bump(int by) { calls += by; }
        function int sum3(int a, int b, int c) { int t = a + b; return t + c; }
        function int later(void);
        script 1 (void) {
            Print(d:fact(6));
            bump(100);
            fact(3);
            Print(d:calls, s:" ", d:sum3(1, 2, 3), s:" ", d:after());
        }
        function int after(void) { return 42; }
    "#
    .replace("function int later(void);", "");
    assert_eq!(prints(&src), ["720", "109 6 42"]);
}

#[test]
fn local_and_map_arrays() {
    let src = r#"
        int grid[2][3] = { {1, 2, 3}, {4, 5, 6} };
        int sized[] = {7, 8, 9};
        int big[3][2][2];
        function int total(void) {
            int t[5] = {1, 1, 1};
            int s = 0;
            for (int i = 0; i < 5; i++) s += t[i];
            return s;
        }
        script 1 (void) {
            int loc[3][2] = { {10, 20}, {30, 40}, {50, 60} };
            loc[2][1] += 1;
            big[2][1][0] = 99;
            big[1][0][1] = 5;
            Print(d:grid[0][2], s:",", d:grid[1][0], s:",", d:grid[1][2], s:",", d:sized[2]);
            Print(d:loc[0][1], s:",", d:loc[1][0], s:",", d:loc[2][1], s:",", d:total());
            Print(d:big[2][1][0], s:",", d:big[1][0][1], s:",", d:big[0][0][0]);
        }
    "#;
    let (state, host) = run_n(src, 1, 0);
    assert_eq!(host.prints, ["3,4,6,9", "20,30,61,3", "99,5,0"]);
    // big is map variable 2: element [2][1][0] = 2*4 + 1*2 + 0 = 10.
    assert_eq!(state.map_array_elem(0, 2, 10), 99);
    assert_eq!(state.map_array_elem(0, 2, 5), 5);
}

#[test]
fn world_and_global_variables() {
    let src = r#"
        world int 3:visits;
        global int 7:score, 8:table[];
        script 1 (void) { visits++; score += 10; table[5] = 3; table[5] *= 7; }
        script 2 (void) { Print(d:visits, s:" ", d:score, s:" ", d:table[5]); }
    "#;
    let mut state = load(src);
    let mut host = Host::default();
    start_script(&mut state, &mut host, 1, &[0; 3], false);
    start_script(&mut state, &mut host, 1, &[0; 3], false);
    start_script(&mut state, &mut host, 2, &[0; 3], false);
    assert_eq!(host.prints, ["2 20 21"]);
}

#[test]
fn script_arguments_and_map_state() {
    let src = r#"
        int last;
        script 3 (int a, int b, int c) { last = a * 100 + b * 10 + c; }
    "#;
    let mut state = load(src);
    let mut host = Host::default();
    start_script(&mut state, &mut host, 3, &[1, 2, 3], false);
    assert_eq!(state.map_var(0, 0), 123);
}

#[test]
fn line_specials() {
    let src = r#"
        #include "zcommon.acs"
        script 1 (void) {
            Door_Open(5, 16);
            Floor_LowerByValue(1, 8, 64);
            Light_ChangeToValue(3, 200);
            Exit_Normal(0);
            int r = ACS_ExecuteWithResult(2, 7, 8);
            Print(d:r);
            SetLineSpecial(4, ACS_Execute, 9, 1);
        }
    "#;
    let mut state = load(src);
    let mut host = Host {
        special_result: 77,
        ..Host::default()
    };
    start_script(&mut state, &mut host, 1, &[0; 3], false);
    assert_eq!(
        host.specials,
        [
            (11, [5, 16, 0, 0, 0]),
            (20, [1, 8, 64, 0, 0]),
            (112, [3, 200, 0, 0, 0]),
            (243, [0; 5]),
            (84, [2, 7, 8, 0, 0])
        ]
    );
    assert_eq!(host.prints, ["77"]);
}

#[test]
fn hud_messages_and_strings() {
    let src = r#"
        #include "zcommon.acs"
        str greeting = "Hi\tthere";
        script 1 (void) {
            SetHudSize(320, 200, 0);
            HudMessage(s:greeting, s:" ", d:42; HUDMSG_FADEOUT, 5, CR_RED, 0.5, 0.25, 2.0, 0.75);
            HudMessageBold(s:"plain"; HUDMSG_PLAIN, 0, CR_GOLD, 1.0, 1.0, 3.0);
            str built = StrParam(s:"n=", d:7, c:'!');
            Print(s:built, s:" ", d:StrLen(built), s:" ", d:StrLen("abc"));
            Print(s:"line\nbreak", x:255, s:" ", b:5);
            Log(s:"logged");
            PrintBold(s:"bold");
        }
    "#;
    let mut state = load(src);
    let mut host = Host::default();
    start_script(&mut state, &mut host, 1, &[0; 3], false);
    assert_eq!(host.huds.len(), 2);
    let h = &host.huds[0];
    assert_eq!(
        (h.text.as_str(), h.type_flags, h.id, h.color),
        ("Hi\tthere 42", 1, 5, 6)
    );
    assert_eq!(
        (h.x, h.y, h.hold, h.opt.clone()),
        (0.5, 0.25, 2.0, vec![0.75])
    );
    assert_eq!((h.hud_w, h.hud_h), (320, 200));
    assert!(host.huds[1].bold && host.huds[1].opt.is_empty());
    assert_eq!(host.huds[1].text, "plain");
    assert_eq!(host.prints[0], "n=7! 4 3");
    assert_eq!(host.prints[1], "line\nbreakFF 101");
    assert!(host.prints.iter().any(|p| p == "bold"));
}

#[test]
fn named_scripts_and_callfuncs() {
    let src = r#"
        #include "zcommon.acs"
        script "Boss" (int hp) { Print(s:"boss ", d:hp); }
        script 1 (void) {
            ACS_NamedExecute("Boss", 0, 9);
            SetActivator(0, AAPTR_PLAYER1);
            int c = strcmp("a", "b");
        }
    "#;
    let mut state = load(src);
    let mut host = Host::default();
    start_script(&mut state, &mut host, -1, &[500, 0, 0], false);
    assert_eq!(host.prints, ["boss 500"]);
    start_script(&mut state, &mut host, 1, &[0; 3], false);
    // ACS_NamedExecute (CallFunc 39) starts the named script with its argument.
    assert_eq!(host.prints, ["boss 500", "boss 9"]);
}

#[test]
fn delays_and_waits_suspend() {
    let src = r#"
        int ticks;
        script 1 (void) {
            Print(s:"start");
            Delay(2);
            Print(s:"after delay");
            Delay(const:1);
            TagWait(4);
            ticks = Timer() + 1;
            Print(s:"end");
        }
    "#;
    let mut state = load(src);
    let mut host = Host::default();
    start_script(&mut state, &mut host, 1, &[0; 3], false);
    assert_eq!(host.prints, ["start"]);
    for _ in 0..6 {
        tick_scripts(&mut state, &mut host);
    }
    assert_eq!(host.prints, ["start", "after delay", "end"]);
    assert_eq!(state.map_var(0, 0), 1);
}

#[test]
fn restart_and_terminate() {
    let src = r#"
        int n;
        script 1 (void) {
            n++;
            if (n < 3) restart;
            Print(d:n);
            terminate;
            Print(s:"never");
        }
    "#;
    assert_eq!(prints(src), ["3"]);
}

#[test]
fn defines_and_includes() {
    let mut include = |name: &str| match name {
        "lib/consts.acs" => Some(
            "#define BASE 40\n#define TWICE (BASE * 2)\n#define MSG \"from include\"\n".to_owned(),
        ),
        "helpers.acs" => Some("function int add2(int x) { return x + TWO; }".to_owned()),
        _ => None,
    };
    let src = r#"
        #include "zcommon.acs"
        #include "lib/consts.acs"
        #define TWO 2
        #include "helpers.acs"
        #include "helpers.acs"
        script BASE + 2 (void) { Print(d:TWICE + 1, s:" ", s:MSG, s:" ", d:add2(1)); }
    "#;
    let bytes = compile_acs(src, "SCRIPTS", &mut include).expect("compiles");
    let mut state = AcsState::new();
    state.push_module(load_behavior(&bytes).unwrap());
    let mut host = Host::default();
    start_script(&mut state, &mut host, 42, &[0; 3], false);
    assert_eq!(host.prints, ["81 from include 3"]);
}

#[test]
fn special_declarations() {
    let src = r#"
        special 250:My_Special(1, 3), -9999:My_Func(2);
        script 1 (void) { My_Special(4, 5); My_Func(1, 2); }
    "#;
    let (_, host) = run_n(src, 1, 0);
    assert_eq!(host.specials, [(250, [4, 5, 0, 0, 0])]);
    assert_eq!(host.callfuncs, [(9999, vec![1, 2])]);
}

#[test]
fn comments_and_case_insensitivity() {
    let src = "// line comment\n/* block\n comment */ SCRIPT 1 (VOID) { INT X = 3; /* inline */ print(D:x * 2); }";
    assert_eq!(prints(src), ["6"]);
}

/// Every internal function pops exactly its arguments and pushes its result:
/// after the call, a special still sees the marker pushed before it.
#[test]
fn builtins_keep_the_stack_balanced() {
    let skip = [
        "delay",
        "tagwait",
        "polywait",
        "scriptwait",
        "namedscriptwait",
        "changelevel",
        "restart",
    ];
    for b in super::builtins::BUILTINS {
        if skip.contains(&b.name.to_ascii_lowercase().as_str()) {
            continue;
        }
        let args = vec!["1"; usize::from(b.argc)].join(", ");
        let call = if b.returns {
            format!("int r = {}({args});", b.name)
        } else {
            format!("{}({args});", b.name)
        };
        let src = format!("script 1 (void) {{ int marker = 1234; {call} Door_Open(marker, 2); }}");
        let (_, host) = run_n(&src, 1, 0);
        assert_eq!(
            host.specials.last(),
            Some(&(11, [1234, 2, 0, 0, 0])),
            "{} broke the stack",
            b.name
        );
    }
}

#[test]
fn optional_arguments_default() {
    let src = r#"script 1 (void) { int t = Spawn("Imp", 1.0, 2.0, 3.0); SetLineSpecial(1, 80); Door_Open(t, 1); }"#;
    let (_, host) = run_n(src, 1, 0);
    assert_eq!(host.specials.last().map(|s| s.0), Some(11));
}

#[test]
fn errors_point_at_the_line() {
    let e = err("script 1 (void)\n{\n  Prnt(1);\n}");
    assert_eq!((e.line, e.message.as_str()), (3, "unknown function prnt"));
    assert_eq!(e.to_string(), "SCRIPTS:3: unknown function prnt");

    let e = err("script 1 (void)\n{\n  Prnt(s:\"x\");\n}");
    assert_eq!(e.line, 3);
    assert!(
        e.message.starts_with("unknown function prnt"),
        "{}",
        e.message
    );

    let e = err("script 1 (void) {\n int x = 1\n x++;\n}");
    assert_eq!(e.line, 3);
    assert!(e.message.contains("expected ';'"), "{}", e.message);

    assert!(
        err("script 1 (void) { Door_Open(1); }")
            .message
            .contains("takes 2 to 3 arguments")
    );
    assert!(
        err("script 1 (void) { y = 2; }")
            .message
            .contains("y isn't declared")
    );
    assert!(
        err("script 1 (void) { } script 1 (void) { }")
            .message
            .contains("defined twice")
    );
    assert!(
        err("script 1 (void) { break; }")
            .message
            .contains("break outside")
    );
    assert!(
        err("function void f(void) { } script 1 (void) { int x = f(); }")
            .message
            .contains("returns nothing")
    );
    assert!(
        err("script 1 (void) { Print(s:\"a\"; 1); }")
            .message
            .contains("no arguments after")
    );
    assert!(err("#import \"lib.acs\"").message.contains("#import"));
    assert!(
        err("script 1 (void) { \"unterminated }")
            .message
            .contains("string")
    );
    assert!(
        err("script 1 (void) { int a[2] = {1, 2, 3}; }")
            .message
            .contains("3 values")
    );
    assert!(
        err("script 1 (void) { switch (1) { case 1: case 1: break; } }")
            .message
            .contains("twice")
    );
    assert!(
        err("script 1 OPEN (int a) { }")
            .message
            .contains("arguments")
    );

    let mut none = |_: &str| None;
    let e = compile_acs("#include \"missing.acs\"", "MAP01", &mut none).unwrap_err();
    assert_eq!(
        (e.file.as_str(), e.message.as_str()),
        ("MAP01", "can't find #include \"missing.acs\"")
    );
    let mut inc = |_: &str| Some("\n\nscript 1 (void) { oops(); }".to_owned());
    let e = compile_acs("#include \"bad.acs\"", "MAP01", &mut inc).unwrap_err();
    assert_eq!((e.file.as_str(), e.line), ("bad.acs", 3));
}

#[test]
fn create_translation_ranges() {
    let src = r#"
        script 1 (void) {
            int m = 55;
            CreateTranslation(97, 160:167=192:207, 64:79=[255, 0, 0]:[0, 0, 255], 1:2=%[0.5, 0.5, 0.5]:[1.0, 1.0, 1.0], 3:4=#[10, 20, 30], 5:6=@40[1, 2, 3]);
            CreateTranslation(98);
            Door_Open(m, 1);
        }
        script 2 DISCONNECT (int player) { }
    "#;
    let (_, host) = run_n(src, 1, 0);
    assert_eq!(host.specials, [(11, [55, 1, 0, 0, 0])]);
}

/// Broken source (every prefix of a real program, and byte noise) gives an
/// error, never a panic.
#[test]
fn broken_source_never_panics() {
    let src = r#"
        #include "zcommon.acs"
        #define N 3
        int grid[2][N] = { {1, 2, 3}, {4, 5, 6} };
        world int 1:w;
        function int f(int a) { return a > 1 ? a * f(a - 1) : 1; }
        script "s" (int x) { switch (x) { case 1: Print(s:"one"); break; default: w++; } }
        script 1 OPEN { for (int i = 0; i < N; i++) { HudMessage(d:grid[1][i]; 0, 1, CR_RED, 0.5, 0.5, 1.0); Delay(1); } }
        script 2 (void) { CreateTranslation(1, 0:1=2:3); do { w--; } until (w <= 0); }
    "#;
    for end in 0..=src.len() {
        if src.is_char_boundary(end) {
            let _ = compile_acs_standalone(&src[..end]);
        }
    }
    let mut seed = 0x1234_5678u32;
    for _ in 0..400 {
        let mut bytes = src.as_bytes().to_vec();
        for _ in 0..4 {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let at = (seed >> 8) as usize % bytes.len();
            bytes[at] = b"{}()[];:,=+-*/%<>!~&|^?#@\"'0aZ_ \n.\\"[(seed >> 20) as usize % 35];
        }
        let text = String::from_utf8_lossy(&bytes);
        if let Ok(b) = compile_acs_standalone(&text) {
            assert!(
                load_behavior(&b).is_ok(),
                "compiled output must load:\n{text}"
            );
        }
    }
}

const LIB: &str = r#"
    #library "MYLIB"
    #include "zcommon.acs"
    #libdefine LIB_STEP 10
    int counter = 5;
    str greeting = "hello lib";
    int table[3] = {10, 20, 30};
    str names[2] = {"a", "bee"};
    function int bump(int by) { counter += by; return counter; }
    function str greet(void) { return greeting; }
    script 900 (void) { Print(s:"lib script ", d:counter); }
"#;

/// A map script `#import`s a library: calls its functions, reads and writes
/// its variables and arrays (strings included), and the library still runs
/// its own scripts on the shared values.
#[test]
fn libraries_link_by_name() {
    let mut inc = |name: &str| (name.eq_ignore_ascii_case("mylib.acs")).then(|| LIB.to_owned());
    let lib = compile_acs_module(LIB, "mylib.acs", &mut |_| None).expect("library compiles");
    assert_eq!(lib.library.as_deref(), Some("MYLIB"));
    let map_src = r#"
        #import "mylib.acs"
        script 1 (void) {
            Print(d:bump(LIB_STEP), s:" ", s:greet(), s:" ", d:table[2], s:" ", s:names[1], s:" ", s:greeting, s:" ", d:counter);
            table[0] = 99;
            counter = 40;
        }
        script 2 (void) { Print(d:table[0]); }
    "#;
    let map = compile_acs_module(map_src, "SCRIPTS", &mut inc).expect("map compiles");
    assert_eq!(
        (map.library, map.imports.clone()),
        (None, vec!["MYLIB".to_owned()])
    );
    let m = load_behavior(&map.bytes).unwrap();
    assert_eq!(m.libraries, ["MYLIB"]);
    // Imported functions have no code in the map module.
    assert!(m.functions.iter().all(|f| f.address == 0));
    let mut state = AcsState::new();
    state.push_module_named(m, "MAP01");
    state.push_module_named(load_behavior(&lib.bytes).unwrap(), "MYLIB");
    let mut host = Host::default();
    start_script(&mut state, &mut host, 1, &[0; 3], false);
    start_script(&mut state, &mut host, 2, &[0; 3], false);
    start_script(&mut state, &mut host, 900, &[0; 3], false);
    assert_eq!(
        host.prints,
        ["15 hello lib 30 bee hello lib 15", "99", "lib script 40"]
    );
}

#[test]
fn import_errors() {
    let mut none = |_: &str| None;
    let e = compile_acs("#import \"gone.acs\"", "SCRIPTS", &mut none).unwrap_err();
    assert!(e.message.contains("can't find #import"), "{}", e.message);
    let mut plain = |_: &str| Some("int x;".to_owned());
    let e = compile_acs("#import \"plain.acs\"", "SCRIPTS", &mut plain).unwrap_err();
    assert!(e.message.contains("#library"), "{}", e.message);
    assert_eq!(e.file, "plain.acs");
}

#[test]
fn known_names_cover_the_tables() {
    let names = acs_known_names();
    let find = |n: &str| {
        names
            .iter()
            .find(|k| k.name.eq_ignore_ascii_case(n))
            .cloned()
    };
    let door = find("Door_Open").unwrap();
    assert_eq!(
        (door.kind, door.number, door.args),
        ("special", 11, Some((2, 3)))
    );
    let delay = find("Delay").unwrap();
    assert_eq!(
        (delay.name, delay.kind, delay.args),
        ("Delay", "function", Some((1, 1)))
    );
    assert_eq!(find("ThingCountName").unwrap().name, "ThingCountName");
    assert_eq!(find("strcmp").unwrap().kind, "callfunc");
    assert_eq!(find("HudMessage").unwrap().kind, "print");
    assert_eq!(find("CR_RED").unwrap().number, 6);
    // Every name it offers compiles as a call or a value.
    for k in names
        .iter()
        .filter(|k| k.kind == "function" || k.kind == "special")
        .take(400)
    {
        let (lo, _) = k.args.unwrap();
        let args = vec!["0"; usize::from(lo)].join(", ");
        compile_acs_standalone(&format!("script 1 (void) {{ {}({args}); }}", k.name))
            .unwrap_or_else(|e| panic!("{}: {e}", k.name));
    }
}
