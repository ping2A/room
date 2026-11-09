//! Hitscan traces (`P_LineAttack` subset).

use crate::actor::ActorId;
use crate::damage::damage_mobj;
use crate::physics::{block_coords, lines_near, path_traverse_blocks};
use crate::world::{ShotKind, ShotTrace, World};
use room_core::dmath;
use room_core::math::{bam_to_fine, finecosine, finesine};
use room_core::{Angle, DVec2, DVec3};

/// Push a hitscan into [`World::recent_shots`] and tag [`World::last_damage`].
fn record_hitscan(world: &mut World, source: ActorId, origin: DVec3, hit: &TraceHit, damage: i32) {
    let source_class = world
        .actors
        .get(source)
        .and_then(|a| world.classes.get(a.class))
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let hit_class = hit
        .actor
        .and_then(|id| world.actors.get(id))
        .and_then(|a| world.classes.get(a.class))
        .map(|c| c.name.clone())
        .unwrap_or_default();
    world.push_shot_trace(ShotTrace {
        tic: world.level.time,
        kind: ShotKind::Hitscan,
        source,
        source_class,
        from: [origin.x(), origin.y(), origin.z()],
        to: [hit.pos.x(), hit.pos.y(), hit.pos.z()],
        hit: hit.actor,
        hit_class,
        damage,
        missile_class: String::new(),
    });
    world.push_fx(crate::fx_events::FxEvent::HitscanFired {
        shooter: source,
        origin,
        end: hit.pos,
        hit_actor: hit.actor,
        damage,
    });
    if let Some(ld) = world.last_damage.as_mut() {
        if ld.tic == world.level.time && ld.cause.is_empty() {
            ld.cause = "hitscan".into();
        }
    }
}

/// `MF_SHOOTABLE`.
const FLAG_SHOOTABLE: u64 = 1 << 1;

/// Result of a hitscan.
#[derive(Clone, Debug)]
pub struct TraceHit {
    /// Actor struck, if any.
    pub actor: Option<ActorId>,
    /// Impact position.
    pub pos: DVec3,
    /// Distance traveled.
    pub distance: f64,
}

/// Doom `shootz = mo->z + (mo->height>>1) + 8` (or ViewHeight+AttackZOffset).
///
/// Actors with an explicit `ViewHeight` (players) shoot from the eye line so
/// freelook hits match the camera crosshair.
pub(crate) fn shoot_origin(world: &World, source: ActorId) -> Option<(DVec3, Angle)> {
    let a = world.actors.get(source)?;
    let class_view = world
        .classes
        .get(a.class)
        .map(|c| c.view_height)
        .unwrap_or(0.0);
    let az = if a.attack_z_offset != 0.0 {
        a.attack_z_offset
    } else {
        world
            .classes
            .get(a.class)
            .map(|c| c.attack_z_offset)
            .unwrap_or(0.0)
    };
    let has_view = a.view_height > 0.0 || class_view > 0.0;
    let view_h = if a.view_height > 0.0 {
        a.view_height
    } else if class_view > 0.0 {
        class_view
    } else {
        a.height * 0.5
    };
    let shoot_z = if has_view {
        a.pos.z() + view_h + az
    } else {
        a.pos.z() + a.height * 0.5 + 8.0
    };
    Some((DVec3::new(a.pos.x(), a.pos.y(), shoot_z), a.pitch))
}

/// Doom `P_AimLineAttack` / `P_LineAttack` XY delta: `(range>>0)*finecosine` in map units.
fn finesine_dir(angle: Angle, range: f64) -> (f64, f64) {
    let fine = bam_to_fine(angle.to_bam().0);
    let range_i = range as i32;
    let dx = f64::from(range_i.wrapping_mul(finecosine(fine))) / 65536.0;
    let dy = f64::from(range_i.wrapping_mul(finesine(fine))) / 65536.0;
    (dx, dy)
}

/// Doom / GZDoom `P_BulletSlope`: autoaim slope, else freelook pitch.
///
/// Units: rise per map-unit run (`fixed/65536`). When no shootable is in the
/// autoaim cone, fall back to `tan(pitch)` so hitscans follow the gun/camera
/// instead of always firing level.
#[must_use]
pub fn bullet_slope(world: &World, source: ActorId) -> f64 {
    let Some(a) = world.actors.get(source) else {
        return 0.0;
    };
    if world.autoaim {
        let base = a.angle.to_bam().0;
        // `1<<26` BAM = 360/64 = 5.625°.
        const AIM_SWEEP: u32 = 1 << 26;
        for d in [0i32, AIM_SWEEP as i32, -(AIM_SWEEP as i32)] {
            let ang = room_core::Bam(base.wrapping_add(d as u32)).to_angle();
            if let Some(slope) = aim_line_attack(world, source, ang, 1024.0) {
                return slope;
            }
        }
    }
    // Clamp so tan stays finite near ±90° (steep shots use the pitch-ray path).
    let pr = a.pitch.radians().clamp(-1.4, 1.4);
    dmath::tan(pr)
}

/// `P_SpawnPlayerMissile` aim from `angle`: the autoaim angle + (slope,
/// linetarget) (centre, then ±5.625°), else `angle` with `None` (caller
/// follows freelook pitch).
#[must_use]
pub(crate) fn player_missile_aim(
    world: &World,
    source: ActorId,
    angle: Angle,
) -> (Angle, Option<(f64, ActorId)>) {
    if world.autoaim {
        let base = angle.to_bam().0;
        const AIM_SWEEP: u32 = 1 << 26;
        for d in [0i32, AIM_SWEEP as i32, -(AIM_SWEEP as i32)] {
            let ang = room_core::Bam(base.wrapping_add(d as u32)).to_angle();
            if let Some(hit) = aim_line_attack_target(world, source, ang, 1024.0) {
                return (ang, Some(hit));
            }
        }
    }
    (angle, None)
}

/// Doom `P_AimLineAttack` subset — returns aimslope if a shootable is sighted.
#[must_use]
pub fn aim_line_attack(world: &World, source: ActorId, angle: Angle, range: f64) -> Option<f64> {
    aim_line_attack_target(world, source, angle, range).map(|(slope, _)| slope)
}

