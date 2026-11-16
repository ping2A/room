//! Chocolate `menues.c` front-end pages drawn natively with Duke art:
//! main menu (0), in-game menu (50), episode select (100), skill select
//! (110), credits (990–992, Atomic picture pages) and quit confirmation
//! (501/502). Pages the host already implements (save/load slots, sound
//! setup, help) come back as [`MenuAction`]s.

use crate::ui_page::UiPage;

const MENUBAR: i16 = 2457;
const INGAMEDUKETHREEDEE: i16 = 2499;
const PLUTOPAKSPRITE: i16 = 2501;
const SPINNINGNUKEICON: i16 = 2813;
const BIGFNTCURSOR: i16 = 2820;
const CREDITS_FIRST: i16 = 2504;

/// Menu navigation input (one press).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuKey {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Escape,
    /// `Y` on a yes/no page.
    Yes,
    /// `N` on a yes/no page.
    No,
}

/// What the host should do after a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    None,
    /// Close the menu and resume play.
    Resume,
    /// Chocolate `newgame(volume, 0, skill)`; `skill` is `ud.player_skill` (1–4).
    NewGame {
        volume: u8,
        skill: u8,
    },
    OpenSave,
    OpenLoad,
    OpenOptions,
    OpenHelp,
    /// Menu 503 confirmed: leave the game for the title menu.
    QuitToTitle,
    /// Menu 501/502 confirmed: exit the program.
    QuitGame,
}

/// Current Chocolate `current_menu` page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuPage {
    Main,
    InGame,
    Episode,
    Skill,
    Credits(u8),
    QuitConfirm,
    QuitToTitleConfirm,
}

/// Native Duke menu state.
#[derive(Clone, Debug)]
pub struct DukeMenu {
    pub page: MenuPage,
    probey: usize,
    in_game: bool,
    volume: u8,
    /// Episode names (`definevolumename`, uppercase).
    pub volume_names: Vec<String>,
    /// Skill names (`defineskillname`).
    pub skill_names: Vec<String>,
    sounds: Vec<&'static str>,
}

impl DukeMenu {
    /// Open Chocolate's menu 0 (title) or 50 (in game).
    #[must_use]
    pub fn new(in_game: bool, volume_names: Vec<String>, skill_names: Vec<String>) -> Self {
        let pick = |names: Vec<String>, fallback: &[&str]| -> Vec<String> {
            let names: Vec<String> = names.into_iter().filter(|n| !n.is_empty()).collect();
            if names.is_empty() {
                fallback.iter().map(|s| (*s).to_string()).collect()
            } else {
                names
            }
        };
        Self {
            page: if in_game {
                MenuPage::InGame
            } else {
                MenuPage::Main
            },
            probey: 0,
            in_game,
            volume: 0,
            volume_names: pick(
                volume_names,
                &[
                    "L.A. MELTDOWN",
                    "LUNAR APOCALYPSE",
                    "SHRAPNEL CITY",
                    "THE BIRTH",
                ],
            ),
            skill_names: pick(
                skill_names,
                &[
                    "PIECE OF CAKE",
                    "LET'S ROCK",
                    "COME GET SOME",
                    "DAMN I'M GOOD",
                ],
            ),
            sounds: Vec::new(),
        }
    }

