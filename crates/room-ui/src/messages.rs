//! On-screen game messages (pickup / secret / keyed door).

use crate::doom_text::draw_stcfn_string;
use crate::draw::UiDrawList;
use std::collections::VecDeque;

/// Timed HUD message queue.
#[derive(Clone, Debug, Default)]
pub struct MessageQueue {
    /// Active messages (front = oldest).
    entries: VecDeque<Message>,
}

#[derive(Clone, Debug, PartialEq)]
struct Message {
    text: String,
    /// Remaining display tics.
    tics: u32,
}

impl MessageQueue {
    /// Empty queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a message shown for `tics` (Doom default ~70).
    pub fn push(&mut self, text: impl Into<String>, tics: u32) {
        self.entries.push_back(Message {
            text: text.into(),
            tics,
        });
        while self.entries.len() > 4 {
            self.entries.pop_front();
        }
    }

    /// Advance timers by one tic; drop expired.
    pub fn tick(&mut self) {
        for m in &mut self.entries {
            m.tics = m.tics.saturating_sub(1);
        }
        while self.entries.front().is_some_and(|m| m.tics == 0) {
            self.entries.pop_front();
        }
    }

    /// Draw stacked messages near the top of the HUD.
    ///
    /// Uses `STCFN*` patches (same path as the status bar) so messages never
    /// fall through to the 5×7 canvas bitmap when SoftFont is missing.
    pub fn draw(&self, list: &mut UiDrawList, x: f32, y0: f32) {
        let mut y = y0;
        for m in &self.entries {
            draw_stcfn_string(list, x, y, &m.text, 1.0);
            y += 10.0;
        }
    }

    /// Number of active messages.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_expire() {
        let mut q = MessageQueue::new();
        q.push("A secret is revealed!", 2);
        assert_eq!(q.len(), 1);
        q.tick();
        assert_eq!(q.len(), 1);
        q.tick();
        assert!(q.is_empty());
    }
}
