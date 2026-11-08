//! Command line dispatch (`C_DoCommand` subset).

use crate::cvar::CvarStore;
use crate::error::ConsoleError;
use std::collections::BTreeMap;

/// Host / game actions emitted by cheat & map commands (applied outside the console).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostCommand {
    /// Toggle god mode (`god` / `iddqd`).
    God,
    /// Toggle no-clip (`noclip`).
    Noclip,
    /// Give inventory class or keyword (`give all`, `give shotgun`, …).
    Give(String),
    /// Change map (`map MAP01`).
    Map(String),
    /// Kill all monsters (`kill monsters`) or self (`kill`).
    Kill(Option<String>),
    /// Toggle level freeze (`freeze`) — holds maptime / movers.
    Freeze,
    /// Toggle Computer Area Map reveal (`iddt` / `am_cheat` bootstrap).
    Iddt,
    /// Toggle game pause (`pause`).
    Pause,
    /// Spawn an actor class in front of the player (`summon ZombieMan`).
    Summon(String),
    /// Print player coordinates (`idmypos`).
    Idmypos,
    /// Print sector, facing textures, moved sectors and nearby doors (`look`).
    Look,
    /// Capture a PNG screenshot of the next presented frame.
    Screenshot,
    /// Start / stop in-game GIF recording.
    GifToggle,
    /// Write a Room `.fsave` (`save` / `save <slot>`).
    Save(Option<String>),
    /// Restore a Room `.fsave` (`load` / `load <slot>`).
    Load(Option<String>),
    /// Start a Room `.fdem` recording and, for native Duke, an Atomic `.DMO` recording.
    Record(Option<String>),
    /// Finish the current Room demo (`stop` / `stopdemo`).
    StopDemo,
    /// Replay a Room `.fdem` (`playdemo` / `playdemo <slot>`).
    PlayDemo(Option<String>),
    /// Play a native Duke Atomic `.DMO` file (`playatomicdemo <path>`).
    PlayAtomicDemo(String),
    /// `puke <script> [arg1] [arg2] [arg3]` — start ACS by number (`docs/33` BD5).
    Puke {
        /// Script number.
        script: i32,
        /// Up to three script args.
        args: [i32; 3],
    },
    /// `pukename <name> [always] [arg1] [arg2] [arg3]`.
    PukeName {
        /// Script name (`SNAM`).
        name: String,
        /// Prefer `ACS_ExecuteAlways` semantics.
        always: bool,
        /// Up to three script args.
        args: [i32; 3],
    },
}

/// Result of executing a console line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecResult {
    /// Lines to print.
    pub output: Vec<String>,
    /// Game actions for the host to apply.
    pub host: Vec<HostCommand>,
}

/// Console state: CVars + scrollback of printed lines.
#[derive(Clone, Debug, Default)]
pub struct Console {
    /// CVar table.
    pub cvars: CvarStore,
    /// Recent printed lines.
    pub scrollback: Vec<String>,
    /// User `alias` expansions (name → command line).
    pub aliases: BTreeMap<String, String>,
}

impl Console {
    /// New console with engine default CVars.
    #[must_use]
    pub fn with_defaults() -> Self {
        let mut cvars = CvarStore::new();
        cvars.register_engine_defaults();
        Self {
            cvars,
            scrollback: Vec::new(),
            aliases: BTreeMap::new(),
        }
    }

    /// Print a line into scrollback.
    pub fn print(&mut self, line: impl Into<String>) {
        self.scrollback.push(line.into());
    }

