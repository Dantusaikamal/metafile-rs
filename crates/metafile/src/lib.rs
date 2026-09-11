//! High-level composition of the independent WMF playback and SVG crates.

use metafile_core::Result;
pub use metafile_core::{MetafileFormat, RenderOptions};
use metafile_svg::SvgRenderer;
pub use metafile_wmf::WmfInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetafileInfo {
    pub format: MetafileFormat,
    pub byte_length: usize,
    pub bounds: Option<metafile_core::Rect>,
    pub estimated_width: Option<f64>,
    pub estimated_height: Option<f64>,
    pub record_count: Option<u32>,
    pub warnings: Vec<metafile_core::Diagnostic>,
    pub details: FormatSpecificInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "format", content = "metadata", rename_all = "lowercase")]
pub enum FormatSpecificInfo {
    Wmf(WmfInfo),
}

impl MetafileInfo {
    fn from_wmf(byte_length: usize, wmf: WmfInfo) -> Self {
        Self {
            format: wmf.format,
            byte_length,
            bounds: wmf.bounds,
            estimated_width: wmf.estimated_width,
            estimated_height: wmf.estimated_height,
            record_count: Some(wmf.parsed_record_count),
            warnings: wmf.warnings.clone(),
            details: FormatSpecificInfo::Wmf(wmf),
        }
    }
}

pub fn inspect(bytes: &[u8]) -> Result<MetafileInfo> {
    inspect_with_options(bytes, &RenderOptions::default())
}

pub fn inspect_with_options(bytes: &[u8], options: &RenderOptions) -> Result<MetafileInfo> {
    reject_enhanced_metafile(bytes)?;
    metafile_wmf::inspect_with_options(bytes, options)
        .map(|wmf| MetafileInfo::from_wmf(bytes.len(), wmf))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderResult {
    pub svg: String,
    pub metadata: MetafileInfo,
    pub diagnostics: Vec<metafile_core::Diagnostic>,
}

pub fn to_svg(bytes: &[u8], options: RenderOptions) -> Result<RenderResult> {
    reject_enhanced_metafile(bytes)?;
    let mut renderer = SvgRenderer::new(options.limits.clone());
    let result = metafile_wmf::playback(bytes, &options, &mut renderer)?;
    let svg = renderer.finish(result.output_bounds, result.physical_size)?;
    Ok(RenderResult {
        svg,
        metadata: MetafileInfo::from_wmf(bytes.len(), result.metadata),
        diagnostics: result.diagnostics,
    })
}

fn reject_enhanced_metafile(bytes: &[u8]) -> Result<()> {
    if bytes.len() >= 44
        && u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) == 1
        && &bytes[40..44] == b" EMF"
    {
        return Err(metafile_core::MetafileError::UnsupportedFormat);
    }
    Ok(())
}
