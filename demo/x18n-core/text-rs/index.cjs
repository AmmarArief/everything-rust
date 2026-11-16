// Loads the wasm-bindgen (nodejs target) output built by `carcinize build`.
const native = require('./pkg/text_rs.js')

// JSON has no `undefined`, and Rust strings no lone surrogates. Structured values cross the
// bridge with `{"$undefined": true}` and `{"$utf16": [...]}` in their place (`js.rs`),
// so `{ a: undefined }`, `null` and half an emoji all survive.
const UNDEFINED = '$undefined'
const toNative = v => v === undefined
  ? { [UNDEFINED]: true }
  : typeof v === 'string' && !v.isWellFormed() ? { $utf16: Array.from({ length: v.length }, (_, i) => v.charCodeAt(i)) }
  : Array.isArray(v) ? v.map(toNative)
  : v !== null && typeof v === 'object' ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, toNative(x)]))
  : v
const fromNative = v => Array.isArray(v) ? v.map(fromNative)
  : v !== null && typeof v === 'object' ? (v[UNDEFINED] === true && Object.keys(v).length === 1 ? undefined : Array.isArray(v.$utf16) && Object.keys(v).length === 1 ? String.fromCharCode(...v.$utf16) : Object.fromEntries(Object.entries(v).map(([k, x]) => [k, fromNative(x)])))
  : v
module.exports = { ...native }
for (const name of ['diffText', 'applyTextEdits', 'rangeContains']) {
  // Top-level `undefined` stays as is: optional parameters are `Option` in Rust.
  module.exports[name] = (...args) => fromNative(native[name](...args.map(a => a === undefined ? a : toNative(a))))
}
