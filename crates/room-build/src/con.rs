//! The level table of Duke's `USER.CON`: `definevolumename`,
//! `definelevelname` and the per-episode `music` lists. Everything else in
//! the CON language is ignored here.

/// One `definelevelname` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelInfo {
    /// Episode (0-based).
    pub volume: u8,
    /// Level within the episode (0-based).
    pub level: u8,
    /// Map file (upper case, e.g. `E1L1.MAP`).
    pub file: String,
    /// Par time in seconds, when it parses.
    pub par_seconds: Option<u32>,
    /// Display name.
    pub name: String,
    /// Music file (upper case, e.g. `STALKER.MID`).
    pub music: Option<String>,
}

/// Levels from `USER.CON` text, with episode names.
#[must_use]
pub fn parse_user_con(text: &str) -> (Vec<LevelInfo>, Vec<String>) {
    let mut levels = Vec::new();
    let mut volumes: Vec<String> = Vec::new();
    let mut music: Vec<(u8, Vec<String>)> = Vec::new();
    let mut lines = text.lines().map(strip_comment).peekable();
    while let Some(line) = lines.next() {
        let mut words = line.split_whitespace();
        match words.next().map(str::to_ascii_lowercase).as_deref() {
            Some("definevolumename") => {
                let Some(v) = words.next().and_then(|w| w.parse::<usize>().ok()) else {
                    continue;
                };
                if v < 16 {
                    if volumes.len() <= v {
                        volumes.resize(v + 1, String::new());
                    }
                    volumes[v] = words.collect::<Vec<_>>().join(" ");
                }
            }
            Some("definelevelname") => {
                let (Some(v), Some(l), Some(file)) = (
                    words.next().and_then(|w| w.parse::<u8>().ok()),
                    words.next().and_then(|w| w.parse::<u8>().ok()),
                    words.next(),
                ) else {
                    continue;
                };
                let par = words.next().and_then(parse_time);
                let _designer_time = words.next();
                levels.push(LevelInfo {
                    volume: v,
                    level: l,
                    file: file.to_ascii_uppercase(),
                    par_seconds: par,
                    name: words.collect::<Vec<_>>().join(" "),
                    music: None,
                });
            }
            Some("music") => {
                let Some(v) = words.next().and_then(|w| w.parse::<u8>().ok()) else {
                    continue;
                };
                let mut files: Vec<String> = words.map(str::to_ascii_uppercase).collect();
                // The list continues on following lines until the next keyword.
                while let Some(next) = lines.peek() {
                    let first = next.split_whitespace().next().unwrap_or("");
                    if first.is_empty() {
                        lines.next();
                        continue;
                    }
                    if !first.to_ascii_lowercase().ends_with(".mid") {
                        break;
                    }
                    files.extend(next.split_whitespace().map(str::to_ascii_uppercase));
                    lines.next();
                }
                music.push((v, files));
            }
            _ => {}
        }
    }
    for lvl in &mut levels {
        // `music 0` is the menu / briefing; episodes start at 1.
        lvl.music = music
            .iter()
            .find(|(v, _)| *v == lvl.volume + 1)
            .and_then(|(_, f)| f.get(usize::from(lvl.level)).cloned());
    }
    (levels, volumes)
}

/// `music 0` — the menu / briefing track (upper-case file name).
#[must_use]
pub fn parse_menu_music(text: &str) -> Option<String> {
    let mut lines = text.lines().map(strip_comment).peekable();
    while let Some(line) = lines.next() {
        let mut words = line.split_whitespace();
        if !words
            .next()
            .is_some_and(|w| w.eq_ignore_ascii_case("music"))
        {
            continue;
        }
        if words.next().and_then(|w| w.parse::<u8>().ok()) != Some(0) {
            continue;
        }
        let mut files: Vec<String> = words.map(str::to_ascii_uppercase).collect();
        while let Some(next) = lines.peek() {
            let first = next.split_whitespace().next().unwrap_or("");
            if first.is_empty() {
                lines.next();
                continue;
            }
            if !first.to_ascii_lowercase().ends_with(".mid") {
                break;
            }
            files.extend(next.split_whitespace().map(str::to_ascii_uppercase));
            lines.next();
        }
        return files.into_iter().find(|f| !f.is_empty());
    }
    None
}

/// `defineskillname` entries from `USER.CON`, in skill order.
#[must_use]
pub fn parse_skill_names(text: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for line in text.lines().map(strip_comment) {
        let mut words = line.split_whitespace();
        if !words
            .next()
            .is_some_and(|w| w.eq_ignore_ascii_case("defineskillname"))
        {
            continue;
        }
        let Some(n) = words.next().and_then(|w| w.parse::<usize>().ok()) else {
            continue;
        };
        if n < 8 {
            if names.len() <= n {
                names.resize(n + 1, String::new());
            }
            names[n] = words.collect::<Vec<_>>().join(" ");
        }
    }
    names
}

fn strip_comment(line: &str) -> &str {
    line.find("//").map_or(line, |i| &line[..i])
}

fn parse_time(s: &str) -> Option<u32> {
    let (m, sec) = s.split_once(':')?;
    Some(m.parse::<u32>().ok()? * 60 + sec.parse::<u32>().ok()?)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn reads_levels_volumes_and_music() {
        let text = "definevolumename 0 L.A. MELTDOWN\n\
            definelevelname 0 0 E1L1.map 01:45 00:53 HOLLYWOOD HOLOCAUST // c\n\
            definelevelname 0 1 e1l2.map 0T:HX 03:21 RED LIGHT DISTRICT\n\
            music 0 GRABBAG.MID\n\
            music 1 stalker.mid\n\
            \n        dethtoll.mid\n\
            definequote 1 hi\n";
        let (levels, vols) = parse_user_con(text);
        assert_eq!(vols, ["L.A. MELTDOWN"]);
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[0].file, "E1L1.MAP");
        assert_eq!(levels[0].par_seconds, Some(105));
        assert_eq!(levels[0].name, "HOLLYWOOD HOLOCAUST");
        assert_eq!(levels[0].music.as_deref(), Some("STALKER.MID"));
        assert_eq!(parse_menu_music(text).as_deref(), Some("GRABBAG.MID"));
        assert_eq!(levels[1].par_seconds, None);
        assert_eq!(levels[1].music.as_deref(), Some("DETHTOLL.MID"));
    }

    #[test]
    fn reads_skill_names() {
        let text = "defineskillname 1 LET'S ROCK\ndefineskillname 0 PIECE OF CAKE // x\n\
                    defineskillname x BAD\n";
        assert_eq!(parse_skill_names(text), ["PIECE OF CAKE", "LET'S ROCK"]);
    }
}
