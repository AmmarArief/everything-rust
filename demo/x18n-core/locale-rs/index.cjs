// Loads the wasm-bindgen (nodejs target) output built by `carcinize build`.
const native = require('./pkg/locale_rs.js')

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
// Per function: which arguments are structured. Others pass through: an `undefined`
// there is an omitted optional argument, an `Option` in Rust.
for (const [name, structured] of Object.entries({ resolveFallbacks: [true, false, false], fallbackChain: [true, false] })) {
  module.exports[name] = (...args) => fromNative(native[name](...args.map((a, i) => structured[i] ? toNative(a) : a)))
}
