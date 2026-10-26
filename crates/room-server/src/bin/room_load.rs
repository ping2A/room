//! Load / soak harness for `room-server` (docs/22 §6 / §9).
//!
//! ```text
//! cargo run -p room-server --release --bin room-load -- --rooms 1000 --tics 35
//! cargo run -p room-server --release --bin room-load -- --rooms 5000 --players 2 --concurrency 128
//! cargo run -p room-server --release --bin room-load -- --rooms 256 --soak-secs 60 --sample-rss
//! ```

use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use room_proto::{
    BinaryMsg, ControlMsg, HelloMsg, NetTicCmd, TicCmdMsg, TicFrameMsg, encode_binary,
    encode_control, parse_binary, parse_control,
};
use room_server::{CreateRoomRequest, RoomRegistry, ServerConfig, bind_ephemeral};
use std::process::Command;
use std::time::Instant;
use tokio_tungstenite::tungstenite::Message;

#[derive(Parser, Debug)]
#[command(name = "room-load", about = "room-server multi-room load / soak")]
struct Args {
    /// Number of rooms to create.
    #[arg(long, default_value_t = 1_000)]
    rooms: usize,
    /// Tics to exchange per room after start.
    #[arg(long, default_value_t = 35)]
    tics: u32,
    /// Players per room (2..=8).
    #[arg(long, default_value_t = 2)]
    players: u8,
    /// Max rooms allowed on the in-process server.
    #[arg(long, default_value_t = 5_000)]
    max_rooms: usize,
    /// Concurrent room batches for connect / drive.
    #[arg(long, default_value_t = 64)]
    concurrency: usize,
    /// Extra wall-clock soak seconds after the tic pass (keeps sockets open).
    #[arg(long, default_value_t = 0)]
    soak_secs: u64,
    /// Fail if relay p99 latency exceeds this many microseconds (0 = no check).
    #[arg(long, default_value_t = 0)]
    max_p99_us: u64,
    /// Sample process RSS (KiB via `ps`) before/after and during soak.
    #[arg(long, default_value_t = false)]
    sample_rss: bool,
    /// Fail if RSS grows by more than this many KiB during soak (0 = no check).
    #[arg(long, default_value_t = 0)]
    max_rss_growth_kib: u64,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    if !(2..=8).contains(&args.players) {
        eprintln!("room-load: --players must be 2..=8");
        std::process::exit(2);
    }

