//! Thin byte-in/data-out WebAssembly bindings. Loading is owned by the host.
use metafile::{inspect, to_svg, RenderOptions};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorPayload {
    code: &'static str,
    message: String,
    offset: Option<usize>,
    record_index: Option<usize>,
    record_type: Option<u32>,
    restore_dc_value: Option<i32>,
}
fn js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value)
        .map_err(|e| structured_error("serialization_error", &format!("serialization failed: {e}")))
}
fn structured_error(code: &str, message: &str) -> JsValue {
    let object = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &object,
        &JsValue::from_str("code"),
        &JsValue::from_str(code),
    );
    let _ = js_sys::Reflect::set(
        &object,
        &JsValue::from_str("message"),
        &JsValue::from_str(message),
    );
    object.into()
}
fn error(e: metafile_core::MetafileError) -> JsValue {
    let restore_dc_value = match &e {
        metafile_core::MetafileError::InvalidRestoreDc { value, .. } => Some(*value),
        _ => None,
    };
    let (offset, record_index, record_type) = match &e {
        metafile_core::MetafileError::TruncatedInput { offset, .. }
        | metafile_core::MetafileError::InvalidRecordSize { offset, .. }
        | metafile_core::MetafileError::RecordOutOfBounds { offset, .. } => {
            (Some(*offset), None, None)
        }
        metafile_core::MetafileError::InvalidObjectHandle { record_index, .. }
        | metafile_core::MetafileError::ObjectInUse { record_index, .. }
        | metafile_core::MetafileError::InvalidRestoreDc { record_index, .. }
        | metafile_core::MetafileError::InvalidBitmap { record_index, .. }
        | metafile_core::MetafileError::UnsupportedBitmap { record_index, .. } => {
            (None, Some(*record_index), None)
        }
        metafile_core::MetafileError::InvalidEmfPlus {
            offset,
            outer_record_index,
            record_type,
            ..
        } => (
            Some(*offset),
            Some(*outer_record_index),
            Some(u32::from(*record_type)),
        ),
        _ => (None, None, None),
    };
    let code = match &e {
        metafile_core::MetafileError::UnsupportedFormat => "unsupported_format",
        metafile_core::MetafileError::FormatMismatch { .. } => "format_mismatch",
        metafile_core::MetafileError::InvalidHeader(_) => "invalid_header",
        metafile_core::MetafileError::InvalidPlaceableHeader(_) => "invalid_placeable_header",
        metafile_core::MetafileError::InvalidChecksum { .. } => "invalid_checksum",
        metafile_core::MetafileError::TruncatedInput { .. } => "truncated_input",
        metafile_core::MetafileError::InvalidRecordSize { .. } => "invalid_record_size",
        metafile_core::MetafileError::RecordOutOfBounds { .. } => "record_out_of_bounds",
        metafile_core::MetafileError::ResourceLimitExceeded { .. } => "resource_limit",
        metafile_core::MetafileError::InvalidObjectHandle { .. } => "invalid_object_handle",
        metafile_core::MetafileError::ObjectInUse { .. } => "object_in_use",
        metafile_core::MetafileError::InvalidRestoreDc { .. } => "invalid_restore_dc",
        metafile_core::MetafileError::InvalidBitmap { .. } => "invalid_bitmap",
        metafile_core::MetafileError::UnsupportedBitmap { .. } => "unsupported_bitmap",
        metafile_core::MetafileError::UnsupportedCriticalFeature(_) => "unsupported_feature",
        metafile_core::MetafileError::InvalidEmfPlus { .. } => "invalid_emf_plus",
        metafile_core::MetafileError::SvgGeneration(_) => "svg_generation",
    };
    js(&ErrorPayload {
        code,
        message: e.to_string(),
        offset,
        record_index,
        record_type,
        restore_dc_value,
    })
    .unwrap_or_else(|_| structured_error(code, &e.to_string()))
}
#[wasm_bindgen(js_name = inspectWmf)]
pub fn inspect_wmf(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let info = inspect(bytes).map_err(error)?;
    if info.format != metafile_core::MetafileFormat::Wmf {
        return Err(error(metafile_core::MetafileError::FormatMismatch {
            expected: metafile_core::MetafileFormat::Wmf,
            actual: info.format,
        }));
    }
    js(&info)
}
#[wasm_bindgen(js_name = wmfToSvg)]
pub fn wmf_to_svg(bytes: &[u8], options: Option<JsValue>) -> Result<JsValue, JsValue> {
    let info = inspect(bytes).map_err(error)?;
    if info.format != metafile_core::MetafileFormat::Wmf {
        return Err(error(metafile_core::MetafileError::FormatMismatch {
            expected: metafile_core::MetafileFormat::Wmf,
            actual: info.format,
        }));
    }
    metafile_to_svg(bytes, options)
}

#[wasm_bindgen(js_name = inspectMetafile)]
pub fn inspect_metafile(bytes: &[u8]) -> Result<JsValue, JsValue> {
    inspect(bytes).map_err(error).and_then(|v| js(&v))
}

#[wasm_bindgen(js_name = metafileToSvg)]
pub fn metafile_to_svg(bytes: &[u8], options: Option<JsValue>) -> Result<JsValue, JsValue> {
    let options = options.map_or_else(
        || Ok(RenderOptions::default()),
        |v| {
            serde_wasm_bindgen::from_value(v)
                .map_err(|e| structured_error("invalid_options", &format!("invalid options: {e}")))
        },
    )?;
    to_svg(bytes, options).map_err(error).and_then(|v| js(&v))
}
