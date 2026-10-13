//! MODELDEF path → mesh cache and actor wiring helpers (M10).

use crate::models::{ModelError, ModelMesh, append_model, parse_model_bytes};
use crate::scene::SceneLists;
use crate::ue1::parse_ue1;
use room_defs::ModelDefs;
use room_vfs::{Namespace, Vfs};
use glam::Vec3;
use std::collections::BTreeMap;

/// Cached meshes keyed by resolved path (lowercase).
#[derive(Clone, Debug, Default)]
pub struct ModelCatalog {
    /// Path → mesh.
    meshes: BTreeMap<String, ModelMesh>,
}

impl ModelCatalog {
    /// Empty catalog.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Load (or return cached) mesh for `path` (MODELDEF `Path` + model file).
    pub fn load(&mut self, vfs: &Vfs, path: &str) -> Result<&ModelMesh, ModelError> {
        let key = path.replace('\\', "/").to_ascii_lowercase();
        if !self.meshes.contains_key(&key) {
            let mesh = if key.ends_with("_d.3d") || key.ends_with("_a.3d") {
                load_ue1_pair(vfs, &key)?
            } else {
                let data = read_model_bytes(vfs, &key)?;
                parse_model_bytes(&data, &key)?
            };
            self.meshes.insert(key.clone(), mesh);
        }
        Ok(self.meshes.get(&key).expect("inserted above"))
    }

    /// Cached mesh if already loaded.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&ModelMesh> {
        self.meshes
            .get(&path.replace('\\', "/").to_ascii_lowercase())
    }
}

/// Join MODELDEF `Path` + model filename.
#[must_use]
pub fn join_model_path(base: &str, file: &str) -> String {
    let base = base.replace('\\', "/").trim_end_matches('/').to_string();
    let file = file.replace('\\', "/");
    if base.is_empty() {
        file
    } else if file.starts_with('/') {
        file
    } else {
        format!("{base}/{file}")
    }
}

fn read_model_bytes(vfs: &Vfs, path: &str) -> Result<Vec<u8>, ModelError> {
    let name = path.rsplit('/').next().unwrap_or(path);
    for candidate in [path, name] {
        if let Some(id) = vfs
            .find(candidate, Namespace::Global)
            .or_else(|| vfs.find_any_ns(candidate))
        {
            let bytes = vfs
                .read(id)
                .map_err(|e| ModelError::Message(e.to_string()))?;
            return Ok(bytes.to_vec());
        }
    }
    Err(ModelError::Message(format!("model not in VFS: `{path}`")))
}

/// Load UE1 pair (`*_d.3d` + `*_a.3d`); returns frame 0 mesh.
fn load_ue1_pair(vfs: &Vfs, path: &str) -> Result<ModelMesh, ModelError> {
    let (d_path, a_path) = if path.ends_with("_d.3d") {
        (path.to_string(), path.replacen("_d.3d", "_a.3d", 1))
    } else if path.ends_with("_a.3d") {
        (path.replacen("_a.3d", "_d.3d", 1), path.to_string())
    } else {
        return Err(ModelError::Message("UE1 path must end _d.3d/_a.3d".into()));
    };
    let d = read_model_bytes(vfs, &d_path)?;
    let a = read_model_bytes(vfs, &a_path)?;
    let model = parse_ue1(&d, &a)?;
    model
        .frame_mesh(0)
        .cloned()
        .ok_or_else(|| ModelError::Message("UE1 has no frames".into()))
}

/// One actor pose for MODELDEF draw.
#[derive(Clone, Debug)]
pub struct ModelActorPose {
    /// Class name (MODELDEF key).
    pub class: String,
    /// World origin (feet).
    pub origin: Vec3,
    /// Sector light 0..=1.
    pub light: f32,
}

/// Append MODELDEF meshes for posed actors into the scene (unlit bootstrap).
pub fn append_actor_models(
    lists: &mut SceneLists,
    catalog: &mut ModelCatalog,
    vfs: &Vfs,
    defs: &ModelDefs,
    poses: &[ModelActorPose],
) {
    for pose in poses {
        let key = pose.class.to_ascii_lowercase();
        let Some(def) = defs.by_actor.get(&key) else {
            continue;
        };
        let Some(file) = def.models.get(&0) else {
            continue;
        };
        let full = join_model_path(&def.path, file);
        let scale = Vec3::from_array(def.scale);
        let origin =
            pose.origin + Vec3::new(def.offset[0], def.offset[1], def.offset[2] + def.z_offset);
        let skin = def
            .skins
            .get(&0)
            .cloned()
            .unwrap_or_else(|| "__MODEL".into());
        let Ok(mesh) = catalog.load(vfs, &full) else {
            continue;
        };
        append_model(lists, mesh, origin, scale, pose.light, &skin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_paths() {
        assert_eq!(
            join_model_path("models/doom", "imp.md3"),
            "models/doom/imp.md3"
        );
        assert_eq!(join_model_path("", "x.obj"), "x.obj");
    }
}
