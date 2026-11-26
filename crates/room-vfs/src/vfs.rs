//! Mounted virtual filesystem.

use crate::archive::{
    ArchiveEntry, StoredPayload, is_map_wad, is_nested_root_wad, parse_directory, parse_zip,
    read_archive_payload,
};
use crate::error::VfsError;
use crate::lump::{ContainerId, ContainerInfo, ContainerKind, LumpId, LumpInfo};
use crate::namespace::Namespace;
use crate::short_name::ShortName;
use crate::source::{ContainerSource, MemorySource};
use crate::wad::{WadEntry, parse_wad, read_wad_lump};
use indexmap::IndexMap;
use md5::{Digest, Md5};
use smallvec::SmallVec;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
enum LumpData {
    Wad(WadEntry),
    Archive(ArchiveEntry),
}

struct ContainerSlot {
    info: ContainerInfo,
    source: Arc<dyn ContainerSource>,
}

/// Builder for a [`Vfs`].
pub struct VfsBuilder {
    containers: Vec<ContainerSlot>,
    lumps: Vec<(LumpInfo, LumpData)>,
    active_filters: Vec<String>,
    warnings: Vec<String>,
    /// Mount `maps/NAME.wad` entries of archives as map WADs (GZDoom).
    nested_map_wads: bool,
}

