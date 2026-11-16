//! JS bindings for `packages/core/src/text.ts`: generated from the signatures in `port.rs` by carcinize.
//! Regenerate with `carcinize refresh` after changing them.

use wasm_bindgen::prelude::*;

pub mod js;
pub mod port;
pub mod utf16;

/// `diffText`.
fn diff_text_inner(before: String, after: String) -> Result<String, String> {
    let out = port::diff_text(before, after);
    crate::js::to_json(&out)
}

#[wasm_bindgen(js_name = "diffText")]
pub fn diff_text(before: String, after: String) -> Result<String, JsError> {
    diff_text_inner(before, after).map_err(|e| JsError::new(&e))
}

/// `applyTextEdits`.
fn apply_text_edits_inner(text: String, edits: String) -> Result<String, String> {
    let edits = crate::js::from_json(&edits)?;
    let out = port::apply_text_edits(text, edits)?;
    crate::js::to_json(&out)
}

#[wasm_bindgen(js_name = "applyTextEdits")]
pub fn apply_text_edits(text: String, edits: String) -> Result<String, JsError> {
    apply_text_edits_inner(text, edits).map_err(|e| JsError::new(&e))
}

/// `rangeContains`.
fn range_contains_inner(range: String, offset: f64) -> Result<bool, String> {
    let range = crate::js::from_json(&range)?;
    let out = port::range_contains(range, offset)?;
    Ok(out)
}

#[wasm_bindgen(js_name = "rangeContains")]
pub fn range_contains(range: String, offset: f64) -> Result<bool, JsError> {
    range_contains_inner(range, offset).map_err(|e| JsError::new(&e))
}

/// `hashText`.
fn hash_text_inner(text: String) -> Result<String, String> {
    let out = port::hash_text(text);
    Ok(out)
}

#[wasm_bindgen(js_name = "hashText")]
pub fn hash_text(text: String) -> Result<String, JsError> {
    hash_text_inner(text).map_err(|e| JsError::new(&e))
}
