//! Built-in list menu (M5/M13; MENUDEF via [`crate::menudef_runtime`]).

use crate::doom_text::draw_stcfn_string;
use crate::draw::{Rect, Rgba, UiDrawList, Vec2};
use crate::font::{FontId, FontRegistry};

/// One selectable row.
#[derive(Clone, Debug)]
pub struct MenuItem {
    /// Label shown to the player (when `patch` is empty).
    pub label: String,
    /// Command / submenu id executed on activate.
    pub action: String,
    /// Optional Doom patch lump drawn instead of (or beside) text (`M_NGAME`, …).
    pub patch: Option<String>,
}

impl MenuItem {
    /// Text-only row.
    #[must_use]
    pub fn text(label: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            action: action.into(),
            patch: None,
        }
    }

    /// Patch row (Doom main-menu style).
    #[must_use]
    pub fn patch_item(patch: impl Into<String>, action: impl Into<String>) -> Self {
        let patch = patch.into();
        Self {
            label: patch.clone(),
            action: action.into(),
            patch: Some(patch),
        }
    }
}

/// Simple vertical list menu (Doom main menu shape).
#[derive(Clone, Debug)]
pub struct ListMenu {
    /// Title (empty = no title banner).
    pub title: String,
    /// Items.
    pub items: Vec<MenuItem>,
    /// Selected index.
    pub selected: usize,
    /// First visible item index (long lists / Graphics).
    pub scroll: usize,
    /// How many rows fit on screen (`0` = draw all, no scroll).
    pub visible_rows: usize,
    /// Top-left of the list in virtual coords.
    pub origin: Vec2,
    /// Dim the playfield behind the menu (pause / options). Off for title+MENUDEF.
    pub dim_background: bool,
    /// Vertical pitch between rows.
    pub line_spacing: f32,
    /// Hide the title string (MENUDEF patch menus already have art).
    pub show_title: bool,
}

impl ListMenu {
    /// In-game pause menu.
    #[must_use]
    pub fn pause() -> Self {
        Self {
            title: "PAUSE".into(),
            items: vec![
                MenuItem::text("RESUME", "menu_resume"),
                MenuItem::text("SAVE GAME", "menu_savegame"),
                MenuItem::text("LOAD GAME", "menu_loadgame"),
                MenuItem::text("OPTIONS", "openmenu OptionsMenu"),
                MenuItem::text("END GAME", "menu_endgame"),
                MenuItem::text("QUIT", "menu_quit"),
            ],
            selected: 0,
            scroll: 0,
            visible_rows: 0,
            // Doom/GZDoom-style column (BigFont + skull); no chrome panel.
            origin: Vec2::new(104.0, 72.0),
            dim_background: true,
            line_spacing: 18.0,
            show_title: true,
        }
    }

    /// Quit confirmation.
    #[must_use]
    pub fn quit_confirm() -> Self {
        Self {
            title: "ARE YOU SURE YOU WANT TO QUIT?".into(),
            items: vec![
                MenuItem::text("YES", "menu_quit_yes"),
                MenuItem::text("NO", "menu_quit_no"),
            ],
            selected: 1,
            scroll: 0,
            visible_rows: 0,
            origin: Vec2::new(96.0, 96.0),
            dim_background: true,
            line_spacing: 16.0,
            show_title: true,
        }
    }

    /// End-game confirmation (return to title).
    #[must_use]
    pub fn endgame_confirm() -> Self {
        Self {
            title: "END CURRENT GAME?".into(),
            items: vec![
                MenuItem::text("YES", "menu_endgame_yes"),
                MenuItem::text("NO", "menu_endgame_no"),
            ],
            selected: 1,
            scroll: 0,
            visible_rows: 0,
            origin: Vec2::new(100.0, 96.0),
            dim_background: true,
            line_spacing: 16.0,
            show_title: true,
        }
    }