    /// Drain queued CON sound labels (`KICK_HIT`, `PISTOL_BODYHIT`…).
    pub fn take_sounds(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.sounds)
    }

    /// Highlighted row.
    #[must_use]
    pub fn selection(&self) -> usize {
        self.probey
    }

    fn rows(&self) -> usize {
        match self.page {
            MenuPage::Main => 6,
            MenuPage::InGame => 7,
            MenuPage::Episode => self.volume_names.len().clamp(1, 4),
            MenuPage::Skill => self.skill_names.len().clamp(1, 4),
            _ => 0,
        }
    }

    fn goto(&mut self, page: MenuPage) {
        self.page = page;
        self.probey = 0;
    }

    fn back_to_root(&mut self) {
        self.goto(if self.in_game {
            MenuPage::InGame
        } else {
            MenuPage::Main
        });
    }

    /// Chocolate `probe` + the page's switch for one key.
    pub fn key(&mut self, key: MenuKey) -> MenuAction {
        match self.page {
            MenuPage::Credits(n) => {
                // Atomic: three picture pages, wrap both ways.
                match key {
                    MenuKey::Escape => {
                        self.sounds.push("EXITMENUSOUND");
                        self.goto(MenuPage::Main);
                    }
                    MenuKey::Left | MenuKey::Up => {
                        self.sounds.push("KICK_HIT");
                        self.page = MenuPage::Credits(if n == 0 { 2 } else { n - 1 });
                    }
                    MenuKey::Right | MenuKey::Down | MenuKey::Enter => {
                        self.sounds.push("KICK_HIT");
                        self.page = MenuPage::Credits(if n >= 2 { 0 } else { n + 1 });
                    }
                    _ => {}
                }
                return MenuAction::None;
            }
            MenuPage::QuitConfirm | MenuPage::QuitToTitleConfirm => {
                let quit_game = self.page == MenuPage::QuitConfirm;
                return match key {
                    MenuKey::Yes | MenuKey::Enter => {
                        if quit_game {
                            MenuAction::QuitGame
                        } else {
                            // Chocolate 503: `gm = MODE_DEMO; cmenu(0)`.
                            self.in_game = false;
                            self.goto(MenuPage::Main);
                            MenuAction::QuitToTitle
                        }
                    }
                    MenuKey::No | MenuKey::Escape => {
                        self.sounds.push("EXITMENUSOUND");
                        self.back_to_root();
                        MenuAction::None
                    }
                    _ => MenuAction::None,
                };
            }
            _ => {}
        }
        let n = self.rows().max(1);
        match key {
            MenuKey::Up => {
                self.sounds.push("KICK_HIT");
                self.probey = if self.probey == 0 {
                    n - 1
                } else {
                    self.probey - 1
                };
                MenuAction::None
            }
            MenuKey::Down => {
                self.sounds.push("KICK_HIT");
                self.probey = (self.probey + 1) % n;
                MenuAction::None
            }
            MenuKey::Escape => {
                self.sounds.push("EXITMENUSOUND");
                match self.page {
                    MenuPage::Episode => {
                        self.back_to_root();
                        MenuAction::None
                    }
                    MenuPage::Skill => {
                        self.goto(MenuPage::Episode);
                        self.probey = usize::from(self.volume);
                        MenuAction::None
                    }
                    MenuPage::InGame => MenuAction::Resume,
                    _ => MenuAction::None,
                }
            }
            MenuKey::Enter => {
                if self.page != MenuPage::Skill {
                    self.sounds.push("PISTOL_BODYHIT");
                }
                self.select()
            }
            _ => MenuAction::None,
        }
    }

    fn select(&mut self) -> MenuAction {
        let x = self.probey;
        match self.page {
            MenuPage::Main => match x {
                0 => {
                    self.goto(MenuPage::Episode);
                    MenuAction::None
                }
                1 => MenuAction::OpenOptions,
                2 => MenuAction::OpenLoad,
                3 => MenuAction::OpenHelp,
                4 => {
                    self.goto(MenuPage::Credits(0));
                    MenuAction::None
                }
                _ => {
                    self.goto(MenuPage::QuitConfirm);
                    MenuAction::None
                }
            },
            MenuPage::InGame => match x {
                0 => {
                    self.goto(MenuPage::Episode);
                    MenuAction::None
                }
                1 => MenuAction::OpenSave,
                2 => MenuAction::OpenLoad,
                3 => MenuAction::OpenOptions,
                4 => MenuAction::OpenHelp,
                5 => {
                    self.goto(MenuPage::QuitToTitleConfirm);
                    MenuAction::None
                }
                _ => {
                    self.goto(MenuPage::QuitConfirm);
                    MenuAction::None
                }
            },
            MenuPage::Episode => {
                self.volume = x as u8;
                self.goto(MenuPage::Skill);
                self.probey = 1;
                MenuAction::None
            }
            MenuPage::Skill => {
                self.sounds.push(match x {
                    0 => "JIBBED_ACTOR6",
                    1 => "BONUS_SPEECH1",
                    2 => "DUKE_GETWEAPON2",
                    _ => "JIBBED_ACTOR5",
                });
                MenuAction::NewGame {
                    volume: self.volume,
                    skill: x as u8 + 1,
                }
            }
            _ => MenuAction::None,
        }
    }

    /// Chocolate `probe` cursor: two spinning nukes either side of a
    /// centred list (x == 160), else BIGFNTCURSOR-offset left nuke.
    fn cursor(page: &mut UiPage<'_>, x: i32, y: i32, step: i32, row: usize, clock: i64, shade: i8) {
        let ry = y + row as i32 * step - 4;
        let spin = ((clock >> 3) % 7) as i16;
        if x == 160 {
            let centre = 320 >> 2;
            page.sprite(
                160 + (centre >> 1) + 70,
                ry,
                SPINNINGNUKEICON + 6 - ((6 + (clock >> 3)) % 7) as i16,
                shade,
                0,
                false,
                false,
            );
            page.sprite(
                160 - (centre >> 1) - 70,
                ry,
                SPINNINGNUKEICON + spin,
                shade,
                0,
                false,
                false,
            );
        } else {
            let w = page.tile_width(BIGFNTCURSOR);
            page.sprite(
                x - w - 4,
                ry,
                SPINNINGNUKEICON + spin,
                shade,
                0,
                false,
                false,
            );
        }
    }

    fn menubar(page: &mut UiPage<'_>, title: &str) {
        page.sprite(160, 19, MENUBAR, 16, 0, false, false);
        page.menutext(160, 24, 0, 0, title);
    }

    /// Draw the page; `clock` is the 120 Hz `totalclock`.
    pub fn draw(&self, page: &mut UiPage<'_>, clock: i64) {
        let sintable = |a: i64| room_build_engine::helpers::sincos_q14((a & 2047) as i32).0;
        let sh = (4 - (sintable(clock << 4) >> 11)) as i8;
        let plu_shade = (sintable(clock << 4) >> 11) as i8;
        match self.page {
            MenuPage::Main | MenuPage::InGame => {
                let in_game = self.page == MenuPage::InGame;
                page.sprite(
                    160,
                    if in_game { 32 } else { 28 },
                    INGAMEDUKETHREEDEE,
                    0,
                    0,
                    false,
                    false,
                );
                page.sprite(260, 36, PLUTOPAKSPRITE + 2, plu_shade, 0, false, false);
                let items: &[&str] = if in_game {
                    &[
                        "NEW GAME",
                        "SAVE GAME",
                        "LOAD GAME",
                        "OPTIONS",
                        " HELP",
                        "QUIT TO TITLE",
                        "QUIT GAME",
                    ]
                } else {
                    &[
                        "NEW GAME",
                        "OPTIONS",
                        "LOAD GAME",
                        "HELP",
                        "CREDITS",
                        "QUIT",
                    ]
                };
                Self::cursor(page, 160, 67, 16, self.probey, clock, sh);
                for (k, item) in items.iter().enumerate() {
                    page.menutext(160, 67 + 16 * k as i32, 0, 0, item);
                }
            }
            MenuPage::Episode => {
                Self::menubar(page, "SELECT AN EPISODE");
                Self::cursor(page, 160, 60, 20, self.probey, clock, sh);
                for (k, name) in self.volume_names.iter().take(4).enumerate() {
                    page.menutext(160, 60 + 20 * k as i32, 0, 0, name);
                }
            }
            MenuPage::Skill => {
                Self::menubar(page, "SELECT SKILL");
                Self::cursor(page, 160, 70, 19, self.probey, clock, sh);
                for (k, name) in self.skill_names.iter().take(4).enumerate() {
                    page.menutext(160, 70 + 19 * k as i32, 0, 0, name);
                }
            }
            MenuPage::Credits(n) => {
                // Atomic (PLUTOPAK) credits are three full picture pages.
                page.sprite(160, 100, CREDITS_FIRST + i16::from(n), 0, 0, false, true);
            }
            MenuPage::QuitConfirm => {
                page.gametext(160, 90, "Are you sure you want to quit?", 0);
                page.gametext(160, 99, "(Y/N)", 0);
            }
            MenuPage::QuitToTitleConfirm => {
                page.gametext(160, 90, "Quit to Title?", 0);
                page.gametext(160, 99, "(Y/N)", 0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu(in_game: bool) -> DukeMenu {
        DukeMenu::new(in_game, Vec::new(), Vec::new())
    }

    #[test]
    fn new_game_goes_episode_then_skill() {
        let mut m = menu(false);
        assert_eq!(m.key(MenuKey::Enter), MenuAction::None);
        assert_eq!(m.page, MenuPage::Episode);
        m.key(MenuKey::Down);
        m.key(MenuKey::Enter);
        assert_eq!(m.page, MenuPage::Skill);
        m.key(MenuKey::Down);
        assert_eq!(
            m.key(MenuKey::Enter),
            MenuAction::NewGame {
                volume: 1,
                skill: 3
            }
        );
        assert!(m.take_sounds().contains(&"DUKE_GETWEAPON2"));
    }

    #[test]
    fn up_wraps_and_escape_backs_out() {
        let mut m = menu(true);
        m.key(MenuKey::Up);
        assert_eq!(m.selection(), 6);
        assert_eq!(m.key(MenuKey::Enter), MenuAction::None);
        assert_eq!(m.page, MenuPage::QuitConfirm);
        m.key(MenuKey::No);
        assert_eq!(m.page, MenuPage::InGame);
        assert_eq!(m.key(MenuKey::Escape), MenuAction::Resume);
    }

    #[test]
    fn credits_cycle_three_pages() {
        let mut m = menu(false);
        for _ in 0..4 {
            m.key(MenuKey::Down);
        }
        m.key(MenuKey::Enter);
        assert_eq!(m.page, MenuPage::Credits(0));
        m.key(MenuKey::Left);
        assert_eq!(m.page, MenuPage::Credits(2));
        m.key(MenuKey::Right);
        assert_eq!(m.page, MenuPage::Credits(0));
    }
}
