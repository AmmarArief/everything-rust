# Changelog

## Unreleased

- HTTP servers, in all six languages: `app record --http` (or `--port`) starts the original,
  sends requests from the `curl` lines in its docs, `--inputs` and traps (path segments, query
  values, bodies, an unknown route), twice, and stores each request and response in
  `rust/app-cases/http/`. `app replay` runs any build the same way. `app front-door` scaffolds an
  axum server that starts the original on its own port and proxies every route no unit answers,
  counting each fallback; `app wire --route` hands axum routes (`GET /users/{id}`, `*`) to a unit
  behind its kill switch; `app lift` removes the proxy. Proved end to end on a small server in
  each language, replayed with the original moved away.
- `app lift` (also MCP tool `app_lift`) for CLI apps: refuses until replay passes with fallback
  rate 0 and every unit passes gate, listing every blocker. On a copy first, it removes the
  fallback and the kill switches, turns each unit crate into a plain library (no `cdylib`,
  bindings or generated wrappers), adds a trycmd test that runs `rust/app-cases` on
  `cargo test`, runs the lift checklist (fmt, clippy, tests, `DECISIONS.md` targets, assets) and
  replays the pure binary; only then are the changes written back. Proved end to end on the
  small CLI in all six languages, replayed with the original moved away.
- The front door routes argv that names no command (no arguments, unknown commands) to the app's
  own name, so a unit can answer it; replay lists the cases that fell back.
- `app front-door` and `app wire` (also MCP tools `app_front_door` and `app_wire`) for CLI apps.
  `front-door` scaffolds `rust/app`, a Rust binary with a clap parser for the recorded commands
  and flags where every command runs the original and counts the fallback; it builds and replays
  (100% passed at 100% fallback). `wire` checks a unit with `gate`, adds its crate to `rust/app`,
  writes a glue file to fill in, routes the unit's commands to it behind a
  `RUST_ANYTHING_UNIT_<NAME>=original` kill switch, then builds and replays. Proved on the same
  small CLI in all six languages.
- `app record` and `app replay` (also MCP tools `app_record` and `app_replay`): app-level
  behavior for CLIs in all six languages. `record` runs the original on its documented examples,
  an `--inputs` file and semantic traps, twice each, turns what differs between runs into
  wildcards, applies named normalizers (`timestamps`, `uuids`, `key-order`, `name=regex`), and
  writes trycmd-compatible cases to `rust/app-cases/`. `replay` runs any build against them,
  prints `passed/total`, writes a report with a diff per failing case, exits non-zero on a
  mismatch, and reports the fallback rate of builds that write `RUST_ANYTHING_FALLBACK_LOG`.

## 1.0.1 (2026-11-16)

- npm: one `rust-anything` package that carries the binaries for macOS, Linux and Windows.

## 1.0.0 (2026-11-16)

First version: the full loop for TypeScript/JavaScript, Python, C, Go, Ruby and Java.

- Python: PyO3 crates, typed exceptions, literal defaults and keyword-only parameters kept.
- C: drop-in `extern "C"` replacements and a header; crash-safe recording through ctypes; shadow
  builds with `-D` renames.
- Go, Ruby, Java: one JSON dispatcher and generated wrappers (cgo package, Fiddle file, JNI
  classes) with the original API; generated drivers record and replay in the original runtime;
  shadow drop-ins per language.
- Direct bridges: PyO3 conversion without JSON and `pynum::Num`, a magnus extension for Ruby,
  typed JNI and cgo functions, and a generated binary wire for lists, maps, structs and several
  results in Go and Java, and for TS through napi-rs and wasm-bindgen; `bench` times calls in process and ends with a verdict.
- `profile`: hot functions of a real workload (node `--cpu-prof`, cProfile, `go test` pprof, JFR,
  a Ruby tracer) with callers, call counts and advice. Batch classes (`--batch fn:param`) for TS,
  Python, Ruby, Go and Java keep items in Rust and run on all cores, with mimalloc; a
  `<Function>Index` in `port.rs` prepares the items once. x18n's search: 20x faster than TS (11.5x in wasm), with an index that ranks by itself; Go
  and Java batches 4x and 3x faster than the originals.
- Run end to end on Go's `path`, Ruby's `shellwords`, Python's `colorsys`, and JDK and glibc
  behavior; fixes from those runs: exact float parsing, crash isolation, `module_function :name`,
  numeric inference for untyped parameters, ints in Python inputs.

- `analyze`: units, port order, risks with `file:line`, bridge per unit, npm-to-crate fit,
  readiness and priority; counts bridgeable exports; follows tsconfig paths and workspace packages.
- `bridge` / `refresh`: crates with napi-rs or wasm-bindgen bindings generated from `port.rs`;
  functions, async functions and classes; JS-only exports stay in TS with a reason.
- `record` / `diff` / `bench`: recorded TS behavior from shaped, edge-case inputs and class
  scenarios, replayed against Rust; timing with the bridge included.
- `shadow`: drop-in modules running TS and Rust side by side, `RUST_ANYTHING_MODE=old|shadow|new`.
- `gate`: fmt, clippy, tests, `unsafe` audit, nothing unported, build, API, behavior.
- `setup`: registers the MCP server and the workflow skill with Claude Code, Codex, Cursor and
  Gemini CLI, keeping other settings and backing up each file it changes; `--remove` and
  `--dry-run`.
- `mcp`: every step as an MCP tool; agent skill in `skills/rust-anything`.
- Install with `npm install -g rust-anything` (prebuilt binaries for macOS, Linux and Windows) or
  from the binaries attached to the GitHub release.
