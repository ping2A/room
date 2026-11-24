//! `room-admin` — live ratatui console for room-server admin API.

mod api;
mod app;
mod http;
mod ui;

use api::Client;
use app::App;
use clap::Parser;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, Stdout};
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(
    name = "room-admin",
    about = "Terminal admin console for room-server (docs/22 §7)"
)]
struct Args {
    /// Admin base URL (`http://host:port`).
    #[arg(long, env = "ROOM_ADMIN_URL", default_value = "http://127.0.0.1:8766")]
    url: String,
    /// Bearer token (`ROOM_ADMIN_TOKEN`) when no MFA users exist.
    #[arg(long, env = "ROOM_ADMIN_TOKEN", default_value = "")]
    token: String,
    /// Admin username for session login.
    #[arg(long, env = "ROOM_ADMIN_USER", default_value = "")]
    user: String,
    /// Admin password for session login.
    #[arg(long, env = "ROOM_ADMIN_PASSWORD", default_value = "")]
    password: String,
    /// Poll interval in milliseconds.
    #[arg(long, env = "ROOM_ADMIN_REFRESH_MS", default_value_t = 1000)]
    refresh_ms: u64,
    /// Fetch once and print a text snapshot (no TUI; for scripts / smoke).
    #[arg(long, default_value_t = false)]
    once: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut client = Client::new(&args.url)?;
    if !args.token.is_empty() {
        client.set_bearer(&args.token);
    }
    if !args.user.is_empty() {
        let pass = if args.password.is_empty() {
            read_password(&args.user)?
        } else {
            args.password.clone()
        };
        client.login(&args.user, &pass)?;
    } else if args.token.is_empty() {
        // Dev servers often bootstrap admin/admin — try that, else open API.
        if client.login("admin", "admin").is_err() {
            // Open (no auth) local binds still work with empty credentials.
        }
    }

    // Probe once before taking over the terminal so auth errors are readable.
    client
        .summary()
        .map_err(|e| anyhow::anyhow!("cannot reach admin API at {}: {e}", args.url))?;

    let mut app = App::new(client, args.url.clone(), args.refresh_ms);
    app.refresh();
    if args.once {
        print_once(&app);
        return Ok(());
    }

    if !io::IsTerminal::is_terminal(&io::stdin()) || !io::IsTerminal::is_terminal(&io::stdout()) {
        anyhow::bail!(
            "admin TUI needs an interactive terminal (stdin/stdout are not a TTY).\n\
             Use --once for a text snapshot, or run from a real terminal pane."
        );
    }

    let mut terminal = setup_terminal()?;
    // Always leave the alternate screen / raw mode, even on panic.
    let panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        panic_hook(info);
    }));
    let result = run_loop(&mut terminal, &mut app);
    restore_terminal(&mut terminal)?;
    result
}

fn print_once(app: &App) {
    println!("url={}", app.url);
    println!(
        "summary rooms={} node={} draining={} verified={}",
        app.summary.rooms, app.summary.node_id, app.summary.draining, app.summary.verified_rooms
    );
    println!(
        "metrics ws_msgs={} tic_frames={} mean_relay_us={}",
        app.metrics.ws_msgs,
        app.metrics.tic_frames,
        app.metrics
            .mean_relay_us()
            .map(|u| u.to_string())
            .unwrap_or_else(|| "—".into())
    );
    println!("rooms={}", app.rooms.len());
    for r in &app.rooms {
        println!(
            "  {} map={} seats={}/{} started={} mod={}",
            r.id,
            r.map,
            r.occupied,
            r.max_players,
            r.started,
            r.game_mod.as_deref().unwrap_or("-")
        );
    }
    if let Some(ins) = &app.inspect {
        println!(
            "inspect {} game={} proto={} tic={} desyncs={} waiting={:?} relay={:?}",
            ins.id,
            ins.game,
            ins.protocol,
            ins.next_tic,
            ins.desync_count,
            ins.waiting_seats,
            ins.last_relay_us
        );
        println!(
            "  seats={} spectators={} outline_pts={} uptime={}s stall={}ms",
            ins.seats.len(),
            ins.spectators.len(),
            ins.outline.len(),
            ins.uptime_secs,
            ins.stall_fill_ms
        );
        for s in &ins.seats {
            println!(
                "  seat{} name={} state={} ip={} rtt={:?}ms miss={}",
                s.seat, s.name, s.state, s.remote_ip, s.rtt_ms, s.miss_streak
            );
        }
    }
    println!(
        "bans={} audit={} mods={}",
        app.bans.len(),
        app.audit.len(),
        app.mods.len()
    );
    println!("status={}", app.status);
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;
        app.tick();

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                app.on_key(key);
            }
        }
        if app.should_quit {
            break;
        }
    }
    Ok(())
}

fn setup_terminal() -> anyhow::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn read_password(user: &str) -> anyhow::Result<String> {
    eprint!("password for {user}: ");
    // Best-effort: turn echo off via crossterm if possible; fall back to stdin line.
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim_end_matches(['\r', '\n']).to_owned())
}
