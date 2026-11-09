//! Sound zones (ZDoom `MapLoader::FloodZones`): sectors joined by two-sided
//! lines form a zone; UDMF `zoneboundary` lines (ZDoom `ML_ZONEBOUNDARY`)
//! split zones. Each zone carries one reverb environment, set by
//! `SoundEnvironment` things (DoomEdNum 9048).

use crate::MapGeometry;

/// DoomEdNum of ZDoom's `SoundEnvironment` thing.
pub const SOUND_ENVIRONMENT_DOOMEDNUM: i16 = 9048;

/// Zone index per sector (`sector_zone[sector]`) and the zone count.
#[must_use]
pub fn flood_sound_zones(map: &MapGeometry) -> (Vec<u16>, usize) {
    use crate::udmf::ext::LINE_ZONEBOUNDARY;
    let n = map.sectors.len();
    // Adjacency through two-sided, non-boundary lines.
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (li, line) in map.lines.iter().enumerate() {
        let (Some(f), Some(b)) = (line.front_sector, line.back_sector) else {
            continue;
        };
        if map.udmf_user.line_ext(li) & LINE_ZONEBOUNDARY != 0 {
            continue;
        }
        let (f, b) = (f.raw() as usize, b.raw() as usize);
        if f < n && b < n && f != b {
            adj[f].push(b);
            adj[b].push(f);
        }
    }
    let mut zone = vec![u16::MAX; n];
    let mut count = 0usize;
    let mut stack = Vec::new();
    for start in 0..n {
        if zone[start] != u16::MAX {
            continue;
        }
        let z = u16::try_from(count).unwrap_or(u16::MAX - 1);
        count += 1;
        zone[start] = z;
        stack.push(start);
        while let Some(s) = stack.pop() {
            for &o in &adj[s] {
                if zone[o] == u16::MAX {
                    zone[o] = z;
                    stack.push(o);
                }
            }
        }
    }
    (zone, count)
}