    /// Doom-like main menu stub (text fallback when MENUDEF missing).
    #[must_use]
    pub fn doom_main() -> Self {
        Self {
            title: String::new(),
            items: vec![
                MenuItem::patch_item("M_NGAME", "menu_newgame"),
                MenuItem::patch_item("M_OPTION", "openmenu OptionsMenu"),
                MenuItem::patch_item("M_LOADG", "menu_loadgame"),
                MenuItem::patch_item("M_SAVEG", "menu_savegame"),
                MenuItem::patch_item("M_QUITG", "menu_quit"),
            ],
            selected: 0,
            scroll: 0,
            visible_rows: 0,
            origin: Vec2::new(97.0, 72.0),
            dim_background: false,
            line_spacing: 16.0,
            show_title: false,
        }
    }

    /// Doom II / FreeDoom Phase 2 skill picker (no episode select).
    #[must_use]
    pub fn doom_skill() -> Self {
        Self {
            title: "Choose Skill Level:".into(),
            items: vec![
                MenuItem::text("I'm too young to die.", "skill 1"),
                MenuItem::text("Hey, not too rough.", "skill 2"),
                MenuItem::text("Hurt me plenty.", "skill 3"),
                MenuItem::text("Ultra-Violence.", "skill 4"),
                MenuItem::text("Nightmare!", "skill 5"),
            ],
            selected: 2,
            scroll: 0,
            visible_rows: 0,
            origin: Vec2::new(48.0, 64.0),
            dim_background: true,
            line_spacing: 16.0,
            show_title: true,
        }
    }

    /// Skill picker with MAPINFO names (`skill 1`… in order, at most five).
    #[must_use]
    pub fn named_skill(names: &[String]) -> Self {
        let mut m = Self::doom_skill();
        if names.is_empty() {
            return m;
        }
        m.items = names
            .iter()
            .take(5)
            .enumerate()
            .map(|(i, n)| MenuItem::text(n.as_str(), format!("skill {}", i + 1)))
            .collect();
        m.selected = m.selected.min(m.items.len() - 1);
        m
    }

    /// Episode picker using MAPINFO names (`episode 1`… in order).
    #[must_use]
    pub fn named_episode(names: &[String]) -> Self {
        if names.is_empty() {
            return Self::doom_episode();
        }
        Self {
            title: "Select Episode".into(),
            items: names
                .iter()
                .enumerate()
                .map(|(i, n)| MenuItem::text(n.as_str(), format!("episode {}", i + 1)))
                .collect(),
            selected: 0,
            scroll: 0,
            visible_rows: 0,
            origin: Vec2::new(48.0, 56.0),
            dim_background: true,
            line_spacing: 16.0,
            show_title: true,
        }
    }

    /// Ultimate Doom episode picker.
    #[must_use]
    pub fn doom_episode() -> Self {
        Self {
            title: "Which Episode?".into(),
            items: vec![
                MenuItem::text("Knee-Deep in the Dead", "episode 1"),
                MenuItem::text("The Shores of Hell", "episode 2"),
                MenuItem::text("Inferno", "episode 3"),
                MenuItem::text("Thy Flesh Consumed", "episode 4"),
            ],
            selected: 0,
            scroll: 0,
            visible_rows: 0,
            origin: Vec2::new(48.0, 64.0),
            dim_background: true,
            line_spacing: 16.0,
            show_title: true,
        }
    }

    /// Keep [`Self::scroll`] so [`Self::selected`] stays in the visible window.
    pub fn ensure_scroll_visible(&mut self) {
        let vis = self.visible_rows;
        if vis == 0 || self.items.is_empty() {
            self.scroll = 0;
            return;
        }
        let vis = vis.min(self.items.len());
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + vis {
            self.scroll = self.selected + 1 - vis;
        }
        let max_scroll = self.items.len().saturating_sub(vis);
        self.scroll = self.scroll.min(max_scroll);
    }

