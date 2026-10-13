//! Lump and container identifiers / metadata.

use crate::namespace::Namespace;
use crate::short_name::ShortName;
use bitflags::bitflags;

/// Opaque lump index into a [`crate::Vfs`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct LumpId(pub u32);

impl LumpId {
    /// Raw index.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Opaque container index into a [`crate::Vfs`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct ContainerId(pub u32);

impl ContainerId {
    /// Raw index.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

bitflags! {
    /// Lump flags (subset of GZDoom `LUMPF_*`).
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
    pub struct LumpFlags: u32 {
        /// Only full-path lookup (no short name).
        const FULLPATH = 1 << 0;
        /// Candidate flat (F_END flat hack).
        const MAYBE_FLAT = 1 << 1;
        /// Zero-size marker lump.
        const MARKER = 1 << 2;
    }
}

/// Kind of mounted container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerKind {
    /// IWAD or PWAD.
    Wad,
    /// ZIP / PK3.
    Zip,
    /// 7z / PK7.
    SevenZ,
    /// Directory tree.
    Directory,
    /// Single loose file.
    LooseFile,
}

/// Mounted container metadata.
#[derive(Clone, Debug)]
pub struct ContainerInfo {
    /// Display / path name.
    pub name: String,
    /// Container format.
    pub kind: ContainerKind,
    /// MD5 of the container bytes (empty for directories).
    pub md5: [u8; 16],
}

/// One lump entry after mount.
#[derive(Clone, Debug)]
pub struct LumpInfo {
    /// Owning container.
    pub container: ContainerId,
    /// Short name (may be empty when [`LumpFlags::FULLPATH`]).
    pub short_name: ShortName,
    /// Full path using `/` separators, lowercase for lookup; display keeps original when known.
    pub full_path: String,
    /// Namespace.
    pub namespace: Namespace,
    /// Uncompressed size in bytes.
    pub size: u32,
    /// Flags.
    pub flags: LumpFlags,
}
