# Real-world ports

The crates carcinize produced in the runs described in
[`docs/real-world-languages.md`](../../docs/real-world-languages.md): each holds the Rust port
(`src/port.rs`), the generated bridge, `cases.jsonl` (the recorded behavior of the original) and,
where used, `inputs.jsonl` (extra inputs passed to `carcinize record --inputs`).

| Crate | Original | Where it came from |
| --- | --- | --- |
| `go-path-rs` | Go's `path` package | `$(go env GOROOT)/src/path` (Go 1.24), as module `example.com/gopath`, with `internal/bytealg` calls replaced by their `strings` equivalents |
| `ruby-shellwords-rs` | Ruby's `shellwords.rb` | `RbConfig::CONFIG["rubylibdir"]/shellwords.rb` (Ruby 3.3), as `lib/shellwords.rb` of a project with a `Gemfile` |
| `python-colorsys-rs` | Python's `colorsys.py` | CPython 3.13's `Lib/colorsys.py`, as `src/colorsys_pkg/colorsys.py` of a project with a `pyproject.toml` |
| `java-jdk-rs` | JDK 21 behind `original/Jdk.java` | `original/Jdk.java` in `src/main/java/com/example/jdk/` of a project with a `pom.xml` |
| `c-libc-rs` | glibc behind `original/facade.c` | `original/facade.c` in `src/` of a project with a `Makefile` |

The originals are not copied here: the standard-library sources keep their own licenses, and the
ports name theirs in `src/port.rs`. To rerun one, put the original in place as described, point
`carcinize.json`'s `root` at that project, and run:

```sh
carcinize record <crate> --inputs <crate>/inputs.jsonl   # or keep the recorded cases.jsonl
carcinize gate <crate>
carcinize bench <crate>
```
