//! CLI tools: wad ls/extract, map dump, nodebuild, demo verify, render-to-png.

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use ferrite_vfs::{Namespace, Vfs, VfsBuilder};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "ferrite", version, about = "Ferrite engine tools")]
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
    /// Ferrite demo (`.fdem` / `.fdbn`) tools
    Demo {
        #[command(subcommand)]
        command: DemoCmd,
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
                    let id = ferrite_vfs::ContainerId(i as u32);
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
                    ferrite_map::load_map(&vfs, &map).with_context(|| format!("load map {map}"))?;
                if rebuild_nodes {
                    ferrite_nodebuild::apply_nodes(&mut geom);
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
                    ferrite_map::load_map(&vfs, &map).with_context(|| format!("load map {map}"))?;
                if rebuild_nodes {
                    ferrite_nodebuild::apply_nodes(&mut geom);
                }
                let svg = ferrite_map::map_to_svg(&geom, scale);
                std::fs::write(&output, svg.as_bytes())
                    .with_context(|| format!("write {}", output.display()))?;
                println!("wrote {} ({} bytes)", output.display(), svg.len());
            }
            MapCmd::Nodebuild { path, map } => {
                let vfs = mount_one(&path)?;
                let mut geom =
                    ferrite_map::load_map(&vfs, &map).with_context(|| format!("load map {map}"))?;
                ferrite_nodebuild::apply_nodes(&mut geom);
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
                    let bundle = ferrite_game::DemoBundle::from_fdbn(&bytes)
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
                    let demo = ferrite_game::DemoRecorder::from_fdem(&bytes)
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
                    let bundle = ferrite_game::DemoBundle::from_fdbn(&bytes)
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
                    let demo = ferrite_game::DemoRecorder::from_fdem(&bytes)
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
                        let parsed = ferrite_defs::parse_mapinfo(&text)?;
                        print!("{}", ferrite_defs::dump_debug(&parsed));
                    }
                    "sndinfo" => {
                        let text = read_text_lump(&vfs, "SNDINFO").context("no SNDINFO lump")?;
                        let parsed = ferrite_defs::parse_sndinfo(&text)?;
                        print!("{}", ferrite_defs::dump_debug(&parsed));
                    }
                    "gameinfo" => {
                        let text = read_text_lump(&vfs, "GAMEINFO").context("no GAMEINFO lump")?;
                        let parsed = ferrite_defs::parse_gameinfo(&text)?;
                        print!("{}", ferrite_defs::dump_debug(&parsed));
                    }
                    "language" => {
                        let text = read_text_lump(&vfs, "LANGUAGE").context("no LANGUAGE lump")?;
                        let parsed = ferrite_defs::parse_language(&text)?;
                        print!("{}", ferrite_defs::dump_debug(&parsed));
                    }
                    "textures" => {
                        let text = read_text_lump(&vfs, "TEXTURES").context("no TEXTURES lump")?;
                        let parsed = ferrite_defs::parse_textures(&text)?;
                        print!("{}", ferrite_defs::dump_debug(&parsed));
                    }
                    "animdefs" => {
                        let text = read_text_lump(&vfs, "ANIMDEFS").context("no ANIMDEFS lump")?;
                        let parsed = ferrite_defs::parse_animdefs(&text)?;
                        print!("{}", ferrite_defs::dump_debug(&parsed));
                    }
                    "cvarinfo" => {
                        let text = read_text_lump(&vfs, "CVARINFO").context("no CVARINFO lump")?;
                        let parsed = ferrite_defs::parse_cvarinfo(&text)?;
                        print!("{}", ferrite_defs::dump_debug(&parsed));
                    }
                    "fontdefs" => {
                        let text = read_text_lump(&vfs, "FONTDEFS").context("no FONTDEFS lump")?;
                        let parsed = ferrite_defs::parse_fontdefs(&text)?;
                        print!("{}", ferrite_defs::dump_debug(&parsed));
                    }
                    "all" => {
                        let parsed = ferrite_defs::DefinitionsBuilder::new(&vfs).build()?;
                        print!("{}", ferrite_defs::dump_debug(&parsed));
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
    std::sync::Arc<ferrite_sim::ClassRegistry>,
    std::sync::Arc<ferrite_sim::StateTable>,
)> {
    let path = actors.cloned().or_else(default_doom_basic_path);
    let Some(path) = path else {
        bail!("no DECORATE actors file (pass --actors or place testdata/actors/doom_basic.txt)");
    };
    let src = std::fs::read_to_string(&path)
        .with_context(|| format!("read actors {}", path.display()))?;
    let ir = ferrite_decorate::parse_decorate(&src)?;
    let classes = std::sync::Arc::new(
        ferrite_sim::ClassRegistry::from_decorate(&ir).map_err(|e| anyhow::anyhow!("{e}"))?,
    );
    let states = std::sync::Arc::new(ferrite_sim::StateTable::from_decorate(&ir, &classes));
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
    demo: &ferrite_game::DemoRecorder,
    vfs: &Vfs,
    map_override: Option<&str>,
    classes: &std::sync::Arc<ferrite_sim::ClassRegistry>,
    states: &std::sync::Arc<ferrite_sim::StateTable>,
) -> Result<()> {
    let map_name = map_override.unwrap_or(demo.map.as_str());
    let mut geom =
        ferrite_map::load_map(vfs, map_name).with_context(|| format!("load map {map_name}"))?;
    ferrite_nodebuild::apply_nodes(&mut geom);
    let skill = if demo.skill == 0 { 3 } else { demo.skill };
    let mut world = ferrite_sim::World::from_map_with_skill(
        geom,
        std::sync::Arc::clone(classes),
        std::sync::Arc::clone(states),
        skill,
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    world.rng.set_seed(demo.rng_seed);
    let player = ferrite_game::DemoPlayer { demo };
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

fn read_text_lump(vfs: &Vfs, name: &str) -> Option<String> {
    let id = vfs
        .find(name, Namespace::Global)
        .or_else(|| vfs.find_any_ns(name))?;
    let bytes = vfs.read(id).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
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
