# Changelog

## 1.2.0 (2026-11-17)

Numeric code, already compiled code, and the edges of what a recording proves, after the first
round of feedback. rust-anything is now on crates.io: `cargo install rust-anything`.

- Float tolerance for numeric code: `record --float-tolerance 1e-9` (and `app record`) lets floats
  match when they differ by at most that share of the larger value (absolute below 1); `diff`,
  `gate`, `app replay` and the lift use it. Integers, text, structure and formatting (`3` against
  `3.0`) still match exactly. `diff --float-tolerance` tries one for a single run.
- Noise is sorted by kind: when the original's two runs disagree, `app record` reports `values`
  (now `[..]`), `order` (the same lines in another order: replay now compares them in any order
  instead of wildcarding them, so a port must still produce exactly those lines) and `lines` (a
  `...` that matches anything), by case and field. The lift's trycmd test skips `order` cases and
  says to run `app replay` in CI for them.
- Regex traps: functions in files that use regexes get extra string inputs where engines
  disagree (Turkish i, Kelvin sign, long s, joiners, non-ASCII digits and spaces, line separators,
  final sigma), in all six languages. Every string trap now also includes a dotless `ı`, a
  zero-width-joiner emoji and a non-breaking space.
- C: recording runs the library in the "C" locale, as a C program has unless it calls
  `setlocale`. It ran in the user's locale (Python sets it at startup), so `isspace` and friends
  could record a different answer per machine; the new non-breaking-space trap caught it.
- The pitch says what passing means: the Rust matches the original's recorded behavior. The
  README has a "What it can't catch" section.
- Already compiled code: `profile` and `analyze` flag Python functions compiled by numba
  (`@njit`, `@jit`, `@vectorize`, ...), Cython or JAX, and the hot functions that call them, since
  a Rust port gains little speed there.
- Go: crates no longer build a static library. Go 1.26.11 and 1.27.4 refuse to link archives
  that carry LLVM bitcode, which every Rust static library does; cgo links the shared library
  anyway. Run `rust-anything refresh <crate>` on a Go crate bridged by an earlier version.

## 1.1.0 (2026-11-16)

Whole-app ports: rust-anything now takes a TypeScript/JavaScript, Python, C, Go, Ruby or Java app
all the way to a pure Rust app, proving at every step that it still behaves the same. CLIs and HTTP
servers in all six languages, and libraries (proven end to end in TypeScript, Python and C so
far); every `app` command is also an MCP tool (`app_record`, `app_replay`, `app_front_door`, `app_wire`, `app_lift`).

- Plan: `analyze --whole-app` gives the app's shape (CLI, server or library) and entry points,
  every external dependency (imported, or declared in package.json, pyproject or
  requirements.txt, go.mod, Gemfile or gemspec, pom.xml or Gradle) with its crate and fit, the port
  order as pieces (units that import each other move together), and numbered decisions with a
  recommended answer for the user. POSIX headers and `com.sun.*` now map to their Rust
  counterparts.
- Record: `app record` runs the original on its own documented examples, an `--inputs` file and
  semantic traps (non-ASCII and emoji, negative division and modulo, overflow, float extremes,
  map order, missing files), twice; what differs between the two runs becomes a wildcard, and
  named normalizers (`timestamps`, `uuids`, `key-order`, `name=regex`) are saved with the cases.
  CLI cases are trycmd files in `rust/app-cases/` (argv, stdin, env and files in; stdout, stderr,
  exit code and files out). With `--http` (or `--port`) the command starts a server, and requests
  from the `curl` lines in its docs, `--inputs` and traps are stored in `rust/app-cases/http/`.
- Replay: `app replay` runs any build (the original, the front door, the final binary) against
  the cases, prints `passed/total` and the fallback rate, writes a report with a diff per failing
  case, and exits non-zero on any mismatch, so CI can run it. Any build can report its fallbacks
  through the file named by `RUST_ANYTHING_FALLBACK_LOG`. `--case <name>` reruns one
  failing case, the fast loop for fixing a mismatch.
- Front door: `app front-door` scaffolds `rust/app`, which owns the entry point from day one and
  hands what is not ported yet to the original, counting every fallback: for a CLI a clap binary
  that runs the original with the same argv, stdin and environment; for a server an axum app that
  starts the original on its own port and proxies to it; for a library (no `main`, no server, or
  `--library`) a Rust crate with the public API, while the original package keeps its callers.
  Replay against a fresh front door passes 100% at 100% fallback.
- Wire: after a unit passes `gate`, `app wire` adds its crate to `rust/app` and routes commands
  (`--command`), routes (`--route 'GET /users/{id}'`, `*`) or, for a library, the unit's exports
  to it, behind a kill switch, `RUST_ANYTHING_UNIT_<NAME>=original`; it writes a glue file to fill
  in once, then builds and replays. For a library it writes the unit's shadow drop-in for the
  original package to import, and shadows in every language but C (switched at build time) honor
  the kill switch.
- Lift: once the fallback rate is 0 and every unit passes `gate`, `app lift` makes it pure Rust.
  It refuses until then and lists every blocker; it works on a copy, removes the fallback path and
  the kill switches, turns each unit crate into a plain library (no `cdylib`, bindings or
  generated wrappers), runs the lift checklist (fmt, clippy, tests, every target in
  `rust/DECISIONS.md`, assets, replay in CI) and replays the pure binary, and writes the result
  back only when all of it passes. CLIs get a trycmd test that runs the cases on `cargo test`.
  The original is never touched.
- `gate` checks error paths: every place the original fails on purpose (`throw`, `raise`, an
  `error` result) needs a recorded failing case, and `port.rs` may not `unwrap`, `expect` or
  `panic!` in a function whose original returns an error, unless marked `// INFALLIBLE: <why>`.
- `record` adds the semantic traps to every function: each trap value of a parameter's type, one
  parameter at a time, next to the generated cases.
- Proved end to end on a small CLI and a small HTTP server written in each of the six languages,
  replayed with the original moved away, and on libraries (a C library lifted to one pure Rust
  crate; TypeScript and Python libraries moved unit by unit).
- The skill gains a whole-app guide (`whole-app.md`, installed by `setup`): it grills the user in
  rounds with a recommended answer for each question, researches dependencies against primary
  sources, and keeps the port going across sessions with `rust/MAP.md` (claim one unit at a
  time), ADRs in `rust/DECISIONS.md`, `rust/GLOSSARY.md` and a `rust/HANDOFF.md` at each
  checkpoint. It fixes red replays one case at a time, can port a wave in parallel with
  subagents, and ends with `rust/cutover.sh` for the steps only the user can do. Grilling,
  research and these practices are adapted from Matt Pocock's agent skills.
- The pitch is now rewriting a whole app in Rust; making the slow part fast stays as a smaller use
  of the same tool.

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
