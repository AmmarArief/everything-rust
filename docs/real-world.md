# Real-world runs

What happened when carcinize met real code, and what changed in the tool because of it.

## x18n (the case-study repository)

| Target | What ran | Result |
| --- | --- | --- |
| `packages/core`, 9 units, 29 functions | analyze, bridge, record, diff, bench, shadow, gate | every gate passes; 9,000+ recorded cases match; slower than TS through a bridge ([report](../demo/x18n-core/README.md)) |
| `packages/builtins/src/formats/shared/`, 7 functions | same | 3,672/3,672 cases match, 960 of them built from 40 real files of the repository (YAML locales, JSON, Markdown, TS), in LF and CRLF |
| whole repository | analyze | 142 units; `start_here` now lists only units with bridgeable exports |

Found in the tool, and fixed:

- `analyze` recommended the format parsers first, but their only exports are plugin factories
  (`properties(): Plugin`). `analyze` now counts bridgeable exports with the rules `bridge`
  applies, and leaves units with none out of `start_here`.
- Types imported from workspace packages (`@x18n/core`) were not followed, so `Plugin` looked
  like data. Type lookup now follows workspace packages to their source.
- `record` could not run a module importing `@x18n/core` at runtime: the package has no build
  output. The resolve hook now maps workspace packages to their source.
- Shaped inputs found a real bug in the `text.ts` port (Infinity in a range), then an order
  dependency on V8's sort with NaN comparators; `js::sort` now reproduces V8.

## Fixtures written to stress the tool

| Fixture | Exercises | Result |
| --- | --- | --- |
| `tests/fixtures/async` | async functions on both bridges | 900/900, both gates pass |
| `tests/fixtures/classes` | classes: state, constructor errors, `Map` order, `split('')` on emoji | 600/600 scenarios, both gates pass |
| `tests/fixtures/aliases` | tsconfig `paths` with `extends`, comments, trailing commas | analyze and record follow them |
| `tests/fixtures/monorepo` | a package importing a sibling with no build output | record resolves it to source |

Found in the tool, and fixed: napi turned `None` into `null` (JS gives `undefined`); scenarios
too short and too random to catch a stateful bug (now up to 12 calls, reusing earlier strings);
generated `let out = ()` and immediately-called closures that clippy rejects.

## External libraries (static only)

`unjs/scule`, `unjs/ufo` and `unjs/pathe` were cloned and analyzed and bridged. Running their
code (`record` executes the original) was refused by the session's safety policy for code
downloaded from the internet, so these runs stop before `record`.

| Library | analyze | bridge |
| --- | --- | --- |
| scule (case conversion) | 2 units, ready | all 11 functions bridgeable after the fix below |
| ufo (URL utilities) | 7 units, ready; `punycode.ts` ranked first | `toASCII` bridged (untyped JS: crosses as JSON) |
| pathe (path utilities) | 5 units; `_glob.ts` caution (dynamic regex) | 13 functions bridged; `typeof path.join`-typed exports cross as JSON |

Found in the tool, and fixed: constrained generics (`camelCase<T extends string | readonly
string[]>`) were treated as "any value, functions included", which kept 10 of scule's 11
functions in TS. Type parameters with data constraints are now substituted by the constraint.

A port of scule was written (`demo` not included: it was never recorded against the original).
