//! Converted-mod modules (`docs/37` ZW6, engine-side heap design).
//!
//! A RoomMod module is the AOT module of the mod's *unlinked* program plus
//! three custom sections:
//! - `room.zvm.program`: [`CompiledProgram::to_bytes`];
//! - `room.zvm.flags`: version byte, then per function `compiled | hot << 1`;
//! - `room.zvm.abi`: the `"zvm"` import list it was built against.
//!
//! [`read_room_mod`] checks the module (wasm validation, imports exactly the
//! `"zvm"` runtime, sections consistent) and returns the program, so the
//! game runs the mod with no ZScript compiler. [`register_shipped`] makes the
//! module the one the VM attaches when that program is loaded.

use crate::{Artifact, CodegenError, aot_abi, program_digest, validate};
use room_zvm::CompiledProgram;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Custom section with the program image.
pub const SECTION_PROGRAM: &str = "room.zvm.program";
/// Custom section with per-function flags.
pub const SECTION_FLAGS: &str = "room.zvm.flags";
/// Custom section with the import ABI fingerprint.
pub const SECTION_ABI: &str = "room.zvm.abi";
/// Custom section listing the ZScript sources compiled into the program that
/// stay outside the package (the engine's own ZScript): `path\tdigest` lines.
pub const SECTION_SOURCES: &str = "room.zvm.sources";

/// Digest of a source text, as recorded in [`SECTION_SOURCES`].
#[must_use]
pub fn source_digest(text: &str) -> String {
    let mut a: u64 = 0xcbf2_9ce4_8422_2325;
    let mut b: u64 = 0x9e37_79b9_7f4a_7c15;
    for &x in text.as_bytes() {
        a = (a ^ u64::from(x)).wrapping_mul(0x0000_0100_0000_01b3);
        b = (b.rotate_left(13) ^ u64::from(x)).wrapping_mul(0xff51_afd7_ed55_8ccd);
    }
    format!("{a:016x}{b:016x}{:x}", text.len())
}
const FLAGS_VERSION: u8 = 1;

fn leb(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn custom(out: &mut Vec<u8>, name: &str, data: &[u8]) {
    let mut body = Vec::new();
    leb(&mut body, name.len() as u64);
    body.extend_from_slice(name.as_bytes());
    body.extend_from_slice(data);
    out.push(0);
    leb(out, body.len() as u64);
    out.extend_from_slice(&body);
}

/// Text form of the import list (`name(params)->results;…`).
#[must_use]
pub fn abi_fingerprint() -> String {
    let mut s = String::new();
    for (n, p, r) in aot_abi::IMPORTS {
        s.push_str(&format!("{n}({})->({});", p.join(","), r.join(",")));
    }
    s
}

pub(crate) fn append_sections(art: &mut Artifact, program: &CompiledProgram) {
    append_sections_with(art, program, &[]);
}

fn append_sections_with(
    art: &mut Artifact,
    program: &CompiledProgram,
    external: &[(String, String)],
) {
    let list: String = external
        .iter()
        .map(|(p, text)| format!("{p}\t{}\n", source_digest(text)))
        .collect();
    custom(&mut art.bytes, SECTION_SOURCES, list.as_bytes());
    custom(&mut art.bytes, SECTION_PROGRAM, &program.to_bytes());
    let mut flags = vec![FLAGS_VERSION];
    for (c, h) in art.compiled.iter().zip(&art.hot) {
        flags.push(u8::from(*c) | u8::from(*h) << 1);
    }
    custom(&mut art.bytes, SECTION_FLAGS, &flags);
    custom(&mut art.bytes, SECTION_ABI, abi_fingerprint().as_bytes());
}

/// A loaded converted-mod module.
#[derive(Clone, Debug)]
pub struct RoomMod {
    /// Unlinked program (what `room_zscript::check` produced at convert time).
    pub program: CompiledProgram,
    /// The module bytes.
    pub bytes: Vec<u8>,
    /// Per function: compiled to wasm.
    pub compiled: Vec<bool>,
    /// Per function: entered through wasm under the `auto` policy.
    pub hot: Vec<bool>,
    /// Sources outside the package compiled into the program: `(path, digest)`.
    pub external_sources: Vec<(String, String)>,
}

impl RoomMod {
    /// True when `sources` (what the VFS ships besides this module) are exactly
    /// the external sources the module was compiled with.
    #[must_use]
    pub fn matches_external(&self, sources: &[(String, String)]) -> bool {
        let mut have: Vec<(String, String)> = sources
            .iter()
            .map(|(p, t)| (p.to_ascii_lowercase(), source_digest(t)))
            .collect();
        let mut want: Vec<(String, String)> = self
            .external_sources
            .iter()
            .map(|(p, d)| (p.to_ascii_lowercase(), d.clone()))
            .collect();
        have.sort();
        want.sort();
        have == want
    }
}

/// Parse and check a RoomMod module.
pub fn read_room_mod(bytes: &[u8]) -> Result<RoomMod, CodegenError> {
    let bad = |m: String| CodegenError::Invalid(m);
    validate(bytes)?;
    let mut program = None;
    let mut flags = None;
    let mut abi = None;
    let mut external = None;
    let mut imports = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        match payload.map_err(|e| bad(e.to_string()))? {
            wasmparser::Payload::CustomSection(c) => match c.name() {
                SECTION_PROGRAM => program = Some(c.data().to_vec()),
                SECTION_FLAGS => flags = Some(c.data().to_vec()),
                SECTION_ABI => abi = Some(c.data().to_vec()),
                SECTION_SOURCES => external = Some(c.data().to_vec()),
                _ => {}
            },
            wasmparser::Payload::ImportSection(r) => {
                for imp in r {
                    let imp = imp.map_err(|e| bad(e.to_string()))?;
                    imports.push((imp.module.to_owned(), imp.name.to_owned()));
                }
            }
            _ => {}
        }
    }
    let program = program.ok_or_else(|| bad(format!("no {SECTION_PROGRAM} section")))?;
    let program = CompiledProgram::from_bytes(&program).map_err(bad)?;
    let flags = flags.ok_or_else(|| bad(format!("no {SECTION_FLAGS} section")))?;
    if flags.first() != Some(&FLAGS_VERSION) || flags.len() != program.functions.len() + 1 {
        return Err(bad("flags section does not match the program".into()));
    }
    if abi.as_deref() != Some(abi_fingerprint().as_bytes()) {
        return Err(bad(
            "module was built against a different zvm import ABI; convert the mod again".into(),
        ));
    }
    let expected: Vec<(String, String)> = aot_abi::IMPORTS
        .iter()
        .map(|(n, _, _)| (aot_abi::IMPORT_MODULE.to_owned(), (*n).to_owned()))
        .collect();
    if imports != expected {
        return Err(bad(
            "module imports something other than the zvm runtime".into()
        ));
    }
    let compiled = flags[1..].iter().map(|f| f & 1 != 0).collect();
    let hot = flags[1..].iter().map(|f| f & 2 != 0).collect();
    let external_sources = String::from_utf8_lossy(&external.unwrap_or_default())
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(p, d)| (p.to_owned(), d.to_owned()))
        .collect();
    Ok(RoomMod {
        program,
        bytes: bytes.to_vec(),
        compiled,
        hot,
        external_sources,
    })
}

