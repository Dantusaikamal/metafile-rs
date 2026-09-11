//! Thin byte-in/data-out WebAssembly bindings. Loading is owned by the host.
use metafile_wmf::{inspect, to_svg, RenderOptions};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct ErrorPayload {
    code: &'static str,
    message: String,
    offset: Option<usize>,
    record_index: Option<usize>,
}
fn js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value)
        .map_err(|e| JsValue::from_str(&format!("serialization failed: {e}")))
}
fn error(e: metafile_core::MetafileError) -> JsValue {
    let (offset, record_index) = match &e {
        metafile_core::MetafileError::TruncatedInput { offset, .. }
        | metafile_core::MetafileError::InvalidRecordSize { offset, .. }
        | metafile_core::MetafileError::RecordOutOfBounds { offset, .. } => (Some(*offset), None),
        metafile_core::MetafileError::InvalidObjectHandle { record_index, .. }
        | metafile_core::MetafileError::InvalidBitmap { record_index, .. } => {
            (None, Some(*record_index))
        }
        _ => (None, None),
    };
    let code = match &e {
        metafile_core::MetafileError::UnsupportedFormat => "unsupported_format",
        metafile_core::MetafileError::InvalidHeader(_) => "invalid_header",
        metafile_core::MetafileError::InvalidPlaceableHeader(_) => "invalid_placeable_header",
        metafile_core::MetafileError::InvalidChecksum { .. } => "invalid_checksum",
        metafile_core::MetafileError::TruncatedInput { .. } => "truncated_input",
        metafile_core::MetafileError::InvalidRecordSize { .. } => "invalid_record_size",
        metafile_core::MetafileError::RecordOutOfBounds { .. } => "record_out_of_bounds",
        metafile_core::MetafileError::ResourceLimitExceeded { .. } => "resource_limit",
        metafile_core::MetafileError::InvalidObjectHandle { .. } => "invalid_object_handle",
        metafile_core::MetafileError::InvalidBitmap { .. } => "invalid_bitmap",
        metafile_core::MetafileError::UnsupportedCriticalFeature(_) => "unsupported_feature",
        metafile_core::MetafileError::SvgGeneration(_) => "svg_generation",
    };
    js(&ErrorPayload {
        code,
        message: e.to_string(),
        offset,
        record_index,
    })
    .unwrap_or_else(|_| JsValue::from_str(&e.to_string()))
}
#[wasm_bindgen(js_name = inspectWmf)]
pub fn inspect_wmf(bytes: &[u8]) -> Result<JsValue, JsValue> {
    inspect(bytes).map_err(error).and_then(|v| js(&v))
}
#[wasm_bindgen(js_name = wmfToSvg)]
pub fn wmf_to_svg(bytes: &[u8], options: Option<JsValue>) -> Result<JsValue, JsValue> {
    let options = options.map_or_else(
        || Ok(RenderOptions::default()),
        |v| {
            serde_wasm_bindgen::from_value(v)
                .map_err(|e| JsValue::from_str(&format!("invalid options: {e}")))
        },
    )?;
    to_svg(bytes, options).map_err(error).and_then(|v| js(&v))
}
