//! Ratatui drawing for the admin console.

use crate::api::RoomInspect;
use crate::app::{App, Tab};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, Paragraph, Row, Sparkline, Table, Wrap,
};

const ACCENT: Color = Color::Rgb(200, 60, 50);
const MUTED: Color = Color::DarkGray;
const OK: Color = Color::Green;
const WARN: Color = Color::Yellow;
const BAD: Color = Color::Red;
const INFO: Color = Color::Cyan;

/// Draw one frame.
pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(1),
        ])
        .split(area);

    draw_header(frame, chunks[0], app);
    draw_tabs(frame, chunks[1], app);
    match app.tab {
        Tab::Dashboard => draw_dashboard(frame, chunks[2], app),
        Tab::Network => draw_network(frame, chunks[2], app),
        Tab::Rooms => draw_rooms(frame, chunks[2], app),
        Tab::Detail => draw_detail(frame, chunks[2], app),
        Tab::Bans => draw_bans(frame, chunks[2], app),
        Tab::Audit => draw_audit(frame, chunks[2], app),
        Tab::Mods => draw_mods(frame, chunks[2], app),
    }
    draw_status(frame, chunks[3], app);

    if let Some(prompt) = &app.prompt {
        draw_prompt(frame, area, prompt.title.as_str(), prompt.value.as_str());
    }
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let drain = if app.summary.draining || app.metrics.draining != 0 {
        Span::styled(" DRAINING ", Style::default().fg(Color::Black).bg(WARN))
    } else {
        Span::styled(" live ", Style::default().fg(OK))
    };
    let mean = app
        .metrics
        .mean_relay_us()
        .map(|u| format!("{u}µs relay"))
        .unwrap_or_else(|| "relay —".into());
    let title = Line::from(vec![
        Span::styled(
            " ROOM ADMIN ",
            Style::default()
                .fg(Color::Black)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        drain,
        Span::raw(" "),
        Span::styled(&app.url, Style::default().fg(MUTED)),
        Span::raw("  "),
        Span::raw(format!(
            "node {} · {} rms · {} ver · {:.0} msg/s · {:.0} fr/s · {}",
            app.summary.node_id,
            app.summary.rooms,
            app.summary.verified_rooms,
            app.rates.ws_msgs_per_s,
            app.rates.tic_frames_per_s,
            mean
        )),
    ]);
    let help = Line::from(vec![
        key("Tab"),
        Span::raw("tabs  "),
        key("↑↓"),
        Span::raw("select  "),
        key("Enter"),
        Span::raw("detail  "),
        key("c"),
        Span::raw("close  "),
        key("n"),
        Span::raw("notice  "),
        key("k"),
        Span::raw("kick  "),
        key("r"),
        Span::raw("refresh  "),
        key("q"),
        Span::raw("quit"),
    ]);
    frame.render_widget(
        Paragraph::new(vec![title, help]).block(Block::default().borders(Borders::BOTTOM)),
        area,
    );
}

fn key(label: &str) -> Span<'_> {
    Span::styled(
        format!(" {label} "),
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    )
}

