//! Atomic sector shade cyclers (`premap` / `movecyclers`).

use room_build_engine::helpers::sincos_q14;
use room_build_engine::world::BuildWorld;

use crate::prelevel::ShadeCycler;

/// Advance `movecyclers` at its caller's 15 Hz cadence.
pub(crate) fn tick_shade_cyclers(world: &mut BuildWorld, cyclers: &mut [ShadeCycler]) {
    for cycler in cyclers.iter_mut().rev() {
        let Ok(sector_index) = usize::try_from(cycler.sector) else {
            continue;
        };
        let Some(sector) = world.sectors.get(sector_index).copied() else {
            continue;
        };
        let (sin, _) = sincos_q14(i32::from(cycler.phase) & 2047);
        let mut shade = i32::from(cycler.max_shade) + (sin >> 10);
        let minimum = i16::from(cycler.min_shade as u8);
        let maximum = i16::from(cycler.max_shade);
        if shade < i32::from(minimum) {
            shade = i32::from(minimum);
        } else if shade > i32::from(maximum) {
            shade = i32::from(maximum);
        }
        cycler.phase = cycler.phase.wrapping_add(sector.extra);
        if !cycler.active {
            continue;
        }

        let shade = shade as i8;
        if let Some(sector) = world.sectors.get_mut(sector_index) {
            sector.floorshade = shade;
            sector.ceilingshade = shade;
        }
        let first_wall = sector.wallptr.max(0) as usize;
        let wall_end = first_wall.saturating_add(sector.wallnum.max(0) as usize);
        for wall_index in first_wall..wall_end.min(world.walls.len()) {
            let (cstat, nextwall) = {
                let wall = &mut world.walls[wall_index];
                if wall.hitag == 1 {
                    continue;
                }
                wall.shade = shade;
                (wall.cstat, wall.nextwall)
            };
            if cstat & 2 != 0 && nextwall >= 0 {
                if let Some(peer) = world.walls.get_mut(nextwall as usize) {
                    peer.shade = shade;
                }
            }
        }
    }
}
