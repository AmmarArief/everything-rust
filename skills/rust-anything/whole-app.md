# Whole app to a pure Rust app

The goal: a standalone Rust app that does what the original does, with the original kept only as
reference. The app stays runnable at every step: a Rust **front door** owns the entry point early
and hands whatever is not ported yet to the original, units move to Rust one at a time through
steps 1 to 5 of [SKILL.md](SKILL.md), and the **lift** at the end removes the fallback. The user
makes the decisions; you find the facts.

Everything you write for the port lives in `<repo>/rust/`: `DECISIONS.md`, `research/`,
`app-cases/`, the unit crates, and the `app/` crate. Why this shape, with sources:
https://github.com/erkamyaman/rust-anything/blob/main/docs/research/whole-app-port.md

## 1. Grill the user

Interview the user until every decision the port depends on is settled. Map them as a **design
tree**: each decision branches into the ones that hang off it. Ask in **rounds**: a round is the
whole **frontier**, every decision whose prerequisites are already settled. Number each question,
give your recommended answer, and word it so "yes" accepts the recommendation:

```
❓ **Q1** - **<title>**: <question, with the choices>

➡️ <recommended answer>
```

Wait for the answers, recompute the frontier, ask the next round. A question that depends on
another open question waits for a later round.

Facts are yours to find, never the user's: run `analyze` and `profile`, read the code, and send
subagents for the rest. Questions downstream of a running lookup wait; ask the rest now.

Seed branches (the tree grows from the answers):

- **Shape**: CLI, HTTP server, worker, library, desktop or UI app. Which entry points.
- **Scope**: every feature, or drop dead and deprecated ones. `keep` units (UI, editor code):
  their Rust replacement, or the part that stays outside Rust.
- **Equivalence bar**: byte-for-byte output, or equal after normalizing (key order, float
  formatting, whitespace, error text). Exit codes, error messages, log lines.
- **Compatibility**: config files, environment variables, on-disk and wire formats, CLI flags,
  HTTP routes and status codes that must stay the same.
- **Dependencies**: a Rust crate for every external dependency the plan lists, most of all fit
  `none` or `unknown`. Async runtime, HTTP framework, database layer.
- **Targets**: platforms, minimum Rust version, packaging, binary size or speed goals.
- **Checkpoints**: when you stop for the user's go-ahead (default: after each wave of the plan).
- **Cutover**: switch at once, or run the Rust app beside the original first.

Done when the frontier is empty and the user confirms you share one understanding. Write every
decision with its reason to `rust/DECISIONS.md`; it is the source of truth for the rest of the
port.

## 2. Research, in the background

For each dependency and framework decision, and each risk the plan flags as `high`, send a
background subagent to research it against **primary sources** only: the source library's own
docs and code, docs.rs, the crate's repository, language specs. Each writes one file,
`rust/research/<topic>.md`, citing the source of every claim and listing the semantic traps it
found.

Keep working while they read: only units that depend on a pending topic wait for it. Read a
topic's file before porting any unit it covers.

## 3. Record the whole app

Capture what the app does from the outside, so every later build is held to it:

- **CLI**: arguments, stdin and environment in; stdout, stderr, exit code and files written out.
- **HTTP server**: requests in; status, headers that matter and bodies out.
- **Library**: the public API, through `rust-anything record` on each entry unit.

Inputs: the app's tests, examples and real workloads, its error paths (bad input, missing files,
failed calls), and the semantic traps: non-ASCII and emoji strings, negative division and
modulo, integer overflow, large and tiny floats, map and set order, regex lookaround.

Run the original twice on every case. Where its own runs disagree (timestamps, ids, ordering),
the difference is **noise**: normalize it in the case, never in the port. Store the cases in
`rust/app-cases/` with a `replay` script that runs any build against them and prints
`passed/total`. The original must pass 100%.

## 4. Build the front door

Create `rust/app`, the Rust binary that owns the entry point from now on, with the argument
parser, config loading and server setup from `DECISIONS.md`. Everything it does not handle yet
falls back to the original:

- **CLI**: run the original with the same arguments, stdin and environment, and pass its output
  and exit code through.
- **HTTP server**: proxy every route not ported yet to the original server.

`replay` against the front door must match 100% before any unit moves. Count the fallbacks: the
**fallback rate** is the share of cases the original still answers, and it only goes down.

## 5. Port every unit

Follow the plan's waves, lowest first, so every unit's dependencies are already Rust; take a
tangled core in one large piece rather than writing glue around it. Each unit goes through steps 1
to 5 of SKILL.md, then:

1. Wire the unit into the front door, and stop falling back for what it covers.
2. Run `replay`: 100% again, and a lower fallback rate.
3. Keep a **kill switch**: one environment variable per unit sends it back to the original, until
   the lift.

Translate faithfully first, with the original's structure, types and error paths; make it
idiomatic only after `replay` passes, and run `replay` again. Where the original returns or throws
an error, return an error: `unwrap`, `expect` and `panic!` are for states the original also
treats as impossible.

Stop at the checkpoints from `DECISIONS.md`, and report the units done, gate results, `replay`
results, the fallback rate and what is left.

## 6. Lift to a pure Rust app

When the fallback rate is zero and every unit passes `gate`:

1. Delete the fallback path and the kill switches.
2. Call the units' `port.rs` code directly from `rust/app`; drop the bindings and the generated
   wrappers.
3. Work the lift checklist: assets and templates embedded or shipped, every target from
   `DECISIONS.md` builds, packaging and release, CI runs `replay`.
4. Run `rust-anything gate` on every unit crate, and `cargo clippy --all-targets -- -D warnings`
   and `cargo test` on `rust/app`.

## 7. Prove the app, then cut over

Run `replay` against the pure Rust app: every case matches to the bar in `DECISIONS.md`. A
mismatch is a bug in the port, never in the cases; change a case only with the user's approval,
and record why in `DECISIONS.md`.

Report to the user: replay results, gate results for every crate, speed against the original, and
every known behavior difference. The user decides the cutover. Leave the original in place as
reference until they say otherwise.
