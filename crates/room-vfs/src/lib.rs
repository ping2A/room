//! Archives, load order, namespaces, and lump lookup.
//!
//! Equivalent of GZDoom's `FileSystem` / `FResourceFile`. See `docs/03-vfs.md`.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub(crate) mod archive;
pub mod autoload;
pub mod error;
pub mod iwad;
pub mod lump;
pub mod namespace;
pub mod short_name;
pub mod source;
pub mod vfs;
pub(crate) mod wad;
pub mod write;

pub use autoload::{AutoloadRequest, autoload_sections, collect_autoload_paths};
pub use error::VfsError;
pub use iwad::{IwadInfo, IwadMatch, IwadRecord, detect_iwad, parse_iwadinfo};
pub use lump::{ContainerId, ContainerInfo, ContainerKind, LumpFlags, LumpId, LumpInfo};
pub use namespace::Namespace;
pub use short_name::ShortName;
pub use source::{ContainerSource, FileSource, MemorySource};
pub use vfs::{Vfs, VfsBuilder, zip_from_files, zip_from_files_with_method};
pub use write::WadWriter;
