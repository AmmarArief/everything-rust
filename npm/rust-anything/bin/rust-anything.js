#!/usr/bin/env node
'use strict'
const { spawnSync } = require('node:child_process')

const key = `${process.platform}-${process.arch}`
const exe = process.platform === 'win32' ? 'rust-anything.exe' : 'rust-anything'
let bin
try {
  bin = require.resolve(`rust-anything-${key}/bin/${exe}`)
} catch {
  console.error(
    `rust-anything: no prebuilt binary for ${key}.\n` +
      'Install it with Cargo instead: cargo install --git https://github.com/erkamyaman/rust-anything',
  )
  process.exit(1)
}

const result = spawnSync(bin, process.argv.slice(2), { stdio: 'inherit' })
if (result.error) {
  console.error(`rust-anything: ${result.error.message}`)
  process.exit(1)
}
if (result.signal) {
  process.kill(process.pid, result.signal)
}
process.exit(result.status ?? 1)