fn draw_tabs(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let spans: Vec<Span<'_>> = Tab::ALL
        .into_iter()
        .flat_map(|t| {
            let style = if t == app.tab {
                Style::default()
                    .fg(Color::Black)
                    .bg(ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(MUTED)
            };
            [
                Span::styled(format!(" {} ", t.label()), style),
                Span::raw(" "),
            ]
        })
        .collect();
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_dashboard(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(4)])
        .split(area);

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(chunks[0]);

    let m = &app.metrics;
    let cards = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                "CLUSTER",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                "   rooms {:>4}   verified {:>4}   draining {:>2}",
                m.rooms, m.verified_rooms, m.draining
            )),
        ]),
        Line::from(format!(
            "WS msgs {:>12}   bytes {:>14}   viol {:>8}",
            m.ws_msgs,
            format_bytes(m.ws_bytes),
            m.violations
        )),
        Line::from(format!(
            "tic frames {:>9}   mean relay {:>10}   samples {:>8}",
            m.tic_frames,
            m.mean_relay_us()
                .map(|u| format!("{u} µs"))
                .unwrap_or_else(|| "—".into()),
            m.relay_latency_count
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "RATES  ",
                Style::default().fg(INFO).add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                "{:>7.1} msg/s   {:>10}/s   {:>7.1} frames/s",
                app.rates.ws_msgs_per_s,
                format_bytes(app.rates.ws_bytes_per_s as u64),
                app.rates.tic_frames_per_s
            )),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Enter opens Detail for the selected room · 2 Net for relay histogram",
            Style::default().fg(MUTED),
        )),
    ])
    .block(Block::default().borders(Borders::ALL).title(" Overview "));
    frame.render_widget(cards, top[0]);

    draw_relay_panel(frame, top[1], app, " Relay latency ");
    draw_room_table(frame, chunks[1], app, " Rooms ");
}

fn draw_network(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Length(7),
            Constraint::Min(5),
        ])
        .split(area);

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[0]);

    draw_relay_panel(frame, top[0], app, " Cluster relay histogram ");
    draw_net_totals(frame, top[1], app);

    draw_selected_room_net(frame, chunks[1], app);
    draw_room_net_table(frame, chunks[2], app);
}

fn draw_relay_panel(frame: &mut Frame<'_>, area: Rect, app: &App, title: &str) {
    let labels = ["≤100µs", "≤300µs", "≤1ms", "≤2ms", "≤5ms", "+Inf"];
    let deltas = app.metrics.relay_bucket_deltas();
    let max = deltas.iter().copied().max().unwrap_or(1).max(1);
    let mut lines = vec![Line::from(format!(
        "mean {} · n={}",
        app.metrics
            .mean_relay_us()
            .map(|u| format!("{u} µs"))
            .unwrap_or_else(|| "—".into()),
        app.metrics.relay_latency_count
    ))];
    for (label, count) in labels.iter().zip(deltas.iter()) {
        let width = area.width.saturating_sub(18) as usize;
        let filled = ((*count as f64 / max as f64) * width as f64).round() as usize;
        let bar: String = std::iter::repeat_n('█', filled)
            .chain(std::iter::repeat_n('░', width.saturating_sub(filled)))
            .collect();
        let style = if *label == "+Inf" && *count > 0 {
            Style::default().fg(BAD)
        } else if *label == "≤5ms" && *count > 0 {
            Style::default().fg(WARN)
        } else {
            Style::default().fg(OK)
        };
        lines.push(Line::from(vec![
            Span::raw(format!("{label:<7} {count:>6} ")),
            Span::styled(bar, style),
        ]));
    }
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title)),
        area,
    );
}

fn draw_net_totals(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let m = &app.metrics;
    let spark_data: Vec<u64> = m.relay_bucket_deltas().into();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(2)])
        .split(area);

    let body = Paragraph::new(vec![
        Line::from(vec![Span::styled(
            "THROUGHPUT",
            Style::default().fg(INFO).add_modifier(Modifier::BOLD),
        )]),
        Line::from(format!(
            "  {:>8.1} msg/s     {:>10}/s",
            app.rates.ws_msgs_per_s,
            format_bytes(app.rates.ws_bytes_per_s as u64)
        )),
        Line::from(format!(
            "  {:>8.1} frames/s  viol {:>8}",
            app.rates.tic_frames_per_s, m.violations
        )),
    ])
    .block(Block::default().borders(Borders::ALL).title(" Rates "));
    frame.render_widget(body, chunks[0]);

    let spark = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Bucket shape "),
        )
        .data(&spark_data)
        .style(Style::default().fg(ACCENT));
    frame.render_widget(spark, chunks[1]);
}

