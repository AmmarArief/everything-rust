# Real code in six languages

carcinize was run end to end (analyze, bridge, record, port, diff, gate, bench) on real code in
each language it supports. The code had to be already on this machine: code downloaded from the
internet was not executed. So the targets are standard-library modules shipped with the
installed toolchains, and for Java and C, where no suitable library source was on disk, the JDK
and glibc themselves serve as the oracle behind a thin facade.

The ports, recorded cases and extra inputs are in [`demo/real-world`](../demo/real-world/README.md).

| Language | Code | Functions | Recorded cases | Result | Speed, bridge included |
| --- | --- | ---: | ---: | --- | --- |
| Go | `path` from Go 1.24's standard library | 7 (+1 kept) | 2,642 (542 from Go's own test tables) | 2,642/2,642, gate passes | 0.13-0.23x |
| Ruby | `shellwords.rb` from Ruby 3.3 | 3 | 1,617 (446 raise) | 1,617/1,617, gate passes | 0.49-0.88x |
| Python | `colorsys.py` from CPython 3.13 | 6 | 2,400 | 2,400/2,400, gate passes | 0.34-0.45x |
| Java | JDK 21 behind a facade (`Integer`, `Long`, `Math`, `Double.toString`, `URLEncoder`, `Base64`, `toUpperCase`) | 11 | 4,492 (226 throw) | 4,492/4,492, gate passes | 0.05-0.41x |
| C | glibc behind a facade (`strtol`, `atoi`, `strtod`, `strspn`, `strcmp`, ctype) | 9 | 1,860 (6 crash) | 1,859/1,859 after dropping one undefined-behavior case, gate passes | about 1x (see below) |
| C | aklomp/base64 (vendored in mypyc's runtime) | 0 of 7 | - | plan only: every function takes output buffers or a state struct | - |

Speed below 1x means the Rust port, called the way the original's callers call it, is slower.
For these small functions the bridge (cgo, Fiddle, JNI or PyO3 plus JSON) costs more than the
work. That matches the x18n result for TypeScript: move whole hot paths, not leaf functions. The
C numbers go through Python's ctypes on both sides, which dominates; they do not measure C
against Rust.

## What the runs found

### Bugs in carcinize, fixed

- **Floats changed by one ULP** (Java run). `serde_json`'s default float parser is not always
  correctly rounded. carcinize perturbed recorded inputs when it read them, and generated crates
  perturbed them again. 43 `Double.toString` cases failed for a correct port. Fixed with
  `serde_json`'s `float_roundtrip` feature in carcinize and in every crate it generates (napi-rs,
  wasm and the JSON dispatcher).
- **C crashes killed recording** (C run). Inputs outside a C function's contract (`isalpha(INT_MAX)`)
  segfault glibc. The C driver now runs each case in a forked child and records
  `crash: SIGSEGV`; `diff` accepts any port behavior there, since C's is undefined.
- **`module_function :name` not seen** (Ruby run). `shellwords` declares its module functions
  with symbol arguments after the methods; the exporter only knew the bare `module_function`
  form. Both forms are found now.
- **Untyped code got garbage inputs** (Python run). `colorsys` has no type hints, so 1,709 of
  1,800 generated cases were `TypeError`s. Unannotated parameters used as numbers (`r - g`,
  `max(r, g, b)`, `x < 1`) are now typed `number`, in Python and Ruby: 94 errors remain, all real
  (`ZeroDivisionError`).
- **Ints passed as floats never recorded** (Python run). Python callers pass `1` as often as
  `1.0`, and `colorsys` passes ints through (`hsv_to_rgb(0, 0, 7)` is `(7, 7, 7)`). Recording now
  passes integral numbers as ints in every fourth Python case; that exposed 134 mismatches in a
  port typed `f64`.
- **Messages from the TypeScript days**: "must stay in JS", "kept in TS", "characterization tests
  against the TS code" now name the source language.
- **Reopened core classes** (`class String` in `shellwords.rb`) are now a monkeypatching risk.

### Behavior a port has to match, found by `diff`

Each line was a mismatch in a first port, caught by the recorded cases. Lines marked *fixture*
came from the small test projects in `tests/fixtures/`, written for each language; the rest from
the real-world runs above.

