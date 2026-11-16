//! JS bindings for `packages/core/src/locale.ts`: generated from the signatures in `port.rs` by carcinize.
//! Regenerate with `carcinize refresh` after changing them.

use wasm_bindgen::prelude::*;

pub mod js;
pub mod port;
pub mod utf16;

/// `resolveFallbacks`.
fn resolve_fallbacks_inner(
    setting: String,
    locales: Vec<String>,
    source_locale: String,
) -> Result<String, String> {
    let setting = crate::js::from_json(&setting)?;
    let out = port::resolve_fallbacks(setting, locales, source_locale)?;
    crate::js::to_json(&out)
}

#[wasm_bindgen(js_name = "resolveFallbacks")]
pub fn resolve_fallbacks(
    setting: String,
    locales: Vec<String>,
    source_locale: String,
) -> Result<String, JsError> {
    resolve_fallbacks_inner(setting, locales, source_locale).map_err(|e| JsError::new(&e))
}

/// `fallbackChain`.
fn fallback_chain_inner(fallbacks: String, locale: String) -> Result<String, String> {
    let fallbacks = crate::js::from_json(&fallbacks)?;
    let out = port::fallback_chain(fallbacks, locale)?;
    crate::js::to_json(&out)
}

#[wasm_bindgen(js_name = "fallbackChain")]
pub fn fallback_chain(fallbacks: String, locale: String) -> Result<String, JsError> {
    fallback_chain_inner(fallbacks, locale).map_err(|e| JsError::new(&e))
}

/// `localeSpellingKey`.
fn locale_spelling_key_inner(code: String) -> Result<String, String> {
    let out = port::locale_spelling_key(code);
    Ok(out)
}

#[wasm_bindgen(js_name = "localeSpellingKey")]
pub fn locale_spelling_key(code: String) -> Result<String, JsError> {
    locale_spelling_key_inner(code).map_err(|e| JsError::new(&e))
}
