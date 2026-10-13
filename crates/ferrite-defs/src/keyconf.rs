//! KEYCONF parser (restricted console-script weapon / bind / playerclass setup).

use crate::error::DefsError;

/// One KEYCONF command after whitelist filtering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyConfCmd {
    /// `alias name "body"`.
    Alias {
        /// Alias name.
        name: String,
        /// Expansion command line.
        body: String,
    },
    /// `defaultbind key command`.
    DefaultBind {
        /// Key name.
        key: String,
        /// Command to bind.
        cmd: String,
    },
    /// `addkeysection "title" ininame`.
    AddKeySection {
        /// Menu section title.
        title: String,
        /// INI subsection name.
        ini: String,
    },
    /// `addmenukey "description" command` (requires a prior `addkeysection`).
    AddMenuKey {
        /// Menu row label.
        description: String,
        /// Bound command.
        command: String,
    },
    /// `weaponsection name`.
    WeaponSection(String),
    /// `setslot n weapon…` (clears then fills when strict).
    SetSlot {
        /// Slot index.
        slot: i32,
        /// Weapon class names.
        weapons: Vec<String>,
    },
    /// `addslot n weapon`.
    AddSlot {
        /// Slot index.
        slot: i32,
        /// Weapon class.
        weapon: String,
    },
    /// `addslotdefault n weapon`.
    AddSlotDefault {
        /// Slot index.
        slot: i32,
        /// Weapon class.
        weapon: String,
    },
    /// `clearplayerclasses`.
    ClearPlayerClasses,
    /// `addplayerclass Class [nomenu]`.
    AddPlayerClass {
        /// PlayerPawn class name.
        class: String,
        /// Hide from class menu.
        nomenu: bool,
    },
}

/// Compiled KEYCONF (command list in lump order).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyConf {
    /// Whitelisted commands.
    pub commands: Vec<KeyConfCmd>,
    /// Skipped / unknown lines (warnings).
    pub warnings: Vec<String>,
}

/// Parse a KEYCONF lump (line-oriented; `//` comments; whitelist only).
pub fn parse_keyconf(text: &str) -> Result<KeyConf, DefsError> {
    let mut info = KeyConf::default();
    for (lineno, raw) in text.lines().enumerate() {
        let line = strip_comment(raw);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let tokens = tokenize(line);
        if tokens.is_empty() {
            continue;
        }
        let cmd = tokens[0].to_ascii_lowercase();
        match cmd.as_str() {
            "alias" => {
                if tokens.len() < 3 {
                    info.warnings
                        .push(format!("KEYCONF:{}: alias needs name and body", lineno + 1));
                    continue;
                }
                info.commands.push(KeyConfCmd::Alias {
                    name: tokens[1].clone(),
                    body: tokens[2..].join(" "),
                });
            }
            "defaultbind" => {
                if tokens.len() < 3 {
                    info.warnings.push(format!(
                        "KEYCONF:{}: defaultbind needs key and command",
                        lineno + 1
                    ));
                    continue;
                }
                info.commands.push(KeyConfCmd::DefaultBind {
                    key: tokens[1].clone(),
                    cmd: tokens[2..].join(" "),
                });
            }
            "addkeysection" => {
                if tokens.len() < 3 {
                    info.warnings.push(format!(
                        "KEYCONF:{}: addkeysection needs title and ini name",
                        lineno + 1
                    ));
                    continue;
                }
                info.commands.push(KeyConfCmd::AddKeySection {
                    title: tokens[1].clone(),
                    ini: tokens[2].clone(),
                });
            }
            "addmenukey" => {
                if tokens.len() < 3 {
                    info.warnings.push(format!(
                        "KEYCONF:{}: addmenukey needs description and command",
                        lineno + 1
                    ));
                    continue;
                }
                info.commands.push(KeyConfCmd::AddMenuKey {
                    description: tokens[1].clone(),
                    command: tokens[2..].join(" "),
                });
            }
            "weaponsection" => {
                if tokens.len() < 2 {
                    info.warnings.push(format!(
                        "KEYCONF:{}: weaponsection needs a name",
                        lineno + 1
                    ));
                    continue;
                }
                info.commands
                    .push(KeyConfCmd::WeaponSection(tokens[1].clone()));
            }
            "setslot" => {
                if tokens.len() < 2 {
                    info.warnings
                        .push(format!("KEYCONF:{}: setslot needs a slot", lineno + 1));
                    continue;
                }
                let Ok(slot) = tokens[1].parse::<i32>() else {
                    info.warnings.push(format!(
                        "KEYCONF:{}: setslot: bad slot {:?}",
                        lineno + 1,
                        tokens[1]
                    ));
                    continue;
                };
                info.commands.push(KeyConfCmd::SetSlot {
                    slot,
                    weapons: tokens[2..].to_vec(),
                });
            }
            "addslot" => {
                if tokens.len() < 3 {
                    info.warnings.push(format!(
                        "KEYCONF:{}: addslot needs slot and weapon",
                        lineno + 1
                    ));
                    continue;
                }
                let Ok(slot) = tokens[1].parse::<i32>() else {
                    info.warnings.push(format!(
                        "KEYCONF:{}: addslot: bad slot {:?}",
                        lineno + 1,
                        tokens[1]
                    ));
                    continue;
                };
                info.commands.push(KeyConfCmd::AddSlot {
                    slot,
                    weapon: tokens[2].clone(),
                });
            }
            "addslotdefault" => {
                if tokens.len() < 3 {
                    info.warnings.push(format!(
                        "KEYCONF:{}: addslotdefault needs slot and weapon",
                        lineno + 1
                    ));
                    continue;
                }
                let Ok(slot) = tokens[1].parse::<i32>() else {
                    info.warnings.push(format!(
                        "KEYCONF:{}: addslotdefault: bad slot {:?}",
                        lineno + 1,
                        tokens[1]
                    ));
                    continue;
                };
                info.commands.push(KeyConfCmd::AddSlotDefault {
                    slot,
                    weapon: tokens[2].clone(),
                });
            }
            "clearplayerclasses" => {
                info.commands.push(KeyConfCmd::ClearPlayerClasses);
            }
            "addplayerclass" => {
                if tokens.len() < 2 {
                    info.warnings.push(format!(
                        "KEYCONF:{}: addplayerclass needs a class name",
                        lineno + 1
                    ));
                    continue;
                }
                let nomenu = tokens[2..].iter().any(|t| t.eq_ignore_ascii_case("nomenu"));
                info.commands.push(KeyConfCmd::AddPlayerClass {
                    class: tokens[1].clone(),
                    nomenu,
                });
            }
            other => {
                info.warnings.push(format!(
                    "KEYCONF:{}: ignored non-whitelist command `{other}`",
                    lineno + 1
                ));
            }
        }
    }
    Ok(info)
}