impl Default for VfsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl VfsBuilder {
    /// Empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            containers: Vec::new(),
            lumps: Vec::new(),
            active_filters: Vec::new(),
            warnings: Vec::new(),
            nested_map_wads: true,
        }
    }

    /// Mount each `maps/NAME.wad` of a PK3 / PK7 / directory right after it,
    /// so its map loads as `NAME` (GZDoom: the map takes the file's name).
    /// On by default; the web editor turns it off because it mounts those
    /// WADs itself under `pack.pk3:maps/name.wad` to edit them.
    #[must_use]
    pub fn nested_map_wads(mut self, on: bool) -> Self {
        self.nested_map_wads = on;
        self
    }

    /// Set active `filter/<name>/` directory names (from IWAD Autoname).
    #[must_use]
    pub fn filters(mut self, filters: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.active_filters = filters.into_iter().map(Into::into).collect();
        self
    }

    /// Mount bytes already in memory.
    pub fn mount_memory(
        mut self,
        name: impl Into<String>,
        data: impl Into<bytes::Bytes>,
    ) -> Result<Self, VfsError> {
        let name = name.into();
        let data = data.into();
        let source = Arc::new(MemorySource::new(data.clone()));
        self.mount_source(name, source, Some(data))?;
        Ok(self)
    }

    /// Mount each path from [`crate::collect_autoload_paths`] (or any ordered list).
    pub fn mount_paths(
        mut self,
        paths: impl IntoIterator<Item = impl AsRef<Path>>,
    ) -> Result<Self, VfsError> {
        for path in paths {
            self = self.mount_path(path)?;
        }
        Ok(self)
    }

    /// Mount a filesystem path (file or directory).
    pub fn mount_path(mut self, path: impl AsRef<Path>) -> Result<Self, VfsError> {
        let path = path.as_ref();
        let name = path.display().to_string();
        if path.is_dir() {
            let id = self.push_container(
                name,
                ContainerKind::Directory,
                [0u8; 16],
                Arc::new(MemorySource::new(bytes::Bytes::new())),
            )?;
            let entries = parse_directory(path, id, &self.active_filters)?;
            self.append_archive_entries(entries)?;
            return Ok(self);
        }
        // One copy of the file: the source and the parser share these bytes.
        let data = crate::mapped::load(path)?;
        let source = Arc::new(MemorySource::new(data.clone()));
        self.mount_source(name, source, Some(data))?;
        Ok(self)
    }

    fn mount_source(
        &mut self,
        name: String,
        source: Arc<dyn ContainerSource>,
        peek: Option<bytes::Bytes>,
    ) -> Result<(), VfsError> {
        let bytes = match peek {
            Some(p) => p,
            None => bytes::Bytes::from(source.read_all()?),
        };
        let md5 = md5_bytes(&bytes);
        let kind = detect_kind(&bytes, Path::new(&name));
        let id = self.push_container(name.clone(), kind, md5, Arc::clone(&source))?;
        match kind {
            ContainerKind::Wad => {
                let entries = parse_wad(source.as_ref(), id)?;
                for e in entries {
                    if e.truncated {
                        self.warnings
                            .push(format!("{name}: truncated lump {}", e.info.short_name));
                    }
                    self.lumps.push((e.info.clone(), LumpData::Wad(e)));
                }
            }
            ContainerKind::Zip => {
                let entries = parse_zip(&bytes, id, &self.active_filters)?;
                self.append_archive_entries(entries)?;
            }
            ContainerKind::SevenZ => {
                #[cfg(feature = "7z")]
                {
                    let entries = crate::archive::parse_sevenz(&bytes, id, &self.active_filters)?;
                    self.append_archive_entries(entries)?;
                }
                #[cfg(not(feature = "7z"))]
                {
                    return Err(VfsError::parse(
                        "7z support disabled; enable room-vfs feature `7z`",
                    ));
                }
            }
            ContainerKind::Directory => {}
            ContainerKind::LooseFile => {
                let stem = Path::new(&name)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("LOOSE");
                let short = ShortName::from_stem(stem);
                let size = u32::try_from(bytes.len())
                    .map_err(|_| VfsError::parse("loose file too large"))?;
                let info = LumpInfo {
                    container: id,
                    short_name: short,
                    full_path: short.as_str().to_ascii_lowercase(),
                    namespace: Namespace::Global,
                    size,
                    flags: Default::default(),
                };
                self.lumps.push((
                    info.clone(),
                    LumpData::Archive(ArchiveEntry {
                        info,
                        payload: StoredPayload::Inflated(bytes.clone()),
                    }),
                ));
            }
        }
        Ok(())
    }

    fn append_archive_entries(&mut self, entries: Vec<ArchiveEntry>) -> Result<(), VfsError> {
        let mut nested: Vec<(String, bytes::Bytes)> = Vec::new();
        // (mount name, map name from the file stem, bytes)
        let mut maps: Vec<(String, String, bytes::Bytes)> = Vec::new();
        for e in entries {
            if is_nested_root_wad(&e.info.full_path) && !is_map_wad(&e.info.full_path) {
                if let Ok(b) = read_archive_payload(&e.payload) {
                    nested.push((e.info.full_path.clone(), b));
                }
            }
            if self.nested_map_wads && is_map_wad(&e.info.full_path) {
                let data = read_archive_payload(&e.payload).ok();
                if let Some(data) =
                    data.filter(|d| d.starts_with(b"PWAD") || d.starts_with(b"IWAD"))
                {
                    let owner = self
                        .containers
                        .get(e.info.container.0 as usize)
                        .map(|c| c.info.name.clone())
                        .unwrap_or_default();
                    let stem = e
                        .info
                        .full_path
                        .rsplit('/')
                        .next()
                        .unwrap_or_default()
                        .rsplit_once('.')
                        .map_or("", |(s, _)| s)
                        .to_owned();
                    maps.push((format!("{owner}:{}", e.info.full_path), stem, data));
                }
            }
            self.lumps.push((e.info.clone(), LumpData::Archive(e)));
        }
        for (name, data) in nested {
            self.mount_source(name, Arc::new(MemorySource::new(data.clone())), Some(data))?;
        }
        for (name, stem, data) in maps {
            let first = self.lumps.len();
            if let Err(e) = self.mount_source(
                name.clone(),
                Arc::new(MemorySource::new(data.clone())),
                Some(data),
            ) {
                self.warnings.push(format!("{name}: {e}"));
                continue;
            }
            // GZDoom names the map after the file, whatever its header says.
            if let Some((info, payload)) = self.lumps.get_mut(first) {
                let want = ShortName::from_stem(&stem);
                if !stem.is_empty() && info.short_name != want {
                    info.short_name = want;
                    info.full_path = want.as_str().to_ascii_lowercase();
                    if let LumpData::Wad(w) = payload {
                        w.info.short_name = want;
                        w.info.full_path = info.full_path.clone();
                    }
                }
            }
        }
        Ok(())
    }

    fn push_container(
        &mut self,
        name: String,
        kind: ContainerKind,
        md5: [u8; 16],
        source: Arc<dyn ContainerSource>,
    ) -> Result<ContainerId, VfsError> {
        let id = ContainerId(
            u32::try_from(self.containers.len())
                .map_err(|_| VfsError::parse("too many containers"))?,
        );
        self.containers.push(ContainerSlot {
            info: ContainerInfo { name, kind, md5 },
            source,
        });
        Ok(id)
    }

    /// Finish building the VFS.
    pub fn build(self) -> Result<Vfs, VfsError> {
        let mut by_short: IndexMap<(Namespace, u64), SmallVec<[LumpId; 2]>> = IndexMap::new();
        let mut by_full: HashMap<String, LumpId> = HashMap::new();
        let mut infos = Vec::with_capacity(self.lumps.len());
        let mut data = Vec::with_capacity(self.lumps.len());

        for (info, payload) in self.lumps {
            let id =
                LumpId(u32::try_from(infos.len()).map_err(|_| VfsError::parse("too many lumps"))?);
            if !info.short_name.is_empty() && !info.flags.contains(crate::lump::LumpFlags::FULLPATH)
            {
                by_short
                    .entry((info.namespace, info.short_name.as_u64()))
                    .or_default()
                    .push(id);
            }
            if !info.full_path.is_empty() {
                by_full.insert(info.full_path.clone(), id);
                // also without extension
                if let Some((stem, _)) = info.full_path.rsplit_once('.') {
                    by_full.entry(stem.to_owned()).or_insert(id);
                }
            }
            infos.push(info);
            data.push(payload);
        }

        Ok(Vfs {
            containers: self.containers,
            infos,
            data,
            by_short,
            by_full,
            warnings: self.warnings,
            cache: Mutex::new(LruCache::new(256 * 1024 * 1024)),
        })
    }
}

