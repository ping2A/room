//! CLI tools: wad ls/extract, map dump, nodebuild, demo verify, render-to-png.

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use room_vfs::{Namespace, Vfs, VfsBuilder};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "room", version, about = "Room engine tools")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Inspect WAD/PK3/directory archives
    Wad {
        #[command(subcommand)]
        command: WadCmd,
    },
    /// Dump compiled definition registries
    Defs {
        #[command(subcommand)]
        command: DefsCmd,
    },
    /// Map load / nodebuild tools
    Map {
        #[command(subcommand)]
        command: MapCmd,
    },
    /// Room demo (`.fdem` / `.fdbn`) tools
    Demo {
        #[command(subcommand)]
        command: DemoCmd,
    },
    /// Duke Nukem 3D tools (native runtime track, docs/25)
    Duke {
        #[command(subcommand)]
        command: DukeCmd,
    },
    /// Face-swap tools (`docs/26-face-swap.md`)
    Faceswap {
        #[command(subcommand)]
        command: FaceswapCmd,
    },
}

#[derive(Subcommand)]
enum FaceswapCmd {
    /// Bake a face onto every sprite lump matching `--prefix` (writes a PK3)
    Bake {
        /// IWAD / PWAD / PK3 with sprites + PLAYPAL
        #[arg(long)]
        iwad: PathBuf,
        /// Face image (PNG RGBA)
        #[arg(long)]
        face: PathBuf,
        /// Manual eyes `x1,y1,x2,y2` (required; detection is F3)
        #[arg(long)]
        eyes: String,
        /// Chin `x,y` (default: midpoint below eyes)
        #[arg(long)]
        chin: Option<String>,
        /// Sprite prefix (e.g. TROO, POSS)
        #[arg(long)]
        prefix: String,
        /// Optional HEADANCHORS text file
        #[arg(long)]
        anchors: Option<PathBuf>,
        /// Output PK3 path
        #[arg(short, long, default_value = "faces.pk3")]
        output: PathBuf,
    },
    /// Estimate anchors for a sprite prefix and print HEADANCHORS text
    AnchorsEstimate {
        #[arg(long)]
        iwad: PathBuf,
        #[arg(long)]
        prefix: String,
    },
}

#[derive(Subcommand)]
enum DukeCmd {
    /// Diff two per-tic sync traces (`docs/duke-trace-format.md`)
    TraceDiff {
        /// Reference / left trace
        a: PathBuf,
        /// Candidate / right trace
        b: PathBuf,
    },
    /// Print SHA-256 of a trace file (for the committed manifest)
    TraceHash {
        /// Trace path
        path: PathBuf,
    },
    /// Soft-check `testdata/duke-trace-manifest.toml` (ok if missing; fail only on bad hash)
    TraceCheckManifest {
        /// Manifest path
        #[arg(default_value = "testdata/duke-trace-manifest.toml")]
        path: PathBuf,
    },
    /// Extract `DEMO1`–`DEMO3.DMO` from a `DUKE3D.GRP` (for out-of-tree reference playback)
    ExtractDemos {
        /// Path to `DUKE3D.GRP`
        #[arg(default_value = "testdata/duke/duke3d.grp")]
        grp: PathBuf,
        /// Output directory
        #[arg(short, long, default_value = "testdata/duke/demos")]
        out: PathBuf,
    },
    /// Soft FP preview of a Build map (PGM). Free-fly steps optional.
    SoftView {
        /// Path to `DUKE3D.GRP` (or a raw `.MAP` when `--map-file` is set)
        #[arg(default_value = "testdata/duke/duke3d.grp")]
        grp: PathBuf,
        /// Map name inside the GRP (e.g. `E1L1.MAP`)
        #[arg(long, default_value = "E1L1.MAP")]
        map: String,
        /// Load this `.MAP` file instead of reading from the GRP
        #[arg(long)]
        map_file: Option<PathBuf>,
        /// Output PGM path
        #[arg(short, long, default_value = "soft-view.pgm")]
        output: PathBuf,
        /// Frame width
        #[arg(long, default_value_t = 320)]
        width: u32,
        /// Frame height
        #[arg(long, default_value_t = 200)]
        height: u32,
        /// Forward map units before capture (noclip FreeFly; `--forward=-64` back)
        #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
        forward: i32,
        /// Turn (Build angle units) before capture (`--turn=-64` left)
        #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
        turn: i16,
        /// Look (horiz delta) before capture (use `--look=-10` for negatives)
        #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
        look: i16,
        /// Vertical move in Build Z units (up is negative Z; `--up=-256`)
        #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
        up: i32,
        /// Strafe right (+) / left (−); use `--strafe=-50` for left
        #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
        strafe: i32,
    },
}

