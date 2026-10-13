//! Embedded UDMF demo room for the web viewer (no IWAD required).

use ferrite_map::{MapGeometry, load_map};
use ferrite_vfs::{VfsBuilder, WadWriter};

/// Minimal square room as an in-memory UDMF PWAD.
pub fn load_embedded_demo_map() -> Result<MapGeometry, String> {
    let text = r#"namespace = "zdoom";
sector {
  heightfloor = 0;
  heightceiling = 128;
  texturefloor = "FLOOR0_1";
  textureceiling = "CEIL1_1";
  lightlevel = 192;
}
vertex { x = 0.0; y = 0.0; }
vertex { x = 256.0; y = 0.0; }
vertex { x = 256.0; y = 256.0; }
vertex { x = 0.0; y = 256.0; }
sidedef { sector = 0; texturemiddle = "STARTAN2"; }
sidedef { sector = 0; texturemiddle = "STARTAN2"; }
sidedef { sector = 0; texturemiddle = "STARTAN2"; }
sidedef { sector = 0; texturemiddle = "STARTAN2"; }
linedef { v1 = 0; v2 = 1; sidefront = 0; }
linedef { v1 = 1; v2 = 2; sidefront = 1; }
linedef { v1 = 2; v2 = 3; sidefront = 2; }
linedef { v1 = 3; v2 = 0; sidefront = 3; }
thing { x = 128.0; y = 128.0; angle = 90; type = 1; }
"#;
    let wad = WadWriter::pwad()
        .add("MAP01", Vec::new())
        .add("TEXTMAP", text.as_bytes().to_vec())
        .add("ENDMAP", Vec::new())
        .finish()
        .map_err(|e| e.to_string())?;
    let vfs = VfsBuilder::new()
        .mount_memory("webroom.wad", wad)
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;
    load_map(&vfs, "MAP01").map_err(|e| e.to_string())
}
