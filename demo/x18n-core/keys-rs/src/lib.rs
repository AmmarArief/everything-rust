//! JS bindings for `packages/core/src/keys.ts`: generated from the signatures in `port.rs` by carcinize.
//! Regenerate with `carcinize refresh` after changing them.

use wasm_bindgen::prelude::*;

pub mod js;
pub mod port;
pub mod utf16;

/// `joinKey`.
fn join_key_inner(namespace: Option<String>, path: Vec<String>) -> Result<String, String> {
    let out = port::join_key(namespace, path);
    Ok(out)
}

#[wasm_bindgen(js_name = "joinKey")]
pub fn join_key(namespace: Option<String>, path: Vec<String>) -> Result<String, JsError> {
    join_key_inner(namespace, path).map_err(|e| JsError::new(&e))
}

/// `splitKey`.
fn split_key_inner(key: String, namespaced: Option<bool>) -> Result<String, String> {
    let out = port::split_key(key, namespaced);
    crate::js::to_json(&out)
}

#[wasm_bindgen(js_name = "splitKey")]
pub fn split_key(key: String, namespaced: Option<bool>) -> Result<String, JsError> {
    split_key_inner(key, namespaced).map_err(|e| JsError::new(&e))
}

/// `isKeyUnder`.
fn is_key_under_inner(key: String, prefix: String) -> Result<bool, String> {
    let out = port::is_key_under(key, prefix);
    Ok(out)
}

#[wasm_bindgen(js_name = "isKeyUnder")]
pub fn is_key_under(key: String, prefix: String) -> Result<bool, JsError> {
    is_key_under_inner(key, prefix).map_err(|e| JsError::new(&e))
}

/// `replaceKeyPrefix`.
fn replace_key_prefix_inner(key: String, from: String, to: String) -> Result<Vec<u16>, String> {
    let out = port::replace_key_prefix(key, from, to);
    Ok(out)
}

#[wasm_bindgen(js_name = "replaceKeyPrefix")]
pub fn replace_key_prefix(key: String, from: String, to: String) -> Result<Vec<u16>, JsError> {
    replace_key_prefix_inner(key, from, to).map_err(|e| JsError::new(&e))
}