fn detect_kind(bytes: &[u8], path: &Path) -> ContainerKind {
    if bytes.len() >= 4 && (&bytes[0..4] == b"IWAD" || &bytes[0..4] == b"PWAD") {
        return ContainerKind::Wad;
    }
    if bytes.len() >= 4 && &bytes[0..4] == b"PK\x03\x04" {
        return ContainerKind::Zip;
    }
    if bytes.len() >= 6 && &bytes[0..6] == b"7z\xBC\xAF'\x1C" {
        return ContainerKind::SevenZ;
    }
    // ZIP may start with empty span; also detect by extension.
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "pk3" | "ipk3" | "zip" | "pke" => ContainerKind::Zip,
        "pk7" | "ipk7" | "7z" => ContainerKind::SevenZ,
        "wad" | "iwad" => ContainerKind::Wad,
        _ => ContainerKind::LooseFile,
    }
}

fn md5_bytes(data: &[u8]) -> [u8; 16] {
    let digest = Md5::digest(data);
    let mut out = [0u8; 16];
    out.copy_from_slice(&digest);
    out
}

struct LruCache {
    budget: usize,
    used: usize,
    map: IndexMap<LumpId, bytes::Bytes>,
}

impl LruCache {
    fn new(budget: usize) -> Self {
        Self {
            budget,
            used: 0,
            map: IndexMap::new(),
        }
    }

    fn get(&mut self, id: LumpId) -> Option<bytes::Bytes> {
        let val = self.map.shift_remove(&id)?;
        self.map.insert(id, val.clone());
        Some(val)
    }

    fn insert(&mut self, id: LumpId, data: bytes::Bytes) {
        if let Some(old) = self.map.shift_remove(&id) {
            self.used = self.used.saturating_sub(old.len());
        }
        while self.used + data.len() > self.budget && !self.map.is_empty() {
            if let Some((_, old)) = self.map.shift_remove_index(0) {
                self.used = self.used.saturating_sub(old.len());
            }
        }
        self.used += data.len();
        self.map.insert(id, data);
    }

    fn clear(&mut self) {
        self.map.clear();
        self.used = 0;
    }
}

/// Mounted virtual filesystem.
pub struct Vfs {
    containers: Vec<ContainerSlot>,
    infos: Vec<LumpInfo>,
    data: Vec<LumpData>,
    by_short: IndexMap<(Namespace, u64), SmallVec<[LumpId; 2]>>,
    by_full: HashMap<String, LumpId>,
    warnings: Vec<String>,
    cache: Mutex<LruCache>,
}

impl Vfs {
    /// Start a builder.
    #[must_use]
    pub fn builder() -> VfsBuilder {
        VfsBuilder::new()
    }