    /// Execute a command line (`;`-separated statements).
    pub fn exec(&mut self, line: &str) -> Result<ExecResult, ConsoleError> {
        let mut output = Vec::new();
        let mut host = Vec::new();
        for stmt in split_statements(line) {
            let stmt = stmt.trim();
            if stmt.is_empty() {
                continue;
            }
            let tokens = tokenize(stmt);
            if tokens.is_empty() {
                continue;
            }
            let cmd = tokens[0].as_str();
            let args = &tokens[1..];
            let cmd_key = cmd.to_ascii_lowercase();
            // Expand user aliases once (no recursion in M5).
            if let Some(expansion) = self.aliases.get(&cmd_key).cloned() {
                let expanded = if args.is_empty() {
                    expansion
                } else {
                    format!("{expansion} {}", args.join(" "))
                };
                let nested = self.exec(&expanded)?;
                output.extend(nested.output);
                host.extend(nested.host);
                continue;
            }
            match cmd_key.as_str() {
                "echo" => {
                    let msg = args.join(" ");
                    self.print(&msg);
                    output.push(msg);
                }
                "set" | "seta" => {
                    if args.len() < 2 {
                        return Err(ConsoleError::Message("usage: set <cvar> <value>".into()));
                    }
                    let name = args[0].as_str();
                    let value = args[1..].join(" ");
                    if name.eq_ignore_ascii_case("r_quality") {
                        let level: i32 = value
                            .parse()
                            .map_err(|_| ConsoleError::Message(format!("bad int '{value}'")))?;
                        self.cvars.apply_quality_preset(level);
                        let msg = format!("r_quality = {level} (preset applied)");
                        self.print(&msg);
                        output.push(msg);
                    } else {
                        self.cvars.set(name, &value)?;
                    }
                }
                "get" => {
                    if args.len() != 1 {
                        return Err(ConsoleError::Message("usage: get <cvar>".into()));
                    }
                    let v = self
                        .cvars
                        .get(&args[0])
                        .ok_or_else(|| ConsoleError::Unknown(args[0].clone()))?;
                    let msg = format!("{} = {}", args[0], v.display());
                    self.print(&msg);
                    output.push(msg);
                }
                "toggle" => {
                    if args.len() != 1 {
                        return Err(ConsoleError::Message("usage: toggle <cvar>".into()));
                    }
                    let now = self.cvars.toggle(&args[0])?;
                    let msg = format!("{} = {}", args[0], now);
                    self.print(&msg);
                    output.push(msg);
                }
                "reset" | "resetcvar" => {
                    if args.len() != 1 {
                        return Err(ConsoleError::Message("usage: reset <cvar>".into()));
                    }
                    self.cvars.reset(&args[0])?;
                }
                "cvarlist" => {
                    let lines: Vec<String> = self
                        .cvars
                        .list()
                        .map(|(name, val)| format!("{name} = {}", val.display()))
                        .collect();
                    for msg in lines {
                        self.print(&msg);
                        output.push(msg);
                    }
                }
                "writeconfig" | "saveconfig" => {
                    let path = args
                        .first()
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(default_config_path);
                    crate::config::save_ini_file(&self.cvars, &path)?;
                    let msg = format!("wrote {}", path.display());
                    self.print(&msg);
                    output.push(msg);
                }
                "exec" | "loadconfig" => {
                    if args.is_empty() {
                        return Err(ConsoleError::Message(
                            "usage: exec <file.ini>|loadconfig [path]".into(),
                        ));
                    }
                    let path = std::path::PathBuf::from(&args[0]);
                    let applied = crate::config::load_ini_file(&mut self.cvars, &path)?;
                    for (key, cmd) in &applied.binds {
                        self.aliases
                            .insert(format!("bind_{}", key.to_ascii_lowercase()), cmd.clone());
                    }
                    let msg = format!(
                        "loaded {} ({} cvars, {} binds)",
                        path.display(),
                        applied.set,
                        applied.binds.len()
                    );
                    self.print(&msg);
                    output.push(msg);
                }
                "god" | "iddqd" => {
                    host.push(HostCommand::God);
                    let msg = "Degreelessness Mode On/Off".to_owned();
                    self.print(&msg);
                    output.push(msg);
                }
                "idkfa" => {
                    host.push(HostCommand::Give("idkfa".into()));
                    let msg = "Very Happy Ammo Added".to_owned();
                    self.print(&msg);
                    output.push(msg);
                }
                "idfa" => {
                    host.push(HostCommand::Give("idfa".into()));
                    let msg = "Ammo (no keys) Added".to_owned();
                    self.print(&msg);
                    output.push(msg);
                }
                "idchoppers" => {
                    host.push(HostCommand::Give("idchoppers".into()));
                    let msg = "...doesn't suck - GM".to_owned();
                    self.print(&msg);
                    output.push(msg);
                }
                "noclip" | "idclip" | "idspispopd" => {
                    host.push(HostCommand::Noclip);
                    let msg = "No clipping mode ON/OFF".to_owned();
                    self.print(&msg);
                    output.push(msg);
                }
                "freeze" => {
                    host.push(HostCommand::Freeze);
                    let msg = "Freeze mode ON/OFF".to_owned();
                    self.print(&msg);
                    output.push(msg);
                }
                "iddt" => {
                    host.push(HostCommand::Iddt);
                    let msg = "Automap reveal ON/OFF".to_owned();
                    self.print(&msg);
                    output.push(msg);
                }
                "idmypos" => {
                    host.push(HostCommand::Idmypos);
                    let msg = "Position".to_owned();
                    self.print(&msg);
                    output.push(msg);
                }
                "look" => {
                    host.push(HostCommand::Look);
                }
                "pause" => {
                    host.push(HostCommand::Pause);
                    let msg = "Pause".to_owned();
                    self.print(&msg);
                    output.push(msg);
                }
                "screenshot" | "shot" => {
                    host.push(HostCommand::Screenshot);
                    let msg = "Screenshot".to_owned();
                    self.print(&msg);
                    output.push(msg);
                }
                "gifrecord" | "gifstart" | "gifstop" | "gif" => {
                    host.push(HostCommand::GifToggle);
                    let msg = "GIF record toggle".to_owned();
                    self.print(&msg);
                    output.push(msg);
                }
                "save" | "savegame" => {
                    let slot = args.first().cloned();
                    host.push(HostCommand::Save(slot.clone()));
                    let msg = match slot.as_deref() {
                        Some(s) => format!("Save {s}"),
                        None => "Save".to_owned(),
                    };
                    self.print(&msg);
                    output.push(msg);
                }
                "load" | "loadgame" => {
                    let slot = args.first().cloned();
                    host.push(HostCommand::Load(slot.clone()));
                    let msg = match slot.as_deref() {
                        Some(s) => format!("Load {s}"),
                        None => "Load".to_owned(),
                    };
                    self.print(&msg);
                    output.push(msg);
                }
                "record" => {
                    let slot = args.first().cloned();
                    host.push(HostCommand::Record(slot.clone()));
                    let msg = match slot.as_deref() {
                        Some(s) => format!("Record {s}"),
                        None => "Record".to_owned(),
                    };
                    self.print(&msg);
                    output.push(msg);
                }
                "stop" | "stopdemo" => {
                    host.push(HostCommand::StopDemo);
                    let msg = "Stop demo".to_owned();
                    self.print(&msg);
                    output.push(msg);
                }
                "playdemo" => {
                    let slot = args.first().cloned();
                    host.push(HostCommand::PlayDemo(slot.clone()));
                    let msg = match slot.as_deref() {
                        Some(s) => format!("Playdemo {s}"),
                        None => "Playdemo".to_owned(),
                    };
                    self.print(&msg);
                    output.push(msg);
                }
                "playatomicdemo" => {
                    let Some(path) = args.first().cloned() else {
                        return Err(ConsoleError::Message(
                            "usage: playatomicdemo <path/to/demo.dmo>".into(),
                        ));
                    };
                    host.push(HostCommand::PlayAtomicDemo(path.clone()));
                    let msg = format!("Play Atomic Duke demo {path}");
                    self.print(&msg);
                    output.push(msg);
                }
                "summon" => {
                    if args.is_empty() {
                        return Err(ConsoleError::Message("usage: summon <class>".into()));
                    }
                    let class = args[0].clone();
                    host.push(HostCommand::Summon(class.clone()));
                    let msg = format!("Summon {class}");
                    self.print(&msg);
                    output.push(msg);
                }
                "give" => {
                    if args.is_empty() {
                        return Err(ConsoleError::Message("usage: give <item|all>".into()));
                    }
                    let item = args.join(" ");
                    host.push(HostCommand::Give(item.clone()));
                    let msg = format!("Give {item}");
                    self.print(&msg);
                    output.push(msg);
                }
                "map" | "changemap" => {
                    if args.len() != 1 {
                        return Err(ConsoleError::Message("usage: map <MAPNAME>".into()));
                    }
                    host.push(HostCommand::Map(args[0].clone()));
                    let msg = format!("Map {}", args[0]);
                    self.print(&msg);
                    output.push(msg);
                }
                "kill" => {
                    let target = args.first().cloned();
                    host.push(HostCommand::Kill(target.clone()));
                    let msg = match target.as_deref() {
                        Some(t) => format!("Kill {t}"),
                        None => "Kill".to_owned(),
                    };
                    self.print(&msg);
                    output.push(msg);
                }
                "puke" => {
                    if args.is_empty() {
                        return Err(ConsoleError::Message(
                            "usage: puke <script> [arg1] [arg2] [arg3]".into(),
                        ));
                    }
                    let script: i32 = args[0].parse().map_err(|_| {
                        ConsoleError::Message(format!("puke: bad script '{}'", args[0]))
                    })?;
                    let mut script_args = [0i32; 3];
                    for (i, a) in args.iter().skip(1).take(3).enumerate() {
                        script_args[i] = a
                            .parse()
                            .map_err(|_| ConsoleError::Message(format!("puke: bad arg '{a}'")))?;
                    }
                    host.push(HostCommand::Puke {
                        script,
                        args: script_args,
                    });
                    let msg = format!("Puke {script}");
                    self.print(&msg);
                    output.push(msg);
                }
                "pukename" => {
                    if args.is_empty() {
                        return Err(ConsoleError::Message(
                            "usage: pukename <name> [always] [arg1] [arg2] [arg3]".into(),
                        ));
                    }
                    let name = args[0].clone();
                    let mut rest = &args[1..];
                    let always = rest
                        .first()
                        .is_some_and(|s| s.eq_ignore_ascii_case("always"));
                    if always {
                        rest = &rest[1..];
                    }
                    let mut script_args = [0i32; 3];
                    for (i, a) in rest.iter().take(3).enumerate() {
                        script_args[i] = a.parse().map_err(|_| {
                            ConsoleError::Message(format!("pukename: bad arg '{a}'"))
                        })?;
                    }
                    host.push(HostCommand::PukeName {
                        name: name.clone(),
                        always,
                        args: script_args,
                    });
                    let msg = if always {
                        format!("PukeName {name} always")
                    } else {
                        format!("PukeName {name}")
                    };
                    self.print(&msg);
                    output.push(msg);
                }
                "alias" => {
                    if args.is_empty() {
                        let listed: Vec<String> = self
                            .aliases
                            .iter()
                            .map(|(name, body)| format!("alias {name} \"{body}\""))
                            .collect();
                        for msg in listed {
                            self.print(&msg);
                            output.push(msg);
                        }
                    } else if args.len() == 1 {
                        let key = args[0].to_ascii_lowercase();
                        if let Some(body) = self.aliases.get(&key) {
                            let msg = format!("alias {} \"{body}\"", args[0]);
                            self.print(&msg);
                            output.push(msg);
                        } else {
                            return Err(ConsoleError::Unknown(args[0].clone()));
                        }
                    } else {
                        let key = args[0].to_ascii_lowercase();
                        let body = args[1..].join(" ");
                        self.aliases.insert(key, body);
                    }
                }
                "unalias" => {
                    if args.len() != 1 {
                        return Err(ConsoleError::Message("usage: unalias <name>".into()));
                    }
                    self.aliases.remove(&args[0].to_ascii_lowercase());
                }
                "cmdlist" => {
                    for name in [
                        "echo",
                        "set",
                        "get",
                        "toggle",
                        "reset",
                        "cvarlist",
                        "cmdlist",
                        "alias",
                        "unalias",
                        "god",
                        "iddqd",
                        "idkfa",
                        "idfa",
                        "idchoppers",
                        "idbehold",
                        "iddt",
                        "idmypos",
                        "look",
                        "idclev",
                        "pause",
                        "screenshot",
                        "shot",
                        "gifrecord",
                        "gif",
                        "summon",
                        "noclip",
                        "idclip",
                        "idspispopd",
                        "freeze",
                        "give",
                        "map",
                        "changemap",
                        "playatomicdemo",
                        "kill",
                    ] {
                        self.print(name);
                        output.push(name.to_owned());
                    }
                }
                other => {
                    if other.starts_with("idbehold") {
                        let letter = other
                            .strip_prefix("idbehold")
                            .unwrap_or("")
                            .chars()
                            .next()
                            .or_else(|| args.first().and_then(|s| s.chars().next()))
                            .unwrap_or('\0')
                            .to_ascii_lowercase();
                        if !"vsiral".contains(letter) {
                            let msg = "usage: idbehold[v|s|i|r|a|l]".to_owned();
                            self.print(&msg);
                            output.push(msg);
                        } else {
                            host.push(HostCommand::Give(format!("idbehold{letter}")));
                            let msg = "Power-up Toggled".to_owned();
                            self.print(&msg);
                            output.push(msg);
                        }
                    } else if other.starts_with("idclev") {
                        match parse_idclev_map(other, args) {
                            Some(map) => {
                                host.push(HostCommand::Map(map.clone()));
                                let msg = format!("Changing level to {map}");
                                self.print(&msg);
                                output.push(msg);
                            }
                            None => {
                                let msg = "usage: idclev <MAP01|E1M1|01>".to_owned();
                                self.print(&msg);
                                output.push(msg);
                            }
                        }
                    } else if args.is_empty() {
                        // Bare `cvar` get.
                        if let Some(v) = self.cvars.get(other) {
                            let msg = format!("{other} = {}", v.display());
                            self.print(&msg);
                            output.push(msg);
                        } else {
                            return Err(ConsoleError::Unknown(other.to_owned()));
                        }
                    } else if self.cvars.get(other).is_some() {
                        let value = args.join(" ");
                        if other.eq_ignore_ascii_case("r_quality") {
                            let level: i32 = value
                                .parse()
                                .map_err(|_| ConsoleError::Message(format!("bad int '{value}'")))?;
                            self.cvars.apply_quality_preset(level);
                            let msg = format!("r_quality = {level} (preset applied)");
                            self.print(&msg);
                            output.push(msg);
                        } else {
                            self.cvars.set(other, &value)?;
                        }
                    } else {
                        return Err(ConsoleError::Unknown(other.to_owned()));
                    }
                }
            }
        }
        Ok(ExecResult { output, host })
    }
}

