# carcinize

> Working name. To be decided.

Plan and check incremental migrations of TypeScript/JavaScript codebases to Rust.

carcinize does not translate code. Your coding agent (Claude Code, Codex, Cursor, ...) writes
the Rust. carcinize gives it reliable ground truth: what to port first, in which order, with which bridge,
and which JS behaviors will silently change.

## Status

| Step | Command | State |
| --- | --- | --- |
| 1. Analyze: ranked, ordered migration plan | `carcinize analyze` | done (TS/JS) |
| 2. Connect: scaffold napi-rs / wasm-bindgen bridge | `carcinize bridge` | planned |
| 3. Compare: record TS behavior, diff the Rust port | `carcinize diff` | planned |
| 4. Shadow: run both in production, log mismatches | `carcinize shadow` | planned |
| 5. Gate: clippy, fmt, tests, API snapshot | `carcinize gate` | planned |
| Agent surface: MCP server + skill | | planned |
| Other source languages (Python first) | | planned |

## Usage

```sh
cargo run --release -- analyze path/to/repo            # Markdown plan on stdout
cargo run --release -- analyze path/to/repo -o plan.md
cargo run --release -- analyze path/to/repo --json     # for agents; schema carcinize/analyze@1
```

## What `analyze` does

1. Finds packages (`package.json`) and source files, skipping `node_modules`, build output,
   fixtures, playgrounds and tool configs.
2. Parses every file with [oxc](https://oxc.rs), including `<script>` blocks of Vue, Svelte and
   Astro components.
3. Groups files into **units**: a folder of source files, or a file at a package's source root.
4. Resolves imports into a unit graph and orders it in **waves**: a unit's imports come in earlier waves.
   Import cycles are reported and ported together.
5. Finds **risks** with file and line: JS behavior a straight translation gets wrong (see below).
6. Picks a **bridge**: napi-rs for code that needs Node, wasm-bindgen for portable code,
   keep in JS for UI and editor code.
7. Maps npm dependencies to crates and flags the ones with no Rust equivalent.
8. Scores each unit:
   - **readiness**: how safe the port looks (risks, size, fan-in, dependencies, tests),
   - **priority**: readiness balanced with value (loops and string work, tests, size).
   `start_here` lists the best first ports by priority.

## Risks it detects

| Risk | Severity | Example |
| --- | --- | --- |
| String offsets | high | `s.slice(i)`, `indexOf`, `matchAll`, the `d` flag: UTF-16 vs UTF-8 |
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
cargo test
cargo clippy --all-targets
cargo fmt
```

Design choices and their reasons: [DECISIONS.md](./DECISIONS.md).