/// [`aim_line_attack`] plus the sighted actor (`linetarget`).
#[must_use]
pub(crate) fn aim_line_attack_target(
    world: &World,
    source: ActorId,
    angle: Angle,
    range: f64,
) -> Option<(f64, ActorId)> {
    let (origin, _) = shoot_origin(world, source)?;
    let (dir_x, dir_y) = {
        let (dx, dy) = finesine_dir(angle, range);
        (dx / range, dy / range)
    };
    let tdx = dir_x * range;
    let tdy = dir_y * range;
    // Doom init: ±(SCREENHEIGHT/2)/(SCREENWIDTH/2) → ±0.625.
    let mut top_slope = 0.625;
    let mut bottom_slope = -0.625;
    let shootz = origin.z();

    #[derive(Clone, Copy)]
    enum Hit {
        Line { dist: f64, lid: usize },
        Thing { dist: f64, id: ActorId },
    }
    let mut hits: Vec<Hit> = Vec::new();
    let p0 = DVec2::new(origin.x(), origin.y());
    let p1 = DVec2::new(origin.x() + tdx, origin.y() + tdy);
    let mid = DVec2::new((p0.x() + p1.x()) * 0.5, (p0.y() + p1.y()) * 0.5);
    let radius = (range * 0.5).max(64.0) + 32.0;
    for lid in lines_near(&world.map, mid, radius) {
        let Some(line) = world.map.lines.get(lid.raw() as usize) else {
            continue;
        };
        let Some(v1) = world.map.vertices.get(line.v1.raw() as usize) else {
            continue;
        };
        let Some(v2) = world.map.vertices.get(line.v2.raw() as usize) else {
            continue;
        };
        let Some(t) = ray_hits_seg(p0, p1, v1.pos, v2.pos) else {
            continue;
        };
        let dist = t * range;
        if dist > 1e-6 && dist <= range {
            hits.push(Hit::Line {
                dist,
                lid: lid.raw() as usize,
            });
        }
    }
    // Vanilla only sees things in `P_PathTraverse` block cells (corner skip).
    let path_blocks = path_traverse_blocks(
        &world.map.blockmap,
        origin.x(),
        origin.y(),
        origin.x() + tdx,
        origin.y() + tdy,
    );
    let path_set: std::collections::BTreeSet<(i32, i32)> = path_blocks.into_iter().collect();
    for &id in &world.thinkers.default {
        if id == source {
            continue;
        }
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.destroyed || (a.flags & FLAG_SHOOTABLE) == 0 {
            continue;
        }
        if !path_set.is_empty() {
            let (bx, by) = block_coords(&world.map.blockmap, a.pos.x(), a.pos.y());
            if !path_set.contains(&(i32::from(bx), i32::from(by))) {
                continue;
            }
        }
        // Doom `PIT_AddThingIntercepts`: corner-to-corner diagonal, not AABB entry.
        let Some(frac) = thing_intercept_frac(
            origin.x(),
            origin.y(),
            tdx,
            tdy,
            a.pos.x(),
            a.pos.y(),
            a.radius,
        ) else {
            continue;
        };
        let dist = frac * range;
        if dist > 1e-6 && dist <= range {
            hits.push(Hit::Thing { dist, id });
        }
    }
    hits.sort_by(|a, b| {
        let da = match a {
            Hit::Line { dist, .. } | Hit::Thing { dist, .. } => *dist,
        };
        let db = match b {
            Hit::Line { dist, .. } | Hit::Thing { dist, .. } => *dist,
        };
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    });

    for hit in hits {
        match hit {
            Hit::Line { dist, lid } => {
                let Some(line) = world.map.lines.get(lid) else {
                    continue;
                };
                if line.back_sector.is_none() || blocks_hitscan(world, lid as u32) {
                    return None; // solid / blockhitscan wall — no target
                }
                let Some((open_bottom, open_top)) = line_opening_z(world, line) else {
                    return None;
                };
                if open_top <= open_bottom {
                    return None;
                }
                let fs = line
                    .front_sector
                    .and_then(|s| world.sectors.get(s.raw() as usize));
                let bs = line
                    .back_sector
                    .and_then(|s| world.sectors.get(s.raw() as usize));
                if let (Some(fs), Some(bs)) = (fs, bs) {
                    if (fs.floor_z - bs.floor_z).abs() > 1e-6 {
                        let slope = (open_bottom - shootz) / dist;
                        if slope > bottom_slope {
                            bottom_slope = slope;
                        }
                    }
                    if (fs.ceiling_z - bs.ceiling_z).abs() > 1e-6 {
                        let slope = (open_top - shootz) / dist;
                        if slope < top_slope {
                            top_slope = slope;
                        }
                    }
                }
                if top_slope <= bottom_slope {
                    return None;
                }
            }
            Hit::Thing { dist, id } => {
                let Some(a) = world.actors.get(id) else {
                    continue;
                };
                let thing_top = (a.pos.z() + a.height - shootz) / dist;
                if thing_top < bottom_slope {
                    continue; // over
                }
                let thing_bot = (a.pos.z() - shootz) / dist;
                if thing_bot > top_slope {
                    continue; // under
                }
                let slope = (thing_top.min(top_slope) + thing_bot.max(bottom_slope)) * 0.5;
                if std::env::var_os("ROOM_TRACE_WALL").is_some() {
                    let name = world
                        .classes
                        .get(a.class)
                        .map(|c| c.name.as_str())
                        .unwrap_or("?");
                    eprintln!(
                        "AIM@{} {} dist={dist:.1} slope={slope:.6} (~{}) top={top_slope:.3} bot={bottom_slope:.3} shootz={shootz:.3} pz={:.3} tz={:.3} th={:.3} px={:.1} py={:.1} tx={:.1} ty={:.1}",
                        world.level.time,
                        name,
                        (slope * 65536.0).round() as i32,
                        world.actors.get(source).map(|s| s.pos.z()).unwrap_or(0.0),
                        a.pos.z(),
                        a.height,
                        world.actors.get(source).map(|s| s.pos.x()).unwrap_or(0.0),
                        world.actors.get(source).map(|s| s.pos.y()).unwrap_or(0.0),
                        a.pos.x(),
                        a.pos.y(),
                    );
                }
                return Some((slope, id));
            }
        }
    }
    None
}

/// ACS `PickActor` / `P_LinePickActor` bootstrap — first shootable along the aim ray.
#[must_use]
pub fn line_pick_actor(
    world: &World,
    source: ActorId,
    angle: Angle,
    range: f64,
) -> Option<ActorId> {
    let (origin, _) = shoot_origin(world, source)?;
    let (dir_x, dir_y) = {
        let (dx, dy) = finesine_dir(angle, range);
        (dx / range, dy / range)
    };
    let tdx = dir_x * range;
    let tdy = dir_y * range;
    let mut best: Option<(f64, ActorId)> = None;
    let path_blocks = path_traverse_blocks(
        &world.map.blockmap,
        origin.x(),
        origin.y(),
        origin.x() + tdx,
        origin.y() + tdy,
    );
    let path_set: std::collections::BTreeSet<(i32, i32)> = path_blocks.into_iter().collect();
    for &id in &world.thinkers.default {
        if id == source {
            continue;
        }
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.destroyed || (a.flags & FLAG_SHOOTABLE) == 0 {
            continue;
        }
        if !path_set.is_empty() {
            let (bx, by) = block_coords(&world.map.blockmap, a.pos.x(), a.pos.y());
            if !path_set.contains(&(i32::from(bx), i32::from(by))) {
                continue;
            }
        }
        let Some(frac) = thing_intercept_frac(
            origin.x(),
            origin.y(),
            tdx,
            tdy,
            a.pos.x(),
            a.pos.y(),
            a.radius,
        ) else {
            continue;
        };
        let dist = frac * range;
        if dist > 1e-6 && dist <= range {
            if best.map(|(d, _)| dist < d).unwrap_or(true) {
                best = Some((dist, id));
            }
        }
    }
    best.map(|(_, id)| id)
}

/// Fire a hitscan using the source's pitch as aim slope.
pub fn line_attack(
    world: &mut World,
    source: ActorId,
    angle: Angle,
    range: f64,
    damage: i32,
) -> TraceHit {
    let pitch = world
        .actors
        .get(source)
        .map(|a| a.pitch)
        .unwrap_or(Angle::ZERO);
    // Steep free-look: use a unit 3D ray (tan blows up near ±90°).
    if pitch.degrees().abs() > 80.0 {
        return line_attack_pitch_ray(world, source, angle, range, damage, pitch);
    }
    let aim_slope = dmath::tan(pitch.radians());
    line_attack_with_slope(world, source, angle, range, damage, aim_slope)
}

/// Free-look / near-vertical hitscan (unit direction from yaw×pitch).
fn line_attack_pitch_ray(
    world: &mut World,
    source: ActorId,
    angle: Angle,
    range: f64,
    damage: i32,
    pitch: Angle,
) -> TraceHit {
    let Some((origin, _)) = shoot_origin(world, source) else {
        return TraceHit {
            actor: None,
            pos: DVec3::ZERO,
            distance: 0.0,
        };
    };
    let rad = angle.radians();
    let pr = pitch.radians();
    let cp = dmath::cos(pr);
    let sp = dmath::sin(pr);
    let dir = DVec3::new(dmath::cos(rad) * cp, dmath::sin(rad) * cp, sp);
    let horiz = dmath::sqrt(dir.x() * dir.x() + dir.y() * dir.y());
    let aim_slope = if horiz > 1e-12 { dir.z() / horiz } else { 0.0 };
    // Reuse sloped path when there is horizontal run; else Z-only thing test.
    if horiz > 1e-12 {
        let ang = Angle::from_radians(dmath::atan2(dir.y(), dir.x()));
        return line_attack_with_slope(world, source, ang, range, damage, aim_slope);
    }
    // Pure vertical: hit first shootable in the XY footprint.
    let mut best: Option<(ActorId, f64, DVec3)> = None;
    for &id in &world.thinkers.default.clone() {
        if id == source {
            continue;
        }
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.destroyed || (a.flags & FLAG_SHOOTABLE) == 0 {
            continue;
        }
        if (origin.x() - a.pos.x()).abs() > a.radius || (origin.y() - a.pos.y()).abs() > a.radius {
            continue;
        }
        let z0 = a.pos.z();
        let z1 = z0 + a.height;
        if dir.z().abs() < 1e-12 {
            continue;
        }
        let t0 = (z0 - origin.z()) / dir.z();
        let t1 = (z1 - origin.z()) / dir.z();
        let (tlo, thi) = if t0 <= t1 { (t0, t1) } else { (t1, t0) };
        let t = tlo.max(0.0);
        if t > thi || t > range {
            continue;
        }
        let pos = DVec3::new(origin.x(), origin.y(), origin.z() + dir.z() * t);
        if best.as_ref().is_none_or(|(_, d, _)| t < *d) {
            best = Some((id, t, pos));
        }
    }
    let hit = if let Some((id, dist, pos)) = best {
        spawn_hitscan_impact(world, Some(id), pos, source, damage);
        damage_mobj(world, id, Some(source), Some(source), damage);
        TraceHit {
            actor: Some(id),
            pos,
            distance: dist,
        }
    } else {
        TraceHit {
            actor: None,
            pos: DVec3::new(
                origin.x() + dir.x() * range,
                origin.y() + dir.y() * range,
                origin.z() + dir.z() * range,
            ),
            distance: range,
        }
    };
    record_hitscan(world, source, origin, &hit, damage);
    hit
}

