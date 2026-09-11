//! High-level composition of the independent WMF playback and SVG crates.

pub use metafile_core::RenderOptions;
use metafile_core::Result;
use metafile_svg::SvgRenderer;
pub use metafile_wmf::{inspect, inspect_with_options, WmfInfo};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderResult {
    pub svg: String,
    pub metadata: WmfInfo,
    pub diagnostics: Vec<metafile_core::Diagnostic>,
}

pub fn to_svg(bytes: &[u8], options: RenderOptions) -> Result<RenderResult> {
    let mut renderer = SvgRenderer::new(options.limits.clone());
    let result = metafile_wmf::playback(bytes, &options, &mut renderer)?;
    let svg = renderer.finish(result.output_bounds, result.physical_size)?;
    Ok(RenderResult {
        svg,
        metadata: result.metadata,
        diagnostics: result.diagnostics,
    })
}
