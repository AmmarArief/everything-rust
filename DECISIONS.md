# Decisions

Choices made so far, with reasons. Each one can be revisited.

## Product

| # | Decision | Reason |
| --- | --- | --- |
| D1 | A toolkit your agent drives, not a translator that calls a model itself. | No model cost or vendor lock-in; works with any agent. The checks are the part an agent cannot do reliably. Same shape as REA. |
| D2 | TypeScript/JavaScript first. | No existing tool covers TS/JS to Rust. C/C++ and Python are crowded (DARPA TRACTOR, C2Rust, Depyler). |
| D3 | Core plus one plugin per source language: `src/ts/`, then `src/lang/{python,c,go,ruby,java}.rs` on tree-sitter. | The plan, ordering, scoring and report are the same for every language; parsers, bridges and risks are not. |
| D4 | Written in Rust, parsing with oxc. | Fast (x18n, 27k lines, in about 0.1 s), and the tool uses the language it moves people to. |
| D5 | Shadow mode for every language. | Cloudflare FL2 ran old and new side by side in one process and moved traffic gradually. Tests prove a port; production traffic confirms it. |
| D6 | Working name `carcinize`. | Placeholder. Name to be decided. |

## `analyze`

| # | Decision | Reason |
| --- | --- | --- |
| A1 | A unit is a folder, or a file directly in a package's source root. | Folders are how most codebases mark modules; root files are usually standalone. |
| A2 | Skip `node_modules`, `dist`, `build`, `out`, `target`, `coverage`, `fixtures`, `__fixtures__`, `__snapshots__`, `playground`, `examples`, tool configs and `.d.ts`. | Not product code; fixtures hold sample projects that would distort the plan. |
| A3 | `.tsx`, `.jsx`, `.vue`, `.svelte`, `.astro`, and code importing Vue/React/Svelte/Nuxt or `vscode`, is kept in JS. | The prompt asked to flag UI-heavy parts; editor extension hosts make native modules painful. Their scripts are still parsed for imports. |
| A4 | Bridge: napi-rs when the unit or anything it imports needs Node, or its package's `src/` does; wasm-bindgen otherwise. | Portable code must keep running in browsers; Node-only code gets the faster native bridge. Build scripts next to `src/` do not make a package Node-only. |
| A5 | Waves come from strongly connected components of the unit graph. | Leaves first means a Rust unit never calls back into JS; cycles must move together. |
| A6 | Risks are heuristics: method names like `slice` also match arrays. | False positives cost a look; false negatives cost silent bugs. Each risk lists its first lines so a human or agent can check quickly. |
| A7 | Readiness = 100 - weighted risks (log of count) - size - fan-in - dependency penalties + test bonus. | Log keeps big files from drowning in counts; fan-in pushes widely-used units later, where a mistake costs more. |
| A8 | Priority = half readiness, half value (loops and string work per line, tests, size between 80 and 600 lines). | Readiness alone picks trivial declaration files. A first port should be safe and worth the bridge. |
| A9 | npm to crate table is hand-curated; unknown packages are reported as `unknown`, never guessed. | The prompt: flag missing equivalents instead of hand-rolling silently. |
| A10 | JSON output carries `schema: carcinize/analyze@1`. | Agents read it; it is a wire contract and gets versioned. |

## The loop

| # | Decision | Reason |
| --- | --- | --- |
| L1 | `bridge` splits `lib.rs` (bindings) from `port.rs` (plain Rust, `#![forbid(unsafe_code)]`). | Binding macros expand to unsafe code; the port itself never needs it, and it stays testable without a JS runtime. |
| L2 | `bridge` refuses to write into a non-empty folder, and refuses `blocked`/`keep` units without `--force`. | A port in progress is the user's work; the plan's verdicts should not be bypassed by accident. |
| L3 | napi bindings catch panics and throw JS errors. | A panic unwinding into Node aborts the process; in shadow mode that would take the app down. |
| L4 | TS types map to `String`, `f64`, `bool`, `Vec`, `Option`; anything structured crosses as JSON. | Faithful to JS (numbers are f64) and always compiles; the port can introduce real types behind the binding. |
| L5 | `record` generates deterministic inputs from parameter types, with edge-case corpora, and derives one string argument from another half the time. | Reproducible recordings; search-like functions rarely hit on unrelated random strings. |
| L6 | Recording and diffing run in Node through generated scripts, with a lossless codec (`undefined`, `NaN`, `-0`, BigInt, Map/Set, key order, lone surrogates). | The comparison sees exactly what a JS caller sees. Lone surrogates were found by the fixture tests: JS `slice` can split emoji. |
| L7 | Both sides throwing counts as agreement; messages are not compared. | Error text is rarely a contract and would make every port fail on wording. |
| L8 | wasm builds use `cargo` + `wasm-bindgen-cli` (version-checked), not wasm-pack. | wasm-pack downloads tools at run time, which fails offline and in locked-down CI. |
| L9 | Shadow mode returns the original's result, runs Rust second, and never lets Rust errors reach callers. | Shipping shadow code must not change behavior; only the report sees Rust. |
| L10 | The MCP server is a small hand-written JSON-RPC loop on serde_json. | The stdio transport needs four methods; an SDK would add churn for no gain. |