/// Resolve `idclev` / `idclev01` / `idclev e1m1` → map lump name.
fn parse_idclev_map(cmd: &str, args: &[String]) -> Option<String> {
    let rest = cmd.strip_prefix("idclev").unwrap_or("");
    let mut raw = String::new();
    raw.push_str(rest);
    for a in args {
        raw.push_str(a);
    }
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty() {
        return None;
    }
    // Explicit lump names: e1m1, map07, …
    if s.starts_with('e') && s.contains('m') {
        return Some(s.to_ascii_uppercase());
    }
    if s.starts_with("map") {
        return Some(s.to_ascii_uppercase());
    }
    // Two digits → Doom II / FreeDoom `MAPNN` (M5 default).
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 2 {
        let a = u32::from(digits.as_bytes()[0] - b'0');
        let b = u32::from(digits.as_bytes()[1] - b'0');
        let n = a * 10 + b;
        if (1..=32).contains(&n) {
            return Some(format!("MAP{n:02}"));
        }
    }
    None
}

fn default_config_path() -> std::path::PathBuf {
    if let Some(dir) = dirs_config() {
        return dir.join("room.ini");
    }
    std::path::PathBuf::from("room.ini")
}

fn dirs_config() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|h| {
            std::path::PathBuf::from(h)
                .join("Library")
                .join("Application Support")
                .join("room")
        })
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|a| std::path::PathBuf::from(a).join("room"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
            })
            .map(|p| p.join("room"))
    }
}

