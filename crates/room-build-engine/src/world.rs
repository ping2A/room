//! `BuildWorld` capacities and sprite sector/stat lists.
//! Spec: `docs/build-engine-primitives/01-world.md`.

pub const MAXSECTORS: usize = 1024;
pub const MAXWALLS: usize = 8192;
pub const MAXSPRITES: usize = 4096;
pub const MAXSTATUS: usize = 1024;

const FREE_SENTINEL: i16 = -1;

/// Intrusive sprite lists by sector and by status, plus a free list.
///
/// Head-insert on insert/change; free sprites hang off `headspritestat[MAXSTATUS]`.
#[derive(Clone, Debug)]
pub struct SpriteLists {
    pub headspritesect: [i16; MAXSECTORS + 1],
    pub headspritestat: [i16; MAXSTATUS + 1],
    pub prevspritesect: [i16; MAXSPRITES],
    pub nextspritesect: [i16; MAXSPRITES],
    pub prevspritestat: [i16; MAXSPRITES],
    pub nextspritestat: [i16; MAXSPRITES],
    /// Sector of each sprite; `-1` if free.
    pub sprite_sect: [i16; MAXSPRITES],
    /// Status of each sprite; `MAXSTATUS as i16` if free.
    pub sprite_stat: [i16; MAXSPRITES],
}

impl Default for SpriteLists {
    fn default() -> Self {
        Self::new()
    }
}

impl SpriteLists {
    pub fn new() -> Self {
        let mut s = Self {
            headspritesect: [FREE_SENTINEL; MAXSECTORS + 1],
            headspritestat: [FREE_SENTINEL; MAXSTATUS + 1],
            prevspritesect: [FREE_SENTINEL; MAXSPRITES],
            nextspritesect: [FREE_SENTINEL; MAXSPRITES],
            prevspritestat: [FREE_SENTINEL; MAXSPRITES],
            nextspritestat: [FREE_SENTINEL; MAXSPRITES],
            sprite_sect: [-1; MAXSPRITES],
            sprite_stat: [MAXSTATUS as i16; MAXSPRITES],
        };
        // Free list: all sprites in status MAXSTATUS, head-insert 0..n-1 so
        // first alloc is 0 (document vs vanilla when goldens land).
        for i in (0..MAXSPRITES).rev() {
            s.link_stat(i as i16, MAXSTATUS as i16);
            s.sprite_sect[i] = -1;
            s.sprite_stat[i] = MAXSTATUS as i16;
        }
        s
    }

    pub(crate) fn link_stat(&mut self, i: i16, stat: i16) {
        let su = stat as usize;
        let iu = i as usize;
        let head = self.headspritestat[su];
        self.prevspritestat[iu] = FREE_SENTINEL;
        self.nextspritestat[iu] = head;
        if head >= 0 {
            self.prevspritestat[head as usize] = i;
        }
        self.headspritestat[su] = i;
    }

    pub(crate) fn unlink_stat(&mut self, i: i16) {
        let iu = i as usize;
        let prev = self.prevspritestat[iu];
        let next = self.nextspritestat[iu];
        let stat = self.sprite_stat[iu] as usize;
        if prev < 0 {
            self.headspritestat[stat] = next;
        } else {
            self.nextspritestat[prev as usize] = next;
        }
        if next >= 0 {
            self.prevspritestat[next as usize] = prev;
        }
        self.prevspritestat[iu] = FREE_SENTINEL;
        self.nextspritestat[iu] = FREE_SENTINEL;
    }

    pub(crate) fn link_sect(&mut self, i: i16, sect: i16) {
        let su = sect as usize;
        let iu = i as usize;
        let head = self.headspritesect[su];
        self.prevspritesect[iu] = FREE_SENTINEL;
        self.nextspritesect[iu] = head;
        if head >= 0 {
            self.prevspritesect[head as usize] = i;
        }
        self.headspritesect[su] = i;
    }

    fn unlink_sect(&mut self, i: i16) {
        let iu = i as usize;
        let prev = self.prevspritesect[iu];
        let next = self.nextspritesect[iu];
        let sect = self.sprite_sect[iu];
        if sect < 0 {
            return;
        }
        let su = sect as usize;
        if prev < 0 {
            self.headspritesect[su] = next;
        } else {
            self.nextspritesect[prev as usize] = next;
        }
        if next >= 0 {
            self.prevspritesect[next as usize] = prev;
        }
        self.prevspritesect[iu] = FREE_SENTINEL;
        self.nextspritesect[iu] = FREE_SENTINEL;
    }

    /// Take sprite index `i` off the free list and link into `sect`/`stat`.
    /// Used when loading a map that already assigned sprite indices.
    pub fn place_sprite_at(&mut self, i: i16, sect: i16, stat: i16) -> bool {
        if !(0..MAXSPRITES as i16).contains(&i)
            || !(0..MAXSECTORS as i16).contains(&sect)
            || !(0..MAXSTATUS as i16).contains(&stat)
        {
            return false;
        }
        let iu = i as usize;
        if self.sprite_stat[iu] != MAXSTATUS as i16 {
            return false; // not free
        }
        self.unlink_stat(i);
        self.sprite_sect[iu] = sect;
        self.sprite_stat[iu] = stat;
        self.link_stat(i, stat);
        self.link_sect(i, sect);
        true
    }

