---
name: rust-anything
description: Port TypeScript/JavaScript, Python, C, Go, Ruby or Java to Rust with the rust-anything CLI or MCP tools, from one hot function up to a whole app rewritten as a pure Rust app - profile, plan, bridge one unit at a time, record the original's behavior, port, diff, bench, shadow and gate; batch hot loops so they run in Rust on all cores. Use when asked to rewrite, port or migrate code or an app to Rust, to plan such a migration, or to make slow code faster with Rust.
---

# Migration to Rust with rust-anything

rust-anything gives you ground truth; you write the Rust. Port one unit at a time, even when the
goal is the whole app, and call a port done only when `gate` passes.

**Whole app to a pure Rust app** (the user wants the app rewritten in Rust, with the original kept
only as reference): read [whole-app.md](whole-app.md) first and follow it. It grills the user on
the decisions, researches the dependencies, records the whole app, puts a Rust front door in front
of the original, runs the steps below for every unit, then lifts the app to a standalone Rust
binary.

Languages: TypeScript/JavaScript, Python, C, Go, Ruby, Java. The language is detected; `--lang`
overrides it. The steps are the same everywhere; only the bridge differs (napi-rs or wasm for
TS, PyO3, a C ABI, cgo, a magnus extension, JNI), and the original's callers keep their API.

Tools: the `rust-anything` CLI, or the same steps as MCP tools (`rust-anything mcp`): `profile`,
`analyze`, `bridge`, `refresh`, `record`, `build`, `diff`, `bench`, `shadow`, `gate`.
If the MCP tools are not available, use the CLI; if `rust-anything` is not installed, ask the user
to install it (`cargo install --git https://github.com/erkamyaman/rust-anything`) and run
`rust-anything setup`, then restart the agent.

## 0. Profile: find what is worth moving

When the goal is speed, measure before choosing anything:

```sh
rust-anything profile <repo> -- node bench.mjs      # or: python3 bench.py, ruby bench.rb,
                                                #     java -cp out Main, go test -run X -bench . ./pkg
```

It runs a real workload under the runtime's own profiler and lists hot functions with their
callers and advice. Go profiles only through `go test`; C is not supported (use `perf`).

- A small function called once per item is **not** worth porting alone: one call into Rust costs
  more than its work, and JITs (V8, the JVM, Go) run it in nanoseconds. Port it together with its
  callers, or batch it (step 2b). The advice line says which, with the exact command.
- If the user only wants a migration, skip to step 1.

## 1. Plan, then stop

```sh
rust-anything analyze <repo> -o plan.md        # people
rust-anything analyze <repo> --json            # you: schema rust-anything/analyze@1
```

- Present the `start_here` units (best first ports) and the order by package and wave.
- Call out `keep` units (UI, editor code: they stay in the source language), `blocked` units and
  their reason, and dependencies with fit `none` or `unknown`. Do not hand-roll a missing crate
  silently: flag it.
- **Wait for the user's approval of the plan and of the first unit.**

## 2. Port one unit

```sh
rust-anything bridge <repo> <unit-id>          # crate in <repo>/rust/<name>; never overwrites
rust-anything record <crate>                   # the original's behavior on edge-case inputs -> cases.jsonl
```

- Add real-world inputs with `--inputs cases.jsonl` (`{"fn": name, "args": [...]}` per line) when
  the generated ones cannot reach the interesting paths (structured arguments, domain strings).
- TS that cannot run directly in Node (extensionless imports, path aliases): record from built
  output with `--module dist/x.mjs`, or pass `--node-arg=--import=tsx`.
- If the original has no tests, `record` is the characterization suite: keep `cases.jsonl`.

Write the port in `src/port.rs` only; `src/lib.rs` and the wrappers are generated. After changing
a signature in `port.rs`, run `rust-anything refresh <crate>`.

- Read each risk the plan lists for the unit, at its `file:line`, before writing code.
- String offsets: JS and Java index UTF-16 code units, Rust UTF-8 bytes: use `src/utf16.rs`.
- Errors: return `Err("TypeName: message")`; it becomes that exception in Python, Ruby and Java,
  and the `error` text in Go.