fn split_statements(line: &str) -> Vec<&str> {
    // Simple split on `;` (no quote awareness beyond tokenize).
    line.split(';').collect()
}

fn tokenize(stmt: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for ch in stmt.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::cvar::CvarValue;

    #[test]
    fn set_get_toggle() {
        let mut con = Console::with_defaults();
        con.exec("set cl_run 1").unwrap();
        assert_eq!(con.cvars.get("cl_run"), Some(&CvarValue::Bool(true)));
        let r = con.exec("get cl_run").unwrap();
        assert!(r.output[0].contains("true"));
        con.exec("toggle cl_run").unwrap();
        assert_eq!(con.cvars.get("cl_run"), Some(&CvarValue::Bool(false)));
        let rq = con.exec("r_quality 2").unwrap();
        assert!(rq.output.iter().any(|l| l.contains("preset")));
        assert_eq!(con.cvars.get("gl_bloom"), Some(&CvarValue::Bool(true)));
        assert_eq!(con.cvars.get("gl_ssao"), Some(&CvarValue::Int(1)));
        con.exec("echo hello world").unwrap();
        assert_eq!(
            con.scrollback.last().map(String::as_str),
            Some("hello world")
        );
    }

    #[test]
    fn alias_expands() {
        let mut con = Console::with_defaults();
        con.exec("alias hi echo hello").unwrap();
        let r = con.exec("hi").unwrap();
        assert_eq!(r.output, vec!["hello".to_owned()]);
        con.exec("unalias hi").unwrap();
        assert!(con.exec("hi").is_err());
    }

    #[test]
    fn host_cheats_and_map() {
        let mut con = Console::with_defaults();
        let r = con
            .exec("god; noclip; freeze; give all; map MAP01; kill monsters")
            .unwrap();
        assert_eq!(
            r.host,
            vec![
                HostCommand::God,
                HostCommand::Noclip,
                HostCommand::Freeze,
                HostCommand::Give("all".into()),
                HostCommand::Map("MAP01".into()),
                HostCommand::Kill(Some("monsters".into())),
            ]
        );
    }

    #[test]
    fn puke_and_pukename_emit_host() {
        let mut con = Console::with_defaults();
        let r = con.exec("puke 7 1 2; pukename DoorOpen always 3").unwrap();
        assert_eq!(
            r.host,
            vec![
                HostCommand::Puke {
                    script: 7,
                    args: [1, 2, 0],
                },
                HostCommand::PukeName {
                    name: "DoorOpen".into(),
                    always: true,
                    args: [3, 0, 0],
                },
            ]
        );
    }

    #[test]
    fn idclip_aliases_emit_noclip() {
        let mut con = Console::with_defaults();
        let r = con.exec("idclip; idspispopd").unwrap();
        assert_eq!(r.host, vec![HostCommand::Noclip, HostCommand::Noclip]);
    }

    #[test]
    fn idkfa_and_idfa_emit_give() {
        let mut con = Console::with_defaults();
        let r = con.exec("idkfa; idfa").unwrap();
        assert_eq!(
            r.host,
            vec![
                HostCommand::Give("idkfa".into()),
                HostCommand::Give("idfa".into()),
            ]
        );
    }

    #[test]
    fn idchoppers_and_idbehold_emit_give() {
        let mut con = Console::with_defaults();
        let r = con.exec("idchoppers; idbeholdv; idbehold s").unwrap();
        assert_eq!(
            r.host,
            vec![
                HostCommand::Give("idchoppers".into()),
                HostCommand::Give("idbeholdv".into()),
                HostCommand::Give("idbeholds".into()),
            ]
        );
    }

    #[test]
    fn idclev_and_idmypos_emit_host() {
        let mut con = Console::with_defaults();
        let r = con
            .exec("idclev01; idclev e1m8; idclev map07; idmypos")
            .unwrap();
        assert_eq!(
            r.host,
            vec![
                HostCommand::Map("MAP01".into()),
                HostCommand::Map("E1M8".into()),
                HostCommand::Map("MAP07".into()),
                HostCommand::Idmypos,
            ]
        );
    }

    #[test]
    fn iddt_emits_host_command() {
        let mut con = Console::with_defaults();
        let r = con.exec("iddt").unwrap();
        assert_eq!(r.host, vec![HostCommand::Iddt]);
    }

    #[test]
    fn pause_and_summon_emit_host() {
        let mut con = Console::with_defaults();
        let r = con.exec("pause; summon ZombieMan").unwrap();
        assert_eq!(
            r.host,
            vec![HostCommand::Pause, HostCommand::Summon("ZombieMan".into()),]
        );
    }

    #[test]
    fn screenshot_and_gif_emit_host() {
        let mut con = Console::with_defaults();
        let r = con.exec("screenshot; gifrecord").unwrap();
        assert_eq!(
            r.host,
            vec![HostCommand::Screenshot, HostCommand::GifToggle]
        );
    }

    #[test]
    fn save_and_load_emit_host() {
        let mut con = Console::with_defaults();
        let r = con.exec("save; load; save e1; loadgame e1").unwrap();
        assert_eq!(
            r.host,
            vec![
                HostCommand::Save(None),
                HostCommand::Load(None),
                HostCommand::Save(Some("e1".into())),
                HostCommand::Load(Some("e1".into())),
            ]
        );
    }

    #[test]
    fn record_and_playdemo_emit_host() {
        let mut con = Console::with_defaults();
        let r = con
            .exec("record; stop; playdemo; record e1; playdemo e1")
            .unwrap();
        assert_eq!(
            r.host,
            vec![
                HostCommand::Record(None),
                HostCommand::StopDemo,
                HostCommand::PlayDemo(None),
                HostCommand::Record(Some("e1".into())),
                HostCommand::PlayDemo(Some("e1".into())),
            ]
        );
    }
}