    /// Append an in-memory WAD/ZIP as a late-mounted container (overrides prior
    /// short names). Used for mod-generated lumps (`docs/31` MA3).
    ///
    /// # Errors
    ///
    /// Parse / size failures from the container.
    pub fn mount_memory(
        &mut self,
        name: impl Into<String>,
        data: impl Into<bytes::Bytes>,
    ) -> Result<(), VfsError> {
        let name = name.into();
        let data = data.into();
        let source: Arc<dyn ContainerSource> = Arc::new(MemorySource::new(data.clone()));
        let md5 = md5_bytes(data.as_ref());
        let kind = detect_kind(data.as_ref(), Path::new(&name));
        let id = ContainerId(
            u32::try_from(self.containers.len())
                .map_err(|_| VfsError::parse("too many containers"))?,
        );
        self.containers.push(ContainerSlot {
            info: ContainerInfo {
                name: name.clone(),
                kind,
                md5,
            },
            source: Arc::clone(&source),
        });
        let mut new_lumps: Vec<(LumpInfo, LumpData)> = Vec::new();
        match kind {
            ContainerKind::Wad => {
                let entries = parse_wad(source.as_ref(), id)?;
                for e in entries {
                    if e.truncated {
                        self.warnings
                            .push(format!("{name}: truncated lump {}", e.info.short_name));
                    }
                    new_lumps.push((e.info.clone(), LumpData::Wad(e)));
                }
            }
            ContainerKind::Zip => {
                let entries = parse_zip(&data, id, &[])?;
                for e in entries {
                    new_lumps.push((e.info.clone(), LumpData::Archive(e)));
                }
            }
            _ => {
                return Err(VfsError::parse(
                    "mount_memory on Vfs only supports WAD/ZIP containers",
                ));
            }
        }
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
        for (info, payload) in new_lumps {
            let lid = LumpId(
                u32::try_from(self.infos.len()).map_err(|_| VfsError::parse("too many lumps"))?,
            );
            if !info.short_name.is_empty() && !info.flags.contains(crate::lump::LumpFlags::FULLPATH)
            {
                self.by_short
                    .entry((info.namespace, info.short_name.as_u64()))
                    .or_default()
                    .push(lid);
            }
            if !info.full_path.is_empty() {
                self.by_full.insert(info.full_path.clone(), lid);
                if let Some((stem, _)) = info.full_path.rsplit_once('.') {
                    self.by_full.insert(stem.to_owned(), lid);
                }
            }
            self.infos.push(info);
            self.data.push(payload);
        }
        Ok(())
    }

    /// Last-loaded short name in `ns`.
    #[must_use]
    pub fn find(&self, name: &str, ns: Namespace) -> Option<LumpId> {
        let key = (ns, ShortName::from_stem(name).as_u64());
        self.by_short.get(&key).and_then(|v| v.last().copied())
    }

    /// Global first, then any namespace (GZDoom `CheckNumForName` fallback style).
    #[must_use]
    pub fn find_any_ns(&self, name: &str) -> Option<LumpId> {
        self.find(name, Namespace::Global).or_else(|| {
            let sn = ShortName::from_stem(name).as_u64();
            self.by_short
                .iter()
                .rev()
                .find(|((_, n), _)| *n == sn)
                .and_then(|(_, v)| v.last().copied())
        })
    }

    /// Full-path lookup (`textures/foo.png`); also tries without extension.
    #[must_use]
    pub fn find_full(&self, path: &str) -> Option<LumpId> {
        let key = path.replace('\\', "/").to_ascii_lowercase();
        self.by_full.get(&key).copied().or_else(|| {
            let stem = key.rsplit_once('.').map(|(s, _)| s.to_owned())?;
            self.by_full.get(&stem).copied()
        })
    }