## Learned from porting x18n's core

| # | Decision | Reason |
| --- | --- | --- |
| X1 | `bridge` keeps exports in TS when their types hold functions (followed through imports, re-exports and `extends`), when they are generic, iterators, RegExp, or use `instanceof`; `--keep` adds more by hand. | 26 of x18n core's 55 exports are of this kind; ported, they would have changed the public API or broken silently. |
| X2 | Structured values cross with markers for `undefined`, NaN/Infinity/-0 and lone surrogates; the loader pads omitted structured arguments with `undefined`. | Each one produced real mismatches: `splitKey`'s `namespace: undefined`, `toWireRange` with NaN, `diffText` splitting an emoji, `isOutdated(change)` with one argument. |
| X3 | Overload signatures (no body) are skipped; type predicates map to `bool`; aliases resolve through imports; untyped defaults take the default's type. | `definePlugin` was listed twice; `isMessageLeaf` returned JSON; `KeyString` became JSON instead of `String`. |
| X4 | `record` installs a resolve hook, takes `--resolve-from`, and retries with `--experimental-transform-types`. | x18n's sources import without extensions, need `valibot` and `nostics`, and use parameter properties. |
| X5 | `bench` is a separate step, not part of `gate`. | Speed is not correctness, but it decides whether a port should ship: every x18n core port was correct and slower. |
| X6 | Risk guidance names `shift_remove` and UTF-16 string ordering. | `serde_json::Map::remove` reorders keys under `preserve_order`; JS `<` on strings compares UTF-16 units. |

## More languages

| # | Decision | Reason |
| --- | --- | --- |
| M1 | Every language maps its types into the TS-like dialect (`string`, `number`, `int`, `T[]`, `Record<string, T>`, `T \| undefined`). | The planner, input generator and port stubs stay one implementation; a language plugin only discovers, scans, resolves and lists exports. |
| M2 | Python uses PyO3 (abi3), C a plain C ABI, and Go, Ruby and Java one JSON dispatcher (`carcinize_call`, or one JNI method) behind a generated wrapper with the original API. | PyO3 and the C ABI are each language's native extension path. For the other three, one dispatcher keeps the Rust side identical, and the generated wrapper keeps callers unchanged. |
| M3 | `diff` goes through the same wrapper callers will use (cgo package, Fiddle file, JNI classes, PyO3 package). | The test must see what callers see: wrapper bugs, type conversions and exception mapping included. |
| M4 | Record and diff run in the source language's own runtime, with generated drivers; nothing is written into the original repository (Go builds its driver as a separate module with `replace`). | The original must run exactly as it does for its callers, and carcinize must not edit code it is migrating. |
| M5 | C records each case in a forked child; a crash is recorded as `crash: <signal>` and accepts any port behavior. | Inputs outside a C function's contract are undefined behavior: real glibc segfaults on `isalpha(INT_MAX)`. |
| M6 | C strings arrive in the port as `Option<&[u8]>`; pointer parameters other than `const char *`, pointer returns, structs and variadics stay in C. All `unsafe` is in the generated `lib.rs`, each with a `// SAFETY:` comment. | Ownership of buffers and structs has to be designed, not guessed; the port itself stays `#![forbid(unsafe_code)]`. |
| M7 | Errors compare by exception type (Python, Ruby, Java), Go `error` values by message, Go panics by being panics. A port signals one with `Err("TypeName: message")`. | Messages are rarely a contract, except Go's error strings, which callers often compare. |
| M8 | Unannotated parameters used as numbers become `number` (Python, Ruby); Python recording passes integral numbers as ints in every fourth case. | `colorsys` got 1,709 garbage cases out of 1,800 without inference, and its int results were never recorded without ints. |
| M9 | `serde_json` runs with `float_roundtrip` in carcinize and in every generated crate. | The default parser can be one ULP off; Java's `Double.toString` exposed it in 43 cases. |
| M10 | Shadow per language: a Go package, a Ruby file that `prepend`s, `<Class>Shadow` Java classes, a C file with `-D` renames and a `shadow` crate feature. | Each is the least invasive drop-in that language allows. |

