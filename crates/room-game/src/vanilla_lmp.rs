//! Classic Doom / Boom `.lmp` demo parse (DEMO1–4 style).
//!
//! Header layout matches Doom 1.9 (`version == 109`). Playback sync is a separate
//! milestone track; this module only decodes bytes into [`crate::DemoRecorder`].

use crate::demo::{DemoError, DemoRecorder, DemoTic};
use room_defs::Complevel;
use room_sim::{Buttons, TicCmd};

/// Parsed vanilla / Boom demo lump.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VanillaLmp {
    /// Engine version byte (`109` = Doom 1.9).
    pub version: u8,
    /// Skill 1..=5 (lump stores 0..=4).
    pub skill: u8,
    /// Episode (Doom 1); unused / 1 for Doom II.
    pub episode: u8,
    /// Map number (1-based).
    pub map: u8,
    /// Deathmatch mode flag.
    pub deathmatch: u8,
    /// `-respawn`.
    pub respawn: bool,
    /// `-fast`.
    pub fast: bool,
    /// `-nomonsters`.
    pub nomonsters: bool,
    /// Console player index.
    pub consoleplayer: u8,
    /// `playeringame[4]`.
    pub playeringame: [bool; 4],
    /// Per-tic player commands (only slots with `playeringame`).
    pub tics: Vec<Vec<TicCmd>>,
}

impl VanillaLmp {
    /// Parse a Doom 1.9-style demo lump.
    pub fn from_lmp(bytes: &[u8]) -> Result<Self, DemoError> {
        if bytes.len() < 14 {
            return Err(DemoError::Message("vanilla .lmp too short".into()));
        }
        let version = bytes[0];
        if version != 109 && version != 111 && version != 209 {
            // 109 Doom 1.9, 111 Boom, 209 MBF — accept common complevels.
            return Err(DemoError::Message(format!(
                "unsupported demo version {version}"
            )));
        }
        let skill = bytes[1].saturating_add(1).clamp(1, 5);
        let episode = bytes[2];
        let map = bytes[3];
        let deathmatch = bytes[4];
        let respawn = bytes[5] != 0;
        let fast = bytes[6] != 0;
        let nomonsters = bytes[7] != 0;
        let consoleplayer = bytes[8];
        let mut playeringame = [false; 4];
        for i in 0..4 {
            playeringame[i] = bytes[9 + i] != 0;
        }
        let active: Vec<usize> = playeringame
            .iter()
            .enumerate()
            .filter_map(|(i, &on)| on.then_some(i))
            .collect();
        if active.is_empty() {
            return Err(DemoError::Message("no players in demo".into()));
        }
        let mut o = 13usize;
        let mut tics = Vec::new();
        while o < bytes.len() {
            if bytes[o] == 0x80 {
                break;
            }
            let need = active.len() * 4;
            if o + need > bytes.len() {
                return Err(DemoError::Message("vanilla .lmp truncated body".into()));
            }
            let mut slot_cmds = vec![TicCmd::default(); 4];
            for &pi in &active {
                let fwd = bytes[o] as i8;
                let side = bytes[o + 1] as i8;
                let angle = bytes[o + 2] as i8;
                let raw = bytes[o + 3];
                o += 4;
                // Doom ticcmd buttons: bit0 ATTACK, bit1 USE, bit2 CHANGE,
                // bits3–5 weapon (`wp_*` 0..=6). Strip CHANGE/weapon so they
                // do not collide with Room `WEAPONNEXT` (bit5).
                const BT_CHANGE: u8 = 1 << 2;
                const BT_WEAPONMASK: u8 = (1 << 3) | (1 << 4) | (1 << 5);
                const BT_WEAPONSHIFT: u8 = 3;
                let mut buttons = raw;
                let mut weapon_select = 0u8;
                if raw & BT_CHANGE != 0 {
                    // `wp_fist=0` … `wp_bfg=6` → Room slots 1..=7.
                    weapon_select = ((raw & BT_WEAPONMASK) >> BT_WEAPONSHIFT) + 1;
                    buttons &= !(BT_CHANGE | BT_WEAPONMASK);
                }
                slot_cmds[pi] = TicCmd {
                    forward_move: i16::from(fwd).saturating_mul(256),
                    side_move: i16::from(side).saturating_mul(256),
                    yaw: i16::from(angle).saturating_mul(256),
                    buttons: Buttons(u32::from(buttons)),
                    weapon_select,
                    ..TicCmd::default()
                };
            }
            // Compact to active prefix for DemoRecorder (player 0..=n-1).
            let cmds: Vec<TicCmd> = active.iter().map(|&i| slot_cmds[i]).collect();
            tics.push(cmds);
        }
        Ok(Self {
            version,
            skill,
            episode,
            map,
            deathmatch,
            respawn,
            fast,
            nomonsters,
            consoleplayer,
            playeringame,
            tics,
        })
    }

    /// Map lump name. `doom2` selects `MAPxx` vs `ExMy` (both store episode=1 for map 1–9).
    #[must_use]
    pub fn map_name(&self, doom2: bool) -> String {
        if doom2 {
            format!("MAP{:02}", self.map.max(1))
        } else {
            format!("E{}M{}", self.episode.max(1), self.map.max(1))
        }
    }

    /// Complevel guess from version byte.
    #[must_use]
    pub fn complevel(&self) -> Complevel {
        match self.version {
            109 => Complevel::Vanilla,
            111 => Complevel::Boom,
            209 => Complevel::Mbf,
            _ => Complevel::Vanilla,
        }
    }

    /// Convert to a Room [`DemoRecorder`] (no checksums — verify later).
    #[must_use]
    pub fn to_recorder(&self, doom2: bool) -> DemoRecorder {
        let mut rec = DemoRecorder::start_with_complevel(self.map_name(doom2), 0, self.complevel());
        rec.skill = self.skill;
        rec.player_count = self.playeringame.iter().filter(|&&p| p).count() as u8;
        rec.consoleplayer = self.consoleplayer;
        for cmds in &self.tics {
            rec.tics.push(DemoTic {
                cmds: cmds.clone(),
                checksum: None,
            });
        }
        rec
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_minimal_doom19_lmp() {
        // version, skill0, ep1, map1, flags…, p1 only, one tic, end.
        let bytes = [
            109, 2, 1, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, // header
            25, 0, 0, 0, // walk forward
            0x80,
        ];
        let d = VanillaLmp::from_lmp(&bytes).unwrap();
        assert_eq!(d.version, 109);
        assert_eq!(d.skill, 3);
        assert_eq!(d.map_name(true), "MAP01");
        assert_eq!(d.map_name(false), "E1M1");
        assert_eq!(d.tics.len(), 1);
        assert_eq!(d.tics[0][0].forward_move, 25 * 256);
        assert_eq!(d.complevel(), Complevel::Vanilla);
    }
}