    /// All lumps with this short name in `ns`, load order.
    pub fn find_all<'a>(&'a self, name: &str, ns: Namespace) -> impl Iterator<Item = LumpId> + 'a {
        let key = (ns, ShortName::from_stem(name).as_u64());
        self.by_short.get(&key).into_iter().flatten().copied()
    }

    /// Lookup inside one container only.
    #[must_use]
    pub fn find_in_container(&self, c: ContainerId, name: &str) -> Option<LumpId> {
        let sn = ShortName::from_stem(name);
        self.infos.iter().enumerate().rev().find_map(|(i, info)| {
            if info.container == c && info.short_name == sn {
                Some(LumpId(i as u32))
            } else {
                None
            }
        })
    }

    /// Lump metadata.
    #[must_use]
    pub fn lump(&self, id: LumpId) -> Option<&LumpInfo> {
        self.infos.get(id.0 as usize)
    }

    /// Total mounted lumps (including markers).
    #[must_use]
    pub fn lump_count(&self) -> usize {
        self.infos.len()
    }

    /// Iterate all lump IDs in mount order.
    pub fn iter_lumps(&self) -> impl Iterator<Item = LumpId> + '_ {
        (0..self.infos.len()).map(|i| LumpId(i as u32))
    }

    /// Container metadata.
    #[must_use]
    pub fn container(&self, id: ContainerId) -> Option<&ContainerInfo> {
        self.containers.get(id.0 as usize).map(|c| &c.info)
    }

    /// Read lump bytes (cached).
    pub fn read(&self, id: LumpId) -> Result<bytes::Bytes, VfsError> {
        if let Ok(mut cache) = self.cache.lock() {
            if let Some(hit) = cache.get(id) {
                return Ok(hit);
            }
        }
        let idx = id.0 as usize;
        let bytes = match self.data.get(idx).ok_or(VfsError::UnknownLump(id.0))? {
            LumpData::Wad(e) => {
                let c = &self.containers[e.info.container.0 as usize];
                read_wad_lump(c.source.as_ref(), e)?
            }
            LumpData::Archive(e) => read_archive_payload(&e.payload)?,
        };
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(id, bytes.clone());
        }
        Ok(bytes)
    }

    /// MD5 of a container's raw bytes (directories are zeros).
    #[must_use]
    pub fn container_md5(&self, c: ContainerId) -> [u8; 16] {
        self.containers
            .get(c.0 as usize)
            .map(|c| c.info.md5)
            .unwrap_or([0; 16])
    }

    /// Iterate all lumps in load order.
    pub fn iter(&self) -> impl Iterator<Item = (LumpId, &LumpInfo)> {
        self.infos
            .iter()
            .enumerate()
            .map(|(i, info)| (LumpId(i as u32), info))
    }

    /// Number of lumps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.infos.len()
    }

    /// True if no lumps.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.infos.is_empty()
    }

    /// Mount-time warnings.
    #[must_use]
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Container count.
    #[must_use]
    pub fn container_count(&self) -> usize {
        self.containers.len()
    }
}

/// Helper used by tests / CLI for building a zip in memory (deflate).
pub fn zip_from_files(files: &[(&str, &[u8])]) -> Result<Vec<u8>, VfsError> {
    zip_from_files_with_method(files, zip::CompressionMethod::Deflated)
}

