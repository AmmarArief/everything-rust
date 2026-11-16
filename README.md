# carcinize

> Working name. To be decided.

Move a codebase to Rust one module at a time, without breaking it. TypeScript/JavaScript,
Python, C, Go, Ruby and Java.

carcinize does not translate code: your coding agent (Claude Code, Codex, Cursor, ...) or you write
the Rust. carcinize supplies what an agent cannot guess reliably: what to port first and in which
order, a bridge so the rest of the code can call the Rust with its API unchanged, the exact
behavior the port must match, a safe way to ship it, and a gate that says when a port is done.

## Languages

| Language | Unit | Bridge | Recorded and replayed by |
| --- | --- | --- | --- |
| TypeScript/JavaScript | folder or file | napi-rs or wasm-bindgen, classes and async included | Node, running the `.ts` sources |
| Python | package or module | PyO3 (abi3), typed exceptions | Python, importing the original |
| C | `.c` file | `extern "C"` functions with the original names and a header | the original compiled to a shared library, called through ctypes, one fork per case |
| Go | package | JSON dispatcher + generated cgo package with the original API | a generated Go driver, built as its own module |
| Ruby | file or folder of module functions | JSON dispatcher + generated Fiddle wrapper defining the same modules | a generated Ruby driver |
| Java | package of `public static` methods | JSON dispatcher over JNI + generated wrapper classes | a generated reflection driver |

The language is detected from the repository (`go.mod`, `pom.xml`, `pyproject.toml`, `Gemfile`,
`package.json`, else the most common source files); `--lang` overrides it. Types come from the
source: TS types, Python hints, C and Go and Java signatures, Ruby YARD tags; untyped parameters
used as numbers are inferred as numbers.

## The loop

| Step | Command | What it does |
| --- | --- | --- |
| 1. Plan | `carcinize analyze <repo>` | Units, port order, risks with `file:line`, bridge per unit, dependency-to-crate fit, best first ports |
| 2. Bridge | `carcinize bridge <repo> <unit>` | Crate with the bindings (`lib.rs`, plus the wrapper the source language loads) and stubs to port (`port.rs`) |
| 3. Record | `carcinize record <crate>` | Runs the original on generated edge cases (non-ASCII, emoji, `-0`, `NaN`, ints where floats are expected, ...) and saves `cases.jsonl` |
| 3. Build | `carcinize build <crate>` | Builds the Rust side: `.node`, wasm package, Python extension or shared library |
| 3. Diff | `carcinize diff <crate>` | Replays the recorded cases against Rust, called the way the original's callers call it; lists mismatches |
| 3. Bench | `carcinize bench <crate>` | Times the recorded cases on the original and on Rust, bridge included |
| 4. Shadow | `carcinize shadow <crate>` | Drop-in that runs both, returns the original's result, reports mismatches; `CARCINIZE_MODE=old\|shadow\|new` |
| 5. Gate | `carcinize gate <crate>` | fmt, clippy `-D warnings`, tests, `unsafe` audit, nothing unported, build, API, behavior |
| Agents | `carcinize mcp` | Every step as an MCP tool over stdio; workflow skill in `skills/carcinize/SKILL.md` |

## Case study: x18n's core

All 29 functions of x18n's core that can move without an API change were ported with carcinize:
every gate passes and 7,925 recorded cases match exactly. `carcinize bench` then showed the ports
are slower than the TS as called from JS, because the bridge costs more than the work. Full report:
[`demo/x18n-core`](./demo/x18n-core/README.md). The lesson is built in: measure with `bench` before
shipping a port.

## Real code in six languages

Go's `path`, Ruby's `shellwords`, Python's `colorsys`, JDK and glibc behavior behind small
facades: every port passes its gate, 13,010 recorded cases in all. The runs found and fixed
bugs in carcinize (floats off by one ULP through `serde_json`, C crashes during recording,
untyped code getting garbage inputs). With direct bridges, the Ruby port runs 4.4x faster than
Ruby and the Python port 1.35x faster than CPython; Go and Java leaf functions stay faster in
their own language, and `bench` says to port bigger units there.
Report: [`docs/real-world-languages.md`](./docs/real-world-languages.md).

## Example: a real module from x18n