/// Doom `P_LineAttack` with an explicit `aimslope` (rise per map-unit of run).
pub fn line_attack_with_slope(
    world: &mut World,
    source: ActorId,
    angle: Angle,
    range: f64,
    damage: i32,
    aim_slope: f64,
) -> TraceHit {
    let Some((origin, _)) = shoot_origin(world, source) else {
        return TraceHit {
            actor: None,
            pos: DVec3::ZERO,
            distance: 0.0,
        };
    };
    let (tdx, tdy) = finesine_dir(angle, range);
    let dir_x = tdx / range;
    let dir_y = tdy / range;
    // Parameterize by horizontal distance; z = shootz + aim_slope * t.
    let _dir = DVec3::new(dir_x, dir_y, aim_slope);

    let mut best: Option<(ActorId, f64, DVec3)> = None;
    let path_blocks = path_traverse_blocks(
        &world.map.blockmap,
        origin.x(),
        origin.y(),
        origin.x() + tdx,
        origin.y() + tdy,
    );
    let path_set: std::collections::BTreeSet<(i32, i32)> = path_blocks.into_iter().collect();
    let ids: Vec<ActorId> = world.thinkers.default.clone();
    for id in ids {
        if id == source {
            continue;
        }
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.destroyed || (a.flags & FLAG_SHOOTABLE) == 0 {
            continue;
        }
        if !path_set.is_empty() {
            let (bx, by) = block_coords(&world.map.blockmap, a.pos.x(), a.pos.y());
            if !path_set.contains(&(i32::from(bx), i32::from(by))) {
                continue;
            }
        }
        let z0 = a.pos.z();
        let z1 = z0 + a.height;
        let Some(frac) = thing_intercept_frac(
            origin.x(),
            origin.y(),
            tdx,
            tdy,
            a.pos.x(),
            a.pos.y(),
            a.radius,
        ) else {
            continue;
        };
        let t = frac * range;
        if t <= 1e-6 || t > range {
            continue;
        }
        // Doom `PTR_ShootTraverse`: thing slope vs aimslope at intercept dist.
        let thing_top = (z1 - origin.z()) / t;
        let thing_bot = (z0 - origin.z()) / t;
        if thing_top < aim_slope || thing_bot > aim_slope {
            continue;
        }
        let pos = DVec3::new(
            origin.x() + dir_x * t,
            origin.y() + dir_y * t,
            origin.z() + aim_slope * t,
        );
        if best.as_ref().is_none_or(|(_, d, _)| t < *d) {
            best = Some((id, t, pos));
        }
    }

    let wall = nearest_shoot_wall(world, origin, dir_x, dir_y, aim_slope, range);
    let plane = nearest_shoot_plane(world, origin, dir_x, dir_y, aim_slope, range);
    let thing_dist = best.as_ref().map(|(_, d, _)| *d);
    let wall_dist = wall.as_ref().map(|(d, _, _)| *d);
    let plane_dist = plane.as_ref().map(|(d, _, _, _)| *d);
    let nearest = [thing_dist, wall_dist, plane_dist]
        .into_iter()
        .flatten()
        .fold(f64::INFINITY, f64::min);
    if thing_dist == Some(nearest) {
        let (id, dist, pos) = best.unwrap();
        if std::env::var_os("ROOM_TRACE_WALL").is_some()
            && (1330..=1335).contains(&world.level.time)
        {
            let name = world
                .actors
                .get(id)
                .and_then(|a| world.classes.get(a.class))
                .map(|c| c.name.as_str())
                .unwrap_or("?");
            eprintln!(
                "HIT@{} {} dmg={damage} dist={dist:.1} idx={}",
                world.level.time, name, world.vanilla_rng.prndindex
            );
        }
        spawn_hitscan_impact(world, Some(id), pos, source, damage);
        damage_mobj(world, id, Some(source), Some(source), damage);
        let hit = TraceHit {
            actor: Some(id),
            pos,
            distance: dist,
        };
        record_hitscan(world, source, origin, &hit, damage);
        return hit;
    }
    if wall_dist == Some(nearest) {
        let (wd, wpos, lid) = wall.unwrap();
        if std::env::var_os("ROOM_TRACE_WALL").is_some()
            && (1330..=1335).contains(&world.level.time)
        {
            eprintln!(
                "WALLHIT@{} dist={wd:.1} z={:.1} idx={}",
                world.level.time,
                wpos.z(),
                world.vanilla_rng.prndindex
            );
        }
        if !wall_hit_is_sky(world, lid.raw(), wpos.z()) {
            // Vanilla `PTR_ShootTraverse` / GZDoom `P_LineAttack`: the puff sits
            // 4 units back along the shot, so it stays in the shooter's sector
            // instead of landing on the far side's floor (a raised or sloped
            // sector behind a lower wall put puffs on top of the wall).
            let back = (wd - 4.0).max(0.0);
            let ppos = DVec3::new(
                origin.x() + dir_x * back,
                origin.y() + dir_y * back,
                origin.z() + aim_slope * back,
            );
            spawn_puff(world, ppos);
            spawn_wall_bullet_decal(world, source, wpos, Some(lid.raw()));
            let _ = crate::specials::damage_linedef(world, lid, Some(source), damage);
        }
        let hit = TraceHit {
            actor: None,
            pos: wpos,
            distance: wd,
        };
        record_hitscan(world, source, origin, &hit, damage);
        return hit;
    }
    if let Some((pd, ppos, sid, part)) = plane {
        spawn_puff(world, ppos);
        let _ = crate::specials::damage_sector_plane(world, sid, part, damage);
        let hit = TraceHit {
            actor: None,
            pos: ppos,
            distance: pd,
        };
        record_hitscan(world, source, origin, &hit, damage);
        return hit;
    }
    let pos = DVec3::new(
        origin.x() + dir_x * range,
        origin.y() + dir_y * range,
        origin.z() + aim_slope * range,
    );
    let hit = TraceHit {
        actor: None,
        pos,
        distance: range,
    };
    record_hitscan(world, source, origin, &hit, damage);
    hit
}

/// Piercing rail hitscan (`A_RailAttack` / `A_CustomRailgun` bootstrap).
///
/// Damages every shootable along the aim ray until a wall or `limit` hits
/// (`limit <= 0` = unlimited). Returns the last impact for callers.
pub fn rail_attack(
    world: &mut World,
    source: ActorId,
    angle: Angle,
    range: f64,
    damage: i32,
    limit: i32,
) -> TraceHit {
    rail_attack_offset(world, source, angle, range, damage, limit, 0.0)
}

/// Rail trace with a lateral muzzle offset (UZDoom `FRailParams::offset_xy`).
pub fn rail_attack_offset(
    world: &mut World,
    source: ActorId,
    angle: Angle,
    range: f64,
    damage: i32,
    limit: i32,
    offset_xy: f64,
) -> TraceHit {
    rail_attack_offset_puff(world, source, angle, range, damage, limit, offset_xy, None)
}

