//! `COMPLVL` lump → engine complevel preset (GZDoom ≥ 4.11 / DSDA).

use crate::error::DefsError;

/// Compatibility level selected by a `COMPLVL` lump (or CLI / CVar later).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Complevel {
    /// No `COMPLVL` lump / unspecified (GZDoom/ZDoom defaults).
    #[default]
    Default,
    /// Doom 1.9 / limit-removing vanilla.
    Vanilla,
    /// Boom 2.02.
    Boom,
    /// MBF.
    Mbf,
    /// MBF21.
    Mbf21,
}

impl Complevel {
    /// GZDoom `compatmode` preset index used for this complevel.
    ///
    /// Maps to `ferrite_sim::CompatMode` (Default=0, Doom=1, Boom=3, MBF=5, MBF21=8).
    #[must_use]
    pub const fn compatmode_id(self) -> u8 {
        match self {
            Self::Default => 0,
            Self::Vanilla => 1,
            Self::Boom => 3,
            Self::Mbf => 5,
            Self::Mbf21 => 8,
        }
    }
}

/// Parse a `COMPLVL` lump body (`vanilla` / `boom` / `mbf` / `mbf21`).
///
/// First non-comment token wins (GZDoom takes the lump as a short keyword).
pub fn parse_complvl(text: &str) -> Result<Complevel, DefsError> {
    for raw in text.lines() {
        let line = raw.split(['#', ';', '/']).next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        // Take first whitespace-separated token.
        let token = line.split_whitespace().next().unwrap_or("");
        return match token.to_ascii_lowercase().as_str() {
            "vanilla" | "doom" | "limitremoving" | "limit-removing" => Ok(Complevel::Vanilla),
            "boom" => Ok(Complevel::Boom),
            "mbf" => Ok(Complevel::Mbf),
            "mbf21" => Ok(Complevel::Mbf21),
            other => Err(DefsError::Message(format!(
                "COMPLVL: unknown complevel `{other}` (expected vanilla/boom/mbf/mbf21)"
            ))),
        };
    }
    Err(DefsError::Message(
        "COMPLVL: empty lump (expected vanilla/boom/mbf/mbf21)".into(),
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_keywords() {
        assert_eq!(parse_complvl("mbf21\n").unwrap(), Complevel::Mbf21);
        assert_eq!(parse_complvl("  Boom  ").unwrap(), Complevel::Boom);
        assert_eq!(parse_complvl("# c\nvanilla").unwrap(), Complevel::Vanilla);
        assert_eq!(parse_complvl("mbf ; comment").unwrap(), Complevel::Mbf);
        assert_eq!(parse_complvl("doom").unwrap(), Complevel::Vanilla);
    }

    #[test]
    fn rejects_unknown() {
        assert!(parse_complvl("gzdoom").is_err());
        assert!(parse_complvl("").is_err());
        assert!(parse_complvl("# only comment\n").is_err());
    }
}