#[derive(Subcommand)]
enum DemoCmd {
    /// Print `.fdem` / `.fdbn` header / tic summary
    Info {
        /// Demo path (`.fdem` or `.fdbn`)
        path: PathBuf,
    },
    /// Replay a demo against a map archive and check CSUM chunks
    Verify {
        /// Demo path (`.fdem` or `.fdbn`)
        path: PathBuf,
        /// Archive containing the map (WAD/PK3/dir)
        #[arg(long)]
        wad: PathBuf,
        /// Map lump name (single `.fdem` only; defaults to FDHD map field)
        #[arg(long)]
        map: Option<String>,
        /// DECORATE actors file (defaults to `testdata/actors/doom_basic.txt` if present)
        #[arg(long)]
        actors: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum WadCmd {
    /// List lumps in load order
    Ls {
        /// Archive path (WAD, PK3, or directory)
        path: PathBuf,
    },
    /// Show archive summary
    Info {
        /// Archive path
        path: PathBuf,
    },
    /// Extract one lump by short name or full path
    Extract {
        /// Archive path
        path: PathBuf,
        /// Short name or full path
        lump: String,
        /// Output file
        #[arg(short, long)]
        output: PathBuf,
    },
}

#[derive(Subcommand)]
enum MapCmd {
    /// Load a map and print geometry summary
    Info {
        /// Archive path
        path: PathBuf,
        /// Map lump name (e.g. MAP01)
        map: String,
        /// Rebuild nodes with the sector-fan builder
        #[arg(long)]
        rebuild_nodes: bool,
    },
    /// Write a 2D SVG of linedefs (and player starts)
    Render2d {
        /// Archive path
        path: PathBuf,
        /// Map lump name (e.g. MAP01)
        map: String,
        /// Output SVG path
        #[arg(short, long)]
        output: PathBuf,
        /// World-units to SVG pixels
        #[arg(long, default_value_t = 0.25)]
        scale: f64,
        /// Rebuild nodes before dump (no effect on SVG lines)
        #[arg(long)]
        rebuild_nodes: bool,
    },
    /// Rebuild classic game nodes with the sector-fan builder
    Nodebuild {
        /// Archive path
        path: PathBuf,
        /// Map lump name (e.g. MAP01)
        map: String,
    },
}

#[derive(Subcommand)]
enum DefsCmd {
    /// Parse and dump a registry from mounted archives
    Dump {
        /// Archives to mount (load order)
        paths: Vec<PathBuf>,
        /// Registry: all | mapinfo | sndinfo | gameinfo | language | textures | animdefs | cvarinfo | fontdefs
        #[arg(long, default_value = "mapinfo")]
        registry: String,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Faceswap { command } => match command {
            FaceswapCmd::Bake {
                iwad,
                face,
                eyes,
                chin,
                prefix,
                anchors,
                output,
            } => faceswap_bake(iwad, face, eyes, chin, prefix, anchors, output)?,
            FaceswapCmd::AnchorsEstimate { iwad, prefix } => {
                faceswap_anchors_estimate(iwad, prefix)?
            }
        },
        Commands::Duke { command } => match command {
            DukeCmd::TraceDiff { a, b } => {
                let left = room_duke::load_trace(&a).map_err(anyhow::Error::msg)?;
                let right = room_duke::load_trace(&b).map_err(anyhow::Error::msg)?;
                match room_duke::diff_traces(&left, &right) {
                    None => {
                        println!("ok: traces match ({} tics)", left.tics.len());
                    }
                    Some(diff) => {
                        println!("{diff}");
                        std::process::exit(1);
                    }
                }
            }
            DukeCmd::TraceHash { path } => {
                let bytes =
                    std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
                println!("{}", room_duke::sha256_hex(&bytes));
            }
            DukeCmd::TraceCheckManifest { path } => {
                duke_trace_check_manifest(&path)?;
            }
            DukeCmd::ExtractDemos { grp, out } => {
                duke_extract_demos(&grp, &out)?;
            }
            DukeCmd::SoftView {
                grp,
                map,
                map_file,
                output,
                width,
                height,
                forward,
                turn,
                look,
                up,
                strafe,
            } => {
                duke_soft_view(
                    &grp,
                    &map,
                    map_file.as_deref(),
                    &output,
                    width,
                    height,
                    room_build_engine::FreeFlyCmd {
                        forward,
                        turn,
                        look,
                        up,
                        strafe,
                    },
                )?;
            }
        },
        Commands::Wad { command } => match command {
            WadCmd::Ls { path } => {
                let vfs = mount_one(&path)?;
                for (id, info) in vfs.iter() {
                    println!(
                        "{:>6}  {:>8}  {:<10}  {:<8}  {}",
                        id.raw(),
                        info.size,
                        info.namespace.as_str(),
                        info.short_name,
                        info.full_path
                    );
                }
            }
            WadCmd::Info { path } => {
                let vfs = mount_one(&path)?;
                println!("path: {}", path.display());
                println!("containers: {}", vfs.container_count());
                println!("lumps: {}", vfs.len());
                for i in 0..vfs.container_count() {
                    let id = room_vfs::ContainerId(i as u32);
                    if let Some(c) = vfs.container(id) {
                        println!(
                            "  [{}] {:?} {} md5={}",
                            i,
                            c.kind,
                            c.name,
                            hex::encode(c.md5)
                        );
                    }
                }
                for w in vfs.warnings() {
                    println!("warning: {w}");
                }
            }
            WadCmd::Extract { path, lump, output } => {
                let vfs = mount_one(&path)?;
                let id = vfs
                    .find_full(&lump)
                    .or_else(|| vfs.find_any_ns(&lump))
                    .with_context(|| format!("lump not found: {lump}"))?;
                let data = vfs.read(id)?;
                std::fs::write(&output, &data)
                    .with_context(|| format!("write {}", output.display()))?;
                println!("wrote {} bytes to {}", data.len(), output.display());
            }
        },
        Commands::Map { command } => match command {
            MapCmd::Info {
                path,
                map,
                rebuild_nodes,
            } => {
                let vfs = mount_one(&path)?;
                let mut geom =
                    room_map::load_map(&vfs, &map).with_context(|| format!("load map {map}"))?;
                if rebuild_nodes {
                    room_nodebuild::apply_nodes(&mut geom);
                }
                println!("map: {}", geom.name);
                println!("format: {:?}", geom.format);
                println!("vertices: {}", geom.vertices.len());
                println!("linedefs: {}", geom.lines.len());
                println!("sidedefs: {}", geom.sides.len());
                println!("sectors: {}", geom.sectors.len());
                println!("things: {}", geom.things.len());
                println!(
                    "blockmap: {}x{} ({} blocks)",
                    geom.blockmap.columns,
                    geom.blockmap.rows,
                    geom.blockmap.blocks.len()
                );
                match &geom.game_nodes {
                    Some(n) => println!(
                        "nodes: {} segs, {} subsectors, {} bsp nodes",
                        n.segs.len(),
                        n.subsectors.len(),
                        n.nodes.len()
                    ),
                    None => println!("nodes: (none)"),
                }
                match &geom.reject {
                    Some(r) => {
                        println!("reject: {} sectors, {} bytes", r.num_sectors, r.bits.len())
                    }
                    None => println!("reject: (none)"),
                }
                println!("md5: {}", hex::encode(geom.md5));
            }
            MapCmd::Render2d {
                path,
                map,
                output,
                scale,
                rebuild_nodes,
            } => {
                let vfs = mount_one(&path)?;
                let mut geom =
                    room_map::load_map(&vfs, &map).with_context(|| format!("load map {map}"))?;
                if rebuild_nodes {
                    room_nodebuild::apply_nodes(&mut geom);
                }
                let svg = room_map::map_to_svg(&geom, scale);
                std::fs::write(&output, svg.as_bytes())
                    .with_context(|| format!("write {}", output.display()))?;
                println!("wrote {} ({} bytes)", output.display(), svg.len());
            }
            MapCmd::Nodebuild { path, map } => {
                let vfs = mount_one(&path)?;
                let mut geom =
                    room_map::load_map(&vfs, &map).with_context(|| format!("load map {map}"))?;
                room_nodebuild::apply_nodes(&mut geom);
                match &geom.game_nodes {
                    Some(n) => println!(
                        "{}: rebuilt — {} segs, {} subsectors, {} bsp nodes",
                        geom.name,
                        n.segs.len(),
                        n.subsectors.len(),
                        n.nodes.len()
                    ),
                    None => println!("{}: rebuild produced no nodes", geom.name),
                }
            }
        },
        Commands::Demo { command } => match command {
            DemoCmd::Info { path } => {
                let bytes =
                    std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
                if is_fdbn(&bytes) {
                    let bundle = room_game::DemoBundle::from_fdbn(&bytes)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    println!("path: {}", path.display());
                    println!("format: fdbn");
                    println!("segments: {}", bundle.demos.len());
                    println!("fingerprint: {:016x}", bundle.fingerprint());
                    for (i, demo) in bundle.demos.iter().enumerate() {
                        let csums = demo.tics.iter().filter(|t| t.checksum.is_some()).count();
                        println!(
                            "  [{i}] map={} skill={} seed={} tics={} csums={}",
                            demo.map,
                            demo.skill,
                            demo.rng_seed,
                            demo.tics.len(),
                            csums
                        );
                    }
                } else {
                    let demo = room_game::DemoRecorder::from_fdem(&bytes)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    println!("path: {}", path.display());
                    println!("format: fdem");
                    println!("map: {}", demo.map);
                    println!("skill: {}", demo.skill);
                    println!("rng_seed: {}", demo.rng_seed);
                    println!("players: {}", demo.player_count);
                    println!("consoleplayer: {}", demo.consoleplayer);
                    println!("tics: {}", demo.tics.len());
                    let csums = demo.tics.iter().filter(|t| t.checksum.is_some()).count();
                    println!("checksum samples: {csums}");
                    println!("fingerprint: {:016x}", demo.fingerprint());
                }
            }
            DemoCmd::Verify {
                path,
                wad,
                map,
                actors,
            } => {
                let bytes =
                    std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
                let (classes, states) = load_demo_actors(actors.as_ref())?;
                let vfs = mount_one(&wad)?;
                if is_fdbn(&bytes) {
                    let bundle = room_game::DemoBundle::from_fdbn(&bytes)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    for demo in &bundle.demos {
                        verify_one_demo(demo, &vfs, None, &classes, &states)?;
                    }
                    println!(
                        "ok: {} segments verified against {}",
                        bundle.demos.len(),
                        wad.display()
                    );
                } else {
                    let demo = room_game::DemoRecorder::from_fdem(&bytes)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    verify_one_demo(&demo, &vfs, map.as_deref(), &classes, &states)?;
                    println!(
                        "ok: {} tics verified against {} / {}",
                        demo.tics.len(),
                        wad.display(),
                        map.as_deref().unwrap_or(&demo.map)
                    );
                }
            }
        },
        Commands::Defs { command } => match command {
            DefsCmd::Dump { paths, registry } => {
                if paths.is_empty() {
                    bail!("provide at least one archive path");
                }
                let vfs = mount_many(&paths)?;
                let reg = registry.to_ascii_lowercase();
                match reg.as_str() {
                    "mapinfo" => {
                        let text = read_text_lump(&vfs, "MAPINFO")
                            .or_else(|| read_text_lump(&vfs, "ZMAPINFO"))
                            .context("no MAPINFO/ZMAPINFO lump")?;
                        let parsed = room_defs::parse_mapinfo(&text)?;
                        print!("{}", room_defs::dump_debug(&parsed));
                    }
                    "sndinfo" => {
                        let text = read_text_lump(&vfs, "SNDINFO").context("no SNDINFO lump")?;
                        let parsed = room_defs::parse_sndinfo(&text)?;
                        print!("{}", room_defs::dump_debug(&parsed));
                    }
                    "gameinfo" => {
                        let text = read_text_lump(&vfs, "GAMEINFO").context("no GAMEINFO lump")?;
                        let parsed = room_defs::parse_gameinfo(&text)?;
                        print!("{}", room_defs::dump_debug(&parsed));
                    }
                    "language" => {
                        let text = read_text_lump(&vfs, "LANGUAGE").context("no LANGUAGE lump")?;
                        let parsed = room_defs::parse_language(&text)?;
                        print!("{}", room_defs::dump_debug(&parsed));
                    }
                    "textures" => {
                        let text = read_text_lump(&vfs, "TEXTURES").context("no TEXTURES lump")?;
                        let parsed = room_defs::parse_textures(&text)?;
                        print!("{}", room_defs::dump_debug(&parsed));
                    }
                    "animdefs" => {
                        let text = read_text_lump(&vfs, "ANIMDEFS").context("no ANIMDEFS lump")?;
                        let parsed = room_defs::parse_animdefs(&text)?;
                        print!("{}", room_defs::dump_debug(&parsed));
                    }
                    "cvarinfo" => {
                        let text = read_text_lump(&vfs, "CVARINFO").context("no CVARINFO lump")?;
                        let parsed = room_defs::parse_cvarinfo(&text)?;
                        print!("{}", room_defs::dump_debug(&parsed));
                    }
                    "fontdefs" => {
                        let text = read_text_lump(&vfs, "FONTDEFS").context("no FONTDEFS lump")?;
                        let parsed = room_defs::parse_fontdefs(&text)?;
                        print!("{}", room_defs::dump_debug(&parsed));
                    }
                    "all" => {
                        let parsed = room_defs::DefinitionsBuilder::new(&vfs).build()?;
                        print!("{}", room_defs::dump_debug(&parsed));
                    }
                    other => bail!("unknown registry {other:?}"),
                }
            }
        },
    }
    Ok(())
}

fn is_fdbn(bytes: &[u8]) -> bool {
    bytes.starts_with(b"FDBN")
}

fn load_demo_actors(
    actors: Option<&PathBuf>,
) -> Result<(
    std::sync::Arc<room_sim::ClassRegistry>,
    std::sync::Arc<room_sim::StateTable>,
)> {
    let path = actors.cloned().or_else(default_doom_basic_path);
    let Some(path) = path else {
        bail!("no DECORATE actors file (pass --actors or place testdata/actors/doom_basic.txt)");
    };
    let src = std::fs::read_to_string(&path)
        .with_context(|| format!("read actors {}", path.display()))?;
    let ir = room_decorate::parse_decorate(&src)?;
    let classes = std::sync::Arc::new(
        room_sim::ClassRegistry::from_decorate(&ir).map_err(|e| anyhow::anyhow!("{e}"))?,
    );
    let states = std::sync::Arc::new(room_sim::StateTable::from_decorate(&ir, &classes));
    Ok((classes, states))
}

fn default_doom_basic_path() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("testdata/actors/doom_basic.txt"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/actors/doom_basic.txt"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

fn verify_one_demo(
    demo: &room_game::DemoRecorder,
    vfs: &Vfs,
    map_override: Option<&str>,
    classes: &std::sync::Arc<room_sim::ClassRegistry>,
    states: &std::sync::Arc<room_sim::StateTable>,
) -> Result<()> {
    let map_name = map_override.unwrap_or(demo.map.as_str());
    let mut geom =
        room_map::load_map(vfs, map_name).with_context(|| format!("load map {map_name}"))?;
    room_nodebuild::apply_nodes(&mut geom);
    let skill = if demo.skill == 0 { 3 } else { demo.skill };
    let mut world = room_sim::World::from_map_with_skill(
        geom,
        std::sync::Arc::clone(classes),
        std::sync::Arc::clone(states),
        skill,
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    world.rng.set_seed(demo.rng_seed);
    let player = room_game::DemoPlayer { demo };
    let result = player
        .verify(&mut world)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    if let Some(tic) = result.first_mismatch {
        bail!(
            "{}: checksum mismatch at tic {tic} (ok through {})",
            demo.map,
            result.tics_ok
        );
    }
    println!(
        "  ok {} — {} tics (seed {})",
        demo.map, result.tics_ok, demo.rng_seed
    );
    Ok(())
}

fn mount_one(path: &PathBuf) -> Result<Vfs> {
    if !path.exists() {
        bail!("path does not exist: {}", path.display());
    }
    Ok(VfsBuilder::new().mount_path(path)?.build()?)
}

fn mount_many(paths: &[PathBuf]) -> Result<Vfs> {
    let mut b = VfsBuilder::new();
    for p in paths {
        if !p.exists() {
            bail!("path does not exist: {}", p.display());
        }
        b = b.mount_path(p)?;
    }
    Ok(b.build()?)
}

fn duke_soft_view(
    grp_path: &std::path::Path,
    map_name: &str,
    map_file: Option<&std::path::Path>,
    output: &std::path::Path,
    width: u32,
    height: u32,
    cmd: room_build_engine::FreeFlyCmd,
) -> Result<()> {
    let (map_bytes, tiles) = if let Some(p) = map_file {
        let map_bytes = std::fs::read(p).with_context(|| format!("read {}", p.display()))?;
        (map_bytes, None)
    } else {
        let grp_data =
            std::fs::read(grp_path).with_context(|| format!("read {}", grp_path.display()))?;
        let grp = room_build::grp::Grp::parse(&grp_data)
            .map_err(|e| anyhow::anyhow!("GRP parse: {e}"))?;
        let body = grp
            .get(map_name)
            .ok_or_else(|| anyhow::anyhow!("map {map_name} not in GRP"))?
            .to_vec();
        let world_tmp = room_build_engine::BuildWorld::load_map_bytes(&body)
            .map_err(|e| anyhow::anyhow!("load map: {e}"))?;
        let pics = world_tmp.referenced_picnums();
        let mut set = room_build::art::TileSet::new();
        for name in grp.names() {
            if name.starts_with("TILES") && name.ends_with(".ART") {
                if let Some(art) = grp.get(name) {
                    let _ = set.add_art(art);
                }
            }
        }
        let tiles = room_build_engine::SoftTiles::from_tile_set_pics(&set, &pics);
        (body, Some(tiles))
    };
    let world = room_build_engine::BuildWorld::load_map_bytes(&map_bytes)
        .map_err(|e| anyhow::anyhow!("load map: {e}"))?;
    let view0 = world.build_view(
        world.start[0],
        world.start[1],
        world.start[2],
        world.start_ang,
        100,
        world.start_sector,
    );
    let mut fly = room_build_engine::FreeFly::from_view(&view0);
    if cmd.forward != 0 || cmd.strafe != 0 || cmd.up != 0 || cmd.turn != 0 || cmd.look != 0 {
        fly.apply(&world, cmd);
    }
    let view = fly.view(&world);
    let mut frame = room_build_engine::SoftFrame::new(width, height);
    if let Some(ref tiles) = tiles {
        room_build_engine::render_fp_textured(&world, &view, &mut frame, tiles);
    } else {
        room_build_engine::render_fp_solid(&world, &view, &mut frame);
    }
    write_pgm(output, &frame)?;
    println!(
        "wrote {} ({}x{}, sect={}, visible={}, textured={})",
        output.display(),
        width,
        height,
        view.sect,
        view.visible_sectors.len(),
        tiles.is_some()
    );
    Ok(())
}

fn write_pgm(path: &std::path::Path, frame: &room_build_engine::SoftFrame) -> Result<()> {
    use std::io::Write;
    let mut f =
        std::fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    writeln!(f, "P5\n{} {}\n255", frame.width, frame.height)?;
    f.write_all(&frame.pixels)?;
    Ok(())
}

fn duke_extract_demos(grp_path: &std::path::Path, out: &std::path::Path) -> Result<()> {
    let bytes = std::fs::read(grp_path).with_context(|| format!("read {}", grp_path.display()))?;
    let grp = room_build::Grp::parse(&bytes).map_err(|e| anyhow::anyhow!("{e}"))?;
    std::fs::create_dir_all(out).with_context(|| format!("mkdir {}", out.display()))?;
    for name in ["DEMO1.DMO", "DEMO2.DMO", "DEMO3.DMO"] {
        let Some(body) = grp.get(name) else {
            bail!("{name} not in {}", grp_path.display());
        };
        let dest = out.join(name);
        std::fs::write(&dest, body).with_context(|| format!("write {}", dest.display()))?;
        println!("wrote {} ({} bytes)", dest.display(), body.len());
    }
    Ok(())
}

/// Soft-check the Duke trace manifest: missing files / empty sha are OK;
/// a present file with the wrong hash fails.
fn duke_trace_check_manifest(path: &std::path::Path) -> Result<()> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut demo = String::new();
    let mut file = String::new();
    let mut sha = String::new();
    let mut bad = 0_u32;
    let mut checked = 0_u32;
    let flush = |demo: &str, file: &str, sha: &str, bad: &mut u32, checked: &mut u32| {
        if file.is_empty() {
            return;
        }
        let p = std::path::Path::new(file);
        if !p.is_file() {
            println!("{demo}: missing ({file})");
            return;
        }
        if sha.is_empty() {
            println!("{demo}: present, sha256 unset ({file})");
            return;
        }
        *checked += 1;
        match std::fs::read(p) {
            Ok(bytes) => {
                let got = room_duke::sha256_hex(&bytes);
                if got == sha {
                    println!("{demo}: ok");
                } else {
                    println!("{demo}: HASH MISMATCH\n  expected {sha}\n  got      {got}");
                    *bad += 1;
                }
            }
            Err(e) => {
                println!("{demo}: read error: {e}");
                *bad += 1;
            }
        }
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[trace]]" {
            flush(&demo, &file, &sha, &mut bad, &mut checked);
            demo.clear();
            file.clear();
            sha.clear();
            continue;
        }
        if let Some(v) = line.strip_prefix("demo = ") {
            demo = v.trim().trim_matches('"').to_owned();
        } else if let Some(v) = line.strip_prefix("path = ") {
            file = v.trim().trim_matches('"').to_owned();
        } else if let Some(v) = line.strip_prefix("sha256 = ") {
            sha = v.trim().trim_matches('"').to_owned();
        }
    }
    flush(&demo, &file, &sha, &mut bad, &mut checked);
    if bad > 0 {
        bail!("{bad} trace hash mismatch(es)");
    }
    println!("manifest soft-check done ({checked} hashed file(s) verified)");
    Ok(())
}

fn read_text_lump(vfs: &Vfs, name: &str) -> Option<String> {
    let id = vfs
        .find(name, Namespace::Global)
        .or_else(|| vfs.find_any_ns(name))?;
    let bytes = vfs.read(id).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn parse_xy2(s: &str) -> Result<(f32, f32, f32, f32)> {
    let p: Vec<_> = s.split(',').map(str::trim).collect();
    if p.len() != 4 {
        bail!("eyes must be x1,y1,x2,y2");
    }
    Ok((p[0].parse()?, p[1].parse()?, p[2].parse()?, p[3].parse()?))
}

fn parse_xy(s: &str) -> Result<(f32, f32)> {
    let p: Vec<_> = s.split(',').map(str::trim).collect();
    if p.len() != 2 {
        bail!("point must be x,y");
    }
    Ok((p[0].parse()?, p[1].parse()?))
}

fn load_png_rgba(path: &std::path::Path) -> Result<room_faceswap::RgbaImage> {
    let data = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let decoder = png::Decoder::new(std::io::Cursor::new(data));
    let mut reader = decoder.read_info().context("png header")?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).context("png frame")?;
    let w = info.width;
    let h = info.height;
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    match info.color_type {
        png::ColorType::Rgba => pixels.extend_from_slice(&buf[..info.buffer_size()]),
        png::ColorType::Rgb => {
            for c in buf[..info.buffer_size()].chunks_exact(3) {
                pixels.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        png::ColorType::Grayscale => {
            for &g in &buf[..info.buffer_size()] {
                pixels.extend_from_slice(&[g, g, g, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for c in buf[..info.buffer_size()].chunks_exact(2) {
                pixels.extend_from_slice(&[c[0], c[0], c[0], c[1]]);
            }
        }
        other => bail!("unsupported PNG color type {other:?}"),
    }
    Ok(room_faceswap::RgbaImage {
        width: w,
        height: h,
        pixels,
    })
}

fn faceswap_bake(
    iwad: PathBuf,
    face: PathBuf,
    eyes: String,
    chin: Option<String>,
    prefix: String,
    anchors: Option<PathBuf>,
    output: PathBuf,
) -> Result<()> {
    use room_faceswap::{
        CompositeOptions, Consent, FaceId, ManualPoints, PalettedSprite, align_manual, bake_class,
        lump_pixel_hash, parse_headanchors,
    };
    use room_formats::patch::{encode_patch, parse_patch};
    use room_formats::playpal::parse_playpal;

    let (x1, y1, x2, y2) = parse_xy2(&eyes)?;
    let chin_xy = if let Some(c) = chin {
        parse_xy(&c)?
    } else {
        ((x1 + x2) * 0.5, y1 + (x2 - x1).abs() * 1.2)
    };
    let src = load_png_rgba(&face)?;
    let profile = align_manual(
        &src,
        ManualPoints {
            left_eye: (x1, y1),
            right_eye: (x2, y2),
            chin: chin_xy,
        },
        "bake",
        FaceId(1),
        Consent::Attested,
        0,
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    let vfs = Vfs::builder().mount_path(&iwad)?.build()?;
    let pal_id = vfs
        .find("PLAYPAL", Namespace::Global)
        .or_else(|| vfs.find_any_ns("PLAYPAL"))
        .context("PLAYPAL")?;
    let playpal = parse_playpal(&vfs.read(pal_id)?)?;
    let pal = playpal.palettes.first().context("empty PLAYPAL")?.rgb;

    let anchors_file = if let Some(p) = anchors {
        Some(parse_headanchors(&std::fs::read_to_string(p)?).map_err(|e| anyhow::anyhow!("{e}"))?)
    } else if let Ok(t) = std::fs::read_to_string("wadsrc/room/HEADANCHORS") {
        Some(parse_headanchors(&t).map_err(|e| anyhow::anyhow!("{e}"))?)
    } else {
        None
    };

    let pref = prefix.to_ascii_uppercase();
    let mut lumps = Vec::new();
    for id in vfs.iter_lumps() {
        let info = vfs.lump(id).context("lump")?;
        let name = info.short_name.to_string().to_ascii_uppercase();
        if !name.starts_with(&pref) || name.len() < 6 {
            continue;
        }
        let Ok(bytes) = vfs.read(id) else { continue };
        let Ok(patch) = parse_patch(&bytes) else {
            continue;
        };
        let (indices, alpha) = patch.to_indices_masked();
        let hash = lump_pixel_hash(&indices, &alpha);
        lumps.push((
            name,
            PalettedSprite {
                width: u32::from(patch.width),
                height: u32::from(patch.height),
                left_offset: i32::from(patch.left_offset),
                top_offset: i32::from(patch.top_offset),
                indices,
                alpha,
            },
            hash,
        ));
    }
    if lumps.is_empty() {
        bail!("no sprites with prefix {pref}");
    }
    let baked = bake_class(
        &lumps,
        &pal,
        &profile,
        anchors_file.as_ref(),
        &CompositeOptions::default(),
        &[],
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut owned: Vec<(String, Vec<u8>)> = Vec::new();
    for lump in &baked.lumps {
        let raw = encode_patch(
            lump.sprite.width as u16,
            lump.sprite.height as u16,
            lump.sprite.left_offset as i16,
            lump.sprite.top_offset as i16,
            &lump.sprite.indices,
            &lump.sprite.alpha,
        )?;
        let path = format!("sprites/{}.lmp", lump.name);
        owned.push((path, raw));
    }
    owned.push((
        "FACEPACK.txt".into(),
        format!(
            "generator=room-faceswap\nprefix={pref}\nlumps={}\n",
            baked.lumps.len()
        )
        .into_bytes(),
    ));
    for (p, b) in &owned {
        files.push((p.clone(), b.clone()));
    }
    let refs: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    let zip = room_vfs::zip_from_files(&refs)?;
    std::fs::write(&output, zip).with_context(|| format!("write {}", output.display()))?;
    println!("baked {} lumps → {}", baked.lumps.len(), output.display());
    Ok(())
}

fn faceswap_anchors_estimate(iwad: PathBuf, prefix: String) -> Result<()> {
    use room_faceswap::{ClassHint, estimate_anchor, yaw_from_lump};
    use room_formats::patch::parse_patch;

    let vfs = Vfs::builder().mount_path(&iwad)?.build()?;
    let pref = prefix.to_ascii_uppercase();
    println!("// HEADANCHORS estimate prefix={pref}");
    for id in vfs.iter_lumps() {
        let info = vfs.lump(id).context("lump")?;
        let name = info.short_name.to_string().to_ascii_uppercase();
        if !name.starts_with(&pref) || name.len() < 6 {
            continue;
        }
        let Ok(bytes) = vfs.read(id) else { continue };
        let Ok(patch) = parse_patch(&bytes) else {
            continue;
        };
        let (_idx, alpha) = patch.to_indices_masked();
        let a = estimate_anchor(
            u32::from(patch.width),
            u32::from(patch.height),
            &alpha,
            &name,
            ClassHint::default(),
        );
        println!("Anchor \"{name}\" hash 0");
        println!("{{");
        if matches!(a.visible, room_faceswap::Visibility::Hidden) {
            println!("    Hidden");
        } else {
            println!("    Center {:.1}, {:.1}", a.center.0, a.center.1);
            println!("    Radius {:.1}, {:.1}", a.radius.0, a.radius.1);
            println!("    Roll 0");
            let _ = yaw_from_lump(&name);
            println!(
                "    Yaw {}",
                match a.yaw {
                    room_faceswap::HeadYaw::Front => "Front",
                    room_faceswap::HeadYaw::ThreeQuarterLeft => "ThreeQuarterLeft",
                    room_faceswap::HeadYaw::ThreeQuarterRight => "ThreeQuarterRight",
                    room_faceswap::HeadYaw::ProfileLeft => "ProfileLeft",
                    room_faceswap::HeadYaw::ProfileRight => "ProfileRight",
                    room_faceswap::HeadYaw::Back => "Back",
                }
            );
            println!("    KeepTop {:.2}", a.keep_top);
        }
        println!("}}");
        println!();
    }
    Ok(())
}

mod hex {
    pub fn encode(bytes: [u8; 16]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut s = String::with_capacity(32);
        for b in bytes {
            s.push(HEX[(b >> 4) as usize] as char);
            s.push(HEX[(b & 0xf) as usize] as char);
        }
        s
    }
}
