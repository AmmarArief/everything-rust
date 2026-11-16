# Changelog

## 0.1.0 (unreleased)

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
- Run end to end on Go's `path`, Ruby's `shellwords`, Python's `colorsys`, and JDK and glibc
  behavior; fixes from those runs: exact float parsing, crash isolation, `module_function :name`,
  numeric inference for untyped parameters, ints in Python inputs.

- `analyze`: units, port order, risks with `file:line`, bridge per unit, npm-to-crate fit,
  readiness and priority; counts bridgeable exports; follows tsconfig paths and workspace packages.
- `bridge` / `refresh`: crates with napi-rs or wasm-bindgen bindings generated from `port.rs`;
  functions, async functions and classes; JS-only exports stay in TS with a reason.
- `record` / `diff` / `bench`: recorded TS behavior from shaped, edge-case inputs and class
  scenarios, replayed against Rust; timing with the bridge included.
- `shadow`: drop-in modules running TS and Rust side by side, `CARCINIZE_MODE=old|shadow|new`.
- `gate`: fmt, clippy, tests, `unsafe` audit, nothing unported, build, API, behavior.
- `mcp`: every step as an MCP tool; agent skill in `skills/carcinize`.
