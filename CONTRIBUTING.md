# Contributing to rust-anything

Thanks for helping. Bug reports, fixes, new language support and write-ups of real ports are all
welcome, and you don't need to ask before opening a pull request for a bug fix.

## Before you start

- **A bug:** open an [issue](https://github.com/erkamyaman/rust-anything/issues/new/choose) with
  the command you ran, its output, your platform and the rust-anything version
  (`rust-anything --version`). A tiny repro project helps most.
- **A bigger change** (a new language, a new command, a change to what a unit or a recording is):
  open an issue first so we can agree on the shape before you write it. The reasons behind past
  choices are in [DECISIONS.md](./DECISIONS.md).
- **A security problem:** don't open a public issue; see [SECURITY.md](./SECURITY.md).

## Set up

You need Rust (stable, 1.97 or newer). The tests also run the language each part handles, and
skip a part whose runtime is missing, so install the ones you touch:

| For | Install |
| --- | --- |
| TypeScript/JavaScript | Node 24 (22.6+ works except for `profile`) |
| Python | Python 3.9+ |
| C | a C compiler (`cc`) |
| Go | Go 1.22+ |
| Ruby | Ruby 3.x |
| Java | a JDK (17+) |
| PHP | PHP 8 |
| C# | the .NET SDK |
| wasm bridges | `rustup target add wasm32-unknown-unknown` and the matching `wasm-bindgen-cli` |

```sh
git clone https://github.com/erkamyaman/rust-anything
cd rust-anything
cargo test
```

## Make the change

- **Every fix comes with a test that fails without it.** Run the test against the old code once to
  be sure it does.
- **Where tests go:**
  - `tests/analyze.rs`: language detection, units, risks, dependencies, `--whole-app`.
  - `tests/loop.rs`: bridge, record, diff and shadow per language, with stand-ins instead of builds.
  - `tests/native.rs`: bridges built for real, then loaded and replayed.
  - `tests/app.rs` and `tests/http.rs`: whole-app record, replay, front door and lift, for CLIs and
    servers.
  - `tests/library.rs`: whole-library ports.
  - Unit tests live next to the code.
- **Fixtures** are small apps under `tests/fixtures/`. The CLI and HTTP fixtures implement the same
  commands and routes in every language (`tcli upper|len|div|words|copy|now`, `/hello`, `/upper`,
  `/div/{a}/{b}`, `/now`), so keep a new one identical in behavior to the others.
- **Generated code** (the bridges, drivers and front doors rust-anything writes) has to pass
  `clippy -D warnings` too, since `gate` runs it on users' crates.
- **Style:** match the code around you. Keep comments for what the code can't say on its own, and
  put the reasoning for a change in the pull request instead.

## Add a language

PHP and C# were added as previews in two commits worth reading first:
[e8ac243](https://github.com/erkamyaman/rust-anything/commit/e8ac243) (PHP) and
[caea59a](https://github.com/erkamyaman/rust-anything/commit/caea59a) (C#). A new language starts
the same way:

1. A plugin in `src/lang/<name>.rs`: discovery, scan (imports and risks), resolve, the
   package-to-crate table and exports.
2. Detection and dispatch in `src/lang/mod.rs`.
3. Whole-app support in `src/whole_app.rs`: entry points, servers, declared dependencies.
4. The CLI and HTTP fixtures, a library fixture, and tests in `tests/analyze.rs`, `tests/app.rs`
   and `tests/http.rs`.
5. `bridge` refuses the language's units until a bridge exists, with a clear message.

## Open the pull request

Before you push:

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

- Say what was wrong, how to reproduce it, and how the change fixes it.
- Link the issue (`Fixes #123`).
- Note anything you couldn't test (another OS, a runtime you don't have).
- Keep it to one change: two fixes are easier to review as two pull requests.

CI runs on Ubuntu and macOS with every runtime installed. For a first-time contributor, GitHub
waits for a maintainer to approve the CI run, so expect a short delay before the checks start.

Pull requests are merged with their commits kept, so your history stays yours. If a change needs
small adjustments, a maintainer may add a commit on top rather than ask for another round.

## Share a real port

Ports of real projects are the best evidence of what works and what breaks. If you used
rust-anything on something real, an issue or a pull request adding it to
[docs/real-world.md](./docs/real-world.md) is very welcome. Include the project, the commands you
ran, the case counts, and anything that surprised you.

## Code of conduct

Everyone taking part is expected to follow the [code of conduct](./CODE_OF_CONDUCT.md).
