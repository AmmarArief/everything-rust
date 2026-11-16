# Design: whole app to a pure Rust app

Status: milestones 1 to 3 built (CLIs: record, replay, front door, wire, lift); 4 to 6 to come.

## Goal

rust-anything ports a whole app (TypeScript/JavaScript, Python, C, Go, Ruby or Java) to a
standalone Rust app, with the original kept only as reference. The user's coding agent writes the
Rust; rust-anything decides the order, scaffolds, and proves at every step that the app still
behaves the same. All six languages ship in the first version.

The workflow the agent follows is in `skills/rust-anything/whole-app.md`. The evidence behind it
is in `docs/research/whole-app-port.md`. This document is about what the tool needs so the agent
can follow that workflow without hand-written scripts.

## What already exists and stays

`analyze` (units, waves, risks, dependencies), `bridge`, `record`, `port.rs`, `build`, `diff`,
`bench`, `shadow`, `gate`, `profile`, and the MCP server. Every unit still goes through them. They
prove a unit equals the original at the function level.

## What is missing

1. **App-level proof.** Today's proof stops at a unit's functions. A whole port needs proof that
   the whole program behaves the same: what goes in and out of the process, or over HTTP.
2. **A Rust entry point early.** Today the original calls Rust. A whole port needs the reverse:
   Rust owns `main` and calls the original for what is not ported yet (Turborepo's Rust shim,
   Mercurial's `rhg` fallback).
3. **The lift.** Nothing turns unit crates plus bindings into one pure Rust binary.
4. **Whole-app planning.** `analyze` ranks units for a first port; it does not find entry points,
   map every dependency to a crate for every language, or say what a `keep` unit becomes.

The app-level parts (1 to 3) treat the app as a black box, so they work the same for all six
languages. That is what makes "all six at once" realistic: the per-language work is already in
`bridge`, `record` and `diff`.

## New commands

Each is also an MCP tool with the same name.

### `rust-anything app record <repo>`

Runs the original on app-level cases and stores them in `rust/app-cases/`.

- **CLI cases**: argv, stdin, env and input files in; stdout, stderr, exit code and files written
  out. One case per file, in a format a person can read and edit (trycmd-style).
- **HTTP cases**: the agent or user gives a start command and a port; requests come from a list
  (method, path, headers, body) and the responses are stored.
- Inputs from the app's tests and examples, `--inputs` files, and a built-in set of semantic traps
  per language (string length and offsets, division and modulo signs, overflow, float printing,
  map order, regex features).
- Runs the original **twice**. Fields that differ between the two runs are marked as noise and
  normalized (Diffy's idea). The user can add normalizers by name (timestamps, uuids, key order).

### `rust-anything app replay <repo> [--build <command>]`

Runs any build (the original, the front door, the final binary) against `rust/app-cases/` and
prints `passed/total` and the **fallback rate** (cases still answered by the original). It writes a
report with a diff per failing case. Exit code non-zero on any mismatch, so CI can run it.

### `rust-anything app front-door <repo>`

Scaffolds `rust/app`, the Rust binary that owns the entry point from day one:

- **CLI**: a `clap` parser for the original's commands and flags (taken from `analyze` and the
  recorded cases), where every command starts as "run the original with the same argv, stdin and
  env and pass its output and exit code through".
- **HTTP**: an `axum` server where every route starts as a proxy to the original server.
- A **kill switch** per unit: `RUST_ANYTHING_UNIT_<NAME>=original` sends that unit back to the
  original until the lift.
- Every fallback is counted, so `replay` can report the fallback rate.

### `rust-anything app wire <repo> <unit>`

After a unit passes `gate`, connects its `port.rs` code to the front door: the commands or routes
it covers stop falling back. The agent writes the glue; this command generates the call sites and
the kill switch and runs `replay`.

### `rust-anything app lift <repo>`

Only when the fallback rate is zero and every unit passes `gate`:

- removes the fallback path and the kill switches,
- turns each unit crate into a plain library dependency of `rust/app` (drops `cdylib`, bindings,
  generated wrappers),
- runs the lift checklist: embedded assets, every target in `DECISIONS.md` builds, `replay` in CI,
- runs `gate` on everything and `replay` on the pure binary.

## Changes to existing commands

- `analyze --whole-app`: entry points (bin scripts, `main`, server start), every external
  dependency mapped to a Rust crate with a fit for all six languages (today only the npm mapping is
  complete), `keep` units listed as decisions for the grilling round, and a wave plan that groups
  tangled units into one piece.
- `gate`: flags `unwrap`, `expect` and `panic!` added in `port.rs` where the original handled an
  error, and fails when a recorded error case is missing.
- `record`: adds the semantic-trap inputs to the generated cases.

## Files the tool writes in the user's repo

```
rust/
  DECISIONS.md        decisions from the grilling round, with reasons (the agent writes it)
  research/           one cited file per dependency or risk (the agent writes them)
  app-cases/          recorded app-level cases, normalizers, last replay report
  app/                the front door, later the whole Rust app
  <unit crates>/      as today
```

## Milestones

1. `app record` and `app replay` for CLIs, all six languages (black box, so one implementation).
2. `app front-door` and `app wire` for CLIs, with fallback rate and kill switches.
3. `app lift` for CLIs. A CLI app can now be ported end to end.
4. HTTP: `app record` and `app replay` for servers, the axum proxy front door.
5. `analyze --whole-app` and the `gate` and `record` changes.
6. Libraries (no `main`): front door is the library's public API in Rust, and the original's
   package re-exports it until the lift.

Each milestone ships as a release, is proved on the repository's fixtures in all six languages,
and on one real open-source CLI per language before it is called done.

## Product changes

The pitch changes from "make the slow part fast, without rewriting the app" to rewriting the app
in Rust, one piece at a time, with proof. That touches the GitHub description, the README intro,
the hero image tagline, `Cargo.toml`, the npm description, and `--help` (which still says
"TypeScript/JavaScript to Rust" only). Making the slow part fast stays as a smaller use of the same
tool.

## Decisions (2026-11-16)

1. **CLIs first**, then servers: the milestone order above stands.
2. **Case format: trycmd-compatible files**, readable and editable, and understood by existing Rust
   tooling.
3. **Front door HTTP framework: axum** by default; the user can choose another in the grilling
   round.
4. **New pitch drafted now, published with milestone 1**, so the README never promises what does
   not exist yet.

Taken while building, without a question to the user (the conventional option, with the reason):

5. **The original's command goes after `--`** (`app record . -- python3 cli.py`), as `profile`
   already takes its workload; it is saved with the cases, so later runs need no arguments.
6. **`--inputs` is JSONL** (`{"name", "args", "stdin", "env", "files"}`), like `record --inputs`.
7. **Settings live in `rust/app-cases/rust-anything.json`**, not a TOML file, so a trycmd
   `*.toml` glob never reads them as a case.
8. **Re-recording overwrites cases of the same name and deletes none**: hand-written cases stay.
9. **The fallback hook is the file named by `RUST_ANYTHING_FALLBACK_LOG`**: one line per
   fallback, created empty to report none. Any build can report through it, not only ours.
10. **The front door's clap parser only routes**: the original always gets the raw argv, and
    anything clap rejects falls back, so the parser never has to match the original exactly.
11. **Argv that names no command routes to the app's own name**, so a unit can take the usage
    and unknown-command paths; without it no CLI reaches fallback rate 0.
12. **`wire` takes `--command` (repeatable), runs `gate` unless `--skip-gate`, and writes the glue
    file once**; only `src/units/mod.rs` is regenerated, so the agent's code is never overwritten.
13. **`front-door` and `wire` build and replay by default** (`--no-check` to skip): the design asks
    for replay after each step.
14. **Lift is all or nothing**: it works on a copy and writes back only when every check passes,
    so a failed lift leaves the repository as it was.
15. **Replay in CI is a trycmd test in `rust/app`** (`tests/app_cases.rs`, run by `cargo test`):
    CI then needs only cargo, and decision 2 chose trycmd files for this. No workflow file is
    written; where CI runs stays the user's.
16. **Lift removes generated wrappers only from crates `bridge` made** (they have its manifest);
    any other crate only loses `cdylib` and binding dependencies.
17. **Asset findings are warnings, cross targets are checked, not linked**: neither can be judged
    for certain locally, and an uninstalled target is skipped with the command that adds it.

## Draft pitch

- One line (GitHub description, npm, `Cargo.toml`): "Rewrite your app in Rust, one piece at a
  time, proving nothing changed. TypeScript, Python, C, Go, Ruby, Java."
- Hero tagline: "Rewrite anything in Rust. Keep it working the whole way."
- Sub-line: "rust-anything ports your app piece by piece, checks every step against the original,
  and hands you a pure Rust app."
- `--help`: "Rewrite TypeScript/JavaScript, Python, C, Go, Ruby or Java apps in Rust, one verified
  piece at a time".
