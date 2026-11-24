//! App state, polling, and key handling.

use crate::api::{
    AdminModRow, AuditEntry, Ban, Client, Metrics, RoomInspect, RoomRow, SeatInspect, Summary,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};

/// Visible tab.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tab {
    /// Cluster dashboard.
    Dashboard,
    /// Network / relay.
    Network,
    /// Room list.
    Rooms,
    /// Selected room detail.
    Detail,
    /// Bans.
    Bans,
    /// Audit log.
    Audit,
    /// Mods allow-list.
    Mods,
}

impl Tab {
    /// Tab order for cycling / the tab bar.
    pub const ALL: [Tab; 7] = [
        Tab::Dashboard,
        Tab::Network,
        Tab::Rooms,
        Tab::Detail,
        Tab::Bans,
        Tab::Audit,
        Tab::Mods,
    ];

    /// Short label for the tab bar.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Tab::Dashboard => "1 Dash",
            Tab::Network => "2 Net",
            Tab::Rooms => "3 Rooms",
            Tab::Detail => "4 Detail",
            Tab::Bans => "5 Bans",
            Tab::Audit => "6 Audit",
            Tab::Mods => "7 Mods",
        }
    }

    fn next(self) -> Self {
        let i = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    fn prev(self) -> Self {
        let i = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(i + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    fn from_digit(c: char) -> Option<Self> {
        match c {
            '1' => Some(Tab::Dashboard),
            '2' => Some(Tab::Network),
            '3' => Some(Tab::Rooms),
            '4' => Some(Tab::Detail),
            '5' => Some(Tab::Bans),
            '6' => Some(Tab::Audit),
            '7' => Some(Tab::Mods),
            _ => None,
        }
    }
}

/// Modal prompt for short text input (notice / kick reason).
#[derive(Clone, Debug)]
pub struct Prompt {
    /// What the prompt is for.
    pub kind: PromptKind,
    /// Title shown above the input.
    pub title: String,
    /// Current buffer.
    pub value: String,
}

/// Prompt purpose.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromptKind {
    /// Broadcast notice to the selected room.
    Notice,
    /// Kick the selected seat with a reason.
    Kick,
}

/// Derived rates between two metric snapshots.
#[derive(Clone, Debug, Default)]
pub struct Rates {
    /// WebSocket messages per second.
    pub ws_msgs_per_s: f64,
    /// Inbound bytes per second.
    pub ws_bytes_per_s: f64,
    /// Tic frames per second.
    pub tic_frames_per_s: f64,
}

/// Live snapshot held by the UI.
#[derive(Debug)]
pub struct App {
    client: Client,
    /// Active tab.
    pub tab: Tab,
    /// Last error / status line.
    pub status: String,
    /// Quit flag.
    pub should_quit: bool,
    /// Refresh interval.
    refresh_every: Duration,
    last_refresh: Instant,
    /// Cluster summary.
    pub summary: Summary,
    /// Prometheus metrics.
    pub metrics: Metrics,
    /// Previous metrics for rate calc.
    prev_metrics: Option<(Instant, Metrics)>,
    /// Instantaneous rates.
    pub rates: Rates,
    /// Room list.
    pub rooms: Vec<RoomRow>,
    /// Selected room index in `rooms`.
    pub room_sel: usize,
    /// Selected room inspect (if any).
    pub inspect: Option<RoomInspect>,
    /// Selected seat index inside inspect.
    pub seat_sel: usize,
    /// Bans.
    pub bans: Vec<Ban>,
    /// Selected ban index.
    pub ban_sel: usize,
    /// Audit (newest last; UI shows reversed).
    pub audit: Vec<AuditEntry>,
    /// Audit scroll (from newest).
    pub audit_sel: usize,
    /// Mods.
    pub mods: Vec<AdminModRow>,
    /// Selected mod index.
    pub mod_sel: usize,
    /// Optional text prompt.
    pub prompt: Option<Prompt>,
    /// Base URL shown in the header.
    pub url: String,
}

impl App {
    /// Create an app with an authenticated client.
    #[must_use]
    pub fn new(client: Client, url: String, refresh_ms: u64) -> Self {
        Self {
            client,
            tab: Tab::Dashboard,
            status: "ready".into(),
            should_quit: false,
            refresh_every: Duration::from_millis(refresh_ms.max(250)),
            last_refresh: Instant::now() - Duration::from_secs(60),
            summary: Summary::default(),
            metrics: Metrics::default(),
            prev_metrics: None,
            rates: Rates::default(),
            rooms: Vec::new(),
            room_sel: 0,
            inspect: None,
            seat_sel: 0,
            bans: Vec::new(),
            ban_sel: 0,
            audit: Vec::new(),
            audit_sel: 0,
            mods: Vec::new(),
            mod_sel: 0,
            prompt: None,
            url,
        }
    }

    /// Selected room id, if any.
    #[must_use]
    pub fn selected_room_id(&self) -> Option<&str> {
        self.rooms.get(self.room_sel).map(|r| r.id.as_str())
    }

    /// Selected seat, if inspect is open.
    #[must_use]
    pub fn selected_seat(&self) -> Option<&SeatInspect> {
        self.inspect
            .as_ref()
            .and_then(|r| r.seats.get(self.seat_sel))
    }

    /// Aggregate RTT / waiting / desync across the inspected room (if any).
    #[must_use]
    pub fn net_room_highlight(&self) -> Option<&RoomInspect> {
        self.inspect.as_ref()
    }

    /// Refresh when due.
    pub fn tick(&mut self) {
        if self.last_refresh.elapsed() >= self.refresh_every {
            self.refresh();
        }
    }

    /// Force a full poll of the admin API.
    pub fn refresh(&mut self) {
        self.last_refresh = Instant::now();
        let mut errs = Vec::new();

        match self.client.summary() {
            Ok(s) => self.summary = s,
            Err(e) => errs.push(format!("summary: {e}")),
        }
        match self.client.metrics() {
            Ok(m) => {
                let now = Instant::now();
                if let Some((prev_at, prev)) = &self.prev_metrics {
                    let dt = now.duration_since(*prev_at).as_secs_f64().max(0.001);
                    self.rates = Rates {
                        ws_msgs_per_s: m.ws_msgs.saturating_sub(prev.ws_msgs) as f64 / dt,
                        ws_bytes_per_s: m.ws_bytes.saturating_sub(prev.ws_bytes) as f64 / dt,
                        tic_frames_per_s: m.tic_frames.saturating_sub(prev.tic_frames) as f64 / dt,
                    };
                }
                self.prev_metrics = Some((now, m.clone()));
                self.metrics = m;
            }
            Err(e) => errs.push(format!("metrics: {e}")),
        }
        match self.client.rooms() {
            Ok(rooms) => {
                let keep = self.selected_room_id().map(str::to_owned);
                self.rooms = rooms;
                if let Some(id) = keep {
                    if let Some(i) = self.rooms.iter().position(|r| r.id == id) {
                        self.room_sel = i;
                    } else if self.room_sel >= self.rooms.len() {
                        self.room_sel = self.rooms.len().saturating_sub(1);
                    }
                } else if self.room_sel >= self.rooms.len() {
                    self.room_sel = self.rooms.len().saturating_sub(1);
                }
            }
            Err(e) => errs.push(format!("rooms: {e}")),
        }
        match self.client.bans() {
            Ok(b) => {
                self.bans = b;
                if self.ban_sel >= self.bans.len() {
                    self.ban_sel = self.bans.len().saturating_sub(1);
                }
            }
            Err(e) => errs.push(format!("bans: {e}")),
        }
        match self.client.audit() {
            Ok(a) => {
                self.audit = a;
                if self.audit_sel >= self.audit.len() {
                    self.audit_sel = self.audit.len().saturating_sub(1);
                }
            }
            Err(e) => errs.push(format!("audit: {e}")),
        }
        match self.client.mods() {
            Ok(m) => {
                self.mods = m;
                if self.mod_sel >= self.mods.len() {
                    self.mod_sel = self.mods.len().saturating_sub(1);
                }
            }
            Err(e) => errs.push(format!("mods: {e}")),
        }

        if let Some(id) = self.selected_room_id().map(str::to_owned) {
            match self.client.inspect(&id) {
                Ok(ins) => {
                    if self.seat_sel >= ins.seats.len() {
                        self.seat_sel = ins.seats.len().saturating_sub(1);
                    }
                    self.inspect = Some(ins);
                }
                Err(e) => {
                    errs.push(format!("inspect: {e}"));
                    self.inspect = None;
                }
            }
        } else {
            self.inspect = None;
        }

        self.status = if errs.is_empty() {
            format!(
                "ok · {} room(s) · {:.0} msg/s · {:.0} frame/s · {}",
                self.rooms.len(),
                self.rates.ws_msgs_per_s,
                self.rates.tic_frames_per_s,
                chrono_like_now()
            )
        } else {
            errs.join(" · ")
        };
    }

    /// Handle a key press. Returns `true` when the UI should redraw.
    pub fn on_key(&mut self, key: KeyEvent) -> bool {
        if let Some(prompt) = self.prompt.as_mut() {
            match key.code {
                KeyCode::Esc => {
                    self.prompt = None;
                    self.status = "cancelled".into();
                }
                KeyCode::Enter => {
                    let done = Prompt {
                        kind: prompt.kind,
                        title: prompt.title.clone(),
                        value: prompt.value.clone(),
                    };
                    self.prompt = None;
                    self.submit_prompt(done);
                }
                KeyCode::Backspace => {
                    prompt.value.pop();
                }
                KeyCode::Char(c)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && prompt.value.len() < 200 =>
                {
                    prompt.value.push(c);
                }
                _ => {}
            }
            return true;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Char('r') => self.refresh(),
            KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.tab = self.tab.prev();
            }
            KeyCode::Tab | KeyCode::Right => self.tab = self.tab.next(),
            KeyCode::Left => self.tab = self.tab.prev(),
            KeyCode::Char(c) if Tab::from_digit(c).is_some() => {
                if let Some(t) = Tab::from_digit(c) {
                    self.tab = t;
                }
            }
            KeyCode::Up => self.move_sel(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_sel(1),
            KeyCode::Enter => {
                if matches!(self.tab, Tab::Rooms | Tab::Dashboard | Tab::Network) {
                    self.tab = Tab::Detail;
                    self.refresh();
                }
            }
            KeyCode::Char('c') if matches!(self.tab, Tab::Rooms | Tab::Detail) => {
                self.close_selected_room();
            }
            KeyCode::Char('n') if matches!(self.tab, Tab::Rooms | Tab::Detail) => {
                self.begin_notice();
            }
            KeyCode::Char('k') if self.tab == Tab::Detail => self.begin_kick(),
            KeyCode::Char('k') => self.move_sel(-1),
            _ => return false,
        }
        true
    }

    fn move_sel(&mut self, delta: isize) {
        match self.tab {
            Tab::Rooms | Tab::Dashboard | Tab::Network => {
                if self.rooms.is_empty() {
                    return;
                }
                let len = self.rooms.len() as isize;
                self.room_sel = ((self.room_sel as isize + delta).rem_euclid(len)) as usize;
            }
            Tab::Detail => {
                let len = self.inspect.as_ref().map(|r| r.seats.len()).unwrap_or(0) as isize;
                if len == 0 {
                    return;
                }
                self.seat_sel = ((self.seat_sel as isize + delta).rem_euclid(len)) as usize;
            }
            Tab::Bans => {
                if self.bans.is_empty() {
                    return;
                }
                let len = self.bans.len() as isize;
                self.ban_sel = ((self.ban_sel as isize + delta).rem_euclid(len)) as usize;
            }
            Tab::Audit => {
                if self.audit.is_empty() {
                    return;
                }
                let len = self.audit.len() as isize;
                self.audit_sel = ((self.audit_sel as isize + delta).rem_euclid(len)) as usize;
            }
            Tab::Mods => {
                if self.mods.is_empty() {
                    return;
                }
                let len = self.mods.len() as isize;
                self.mod_sel = ((self.mod_sel as isize + delta).rem_euclid(len)) as usize;
            }
        }
    }

    fn close_selected_room(&mut self) {
        let Some(id) = self.selected_room_id().map(str::to_owned) else {
            self.status = "no room selected".into();
            return;
        };
        match self.client.close_room(&id) {
            Ok(()) => {
                self.status = format!("closed {id}");
                self.refresh();
            }
            Err(e) => self.status = format!("close failed: {e}"),
        }
    }

    fn begin_notice(&mut self) {
        if self.selected_room_id().is_none() {
            self.status = "no room selected".into();
            return;
        }
        self.prompt = Some(Prompt {
            kind: PromptKind::Notice,
            title: "Notice to room (Enter send · Esc cancel)".into(),
            value: String::new(),
        });
    }

    fn begin_kick(&mut self) {
        let Some(seat) = self.selected_seat().map(|s| s.seat) else {
            self.status = "no seat selected".into();
            return;
        };
        self.prompt = Some(Prompt {
            kind: PromptKind::Kick,
            title: format!("Kick seat {seat} reason (Enter · Esc)"),
            value: "kicked".into(),
        });
    }

    fn submit_prompt(&mut self, prompt: Prompt) {
        match prompt.kind {
            PromptKind::Notice => {
                let Some(id) = self.selected_room_id().map(str::to_owned) else {
                    self.status = "no room selected".into();
                    return;
                };
                match self.client.notice(&id, &prompt.value) {
                    Ok(()) => {
                        self.status = format!("notice → {id}");
                        self.refresh();
                    }
                    Err(e) => self.status = format!("notice failed: {e}"),
                }
            }
            PromptKind::Kick => {
                let Some(id) = self.selected_room_id().map(str::to_owned) else {
                    self.status = "no room selected".into();
                    return;
                };
                let Some(seat) = self.selected_seat().map(|s| s.seat) else {
                    self.status = "no seat selected".into();
                    return;
                };
                match self.client.kick(&id, seat, &prompt.value) {
                    Ok(()) => {
                        self.status = format!("kicked {id}/{seat}");
                        self.refresh();
                    }
                    Err(e) => self.status = format!("kick failed: {e}"),
                }
            }
        }
    }
}

fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let s = secs % 86_400;
    format!("{:02}:{:02}:{:02}Z", s / 3600, (s % 3600) / 60, s % 60)
}
