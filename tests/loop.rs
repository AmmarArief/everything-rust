//! The bridge -> record -> diff -> shadow loop on the fixture repo.
//!
//! A hand-written JS module stands in for the built Rust module, so these tests check the
//! harness itself without compiling a native crate. They need `node` on PATH.

use std::path::{Path, PathBuf};
use std::process::Command;

use carcinize::bridge::{self, Kind, Manifest};
use carcinize::harness::{self, DiffOptions, RecordOptions};
use carcinize::shadow;

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini")
}

/// A fresh crate for `util.ts` in a temp folder.
fn bridged(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("carcinize-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    bridge::bridge(&bridge::Options {
        root: &fixture(),
        unit: "packages/a/src/util.ts",
        kind: Some(Kind::Napi),
        out: Some(dir.clone()),
        name: None,
        force: false,
        keep: Vec::new(),
    })
    .unwrap();
    dir
}

/// JS stand-in for the Rust module; `buggy` miscounts tabs as spaces.
fn stand_in(dir: &Path, buggy: bool) {
    let space = if buggy { "/[ \\t]/" } else { "/ /" };
    std::fs::write(
        dir.join("index.cjs"),
        format!(
            "const after = /(?<=\\$)\\w+/g\n\
             exports.rangeOf = (text, word) => {{ const start = text.indexOf(word); return {{ start, end: start + word.length, rest: text.slice(start) }} }}\n\
             exports.words = text => [...text.matchAll(after)].map(m => m[0])\n\
             exports.count = text => [...text].filter(c => {space}.test(c)).length\n"
        ),
    )
    .unwrap();
}