/// Rail trace with an optional UZDoom puff actor class.
pub fn rail_attack_offset_puff(
    world: &mut World,
    source: ActorId,
    angle: Angle,
    range: f64,
    damage: i32,
    limit: i32,
    offset_xy: f64,
    puff_class: Option<&str>,
) -> TraceHit {
    let Some((mut origin, _)) = shoot_origin(world, source) else {
        return TraceHit {
            actor: None,
            pos: DVec3::ZERO,
            distance: 0.0,
        };
    };
    if offset_xy != 0.0 {
        let fine = bam_to_fine(angle.to_bam().0);
        origin = DVec3::new(
            origin.x() - f64::from(finesine(fine)) * offset_xy / 65536.0,
            origin.y() + f64::from(finecosine(fine)) * offset_xy / 65536.0,
            origin.z(),
        );
    }
    let pitch = world
        .actors
        .get(source)
        .map(|a| a.pitch)
        .unwrap_or(Angle::ZERO);
    let aim_slope = dmath::tan(pitch.radians());
    let (tdx, tdy) = finesine_dir(angle, range);
    let dir_x = tdx / range;
    let dir_y = tdy / range;
    let path_blocks = path_traverse_blocks(
        &world.map.blockmap,
        origin.x(),
        origin.y(),
        origin.x() + tdx,
        origin.y() + tdy,
    );
    let path_set: std::collections::BTreeSet<(i32, i32)> = path_blocks.into_iter().collect();
    let mut hits: Vec<(ActorId, f64, DVec3)> = Vec::new();
    let ids: Vec<ActorId> = world.thinkers.default.clone();
    for id in ids {
        if id == source {
            continue;
        }
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.destroyed || (a.flags & FLAG_SHOOTABLE) == 0 {
            continue;
        }
        if !path_set.is_empty() {
            let (bx, by) = block_coords(&world.map.blockmap, a.pos.x(), a.pos.y());
            if !path_set.contains(&(i32::from(bx), i32::from(by))) {
                continue;
            }
        }
        let z0 = a.pos.z();
        let z1 = z0 + a.height;
        let Some(frac) = thing_intercept_frac(
            origin.x(),
            origin.y(),
            tdx,
            tdy,
            a.pos.x(),
            a.pos.y(),
            a.radius,
        ) else {
            continue;
        };
        let t = frac * range;
        if t <= 1e-6 || t > range {
            continue;
        }
        let thing_top = (z1 - origin.z()) / t;
        let thing_bot = (z0 - origin.z()) / t;
        if thing_top < aim_slope || thing_bot > aim_slope {
            continue;
        }
        let pos = DVec3::new(
            origin.x() + dir_x * t,
            origin.y() + dir_y * t,
            origin.z() + aim_slope * t,
        );
        hits.push((id, t, pos));
    }
    hits.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let wall_dist = nearest_shoot_wall(world, origin, dir_x, dir_y, aim_slope, range)
        .map(|(d, _, _)| d)
        .unwrap_or(range);
    let mut last = TraceHit {
        actor: None,
        pos: DVec3::new(
            origin.x() + dir_x * wall_dist.min(range),
            origin.y() + dir_y * wall_dist.min(range),
            origin.z() + aim_slope * wall_dist.min(range),
        ),
        distance: wall_dist.min(range),
    };
    let mut count = 0i32;
    for (id, dist, pos) in hits {
        if dist > wall_dist {
            break;
        }
        if limit > 0 && count >= limit {
            break;
        }
        let puff_name = puff_class.unwrap_or("BulletPuff");
        let (puff_always, foil_invulnerable, bloodless_impact) = world
            .classes
            .find_name(puff_name)
            .and_then(|class| world.classes.get(class))
            .map(|class| {
                (
                    class.extra_flags.get_name("ALWAYSPUFF"),
                    class.extra_flags.get_name("FOILINVUL"),
                    class.extra_flags.get_name("BLOODLESSIMPACT"),
                )
            })
            .unwrap_or_default();
        let victim_flags = world
            .actors
            .get(id)
            .map(|actor| actor.flags | world.classes.get(actor.class).map_or(0, |c| c.flags))
            .unwrap_or(0);
        let suppress_bleed = victim_flags & crate::class::FLAG_NOBLOOD != 0
            || victim_flags & crate::class::FLAG_DORMANT != 0
            || (victim_flags & crate::class::FLAG_INVULNERABLE != 0 && !foil_invulnerable);
        if suppress_bleed || puff_always {
            spawn_puff_class(world, pos, puff_name);
        }
        let bleed = !suppress_bleed && !bloodless_impact;
        let dealt = damage_mobj(world, id, Some(source), Some(source), damage);
        if bleed {
            spawn_blood(world, id, pos, Some(source), dealt.max(damage));
        }
        last = TraceHit {
            actor: Some(id),
            pos,
            distance: dist,
        };
        count += 1;
    }
    if last.actor.is_none() {
        if let Some((wd, wpos, lid)) =
            nearest_shoot_wall(world, origin, dir_x, dir_y, aim_slope, range)
        {
            if !wall_hit_is_sky(world, lid.raw(), wpos.z()) {
                let puff_name = puff_class.unwrap_or("BulletPuff");
                if world
                    .classes
                    .find_name(puff_name)
                    .and_then(|class| world.classes.get(class))
                    .is_some_and(|class| class.extra_flags.get_name("ALWAYSPUFF"))
                {
                    spawn_puff_class(world, wpos, puff_name);
                }
                spawn_wall_bullet_decal(world, source, wpos, Some(lid.raw()));
                let _ = crate::specials::damage_linedef(world, lid, Some(source), damage);
            }
            last = TraceHit {
                actor: None,
                pos: wpos,
                distance: wd,
            };
        }
    }
    record_hitscan(world, source, origin, &last, damage);
    last
}

/// Nearest destructible floor/ceiling plane along the aim ray (`part` 0 floor / 1 ceiling).
fn nearest_shoot_plane(
    world: &World,
    origin: DVec3,
    dir_x: f64,
    dir_y: f64,
    aim_slope: f64,
    range: f64,
) -> Option<(f64, DVec3, room_core::SectorId, i32)> {
    if aim_slope.abs() < 1e-9 {
        return None;
    }
    let samples = [
        (origin.x(), origin.y()),
        (
            origin.x() + dir_x * range * 0.5,
            origin.y() + dir_y * range * 0.5,
        ),
        (origin.x() + dir_x * range, origin.y() + dir_y * range),
    ];
    let mut best: Option<(f64, DVec3, room_core::SectorId, i32)> = None;
    for &(sx, sy) in &samples {
        let Some(sid) = room_nodebuild::point_in_sector(&world.map, sx, sy)
            .or_else(|| room_nodebuild::point_in_sector_vanilla(&world.map, sx, sy))
        else {
            continue;
        };
        let Some(sec) = world.sectors.get(sid.raw() as usize) else {
            continue;
        };
        for (part, plane_z, hp) in [
            (0_i32, sec.floor_z, sec.health_floor),
            (1_i32, sec.ceiling_z, sec.health_ceiling),
        ] {
            if hp <= 0 {
                continue;
            }
            let t = (plane_z - origin.z()) / aim_slope;
            if t <= 1e-6 || t > range {
                continue;
            }
            let hx = origin.x() + dir_x * t;
            let hy = origin.y() + dir_y * t;
            let hit_sid = room_nodebuild::point_in_sector(&world.map, hx, hy)
                .or_else(|| room_nodebuild::point_in_sector_vanilla(&world.map, hx, hy));
            if hit_sid != Some(sid) {
                continue;
            }
            let pos = DVec3::new(hx, hy, plane_z);
            if best.as_ref().is_none_or(|(d, _, _, _)| t < *d) {
                best = Some((t, pos, sid, part));
            }
        }
    }
    best
}