/// Strip `//` comments outside of double quotes (GZDoom `D_LoadWadSettings`).
fn strip_comment(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut in_quote = false;
    let mut prev = 0u8;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' {
            in_quote = !in_quote;
            out.push('"');
            prev = c;
            i += 1;
            continue;
        }
        if !in_quote && prev == b'/' && c == b'/' {
            // Drop the first slash already pushed.
            out.pop();
            break;
        }
        out.push(c as char);
        prev = c;
        i += 1;
    }
    out
}

fn tokenize(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let mut cur = String::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            if !cur.is_empty() {
                tokens.push(std::mem::take(&mut cur));
            }
            i += 1;
            continue;
        }
        if c == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                cur.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                i += 1; // closing quote
            }
            tokens.push(std::mem::take(&mut cur));
            continue;
        }
        cur.push(c);
        i += 1;
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_whitelist_commands() {
        let text = r#"
// comment
weaponsection "MyMod"
setslot 1 Pistol Fist // trailing
addslotdefault 2 Shotgun
alias +mymodfire "pistol"
defaultbind q "+mymodfire"
addkeysection "My Mod Keys" MyMod
addmenukey "Fire special" "+mymodfire"
clearplayerclasses
addplayerclass DoomPlayer
addplayerclass MyHero nomenu
echo should_warn
"#;
        let info = parse_keyconf(text).unwrap();
        assert!(info.warnings.iter().any(|w| w.contains("echo")));
        assert!(matches!(
            &info.commands[0],
            KeyConfCmd::WeaponSection(s) if s == "MyMod"
        ));
        assert!(matches!(
            &info.commands[1],
            KeyConfCmd::SetSlot { slot: 1, weapons } if weapons.as_slice() == ["Pistol", "Fist"]
        ));
        assert!(matches!(
            &info.commands[2],
            KeyConfCmd::AddSlotDefault { slot: 2, weapon } if weapon == "Shotgun"
        ));
        assert!(matches!(
            &info.commands[3],
            KeyConfCmd::Alias { name, body } if name == "+mymodfire" && body == "pistol"
        ));
        assert!(matches!(&info.commands[7], KeyConfCmd::ClearPlayerClasses));
        assert!(matches!(
            &info.commands[9],
            KeyConfCmd::AddPlayerClass { class, nomenu: true } if class == "MyHero"
        ));
    }

    #[test]
    fn parses_fixture() {
        let text = include_str!("../../../testdata/keyconf/sample.keyconf");
        let info = parse_keyconf(text).unwrap();
        assert!(
            info.commands
                .iter()
                .any(|c| matches!(c, KeyConfCmd::WeaponSection(_)))
        );
        assert!(
            info.commands
                .iter()
                .any(|c| matches!(c, KeyConfCmd::AddPlayerClass { .. }))
        );
        assert!(info.warnings.is_empty());
    }
}
