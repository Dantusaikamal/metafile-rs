//! High-level Windows Metafile inspection and deterministic SVG rendering.
//!
//! This crate provides the format-neutral entry points for WMF, ordinary EMF,
//! EMF+ Only, and EMF+ Dual input. It accepts borrowed bytes, enforces resource
//! limits during playback, and returns self-contained SVG with metadata and
//! diagnostics.
//!
//! # Example
//!
//! ```no_run
//! use metafile::{inspect, to_svg, RenderOptions};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let bytes = std::fs::read("drawing.emf")?;
//! let info = inspect(&bytes)?;
//! println!("detected {:?}", info.format);
//!
//! let rendered = to_svg(&bytes, RenderOptions::default())?;
//! std::fs::write("drawing.svg", rendered.svg)?;
//! # Ok(())
//! # }
//! ```
//!
//! Rendering is deterministic and does not invoke platform graphics APIs,
//! external converters, a browser DOM, or the network. Exact Windows font
//! metrics and uncommon historical records may be approximate or unsupported;
//! those cases are reported through diagnostics or typed errors.

pub use metafile_core::{
    Diagnostic, MetafileError, MetafileFormat, Rect, RenderOptions, ResourceLimits, Result,
};
pub use metafile_emf::EmfInfo;
use metafile_svg::SvgRenderer;
pub use metafile_wmf::WmfInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
/// Format-neutral metadata returned by [`inspect`] and [`to_svg`].
pub struct MetafileInfo {
    /// Detected metafile family.
    pub format: MetafileFormat,
    /// Original input size in bytes.
    pub byte_length: usize,
    /// Logical drawing bounds when they can be determined.
    pub bounds: Option<Rect>,
    /// Estimated output width in CSS pixels when physical sizing is known.
    pub estimated_width: Option<f64>,
    /// Estimated output height in CSS pixels when physical sizing is known.
    pub estimated_height: Option<f64>,
    /// Number of parsed outer-format records, when available.
    pub record_count: Option<u32>,
    /// Non-fatal issues found during inspection.
    pub warnings: Vec<Diagnostic>,
    /// Metadata specific to the detected container format.
    pub details: FormatSpecificInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "format", content = "metadata", rename_all = "lowercase")]
#[non_exhaustive]
/// Format-specific metadata nested inside [`MetafileInfo`].
pub enum FormatSpecificInfo {
    /// Windows Metafile metadata.
    Wmf(WmfInfo),
    /// Enhanced Metafile metadata, including any embedded EMF+ classification.
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

/// Inspects WMF, EMF, or EMF+ bytes using default resource limits.
///
/// This validates and classifies the input without producing SVG.
pub fn inspect(bytes: &[u8]) -> Result<MetafileInfo> {
    inspect_with_options(bytes, &RenderOptions::default())
}

/// Inspects metafile bytes with caller-supplied strictness and resource limits.
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
#[non_exhaustive]
/// Successful format-neutral SVG rendering output.
pub struct RenderResult {
    /// Deterministic, self-contained SVG document.
    pub svg: String,
    /// Metadata collected while parsing and playing back the input.
    pub metadata: MetafileInfo,
    /// Non-fatal approximations and unsupported edge cases encountered.
    pub diagnostics: Vec<Diagnostic>,
}

/// Renders WMF, EMF, or EMF+ bytes to a deterministic SVG document.
///
/// Use [`RenderOptions::strict`] to turn material approximations into typed
/// errors. Resource consumption is bounded by [`RenderOptions::limits`].
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
