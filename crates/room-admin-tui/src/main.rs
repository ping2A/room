//! `room-admin` — live ratatui console for room-server admin API.

mod api;
mod app;
mod http;
mod ui;

use api::{Client, LoginOutcome};
use app::App;
use clap::Parser;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, Stdout, Write};
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
    authenticate(&mut client, &args)?;

    // Probe once before taking over the terminal so auth errors are readable.
    client
        .summary()
        .map_err(|e| auth_probe_error(&args.url, &e))?;

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

fn authenticate(client: &mut Client, args: &Args) -> anyhow::Result<()> {
    if !args.token.is_empty() {
        client.set_bearer(&args.token);
    }

    if let Some((user, pass)) = resolve_password_creds(args)? {
        return finish_login(client, &user, &pass);
    }

    let mode = client.auth_mode().unwrap_or_else(|_| "unknown".into());
    match mode.as_str() {
        "open" => Ok(()),
        "token" => {
            if args.token.is_empty() {
                anyhow::bail!(
                    "admin API expects a bearer token.\n\
                     Set ROOM_ADMIN_TOKEN or pass --token."
                );
            }
            Ok(())
        }
        "mfa" => {
            // Interactive prompt when bootstrap / flags were not provided.
            if !io::IsTerminal::is_terminal(&io::stdin()) {
                anyhow::bail!(
                    "admin API requires MFA session login (HTTP 401 without credentials).\n\
                     Set ROOM_ADMIN_USER + ROOM_ADMIN_PASSWORD, or ROOM_ADMIN_BOOTSTRAP=user:pass\n\
                     (same value as the server .env), then retry."
                );
            }
            eprint!("admin user: ");
            let _ = io::stderr().flush();
            let user = read_line()?;
            if user.is_empty() {
                anyhow::bail!("empty username");
            }
            let pass = read_password(&user)?;
            finish_login(client, &user, &pass)
        }
        _ => {
            // Status probe failed or returned an unexpected mode — try the
            // common local bootstrap pair, otherwise leave bearer/open alone.
            match client.login("admin", "admin") {
                Ok(LoginOutcome::Session) => Ok(()),
                Ok(LoginOutcome::NeedTotp) => {
                    if !io::IsTerminal::is_terminal(&io::stdin()) {
                        return Ok(());
                    }
                    eprint!("TOTP code: ");
                    let _ = io::stderr().flush();
                    let code = read_line()?;
                    client.verify_totp(&code)?;
                    Ok(())
                }
                Err(_) => Ok(()),
            }
        }
    }
}

fn finish_login(client: &mut Client, user: &str, pass: &str) -> anyhow::Result<()> {
    match client.login(user, pass)? {
        LoginOutcome::Session => Ok(()),
        LoginOutcome::NeedTotp => {
            if !io::IsTerminal::is_terminal(&io::stdin()) {
                anyhow::bail!(
                    "password ok, but this account requires a TOTP code.\n\
                     Run `./room admin-tui` from an interactive terminal, or use the web UI."
                );
            }
            eprint!("TOTP code: ");
            let _ = io::stderr().flush();
            let code = read_line()?;
            client.verify_totp(&code)?;
            Ok(())
        }
    }
}

fn resolve_password_creds(args: &Args) -> anyhow::Result<Option<(String, String)>> {
    if !args.user.is_empty() {
        let pass = if args.password.is_empty() {
            if !io::IsTerminal::is_terminal(&io::stdin()) {
                anyhow::bail!(
                    "ROOM_ADMIN_USER set but no password.\n\
                     Pass --password / ROOM_ADMIN_PASSWORD, or run interactively."
                );
            }
            read_password(&args.user)?
        } else {
            args.password.clone()
        };
        return Ok(Some((args.user.clone(), pass)));
    }

    // Same secret the server used on first boot (`user:password`).
    if let Ok(boot) = std::env::var("ROOM_ADMIN_BOOTSTRAP") {
        let boot = boot.trim();
        if !boot.is_empty() {
            let Some((user, pass)) = boot.split_once(':') else {
                anyhow::bail!("ROOM_ADMIN_BOOTSTRAP must be `user:password`");
            };
            let user = user.trim();
            if user.is_empty() || pass.is_empty() {
                anyhow::bail!("ROOM_ADMIN_BOOTSTRAP must be `user:password`");
            }
            return Ok(Some((user.to_owned(), pass.to_owned())));
        }
    }
    Ok(None)
}

fn auth_probe_error(url: &str, err: &http::HttpError) -> anyhow::Error {
    let msg = err.to_string();
    if msg.contains("401") || msg.contains("403") {
        anyhow::anyhow!(
            "cannot reach admin API at {url}: {msg}\n\
             Admin MFA is enabled — the TUI needs a session login.\n\
             Fix: export ROOM_ADMIN_USER / ROOM_ADMIN_PASSWORD (or ROOM_ADMIN_BOOTSTRAP=user:pass\n\
             from .env), then `./room admin-tui`. Bearer tokens are ignored once admins.json exists."
        )
    } else {
        anyhow::anyhow!("cannot reach admin API at {url}: {msg}")
    }
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

fn read_line() -> anyhow::Result<String> {
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim_end_matches(['\r', '\n']).to_owned())
}

fn read_password(user: &str) -> anyhow::Result<String> {
    eprint!("password for {user}: ");
    let _ = io::stderr().flush();
    // Echo stays on (no libc tty dance); fine for loopback admin ops.
    read_line()
}
