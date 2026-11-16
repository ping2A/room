//! Render a generated map to a PPM image (floor heights, walls, things).
//!
//! `cargo run -p room-mapgen --example preview -- <seed> <players> <theme> out.ppm`

use room_mapgen::{Options, Theme, generate};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let players = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(8);
    let theme = match args.get(3).map(String::as_str) {
        Some("base") => Theme::Base,
        Some("hell") => Theme::Hell,
        Some("castle") => Theme::Castle,
        Some("ruins") => Theme::Ruins,
        Some("mixed") => Theme::Mixed,
        _ => Theme::Tech,
    };
    let out = args.get(4).cloned().unwrap_or_else(|| "preview.ppm".into());
    let opts = Options {
        seed,
        players,
        theme,
        verticality: 2,
        ..Options::default()
    };
    let g = match generate(&opts) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    eprintln!("{}", serde_json_report(&g.report));
    let Ok(mut geom) = g.map.to_geometry() else {
        eprintln!("geometry failed");
        std::process::exit(1);
    };
    room_nodebuild::ensure_nodes(&mut geom);
    let (mut minx, mut miny, mut maxx, mut maxy) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for v in &g.map.vertices {
        minx = minx.min(i32::from(v.x));
        miny = miny.min(i32::from(v.y));
        maxx = maxx.max(i32::from(v.x));
        maxy = maxy.max(i32::from(v.y));
    }
    let scale = 4;
    let (w, h) = (
        ((maxx - minx) / scale + 1) as usize,
        ((maxy - miny) / scale + 1) as usize,
    );
    let mut img = vec![[18u8, 18, 22]; w * h];
    for py in 0..h {
        for px in 0..w {
            let x = f64::from(minx) + (px as f64 + 0.5) * f64::from(scale);
            let y = f64::from(maxy) - (py as f64 + 0.5) * f64::from(scale);
            // Inside the map: odd number of one-sided walls to the right.
            let mut inside = false;
            for l in g.map.linedefs.iter().filter(|l| l.side_back == 0xFFFF) {
                let (a, b) = (
                    &g.map.vertices[l.v1 as usize],
                    &g.map.vertices[l.v2 as usize],
                );
                let (ax, ay, bx, by) = (
                    f64::from(a.x),
                    f64::from(a.y),
                    f64::from(b.x),
                    f64::from(b.y),
                );
                if (ay > y) != (by > y) && x < ax + (y - ay) * (bx - ax) / (by - ay) {
                    inside = !inside;
                }
            }
            if !inside {
                continue;
            }
            if let Some(s) = room_nodebuild::point_in_sector(&geom, x, y) {
                let sec = &g.map.sectors[s.raw() as usize];
                let f = f32::from(sec.floor_height);
                let t = ((f + 96.0) / 288.0).clamp(0.0, 1.0);
                let mut c = [
                    (60.0 + 150.0 * t) as u8,
                    (70.0 + 120.0 * t) as u8,
                    (90.0 + 60.0 * (1.0 - t)) as u8,
                ];
                let pic = sec.floor_pic.as_str();
                if pic.starts_with("NUKAGE") || pic.starts_with("SLIME") {
                    c = [60, 200, 60];
                } else if pic.starts_with("LAVA") {
                    c = [230, 90, 20];
                } else if pic.starts_with("FWATER") {
                    c = [40, 90, 220];
                } else if pic.starts_with("GATE") {
                    c = [230, 60, 230];
                } else if pic.starts_with("CRATOP") {
                    c = [150, 110, 60];
                }
                if sec.ceiling_pic == "F_SKY1" {
                    c = [
                        c[0].saturating_add(0),
                        c[1].saturating_add(20),
                        c[2].saturating_add(40),
                    ];
                }
                if sec.ceiling_height - sec.floor_height < 56 {
                    c = [c[0] / 2, c[1] / 2, c[2] / 2];
                }
                img[py * w + px] = c;
            }
        }
    }
    let mut plot = |x: i32, y: i32, c: [u8; 3]| {
        let px = (x - minx) / scale;
        let py = (maxy - y) / scale;
        if px >= 0 && py >= 0 && (px as usize) < w && (py as usize) < h {
            img[py as usize * w + px as usize] = c;
        }
    };
    for l in &g.map.linedefs {
        let (a, b) = (
            &g.map.vertices[l.v1 as usize],
            &g.map.vertices[l.v2 as usize],
        );
        let c = if l.special == 97 {
            [255, 0, 255]
        } else if l.special == 11 {
            [255, 0, 0]
        } else if l.side_back == 0xFFFF {
            [250, 250, 250]
        } else {
            [120, 120, 130]
        };
        let n = ((i32::from(b.x) - i32::from(a.x))
            .abs()
            .max((i32::from(b.y) - i32::from(a.y)).abs())
            / 2)
        .max(1);
        for k in 0..=n {
            let x = i32::from(a.x) + (i32::from(b.x) - i32::from(a.x)) * k / n;
            let y = i32::from(a.y) + (i32::from(b.y) - i32::from(a.y)) * k / n;
            plot(x, y, c);
        }
    }
    for t in &g.map.things {
        let c = match t.type_id {
            11 => [255, 220, 0],
            1..=4 => [0, 255, 0],
            2001..=2006 | 82 => [255, 40, 40],
            14 => [255, 0, 255],
            2011..=2019 | 83 | 8 => [0, 200, 255],
            _ => [200, 200, 200],
        };
        for dy in -6..=6 {
            for dx in -6..=6 {
                if dx * dx + dy * dy <= 36 {
                    plot(i32::from(t.x) + dx, i32::from(t.y) + dy, c);
                }
            }
        }
    }
    let mut data = format!("P6 {w} {h} 255\n").into_bytes();
    for p in img {
        data.extend_from_slice(&p);
    }
    if let Err(e) = std::fs::write(&out, data) {
        eprintln!("{e}");
    }
}

fn serde_json_report(r: &room_mapgen::Report) -> String {
    serde_json::to_string(r).unwrap_or_default()
}
