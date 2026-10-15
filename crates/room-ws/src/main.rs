//! WebSocket packet-server relay for Room web coop (`docs/17` §1.3).
//!
//! Speaks the same `FTRL` relay framing as [`room_net::TcpRelayServer`], but
//! each datagram is one WebSocket binary message (no length prefix). All seats
//! are remote clients — seat 0 becomes the lobby arbitrator in the browser.
//!
//! ```text
//! cargo run -p room-ws -- --bind 127.0.0.1:8765 --players 2
//! ```

use room_net::{
    MSG_DATA, MSG_HELLO, MSG_VOICE, PeerId, pack_peer_join, pack_peer_leave, pack_reject,
    pack_server_data, pack_server_voice, pack_welcome, parse_relay,
};
use std::collections::BTreeMap;
use std::env;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;
use tungstenite::protocol::Message;
use tungstenite::{Error as WsError, accept};

fn main() {
    let (bind, players) = parse_args(env::args().skip(1));
    if let Err(e) = run(&bind, players) {
        eprintln!("room-ws: {e}");
        std::process::exit(1);
    }
}

fn parse_args(mut args: impl Iterator<Item = String>) -> (String, u8) {
    let mut bind = "127.0.0.1:8765".to_owned();
    let mut players = 2u8;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--bind" => {
                if let Some(v) = args.next() {
                    bind = v;
                }
            }
            "--players" => {
                if let Some(v) = args.next() {
                    players = v.parse().unwrap_or(2).clamp(2, 8);
                }
            }
            s if s.starts_with("--bind=") => {
                bind = s["--bind=".len()..].to_owned();
            }
            s if s.starts_with("--players=") => {
                players = s["--players=".len()..].parse().unwrap_or(2).clamp(2, 8);
            }
            "-h" | "--help" => {
                eprintln!(
                    "room-ws — WebSocket lockstep relay\n  --bind ADDR   (default 127.0.0.1:8765)\n  --players N   (2..=8, default 2)"
                );
                std::process::exit(0);
            }
            _ => {}
        }
    }
    (bind, players)
}

enum ClientEvent {
    Hello { seat: PeerId, name: String },
    Data { from: PeerId, payload: Vec<u8> },
    Voice { from: PeerId, payload: Vec<u8> },
    Gone { seat: PeerId },
}