/// Build a ZIP/PK3 with an explicit compression method (deflate / bzip2 / stored).
pub fn zip_from_files_with_method(
    files: &[(&str, &[u8])],
    method: zip::CompressionMethod,
) -> Result<Vec<u8>, VfsError> {
    use std::io::Write;
    let cursor = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(cursor);
    let opts = zip::write::SimpleFileOptions::default().compression_method(method);
    for (name, data) in files {
        zip.start_file(*name, opts)
            .map_err(|e| VfsError::parse(e.to_string()))?;
        zip.write_all(data)?;
    }
    let cursor = zip.finish().map_err(|e| VfsError::parse(e.to_string()))?;
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::write::WadWriter;

    #[test]
    fn pk3_map_wads_mount_under_the_file_name() {
        // maps/bikerace.wad holds marker BIKERACE; maps/arena.wad says MAP01
        // inside but GZDoom still calls the map ARENA.
        let map = |marker: &str| {
            WadWriter::pwad()
                .marker(marker)
                .add("TEXTMAP", b"namespace = \"zdoom\";".to_vec())
                .marker("ENDMAP")
                .finish()
                .unwrap()
        };
        let pk3 = zip_from_files(&[
            ("maps/bikerace.wad", map("BIKERACE").as_slice()),
            ("maps/arena.wad", map("MAP01").as_slice()),
            ("decorate.txt", b"".as_slice()),
        ])
        .unwrap();
        let vfs = VfsBuilder::new()
            .mount_memory("mod.pk3", pk3.clone())
            .unwrap()
            .build()
            .unwrap();
        assert!(vfs.find("BIKERACE", Namespace::Global).is_some());
        assert!(vfs.find("ARENA", Namespace::Global).is_some());
        assert!(vfs.find("MAP01", Namespace::Global).is_none());
        // Off (web editor mounts them itself).
        let vfs = VfsBuilder::new()
            .nested_map_wads(false)
            .mount_memory("mod.pk3", pk3)
            .unwrap()
            .build()
            .unwrap();
        assert!(vfs.find("BIKERACE", Namespace::Global).is_none());
    }

    #[test]
    fn last_loaded_wins() {
        let a = WadWriter::pwad().add("DEMO1", vec![1]).finish().unwrap();
        let b = WadWriter::pwad().add("DEMO1", vec![2]).finish().unwrap();
        let vfs = VfsBuilder::new()
            .mount_memory("a.wad", a)
            .unwrap()
            .mount_memory("b.wad", b)
            .unwrap()
            .build()
            .unwrap();
        let id = vfs.find("DEMO1", Namespace::Global).unwrap();
        assert_eq!(vfs.read(id).unwrap().as_ref(), &[2]);
    }

    #[test]
    fn pk3_namespace_and_filter() {
        let zip = zip_from_files(&[
            ("sprites/troo/TROOA1.png", &[1, 2]),
            ("filter/doom/graphics/TITLE.png", &[3]),
            ("filter/heretic/graphics/TITLE.png", &[4]),
            ("modroot/readme.txt", &[5]),
        ])
        .unwrap();
        // single-root strip: all under? no - mixed. add single root case separately.
        let vfs = VfsBuilder::new()
            .filters(["doom"])
            .mount_memory("test.pk3", zip)
            .unwrap()
            .build()
            .unwrap();
        assert!(vfs.find("TROOA1", Namespace::Sprites).is_some());
        let title = vfs.find("TITLE", Namespace::Graphics).unwrap();
        assert_eq!(vfs.read(title).unwrap().as_ref(), &[3]);
    }

    #[test]
    fn single_root_strip() {
        let zip = zip_from_files(&[
            ("mymod/sprites/POSSA1.png", &[9]),
            ("mymod/MAPINFO", b"map MAP01"),
        ])
        .unwrap();
        let vfs = VfsBuilder::new()
            .mount_memory("mod.pk3", zip)
            .unwrap()
            .build()
            .unwrap();
        assert!(vfs.find("POSSA1", Namespace::Sprites).is_some());
        assert!(vfs.find("MAPINFO", Namespace::Global).is_some());
    }

    #[test]
    fn nested_wad_mounted() {
        let inner = WadWriter::pwad().add("INNER", vec![7, 7]).finish().unwrap();
        let zip = zip_from_files(&[("extra.wad", &inner), ("other.txt", b"hi")]).unwrap();
        let vfs = VfsBuilder::new()
            .mount_memory("pack.pk3", zip)
            .unwrap()
            .build()
            .unwrap();
        let id = vfs.find("INNER", Namespace::Global).unwrap();
        assert_eq!(vfs.read(id).unwrap().as_ref(), &[7, 7]);
        assert!(vfs.container_count() >= 2);
    }

    #[test]
    fn pk3_bzip2_entry_reads() {
        let zip = zip_from_files_with_method(
            &[("graphics/BZ2.txt", b"bzip2-hello")],
            zip::CompressionMethod::Bzip2,
        )
        .unwrap();
        let vfs = VfsBuilder::new()
            .mount_memory("bz2.pk3", zip)
            .unwrap()
            .build()
            .unwrap();
        let id = vfs.find("BZ2", Namespace::Graphics).unwrap();
        assert_eq!(vfs.read(id).unwrap().as_ref(), b"bzip2-hello");
    }

    #[test]
    fn pk3_lzma_entry_reads() {
        let bytes = include_bytes!("../../../testdata/vfs/lzma_hello.pk3");
        let vfs = VfsBuilder::new()
            .mount_memory("lzma.pk3", bytes.to_vec())
            .unwrap()
            .build()
            .unwrap();
        let id = vfs.find("HELLO", Namespace::Graphics).unwrap();
        assert_eq!(vfs.read(id).unwrap().as_ref(), b"lzma-hello");
    }
}