- Numbers in Python and Ruby: ints that the original passes through must come back as ints;
  in Python ports use `crate::pynum::Num` for int-or-float, not `serde_json::Value`.
- C: inputs recorded as `crash:` are undefined behavior; any result passes.
- Ordered maps and object keys: `indexmap` where order reaches output.
- Regex lookaround or backreferences: `fancy-regex`, not `regex`.
- No `unsafe`. If truly needed: minimal, isolated, with a `// SAFETY:` comment.
- Add Rust unit tests in `port.rs` for the cases you reason about.

## 2b. Batch a hot loop

For a function called over many items (search, scoring, formatting a list):

```sh
rust-anything bridge <repo> <unit> --batch fn:param    # or: rust-anything refresh <crate> --batch fn:param
```

This generates a class that takes the `param` values once and keeps them in Rust:
`new FnBatch(items)` (TS, Python, Java), `Mod::FnBatch.new(items)` (Ruby), `NewFnBatch(items)`
(Go). Per query, `run(<other args>)` returns every result as one array, and `rank(<other args>,
min)` returns the item indexes, best first. It runs on all cores, and errors are the original's.
Close the batch when done in Go and Java (finalizers are only a fallback).

- Make the caller build the batch once, not per query. Map returned indexes back to items with a
  plain loop; in JS, `Array.from(order, fn)` costs 20x more.
- To prepare items once (lowercase, masks, position tables), define `pub struct FnIndex` in
  `port.rs` with `new(items)` and `run(<other args>) -> Result<Vec<R>, String>`. It must give the
  same result as the function, item by item: test it against the function over every item.
- When ranking can skip work, give the index its own `rank(<other args>, min) -> Result<Vec<u32>,
  String>` (narrowing to the last query's matches as the user types, table lookups). It must
  match ranking the function's results: score high to low, ties in item order.
- A batch multiplies the port's per-item speed by the cores. Against compiled originals (Go,
  Java) the port itself must be fast: handle ASCII without Unicode tables, build each result in one
  pre-sized `String`, avoid intermediate copies. Keep `diff` passing.
- Items and other arguments: numbers, booleans and strings (Ruby: any value); results: numbers,
  booleans or strings. Not for C.

## 3. Prove it

```sh
rust-anything build <crate>
rust-anything diff <crate>                     # must be N/N
rust-anything bench <crate>                    # original vs Rust, bridge included, with a verdict
```

A mismatch is a behavior difference: fix the port, never the recording. If the difference is
intended, ask the user first, then re-record with their approval.

Follow `bench`'s verdict: ship, keep the slower functions in the source language, or port a
bigger unit (batch or include the callers). Do not ship a port that makes callers slower without
saying so.

## 4. Ship it safely

```sh
rust-anything shadow <crate>                   # drop-in with the original's API
```

Load the shadow where the app loaded the original. `RUST_ANYTHING_MODE=shadow` (default) returns the
original's results and reports Rust mismatches; `new` switches to Rust; `old` turns Rust off.
The public API does not change.

## 5. Done means gate passes

```sh
rust-anything gate <crate>
```

fmt, clippy with warnings as errors, cargo tests, `unsafe` only with SAFETY comments, nothing
`unimplemented!`, every place the original fails on purpose (`throw`, `raise`, an `error` result)
reached by a recorded failing case (add one with `record --inputs` when gate names it), no
`unwrap`, `expect` or `panic!` in a function whose original returns an error (return `Err`; for a
state the original also treats as impossible, a `// INFALLIBLE: <why>` comment), the module builds,
every export present, every recorded case matches.

Then report to the user, and **wait for their go-ahead before the next unit**:

- what changed (files, functions, bridge, batches),
- gate output, diff counts and bench numbers,
- behavior differences or risks left (e.g. error messages differ, lone surrogates become U+FFFD
  in a napi `String`), and what is left in the plan.
