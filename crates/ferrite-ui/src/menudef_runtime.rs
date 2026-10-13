//! Build runtime [`ListMenu`]s from parsed MENUDEF (M7 bootstrap).

use crate::draw::Vec2;
use crate::menu::{ListMenu, MenuItem};
use ferrite_defs::{MenuBlockKind, MenuCmd, MenuDef};

/// Games recognised by `IfGame(...)` filters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuGame {
    /// Doom / Freedoom.
    Doom,
    /// Chex Quest.
    Chex,
    /// Heretic.
    Heretic,
    /// Hexen.
    Hexen,
    /// Strife.
    Strife,
}

/// Options controlling which MENUDEF conditionals are active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuBuildOpts {
    /// Active game for `IfGame`.
    pub game: MenuGame,
    /// `IfOption(ReadThis)`.
    pub read_this: bool,
    /// `IfOption(SwapMenu)`.
    pub swap_menu: bool,
}

impl Default for MenuBuildOpts {
    fn default() -> Self {
        Self {
            game: MenuGame::Doom,
            read_this: false,
            swap_menu: false,
        }
    }
}

/// Build a [`ListMenu`] from a named MENUDEF `ListMenu` block.
///
/// Collects `TextItem` / `PatchItem` rows; skips static patches. Nested
/// `IfGame` / `IfOption` are evaluated with [`MenuBuildOpts`].
#[must_use]
pub fn list_menu_from_def(defs: &MenuDef, name: &str, opts: MenuBuildOpts) -> Option<ListMenu> {
    let idx = *defs.by_name.get(name)?;
    let block = defs.blocks.get(idx)?;
    if !matches!(block.kind, MenuBlockKind::ListMenu) {
        return None;
    }
    let mut items = Vec::new();
    let mut origin = Vec2::new(97.0, 72.0);
    let mut title = name.to_owned();
    collect_items(
        &block.body,
        opts,
        &mut items,
        &mut origin,
        &mut title,
        false,
    );
    if items.is_empty() {
        return None;
    }
    let show_title = !title.is_empty() && title != name;
    Some(ListMenu {
        title,
        items,
        selected: 0,
        origin,
        dim_background: false,
        line_spacing: 16.0,
        show_title,
    })
}

/// Build a [`ListMenu`] from a named MENUDEF `OptionMenu` block.
///
/// Maps `Submenu` / `Command` / `SafeCommand` / `Option` / non-blank `StaticText`
/// into selectable rows (cvar toggles are display-only stubs for now).
#[must_use]
pub fn option_menu_from_def(defs: &MenuDef, name: &str, opts: MenuBuildOpts) -> Option<ListMenu> {
    let idx = *defs.by_name.get(name)?;
    let block = defs.blocks.get(idx)?;
    if !matches!(block.kind, MenuBlockKind::OptionMenu) {
        return None;
    }
    let mut items = Vec::new();
    let mut origin = Vec2::new(48.0, 40.0);
    let mut title = name.to_owned();
    collect_items(&block.body, opts, &mut items, &mut origin, &mut title, true);
    if items.is_empty() {
        return None;
    }
    Some(ListMenu {
        title,
        items,
        selected: 0,
        origin,
        dim_background: true,
        line_spacing: 14.0,
        show_title: true,
    })
}

/// Resolve either a `ListMenu` or `OptionMenu` by name.
#[must_use]
pub fn any_menu_from_def(defs: &MenuDef, name: &str, opts: MenuBuildOpts) -> Option<ListMenu> {
    list_menu_from_def(defs, name, opts).or_else(|| option_menu_from_def(defs, name, opts))
}

