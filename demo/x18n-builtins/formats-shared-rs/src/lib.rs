//! JS bindings for `packages/builtins/src/formats/shared/`: generated from the signatures in `port.rs` by carcinize.
//! Regenerate with `carcinize refresh` after changing them.

use wasm_bindgen::prelude::*;

pub mod js;
pub mod port;
pub mod utf16;

/// `indentUnit`.
fn indent_unit_inner(text: String, indent: String, fallback: String) -> Result<String, String> {
    let indent = crate::js::from_json(&indent)?;
    let out = port::indent_unit(text, indent, fallback)?;
    Ok(out)
}

#[wasm_bindgen(js_name = "indentUnit")]
pub fn indent_unit(text: String, indent: String, fallback: String) -> Result<String, JsError> {
    indent_unit_inner(text, indent, fallback).map_err(|e| JsError::new(&e))
}

/// `detectEol`.
fn detect_eol_inner(text: String) -> Result<String, String> {
    let out = port::detect_eol(text)?;
    Ok(out)
}

#[wasm_bindgen(js_name = "detectEol")]
pub fn detect_eol(text: String) -> Result<String, JsError> {
    detect_eol_inner(text).map_err(|e| JsError::new(&e))
}

/// `leafText`.
fn leaf_text_inner(value: String) -> Result<String, String> {
    let value = crate::js::from_json(&value)?;
    let out = port::leaf_text(value)?;
    crate::js::to_json(&out)
}

#[wasm_bindgen(js_name = "leafText")]
pub fn leaf_text(value: String) -> Result<String, JsError> {
    leaf_text_inner(value).map_err(|e| JsError::new(&e))
}

/// `splice`.
fn splice_inner(text: String, range: String, insert: String) -> Result<String, String> {
    let range = crate::js::from_json(&range)?;
    let out = port::splice(text, range, insert)?;
    crate::js::to_json(&out)
}

#[wasm_bindgen(js_name = "splice")]
pub fn splice(text: String, range: String, insert: String) -> Result<String, JsError> {
    splice_inner(text, range, insert).map_err(|e| JsError::new(&e))
}

/// `appendLines`.
fn append_lines_inner(text: String, chunk: String) -> Result<String, String> {
    let out = port::append_lines(text, chunk)?;
    Ok(out)
}

#[wasm_bindgen(js_name = "appendLines")]
pub fn append_lines(text: String, chunk: String) -> Result<String, JsError> {
    append_lines_inner(text, chunk).map_err(|e| JsError::new(&e))
}

/// `splitLines`.
fn split_lines_inner(text: String) -> Result<String, String> {
    let out = port::split_lines(text)?;
    crate::js::to_json(&out)
}

#[wasm_bindgen(js_name = "splitLines")]
pub fn split_lines(text: String) -> Result<String, JsError> {
    split_lines_inner(text).map_err(|e| JsError::new(&e))
}

/// `withBlankLines`.
fn with_blank_lines_inner(text: String, range: String) -> Result<String, String> {
    let range = crate::js::from_json(&range)?;
    let out = port::with_blank_lines(text, range)?;
    crate::js::to_json(&out)
}

#[wasm_bindgen(js_name = "withBlankLines")]
pub fn with_blank_lines(text: String, range: String) -> Result<String, JsError> {
    with_blank_lines_inner(text, range).map_err(|e| JsError::new(&e))
}