fn run(bind: &str, max_players: u8) -> Result<(), String> {
    let listener = TcpListener::bind(bind).map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    eprintln!("room-ws: listening on ws://{bind}/  (waiting for {max_players} players)");

    let (tx, rx) = mpsc::channel::<ClientEvent>();
    let mut outboxes: BTreeMap<PeerId, Sender<Vec<u8>>> = BTreeMap::new();
    let mut names: BTreeMap<PeerId, String> = BTreeMap::new();

    loop {
        // Accept new TCP → WebSocket upgrades.
        match listener.accept() {
            Ok((stream, addr)) => {
                if outboxes.len() as u8 >= max_players {
                    eprintln!("room-ws: reject {addr} (lobby full)");
                    thread::spawn(move || {
                        if let Ok(mut ws) = accept(stream) {
                            let _ = ws.send(Message::Binary(pack_reject("lobby full").into()));
                            let _ = ws.close(None);
                        }
                    });
                } else if let Some(seat_id) =
                    (0..max_players).find(|id| !outboxes.contains_key(&PeerId(*id)))
                {
                    // Reuse the lowest free seat so refresh/reconnect works.
                    let seat = PeerId(seat_id);
                    let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>();
                    outboxes.insert(seat, out_tx);
                    let tx = tx.clone();
                    eprintln!("room-ws: {addr} → seat {}", seat.0);
                    thread::spawn(move || client_thread(stream, seat, max_players, tx, out_rx));
                } else {
                    eprintln!("room-ws: reject {addr} (no free seat)");
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.to_string()),
        }

        // Fan-out / bookkeeping.
        while let Ok(ev) = rx.try_recv() {
            match ev {
                ClientEvent::Hello { seat, name } => {
                    // Tell the newcomer who is already here.
                    for (&other, other_name) in &names {
                        if let Some(out) = outboxes.get(&seat) {
                            let _ = out.send(pack_peer_join(other, other_name));
                        }
                    }
                    names.insert(seat, name.clone());
                    eprintln!(
                        "room-ws: seat {} hello name={name:?} ({}/{})",
                        seat.0,
                        names.len(),
                        max_players
                    );
                    // Announce join to everyone else.
                    let packet = pack_peer_join(seat, &name);
                    for (&other, out) in &outboxes {
                        if other != seat {
                            let _ = out.send(packet.clone());
                        }
                    }
                }
                ClientEvent::Data { from, payload } => {
                    let packet = pack_server_data(from, &payload);
                    for (&seat, out) in &outboxes {
                        if seat != from {
                            let _ = out.send(packet.clone());
                        }
                    }
                }
                ClientEvent::Voice { from, payload } => {
                    // Cap voice fan-out size (PCM ~2 KiB / 20 ms frame).
                    if payload.len() > 8_192 {
                        continue;
                    }
                    let packet = pack_server_voice(from, &payload);
                    for (&seat, out) in &outboxes {
                        if seat != from {
                            let _ = out.send(packet.clone());
                        }
                    }
                }
                ClientEvent::Gone { seat } => {
                    outboxes.remove(&seat);
                    let left_name = names.remove(&seat).unwrap_or_else(|| "?".into());
                    eprintln!(
                        "room-ws: seat {} disconnected name={left_name:?} ({}/{})",
                        seat.0,
                        names.len(),
                        max_players
                    );
                    let packet = pack_peer_leave(seat);
                    for out in outboxes.values() {
                        let _ = out.send(packet.clone());
                    }
                }
            }
        }

        thread::sleep(Duration::from_millis(2));
    }
}

fn client_thread(
    stream: TcpStream,
    seat: PeerId,
    max_players: u8,
    tx: Sender<ClientEvent>,
    out_rx: Receiver<Vec<u8>>,
) {
    let mut ws = match accept(stream) {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("room-ws: accept seat {}: {e}", seat.0);
            let _ = tx.send(ClientEvent::Gone { seat });
            return;
        }
    };
    if let Err(e) = ws.get_ref().set_nonblocking(true) {
        eprintln!("room-ws: nonblocking seat {}: {e}", seat.0);
    }

    // Wait for HELLO then send WELCOME.
    let mut welcomed = false;
    loop {
        // Outbound first.
        while let Ok(bytes) = out_rx.try_recv() {
            if ws.send(Message::Binary(bytes.into())).is_err() {
                let _ = tx.send(ClientEvent::Gone { seat });
                return;
            }
        }

        match ws.read() {
            Ok(Message::Binary(bin)) => {
                let bytes = bin.as_ref();
                match parse_relay(bytes) {
                    Ok((MSG_HELLO, rest)) if !welcomed => {
                        let name = parse_hello_name(rest);
                        let _ = tx.send(ClientEvent::Hello {
                            seat,
                            name: name.clone(),
                        });
                        if ws
                            .send(Message::Binary(pack_welcome(seat, max_players).into()))
                            .is_err()
                        {
                            let _ = tx.send(ClientEvent::Gone { seat });
                            return;
                        }
                        welcomed = true;
                    }
                    Ok((MSG_DATA, rest)) if welcomed => {
                        let _ = tx.send(ClientEvent::Data {
                            from: seat,
                            payload: rest.to_vec(),
                        });
                    }
                    Ok((MSG_VOICE, rest)) if welcomed => {
                        let _ = tx.send(ClientEvent::Voice {
                            from: seat,
                            payload: rest.to_vec(),
                        });
                    }
                    Ok((MSG_HELLO, _)) if welcomed => {
                        // Ignore duplicate hellos.
                    }
                    _ => {}
                }
            }
            Ok(Message::Ping(p)) => {
                let _ = ws.send(Message::Pong(p));
            }
            Ok(Message::Close(_)) | Err(WsError::ConnectionClosed) => {
                let _ = tx.send(ClientEvent::Gone { seat });
                return;
            }
            Err(WsError::Io(ref e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2));
            }
            Err(WsError::Io(ref e)) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => {
                eprintln!("room-ws: seat {} read: {e}", seat.0);
                let _ = tx.send(ClientEvent::Gone { seat });
                return;
            }
            Ok(_) => {}
        }
    }
}

fn parse_hello_name(rest: &[u8]) -> String {
    if rest.len() < 2 {
        return "player".into();
    }
    let n = u16::from_le_bytes([rest[0], rest[1]]) as usize;
    let end = 2 + n.min(rest.len().saturating_sub(2));
    String::from_utf8_lossy(&rest[2..end]).into_owned()
}
