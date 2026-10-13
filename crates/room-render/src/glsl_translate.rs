//! Custom GLSL → WGSL via naga (GLDEFS `HardwareShader` bootstrap).

use thiserror::Error;

/// GLSL translation / validation errors.
#[derive(Debug, Error)]
pub enum GlslError {
    /// Parse or validate failure.
    #[error("{0}")]
    Message(String),
}

/// Translate a GLSL fragment (or vertex) shader string to WGSL.
///
/// `stage` is `"fragment"` or `"vertex"`. Uses naga's GLSL frontend + WGSL
/// backend — load-time path for GLDEFS `HardwareShader` (docs/13 §10).
pub fn glsl_to_wgsl(source: &str, stage: &str) -> Result<String, GlslError> {
    let naga_stage = match stage.to_ascii_lowercase().as_str() {
        "fragment" | "frag" | "fs" | "pixel" => naga::ShaderStage::Fragment,
        "vertex" | "vert" | "vs" => naga::ShaderStage::Vertex,
        "compute" | "cs" => naga::ShaderStage::Compute,
        other => {
            return Err(GlslError::Message(format!(
                "unknown shader stage `{other}`"
            )));
        }
    };
    let options = naga::front::glsl::Options {
        stage: naga_stage,
        defines: Default::default(),
    };
    let mut frontend = naga::front::glsl::Frontend::default();
    let module = frontend
        .parse(&options, source)
        .map_err(|e| GlslError::Message(format!("GLSL parse: {e}")))?;
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .map_err(|e| GlslError::Message(format!("GLSL validate: {e:#?}")))?;
    let wgsl =
        naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
            .map_err(|e| GlslError::Message(format!("WGSL emit: {e}")))?;
    Ok(wgsl)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn translates_trivial_fragment() {
        let glsl = r#"
#version 450
layout(location = 0) out vec4 fragColor;
void main() {
    fragColor = vec4(1.0, 0.0, 0.0, 1.0);
}
"#;
        let wgsl = glsl_to_wgsl(glsl, "fragment").unwrap();
        assert!(
            wgsl.contains("fn main") || wgsl.contains("fragColor") || wgsl.contains("vec4"),
            "unexpected WGSL:\n{wgsl}"
        );
    }

    #[test]
    fn rejects_bad_glsl() {
        assert!(glsl_to_wgsl("not a shader!!!", "fragment").is_err());
    }
}
