// Loads the wasm-bindgen (nodejs target) output built by `carcinize build`.
const native = require('./pkg/plugins_rs.js')
module.exports = native
