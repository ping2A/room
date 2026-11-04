//! ACS `HudMessage` objects (GZDoom `DHUDMessage` family, `hudmessages.cpp`).
//!
//! The playsim keeps the live list and ticks it; the shell draws it.

/// `HUDMSG_LOG`: also print the text to the console.
pub const HUDMSG_LOG: i32 = 0x8000_0000_u32 as i32;
/// `HUDMSG_NOWRAP`.
pub const HUDMSG_NOWRAP: i32 = 0x0800_0000;
/// `HUDMSG_ALPHA`: the last optional argument is the alpha.
pub const HUDMSG_ALPHA: i32 = 0x1000_0000;
/// `HUDMSG_ADDBLEND`.
pub const HUDMSG_ADDBLEND: i32 = 0x2000_0000;

/// One on-screen ACS HUD message.
#[derive(Clone, Debug, PartialEq)]
pub struct AcsHudMessage {
    /// Replacement id (`0` = never replaced).
    pub id: i32,
    /// Text (`\n` breaks lines; `\x1c` starts a colour code).
    pub text: String,
    /// `x` as passed to `HudMessage` (see GZDoom positioning rules).
    pub x: f32,
    /// `y` as passed to `HudMessage`.
    pub y: f32,
    /// `SetHudSize` width (`0` = screen-relative).
    pub hud_w: i32,
    /// `SetHudSize` height (negative covers the status bar).
    pub hud_h: i32,
    /// `CR_*` colour, or `-1` when `color_name` is set.
    pub color: i32,
    /// Named colour (`HUDMSG_COLORSTRING`).
    pub color_name: Option<String>,
    /// Font (`SetFont`); empty = small font.
    pub font: String,
    /// 0 plain, 1 fade out, 2 type on, 3 fade in/out.
    pub kind: u8,
    /// Raw `HUDMSG_*` flags.
    pub flags: i32,
    /// Hold tics (`0` = forever for plain messages).
    pub hold_tics: i32,
    /// Fade-in tics (kind 3).
    pub fade_in_tics: i32,
    /// Fade-out tics (kinds 1–3).
    pub fade_out_tics: i32,
    /// Tics per typed character (kind 2).
    pub type_on_tics: f32,
    /// Base alpha.
    pub alpha: f32,
    /// GZDoom `State`: 0 fade in, 1 hold, 2 fade out, 3 typing.
    pub state: u8,
    /// Tics in the current state (`Tics`).
    pub tics: i32,
    /// Seat that sees it (`None` = everyone).
    pub seat: Option<usize>,
}

impl AcsHudMessage {
    /// Build from a decoded `HudMessage` call (GZDoom `PCD_ENDHUDMESSAGE` switch).
    #[must_use]
    pub fn from_args(a: &room_acs::HudMessageArgs, seat: Option<usize>) -> Self {
        let kind = (a.type_flags & 0xff) as u8;
        let opt = |i: usize, d: f32| a.opt.get(i).copied().unwrap_or(d);
        let tics = |secs: f32| (secs * 35.0) as i32;
        let (fade_in, fade_out, type_on, alpha_idx) = match kind {
            1 => (0.0, opt(0, 0.5), 0.0, 1),
            2 => (0.0, opt(1, 0.5), opt(0, 0.05), 2),
            3 => (opt(0, 0.5), opt(1, 0.5), 0.0, 2),
            _ => (0.0, 0.0, 0.0, 0),
        };
        let kind = if kind > 3 { 0 } else { kind };
        let alpha = if a.type_flags & HUDMSG_ALPHA != 0 {
            opt(alpha_idx, 1.0)
        } else {
            1.0
        };
        let type_on_tics = {
            let t = type_on * 35.0;
            if t == 0.0 { 0.1 } else { t }
        };
        let fade_in_tics = tics(fade_in);
        let state = match kind {
            1 => 1,
            2 => 3,
            3 => u8::from(fade_in_tics == 0),
            _ => 1,
        };
        Self {
            id: a.id,
            text: a.text.clone(),
            x: a.x,
            y: a.y,
            hud_w: a.hud_w,
            hud_h: a.hud_h,
            color: if a.color_name.is_some() { -1 } else { a.color },
            color_name: a.color_name.clone(),
            font: a.font.clone(),
            kind,
            flags: a.type_flags,
            hold_tics: tics(a.hold),
            fade_in_tics,
            fade_out_tics: tics(fade_out),
            type_on_tics,
            alpha,
            state,
            tics: -1,
            seat,
        }
    }

