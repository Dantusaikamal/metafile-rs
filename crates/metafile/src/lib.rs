//! High-level composition of independent metafile playback and SVG crates.

use metafile_core::Result;
pub use metafile_core::{MetafileFormat, RenderOptions};
pub use metafile_emf::EmfInfo;
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
    Emf(EmfInfo),
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

    fn from_emf(byte_length: usize, emf: EmfInfo) -> Self {
        Self {
            format: emf.format,
            byte_length,
            bounds: Some(emf.bounds),
            estimated_width: (emf.frame.width() > 0.0).then(|| emf.frame.width() * 96.0 / 2540.0),
            estimated_height: (emf.frame.height() > 0.0)
                .then(|| emf.frame.height() * 96.0 / 2540.0),
            record_count: Some(emf.parsed_record_count),
            warnings: emf.warnings.clone(),
            details: FormatSpecificInfo::Emf(emf),
        }
    }
}

pub fn inspect(bytes: &[u8]) -> Result<MetafileInfo> {
    inspect_with_options(bytes, &RenderOptions::default())
}

pub fn inspect_with_options(bytes: &[u8], options: &RenderOptions) -> Result<MetafileInfo> {
    if is_emf(bytes) {
        metafile_emf::inspect_with_options(bytes, options)
            .map(|emf| MetafileInfo::from_emf(bytes.len(), emf))
    } else {
        metafile_wmf::inspect_with_options(bytes, options)
            .map(|wmf| MetafileInfo::from_wmf(bytes.len(), wmf))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderResult {
    pub svg: String,
    pub metadata: MetafileInfo,
    pub diagnostics: Vec<metafile_core::Diagnostic>,
}

pub fn to_svg(bytes: &[u8], options: RenderOptions) -> Result<RenderResult> {
    let mut renderer = SvgRenderer::new(options.limits.clone());
    if is_emf(bytes) {
        let result = metafile_emf::playback(bytes, &options, &mut renderer)?;
        let svg = renderer.finish(result.output_bounds, result.physical_size)?;
        Ok(RenderResult {
            svg,
            metadata: MetafileInfo::from_emf(bytes.len(), result.metadata),
            diagnostics: result.diagnostics,
        })
    } else {
        let result = metafile_wmf::playback(bytes, &options, &mut renderer)?;
        let svg = renderer.finish(result.output_bounds, result.physical_size)?;
        Ok(RenderResult {
            svg,
            metadata: MetafileInfo::from_wmf(bytes.len(), result.metadata),
            diagnostics: result.diagnostics,
        })
    }
}

fn is_emf(bytes: &[u8]) -> bool {
    bytes.len() >= 44
        && u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) == 1
        && &bytes[40..44] == b" EMF"
}
