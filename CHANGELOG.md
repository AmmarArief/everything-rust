# Changelog

## 0.1.0 (unreleased)

First version: the full loop for TypeScript/JavaScript.

- `analyze`: units, port order, risks with `file:line`, bridge per unit, npm-to-crate fit,
  readiness and priority; counts bridgeable exports; follows tsconfig paths and workspace packages.
- `bridge` / `refresh`: crates with napi-rs or wasm-bindgen bindings generated from `port.rs`;
  functions, async functions and classes; JS-only exports stay in TS with a reason.
- `record` / `diff` / `bench`: recorded TS behavior from shaped, edge-case inputs and class
  scenarios, replayed against Rust; timing with the bridge included.
- `shadow`: drop-in modules running TS and Rust side by side, `CARCINIZE_MODE=old|shadow|new`.
- `gate`: fmt, clippy, tests, `unsafe` audit, nothing unported, build, API, behavior.
- `mcp`: every step as an MCP tool; agent skill in `skills/carcinize`.