    /// Insert into sector `sect` and status `stat`. Returns index or `-1`.
    pub fn insertsprite(&mut self, sect: i16, stat: i16) -> i16 {
        if !(0..MAXSECTORS as i16).contains(&sect) || !(0..MAXSTATUS as i16).contains(&stat) {
            return -1;
        }
        let i = self.headspritestat[MAXSTATUS];
        if i < 0 {
            return -1;
        }
        self.unlink_stat(i);
        self.sprite_sect[i as usize] = sect;
        self.sprite_stat[i as usize] = stat;
        self.link_stat(i, stat);
        self.link_sect(i, sect);
        i
    }

    pub fn deletesprite(&mut self, i: i16) -> bool {
        if !(0..MAXSPRITES as i16).contains(&i) {
            return false;
        }
        let iu = i as usize;
        if self.sprite_stat[iu] == MAXSTATUS as i16 {
            return false; // already free
        }
        self.unlink_sect(i);
        self.unlink_stat(i);
        self.sprite_sect[iu] = -1;
        self.sprite_stat[iu] = MAXSTATUS as i16;
        self.link_stat(i, MAXSTATUS as i16);
        true
    }

    pub fn changespritesect(&mut self, i: i16, newsect: i16) -> bool {
        if !(0..MAXSPRITES as i16).contains(&i) || !(0..MAXSECTORS as i16).contains(&newsect) {
            return false;
        }
        let iu = i as usize;
        if self.sprite_stat[iu] == MAXSTATUS as i16 {
            return false;
        }
        if self.sprite_sect[iu] == newsect {
            return true;
        }
        self.unlink_sect(i);
        self.sprite_sect[iu] = newsect;
        self.link_sect(i, newsect);
        true
    }

    pub fn changespritestat(&mut self, i: i16, newstat: i16) -> bool {
        if !(0..MAXSPRITES as i16).contains(&i) || !(0..MAXSTATUS as i16).contains(&newstat) {
            return false;
        }
        let iu = i as usize;
        if self.sprite_stat[iu] == MAXSTATUS as i16 {
            return false;
        }
        if self.sprite_stat[iu] == newstat {
            return true;
        }
        self.unlink_stat(i);
        self.sprite_stat[iu] = newstat;
        self.link_stat(i, newstat);
        true
    }
}

/// Build playsim world (D3+).
#[derive(Clone, Debug)]
pub struct BuildWorld {
    pub numsectors: i16,
    pub numwalls: i16,
    pub sectors: Vec<crate::types::SectorT>,
    pub walls: Vec<crate::types::WallT>,
    pub sprites: Box<[crate::types::SpriteT; MAXSPRITES]>,
    pub lists: SpriteLists,
    pub randomseed: i32,
    pub start: [i32; 3],
    pub start_ang: i16,
    pub start_sector: i16,
}

impl Default for BuildWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildWorld {
    /// Collect wall / floor / ceiling picnums used by loaded geometry.
    pub fn referenced_picnums(&self) -> Vec<i16> {
        let mut pics = Vec::new();
        for s in self.sectors.iter().take(self.numsectors as usize) {
            pics.push(s.floorpicnum);
            pics.push(s.ceilingpicnum);
        }
        for w in self.walls.iter().take(self.numwalls as usize) {
            pics.push(w.picnum);
            pics.push(w.overpicnum);
        }
        pics.sort_unstable();
        pics.dedup();
        pics
    }

    pub fn new() -> Self {
        Self {
            numsectors: 0,
            numwalls: 0,
            sectors: Vec::new(),
            walls: Vec::new(),
            sprites: Box::new([crate::types::SpriteT::default(); MAXSPRITES]),
            lists: SpriteLists::new(),
            randomseed: 1,
            start: [0; 3],
            start_ang: 0,
            start_sector: 0,
        }
    }

    pub fn krand(&mut self) -> i32 {
        crate::numeric::krand_step(&mut self.randomseed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_delete_roundtrip() {
        let mut w = BuildWorld::new();
        w.numsectors = 4;
        let a = w.lists.insertsprite(0, 1);
        let b = w.lists.insertsprite(0, 1);
        assert_eq!(a, 0);
        assert_eq!(b, 1);
        // Head-insert: newest is head.
        assert_eq!(w.lists.headspritestat[1], b);
        assert_eq!(w.lists.nextspritestat[b as usize], a);
        assert!(w.lists.deletesprite(a));
        assert_eq!(w.lists.sprite_stat[a as usize], MAXSTATUS as i16);
        let c = w.lists.insertsprite(2, 3);
        assert_eq!(c, a); // freelist LIFO via head-insert
    }

    #[test]
    fn change_sect_and_stat() {
        let mut lists = SpriteLists::new();
        let i = lists.insertsprite(1, 2);
        assert!(lists.changespritesect(i, 5));
        assert_eq!(lists.sprite_sect[i as usize], 5);
        assert_eq!(lists.headspritesect[5], i);
        assert!(lists.changespritestat(i, 7));
        assert_eq!(lists.sprite_stat[i as usize], 7);
        assert_eq!(lists.headspritestat[7], i);
    }

    #[test]
    fn full_world_rejects() {
        let mut lists = SpriteLists::new();
        for _ in 0..MAXSPRITES {
            assert!(lists.insertsprite(0, 0) >= 0);
        }
        assert_eq!(lists.insertsprite(0, 0), -1);
    }
}