fn draw_selected_room_net(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(ins) = app.net_room_highlight() else {
        frame.render_widget(
            Paragraph::new("Select a room below (↑↓) — Enter opens Detail with seats + RTT.")
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Selected room network "),
                ),
            area,
        );
        return;
    };
    frame.render_widget(
        Paragraph::new(room_net_lines(ins)).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Room {} network ", truncate(&ins.id, 12))),
        ),
        area,
    );
}

fn room_net_lines(ins: &RoomInspect) -> Vec<Line<'static>> {
    let rtts: Vec<u16> = ins.seats.iter().filter_map(|s| s.rtt_ms).collect();
    let rtt_summary = if rtts.is_empty() {
        "no pings yet".into()
    } else {
        let min = rtts.iter().copied().min().unwrap_or(0);
        let max = rtts.iter().copied().max().unwrap_or(0);
        let avg =
            u64::from(rtts.iter().map(|v| u32::from(*v)).sum::<u32>()) / rtts.len().max(1) as u64;
        format!("min {min}  avg {avg}  max {max} ms")
    };
    let waiting = if ins.waiting_seats.is_empty() {
        "none".into()
    } else {
        ins.waiting_seats
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",")
    };
    let desync_style = if ins.desync_count > 0 {
        Style::default().fg(BAD).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(OK)
    };
    vec![
        Line::from(format!(
            "game {} · proto {} · tic {} · uptime {}s · stall {}ms",
            if ins.game.is_empty() {
                "—"
            } else {
                &ins.game
            },
            if ins.protocol.is_empty() {
                "—"
            } else {
                &ins.protocol
            },
            ins.next_tic,
            ins.uptime_secs,
            ins.stall_fill_ms
        )),
        Line::from(format!(
            "seats {}/{} · specs {} · last relay {} · checksum pending {}",
            ins.occupied,
            ins.max_players,
            ins.spectators.len(),
            ins.last_relay_us
                .map(|u| format!("{u} µs"))
                .unwrap_or_else(|| "—".into()),
            ins.pending_checksum_tics
        )),
        Line::from(vec![
            Span::raw(format!(
                "RTT  {rtt_summary}   waiting seats [{waiting}]   desyncs "
            )),
            Span::styled(ins.desync_count.to_string(), desync_style),
        ]),
        Line::from({
            let stalled: Vec<String> = ins
                .seats
                .iter()
                .filter(|s| s.miss_streak > 0)
                .map(|s| format!("{}:{}", s.seat, s.miss_streak))
                .collect();
            if stalled.is_empty() {
                Span::styled("miss streaks: none", Style::default().fg(MUTED))
            } else {
                Span::styled(
                    format!("miss streaks: {}", stalled.join(" ")),
                    Style::default().fg(WARN),
                )
            }
        }),
    ]
}

