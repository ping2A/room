//! SNDSEQ parser (non-C mode, Hexen/ZDoom).

use crate::error::DefsError;
use ferrite_sc::{ScanOptions, Scanner, Tok};
use std::collections::BTreeMap;

/// One command inside a sound sequence.
#[derive(Clone, Debug, PartialEq)]
pub enum SeqCommand {
    /// `play <sound>`
    Play(String),
    /// `playuntildone <sound>`
    PlayUntilDone(String),
    /// `playtime <sound> <tics>`
    PlayTime {
        /// Logical sound name.
        sound: String,
        /// Delay after start, in tics.
        tics: i64,
    },
    /// `playrepeat <sound>`
    PlayRepeat(String),
    /// `playloop <sound> <tics>`
    PlayLoop {
        /// Logical sound name.
        sound: String,
        /// Loop period in tics.
        tics: i64,
    },
    /// `delay <tics>`
    Delay(i64),
    /// `delayrand <min> <max>`
    DelayRand {
        /// Inclusive minimum tics.
        min: i64,
        /// Inclusive maximum tics.
        max: i64,
    },
    /// `volume <v>` (0–100 or float).
    Volume(f64),
    /// `volumerel <v>`
    VolumeRel(f64),
    /// `volumerand <min> <max>`
    VolumeRand {
        /// Minimum volume.
        min: f64,
        /// Maximum volume.
        max: f64,
    },
    /// `attenuation <v>`
    Attenuation(f64),
    /// `stopsound <sound>`
    StopSound(String),
    /// `nostopcutoff`
    NoStopCutoff,
    /// `door <n>` — assign this sequence as door slot `n`.
    Door(i64),
    /// `platform <n>`
    Platform(i64),
    /// `environment <n>`
    Environment(i64),
    /// `slot <name>`
    Slot(String),
}

/// A named sound sequence (`:Name` … `end`).
#[derive(Clone, Debug, PartialEq)]
pub struct SoundSequence {
    /// Sequence name (as written).
    pub name: String,
    /// Ordered commands.
    pub commands: Vec<SeqCommand>,
}

/// Parsed SNDSEQ registry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SoundSeqRegistry {
    /// Sequences by uppercase name.
    pub sequences: BTreeMap<String, SoundSequence>,
    /// Door slot index → sequence name.
    pub door_slots: BTreeMap<i64, String>,
    /// Platform slot index → sequence name.
    pub platform_slots: BTreeMap<i64, String>,
    /// Environment slot index → sequence name.
    pub environment_slots: BTreeMap<i64, String>,
    /// Named slots (`slot` / `[Name …]`).
    pub named_slots: BTreeMap<String, String>,
    /// Non-fatal parse notes.
    pub warnings: Vec<String>,
}

/// Parse an SNDSEQ lump.
pub fn parse_sndseq(text: &str) -> Result<SoundSeqRegistry, DefsError> {
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            c_mode: false,
            semicolon_comments: true,
        },
    );
    let mut reg = SoundSeqRegistry::default();

    loop {
        match sc.next_token()? {
            Tok::Eof => break,
            // Non-C mode keeps `:` attached: `:Platform`.
            Tok::Ident(tok) if tok.starts_with(':') => {
                let name = tok[1..].to_owned();
                if name.is_empty() {
                    return Err(DefsError::Message("SNDSEQ: empty sequence name".into()));
                }
                let seq = parse_sequence_body(&mut sc, name.clone())?;
                apply_slot_cmds(&mut reg, &seq);
                reg.sequences.insert(name.to_ascii_uppercase(), seq);
            }
            // `[SlotName` then `SequenceName]` or `[SlotName]`.
            Tok::Ident(tok) if tok.starts_with('[') => {
                parse_bracket_slot(&mut sc, &mut reg, tok)?;
            }
            Tok::Ident(tok) if Scanner::compare(tok, "end") => {
                reg.warnings.push("SNDSEQ: stray `end`".into());
            }
            Tok::Ident(tok) => {
                reg.warnings
                    .push(format!("SNDSEQ: unexpected top-level `{tok}`"));
            }
            other => {
                return Err(DefsError::Message(format!("SNDSEQ: unexpected {other:?}")));
            }
        }
    }
    Ok(reg)
}

fn parse_bracket_slot(
    sc: &mut Scanner<'_>,
    reg: &mut SoundSeqRegistry,
    first: &str,
) -> Result<(), DefsError> {
    // Forms in non-C mode:
    //   [Slot]              → first = "[Slot]"
    //   [Slot Target]       → first = "[Slot", second = "Target]"
    let rest = &first[1..];
    if let Some(slot) = rest.strip_suffix(']') {
        reg.named_slots
            .insert(slot.to_ascii_uppercase(), slot.to_ascii_uppercase());
        return Ok(());
    }
    let slot = rest.to_owned();
    let second = read_word(sc)?;
    let target = second
        .strip_suffix(']')
        .ok_or_else(|| DefsError::Message("SNDSEQ: expected `]` after slot target".into()))?
        .to_owned();
    reg.named_slots
        .insert(slot.to_ascii_uppercase(), target.to_ascii_uppercase());
    Ok(())
}

