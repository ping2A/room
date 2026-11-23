//! `GET /api/presets/{id}/thumbnail.png?map=MAP01&w=320&h=240`: a map's
//! thumbnail drawn by the software renderer with no GPU (`docs/40` SR10).
//!
//! Built only with the `thumbnails` feature; without it the route answers
//! 501. Doom presets only (the Duke presets have no Doom map to draw).

use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

/// Query of the thumbnail route.
#[derive(Debug, Default, Deserialize)]
pub struct ThumbQuery {
    /// Map lump (default: the preset's default map).
    pub map: Option<String>,
    /// Width in pixels (64–640, default 320).
    pub w: Option<u32>,
    /// Height in pixels (48–480, default 240).
    pub h: Option<u32>,
}

/// A map lump name: 1–8 ASCII letters, digits or `_`.
fn valid_map(name: &str) -> bool {
    (1..=8).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// What to draw: the library paths, map and size, plus a cache key that
/// changes with the files' fingerprints.
#[cfg_attr(not(feature = "thumbnails"), allow(dead_code))]
struct Job {
    files: Vec<std::path::PathBuf>,
    map: String,
    w: u32,
    h: u32,
    key: String,
}

fn job(state: &AppState, id: &str, q: &ThumbQuery) -> Result<Job, StatusCode> {
    let content = &state.registry.config().content;
    let cfg = content.presets();
    let preset = cfg
        .presets
        .iter()
        .find(|p| p.id == id)
        .ok_or(StatusCode::NOT_FOUND)?;
    if preset.game != "doom" {
        return Err(StatusCode::NOT_FOUND);
    }
    let map = q
        .map
        .clone()
        .or_else(|| (!preset.default_map.is_empty()).then(|| preset.default_map.clone()))
        .or_else(|| preset.maps.first().cloned())
        .unwrap_or_else(|| "MAP01".into())
        .to_ascii_uppercase();
    if !valid_map(&map) || (!preset.maps.is_empty() && !preset.maps.contains(&map)) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let w = q.w.unwrap_or(320).clamp(64, 640);
    let h = q.h.unwrap_or(240).clamp(48, 480);
    let mut files = Vec::new();
    let mut key = format!("{id}|{map}|{w}x{h}");
    for file in preset.iwad.iter().chain(&preset.content) {
        let entry = content.get(file).ok_or(StatusCode::NOT_FOUND)?;
        files.push(content.path_of(file).ok_or(StatusCode::NOT_FOUND)?);
        key.push('|');
        key.push_str(&entry.fingerprint);
    }
    if files.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(Job {
        files,
        map,
        w,
        h,
        key,
    })
}

#[cfg(feature = "thumbnails")]
mod draw {
    use super::Job;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex, OnceLock};

    /// Recent thumbnails, newest last.
    const CACHE: usize = 32;

    fn cache() -> &'static Mutex<VecDeque<(String, Arc<Vec<u8>>)>> {
        static C: OnceLock<Mutex<VecDeque<(String, Arc<Vec<u8>>)>>> = OnceLock::new();
        C.get_or_init(|| Mutex::new(VecDeque::new()))
    }

    pub(super) fn cached(key: &str) -> Option<Arc<Vec<u8>>> {
        let c = cache().lock().unwrap_or_else(|p| p.into_inner());
        c.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    pub(super) fn render(job: &Job) -> Result<Arc<Vec<u8>>, String> {
        let mut b = room_vfs::VfsBuilder::new();
        for f in &job.files {
            b = b.mount_path(f).map_err(|e| e.to_string())?;
        }
        let vfs = b.build().map_err(|e| e.to_string())?;
        let png = Arc::new(room_render_soft::shot::map_thumbnail(
            &vfs, &job.map, job.w, job.h,
        )?);
        let mut c = cache().lock().unwrap_or_else(|p| p.into_inner());
        c.retain(|(k, _)| *k != job.key);
        c.push_back((job.key.clone(), png.clone()));
        while c.len() > CACHE {
            c.pop_front();
        }
        Ok(png)
    }
}

/// The route handler.
pub async fn preset_thumbnail(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ThumbQuery>,
) -> Response {
    let job = match job(&state, &id, &q) {
        Ok(j) => j,
        Err(code) => return code.into_response(),
    };
    render(job).await
}

#[cfg(feature = "thumbnails")]
async fn render(job: Job) -> Response {
    let png = match draw::cached(&job.key) {
        Some(p) => p,
        None => match tokio::task::spawn_blocking(move || draw::render(&job)).await {
            Ok(Ok(p)) => p,
            Ok(Err(e)) => {
                tracing::warn!(%e, "thumbnail");
                return (StatusCode::UNPROCESSABLE_ENTITY, e).into_response();
            }
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        },
    };
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        png.as_ref().clone(),
    )
        .into_response()
}

#[cfg(not(feature = "thumbnails"))]
async fn render(_job: Job) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        [(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")],
        "built without the `thumbnails` feature",
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::valid_map;

    #[test]
    fn map_names() {
        assert!(valid_map("MAP01"));
        assert!(valid_map("E1M1"));
        assert!(!valid_map(""));
        assert!(!valid_map("../x"));
        assert!(!valid_map("TOOLONGNAME"));
    }
}