    let started = Instant::now();
    let rss0 = if args.sample_rss { rss_kib() } else { None };
    let cfg = ServerConfig {
        bind: "127.0.0.1:0".into(),
        max_rooms: args.max_rooms.max(args.rooms + 10),
        max_conn_per_ip: 0,
        ..ServerConfig::default()
    };
    let registry = RoomRegistry::new(cfg);
    let (addr, _server) = match bind_ephemeral(registry.clone()).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("room-load: bind failed: {e}");
            std::process::exit(1);
        }
    };
    let base_ws = format!("ws://{addr}");
    let conns = args.rooms.saturating_mul(usize::from(args.players));
    println!(
        "room-load: rooms={} players={} conns≈{} tics={} concurrency={} base={}",
        args.rooms, args.players, conns, args.tics, args.concurrency, base_ws
    );
    if let Some(r) = rss0 {
        println!("room-load: rss_kib_start={r}");
    }

    let mut room_ids = Vec::with_capacity(args.rooms);
    for i in 0..args.rooms {
        match registry.create_room(CreateRoomRequest {
            map: "E1M1".into(),
            skill: 3,
            max_players: args.players,
            enable_verifier: false,
            visibility: "public".into(),
            game_mod: None,
            packs: Vec::new(),
            time_limit: None,
            game: None,
        }) {
            Ok(r) => room_ids.push(r.id),
            Err(e) => {
                eprintln!("room-load: create failed at {i}: {e}");
                std::process::exit(1);
            }
        }
        if (i + 1) % 500 == 0 {
            println!("room-load: created {}/{}", i + 1, args.rooms);
        }
    }

    let mut ok = 0usize;
    let mut failed = 0usize;
    let mut lat_us: Vec<u64> = Vec::new();
    let mut welcome_us: Vec<u64> = Vec::new();
    let batch = args.concurrency.max(1);
    for chunk in room_ids.chunks(batch) {
        let mut joins = Vec::with_capacity(chunk.len());
        for id in chunk {
            let base_ws = base_ws.clone();
            let id = id.clone();
            let tics = args.tics;
            let players = args.players;
            let soak = args.soak_secs;
            joins.push(tokio::spawn(async move {
                drive_room(&base_ws, &id, players, tics, soak).await
            }));
        }
        for j in joins {
            match j.await {
                Ok(Ok(result)) => {
                    ok += 1;
                    welcome_us.push(result.welcome_us);
                    lat_us.extend(result.rtt_us);
                }
                Ok(Err(e)) => {
                    failed += 1;
                    eprintln!("room-load: room failed: {e}");
                }
                Err(e) => {
                    failed += 1;
                    eprintln!("room-load: join error: {e}");
                }
            }
        }
        println!("room-load: progress ok={ok} failed={failed}");
    }

    if !welcome_us.is_empty() {
        welcome_us.sort_unstable();
        let p50 = welcome_us[welcome_us.len() / 2];
        let p99 = welcome_us[(welcome_us.len().saturating_mul(99) / 100).min(welcome_us.len() - 1)];
        println!(
            "room-load: create_to_welcome_us samples={} p50={p50} p99={p99} (docs/22 §6 target_p99<50000)",
            welcome_us.len()
        );
    }
    if !lat_us.is_empty() {
        lat_us.sort_unstable();
        let p50 = lat_us[lat_us.len() / 2];
        let p99 = lat_us[(lat_us.len().saturating_mul(99) / 100).min(lat_us.len() - 1)];
        println!(
            "room-load: client_rtt_us samples={} p50={p50} p99={p99} (WS round-trip; informational)",
            lat_us.len()
        );
        if args.max_p99_us > 0 && p99 > args.max_p99_us {
            eprintln!(
                "room-load: client RTT p99 {p99}us exceeds --max-p99-us {}",
                args.max_p99_us
            );
            std::process::exit(1);
        }
    }
    if let Some((n, avg, le300)) = registry.relay_latency_summary() {
        println!(
            "room-load: server_relay_us count={n} avg={avg} share_le_300us={le300:.3} (docs/22 §6 added relay; p50<300us p99<2000us)"
        );
    }

    let rss1 = if args.sample_rss { rss_kib() } else { None };
    if let (Some(a), Some(b)) = (rss0, rss1) {
        let growth = b.saturating_sub(a);
        let per_conn = if conns > 0 {
            (b.saturating_mul(1024)) / conns as u64
        } else {
            0
        };
        println!(
            "room-load: rss_kib_end={b} growth_kib={growth} bytes_per_conn≈{per_conn} (budget <49152)"
        );
        if args.max_rss_growth_kib > 0 && growth > args.max_rss_growth_kib {
            eprintln!(
                "room-load: RSS grew {growth} KiB > --max-rss-growth-kib {}",
                args.max_rss_growth_kib
            );
            std::process::exit(1);
        }
    }

    println!(
        "room-load: done ok={ok} failed={failed} elapsed={:.2}s",
        started.elapsed().as_secs_f64()
    );
    if failed > 0 {
        std::process::exit(1);
    }
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

struct RoomDrive {
    /// Connect+hello → first seat `welcome` (docs/22 §6 create→welcome).
    welcome_us: u64,
    /// Last TicCmd → TIC_FRAME as seen by the load client (WS RTT).
    rtt_us: Vec<u64>,
}

