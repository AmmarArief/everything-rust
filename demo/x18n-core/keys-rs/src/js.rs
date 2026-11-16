//! JS values that JSON cannot express.

#![forbid(unsafe_code)]

/// JS `undefined` inside a structured value: `{ a: undefined }` is not `{ a: null }`.
/// `index.cjs` turns this marker into `undefined` on the way out, and back on the way in.
pub fn undefined() -> serde_json::Value {
    serde_json::json!({ "$undefined": true })
}

/// Whether a structured value received from JS was `undefined`.
pub fn is_undefined(value: &serde_json::Value) -> bool {
    value.as_object().is_some_and(|o| {
        o.len() == 1 && o.get("$undefined") == Some(&serde_json::Value::Bool(true))
    })
}
