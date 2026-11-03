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
