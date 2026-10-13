//! Optional WebRTC signalling server for netplay.
use ferrite_signal::{SignalState, serve};
use std::env;

fn main() {
    let bind = parse_bind(env::args().skip(1));
    if let Err(e) = serve(&bind, SignalState::new()) {
        eprintln!("ferrite-signal: {e}");
        std::process::exit(1);
    }
}

fn parse_bind(mut args: impl Iterator<Item = String>) -> String {
    let mut bind = "127.0.0.1:3535".to_owned();
    while let Some(a) = args.next() {
        if a == "--bind" {
            if let Some(v) = args.next() {
                bind = v;
            }
        } else if let Some(v) = a.strip_prefix("--bind=") {
            bind = v.to_owned();
        }
    }
    bind
}