fn draw_room_net_table(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let header = Row::new(vec!["id", "map", "state", "seats", "proto*", "mod"])
        .style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD));
    let rows = app.rooms.iter().enumerate().map(|(i, r)| {
        let state = if r.started { "play" } else { "lobby" };
        let style = if i == app.room_sel {
            Style::default()
                .bg(Color::Rgb(40, 20, 20))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        // Protocol for non-selected rooms is unknown without inspect; show started hint.
        let proto = if r.started { "live" } else { "lobby" };
        Row::new(vec![
            truncate(&r.id, 10),
            r.map.clone(),
            state.into(),
            format!("{}/{}", r.occupied, r.max_players),
            proto.into(),
            r.game_mod.clone().unwrap_or_else(|| "—".into()),
        ])
        .style(style)
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Length(7),
            Constraint::Length(6),
            Constraint::Min(8),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Rooms (* select for full network) "),
    );
    frame.render_widget(table, area);
}

fn draw_rooms(frame: &mut Frame<'_>, area: Rect, app: &App) {
    draw_room_table(frame, area, app, " Rooms  (Enter = detail) ");
}

fn draw_room_table(frame: &mut Frame<'_>, area: Rect, app: &App, title: &str) {
    let header = Row::new(vec![
        "id", "map", "sk", "seats", "state", "mod", "packs", "node", "flags",
    ])
    .style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD));
    let rows = app.rooms.iter().enumerate().map(|(i, r)| {
        let state = if r.started { "playing" } else { "lobby" };
        let packs = if r.packs.is_empty() {
            "—".into()
        } else {
            r.packs.join(",")
        };
        let flags = {
            let mut f = Vec::new();
            if r.enable_verifier {
                f.push("ver");
            }
            if r.game_mod.is_some() {
                f.push("mod");
            }
            if f.is_empty() {
                "—".into()
            } else {
                f.join(",")
            }
        };
        let style = if i == app.room_sel {
            Style::default()
                .bg(Color::Rgb(40, 20, 20))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        Row::new(vec![
            truncate(&r.id, 10),
            r.map.clone(),
            r.skill.to_string(),
            format!("{}/{}", r.occupied, r.max_players),
            state.into(),
            r.game_mod.clone().unwrap_or_else(|| "—".into()),
            packs,
            if r.node_id.is_empty() {
                "—".into()
            } else {
                r.node_id.clone()
            },
            flags,
        ])
        .style(style)
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(3),
            Constraint::Length(7),
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Min(6),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(table, area);
}

fn draw_detail(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(ins) = &app.inspect else {
        frame.render_widget(
            Paragraph::new("No room selected. Pick one on the Rooms or Net tab.")
                .block(Block::default().borders(Borders::ALL).title(" Detail ")),
            area,
        );
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Min(6),
            Constraint::Length(8),
        ])
        .split(area);

    let meta_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(chunks[0]);

    let mut meta_lines = vec![
        Line::from(format!(
            "room {}   map {}   skill {}   seats {}/{}   seed {:08x}   {}",
            ins.id,
            ins.map,
            ins.skill,
            ins.occupied,
            ins.max_players,
            ins.rng_seed,
            if ins.started { "STARTED" } else { "lobby" }
        )),
        Line::from(format!(
            "mod {}   packs [{}]   verifier {}   game {}",
            ins.game_mod.as_deref().unwrap_or("—"),
            ins.packs.join(", "),
            ins.enable_verifier,
            if ins.game.is_empty() {
                "—"
            } else {
                &ins.game
            }
        )),
    ];
    meta_lines.extend(room_net_lines(ins));
    frame.render_widget(
        Paragraph::new(meta_lines).block(Block::default().borders(Borders::ALL).title(" Room ")),
        meta_chunks[0],
    );
    draw_minimap(frame, meta_chunks[1], ins);

    let header = Row::new(vec![
        "#", "name", "state", "rtt", "miss", "ip", "mute", "pose", "ticket", "fp",
    ])
    .style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD));
    let rows = ins.seats.iter().enumerate().map(|(i, s)| {
        let mute = format!(
            "{}{}{}",
            if s.muted_audio { "A" } else { "-" },
            if s.muted_video { "V" } else { "-" },
            if s.muted_chat { "C" } else { "-" }
        );
        let pose = s
            .pose
            .as_ref()
            .map(|p| format!("{},{} @{}", p.x, p.y, p.angle))
            .unwrap_or_else(|| "—".into());
        let rtt = s
            .rtt_ms
            .map(|ms| format!("{ms}"))
            .unwrap_or_else(|| "—".into());
        let waiting = ins.waiting_seats.contains(&s.seat);
        let mut style = if i == app.seat_sel {
            Style::default()
                .bg(Color::Rgb(40, 20, 20))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        if waiting || s.miss_streak > 0 {
            style = style.fg(WARN);
        }
        if s.rtt_ms.is_some_and(|ms| ms > 150) {
            style = style.fg(BAD);
        }
        Row::new(vec![
            s.seat.to_string(),
            s.name.clone(),
            s.state.clone(),
            rtt,
            s.miss_streak.to_string(),
            s.remote_ip.clone(),
            mute,
            pose,
            truncate(&s.ticket_id, 10),
            truncate(&s.iwad_fingerprint, 8),
        ])
        .style(style)
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Length(10),
            Constraint::Length(7),
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Length(12),
            Constraint::Length(4),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Min(6),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Seats  (rtt ms · miss = stall fills · k = kick) "),
    );
    frame.render_widget(table, chunks[1]);

    let specs: Vec<ListItem<'_>> = if ins.spectators.is_empty() {
        vec![ListItem::new(Span::styled(
            "(no spectators)",
            Style::default().fg(MUTED),
        ))]
    } else {
        ins.spectators
            .iter()
            .map(|s| {
                ListItem::new(format!(
                    "{}  {}  frames={}",
                    s.name, s.remote_ip, s.frames_sent
                ))
            })
            .collect()
    };
    frame.render_widget(
        List::new(specs).block(Block::default().borders(Borders::ALL).title(" Spectators ")),
        chunks[2],
    );
}

