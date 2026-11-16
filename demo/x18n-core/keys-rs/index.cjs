// Loads the wasm-bindgen (nodejs target) output built by `carcinize build`.
const native = require('./pkg/keys_rs.js')

// JSON has no `undefined`. Structured values cross the bridge with `{"$undefined": true}`
// in its place (`js::undefined()` in Rust), so `{ a: undefined }` and `null` stay distinct.
const UNDEFINED = '$undefined'
const toNative = v => v === undefined
  ? { [UNDEFINED]: true }
  : Array.isArray(v) ? v.map(toNative)
  : v !== null && typeof v === 'object' ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, toNative(x)]))
  : v
const fromNative = v => Array.isArray(v) ? v.map(fromNative)
  : v !== null && typeof v === 'object' ? (v[UNDEFINED] === true && Object.keys(v).length === 1 ? undefined : Object.fromEntries(Object.entries(v).map(([k, x]) => [k, fromNative(x)])))
  : v
module.exports = { ...native }
for (const name of ['splitKey']) {
  // Top-level `undefined` stays as is: optional parameters are `Option` in Rust.
  module.exports[name] = (...args) => fromNative(native[name](...args.map(a => a === undefined ? a : toNative(a))))
}

// `replaceKeyPrefix` returns UTF-16 code units so lone surrogates survive, as in JS.
const fromUnits = (units) => {
  let out = ''
  for (let i = 0; i < units.length; i += 8192)
    out += String.fromCharCode(...units.subarray(i, i + 8192))
  return out
}
module.exports.replaceKeyPrefix = (...args) => fromUnits(native.replaceKeyPrefix(...args))