- Go `path.Clean`: Go's `lazybuf` reads the byte at the write index after it moves back, not the
  last kept byte (9 cases, from Go's own test table).
- *fixture* Go `strings.ToLower` and Ruby `downcase` lower each rune alone: `ΣΊΣΥΦΟΣ` becomes `σίσυφοσ`,
  where Rust's `to_lowercase` applies the final-sigma rule. Go also maps `İ` to `i`, where Rust
  gives `i̇`.
- *fixture* Go slices strings by bytes; a cut through a rune reaches JSON as one U+FFFD per byte.
- *fixture* Ruby `slugify` raised `NoMethodError` for a negative `max` (`slug[0, -1]`
  is `nil`): a bug in the original that the port has to keep or the callers have to be fixed first.
- *fixture* Ruby `Array#sum` uses Kahan-Babuska summation and starts from integer 0, so `[-0.0].sum` is `0.0`.
- Ruby `Float#to_s` uses exponent form from 1e15 up (`1e15.to_s` is `1.0e+15`, `1e14.to_s` is
  `100000000000000.0`); `nil.inspect` inside arrays is `nil`.
- *fixture* Java `\s` is ASCII-only; Rust's `split_whitespace` also splits at U+00A0.
- Python's int-or-float results: `colorsys` keeps ints through `+`, `-` and `*`, so the faithful port
  computes with a small int-or-float type instead of `f64` (134 cases once ints were recorded).
- glibc `strtod("-")` converts nothing and returns `+0.0`, not `-0.0`.

Handled in the first port, and confirmed by the recorded cases:

- Java `Integer.parseInt` accepts any Unicode decimal digit (`١٢`, `２`) through `Character.digit`.
- Java `Double.toString` (JDK 19+) prints the shortest decimal, but never fewer than two digits:
  `4.9E-324`, where Rust's shortest form is `5e-324`.
- Python's `max(nan, 1, 2)` is `nan` (the first value wins unless a later one compares greater);
  `-0.0 % 1.0` is `0.0`.
- glibc `toupper(-128)` is `128` (its tables accept signed `char` values); `strtol` with base 0
  reads `077` as octal and `0x` without a hex digit as `0`.

### Limits these runs showed

- **C APIs that pass buffers.** aklomp/base64's public functions take `char *out, size_t *outlen`
  or a `struct base64_state *`. The C bridge carries scalars and `const char *` only, so it keeps
  them all in C, with the reason. Supporting the output-buffer pattern is the next step for C.
- **C macros.** 43 of base64's 51 files define functions through macros
  (`BASE64_ENC_FUNCTION(arch)`); the parser reports errors there and the plan marks those units
  partial. SIMD intrinsics are not called out as a risk yet.
- **Undefined behavior that does not crash.** `ispunct(1000)` reads past glibc's table and
  returns 1. No port should copy that; the one case was removed from `cases.jsonl` by hand. There
  is no built-in way to mark a case as undefined yet.
- **Dynamic typing costs code.** The `shellwords` port reimplements Ruby's `to_s` and `inspect`
  for numbers, nil, arrays and hashes, because `shellescape` accepts anything. The `colorsys`
  port needs Python's numeric tower. A typed port is shorter, but changes behavior for some
  callers; `diff` shows exactly which.
- **The facades.** The Java and C runs test real JDK and glibc behavior, but through facades
  written for the run, not through a library's own API.

## How each language runs

| | Record | Diff | Bridge | Shadow |
| --- | --- | --- | --- | --- |
| Go | generated driver, built as its own module with a `replace` to the original | the same driver over the cgo wrapper | JSON dispatcher + cgo package with the original API | package with the original API, runs both |
| Ruby | generated driver loads the original files | the same driver loads the Fiddle wrapper | JSON dispatcher + Fiddle wrapper defining the same modules | file to require after the original: `prepend`s both |
| Java | reflection driver compiled with the original sources | the same driver compiled with the JNI wrapper classes | JSON dispatcher over JNI + wrapper classes with the original static methods | `<Class>Shadow` classes |
| Python | in-process driver imports the original | the same driver imports the PyO3 package | PyO3 | module with the original API |
| C | `cc -shared` of the original, called through ctypes, one fork per case | ctypes over the Rust `cdylib` | `extern "C"` functions with the original names and a header | shadow `.c` with `-D` renames; the crate's `shadow` feature exports only `carcinize_` names |