async fn drive_room(
    base_ws: &str,
    room_id: &str,
    players: u8,
    tics: u32,
    soak_secs: u64,
) -> Result<RoomDrive, String> {
    let url = format!("{base_ws}/v2/rooms/{room_id}/ws");
    let n = usize::from(players);
    let welcome_t0 = Instant::now();
    let mut socks: Vec<Ws> = Vec::with_capacity(n);
    for i in 0..n {
        let (ws, _) = tokio_tungstenite::connect_async(&url)
            .await
            .map_err(|e| format!("ws {i}: {e}"))?;
        if let tokio_tungstenite::MaybeTlsStream::Plain(tcp) = ws.get_ref() {
            let _ = tcp.set_nodelay(true);
        }
        socks.push(ws);
    }

    for (i, ws) in socks.iter_mut().enumerate() {
        let hello = ControlMsg::Hello(HelloMsg {
            ticket: room_id.to_owned(),
            client_version: "load".into(),
            abi_major: 1,
            iwad_fingerprint: "load".into(),
            name: Some(format!("p{i}")),
            resume_token: None,
        });
        ws.send(Message::Text(
            encode_control(&hello).map_err(|e| e.to_string())?.into(),
        ))
        .await
        .map_err(|e| e.to_string())?;
    }

    // First welcome closes create→welcome; remaining seats still need their own.
    wait_control(&mut socks[0], |m| matches!(m, ControlMsg::Welcome(_))).await?;
    let welcome_us = u64::try_from(welcome_t0.elapsed().as_micros()).unwrap_or(u64::MAX);
    for ws in socks.iter_mut().skip(1) {
        wait_control(ws, |m| matches!(m, ControlMsg::Welcome(_))).await?;
    }
    for ws in &mut socks {
        ws.send(Message::Text(
            encode_control(&ControlMsg::Ready {})
                .map_err(|e| e.to_string())?
                .into(),
        ))
        .await
        .map_err(|e| e.to_string())?;
    }
    for ws in &mut socks {
        wait_control(ws, |m| matches!(m, ControlMsg::Start { .. })).await?;
    }

    let mask = (1u8 << players).wrapping_sub(1);
    let mut samples = Vec::with_capacity(tics as usize);
    for tic in 0..tics {
        // Prefill all but the last seat, then time last input → TIC_FRAME.
        let last = n - 1;
        for (i, ws) in socks.iter_mut().enumerate().take(last) {
            let cmd = encode_binary(&BinaryMsg::TicCmd(TicCmdMsg {
                tic,
                cmd: NetTicCmd {
                    forward_move: i16::try_from(i).unwrap_or(0),
                    ..NetTicCmd::default()
                },
            }));
            ws.send(Message::Binary(cmd.into()))
                .await
                .map_err(|e| e.to_string())?;
        }
        let cmd = encode_binary(&BinaryMsg::TicCmd(TicCmdMsg {
            tic,
            cmd: NetTicCmd {
                forward_move: i16::try_from(last).unwrap_or(0),
                ..NetTicCmd::default()
            },
        }));
        let t0 = Instant::now();
        socks[last]
            .send(Message::Binary(cmd.into()))
            .await
            .map_err(|e| e.to_string())?;
        let frame = wait_frame(&mut socks[0]).await?;
        samples.push(u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX));
        if frame.tic != tic || frame.seat_mask != mask {
            return Err(format!(
                "bad frame tic={} mask={:#b} want={mask:#b}",
                frame.tic, frame.seat_mask
            ));
        }
        for ws in socks.iter_mut().skip(1) {
            let _ = wait_frame(ws).await?;
        }
    }

    if soak_secs > 0 {
        // Keep sockets alive; send a slow heartbeat tic stream.
        let end = Instant::now() + std::time::Duration::from_secs(soak_secs);
        let mut tic = tics;
        while Instant::now() < end {
            for (i, ws) in socks.iter_mut().enumerate() {
                let cmd = encode_binary(&BinaryMsg::TicCmd(TicCmdMsg {
                    tic,
                    cmd: NetTicCmd {
                        forward_move: i16::try_from(i).unwrap_or(0),
                        ..NetTicCmd::default()
                    },
                }));
                let _ = ws.send(Message::Binary(cmd.into())).await;
            }
            for ws in &mut socks {
                let _ = wait_frame(ws).await;
            }
            tic = tic.saturating_add(1);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
    Ok(RoomDrive {
        welcome_us,
        rtt_us: samples,
    })
}

async fn wait_control<F>(ws: &mut Ws, mut pred: F) -> Result<(), String>
where
    F: FnMut(ControlMsg) -> bool,
{
    for _ in 0..120 {
        match ws.next().await {
            Some(Ok(Message::Text(t))) => {
                if let Ok(m) = parse_control(&t) {
                    if pred(m) {
                        return Ok(());
                    }
                }
            }
            Some(Ok(_)) => {}
            Some(Err(e)) => return Err(e.to_string()),
            None => return Err("ws closed".into()),
        }
    }
    Err("timeout waiting control".into())
}

async fn wait_frame(ws: &mut Ws) -> Result<TicFrameMsg, String> {
    for _ in 0..120 {
        match ws.next().await {
            Some(Ok(Message::Binary(b))) => {
                if let Ok(BinaryMsg::TicFrame(f)) = parse_binary(&b) {
                    return Ok(f);
                }
            }
            Some(Ok(_)) => {}
            Some(Err(e)) => return Err(e.to_string()),
            None => return Err("ws closed".into()),
        }
    }
    Err("timeout waiting frame".into())
}

fn rss_kib() -> Option<u64> {
    let pid = std::process::id().to_string();
    let out = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    s.trim().parse().ok()
}
