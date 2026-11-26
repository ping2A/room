//! DECORATE frame-name parsing for melee states (backslash and bracket frames).

// Test helpers: a failed setup step should panic the test.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_decorate::parse_decorate;
#[test]
fn backslash_and_bracket_frames() {
    let src = "actor T {\nStates {\nSpawn:\n\t\"####\" \"\\\\\" 12\n\t\"####\" \"[]\" 1\n\t\"####\" # 1\n\tStop\n}\n}\n";
    // Also the BD form with single backslash before closer (escapes disabled):
    let src2 =
        "actor T {\nStates {\nSpawn:\n\t\"####\" \"\\\" 12\n\t\"####\" \"[]\" 1\n\tStop\n}\n}\n";
    parse_decorate(src).expect("escaped frames");
    parse_decorate(src2).expect("bd backslash frames");
}