    /// Visible character count of the text while typing (`None` = all).
    #[must_use]
    pub fn typed_chars(&self) -> Option<usize> {
        (self.kind == 2 && self.state == 3)
            .then(|| (self.tics.max(0) as f32 / self.type_on_tics) as usize)
    }

    /// Current opacity.
    #[must_use]
    pub fn current_alpha(&self) -> f32 {
        match self.state {
            0 if self.fade_in_tics > 0 => {
                self.alpha * self.tics.max(0) as f32 / self.fade_in_tics as f32
            }
            2 if self.fade_out_tics > 0 => {
                self.alpha * (self.fade_out_tics - self.tics).max(0) as f32
                    / self.fade_out_tics as f32
            }
            _ => self.alpha,
        }
    }

    /// One tic; returns true when the message has expired.
    pub fn tick(&mut self) -> bool {
        self.tics += 1;
        match self.kind {
            0 => self.hold_tics != 0 && self.hold_tics <= self.tics,
            _ => {
                if self.state == 3 {
                    let total = self.text.chars().filter(|&c| c != '\n').count();
                    if self.typed_chars().is_some_and(|n| n > total) {
                        self.state = 1;
                        self.tics = 0;
                    }
                    return false;
                }
                if self.state == 0 && self.fade_in_tics <= self.tics {
                    self.state = 1;
                    self.tics -= self.fade_in_tics;
                }
                if self.state == 1 && self.hold_tics <= self.tics {
                    self.state = 2;
                    self.tics -= self.hold_tics;
                }
                self.state == 2 && self.fade_out_tics <= self.tics
            }
        }
    }
}

impl crate::World {
    /// `StatusBar->AttachMessage`: a non-zero id replaces the message with that id.
    pub fn attach_hud_message(&mut self, msg: AcsHudMessage) {
        if msg.id != 0 {
            self.hud_messages.retain(|m| m.id != msg.id);
        }
        self.hud_messages.push(msg);
    }

    /// Advance every HUD message one tic and drop the expired ones.
    pub fn tick_hud_messages(&mut self) {
        self.hud_messages.retain_mut(|m| !m.tick());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(kind: i32, id: i32, hold: f32, opt: Vec<f32>) -> room_acs::HudMessageArgs {
        room_acs::HudMessageArgs {
            text: "hi".into(),
            type_flags: kind,
            id,
            color: 6,
            color_name: None,
            x: 0.5,
            y: 0.5,
            hold,
            opt,
            hud_w: 0,
            hud_h: 0,
            font: String::new(),
            bold: false,
            activator: 0,
        }
    }

    #[test]
    fn plain_message_holds_then_expires() {
        let mut m = AcsHudMessage::from_args(&args(0, 1, 1.0, vec![]), None);
        let mut n = 0;
        while !m.tick() {
            n += 1;
        }
        assert_eq!(n, 35);
    }

    #[test]
    fn zero_hold_plain_message_stays() {
        let mut m = AcsHudMessage::from_args(&args(0, 1, 0.0, vec![]), None);
        for _ in 0..1000 {
            assert!(!m.tick());
        }
    }

    #[test]
    fn fade_out_message_fades_after_hold() {
        let mut m = AcsHudMessage::from_args(&args(1, 1, 1.0, vec![1.0]), None);
        for _ in 0..38 {
            assert!(!m.tick());
        }
        assert_eq!(m.state, 2);
        assert!(m.current_alpha() < 1.0);
        let mut n = 0;
        while !m.tick() {
            n += 1;
        }
        assert!(n < 35);
    }
}
