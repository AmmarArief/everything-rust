// Loads the native module built by `carcinize build`.
const native = require('./fuzzy_rs.node')
module.exports = native