## Speed

| # | Decision | Reason |
| --- | --- | --- |
| S1 | No JSON text on the hot path: Python converts objects in Rust (`pyconv.rs`), Ruby is a magnus extension, Java and Go get typed JNI and cgo functions for numbers, booleans and strings; JSON remains the fallback for other types in Java and Go. | JSON cost 10-50x the work: the colorsys port took 11.7 µs per call, CPython 0.5 µs. |
| S2 | Ports of untyped Python can use `pynum::Num`, Python's int-or-float, instead of `serde_json::Value`. | A `Value` per number cost about 600 ns per call in colorsys; `Num` keeps ints as ints at native speed. |
| S3 | `bench` decodes arguments once and times only the calls, in the original's process; C benchmarks link the original (renamed with `-D`) and the port into one C program. | Decoding per call, or calling through ctypes, hid the difference being measured. |
| S5 | Go and Java: types beyond numbers, booleans and strings (lists, arrays, maps, structs, boxed and optional values, several results) cross as a compact binary wire, with encoders and decoders generated per type on both sides; the JSON dispatcher is only the fallback for types the wire cannot express. | `encoding/json` and the Java JSON codec cost 5-8 µs per call; the wire costs 0.4-1 µs, and a Java `List<String>` function became faster than Java. |
| S6 | Java ports report errors through a per-thread `String[2]` slot passed with each call; the generated Java throws the exception (constructors cached as method handles). Native code throws only for null arguments. | An exception raised from JNI costs about 1.5 µs more than one thrown in Java: the JVM unwinds the calling compiled frame on a slow path. A throwing `parseRadix` went from 3.0 to 1.8 µs; calls that succeed cost the same. |
| S4 | `bench` ends with a verdict: ship, keep the slower functions, port a bigger unit, or (for C) optimize the port. Faster is 1.1x or more, slower under 0.9x. | Numbers alone left the decision to the reader; Go and Java leaf functions cannot win per call, and the tool should say so. |

## Known limits

- Tests that import a package by name credit the package, not a unit.
- No runtime profiling yet: "compute-heavy" is a static guess from loops and string work.
- The bridge copies strings and structured values on every call. For fine-grained functions this
  dominates: measure with `bench`. A JSON-free fast path (typed arrays, flat results) is not
  generated yet.
- Shadow mode compares synchronous Rust results; functions that mutate their arguments are run on
  already-mutated input by the second implementation.
- C: only scalar and `const char *` APIs cross the bridge; output buffers (`char *out, size_t *len`)
  and state structs stay in C. Functions defined through macros are not seen.
- Go: methods, generics, variadics, pointers, interfaces and channels stay in Go; struct
  parameters cross as JSON with their exported fields.
- Ruby: module functions only (`def self.x`, `module_function`); classes and methods that take
  blocks stay in Ruby. Without YARD tags, parameters not used as numbers cross as untyped JSON.
- Java: `public static` methods of public classes only; overloads, generics, `char` and object
  types stay in Java. `record` compiles the module's sources with `javac` alone, so code that
  needs Maven dependencies on the classpath cannot be recorded yet.
- No way yet to mark a recorded case as undefined behavior that does not crash (`ispunct(1000)`);
  remove it from `cases.jsonl` by hand.
- Go and Java: leaf functions are faster in their own language, since one call into Rust costs
  more than their work; `bench` recommends porting bigger units there. Interfaces, pointers,
  channels, Java objects other than strings, lists, arrays and maps, and Java primitive arrays
  other than `int[]`, `long[]`, `double[]` and `boolean[]` still cross as JSON.
- Ruby: aliases made with `alias` copy the original method, so shadow mode does not see calls
  through them.