#[test]
fn bridge_writes_a_crate_and_never_overwrites_it() {
    let dir = bridged("bridge");
    let manifest = Manifest::load(&dir).unwrap();
    let names: Vec<&str> = manifest.exports.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["rangeOf", "words", "count"]);
    let lib = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
    // Structured results cross as JSON text; string lists cross natively.
    assert!(lib.contains("pub fn range_of(text: String, word: String) -> napi::Result<String>"));
    assert!(lib.contains("pub fn words(text: String) -> napi::Result<Vec<String>>"));
    let port = std::fs::read_to_string(dir.join("src/port.rs")).unwrap();
    assert!(port.contains(
        "pub fn range_of(text: String, word: String) -> Result<serde_json::Value, String>"
    ));
    assert!(
        dir.join("src/utf16.rs").exists(),
        "util.ts uses string offsets"
    );
    let again = bridge::bridge(&bridge::Options {
        root: &fixture(),
        unit: "packages/a/src/util.ts",
        kind: None,
        out: Some(dir.clone()),
        name: None,
        force: false,
        keep: Vec::new(),
    });
    assert!(again.unwrap_err().to_string().contains("never overwrites"));
    let blocked = bridge::bridge(&bridge::Options {
        root: &fixture(),
        unit: "packages/a/src/svelte.ts",
        kind: None,
        out: Some(dir.join("nope")),
        name: None,
        force: false,
        keep: Vec::new(),
    });
    assert!(blocked.unwrap_err().to_string().contains("Blocked"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn record_and_diff_catch_a_behavior_change() {
    if !node_available() {
        eprintln!("skipped: node not found");
        return;
    }
    let dir = bridged("diff");
    let recorded = harness::record(&RecordOptions {
        crate_dir: &dir,
        count: 120,
        inputs: None,
        module: None,
        resolve_from: None,
        node_args: &[],
    })
    .unwrap();
    assert_eq!(recorded.cases, 360);

    let diff = |dir: &Path| {
        harness::diff(&DiffOptions {
            crate_dir: dir,
            module: None,
            ignore_key_order: false,
            node_args: &[],
        })
        .unwrap()
    };
    stand_in(&dir, true);
    let buggy = diff(&dir);
    assert!(buggy.failed > 0, "the tab bug must show up");
    assert!(buggy.mismatches.iter().all(|m| m["fn"] == "count"));

    stand_in(&dir, false);
    let fixed = diff(&dir);
    assert_eq!((fixed.passed, fixed.failed), (360, 0));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn shadow_returns_the_original_and_reports_mismatches() {
    if !node_available() {
        eprintln!("skipped: node not found");
        return;
    }
    let dir = bridged("shadow");
    stand_in(&dir, true);
    let out = shadow::shadow(&dir, None).unwrap();
    let script = dir.join("use.ts");
    std::fs::write(
        &script,
        format!(
            "import {{ count, shadowStats, onMismatch }} from '{}'\n\
             const seen: string[] = []\n\
             onMismatch(m => seen.push(m.fn))\n\
             console.log(JSON.stringify([count('a b'), count('a\\tb'), shadowStats.calls, seen]))\n",
            out.path.display()
        ),
    )
    .unwrap();
    let run = |mode: &str| {
        let o = Command::new("node")
            .arg(&script)
            .env("CARCINIZE_MODE", mode)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };
    // Shadow: callers get the original's answers; the tab case is reported.
    assert_eq!(run("shadow"), r#"[1,0,2,["count"]]"#);
    assert_eq!(run("old"), r#"[1,0,0,[]]"#);
    // New: the stand-in answers, bug included.
    assert_eq!(run("new"), r#"[1,1,0,[]]"#);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn mcp_runs_analyze() {
    let response = carcinize::mcp::handle(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "analyze", "arguments": { "root": fixture() } },
    }))
    .unwrap();
    assert_eq!(response["result"]["isError"], false);
    assert_eq!(
        response["result"]["structuredContent"]["schema"],
        "carcinize/analyze@1"
    );
}

#[test]
fn record_follows_bundler_style_imports() {
    if !node_available() {
        eprintln!("skipped: node not found");
        return;
    }
    // `fs.ts` imports `./util.js`, a file that only exists as `util.ts`.
    let dir = std::env::temp_dir().join(format!("carcinize-resolve-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    bridge::bridge(&bridge::Options {
        root: &fixture(),
        unit: "packages/a/src/fs.ts",
        kind: Some(Kind::Napi),
        out: Some(dir.clone()),
        name: None,
        force: false,
        keep: Vec::new(),
    })
    .unwrap();
    let recorded = harness::record(&RecordOptions {
        crate_dir: &dir,
        count: 5,
        inputs: None,
        module: None,
        resolve_from: None,
        node_args: &[],
    })
    .unwrap();
    // The paths do not exist: every call throws, but the module loaded and ran.
    assert_eq!((recorded.cases, recorded.errors), (5, 5));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn async_functions_get_async_bindings() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/async");
    for kind in [Kind::Napi, Kind::Wasm] {
        let dir =
            std::env::temp_dir().join(format!("carcinize-async-{kind:?}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        bridge::bridge(&bridge::Options {
            root: &root,
            unit: "src/pipeline.ts",
            kind: Some(kind),
            out: Some(dir.clone()),
            name: None,
            force: false,
            keep: Vec::new(),
        })
        .unwrap();
        let port = std::fs::read_to_string(dir.join("src/port.rs")).unwrap();
        assert!(port.contains("pub async fn count_words(text: String) -> Result<f64, String>"));
        assert!(port.contains("pub fn upper(text: String) -> Result<String, String>"));
        let lib = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
        assert!(
            lib.contains("pub async fn count_words(text: String)"),
            "{lib}"
        );
        let cargo = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
        match kind {
            Kind::Napi => {
                assert!(cargo.contains("features = [\"async\"]") && cargo.contains("futures"))
            }
            Kind::Wasm => assert!(cargo.contains("wasm-bindgen-futures")),
        }
        let index = std::fs::read_to_string(dir.join("index.cjs")).unwrap();
        // Only structured functions are wrapped; `countWords` passes its promise straight through.
        assert!(
            index.contains("normalize: [[1, 0], 'json', true]"),
            "{index}"
        );
        assert!(!index.contains("countWords:"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[test]
fn classes_are_bridged_recorded_and_replayed() {
    if !node_available() {
        eprintln!("skipped: node not found");
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/classes");
    let dir = std::env::temp_dir().join(format!("carcinize-classes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let outcome = bridge::bridge(&bridge::Options {
        root: &root,
        unit: "src/stats.ts",
        kind: Some(Kind::Napi),
        out: Some(dir.clone()),
        name: None,
        force: false,
        keep: Vec::new(),
    })
    .unwrap();
    assert_eq!(outcome.exports, ["class TextStats", "class LruCache"]);
    let manifest = Manifest::load(&dir).unwrap();
    assert!(
        manifest
            .js_only
            .iter()
            .any(|j| j.name == "Blocked" && j.reason.contains("extends"))
    );
    let port = std::fs::read_to_string(dir.join("src/port.rs")).unwrap();
    assert!(port.contains("pub fn new(capacity: f64) -> Result<Self, String>"));
    assert!(port.contains("pub fn get(&mut self, key: String) -> Result<Option<String>, String>"));
    let lib = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
    assert!(lib.contains("#[napi(constructor)]") && lib.contains("inner: port::LruCache"));

    // Record scenarios (the TS uses parameter properties: Node needs to transform it).
    let recorded = harness::record(&RecordOptions {
        crate_dir: &dir,
        count: 150,
        inputs: None,
        module: None,
        resolve_from: None,
        node_args: &[],
    })
    .unwrap();
    assert_eq!(recorded.cases, 300);
    // A JS stand-in with an LRU bug: `get` does not refresh recency.
    std::fs::write(
        dir.join("index.cjs"),
        "class TextStats { constructor(s = '\\n') { this.s = s; this.w = 0; this.l = 0; this.long = '' }\n\
         add(t) { const p = t.split(this.s); this.l += p.length; for (const x of p) for (const w of x.split(/\\s+/).filter(Boolean)) { this.w++; if (w.length > this.long.length) this.long = w } return this.w }\n\
         stats() { return { words: this.w, lines: this.l } } longestWord() { return this.long } reset() { this.w = 0; this.l = 0; this.long = '' } }\n\
         class LruCache { constructor(c) { if (!(c >= 1)) throw new RangeError('x'); this.c = c; this.m = new Map() }\n\
         get(k) { return this.m.get(k) }\n\
         set(k, v) { this.m.delete(k); this.m.set(k, v); const e = []; while (this.m.size > this.c) { const o = this.m.keys().next().value; this.m.delete(o); e.push(o) } return e }\n\
         keys() { return [...this.m.keys()] } }\n\
         module.exports = { TextStats, LruCache }\n",
    )
    .unwrap();
    let diff = harness::diff(&DiffOptions {
        crate_dir: &dir,
        module: None,
        ignore_key_order: false,
        node_args: &[],
    })
    .unwrap();
    assert!(diff.failed > 0, "the recency bug must show up");
    assert!(diff.mismatches.iter().all(|m| m["fn"] == "LruCache"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn tsconfig_aliases_are_followed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/aliases");
    let report = carcinize::analyze::analyze(&root).unwrap();
    let greet = report
        .units
        .iter()
        .find(|u| u.id == "src/greet.ts")
        .unwrap();
    assert_eq!(greet.internal_deps, ["src/lib/", "src/util/"]);
    if !node_available() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("carcinize-alias-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    bridge::bridge(&bridge::Options {
        root: &root,
        unit: "src/greet.ts",
        kind: Some(Kind::Napi),
        out: Some(dir.clone()),
        name: None,
        force: false,
        keep: Vec::new(),
    })
    .unwrap();
    let recorded = harness::record(&RecordOptions {
        crate_dir: &dir,
        count: 5,
        inputs: None,
        module: None,
        resolve_from: None,
        node_args: &[],
    })
    .unwrap();
    assert_eq!((recorded.cases, recorded.errors), (5, 0));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn record_resolves_workspace_packages_to_source() {
    if !node_available() {
        return;
    }
    // `@m/b` imports `@m/a` by name; `@m/a` has no build output (`dist/` does not exist).
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/monorepo");
    let dir = std::env::temp_dir().join(format!("carcinize-monorepo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    bridge::bridge(&bridge::Options {
        root: &root,
        unit: "packages/b/src/titles.ts",
        kind: Some(Kind::Napi),
        out: Some(dir.clone()),
        name: None,
        force: false,
        keep: Vec::new(),
    })
    .unwrap();
    let recorded = harness::record(&RecordOptions {
        crate_dir: &dir,
        count: 10,
        inputs: None,
        module: None,
        resolve_from: None,
        node_args: &[],
    })
    .unwrap();
    assert_eq!((recorded.cases, recorded.errors), (10, 0));
    std::fs::remove_dir_all(&dir).unwrap();
}