    /// Move selection.
    pub fn move_cursor(&mut self, delta: i32) {
        if self.items.is_empty() {
            return;
        }
        let n = self.items.len() as i32;
        let cur = self.selected as i32 + delta;
        self.selected = cur.rem_euclid(n) as usize;
        // Skip noop spacer rows.
        for _ in 0..self.items.len() {
            if self.items[self.selected].action != "noop" {
                break;
            }
            self.selected = (self.selected + if delta >= 0 { 1 } else { self.items.len() - 1 })
                % self.items.len();
        }
        self.ensure_scroll_visible();
    }

    /// Activate selection; returns action string.
    #[must_use]
    pub fn activate(&self) -> Option<&str> {
        let item = self.items.get(self.selected)?;
        if item.action == "noop" {
            return None;
        }
        Some(item.action.as_str())
    }

    /// Top-left of the selectable list in virtual coords.
    #[must_use]
    fn list_origin(&self) -> Vec2 {
        self.origin
    }

    /// Hit-test a virtual 320×200 point; returns item index when inside a row.
    #[must_use]
    pub fn hit_test(&self, x: f32, y: f32) -> Option<usize> {
        let spacing = self.line_spacing.max(8.0);
        let origin = self.list_origin();
        let start = self.scroll;
        let end = if self.visible_rows == 0 {
            self.items.len()
        } else {
            (start + self.visible_rows).min(self.items.len())
        };
        for (row, i) in (start..end).enumerate() {
            let item = &self.items[i];
            if item.action == "noop" {
                continue;
            }
            let row_y = origin.y + row as f32 * spacing;
            let h = if item.patch.is_some() { 16.0 } else { 12.0 };
            let w = 280.0;
            if x >= origin.x - 24.0 && x <= origin.x + w && y >= row_y && y <= row_y + h {
                return Some(i);
            }
        }
        None
    }

    /// Draw into a UI list. `tic` animates the skull selector.
    ///
    /// Text rows prefer `BigFont` (`DBIGFONT`) when present; otherwise native
    /// `STCFN*` patches (Doom 1 IWADs). Dimmed menus only darken the playfield
    /// — no custom chrome box / selection bar.
    pub fn draw(&self, list: &mut UiDrawList, tic: u32, fonts: Option<&FontRegistry>) {
        let spacing = self.line_spacing.max(8.0);
        let list_origin = self.list_origin();
        let (list_x, list_y) = (list_origin.x, list_origin.y);
        let big = fonts.and_then(|r| r.get(FontId::BIG));
        // Scrollable menus use STCFN patches (see item loop); SmallFont SoftFont
        // is reserved for non-menu SoftFont callers.
        let use_small = self.visible_rows > 0;
        if self.dim_background {
            list.fill(
                Rect::new(0.0, 0.0, 320.0, 200.0),
                Rgba::new(0.0, 0.0, 0.0, 0.55),
            );
        }
        if self.show_title && !self.title.is_empty() {
            let ty = (list_y - 22.0).max(8.0);
            if let Some(font) = big {
                let tw = font.measure(&self.title);
                let tx = ((320.0 - tw) * 0.5).max(8.0);
                font.draw(
                    list,
                    FontId::BIG,
                    Vec2::new(tx, ty),
                    self.title.clone(),
                    Rgba::WHITE,
                    1.0,
                );
            } else {
                let tw = self.title.len() as f32 * 9.0;
                let tx = ((320.0 - tw) * 0.5).max(8.0);
                draw_stcfn_string(list, tx, ty, &self.title, 1.0);
            }
        }
        let skull = if (tic / 8) % 2 == 0 {
            "M_SKULL1"
        } else {
            "M_SKULL2"
        };
        let start = self.scroll;
        let end = if self.visible_rows == 0 {
            self.items.len()
        } else {
            (start + self.visible_rows).min(self.items.len())
        };
        if self.visible_rows > 0 && start > 0 {
            draw_stcfn_string(list, list_x + 200.0, list_y - 10.0, "^ MORE", 0.85);
        }
        for (row, i) in (start..end).enumerate() {
            let item = &self.items[i];
            let y = list_y + row as f32 * spacing;
            let selected = i == self.selected;
            if selected && item.action != "noop" {
                list.patch(
                    skull,
                    Vec2::new(list_x - 32.0, y - 2.0),
                    Vec2::new(0.0, 0.0),
                );
            }
            if let Some(patch) = &item.patch {
                list.patch(patch, Vec2::new(list_x, y), Vec2::new(0.0, 0.0));
            } else if item.action == "noop" {
                draw_stcfn_string(list, list_x, y, &item.label, 0.7);
            } else if use_small {
                // Dense scroll menus (Graphics): STCFN patches keep palette
                // shading + fixed pitch. SoftFont SmallFont fill-rects fatten
                // edge-ink glyphs into an unreadable mash at framebuffer scale.
                draw_stcfn_string(list, list_x, y, &item.label, 1.0);
            } else if let Some(font) = big {
                font.draw(
                    list,
                    FontId::BIG,
                    Vec2::new(list_x, y),
                    item.label.clone(),
                    Rgba::WHITE,
                    1.0,
                );
            } else {
                draw_stcfn_string(list, list_x, y, &item.label, 1.0);
            }
        }
        if self.visible_rows > 0 && end < self.items.len() {
            let foot_y = list_y + (end - start) as f32 * spacing + 2.0;
            draw_stcfn_string(list, list_x + 180.0, foot_y.min(190.0), "v MORE", 0.85);
        }
    }
}