/// Nearest linedef that stops a hitscan (horizontal run `t`, `z = shootz + aim_slope*t`).
fn nearest_shoot_wall(
    world: &World,
    origin: DVec3,
    dir_x: f64,
    dir_y: f64,
    aim_slope: f64,
    range: f64,
) -> Option<(f64, DVec3, room_core::LineId)> {
    let p0 = DVec2::new(origin.x(), origin.y());
    let p1 = DVec2::new(origin.x() + dir_x * range, origin.y() + dir_y * range);
    let mid = DVec2::new((p0.x() + p1.x()) * 0.5, (p0.y() + p1.y()) * 0.5);
    let radius = (range * 0.5).max(64.0) + 32.0;
    let mut best: Option<(f64, DVec3, room_core::LineId)> = None;
    for lid in lines_near(&world.map, mid, radius) {
        let Some(line) = world.map.lines.get(lid.raw() as usize) else {
            continue;
        };
        let Some(v1) = world.map.vertices.get(line.v1.raw() as usize) else {
            continue;
        };
        let Some(v2) = world.map.vertices.get(line.v2.raw() as usize) else {
            continue;
        };
        let Some(t_xy) = ray_hits_seg(p0, p1, v1.pos, v2.pos) else {
            continue;
        };
        let dist = t_xy * range;
        if dist <= 1e-6 || dist > range {
            continue;
        }
        if !line_blocks_shot_sloped(world, lid, line, origin.z(), aim_slope, dist) {
            continue;
        }
        let hit_z = origin.z() + aim_slope * dist;
        let pos = DVec3::new(origin.x() + dir_x * dist, origin.y() + dir_y * dist, hit_z);
        if best.as_ref().is_none_or(|(d, _, _)| dist < *d) {
            best = Some((dist, pos, lid));
        }
    }
    best
}

fn sector_plane_is_sky(world: &World, sector: usize, ceiling: bool) -> bool {
    #[allow(clippy::cast_possible_truncation)]
    let key = sector as u32;
    let over = if ceiling {
        world.sector_ceiling_pics.get(&key)
    } else {
        world.sector_floor_pics.get(&key)
    };
    let pic = over.map(String::as_str).or_else(|| {
        world.map.sectors.get(sector).map(|s| {
            if ceiling {
                s.ceiling_pic.as_str()
            } else {
                s.floor_pic.as_str()
            }
        })
    });
    pic.is_some_and(|n| {
        let n = n.trim();
        ["F_SKY1", "F_SKY", "F_SKY001"]
            .iter()
            .any(|s| n.eq_ignore_ascii_case(s))
    })
}

/// A wall hit at height `z` that is really the sky: above a sky ceiling or
/// below a sky floor on either side (Doom "don't shoot the sky", which also
/// covers the invisible upper wall between two sky ceilings). No puff, no decal.
fn shot_hits_sky(world: &World, line: &room_map::Line, z: f64) -> bool {
    [line.front_sector, line.back_sector]
        .into_iter()
        .flatten()
        .any(|sid| {
            let si = sid.raw() as usize;
            let Some(s) = world.sectors.get(si) else {
                return false;
            };
            (z > s.ceiling_z && sector_plane_is_sky(world, si, true))
                || (z < s.floor_z && sector_plane_is_sky(world, si, false))
        })
}

fn wall_hit_is_sky(world: &World, line: u32, z: f64) -> bool {
    world
        .map
        .lines
        .get(line as usize)
        .is_some_and(|l| shot_hits_sky(world, l, z))
}

fn line_opening_z(world: &World, line: &room_map::Line) -> Option<(f64, f64)> {
    let front = line.front_sector?;
    let back = line.back_sector?;
    let fs = world.sectors.get(front.raw() as usize)?;
    let bs = world.sectors.get(back.raw() as usize)?;
    let open_top = fs.ceiling_z.min(bs.ceiling_z);
    let open_bottom = fs.floor_z.max(bs.floor_z);
    Some((open_bottom, open_top))
}

/// Doom `PTR_ShootTraverse` line clip vs `aimslope`.
/// UDMF `blockhitscan`, as changed at run time by `Line_SetBlocking`.
fn blocks_hitscan(world: &World, lid: u32) -> bool {
    world.line_flags(room_core::LineId::from_raw(lid)) & crate::world::ML_BLOCKHITSCAN != 0
}

fn line_blocks_shot_sloped(
    world: &World,
    lid: room_core::LineId,
    line: &room_map::Line,
    shootz: f64,
    aim_slope: f64,
    dist: f64,
) -> bool {
    if line.back_sector.is_none() || blocks_hitscan(world, lid.raw()) {
        return true;
    }
    let Some((open_bottom, open_top)) = line_opening_z(world, line) else {
        return true;
    };
    if open_top <= open_bottom {
        return true;
    }
    let front = line.front_sector.unwrap();
    let back = line.back_sector.unwrap();
    let Some(fs) = world.sectors.get(front.raw() as usize) else {
        return true;
    };
    let Some(bs) = world.sectors.get(back.raw() as usize) else {
        return true;
    };
    if (fs.floor_z - bs.floor_z).abs() > 1e-6 {
        let slope = (open_bottom - shootz) / dist;
        if slope > aim_slope {
            return true;
        }
    }
    if (fs.ceiling_z - bs.ceiling_z).abs() > 1e-6 {
        let slope = (open_top - shootz) / dist;
        if slope < aim_slope {
            return true;
        }
    }
    false
}

/// Doom `PIT_AddThingIntercepts` — frac along trace where the corner diagonal crosses.
///
/// `tdx`/`tdy` are the full trace delta (to `attackrange`), matching `trace.dx/dy`.
fn thing_intercept_frac(
    ox: f64,
    oy: f64,
    tdx: f64,
    tdy: f64,
    cx: f64,
    cy: f64,
    radius: f64,
) -> Option<f64> {
    // `tracepositive = (trace.dx ^ trace.dy) > 0` in fixed; sign-bit xor ≈ opposite signs.
    let tracepositive = (tdx >= 0.0) == (tdy >= 0.0);
    let (x1, y1, x2, y2) = if tracepositive {
        (cx - radius, cy + radius, cx + radius, cy - radius)
    } else {
        (cx - radius, cy - radius, cx + radius, cy + radius)
    };
    let s1 = point_on_divline_side(x1, y1, ox, oy, tdx, tdy);
    let s2 = point_on_divline_side(x2, y2, ox, oy, tdx, tdy);
    if s1 == s2 {
        return None;
    }
    // `P_InterceptVector` (float form of the vanilla routine).
    let dl_dx = x2 - x1;
    let dl_dy = y2 - y1;
    let den = dl_dy * tdx - dl_dx * tdy;
    if den.abs() < 1e-12 {
        return None;
    }
    let num = (x1 - ox) * dl_dy + (oy - y1) * dl_dx;
    let frac = num / den;
    if frac < 0.0 {
        return None;
    }
    Some(frac)
}

/// Doom `P_PointOnDivlineSide` (0 front / 1 back).
fn point_on_divline_side(x: f64, y: f64, ox: f64, oy: f64, dx: f64, dy: f64) -> i32 {
    if dx.abs() < 1e-12 {
        return if x <= ox {
            i32::from(dy > 0.0)
        } else {
            i32::from(dy < 0.0)
        };
    }
    if dy.abs() < 1e-12 {
        return if y <= oy {
            i32::from(dx < 0.0)
        } else {
            i32::from(dx > 0.0)
        };
    }
    let pdx = x - ox;
    let pdy = y - oy;
    let left = dy * pdx;
    let right = pdy * dx;
    i32::from(right >= left)
}

/// Parametric hit of ray a→b against segment c→d; returns t in [0,1] along a→b.
fn ray_hits_seg(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> Option<f64> {
    let r = DVec2::new(b.x() - a.x(), b.y() - a.y());
    let s = DVec2::new(d.x() - c.x(), d.y() - c.y());
    let denom = r.x() * s.y() - r.y() * s.x();
    if denom.abs() < 1e-9 {
        return None;
    }
    let qp = DVec2::new(c.x() - a.x(), c.y() - a.y());
    let t = (qp.x() * s.y() - qp.y() * s.x()) / denom;
    let u = (qp.x() * r.y() - qp.y() * r.x()) / denom;
    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
        Some(t)
    } else {
        None
    }
}

/// Spawn blood on a fleshy hit, else a bullet puff (`P_SpawnBlood` / `P_SpawnPuff`).
fn spawn_hitscan_impact(
    world: &mut World,
    hit: Option<ActorId>,
    pos: DVec3,
    source: ActorId,
    damage: i32,
) {
    let want_blood = hit.is_some_and(|id| victim_bleeds(world, id));
    if want_blood {
        if let Some(id) = hit {
            spawn_blood(world, id, pos, Some(source), damage);
        }
    } else {
        spawn_puff(world, pos);
    }
}

