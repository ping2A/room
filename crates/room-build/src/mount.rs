//! Mount helpers: a Duke Nukem 3D `.GRP` mounts as its converted PWAD, any
//! other path or blob mounts unchanged.

use crate::grp::is_grp;
use crate::wad::grp_to_wad;
use room_vfs::VfsBuilder;
use std::io::Read as _;
use std::path::Path;

/// Mount `path` on `builder`, converting it first when it is a GRP.
pub fn mount_path(builder: VfsBuilder, path: &Path) -> Result<VfsBuilder, String> {
    if path.is_file() && file_is_grp(path) {
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        return mount_bytes(builder, &path.display().to_string(), &data);
    }
    builder.mount_path(path).map_err(|e| e.to_string())
}

/// Mount the bytes of a content file, converting a GRP.
pub fn mount_bytes(builder: VfsBuilder, name: &str, data: &[u8]) -> Result<VfsBuilder, String> {
    if is_grp(data) {
        let wad = grp_to_wad(data).map_err(|e| format!("{name}: {e}"))?;
        return builder
            .mount_memory(format!("{name}.wad"), wad)
            .map_err(|e| e.to_string());
    }
    builder
        .mount_memory(name.to_owned(), data.to_vec())
        .map_err(|e| e.to_string())
}

fn file_is_grp(path: &Path) -> bool {
    let mut head = [0u8; 16];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && is_grp(&head)
}