/// Doom menu SFX logical names (SNDINFO; shell plays them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuSfx {
    /// Cursor moved (`menu/cursor` → DSPISTOL).
    Cursor,
    /// Item activated / submenu opened (`menu/activate`).
    Activate,
    /// Backed out one level (`menu/backup`).
    Backup,
}

impl MenuSfx {
    /// SNDINFO logical name.
    #[must_use]
    pub const fn logical(self) -> &'static str {
        match self {
            Self::Cursor => "menu/cursor",
            Self::Activate => "menu/activate",
            Self::Backup => "menu/backup",
        }
    }
}

/// Result of handling a menu action (shell interprets).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuCommand {
    /// Open another named menu.
    OpenMenu(String),
    /// Start a new game at skill 1..=5.
    StartSkill(u8),
    /// Select episode 1..=4 (Doom 1).
    SelectEpisode(u8),
    /// Quit request (show confirm, or confirmed).
    Quit,
    /// Confirmed quit — exit the process.
    QuitConfirmed,
    /// Close pause menu and resume the level.
    Resume,
    /// Ask to end the current game (return to title).
    EndGame,
    /// Confirmed end game.
    EndGameConfirmed,
    /// Load-game menu stub.
    LoadGame,
    /// Save-game menu stub.
    SaveGame,
    /// Read This / help stub.
    ReadThis,
    /// Unhandled / passthrough action string.
    Other(String),
}

/// Parse a menu item `action` into a command.
#[must_use]
pub fn parse_menu_action(action: &str) -> MenuCommand {
    if let Some(rest) = action.strip_prefix("openmenu ") {
        return MenuCommand::OpenMenu(rest.trim().to_owned());
    }
    if action == "menu_newgame" {
        return MenuCommand::OpenMenu("SkillMenu".into());
    }
    if action == "menu_quit" || action == "quit" {
        return MenuCommand::Quit;
    }
    if action == "menu_quit_yes" {
        return MenuCommand::QuitConfirmed;
    }
    if action == "menu_quit_no" || action == "menu_endgame_no" {
        return MenuCommand::Other("menu_back".into());
    }
    if action == "menu_resume" || action == "resume" {
        return MenuCommand::Resume;
    }
    if action == "menu_endgame" {
        return MenuCommand::EndGame;
    }
    if action == "menu_endgame_yes" {
        return MenuCommand::EndGameConfirmed;
    }
    if action == "menu_loadgame" {
        return MenuCommand::LoadGame;
    }
    if action == "menu_savegame" {
        return MenuCommand::SaveGame;
    }
    if action == "menu_readthis" {
        return MenuCommand::ReadThis;
    }
    if let Some(n) = action.strip_prefix("skill ") {
        if let Ok(level) = n.trim().parse::<u8>() {
            return MenuCommand::StartSkill(level.clamp(1, 5));
        }
    }
    if let Some(n) = action.strip_prefix("episode ") {
        if let Ok(ep) = n.trim().parse::<u8>() {
            return MenuCommand::SelectEpisode(ep.clamp(1, 4));
        }
    }
    MenuCommand::Other(action.to_owned())
}

