//! UDMF lexer throughput smoke check (≥ 100 MB/s accept target).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ferrite_map::{UdmfLexer, UdmfTok};
use std::time::Instant;

fn lex_all(text: &str) -> usize {
    let mut lex = UdmfLexer::new(text);
    let mut n = 0usize;
    loop {
        match lex.next_tok() {
            Ok(UdmfTok::Eof) => break,
            Ok(_) => n += 1,
            Err(e) => panic!("lex error: {e}"),
        }
    }
    n
}

#[test]
fn udmf_lexer_throughput() {
    // Repeat a small valid-ish token soup until ~8 MiB.
    let chunk = "namespace = \"zdoom\";\nthing { x = 0.0; y = 0.0; type = 1; }\nvertex { x = 1.5; y = -2.25; }\nlinedef { v1 = 0; v2 = 1; sidefront = 0; }\n";
    let target = 8 * 1024 * 1024;
    let mut buf = String::with_capacity(target + chunk.len());
    while buf.len() < target {
        buf.push_str(chunk);
    }
    let n = buf.len();

    // Warm-up.
    let _ = lex_all(&buf);

    let t0 = Instant::now();
    let tokens = lex_all(&buf);
    let elapsed = t0.elapsed();
    let secs = elapsed.as_secs_f64().max(1e-9);
    let mibs = (n as f64 / (1024.0 * 1024.0)) / secs;
    eprintln!("UDMF lex: {n} bytes, {tokens} tokens in {elapsed:?} → {mibs:.1} MiB/s");
    assert!(tokens > 0);
    // Roadmap accept: ≥ 100 MiB/s (local ~400+; leave headroom for busy CI).
    assert!(
        mibs >= 100.0,
        "UDMF lexer too slow: {mibs:.1} MiB/s (want ≥ 100)"
    );
}
