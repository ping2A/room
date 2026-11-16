//! The working canvas: a grid of 32-unit cells, each solid or floor of an
//! interned sector, optionally cut in two along a 45° diagonal (chamfered
//! corners, octagonal rooms).

use std::collections::HashMap;

/// World units per cell.
pub const CELL: i32 = 32;

/// Everything that makes two floor cells the same sector.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SecProps {
    /// Floor height.
    pub floor: i16,
    /// Ceiling height.
    pub ceil: i16,
    /// Floor flat.
    pub floor_pic: String,
    /// Ceiling flat.
    pub ceil_pic: String,
    /// Light level.
    pub light: i16,
    /// Doom sector special (damage, blink…).
    pub special: i16,
    /// Sector tag.
    pub tag: i16,
    /// Wall texture for walls and uppers facing this sector.
    pub wall: String,
    /// Texture of this sector's sides when its floor is the higher one
    /// (stair risers, crate sides, platform skirts).
    pub skirt: String,
    /// Teleporter pad: walking onto it teleports to the pad tagged so.
    pub teleport_to: i16,
    /// Secret door (opens on use, stays open).
    pub door: bool,
}

/// Solid material (a wall that is not the room's plain wall).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Mat {
    /// Wall texture.
    pub tex: String,
    /// Line special on its faces (exit switch…).
    pub special: i16,
}

/// What fills a cell (or half-cell).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Area {
    /// Wall: `0` = plain (the facing sector's wall), else a [`Mat`] index.
    Solid(u16),
    /// Floor of an interned sector.
    Floor(u32),
}

impl Area {
    /// Empty space outside the map.
    pub const VOID: Area = Area::Solid(0);

    /// Sector index if floor.
    #[must_use]
    pub fn sector(self) -> Option<u32> {
        match self {
            Area::Floor(s) => Some(s),
            Area::Solid(_) => None,
        }
    }
}

/// 45° cut through a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Diag {
    /// From the south-west corner to the north-east corner. Half `a` is the
    /// south-east triangle (owns the south and east edges).
    Slash,
    /// From the north-west corner to the south-east corner. Half `a` is the
    /// south-west triangle (owns the south and west edges).
    Backslash,
}

/// Cell side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// +y.
    N,
    /// +x.
    E,
    /// -y.
    S,
    /// -x.
    W,
}

/// Who a cell belongs to (for features, things and checks).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    /// Rock.
    None,
    /// Room index.
    Room(u16),
    /// Corridor index.
    Corridor(u16),
}

/// One cell.
#[derive(Clone, Copy, Debug)]
pub struct Cell {
    /// Whole cell, or half `a` when split.
    pub a: Area,
    /// Diagonal cut and half `b`.
    pub split: Option<(Diag, Area)>,
    /// Region owning the cell.
    pub owner: Owner,
    /// Marked by a feature (keeps other features and items off).
    pub busy: bool,
}

impl Cell {
    /// Area owning edge `side`.
    #[must_use]
    pub fn edge(&self, side: Side) -> Area {
        match self.split {
            None => self.a,
            Some((Diag::Slash, b)) => match side {
                Side::S | Side::E => self.a,
                Side::N | Side::W => b,
            },
            Some((Diag::Backslash, b)) => match side {
                Side::S | Side::W => self.a,
                Side::N | Side::E => b,
            },
        }
    }

    /// Whole-cell floor sector (split cells are not walkable spots).
    #[must_use]
    pub fn full_floor(&self) -> Option<u32> {
        if self.split.is_some() {
            return None;
        }
        self.a.sector()
    }
}

/// The canvas plus interned sectors and materials.
#[derive(Clone, Debug)]
pub struct Grid {
    /// Width in cells.
    pub w: i32,
    /// Height in cells.
    pub h: i32,
    cells: Vec<Cell>,
    /// Interned sector properties.
    pub sectors: Vec<SecProps>,
    sector_ids: HashMap<SecProps, u32>,
    /// Interned materials (`0` = plain wall).
    pub mats: Vec<Mat>,
}

impl Grid {
    /// All-solid canvas.
    #[must_use]
    pub fn new(w: i32, h: i32) -> Self {
        Self {
            w,
            h,
            cells: vec![
                Cell {
                    a: Area::VOID,
                    split: None,
                    owner: Owner::None,
                    busy: false,
                };
                (w * h).max(0) as usize
            ],
            sectors: Vec::new(),
            sector_ids: HashMap::new(),
            mats: vec![Mat {
                tex: String::new(),
                special: 0,
            }],
        }
    }

    /// Inside the canvas?
    #[must_use]
    pub fn inside(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.w && y < self.h
    }

    /// Cell at `(x, y)` (solid outside).
    #[must_use]
    pub fn get(&self, x: i32, y: i32) -> Cell {
        if self.inside(x, y) {
            self.cells[(y * self.w + x) as usize]
        } else {
            Cell {
                a: Area::VOID,
                split: None,
                owner: Owner::None,
                busy: false,
            }
        }
    }

    /// Mutable cell (`None` outside).
    pub fn at(&mut self, x: i32, y: i32) -> Option<&mut Cell> {
        if self.inside(x, y) {
            let i = (y * self.w + x) as usize;
            self.cells.get_mut(i)
        } else {
            None
        }
    }

    /// Intern sector properties.
    pub fn sector(&mut self, p: SecProps) -> u32 {
        if let Some(&id) = self.sector_ids.get(&p) {
            return id;
        }
        let id = self.sectors.len() as u32;
        self.sectors.push(p.clone());
        self.sector_ids.insert(p, id);
        id
    }

    /// Intern a material.
    pub fn mat(&mut self, m: Mat) -> u16 {
        if let Some(i) = self.mats.iter().position(|x| *x == m) {
            return i as u16;
        }
        self.mats.push(m);
        (self.mats.len() - 1) as u16
    }

    /// Props of the whole-cell floor at `(x, y)`.
    #[must_use]
    pub fn props(&self, x: i32, y: i32) -> Option<&SecProps> {
        self.get(x, y)
            .full_floor()
            .and_then(|s| self.sectors.get(s as usize))
    }

    /// Re-sector a whole floor cell with `f(props)`; keeps owner.
    pub fn restyle(&mut self, x: i32, y: i32, f: impl FnOnce(&mut SecProps)) {
        let Some(mut p) = self.props(x, y).cloned() else {
            return;
        };
        f(&mut p);
        let id = self.sector(p);
        if let Some(c) = self.at(x, y) {
            c.a = Area::Floor(id);
        }
    }

    /// Turn a cell into a wall of material `mat`.
    pub fn wall(&mut self, x: i32, y: i32, mat: u16) {
        if let Some(c) = self.at(x, y) {
            c.a = Area::Solid(mat);
            c.split = None;
            c.busy = true;
        }
    }
}