/// Push/pop stack for main → skill / episode flow.
#[derive(Clone, Debug, Default)]
pub struct MenuStack {
    /// Active menus (top is drawn / receives input).
    pub stack: Vec<ListMenu>,
    /// Optional MENUDEF for resolving `openmenu` targets.
    pub menudef: Option<room_defs::MenuDef>,
    /// MAPINFO skill names for the new-game skill page (empty: Doom's).
    pub skill_names: Vec<String>,
    /// MAPINFO episode titles (empty: Doom's Knee-Deep / …).
    pub episode_names: Vec<String>,
    /// Start map for each episode (parallel to [`Self::episode_names`]).
    pub episode_maps: Vec<String>,
    /// Last chosen episode (1-based).
    pub episode: u8,
}

impl MenuStack {
    /// Open Doom main menu as the sole root.
    #[must_use]
    pub fn doom_main() -> Self {
        Self {
            stack: vec![ListMenu::doom_main()],
            ..Self::default()
        }
    }

    /// Use the MAPINFO `skill` names (unnamed skills keep their id).
    #[must_use]
    pub fn with_skills(mut self, skills: &[room_defs::SkillDef]) -> Self {
        self.skill_names = skills
            .iter()
            .map(|s| s.name.clone().unwrap_or_else(|| s.id.clone()))
            .collect();
        self
    }

    /// Use the MAPINFO episode names and start maps.
    #[must_use]
    pub fn with_episodes(mut self, episodes: &[room_defs::EpisodeDef]) -> Self {
        self.episode_names = episodes
            .iter()
            .map(|e| e.name.clone().unwrap_or_else(|| e.start_map.clone()))
            .collect();
        self.episode_maps = episodes.iter().map(|e| e.start_map.clone()).collect();
        self
    }

    /// Start map for the last chosen episode.
    #[must_use]
    pub fn start_map(&self) -> Option<&str> {
        let i = usize::from(self.episode.saturating_sub(1));
        self.episode_maps.get(i).map(String::as_str)
    }

    /// Prefer MENUDEF `MainMenu` when available; else built-in Doom main.
    #[must_use]
    pub fn from_menudef_or_doom(defs: Option<&room_defs::MenuDef>) -> Self {
        if let Some(defs) = defs {
            if let Some(mut m) = crate::menudef_runtime::list_menu_from_def(
                defs,
                "MainMenu",
                crate::menudef_runtime::MenuBuildOpts::default(),
            ) {
                m.dim_background = false;
                m.show_title = false;
                return Self {
                    stack: vec![m],
                    menudef: Some(defs.clone()),
                    ..Self::default()
                };
            }
        }
        Self::doom_main()
    }

    /// Open the in-game pause menu.
    #[must_use]
    pub fn pause() -> Self {
        Self {
            stack: vec![ListMenu::pause()],
            ..Self::default()
        }
    }

    /// Top menu, if any.
    #[must_use]
    pub fn top(&self) -> Option<&ListMenu> {
        self.stack.last()
    }

    /// Mutable top.
    pub fn top_mut(&mut self) -> Option<&mut ListMenu> {
        self.stack.last_mut()
    }

    /// Move cursor on the top menu. Returns [`MenuSfx::Cursor`] when the selection changes.
    pub fn move_cursor(&mut self, delta: i32) -> Option<MenuSfx> {
        let m = self.top_mut()?;
        if m.items.is_empty() || delta == 0 {
            return None;
        }
        let prev = m.selected;
        m.move_cursor(delta);
        (m.selected != prev).then_some(MenuSfx::Cursor)
    }