`packages/core/src/fuzzy.ts` from [x18n](https://github.com/x18n-dev/x18n), ported both ways in
[`demo/`](./demo):

```sh
carcinize analyze ../x18n                                      # fuzzy.ts: ready, wasm-bindgen (core runs in browsers)
carcinize bridge ../x18n packages/core/src/fuzzy.ts --out demo/fuzzy-rs
carcinize record demo/fuzzy-rs --count 2000
# write src/port.rs
carcinize gate demo/fuzzy-rs
```

A first port that indexed by Rust `char` passed simple tests but failed `diff`:

```
198/200 cases match
- fuzzyScore("b😀", "a😀b😀c"): expected {"ok":1997}, got {"ok":1998}
```

JS `indexOf` counts UTF-16 code units, so an emoji is two positions. The port in
`demo/*/src/port.rs` uses the generated `utf16` helpers and passes 2000/2000 cases and the gate,
as napi-rs (`demo/fuzzy-napi`) and as wasm-bindgen (`demo/fuzzy-rs`).

The plan carcinize writes for all of x18n: [`docs/examples/x18n.md`](./docs/examples/x18n.md).

## Install

```sh
cargo install --path .
```

`record`, `diff` and `bench` run the original in its own runtime: Node 22.6+ for TS (it runs `.ts`
sources directly), Python 3.9+ for Python and C (ctypes), Go 1.21+, Ruby 3+ with Fiddle, JDK 17+
(`javac` and `java`). C also needs a C compiler. wasm bridges also need `rustup target add wasm32-unknown-unknown` and
`cargo install wasm-bindgen-cli --version <the crate's wasm-bindgen version> --locked`; `build` names
the exact version.

For agents, add the MCP server (`.mcp.json`):

```json
{ "mcpServers": { "carcinize": { "command": "carcinize", "args": ["mcp"] } } }
```

## What `bridge` decides

- Exports that cannot cross a bridge stay in the source language, with the reason, and the shadow
  module re-exports the original. For TS:
  functions that take or return functions, generators and iterators, RegExp, `instanceof` checks,
  generics, and values whose types (followed through imports, re-exports and `extends`) hold
  functions. `--keep "name: reason"` keeps more, e.g. code that depends on `Intl` data.
- TS types map to Rust types, with type aliases resolved through imports and parameter types
  inferred from default values; anything structured crosses as JSON.
- JSON cannot express `undefined`, NaN, Infinity, -0 or lone surrogates: structured values carry
  markers (`{"$undefined": true}`, `{"$num": "NaN"}`, `{"$utf16": [...]}`) that the generated
  `index.cjs` translates, with helpers in the crate's `js.rs`.

`record` runs TS sources as they are: a Node resolve hook follows bundler-style imports
(`./errors` to `./errors.ts`), `--resolve-from` supplies npm packages when the repo has none
installed, and TS that needs transforming (enums, parameter properties) is retried with
`--experimental-transform-types`.

## What `analyze` looks at

- Packages (`package.json`) and source files; skips `node_modules`, build output, fixtures,
  playgrounds, tool configs. Vue/Svelte/Astro `<script>` blocks are parsed for imports.
- **Units**: a folder of source files, or a file at a package's source root.
- **Waves**: a unit's imports come in earlier waves; import cycles are ported together.
- **Bridge**: napi-rs when the unit or anything it imports needs Node; wasm-bindgen for portable
  code; keep in JS for UI and editor code.
- **Readiness** (how safe) and **priority** (safe and worth it: loops and string work, tests, size).

| Risk | Severity | Example |
| --- | --- | --- |
| String offsets | high | `slice`, `indexOf`, `matchAll`, the `d` flag: UTF-16 vs UTF-8 |
| Regex syntax | high | lookaround, backreferences, `y`: not in the `regex` crate |
| Dynamic regex | high | `new RegExp(userPattern)`: JS syntax at runtime |
| Dynamic code | high | `eval`, `new Function`, computed `import()` |
| Dynamic typing | medium | `any`, `'key' in value` |
| Ordered collections | medium | `Map`, `Set`, `Object.keys`: insertion order |
| Async, generators | medium | promise and generator boundaries |
| Class inheritance, reflection | medium | `extends`, `Proxy`, `Reflect`, `Symbol` |
| Callback-shaped APIs | medium | function-typed params and methods: chatty bridges |
| Exceptions, numbers, JSON | low | `throw`, bitwise ops, `JSON.stringify` order |

## Development

```sh
cargo test                        # unit tests, fixture repo, record/diff/shadow loop (needs node)
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Design choices and their reasons: [DECISIONS.md](./DECISIONS.md).