fn draw_minimap(frame: &mut Frame<'_>, area: Rect, ins: &RoomInspect) {
    let inner = Block::default()
        .borders(Borders::ALL)
        .title(" Minimap ")
        .inner(area);
    frame.render_widget(
        Block::default().borders(Borders::ALL).title(" Minimap "),
        area,
    );
    if inner.width < 4 || inner.height < 2 {
        return;
    }
    let mut grid = vec![vec![' '; inner.width as usize]; inner.height as usize];
    let pts: Vec<(i32, i32)> = ins
        .outline
        .chunks_exact(4)
        .flat_map(|c| {
            [
                (i32::from(c[0]), i32::from(c[1])),
                (i32::from(c[2]), i32::from(c[3])),
            ]
        })
        .chain(
            ins.seats
                .iter()
                .filter_map(|s| s.pose.as_ref().map(|p| (p.x, p.y))),
        )
        .collect();
    if pts.is_empty() {
        let msg = "no outline / poses";
        let y = (inner.height / 2) as usize;
        let x = ((inner.width as usize).saturating_sub(msg.len())) / 2;
        if y < grid.len() {
            for (i, ch) in msg.chars().enumerate() {
                if x + i < grid[y].len() {
                    grid[y][x + i] = ch;
                }
            }
        }
    } else {
        let min_x = pts.iter().map(|p| p.0).min().unwrap_or(0);
        let max_x = pts.iter().map(|p| p.0).max().unwrap_or(1);
        let min_y = pts.iter().map(|p| p.1).min().unwrap_or(0);
        let max_y = pts.iter().map(|p| p.1).max().unwrap_or(1);
        let dx = (max_x - min_x).max(1) as f64;
        let dy = (max_y - min_y).max(1) as f64;
        let w = (inner.width - 1) as f64;
        let h = (inner.height - 1) as f64;
        for seg in ins.outline.chunks_exact(4) {
            let (x1, y1, x2, y2) = (
                i32::from(seg[0]),
                i32::from(seg[1]),
                i32::from(seg[2]),
                i32::from(seg[3]),
            );
            let sx = |x: i32| (((f64::from(x - min_x) / dx) * w) as usize).min(w as usize);
            let sy = |y: i32| {
                // Map Y grows up in Doom; flip for screen.
                (((1.0 - f64::from(y - min_y) / dy) * h) as usize).min(h as usize)
            };
            plot_line(&mut grid, sx(x1), sy(y1), sx(x2), sy(y2), '·');
        }
        for s in &ins.seats {
            if let Some(p) = &s.pose {
                let x = (((f64::from(p.x - min_x) / dx) * w) as usize).min(w as usize);
                let y = (((1.0 - f64::from(p.y - min_y) / dy) * h) as usize).min(h as usize);
                let ch = char::from_digit(u32::from(s.seat) % 10, 10).unwrap_or('@');
                if y < grid.len() && x < grid[y].len() {
                    grid[y][x] = ch;
                }
            }
        }
    }
    let lines: Vec<Line<'_>> = grid
        .into_iter()
        .map(|row| {
            Line::from(Span::styled(
                row.into_iter().collect::<String>(),
                Style::default().fg(MUTED),
            ))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn plot_line(grid: &mut [Vec<char>], x0: usize, y0: usize, x1: usize, y1: usize, ch: char) {
    let (mut x0, mut y0, x1, y1) = (x0 as i32, y0 as i32, x1 as i32, y1 as i32);
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        if y0 >= 0 && x0 >= 0 {
            let yu = y0 as usize;
            let xu = x0 as usize;
            if yu < grid.len() && xu < grid[yu].len() && grid[yu][xu] == ' ' {
                grid[yu][xu] = ch;
            }
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn draw_bans(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let header = Row::new(vec!["id", "scope", "value", "exp", "reason"])
        .style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD));
    let rows = app.bans.iter().enumerate().map(|(i, b)| {
        let style = if i == app.ban_sel {
            Style::default()
                .bg(Color::Rgb(40, 20, 20))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let exp = if b.exp == 0 {
            "perm".into()
        } else {
            b.exp.to_string()
        };
        Row::new(vec![
            truncate(&b.id, 10),
            b.scope.clone(),
            b.value.clone(),
            exp,
            b.reason.clone(),
        ])
        .style(style)
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(24),
            Constraint::Length(10),
            Constraint::Min(10),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Bans "));
    frame.render_widget(table, area);
}

fn draw_audit(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let total = app.audit.len();
    let items: Vec<ListItem<'_>> = app
        .audit
        .iter()
        .rev()
        .enumerate()
        .map(|(vis_i, e)| {
            let real = total - 1 - vis_i;
            let style = if real == app.audit_sel {
                Style::default()
                    .bg(Color::Rgb(40, 20, 20))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(format!(
                "{:>10}  {:<12}  {:<14}  {:<16}  {}",
                e.ts, e.actor, e.action, e.target, e.detail
            ))
            .style(style)
        })
        .collect();
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Audit (newest first) "),
        ),
        area,
    );
}

fn draw_mods(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let header = Row::new(vec!["name", "title", "size", "allowed", "fingerprint"])
        .style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD));
    let rows = app.mods.iter().enumerate().map(|(i, m)| {
        let style = if i == app.mod_sel {
            Style::default()
                .bg(Color::Rgb(40, 20, 20))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        Row::new(vec![
            m.name.clone(),
            m.title.clone(),
            format_bytes(m.size),
            if m.allowed { "yes" } else { "no" }.into(),
            truncate(&m.fingerprint, 16),
        ])
        .style(style)
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(14),
            Constraint::Length(24),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Min(12),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Mods (allow/disallow in web Settings) "),
    );
    frame.render_widget(table, area);
}

fn draw_status(frame: &mut Frame<'_>, area: Rect, app: &App) {
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!(" {}", app.status),
            Style::default().fg(MUTED),
        )),
        area,
    );
}

fn draw_prompt(frame: &mut Frame<'_>, area: Rect, title: &str, value: &str) {
    let width = area.width.clamp(40, 72);
    let height = 5;
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let rect = Rect {
        x,
        y,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let body = Paragraph::new(vec![
        Line::from(title),
        Line::from(""),
        Line::from(format!("> {value}█")),
    ])
    .wrap(Wrap { trim: false })
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ACCENT))
            .title(" Input "),
    );
    frame.render_widget(body, rect);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

fn format_bytes(n: u64) -> String {
    if n >= 1_048_576 {
        format!("{:.1} MiB", n as f64 / 1_048_576.0)
    } else if n >= 1024 {
        format!("{:.1} KiB", n as f64 / 1024.0)
    } else {
        format!("{n} B")
    }
}