type Registry = Mutex<HashMap<(u64, u64), Arc<RoomMod>>>;

fn registry() -> &'static Registry {
    static REG: OnceLock<Registry> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Make `m` the module the VM attaches when `m.program` is loaded.
pub fn register_shipped(m: RoomMod) -> Arc<RoomMod> {
    let key = program_digest(&m.program);
    let m = Arc::new(m);
    if let Ok(mut r) = registry().lock() {
        r.insert(key, Arc::clone(&m));
    }
    m
}

/// Forget every registered module (verification runs the original package
/// on the interpreter first).
pub fn clear_shipped() {
    if let Ok(mut r) = registry().lock() {
        r.clear();
    }
}

/// The shipped module for this (unlinked) program, if one was registered.
#[must_use]
pub fn shipped_for(program: &CompiledProgram) -> Option<Arc<RoomMod>> {
    let key = program_digest(program);
    registry().lock().ok()?.get(&key).cloned()
}

/// Compile a converted-mod module from an unlinked program.
pub fn compile_room_mod(program: &CompiledProgram) -> Result<Artifact, CodegenError> {
    compile_room_mod_with(program, &[])
}

/// [`compile_room_mod`], recording `external` `(path, text)` sources that were
/// compiled into `program` but are not part of the package (the engine's
/// ZScript). The loader uses the module only when the VFS ships exactly those.
pub fn compile_room_mod_with(
    program: &CompiledProgram,
    external: &[(String, String)],
) -> Result<Artifact, CodegenError> {
    let mut art = crate::emit::emit_aot(
        program,
        &crate::Options {
            target: crate::Target::RoomMod,
            ..Default::default()
        },
    );
    append_sections_with(&mut art, program, external);
    art.stats.bytes = art.bytes.len();
    validate(&art.bytes)?;
    Ok(art)
}

/// Attach the shipped module registered for `program` to `vm` (built from
/// that program). Returns false when none is registered.
pub fn attach_if_shipped(
    vm: &mut room_zvm::Zvm,
    program: &CompiledProgram,
) -> Result<bool, String> {
    let Some(m) = shipped_for(program) else {
        return Ok(false);
    };
    vm.attach_shipped(&m.bytes, m.compiled.clone(), m.hot.clone())?;
    Ok(true)
}