/// Whether `victim` should spray blood when struck.
pub(crate) fn victim_bleeds(world: &World, victim: ActorId) -> bool {
    world.actors.get(victim).is_some_and(|a| {
        let flags = a.flags | world.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
        (flags & crate::class::FLAG_NOBLOOD) == 0
    })
}

/// Doom `P_SubRandom()<<10` → map units (`/ FRACUNIT`).
fn puff_blood_z_jitter(world: &mut World) -> f64 {
    let r1 = i32::from(world.p_random("pr_spawnpuff"));
    let r2 = i32::from(world.p_random("pr_spawnpuff"));
    f64::from(r1 - r2) * (1024.0 / 65536.0)
}

/// Doom `tics -= P_Random()&3` (floor at 1).
fn jitter_mobj_tics(world: &mut World, id: ActorId) {
    let trim = i32::from(world.p_random("pr_spawnpuff") & 3);
    if let Some(a) = world.actors.get_mut(id) {
        a.tics = (a.tics - trim).max(1);
    }
}

/// Spawn a `Blood` (or `BloodType`) hunk at `pos`.
pub(crate) fn spawn_blood(
    world: &mut World,
    victim: ActorId,
    pos: DVec3,
    source: Option<ActorId>,
    damage: i32,
) {
    let class_name = world
        .actors
        .get(victim)
        .and_then(|a| world.classes.get(a.class))
        .and_then(|c| c.blood_type.first().cloned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Blood".into());
    let Some(class) = world.classes.find_name(&class_name) else {
        return;
    };
    let pos = DVec3::new(pos.x(), pos.y(), pos.z() + puff_blood_z_jitter(world));
    let Ok(id) = world.spawn_at(class, pos, Angle::ZERO) else {
        return;
    };
    let spawn_state = world.actors.get(id).and_then(|a| a.state);
    let blood_state = match damage {
        9..=12 => spawn_state
            .and_then(|s| world.states.get(s))
            .and_then(|s| s.next),
        d if d < 9 => spawn_state
            .and_then(|s| world.states.get(s))
            .and_then(|s| s.next)
            .and_then(|s1| world.states.get(s1).and_then(|s| s.next).or(Some(s1))),
        _ => spawn_state,
    };
    if let Some(a) = world.actors.get_mut(id) {
        a.vel = DVec3::new(0.0, 0.0, 2.0);
        if let Some(st) = blood_state {
            a.state = Some(st);
        }
    }
    jitter_mobj_tics(world, id);
    // Render-only blood spray cue (SH10 / docs/29 J1).
    let dir = source
        .and_then(|s| world.actors.get(s).map(|a| a.pos))
        .map(|sp| {
            let d = DVec3::new(pos.x() - sp.x(), pos.y() - sp.y(), pos.z() - sp.z());
            let len = (d.x() * d.x() + d.y() * d.y() + d.z() * d.z())
                .sqrt()
                .max(1e-3);
            DVec3::new(d.x() / len, d.y() / len, d.z() / len)
        })
        .unwrap_or(DVec3::new(0.0, 0.0, 1.0));
    let color = world
        .actors
        .get(victim)
        .and_then(|a| world.classes.get(a.class))
        .and_then(|c| c.blood_color)
        .map(|rgb| crate::fx_events::Rgb8 {
            r: rgb[0],
            g: rgb[1],
            b: rgb[2],
        })
        .unwrap_or_else(crate::fx_events::Rgb8::blood_red);
    world.push_fx(crate::fx_events::FxEvent::BloodHit {
        victim,
        pos,
        dir,
        damage,
        color,
    });
}

/// Spawn a `BulletPuff` at `pos`.
pub(crate) fn spawn_puff(world: &mut World, pos: DVec3) {
    let class = world
        .puff_override
        .clone()
        .unwrap_or_else(|| "BulletPuff".to_owned());
    spawn_puff_class(world, pos, &class);
}

fn spawn_puff_class(world: &mut World, pos: DVec3, class_name: &str) {
    let Some(class) = world.classes.find_name(class_name) else {
        return;
    };
    let pos = DVec3::new(pos.x(), pos.y(), pos.z() + puff_blood_z_jitter(world));
    let Ok(id) = world.spawn_at(class, pos, Angle::ZERO) else {
        return;
    };
    if let Some(a) = world.actors.get_mut(id) {
        a.vel = DVec3::new(0.0, 0.0, 1.0);
    }
    jitter_mobj_tics(world, id);
}

/// Wall hit → DECALDEF generator for the firing weapon (else `BulletChip`).
fn spawn_wall_bullet_decal(world: &mut World, source: ActorId, pos: DVec3, line: Option<u32>) {
    let generator = hitscan_decal_generator_name(world, source);
    world.spawn_generator_decal(&generator, pos.x(), pos.y(), pos.z(), line);
    if world.decal_events.is_empty() {
        // Hosts map a `BulletChip` without CHIP* lumps to an IWAD puff frame.
        world.spawn_decal("BulletChip", pos.x(), pos.y(), pos.z(), line);
    }
}

/// Non-damaging hitscan trace (`docs/28` BE7).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewTrace {
    /// [`room_script_abi::trace_kind`].
    pub kind: u8,
    /// Actor struck, if any.
    pub actor: Option<ActorId>,
    /// Linedef index when `kind == LINE`.
    pub line_index: u32,
    /// Sector index when `kind == SECTOR`.
    pub sector_index: u32,
    /// Impact position.
    pub pos: DVec3,
    /// Travel distance (map units).
    pub distance: f64,
}

/// Trace from `source` along its yaw/pitch without applying damage or FX.
#[must_use]
pub fn view_trace(world: &World, source: ActorId, range: f64) -> ViewTrace {
    use room_script_abi::trace_kind;
    let Some(a) = world.actors.get(source) else {
        return ViewTrace {
            kind: trace_kind::MISS as u8,
            actor: None,
            line_index: 0,
            sector_index: 0,
            pos: DVec3::ZERO,
            distance: 0.0,
        };
    };
    let pitch = a.pitch;
    if pitch.degrees().abs() > 80.0 {
        return view_trace_pitch_ray(world, source, a.angle, range, pitch);
    }
    let aim_slope = dmath::tan(pitch.radians());
    view_trace_with_slope(world, source, a.angle, range, aim_slope)
}

fn view_trace_pitch_ray(
    world: &World,
    source: ActorId,
    angle: Angle,
    range: f64,
    pitch: Angle,
) -> ViewTrace {
    use room_script_abi::trace_kind;
    let Some((origin, _)) = shoot_origin(world, source) else {
        return ViewTrace {
            kind: trace_kind::MISS as u8,
            actor: None,
            line_index: 0,
            sector_index: 0,
            pos: DVec3::ZERO,
            distance: 0.0,
        };
    };
    let rad = angle.radians();
    let pr = pitch.radians();
    let cp = dmath::cos(pr);
    let sp = dmath::sin(pr);
    let dir = DVec3::new(dmath::cos(rad) * cp, dmath::sin(rad) * cp, sp);
    let horiz = dmath::sqrt(dir.x() * dir.x() + dir.y() * dir.y());
    if horiz > 1e-12 {
        let ang = Angle::from_radians(dmath::atan2(dir.y(), dir.x()));
        let aim_slope = dir.z() / horiz;
        return view_trace_with_slope(world, source, ang, range, aim_slope);
    }
    ViewTrace {
        kind: trace_kind::MISS as u8,
        actor: None,
        line_index: 0,
        sector_index: 0,
        pos: DVec3::new(
            origin.x() + dir.x() * range,
            origin.y() + dir.y() * range,
            origin.z() + dir.z() * range,
        ),
        distance: range,
    }
}

