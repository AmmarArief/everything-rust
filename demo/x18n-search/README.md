# x18n search in Rust

x18n's sidebar search (`fuzzyScore` from `packages/core/src/fuzzy.ts` over every key, then
filter and sort) moved to Rust as a batch, with the keys kept on the Rust side.

```sh
carcinize profile ../x18n -- node --experimental-strip-types $PWD/search.ts .   # fuzzyScore: ~25%, per key
carcinize bridge ../x18n packages/core/src/fuzzy.ts --batch fuzzyScore:text
# port fuzzyScore, then add FuzzyScoreIndex to src/port.rs
carcinize gate .                                   # 2000/2000 recorded cases match
carcinize build . && node --experimental-strip-types bench.ts ../x18n
```

- `src/port.rs`: `fuzzy_score`, the port, and `FuzzyScoreIndex`: keys lowercased to UTF-16 once,
  in one buffer, with a mask of the code units in each key. A unit test checks that it scores
  exactly like `fuzzy_score`.
- `src/lib.rs` (generated): `FuzzyScoreBatch` with `run(query)` (every score, a `Float64Array`)
  and `rank(query, min)` (indexes of scores >= `min`, best first, a `Uint32Array`).
- `search.ts`: the workload `profile` ran: the TS search, 14 queries, three times each.
- `bench.ts`: checks the results are identical to the TS pipeline, then times both.

On 4 cores: TS 8.5 ms per query, Rust 1.2 ms, **7.0x faster**. With wasm (one thread), 2.9x.
