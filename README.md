# carcinize

> Working name. To be decided.

Move a TypeScript/JavaScript codebase to Rust one module at a time, without breaking it.

carcinize does not translate code: your coding agent (Claude Code, Codex, Cursor, ...) or you write
the Rust. carcinize supplies what an agent cannot guess reliably: what to port first and in which
order, a bridge so JS can call the Rust, the exact behavior the port must match, a safe way to
ship it, and a gate that says when a port is done.

## The loop

| Step | Command | What it does |
| --- | --- | --- |
| 1. Plan | `carcinize analyze <repo>` | Units, port order, risks with `file:line`, bridge per unit, npm-to-crate fit, best first ports |
| 2. Bridge | `carcinize bridge <repo> <unit>` | Crate with napi-rs or wasm-bindgen bindings (`lib.rs`) and stubs to port (`port.rs`); UTF-16 helpers when needed |
| 3. Record | `carcinize record <crate>` | Runs the TS original on generated edge cases (non-ASCII, emoji, `-0`, `NaN`, ...) and saves `cases.jsonl` |
| 3. Build | `carcinize build <crate>` | Builds the `.node` (napi-rs) or the wasm-bindgen package |
| 3. Diff | `carcinize diff <crate>` | Replays the recorded cases against Rust, lists mismatches |
| 4. Shadow | `carcinize shadow <crate>` | Drop-in module: runs both, returns the original's result, reports mismatches; `CARCINIZE_MODE=old\|shadow\|new` |
| 5. Gate | `carcinize gate <crate>` | fmt, clippy `-D warnings`, tests, `unsafe` audit, nothing unported, build, API, behavior |
| Agents | `carcinize mcp` | Every step as an MCP tool over stdio; workflow skill in `skills/carcinize/SKILL.md` |

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

Needs Node 22.6+ for `record`, `diff` and `shadow` (it runs `.ts` sources directly).
wasm bridges also need `rustup target add wasm32-unknown-unknown` and
`cargo install wasm-bindgen-cli --version <the crate's wasm-bindgen version> --locked`; `build` names
the exact version.

For agents, add the MCP server (`.mcp.json`):

```json
{ "mcpServers": { "carcinize": { "command": "carcinize", "args": ["mcp"] } } }
```

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