fn view_trace_with_slope(
    world: &World,
    source: ActorId,
    angle: Angle,
    range: f64,
    aim_slope: f64,
) -> ViewTrace {
    use room_script_abi::trace_kind;
    let Some((origin, _)) = shoot_origin(world, source) else {
        return ViewTrace {
            kind: trace_kind::MISS as u8,
            actor: None,
            line_index: 0,
            sector_index: 0,
            pos: DVec3::ZERO,
            distance: 0.0,
        };
    };
    let (tdx, tdy) = finesine_dir(angle, range);
    let dir_x = tdx / range;
    let dir_y = tdy / range;

    let mut best: Option<(ActorId, f64, DVec3)> = None;
    let path_blocks = path_traverse_blocks(
        &world.map.blockmap,
        origin.x(),
        origin.y(),
        origin.x() + tdx,
        origin.y() + tdy,
    );
    let path_set: std::collections::BTreeSet<(i32, i32)> = path_blocks.into_iter().collect();
    let ids: Vec<ActorId> = world.thinkers.default.clone();
    for id in ids {
        if id == source {
            continue;
        }
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.destroyed || (a.flags & FLAG_SHOOTABLE) == 0 {
            continue;
        }
        if !path_set.is_empty() {
            let (bx, by) = block_coords(&world.map.blockmap, a.pos.x(), a.pos.y());
            if !path_set.contains(&(i32::from(bx), i32::from(by))) {
                continue;
            }
        }
        let z0 = a.pos.z();
        let z1 = z0 + a.height;
        let Some(frac) = thing_intercept_frac(
            origin.x(),
            origin.y(),
            tdx,
            tdy,
            a.pos.x(),
            a.pos.y(),
            a.radius,
        ) else {
            continue;
        };
        let t = frac * range;
        if t <= 1e-6 || t > range {
            continue;
        }
        let thing_top = (z1 - origin.z()) / t;
        let thing_bot = (z0 - origin.z()) / t;
        if thing_top < aim_slope || thing_bot > aim_slope {
            continue;
        }
        let pos = DVec3::new(
            origin.x() + dir_x * t,
            origin.y() + dir_y * t,
            origin.z() + aim_slope * t,
        );
        if best.as_ref().is_none_or(|(_, d, _)| t < *d) {
            best = Some((id, t, pos));
        }
    }

    let wall = nearest_shoot_wall(world, origin, dir_x, dir_y, aim_slope, range);
    let plane = nearest_shoot_plane(world, origin, dir_x, dir_y, aim_slope, range);
    let thing_dist = best.as_ref().map(|(_, d, _)| *d);
    let wall_dist = wall.as_ref().map(|(d, _, _)| *d);
    let plane_dist = plane.as_ref().map(|(d, _, _, _)| *d);
    let nearest = [thing_dist, wall_dist, plane_dist]
        .into_iter()
        .flatten()
        .fold(f64::INFINITY, f64::min);
    if thing_dist == Some(nearest) {
        let (id, dist, pos) = best.unwrap();
        return ViewTrace {
            kind: trace_kind::ACTOR as u8,
            actor: Some(id),
            line_index: 0,
            sector_index: 0,
            pos,
            distance: dist,
        };
    }
    if wall_dist == Some(nearest) {
        let (wd, wpos, lid) = wall.unwrap();
        return ViewTrace {
            kind: trace_kind::LINE as u8,
            actor: None,
            line_index: lid.raw(),
            sector_index: 0,
            pos: wpos,
            distance: wd,
        };
    }
    if let Some((pd, ppos, sid, _part)) = plane {
        return ViewTrace {
            kind: trace_kind::SECTOR as u8,
            actor: None,
            line_index: 0,
            sector_index: sid.raw(),
            pos: ppos,
            distance: pd,
        };
    }
    ViewTrace {
        kind: trace_kind::MISS as u8,
        actor: None,
        line_index: 0,
        sector_index: 0,
        pos: DVec3::new(
            origin.x() + dir_x * range,
            origin.y() + dir_y * range,
            origin.z() + aim_slope * range,
        ),
        distance: range,
    }
}

