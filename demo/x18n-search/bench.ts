// x18n's KeySidebar search over 50,000 keys, as typed letter by letter: the TS pipeline
// (score every key, keep matches, sort) against `FuzzyScoreBatch`, which keeps the keys in Rust.
// Run after `carcinize build .`:
//   node --experimental-strip-types bench.ts <path to the x18n checkout>
import { createRequire } from 'node:module'
import { resolve } from 'node:path'

const require = createRequire(import.meta.url)
const x18n = resolve(process.argv[2] ?? '../../../x18n')
const { fuzzyScore } = await import(`${x18n}/packages/core/src/fuzzy.ts`)
const { FuzzyScoreBatch } = require('./index.cjs')

const words = ['app', 'settings', 'title', 'button', 'save', 'cancel', 'error', 'message', 'user', 'profile', 'nav', 'home', 'dashboard', 'form', 'label', 'placeholder', 'tooltip', 'dialog', 'confirm', 'delete']
let seed = 7
const r = (k: number) => (seed = (seed * 1103515245 + 12345) % 2147483648) % k
const keys = Array.from({ length: 50000 }, (_, i) => Array.from({ length: 3 + r(3) }, () => words[r(words.length)]).join('.') + i)

// The original pipeline, from the sidebar.
const ts = (query: string): string[] => keys
  .map(key => ({ key, score: fuzzyScore(query, key) }))
  .filter(m => m.score >= 0)
  .sort((a, b) => b.score - a.score)
  .map(m => m.key)

const t0 = performance.now()
const batch = new FuzzyScoreBatch(keys)
const build = performance.now() - t0
const rust = (query: string): string[] => Array.from(batch.rank(query, 0), (i: number) => keys[i])

const typed = ['s', 'se', 'set', 'sett', 'setti', 'settin', 'setting', 'n', 'na', 'nav', 'navh', 'navho', 'navhom', 'navhome', 'Xq', 'éa']
for (const q of typed) {
  if (ts(q).join('\n') !== rust(q).join('\n'))
    throw new Error(`different results for ${JSON.stringify(q)}`)
}
const time = (search: (q: string) => string[]) => {
  for (let i = 0; i < 2; i++) typed.forEach(search)
  const start = performance.now()
  for (let i = 0; i < 5; i++) typed.forEach(search)
  return (performance.now() - start) / (5 * typed.length)
}
const a = time(ts)
const b = time(rust)
console.log(`same results for ${typed.length} queries; index built in ${build.toFixed(1)} ms`)
console.log(`TS ${a.toFixed(2)} ms per query, Rust ${b.toFixed(2)} ms per query: ${(a / b).toFixed(1)}x faster`)
