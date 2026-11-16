---
name: carcinize
description: Migrate a TypeScript/JavaScript, Python, C, Go, Ruby or Java codebase to Rust incrementally with the carcinize CLI or MCP tools - plan, bridge one unit at a time, record the original's behavior, port, diff, shadow and gate. Use when asked to rewrite, port or migrate code to Rust, or to plan such a migration.
---

# Incremental migration to Rust with carcinize

Works for TypeScript/JavaScript, Python, C, Go, Ruby and Java; the language is detected (or pass
`--lang`). The steps are the same; the bridge and the wrapper the original's callers load differ
(see the README's language table). Examples below use TS terms; read "the source language" for TS.

carcinize gives you ground truth; you write the Rust. Never port a whole codebase at once, and never
call a port done before `gate` passes.

Tools: the `carcinize` CLI, or the same steps as MCP tools (`carcinize mcp`): `analyze`, `bridge`,
`record`, `build`, `diff`, `shadow`, `gate`.

## 1. Plan, then stop

```sh
carcinize analyze <repo> -o plan.md        # people
carcinize analyze <repo> --json            # you: schema carcinize/analyze@1
```

- Present the `start_here` units (best first ports) and the order by package and wave.
- Call out `keep` units (UI, editor code: stay in TS), `blocked` units and their reason, and npm
  dependencies with fit `none` or `unknown`. Do not hand-roll a missing crate silently: flag it.
- **Wait for the user's approval of the plan and of the first unit.**

## 2. Port one unit

```sh
carcinize bridge <repo> <unit-id>          # crate in <repo>/rust/<name>; never overwrites
carcinize record <crate>                   # the original's behavior on edge-case inputs -> cases.jsonl
```

- Add real-world inputs with `--inputs cases.jsonl` (`{"fn": name, "args": [...]}` per line) when the
  generated ones cannot reach the interesting paths (structured arguments, domain strings).
- If the TS source cannot run directly in Node (extensionless imports, path aliases), record from
  built output with `--module dist/x.mjs` or pass `--node-arg=--import=tsx`.
- If the original has no tests, `record` is the characterization suite: keep `cases.jsonl` in the repo.

Write the port in `src/port.rs` only; `src/lib.rs` is the thin binding layer.

- Read each risk the plan lists for the unit, at its `file:line`, before writing code.
- String offsets: use `src/utf16.rs`. JS indexes UTF-16 code units, Rust UTF-8 bytes.
- Speed: before choosing what to port, profile a real workload: `carcinize profile <repo> -- node
  bench.mjs` (or `python3 bench.py`, `go test -run X -bench . ./pkg`, `java -cp out Main`,
  `ruby bench.rb`). A hot function called in a loop is a batch, not a port:
  `carcinize bridge <repo> <unit> --batch fn:param` (or `refresh <crate> --batch fn:param`), then
  have the caller build the batch once (`new FnBatch(items)`, `NewFnBatch(items)` in Go,
  `Mod::FnBatch.new(items)` in Ruby) and call `rank`/`run` per query; close it in Go and Java. Define
  `pub struct FnIndex` in `port.rs` with `new(items)` and `run(...)` to prepare the items once
  (lowercase, masks, sorted tables); it must give the same result as the function, item by item.
- Speed: run `carcinize bench` and follow its verdict. In Go and Java, a call into Rust costs more
  than a small function's work: port the function with its callers, not alone. In Python ports,
  use `crate::pynum::Num` for numbers that may be int or float, not `serde_json::Value`.
- Errors: return `Err("TypeName: message")` (Python, Ruby, Java exception types; Go `error` text).
- Dynamic languages: inputs that are ints in Python or Ruby must come back as ints where the
  original passes them through; `diff` shows where. C: inputs recorded as `crash:` are undefined
  behavior; any result passes.
- Ordered maps and object keys: use `indexmap` where order reaches output.
- Regex lookaround or backreferences: `fancy-regex`, not `regex`.
- No `unsafe`. If truly needed: minimal, isolated, with a `// SAFETY:` comment.
- Add Rust unit tests in `port.rs` for the cases you reason about.

## 3. Prove it

```sh
carcinize build <crate>
carcinize diff <crate>                     # must be N/N
```

A mismatch is a behavior difference: fix the port, never the recording. If the difference is
intended, ask the user first, then re-record with their approval.

## 4. Ship it safely

```sh
carcinize shadow <crate>                   # drop-in module, same exports as the original
```

Import the shadow module where the app imported the original. `CARCINIZE_MODE=shadow` (default)
returns the original's results and reports Rust mismatches (`onMismatch`, `shadowStats`); `new`
switches to Rust; `old` turns Rust off. The public API does not change.

## 5. Done means gate passes

```sh
carcinize gate <crate>
```

fmt, clippy with warnings as errors, cargo tests, `unsafe` only with SAFETY comments, nothing
`unimplemented!`, module builds, every TS export present, every recorded case matches.

Then report to the user, and **wait for their go-ahead before the next unit**:

- what changed (files, functions, bridge),
- gate output and diff counts,
- behavior differences or risks left (e.g. error messages differ, lone surrogates become U+FFFD
  in a napi `String`), and what is left in the plan.