/// DECALDEF `generator` key: player's ready weapon, else the source actor class.
fn hitscan_decal_generator_name(world: &World, source: ActorId) -> String {
    for p in world.players.iter().flatten() {
        if p.mo == Some(source) {
            if let Some(wpn) = p.ready_weapon {
                if let Some(c) = world.classes.get(wpn) {
                    return c.name.clone();
                }
            }
            break;
        }
    }
    world
        .actors
        .get(source)
        .and_then(|a| world.classes.get(a.class))
        .map(|c| c.name.clone())
        .unwrap_or_else(|| "BulletPuff".into())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::{ClassRegistry, StateTable, World};
    use room_decorate::parse_decorate;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};
    use std::sync::Arc;

    #[test]
    fn hitscan_damages_zombie_ahead() {
        let ir = parse_decorate(
            r#"
actor ZombieMan : Actor 3004
{
  Health 20
  Radius 20
  Height 56
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop  Death: POSS H -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let zclass = classes.find_doomednum(3004).unwrap();
        let zid = w
            .spawn_at(
                zclass,
                DVec3::new(200.0, 64.0, 0.0),
                Angle::from_degrees(0.0),
            )
            .unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        // Face +X toward zombie.
        if let Some(a) = w.actors.get_mut(mo) {
            a.angle = Angle::from_degrees(0.0);
        }
        let hit = line_attack(&mut w, mo, Angle::from_degrees(0.0), 1024.0, 20);
        assert_eq!(hit.actor, Some(zid));
        assert!(w.actors.get(zid).unwrap().health <= 0);
    }

    #[test]
    fn vertical_aim_hits_target_below() {
        let ir = parse_decorate(
            r#"
actor ZombieMan : Actor 3004
{
  Health 20
  Radius 20
  Height 56
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 256.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let zclass = classes.find_doomednum(3004).unwrap();
        let zid = w
            .spawn_at(zclass, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.pos = DVec3::new(64.0, 64.0, 120.0);
            a.pitch = Angle::from_degrees(-90.0);
        }
        let hit = line_attack(&mut w, mo, Angle::ZERO, 256.0, 20);
        assert_eq!(hit.actor, Some(zid));
        assert!(w.actors.get(zid).unwrap().health <= 0);
    }

    #[test]
    fn steep_pitch_misses_ground_level_target() {
        let ir = parse_decorate(
            r#"
actor ZombieMan : Actor 3004
{
  Health 20
  Radius 20
  Height 56
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let zclass = classes.find_doomednum(3004).unwrap();
        let zid = w
            .spawn_at(
                zclass,
                DVec3::new(200.0, 64.0, 0.0),
                Angle::from_degrees(0.0),
            )
            .unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.angle = Angle::from_degrees(0.0);
            a.pitch = Angle::from_degrees(80.0); // aim nearly straight up
        }
        let hit = line_attack(&mut w, mo, Angle::from_degrees(0.0), 1024.0, 20);
        assert_eq!(hit.actor, None);
        assert_eq!(w.actors.get(zid).unwrap().health, 20);
    }

    #[test]
    fn bullet_slope_follows_freelook_pitch_without_autoaim() {
        let ir = parse_decorate(
            r#"
actor Looker : Actor 0
{
  Health 100
  Height 56
  ViewHeight 41
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let looker = classes.find_name("Looker").unwrap();
        let id = w
            .spawn_at(looker, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.pitch = Angle::from_degrees(-30.0); // look down
        }
        let slope = bullet_slope(&w, id);
        let expect = dmath::tan((-30.0_f64).to_radians());
        assert!(
            (slope - expect).abs() < 1e-6,
            "freelook slope {slope} != tan(pitch) {expect}"
        );
        // Looking up must aim above the horizon (positive Z slope).
        if let Some(a) = w.actors.get_mut(id) {
            a.pitch = Angle::from_degrees(25.0);
        }
        let up = bullet_slope(&w, id);
        assert!(
            up > 0.3,
            "look-up slope should be clearly positive, got {up}"
        );
    }

    #[test]
    fn autoaim_off_ignores_a_raised_target() {
        let ir = parse_decorate(
            r#"
actor Looker : Actor 0
{
  Health 100
  Height 56
  ViewHeight 41
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
actor Dummy : Actor 0
{
  Health 20
  Radius 20
  Height 56
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 256.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let looker = classes.find_name("Looker").unwrap();
        let dummy = classes.find_name("Dummy").unwrap();
        let id = w
            .spawn_at(looker, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let _ = w
            .spawn_at(dummy, DVec3::new(200.0, 64.0, 80.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.angle = Angle::ZERO;
            a.pitch = Angle::ZERO;
        }
        let aimed = bullet_slope(&w, id);
        w.autoaim = false;
        let free = bullet_slope(&w, id);
        assert!(aimed > 0.15, "autoaim lifts toward the dummy, got {aimed}");
        assert!(free.abs() < 0.05, "autoaim off stays level, got {free}");
    }

    #[test]
    fn pitched_hitscan_hits_elevated_target() {
        let ir = parse_decorate(
            r#"
actor Shooter : Actor 0
{
  Health 100
  Height 56
  ViewHeight 41
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
actor HighDummy : Actor 9931
{
  Health 20
  Radius 20
  Height 56
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 256.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let shooter = classes.find_name("Shooter").unwrap();
        let dummy = classes.find_name("HighDummy").unwrap();
        let sid = w
            .spawn_at(shooter, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        // Target floating well above the horizontal aim line.
        let tid = w
            .spawn_at(dummy, DVec3::new(200.0, 64.0, 80.0), Angle::ZERO)
            .unwrap();
        // Horizontal shot must miss.
        if let Some(a) = w.actors.get_mut(sid) {
            a.pitch = Angle::ZERO;
            a.angle = Angle::ZERO;
        }
        let miss = line_attack(&mut w, sid, Angle::ZERO, 1024.0, 5);
        assert_eq!(miss.actor, None, "level shot should miss elevated target");
        assert_eq!(w.actors.get(tid).unwrap().health, 20);
        // Pitch toward the target (Δz≈80+28mid - 41eye over Δxy≈136 → ~25°).
        if let Some(a) = w.actors.get_mut(sid) {
            a.pitch = Angle::from_degrees(28.0);
        }
        let hit = line_attack(&mut w, sid, Angle::ZERO, 1024.0, 5);
        assert_eq!(
            hit.actor,
            Some(tid),
            "pitched shot must hit elevated target"
        );
    }

    #[test]
    fn line_attack_uses_view_height_and_attack_z_offset() {
        let ir = parse_decorate(
            r#"
actor TallShooter : Actor 0
{
  Health 100
  Height 56
  ViewHeight 41
  AttackZOffset 8
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
actor Dummy : Actor 9930
{
  Health 20
  Radius 20
  Height 56
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let tall = classes.find_name("TallShooter").unwrap();
        let dummy = classes.find_name("Dummy").unwrap();
        assert!((classes.get(tall).unwrap().view_height - 41.0).abs() < 1e-9);
        assert!((classes.get(tall).unwrap().attack_z_offset - 8.0).abs() < 1e-9);
        let shooter = w
            .spawn_at(tall, DVec3::new(64.0, 64.0, 0.0), Angle::from_degrees(0.0))
            .unwrap();
        let _ = w
            .spawn_at(dummy, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let hit = line_attack(&mut w, shooter, Angle::from_degrees(0.0), 1024.0, 1);
        // Origin Z = 0 + ViewHeight 41 + AttackZOffset 8 = 49
        assert!((hit.pos.z() - 49.0).abs() < 1e-6, "z={}", hit.pos.z());
    }

    #[test]
    fn hitscan_hit_spawns_blood() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let blood = classes.find_name("Blood").unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let before = w
            .thinkers
            .default
            .iter()
            .filter(|&&id| w.actors.get(id).is_some_and(|a| a.class == blood))
            .count();
        let zid = w
            .spawn_at(zombie, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.angle = Angle::ZERO;
        }
        let hit = line_attack(&mut w, mo, Angle::ZERO, 1024.0, 5);
        assert_eq!(hit.actor, Some(zid));
        let after = w
            .thinkers
            .default
            .iter()
            .filter(|&&id| w.actors.get(id).is_some_and(|a| a.class == blood))
            .count();
        assert_eq!(after, before + 1);
    }

    #[test]
    fn hitscan_wall_spawns_bullet_puff() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let puff = classes.find_name("BulletPuff").unwrap();
        let before = w
            .thinkers
            .default
            .iter()
            .filter(|&&id| w.actors.get(id).is_some_and(|a| a.class == puff))
            .count();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.angle = Angle::ZERO; // +X into the east wall at x=512
        }
        let hit = line_attack(&mut w, mo, Angle::ZERO, 2048.0, 5);
        assert_eq!(hit.actor, None);
        assert!(
            hit.distance < 512.0,
            "should stop at wall, dist={}",
            hit.distance
        );
        let after = w
            .thinkers
            .default
            .iter()
            .filter(|&&id| w.actors.get(id).is_some_and(|a| a.class == puff))
            .count();
        assert_eq!(after, before + 1);
        // Vanilla: the puff is 4 units short of the wall, inside the room.
        let px = w
            .thinkers
            .default
            .iter()
            .filter_map(|&id| w.actors.get(id).filter(|a| a.class == puff))
            .map(|a| a.pos.x())
            .next()
            .unwrap();
        assert!(
            (px - (hit.pos.x() - 4.0)).abs() < 0.01,
            "puff x={px} wall={}",
            hit.pos.x()
        );
    }

    #[test]
    fn hitscan_open_miss_spawns_no_puff() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let puff = classes.find_name("BulletPuff").unwrap();
        let before = w
            .thinkers
            .default
            .iter()
            .filter(|&&id| w.actors.get(id).is_some_and(|a| a.class == puff))
            .count();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        // Short range into open air — no wall intercept, no puff.
        let hit = line_attack(&mut w, mo, Angle::ZERO, 64.0, 5);
        assert_eq!(hit.actor, None);
        let after = w
            .thinkers
            .default
            .iter()
            .filter(|&&id| w.actors.get(id).is_some_and(|a| a.class == puff))
            .count();
        assert_eq!(after, before);
    }

    #[test]
    fn blockhitscan_stops_shot_through_opening() {
        let ir = parse_decorate(
            r#"
actor ZombieMan : Actor 3004
{
  Health 20
  Radius 20
  Height 56
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room("b", rect(128.0, 0.0, 256.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        for line in &mut map.lines {
            if line.front_sector.is_some() && line.back_sector.is_some() {
                line.block_hitscan = true;
            }
        }
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let zclass = classes.find_doomednum(3004).unwrap();
        let zid = w
            .spawn_at(zclass, DVec3::new(192.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let hit = line_attack(&mut w, mo, Angle::ZERO, 1024.0, 20);
        assert_eq!(hit.actor, None, "blockhitscan should stop the shot");
        assert_eq!(w.actors.get(zid).unwrap().health, 20);
    }

    #[test]
    fn hitscan_wall_blocks_zombie_behind() {
        let ir = parse_decorate(
            r#"
actor ZombieMan : Actor 3004
{
  Health 20
  Radius 20
  Height 56
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        // Zombie past the one-sided east wall of a single room.
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let zclass = classes.find_doomednum(3004).unwrap();
        let zid = w
            .spawn_at(zclass, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let hit = line_attack(&mut w, mo, Angle::ZERO, 1024.0, 20);
        assert_eq!(hit.actor, None, "wall should stop the shot");
        assert_eq!(w.actors.get(zid).unwrap().health, 20);
    }

    #[test]
    fn hitscan_noblood_spawns_puff() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let puff = classes.find_name("BulletPuff").unwrap();
        let blood = classes.find_name("Blood").unwrap();
        let barrel = classes.find_name("ExplosiveBarrel").unwrap();
        let blood0 = w
            .thinkers
            .default
            .iter()
            .filter(|&&id| w.actors.get(id).is_some_and(|a| a.class == blood))
            .count();
        let puff0 = w
            .thinkers
            .default
            .iter()
            .filter(|&&id| w.actors.get(id).is_some_and(|a| a.class == puff))
            .count();
        let bid = w
            .spawn_at(barrel, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.angle = Angle::ZERO;
        }
        let hit = line_attack(&mut w, mo, Angle::ZERO, 1024.0, 5);
        assert_eq!(hit.actor, Some(bid));
        let blood1 = w
            .thinkers
            .default
            .iter()
            .filter(|&&id| w.actors.get(id).is_some_and(|a| a.class == blood))
            .count();
        let puff1 = w
            .thinkers
            .default
            .iter()
            .filter(|&&id| w.actors.get(id).is_some_and(|a| a.class == puff))
            .count();
        assert_eq!(blood1, blood0);
        assert_eq!(puff1, puff0 + 1);
    }
}
