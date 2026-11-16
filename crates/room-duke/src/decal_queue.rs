//! Deterministic Build decal eviction queue.

use serde::{Deserialize, Serialize};

use room_build_engine::types::SpriteT;
use room_build_engine::world::{MAXSPRITES, MAXSTATUS, SpriteLists};

const DECAL_QUEUE_CAPACITY: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct QueuedSprite {
    sprite: i16,
    picnum: i16,
}

impl Default for QueuedSprite {
    fn default() -> Self {
        Self {
            sprite: -1,
            picnum: 0,
        }
    }
}

/// Chocolate's 64-slot FIFO for footprints and blood pools.
///
/// The tile snapshot prevents a stale queue entry from deleting an unrelated
/// sprite after Build has recycled its sprite index.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DukeDecalQueue {
    entries: Vec<QueuedSprite>,
    cursor: usize,
}

impl Default for DukeDecalQueue {
    fn default() -> Self {
        Self {
            entries: vec![QueuedSprite::default(); DECAL_QUEUE_CAPACITY],
            cursor: 0,
        }
    }
}

impl DukeDecalQueue {
    pub(crate) fn contains(&self, sprite: i16, picnum: i16) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.sprite == sprite && entry.picnum == picnum)
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.entries.len() == DECAL_QUEUE_CAPACITY
            && self.cursor < DECAL_QUEUE_CAPACITY
            && self
                .entries
                .iter()
                .all(|entry| entry.sprite == -1 || (0..MAXSPRITES as i16).contains(&entry.sprite))
    }

    /// Add a live sprite and evict the oldest queued decal when its slot wraps.
    pub(crate) fn insert(&mut self, lists: &mut SpriteLists, sprites: &mut [SpriteT], sprite: i16) {
        let Ok(index) = usize::try_from(sprite) else {
            return;
        };
        let Some(current) = sprites.get(index).copied() else {
            return;
        };
        if index >= MAXSPRITES || lists.sprite_stat[index] == MAXSTATUS as i16 {
            return;
        }

        let evicted = self.entries[self.cursor];
        self.entries[self.cursor] = QueuedSprite {
            sprite,
            picnum: current.picnum,
        };
        self.cursor = (self.cursor + 1) % DECAL_QUEUE_CAPACITY;

        let Ok(evicted_index) = usize::try_from(evicted.sprite) else {
            return;
        };
        if evicted_index >= sprites.len()
            || lists.sprite_stat.get(evicted_index).copied() == Some(MAXSTATUS as i16)
            || sprites[evicted_index].picnum != evicted.picnum
        {
            return;
        }

        let status = lists.sprite_stat[evicted_index];
        if status == crate::stat::STAT_MISC || status == crate::stat::STAT_ACTOR {
            let _ = lists.deletesprite(evicted.sprite);
            sprites[evicted_index].statnum = MAXSTATUS as i16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert_sprite(
        lists: &mut SpriteLists,
        sprites: &mut [SpriteT],
        picnum: i16,
        stat: i16,
    ) -> i16 {
        let index = lists.insertsprite(0, stat);
        assert!(index >= 0);
        sprites[index as usize].picnum = picnum;
        sprites[index as usize].statnum = stat;
        index
    }

    #[test]
    fn evicts_oldest_entries_through_five_fifo_wraps() {
        let mut lists = SpriteLists::new();
        let mut sprites = vec![SpriteT::default(); MAXSPRITES];
        let mut queue = DukeDecalQueue::default();

        for picnum in 0..320 {
            let sprite = insert_sprite(&mut lists, &mut sprites, picnum, crate::stat::STAT_MISC);
            queue.insert(&mut lists, &mut sprites, sprite);
        }

        let mut active_picnums: Vec<_> = lists
            .sprite_stat
            .iter()
            .enumerate()
            .filter(|(_, status)| **status == crate::stat::STAT_MISC)
            .map(|(index, _)| sprites[index].picnum)
            .collect();
        active_picnums.sort_unstable();
        assert_eq!(
            active_picnums,
            (256..320).collect::<Vec<_>>(),
            "only the 64 most recent decals should remain active"
        );
        assert_eq!(queue.entries.len(), DECAL_QUEUE_CAPACITY);
        assert!(queue.is_valid());
    }

    #[test]
    fn stale_sprite_id_does_not_evict_a_reused_non_decal() {
        let mut lists = SpriteLists::new();
        let mut sprites = vec![SpriteT::default(); MAXSPRITES];
        let mut queue = DukeDecalQueue::default();
        let first = insert_sprite(&mut lists, &mut sprites, 10, crate::stat::STAT_MISC);
        queue.insert(&mut lists, &mut sprites, first);
        assert!(queue.contains(first, 10));
        assert!(!queue.contains(first, 11));
        assert!(lists.deletesprite(first));

        let recycled = lists.insertsprite(0, crate::stat::STAT_ACTOR);
        assert_eq!(recycled, first);
        sprites[recycled as usize].picnum = 999;
        sprites[recycled as usize].statnum = crate::stat::STAT_ACTOR;
        assert!(!queue.contains(recycled, 999));

        for picnum in 0..64 {
            let sprite = insert_sprite(&mut lists, &mut sprites, picnum, crate::stat::STAT_MISC);
            queue.insert(&mut lists, &mut sprites, sprite);
        }

        assert_eq!(lists.sprite_stat[first as usize], crate::stat::STAT_ACTOR);
        assert_eq!(sprites[first as usize].picnum, 999);
    }

    #[test]
    fn save_roundtrip_preserves_queue_cursor_and_entries() {
        let mut lists = SpriteLists::new();
        let mut sprites = vec![SpriteT::default(); MAXSPRITES];
        let mut queue = DukeDecalQueue::default();
        for picnum in 1..=3 {
            let sprite = insert_sprite(&mut lists, &mut sprites, picnum, crate::stat::STAT_MISC);
            queue.insert(&mut lists, &mut sprites, sprite);
        }

        let encoded = serde_json::to_vec(&queue).expect("serialize decal queue");
        let restored: DukeDecalQueue = serde_json::from_slice(&encoded).expect("restore queue");
        assert_eq!(restored, queue);
        assert!(restored.is_valid());
    }
}