    /// Select item under virtual coords; returns cursor SFX when selection changes.
    pub fn hover(&mut self, x: f32, y: f32) -> Option<MenuSfx> {
        let m = self.top_mut()?;
        let idx = m.hit_test(x, y)?;
        if idx == m.selected {
            return None;
        }
        m.selected = idx;
        m.ensure_scroll_visible();
        Some(MenuSfx::Cursor)
    }

    /// Activate top selection; may push skill menu / return a command.
    /// Second value is the cue to play (`Activate` when something happened).
    pub fn activate(&mut self) -> (Option<MenuCommand>, Option<MenuSfx>) {
        let Some(action) = self.top().and_then(ListMenu::activate).map(str::to_owned) else {
            return (None, None);
        };
        let cmd = parse_menu_action(&action);
        match &cmd {
            MenuCommand::OpenMenu(name) if name.eq_ignore_ascii_case("SkillMenu") => {
                self.stack.push(ListMenu::named_skill(&self.skill_names));
                (None, Some(MenuSfx::Activate))
            }
            MenuCommand::OpenMenu(name) if name.eq_ignore_ascii_case("EpisodeMenu") => {
                if let Some(defs) = self.menudef.as_ref() {
                    if let Some(mut m) = crate::menudef_runtime::list_menu_from_def(
                        defs,
                        name,
                        crate::menudef_runtime::MenuBuildOpts::default(),
                    ) {
                        m.dim_background = true;
                        m.show_title = !m.title.is_empty();
                        self.stack.push(m);
                        return (None, Some(MenuSfx::Activate));
                    }
                }
                self.stack
                    .push(ListMenu::named_episode(&self.episode_names));
                (None, Some(MenuSfx::Activate))
            }
            MenuCommand::Quit => {
                self.stack.push(ListMenu::quit_confirm());
                (None, Some(MenuSfx::Activate))
            }
            MenuCommand::EndGame => {
                self.stack.push(ListMenu::endgame_confirm());
                (None, Some(MenuSfx::Activate))
            }
            MenuCommand::OpenMenu(name) => {
                if let Some(defs) = self.menudef.as_ref() {
                    if let Some(mut m) = crate::menudef_runtime::any_menu_from_def(
                        defs,
                        name,
                        crate::menudef_runtime::MenuBuildOpts::default(),
                    ) {
                        m.dim_background = true;
                        m.show_title = !m.title.is_empty();
                        self.stack.push(m);
                        return (None, Some(MenuSfx::Activate));
                    }
                }
                // Wired Options (prefer over MENUDEF stubs so toggles actually work).
                // Graphics submenu is pushed by the shell with live CVar labels
                // (`openmenu GraphicsMenu` → [`MenuCommand::OpenMenu`]).
                if name.eq_ignore_ascii_case("Options") || name.eq_ignore_ascii_case("OptionsMenu")
                {
                    self.stack.push(ListMenu {
                        title: "Options".into(),
                        items: vec![
                            MenuItem::text("GRAPHICS", "openmenu GraphicsMenu"),
                            MenuItem::text("AUTO AIM", "option autoaim"),
                            MenuItem::text("MOUSE AIMING", "option mouse_look"),
                            MenuItem::text("INVERT MOUSE", "option invertmouse"),
                            MenuItem::text("ALWAYS RUN", "option cl_run"),
                            MenuItem::text("SHOW FPS", "option vid_fps"),
                            MenuItem::text("SENSITIVITY +", "cmd set m_sensitivity 1.5"),
                            MenuItem::text("SENSITIVITY RESET", "cmd set m_sensitivity 1"),
                            MenuItem::text("WRITE CONFIG", "cmd writeconfig"),
                            MenuItem::text("BACK", "menu_back"),
                        ],
                        selected: 0,
                        scroll: 0,
                        visible_rows: 0,
                        origin: Vec2::new(48.0, 48.0),
                        dim_background: true,
                        line_spacing: 14.0,
                        show_title: true,
                    });
                    return (None, Some(MenuSfx::Activate));
                }
                if name.eq_ignore_ascii_case("GraphicsMenu")
                    || name.eq_ignore_ascii_case("Graphics")
                {
                    // Shell replaces this stub with live CVar labels when it
                    // receives OpenMenu("GraphicsMenu").
                    return (Some(cmd), Some(MenuSfx::Activate));
                }
                (Some(cmd), Some(MenuSfx::Activate))
            }
            MenuCommand::SelectEpisode(n) => {
                self.episode = *n;
                self.stack.push(ListMenu::named_skill(&self.skill_names));
                (Some(cmd), Some(MenuSfx::Activate))
            }
            MenuCommand::Other(a) if a == "menu_back" => {
                let _ = self.backup();
                (None, Some(MenuSfx::Backup))
            }
            other => (Some(other.clone()), Some(MenuSfx::Activate)),
        }
    }

