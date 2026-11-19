# Kotlin/JVM Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Kotlin as a source language with `analyze → bridge → record → diff → bench → gate → shadow`, then an Android/NDK profile.

**Architecture:** Kotlin emits JVM static methods (top-level functions → `<File>Kt`, `@JvmStatic` → enclosing class), so Kotlin exports use Java-shaped `prototype` strings. That lets the existing Java codegen, batch native codegen, JNI `lib.rs`, and reflection driver be reused unchanged. Only discovery/scan/type mapping and the `kotlinc` compile step are Kotlin-specific.

**Tech Stack:** Rust 1.97 (edition 2024), `tree-sitter 0.27.2`, `tree-sitter-kotlin-ng 1.1.0` (uses `tree-sitter-language ^0.1`, same as `tree-sitter-java 0.23.5`), `kotlinc`, JNI/`jni = "0.21"`, `cargo ndk` for phase 2.

**Spec:** `/home/vomc/kotlin-support-issue.md` (design doc / draft issue).

## Global Constraints

- `rust-version = "1.97"`, edition 2024; `unsafe_code = "forbid"` in the crate.
- Never guess a crate mapping: unknown libraries stay `unknown` (DECISIONS A9).
- Generated code must pass `clippy -D warnings` (gate runs it on user crates).
- Do not write into the original repository; harnesses use `.rust-anything/` in the crate dir (M4).
- Kotlin strings are UTF-16; keep `Char` blocked and preserve UTF-16 string crossings (issue #110).
- New language must be added to every `Lang` dispatch site, not only `src/lang/mod.rs`.
- `Lang::detect()` currently maps `build.gradle.kts` → Java; Kotlin must be chosen from source extensions, mixed modules stay Java in phase 1.

## Review Focus

- Mixed `.kt`/`.java` Gradle module → must stay Java, not break existing Java projects.
- Top-level Kotlin function → must produce `owner = <File>Kt`, not the file name with `.kt`.
- `@JvmStatic` in `object`/`companion` → must produce the enclosing class as owner.
- Nullable `String?` → must map to `string | undefined`, not `string`.
- Kotlin `Char` → must be blocked with a reason, never crossed as a Rust `char`.
- `kotlinc` missing → `doctor` must report it; harness must fail with a clear message, not a panic.

---

### Task 1: Kotlin language plugin and detection

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/lang/mod.rs`
- Create: `src/lang/kotlin.rs`
- Modify: `src/lib.rs`
- Modify: `src/crates.toml`
- Test: `src/lang/kotlin.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces: `kotlin::discover/scan/resolve/exports/lookup/kotlin_to_ts/receiver`, `Lang::Kotlin`, `lang::lookup` arm.

- [ ] **Step 1: Add dependency** `tree-sitter-kotlin-ng = "1.1.0"` to `Cargo.toml`.

- [ ] **Step 2: Write failing tests in `src/lang/kotlin.rs`** for: `kotlin_to_ts("String?") == "string | undefined"`, `kotlin_to_ts("List<String>") == "string[]"`, `kotlin_to_ts("Char").is_err()`, top-level export owner `com.example.NativeLibKt`, `@JvmStatic` owner `com.example.Math`.

- [ ] **Step 3: Implement `src/lang/kotlin.rs`** with `language()` from `tree_sitter_kotlin_ng::LANGUAGE`, `discover/scan/resolve`, `exports` over function declarations and object declarations, `kotlin_to_ts`, `receiver` (delegate shape to `java::receiver`), `lookup` via `crates::find("kotlin", key)`.

- [ ] **Step 4: Wire `src/lang/mod.rs`**: enum variant, `name`, `registry`, `extensions`, `ALL`, replace `[0usize; 8]`/`(0..8)` with `ALL.len()`, detection by `.kt` vs `.java`, dispatch arms.

- [ ] **Step 5: Add `[[kotlin]]` crate mappings** to `src/crates.toml`; add `pub mod kotlin;` to `src/lib.rs`.

- [ ] **Step 6: Run** `cargo test --lib lang::kotlin` (expected PASS) and `cargo test --lib lang::` (no regressions).

- [ ] **Step 7: Commit** `feat: add Kotlin language plugin and detection`.

### Task 2: Wire Kotlin through the shared Java codegen

**Files:** `src/analyze.rs`, `src/codegen_ffi.rs`, `src/bridge.rs`, `src/shadow.rs`, `src/codegen_batch_native.rs`, `src/risks.rs`, `src/error_paths.rs`, `src/whole_app.rs`, `src/profile.rs`

**Interfaces:**
- Consumes: `Lang::Kotlin`, Kotlin exports with Java-shaped `prototype`.
- Produces: Kotlin units bridged with the same generated artifacts as Java.

- [ ] **Step 1: Add a test** in `tests/loop.rs` that bridges a Kotlin fixture unit and asserts generated `lib.rs` contains `Java_com_example_...` and a Java wrapper file exists.

- [ ] **Step 2: Add `Lang::Kotlin` to each Java match arm** in the files listed; do not change Java behavior.

- [ ] **Step 3: Add `src/codegen_kotlin.rs`** re-exporting the Java generators with Kotlin documentation and a `jni_symbol(owner, name)` helper for name mangling tests.

- [ ] **Step 4: Run** `cargo test --test loop` and `cargo clippy --all-targets -- -D warnings`.

- [ ] **Step 5: Commit** `feat: bridge Kotlin units through the Java JNI codegen`.

### Task 3: kotlinc harness for record/diff/bench

**Files:** Create `src/harness_kotlin.rs`; modify `src/harness.rs`, `src/lib.rs`, `src/doctor.rs`

**Interfaces:**
- Consumes: generated Java wrappers + reflection driver from Task 2.
- Produces: `harness_kotlin::record/diff/bench` matching `harness_java` signatures.

- [ ] **Step 1: Write a test** asserting `compile()` invokes `kotlinc -include-runtime` and uses `javac -cp <fat jar>`.

- [ ] **Step 2: Implement `harness_kotlin.rs`** by adapting `harness_java.rs`: compile Kotlin sources to a fat jar with `-include-runtime`, compile generated Java driver/wrappers against it, run the same `rustanything.RustAnythingDriver`.

- [ ] **Step 3: Wire `src/harness.rs`** dispatch (record/diff/bench) and `doctor.rs` toolchain entry for `kotlinc`.

- [ ] **Step 4: Run** `cargo test --lib harness_kotlin` and `RUST_ANYTHING_REQUIRE=kotlin cargo test` if kotlinc is installed.

- [ ] **Step 5: Commit** `feat: add kotlinc harness for Kotlin record and diff`.

### Task 4: Fixtures and integration tests

**Files:** `tests/fixtures/kotlin/**`, `tests/analyze.rs`, `tests/app.rs`, `tests/http.rs`, `tests/native.rs`

- [ ] **Step 1: Create Kotlin fixture** matching `tcli upper|len|div|words` and `/hello`, `/upper`, `/div/{a}/{b}` behavior of other language fixtures.
- [ ] **Step 2: Add tests** in `analyze.rs`, `app.rs`, `http.rs`, `native.rs` including a UTF-16/nullable regression case.
- [ ] **Step 3: Run** `cargo test` with kotlinc installed; `docs/STATUS.md` updated.
- [ ] **Step 4: Commit** `test: add Kotlin fixtures and integration coverage`.

### Task 5: Android/NDK profile (phase 2)

**Files:** `src/bridge.rs`, `src/codegen_ffi.rs`, `src/doctor.rs`, docs

- [x] **Step 1: Add `--profile android`** with CLI/MCP/docs wiring (this repository has no config file, so there was nothing to wire there).
- [x] **Step 2: Emit `cdylib` + NDK targets** (`aarch64-linux-android`, `armv7-linux-androideabi`, `x86_64-linux-android`) into `jniLibs/<abi>/`, and check every JNI symbol is exported.
- [x] **Step 3: `doctor` checks** Android NDK and the Rust Android targets. `cargo-ndk` is not checked because it is not used: the NDK's clang is the linker.
- [ ] **Step 4: Commit** `feat: add Android NDK build profile` (blocked: this clone has no git identity or credentials).

### Task 6: On-device recording (phase 3)

**Files:** `src/app.rs`, `src/harness_kotlin.rs`, docs

- [ ] **Step 1: `app record --profile android`** via adb instrumentation driver.
- [ ] **Step 2: Document limits**: SDK classes, UI/Compose stays Kotlin, ART JNI exceptions.
- [ ] **Step 3: Commit** `feat: add on-device Android recording profile`.
