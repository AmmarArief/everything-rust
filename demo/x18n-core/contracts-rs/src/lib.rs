//! JS bindings for `packages/core/src/contracts.ts`: generated from the signatures in `port.rs` by carcinize.
//! Regenerate with `carcinize refresh` after changing them.

use wasm_bindgen::prelude::*;

pub mod js;
pub mod port;

/// `toWireRange`.
fn to_wire_range_inner(text: Option<String>, start: f64, end: f64) -> Result<String, String> {
    let out = port::to_wire_range(text, start, end);
    crate::js::to_json(&out)
}

#[wasm_bindgen(js_name = "toWireRange")]
pub fn to_wire_range(text: Option<String>, start: f64, end: f64) -> Result<String, JsError> {
    to_wire_range_inner(text, start, end).map_err(|e| JsError::new(&e))
}