    /// Replace the top menu (used to refresh live CVar labels) or push if empty.
    pub fn replace_top(&mut self, menu: ListMenu) {
        if let Some(top) = self.stack.last_mut() {
            let sel = top.selected.min(menu.items.len().saturating_sub(1));
            *top = menu;
            top.selected = sel;
        } else {
            self.stack.push(menu);
        }
    }

    /// Push a menu onto the stack.
    pub fn push_menu(&mut self, menu: ListMenu) {
        self.stack.push(menu);
    }

    /// Title of the top menu, if any.
    #[must_use]
    pub fn top_title(&self) -> Option<&str> {
        self.top().map(|m| m.title.as_str())
    }

    /// Pop one menu (Escape / backup). Returns whether a menu remains and an optional cue.
    pub fn backup(&mut self) -> (bool, Option<MenuSfx>) {
        if self.stack.len() > 1 {
            self.stack.pop();
            return (!self.stack.is_empty(), Some(MenuSfx::Backup));
        }
        (!self.stack.is_empty(), None)
    }

    /// Draw the top menu.
    pub fn draw(&self, list: &mut UiDrawList, tic: u32, fonts: Option<&FontRegistry>) {
        if let Some(m) = self.top() {
            m.draw(list, tic, fonts);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn pause_menu_resume_parses() {
        let mut stack = MenuStack::pause();
        assert_eq!(
            stack.activate(),
            (Some(MenuCommand::Resume), Some(MenuSfx::Activate))
        );
    }

    #[test]
    fn pause_menu_draw_emits_bigfont_text() {
        let menu = ListMenu::pause();
        let mut list = UiDrawList::new();
        // Without fonts → STCFN fallback; with empty registry same.
        menu.draw(&mut list, 0, None);
        let stcfn = list
            .cmds
            .iter()
            .filter(|c| {
                matches!(
                    c,
                    crate::draw::DrawCmd::Patch { name, .. } if name.starts_with("STCFN")
                )
            })
            .count();
        assert!(
            stcfn >= 20,
            "STCFN fallback for Doom1-style IWADs, got {stcfn}"
        );
        let skull = list.cmds.iter().any(|c| {
            matches!(
                c,
                crate::draw::DrawCmd::Patch { name, .. }
                    if name == "M_SKULL1" || name == "M_SKULL2"
            )
        });
        assert!(skull, "expected animated skull selector");
        let fills = list
            .cmds
            .iter()
            .filter(|c| matches!(c, crate::draw::DrawCmd::Fill { .. }))
            .count();
        assert_eq!(fills, 1, "pause should only dim the playfield");
    }

    #[test]
    fn pause_menu_uses_bigfont_when_loaded() {
        let Some(wad) = freedoom2_wad() else {
            return;
        };
        let vfs = room_vfs::VfsBuilder::new()
            .mount_path(&wad)
            .unwrap()
            .build()
            .unwrap();
        let fonts = FontRegistry::load_doom_fonts(&vfs);
        let menu = ListMenu::pause();
        let mut list = UiDrawList::new();
        menu.draw(&mut list, 0, Some(&fonts));
        let labels: Vec<_> = list
            .cmds
            .iter()
            .filter_map(|c| match c {
                crate::draw::DrawCmd::Text { font, text, .. } if font == "BigFont" => {
                    Some(text.as_str())
                }
                _ => None,
            })
            .collect();
        assert!(labels.contains(&"PAUSE"));
        assert!(labels.contains(&"RESUME"));
        assert!(labels.contains(&"QUIT"));
    }

    fn freedoom2_wad() -> Option<std::path::PathBuf> {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/freedoom/freedoom2.wad");
        p.is_file().then_some(p)
    }

    #[test]
    fn quit_pushes_confirm() {
        let mut stack = MenuStack::pause();
        while stack.top().unwrap().items[stack.top().unwrap().selected].action != "menu_quit" {
            let _ = stack.move_cursor(1);
        }
        let (cmd, sfx) = stack.activate();
        assert_eq!(cmd, None);
        assert_eq!(sfx, Some(MenuSfx::Activate));
        assert_eq!(stack.stack.len(), 2);
        stack.top_mut().unwrap().selected = 0; // Yes
        assert_eq!(
            stack.activate(),
            (Some(MenuCommand::QuitConfirmed), Some(MenuSfx::Activate))
        );
    }

    #[test]
    fn cursor_wraps_and_activates() {
        let mut m = ListMenu::doom_main();
        m.move_cursor(-1);
        assert_eq!(m.selected, m.items.len() - 1);
        assert_eq!(m.activate(), Some("menu_quit"));
        m.move_cursor(1);
        assert_eq!(m.selected, 0);
        let mut list = UiDrawList::new();
        m.draw(&mut list, 0, None);
        assert!(list.cmds.len() >= 4);
    }

    #[test]
    fn mapinfo_skill_names_replace_doom_skills() {
        let skills = [
            room_defs::SkillDef {
                id: "easy".into(),
                name: Some("Piece Of Cake".into()),
            },
            room_defs::SkillDef {
                id: "normal".into(),
                name: None,
            },
        ];
        let mut stack = MenuStack::doom_main().with_skills(&skills);
        let _ = stack.activate();
        let top = stack.top().unwrap();
        let rows: Vec<_> = top
            .items
            .iter()
            .map(|i| (i.label.as_str(), i.action.as_str()))
            .collect();
        assert_eq!(rows, [("Piece Of Cake", "skill 1"), ("normal", "skill 2")]);
        let plain = MenuStack::doom_main();
        assert!(plain.skill_names.is_empty());
    }

    #[test]
    fn hit_test_selects_row() {
        let m = ListMenu::doom_skill();
        let o = m.list_origin();
        let idx = m.hit_test(o.x + 8.0, o.y + 2.0 * m.line_spacing).unwrap();
        assert_eq!(idx, 2);
    }

    #[test]
    fn parse_actions() {
        assert_eq!(parse_menu_action("skill 4"), MenuCommand::StartSkill(4));
        assert_eq!(
            parse_menu_action("episode 2"),
            MenuCommand::SelectEpisode(2)
        );
        assert_eq!(parse_menu_action("menu_loadgame"), MenuCommand::LoadGame);
        assert_eq!(
            parse_menu_action("menu_quit_yes"),
            MenuCommand::QuitConfirmed
        );
    }

    #[test]
    fn episode_select_remembers_the_start_map() {
        let episodes = [
            room_defs::EpisodeDef {
                start_map: "E1M1".into(),
                name: Some("L.A. Meltdown".into()),
            },
            room_defs::EpisodeDef {
                start_map: "E2M1".into(),
                name: Some("Lunar Apocalypse".into()),
            },
        ];
        let mut stack = MenuStack::doom_main().with_episodes(&episodes);
        stack
            .stack
            .push(ListMenu::named_episode(&stack.episode_names.clone()));
        if let Some(top) = stack.stack.last_mut() {
            top.selected = 1;
        }
        let (cmd, _) = stack.activate();
        assert_eq!(cmd, Some(MenuCommand::SelectEpisode(2)));
        assert_eq!(stack.episode, 2);
        assert_eq!(stack.start_map(), Some("E2M1"));
    }
}
