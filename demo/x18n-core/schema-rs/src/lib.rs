//! JS bindings for `packages/core/src/schema.ts`: generated from the signatures in `port.rs` by carcinize.
//! Regenerate with `carcinize refresh` after changing them.

use wasm_bindgen::prelude::*;

pub mod js;
pub mod port;

/// `isJsonObject`.
fn is_json_object_inner(value: String) -> Result<bool, String> {
    let value = crate::js::from_json(&value)?;
    let out = port::is_json_object(value);
    Ok(out)
}

#[wasm_bindgen(js_name = "isJsonObject")]
pub fn is_json_object(value: String) -> Result<bool, JsError> {
    is_json_object_inner(value).map_err(|e| JsError::new(&e))
}
