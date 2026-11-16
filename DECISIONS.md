# Decisions

Choices made so far, with reasons. Each one can be revisited.

## Product

| # | Decision | Reason |
| --- | --- | --- |
| D1 | A toolkit your agent drives, not a translator that calls a model itself. | No model cost or vendor lock-in; works with any agent. The checks are the part an agent cannot do reliably. Same shape as REA. |
| D2 | TypeScript/JavaScript first. | No existing tool covers TS/JS to Rust. C/C++ and Python are crowded (DARPA TRACTOR, C2Rust, Depyler). |
| D3 | Core plus one plugin per source language (`src/ts/` today). | The plan, ordering, scoring and report are the same for every language; parsers, bridges and risks are not. Python (PyO3) is the likely second plugin. |
| D4 | Written in Rust, parsing with oxc. | Fast (x18n, 27k lines, in about 0.1 s), and the tool uses the language it moves people to. |
| D5 | Shadow mode is on the roadmap. | Cloudflare FL2 ran old and new side by side in one process and moved traffic gradually. Tests prove a port; production traffic confirms it. |
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

## Known limits

- tsconfig `paths` aliases (`~/`, `@/`, `#`) are not followed; they are counted per unit.
- Tests that import a package by name credit the package, not a unit.
- No runtime profiling yet: "compute-heavy" is a static guess from loops and string work.
