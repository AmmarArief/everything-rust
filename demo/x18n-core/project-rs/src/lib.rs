//! JS bindings for `packages/core/src/project/`: generated from the signatures in `port.rs` by carcinize.
//! Regenerate with `carcinize refresh` after changing them.

use wasm_bindgen::prelude::*;

pub mod js;
pub mod port;
pub mod utf16;

/// `isOutdated`.
fn is_outdated_inner(source: String, target: String) -> Result<bool, String> {
    let source = crate::js::from_json(&source)?;
    let target = crate::js::from_json(&target)?;
    let out = port::is_outdated(source, target)?;
    Ok(out)
}

#[wasm_bindgen(js_name = "isOutdated")]
pub fn is_outdated(source: String, target: String) -> Result<bool, JsError> {
    is_outdated_inner(source, target).map_err(|e| JsError::new(&e))
}

/// `valueText`.
fn value_text_inner(value: String) -> Result<String, String> {
    let value = crate::js::from_json(&value)?;
    let out = port::value_text(value);
    crate::js::to_json(&out)
}

#[wasm_bindgen(js_name = "valueText")]
pub fn value_text(value: String) -> Result<String, JsError> {
    value_text_inner(value).map_err(|e| JsError::new(&e))
}

/// `sourceHash`.
fn source_hash_inner(value: String) -> Result<String, String> {
    let value = crate::js::from_json(&value)?;
    let out = port::source_hash(value)?;
    Ok(out)
}

#[wasm_bindgen(js_name = "sourceHash")]
pub fn source_hash(value: String) -> Result<String, JsError> {
    source_hash_inner(value).map_err(|e| JsError::new(&e))
}

/// `findReview`.
fn find_review_inner(data: String, id: String) -> Result<String, String> {
    let data = crate::js::from_json(&data)?;
    let out = port::find_review(data, id)?;
    crate::js::to_json(&out)
}

#[wasm_bindgen(js_name = "findReview")]
pub fn find_review(data: String, id: String) -> Result<String, JsError> {
    find_review_inner(data, id).map_err(|e| JsError::new(&e))
}

/// `sortReviews`.
fn sort_reviews_inner(data: String) -> Result<String, String> {
    let data = crate::js::from_json(&data)?;
    let out = port::sort_reviews(data)?;
    crate::js::to_json(&out)
}

#[wasm_bindgen(js_name = "sortReviews")]
pub fn sort_reviews(data: String) -> Result<String, JsError> {
    sort_reviews_inner(data).map_err(|e| JsError::new(&e))
}

/// `prefixMatches`.
fn prefix_matches_inner(usage: String, key: String) -> Result<bool, String> {
    let usage = crate::js::from_json(&usage)?;
    let out = port::prefix_matches(usage, key)?;
    Ok(out)
}

#[wasm_bindgen(js_name = "prefixMatches")]
pub fn prefix_matches(usage: String, key: String) -> Result<bool, JsError> {
    prefix_matches_inner(usage, key).map_err(|e| JsError::new(&e))
}

/// `suppressedRanges`.
fn suppressed_ranges_inner(text: String) -> Result<String, String> {
    let out = port::suppressed_ranges(text);
    crate::js::to_json(&out)
}

#[wasm_bindgen(js_name = "suppressedRanges")]
pub fn suppressed_ranges(text: String) -> Result<String, JsError> {
    suppressed_ranges_inner(text).map_err(|e| JsError::new(&e))
}

/// `categoriesOf`.
fn categories_of_inner(info: String, kind: String) -> Result<String, String> {
    let info = crate::js::from_json(&info)?;
    let kind = crate::js::from_json(&kind)?;
    let out = port::categories_of(info, kind)?;
    crate::js::to_json(&out)
}

#[wasm_bindgen(js_name = "categoriesOf")]
pub fn categories_of(info: String, kind: String) -> Result<String, JsError> {
    categories_of_inner(info, kind).map_err(|e| JsError::new(&e))
}

/// `renderPrompt`.
fn render_prompt_inner(data: String) -> Result<String, String> {
    let data = crate::js::from_json(&data)?;
    let out = port::render_prompt(data)?;
    crate::js::to_json(&out)
}

#[wasm_bindgen(js_name = "renderPrompt")]
pub fn render_prompt(data: String) -> Result<String, JsError> {
    render_prompt_inner(data).map_err(|e| JsError::new(&e))
}