fn collect_items(
    cmds: &[MenuCmd],
    opts: MenuBuildOpts,
    items: &mut Vec<MenuItem>,
    origin: &mut Vec2,
    title: &mut String,
    option_mode: bool,
) {
    let mut i = 0;
    while i < cmds.len() {
        match &cmds[i] {
            MenuCmd::Block { kind, args, body } => {
                let k = kind.to_ascii_lowercase();
                if k == "else" {
                    // Orphan else — skip.
                    i += 1;
                    continue;
                }
                let active = match k.as_str() {
                    "ifgame" => args.iter().any(|a| game_matches(opts.game, a)),
                    "ifoption" => args.iter().any(|a| option_matches(opts, a)),
                    _ => true,
                };
                if active {
                    collect_items(body, opts, items, origin, title, option_mode);
                }
                // Consume following `else` when present.
                if i + 1 < cmds.len() {
                    if let MenuCmd::Block {
                        kind: else_k,
                        body: else_body,
                        ..
                    } = &cmds[i + 1]
                    {
                        if else_k.eq_ignore_ascii_case("else") {
                            if !active {
                                collect_items(else_body, opts, items, origin, title, option_mode);
                            }
                            i += 2;
                            continue;
                        }
                    }
                }
                i += 1;
            }
            MenuCmd::Cmd { name, args } => {
                let n = name.to_ascii_lowercase();
                match n.as_str() {
                    "position" => {
                        if args.len() >= 2 {
                            if let (Ok(x), Ok(y)) = (args[0].parse::<f32>(), args[1].parse::<f32>())
                            {
                                *origin = Vec2::new(x, y);
                            }
                        }
                    }
                    "title" => {
                        if let Some(t) = args.first() {
                            *title = strip_dollar(t);
                        }
                    }
                    "textitem" => {
                        // TextItem "$LABEL", "hotkey", "Action"
                        if args.len() >= 3 {
                            items
                                .push(MenuItem::text(strip_dollar(&args[0]), map_action(&args[2])));
                        } else if args.len() == 2 {
                            items
                                .push(MenuItem::text(strip_dollar(&args[0]), map_action(&args[1])));
                        }
                    }
                    "patchitem" => {
                        // PatchItem "M_NGAME", "n", "PlayerclassMenu"
                        if args.len() >= 3 {
                            let patch = strip_dollar(&args[0]);
                            items.push(MenuItem {
                                label: patch.clone(),
                                action: map_action(&args[2]),
                                patch: Some(patch),
                            });
                        }
                    }
                    "submenu" if option_mode => {
                        // Submenu "$LABEL", "TargetMenu"
                        if args.len() >= 2 {
                            items
                                .push(MenuItem::text(strip_dollar(&args[0]), map_action(&args[1])));
                        }
                    }
                    "command" | "safecommand" if option_mode => {
                        // Command / SafeCommand "$LABEL", "console cmd"
                        if args.len() >= 2 {
                            items.push(MenuItem::text(
                                strip_dollar(&args[0]),
                                format!("cmd {}", args[1]),
                            ));
                        } else if args.len() == 1 {
                            items.push(MenuItem::text(strip_dollar(&args[0]), "cmd"));
                        }
                    }
                    "option" if option_mode => {
                        // Option "$LABEL", "cvar", "ValueSet" — activate toggles the cvar.
                        if let Some(label) = args.first() {
                            let cvar = args.get(1).map(|s| s.as_str()).unwrap_or("");
                            items.push(MenuItem::text(
                                strip_dollar(label),
                                format!("option {cvar}"),
                            ));
                        }
                    }
                    "statictext" | "statictextcentered" if option_mode => {
                        // Non-blank StaticText becomes a non-activating spacer row.
                        let label = args.first().map(|s| strip_dollar(s)).unwrap_or_default();
                        if !label.trim().is_empty() {
                            items.push(MenuItem::text(label, "noop"));
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
        }
    }
}

fn game_matches(game: MenuGame, token: &str) -> bool {
    match token.to_ascii_lowercase().as_str() {
        "doom" => matches!(game, MenuGame::Doom),
        "chex" => matches!(game, MenuGame::Chex),
        "heretic" => matches!(game, MenuGame::Heretic),
        "hexen" => matches!(game, MenuGame::Hexen),
        "strife" => matches!(game, MenuGame::Strife),
        _ => false,
    }
}

fn option_matches(opts: MenuBuildOpts, token: &str) -> bool {
    match token.to_ascii_lowercase().as_str() {
        "readthis" => opts.read_this,
        "swapmenu" => opts.swap_menu,
        // Unrecognised options: treat as false (conservative).
        _ => false,
    }
}

fn strip_dollar(s: &str) -> String {
    s.strip_prefix('$').unwrap_or(s).to_owned()
}

fn map_action(target: &str) -> String {
    // MENUDEF submenu / class names → existing MenuStack actions where known.
    match target {
        "PlayerclassMenu" | "EpisodeMenu" | "SkillMenu" => "menu_newgame".into(),
        "OptionsMenu" | "Options" => "openmenu OptionsMenu".into(),
        "LoadGameMenu" => "menu_loadgame".into(),
        "SaveGameMenu" => "menu_savegame".into(),
        "QuitMenu" => "menu_quit".into(),
        "ReadThisMenu" => "menu_readthis".into(),
        other => format!("openmenu {other}"),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use ferrite_defs::parse_menudef;

    #[test]
    fn builds_main_menu_from_fixture() {
        let text = include_str!("../../../testdata/menudef/gzdoom_menudef.txt");
        let defs = parse_menudef(text).unwrap();
        let menu = list_menu_from_def(&defs, "MainMenu", MenuBuildOpts::default()).unwrap();
        assert!(
            menu.items.len() >= 4,
            "expected ≥4 items, got {:?}",
            menu.items
        );
        assert!(
            menu.items.iter().any(|i| i.action == "menu_newgame"),
            "expected menu_newgame, got {:?}",
            menu.items
        );
        assert!(
            menu.items.iter().any(|i| i.action == "menu_quit"),
            "expected menu_quit, got {:?}",
            menu.items
        );
    }

    #[test]
    fn builds_options_menu_from_fixture() {
        let text = include_str!("../../../testdata/menudef/gzdoom_menudef.txt");
        let defs = parse_menudef(text).unwrap();
        let menu = option_menu_from_def(&defs, "OptionsMenu", MenuBuildOpts::default()).unwrap();
        assert!(
            menu.items.len() >= 5,
            "expected ≥5 options rows, got {:?}",
            menu.items
        );
        assert!(
            menu.items
                .iter()
                .any(|i| i.action.contains("openmenu") || i.action.starts_with("cmd ")),
            "expected submenu/command actions, got {:?}",
            menu.items
        );
    }
}