fn parse_sequence_body(sc: &mut Scanner<'_>, name: String) -> Result<SoundSequence, DefsError> {
    let mut commands = Vec::new();
    loop {
        match sc.next_token()? {
            Tok::Eof => {
                return Err(DefsError::Message(format!(
                    "SNDSEQ `{name}`: missing `end`"
                )));
            }
            Tok::Ident(tok) if Scanner::compare(tok, "end") => break,
            Tok::Ident(tok) => {
                let cmd = tok.to_ascii_lowercase();
                match cmd.as_str() {
                    "play" => commands.push(SeqCommand::Play(read_word(sc)?)),
                    "playuntildone" => commands.push(SeqCommand::PlayUntilDone(read_word(sc)?)),
                    "playtime" => {
                        let sound = read_word(sc)?;
                        let tics = sc.must_get_number()?;
                        commands.push(SeqCommand::PlayTime { sound, tics });
                    }
                    "playrepeat" => commands.push(SeqCommand::PlayRepeat(read_word(sc)?)),
                    "playloop" => {
                        let sound = read_word(sc)?;
                        let tics = sc.must_get_number()?;
                        commands.push(SeqCommand::PlayLoop { sound, tics });
                    }
                    "delay" => commands.push(SeqCommand::Delay(sc.must_get_number()?)),
                    "delayrand" => {
                        let min = sc.must_get_number()?;
                        let max = sc.must_get_number()?;
                        commands.push(SeqCommand::DelayRand { min, max });
                    }
                    "volume" => commands.push(SeqCommand::Volume(read_float(sc)?)),
                    "volumerel" => commands.push(SeqCommand::VolumeRel(read_float(sc)?)),
                    "volumerand" => {
                        let min = read_float(sc)?;
                        let max = read_float(sc)?;
                        commands.push(SeqCommand::VolumeRand { min, max });
                    }
                    "attenuation" => commands.push(SeqCommand::Attenuation(read_float(sc)?)),
                    "stopsound" => commands.push(SeqCommand::StopSound(read_word(sc)?)),
                    "nostopcutoff" => commands.push(SeqCommand::NoStopCutoff),
                    "door" => commands.push(SeqCommand::Door(sc.must_get_number()?)),
                    "platform" => commands.push(SeqCommand::Platform(sc.must_get_number()?)),
                    "environment" => commands.push(SeqCommand::Environment(sc.must_get_number()?)),
                    "slot" => commands.push(SeqCommand::Slot(read_word(sc)?)),
                    other => {
                        return Err(DefsError::Message(format!(
                            "SNDSEQ `{name}`: unknown command `{other}`"
                        )));
                    }
                }
            }
            other => {
                return Err(DefsError::Message(format!(
                    "SNDSEQ `{name}`: unexpected {other:?}"
                )));
            }
        }
    }
    Ok(SoundSequence { name, commands })
}

fn apply_slot_cmds(reg: &mut SoundSeqRegistry, seq: &SoundSequence) {
    let key = seq.name.to_ascii_uppercase();
    for cmd in &seq.commands {
        match *cmd {
            SeqCommand::Door(n) => {
                reg.door_slots.insert(n, key.clone());
            }
            SeqCommand::Platform(n) => {
                reg.platform_slots.insert(n, key.clone());
            }
            SeqCommand::Environment(n) => {
                reg.environment_slots.insert(n, key.clone());
            }
            SeqCommand::Slot(ref s) => {
                reg.named_slots.insert(s.to_ascii_uppercase(), key.clone());
            }
            _ => {}
        }
    }
}

fn read_word(sc: &mut Scanner<'_>) -> Result<String, DefsError> {
    match sc.next_token()? {
        Tok::Ident(s) => Ok(s.to_owned()),
        Tok::String(s) => Ok(s.into_owned()),
        other => Err(DefsError::Message(format!(
            "SNDSEQ: expected word, got {other:?}"
        ))),
    }
}

fn read_float(sc: &mut Scanner<'_>) -> Result<f64, DefsError> {
    match sc.next_token()? {
        Tok::Float(f) => Ok(f),
        Tok::Int(n) => Ok(n as f64),
        Tok::Ident(s) => s
            .parse::<f64>()
            .map_err(|_| DefsError::Message(format!("SNDSEQ: bad float `{s}`"))),
        other => Err(DefsError::Message(format!(
            "SNDSEQ: expected number, got {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn platform_sequence() {
        let text = r#"
:Platform
	playuntildone	PlatformStart
	playrepeat	StoneMove
	stopsound	PlatformStop
	platform 0
end
[Ambient Platform]
"#;
        let reg = parse_sndseq(text).unwrap();
        let seq = reg.sequences.get("PLATFORM").unwrap();
        assert_eq!(seq.commands.len(), 4);
        assert!(matches!(
            &seq.commands[0],
            SeqCommand::PlayUntilDone(s) if s == "PlatformStart"
        ));
        assert_eq!(
            reg.platform_slots.get(&0).map(String::as_str),
            Some("PLATFORM")
        );
        assert_eq!(
            reg.named_slots.get("AMBIENT").map(String::as_str),
            Some("PLATFORM")
        );
    }

    #[test]
    fn ferrite_sndseq_door_slot() {
        let text = include_str!("../../../wadsrc/ferrite/sndseq.txt");
        let reg = parse_sndseq(text).unwrap();
        assert!(reg.sequences.contains_key("DOOROPENNORMAL"));
        assert!(reg.sequences.contains_key("DOORCLOSENORMAL"));
        assert_eq!(
            reg.door_slots.get(&0).map(String::as_str),
            Some("DOOROPENNORMAL")
        );
        let open = &reg.sequences["DOOROPENNORMAL"];
        assert!(matches!(
            &open.commands[0],
            SeqCommand::Play(s) if s == "doors/open"
        ));
    }
}
