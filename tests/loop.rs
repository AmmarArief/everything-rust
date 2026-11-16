//! The bridge -> record -> diff -> shadow loop on the fixture repo.
//!
//! A hand-written JS module stands in for the built Rust module, so these tests check the
//! harness itself without compiling a native crate. They need `node` on PATH.

use std::path::{Path, PathBuf};
use std::process::Command;

use rust_anything::bridge::{self, Kind, Manifest};
use rust_anything::harness::{self, DiffOptions, RecordOptions};
use rust_anything::shadow;

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
    let dir = std::env::temp_dir().join(format!("rust-anything-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    bridge::bridge(&bridge::Options {
        root: &fixture(),
        unit: "packages/a/src/util.ts",
        kind: Some(Kind::Napi),
        out: Some(dir.clone()),
        name: None,
        force: false,
        keep: Vec::new(),
        lang: None,
        batch: Vec::new(),
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
        lang: None,
        batch: Vec::new(),
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
        lang: None,
        batch: Vec::new(),
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
            .env("RUST_ANYTHING_MODE", mode)
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
    let response = rust_anything::mcp::handle(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": { "name": "analyze", "arguments": { "root": fixture() } },
    }))
    .unwrap();
    assert_eq!(response["result"]["isError"], false);
    assert_eq!(
        response["result"]["structuredContent"]["schema"],
        "rust-anything/analyze@1"
    );
}

#[test]
fn record_follows_bundler_style_imports() {
    if !node_available() {
        eprintln!("skipped: node not found");
        return;
    }
    // `fs.ts` imports `./util.js`, a file that only exists as `util.ts`.
    let dir = std::env::temp_dir().join(format!("rust-anything-resolve-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    bridge::bridge(&bridge::Options {
        root: &fixture(),
        unit: "packages/a/src/fs.ts",
        kind: Some(Kind::Napi),
        out: Some(dir.clone()),
        name: None,
        force: false,
        keep: Vec::new(),
        lang: None,
        batch: Vec::new(),
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
        let dir = std::env::temp_dir().join(format!(
            "rust-anything-async-{kind:?}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        bridge::bridge(&bridge::Options {
            root: &root,
            unit: "src/pipeline.ts",
            kind: Some(kind),
            out: Some(dir.clone()),
            name: None,
            force: false,
            keep: Vec::new(),
            lang: None,
            batch: Vec::new(),
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
            Kind::Pyo3 | Kind::CAbi | Kind::Ffi => {}
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
    let dir = std::env::temp_dir().join(format!("rust-anything-classes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let outcome = bridge::bridge(&bridge::Options {
        root: &root,
        unit: "src/stats.ts",
        kind: Some(Kind::Napi),
        out: Some(dir.clone()),
        name: None,
        force: false,
        keep: Vec::new(),
        lang: None,
        batch: Vec::new(),
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
    let report = rust_anything::analyze::analyze(&root).unwrap();
    let greet = report
        .units
        .iter()
        .find(|u| u.id == "src/greet.ts")
        .unwrap();
    assert_eq!(greet.internal_deps, ["src/lib/", "src/util/"]);
    if !node_available() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("rust-anything-alias-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    bridge::bridge(&bridge::Options {
        root: &root,
        unit: "src/greet.ts",
        kind: Some(Kind::Napi),
        out: Some(dir.clone()),
        name: None,
        force: false,
        keep: Vec::new(),
        lang: None,
        batch: Vec::new(),
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
    let dir = std::env::temp_dir().join(format!("rust-anything-monorepo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    bridge::bridge(&bridge::Options {
        root: &root,
        unit: "packages/b/src/titles.ts",
        kind: Some(Kind::Napi),
        out: Some(dir.clone()),
        name: None,
        force: false,
        keep: Vec::new(),
        lang: None,
        batch: Vec::new(),
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

fn python_available() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

#[test]
fn python_modules_bridge_record_and_diff() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/python");
    let dir = std::env::temp_dir().join(format!("rust-anything-python-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let outcome = bridge::bridge(&bridge::Options {
        root: &root,
        unit: "src/textkit/",
        kind: None,
        out: Some(dir.clone()),
        name: None,
        force: false,
        keep: Vec::new(),
        lang: None,
        batch: Vec::new(),
    })
    .unwrap();
    assert_eq!(outcome.kind, Kind::Pyo3);
    let manifest = Manifest::load(&dir).unwrap();
    let kept: Vec<&str> = manifest.js_only.iter().map(|j| j.name.as_str()).collect();
    assert_eq!(kept, ["walk", "apply", "configure"]);
    let port = std::fs::read_to_string(dir.join("src/port.rs")).unwrap();
    // A literal default stays a plain value; `None` as default is an Option.
    assert!(
        port.contains("pub fn slugify(text: String, sep: String, max_length: Option<i64>)"),
        "{port}"
    );
    let lib = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
    assert!(
        lib.contains(r#"signature = (text, sep=String::from("-"), *, max_length=None)"#),
        "{lib}"
    );
    assert!(lib.contains("#[pyclass(name = \"Stack\")]"));
    if !python_available() {
        return;
    }
    let recorded = harness::record(&RecordOptions {
        crate_dir: &dir,
        count: 60,
        inputs: None,
        module: None,
        resolve_from: None,
        node_args: &[],
    })
    .unwrap();
    assert!(recorded.cases > 200);
    // A pure-Python stand-in for the built package: `page_count` truncates instead of flooring.
    let package = dir.join("python/textkit_rs");
    std::fs::create_dir_all(&package).unwrap();
    let original = std::fs::read_to_string(root.join("src/textkit/words.py")).unwrap();
    let stack = std::fs::read_to_string(root.join("src/textkit/stack.py")).unwrap();
    let buggy = original.replace(
        "return -(-total // per_page)",
        "return int(-(-total / per_page))",
    );
    std::fs::write(package.join("__init__.py"), format!("{buggy}\n{stack}")).unwrap();
    let diff = |dir: &Path| {
        harness::diff(&DiffOptions {
            crate_dir: dir,
            module: None,
            ignore_key_order: false,
            node_args: &[],
        })
        .unwrap()
    };
    let buggy = diff(&dir);
    assert!(
        buggy.failed > 0 && buggy.mismatches.iter().all(|m| m["fn"] == "page_count"),
        "{:?}",
        buggy.mismatches
    );
    std::fs::write(package.join("__init__.py"), format!("{original}\n{stack}")).unwrap();
    let fixed = diff(&dir);
    assert_eq!(fixed.failed, 0, "{:?}", fixed.mismatches);
    std::fs::remove_dir_all(&dir).unwrap();
}

fn cc_available() -> bool {
    Command::new("cc")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

#[test]
fn c_units_bridge_record_and_diff() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/c");
    let dir = std::env::temp_dir().join(format!("rust-anything-c-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let outcome = bridge::bridge(&bridge::Options {
        root: &root,
        unit: "src/text.c",
        kind: None,
        out: Some(dir.clone()),
        name: None,
        force: false,
        keep: Vec::new(),
        lang: None,
        batch: Vec::new(),
    })
    .unwrap();
    assert_eq!(outcome.kind, Kind::CAbi);
    let manifest = Manifest::load(&dir).unwrap();
    // A returned pointer needs an ownership design: it stays in C.
    let kept: Vec<&str> = manifest.js_only.iter().map(|j| j.name.as_str()).collect();
    assert_eq!(kept, ["upper_copy"]);
    let lib = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
    assert!(
        lib.contains("pub extern \"C\" fn count_words(s: *const c_char) -> usize"),
        "{lib}"
    );
    let header = std::fs::read_to_string(dir.join("include/text_rs.h")).unwrap();
    assert!(
        header.contains("bool is_palindrome(const char *s);"),
        "{header}"
    );
    if !(cc_available() && python_available()) {
        return;
    }
    let recorded = harness::record(&RecordOptions {
        crate_dir: &dir,
        count: 40,
        inputs: None,
        module: None,
        resolve_from: None,
        node_args: &[],
    })
    .unwrap();
    assert_eq!(recorded.cases, 120);
    // `vowels` forgets upper case: the diff must notice.
    std::fs::write(
        dir.join("src/port.rs"),
        "pub fn count_words(s: Option<&[u8]>) -> usize {\n    s.map_or(0, |s| s.split(|b| b.is_ascii_whitespace() || *b == 0x0b).filter(|w| !w.is_empty()).count())\n}\n\
         pub fn vowels(s: Option<&[u8]>) -> std::ffi::c_int {\n    s.map_or(-1, |s| s.iter().filter(|b| b\"aeiou\".contains(b)).count() as std::ffi::c_int)\n}\n\
         pub fn is_palindrome(s: Option<&[u8]>) -> bool {\n    s.is_some_and(|s| s.iter().eq(s.iter().rev()))\n}\n",
    )
    .unwrap();
    harness::build(&dir).unwrap();
    let diff = harness::diff(&DiffOptions {
        crate_dir: &dir,
        module: None,
        ignore_key_order: false,
        node_args: &[],
    })
    .unwrap();
    assert!(diff.missing.is_empty());
    assert!(diff.failed > 0 && diff.failed < 40, "{diff:?}");
    assert!(
        diff.mismatches.iter().all(|m| m["fn"] == "vowels"),
        "{diff:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn go_available() -> bool {
    Command::new("go")
        .arg("version")
        .output()
        .is_ok_and(|o| o.status.success())
}

#[test]
fn go_packages_bridge_record_and_diff() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/go");
    let dir = std::env::temp_dir().join(format!("rust-anything-go-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let outcome = bridge::bridge(&bridge::Options {
        root: &root,
        unit: "strkit/",
        kind: None,
        out: Some(dir.clone()),
        name: None,
        force: false,
        keep: Vec::new(),
        lang: None,
        batch: Vec::new(),
    })
    .unwrap();
    assert_eq!(outcome.kind, Kind::Ffi);
    let manifest = Manifest::load(&dir).unwrap();
    let kept: Vec<&str> = manifest.js_only.iter().map(|j| j.name.as_str()).collect();
    assert_eq!(kept, ["Join"]);
    let port = std::fs::read_to_string(dir.join("src/port.rs")).unwrap();
    assert!(
        port.contains("pub fn parse_port(s: String) -> Result<i64, String>"),
        "{port}"
    );
    let wrapper = std::fs::read_to_string(dir.join("go/strkit.go")).unwrap();
    assert!(
        wrapper.contains("func ParsePort(s string) (int, error) {"),
        "{wrapper}"
    );
    assert!(wrapper.contains("type Point struct"), "{wrapper}");
    // Batches: a map result cannot be batched; the others get a handle type in the package.
    assert!(bridge::add_batches(&dir, &["WordFreq:text".into()]).is_err());
    bridge::add_batches(&dir, &["Slugify:s".into(), "ParsePort:s".into()]).unwrap();
    let wrapper = std::fs::read_to_string(dir.join("go/strkit.go")).unwrap();
    assert!(
        wrapper.contains("func NewSlugifyBatch(items []string) *SlugifyBatch {"),
        "{wrapper}"
    );
    assert!(
        wrapper.contains("func (b *SlugifyBatch) Run(max int) []string {"),
        "{wrapper}"
    );
    assert!(
        wrapper.contains("func (b *ParsePortBatch) Rank(min float64) ([]int, error) {"),
        "{wrapper}"
    );
    let lib = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
    assert!(lib.contains("static GLOBAL: mimalloc::MiMalloc"), "{lib}");
    if !go_available() {
        return;
    }
    let recorded = harness::record(&RecordOptions {
        crate_dir: &dir,
        count: 30,
        inputs: None,
        module: None,
        resolve_from: None,
        node_args: &[],
    })
    .unwrap();
    assert!(recorded.cases > 100, "{recorded:?}");
    assert!(recorded.errors > 0, "ParsePort returns errors");
    // A port where every function fails: the diff must run through the Go wrapper and say so.
    harness::build(&dir).unwrap();
    let diff = harness::diff(&DiffOptions {
        crate_dir: &dir,
        module: None,
        ignore_key_order: false,
        node_args: &[],
    })
    .unwrap();
    assert_eq!(diff.passed, 0, "{diff:?}");
    assert_eq!(diff.total, recorded.cases);
    let _ = std::fs::remove_dir_all(&dir);
}

fn ruby_available() -> bool {
    Command::new("ruby")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

#[test]
fn ruby_modules_bridge_record_and_diff() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ruby");
    let dir = std::env::temp_dir().join(format!("rust-anything-ruby-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let outcome = bridge::bridge(&bridge::Options {
        root: &root,
        unit: "lib/textkit/",
        kind: None,
        out: Some(dir.clone()),
        name: None,
        force: false,
        keep: Vec::new(),
        lang: None,
        batch: Vec::new(),
    })
    .unwrap();
    assert_eq!(outcome.kind, Kind::Ffi);
    let manifest = Manifest::load(&dir).unwrap();
    let kept: Vec<&str> = manifest.js_only.iter().map(|j| j.name.as_str()).collect();
    assert_eq!(kept, ["each_word"]);
    let port = std::fs::read_to_string(dir.join("src/port.rs")).unwrap();
    // YARD types, a literal default kept as a plain value, and `nil` as an Option.
    assert!(
        port.contains("pub fn slugify(text: String, sep: String, max: Option<i64>)"),
        "{port}"
    );
    let wrapper = std::fs::read_to_string(dir.join("ruby/textkit_rs.rb")).unwrap();
    assert!(
        wrapper.contains("def self.slugify(text, sep = \"-\", max: nil)"),
        "{wrapper}"
    );
    // Batches keep the original's other parameters, defaults and keywords.
    bridge::add_batches(&dir, &["slugify:text".into(), "page_count:total".into()]).unwrap();
    let wrapper = std::fs::read_to_string(dir.join("ruby/textkit_rs.rb")).unwrap();
    assert!(wrapper.contains("class SlugifyBatch"), "{wrapper}");
    assert!(
        wrapper.contains("def run(sep = \"-\", max: nil)"),
        "{wrapper}"
    );
    assert!(
        wrapper.contains("def rank(per_page = 10, min: 0)"),
        "{wrapper}"
    );
    if !ruby_available() {
        return;
    }
    let recorded = harness::record(&RecordOptions {
        crate_dir: &dir,
        count: 30,
        inputs: None,
        module: None,
        resolve_from: None,
        node_args: &[],
    })
    .unwrap();
    assert!(recorded.cases > 100, "{recorded:?}");
    // `page_count` raises ArgumentError for a non-positive page size.
    assert!(recorded.errors > 0);
    harness::build(&dir).unwrap();
    let diff = harness::diff(&DiffOptions {
        crate_dir: &dir,
        module: None,
        ignore_key_order: false,
        node_args: &[],
    })
    .unwrap();
    // Unported functions panic, which Ruby sees as a RuntimeError: nothing can match.
    assert_eq!(diff.passed, 0, "{diff:?}");
    assert_eq!(diff.total, recorded.cases);
    let _ = std::fs::remove_dir_all(&dir);
}

fn java_available() -> bool {
    Command::new("javac")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

#[test]
fn java_classes_bridge_record_and_diff() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/java");
    let dir = std::env::temp_dir().join(format!("rust-anything-java-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let outcome = bridge::bridge(&bridge::Options {
        root: &root,
        unit: "src/main/java/com/example/text/",
        kind: None,
        out: Some(dir.clone()),
        name: Some("text-rs".into()),
        force: false,
        keep: Vec::new(),
        lang: None,
        batch: Vec::new(),
    })
    .unwrap();
    assert_eq!(outcome.kind, Kind::Ffi);
    let port = std::fs::read_to_string(dir.join("src/port.rs")).unwrap();
    assert!(
        port.contains("pub fn parse_port(text: String) -> Result<i64, String>"),
        "{port}"
    );
    let lib = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
    assert!(
        lib.contains("fn Java_rustanything_TextRsNative_call"),
        "{lib}"
    );
    let wrapper = std::fs::read_to_string(dir.join("java/com/example/text/Strings.java")).unwrap();
    assert!(
        wrapper.contains("public static List<String> words(String text)"),
        "{wrapper}"
    );
    // Batches: a list result cannot be batched; the others get a class beside the original.
    assert!(bridge::add_batches(&dir, &["words:text".into()]).is_err());
    bridge::add_batches(&dir, &["slugify:text".into(), "pages:total".into()]).unwrap();
    let batch =
        std::fs::read_to_string(dir.join("java/com/example/text/SlugifyBatch.java")).unwrap();
    assert!(
        batch.contains("public final class SlugifyBatch implements AutoCloseable"),
        "{batch}"
    );
    assert!(batch.contains("public String[] run(int max)"), "{batch}");
    let pages = std::fs::read_to_string(dir.join("java/com/example/text/PagesBatch.java")).unwrap();
    assert!(
        pages.contains("public int[] rank(int perPage, double min)"),
        "{pages}"
    );
    let lib = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
    assert!(
        lib.contains("fn Java_com_example_text_SlugifyBatch_nRun"),
        "{lib}"
    );
    if !java_available() {
        return;
    }
    let recorded = harness::record(&RecordOptions {
        crate_dir: &dir,
        count: 30,
        inputs: None,
        module: None,
        resolve_from: None,
        node_args: &[],
    })
    .unwrap();
    assert!(recorded.cases > 100, "{recorded:?}");
    // `parsePort` throws NumberFormatException for text.
    assert!(recorded.errors > 0);
    harness::build(&dir).unwrap();
    let diff = harness::diff(&DiffOptions {
        crate_dir: &dir,
        module: None,
        ignore_key_order: false,
        node_args: &[],
    })
    .unwrap();
    // Unported functions panic, which Java sees as a RuntimeException: nothing can match.
    assert_eq!(diff.passed, 0, "{diff:?}");
    assert_eq!(diff.total, recorded.cases);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn batches_keep_items_in_rust() {
    let dir = bridged("batch");
    std::fs::write(
        dir.join("src/port.rs"),
        "pub fn range_of(text: String, word: String) -> Result<serde_json::Value, String> { let _ = (text, word); Ok(serde_json::Value::Null) }\n\
         pub fn words(text: String) -> Vec<String> { vec![text] }\n\
         pub fn count(text: String) -> f64 { text.matches(' ').count() as f64 }\n",
    )
    .unwrap();
    // Structured results cannot be batched, and a refused batch leaves the crate as it was.
    let refused = bridge::add_batches(&dir, &["rangeOf:text".into()]).unwrap_err();
    assert!(
        format!("{refused:#}").contains("numbers, booleans or strings"),
        "{refused:#}"
    );
    assert!(Manifest::load(&dir).unwrap().batches.is_empty());
    assert!(bridge::add_batches(&dir, &["count:nope".into()]).is_err());

    bridge::add_batches(&dir, &["count:text".into()]).unwrap();
    let lib = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
    assert!(lib.contains("#[napi(js_name = \"CountBatch\")]"), "{lib}");
    assert!(lib.contains("pub struct CountDefaultIndex"));
    assert!(lib.contains("par_iter()"), "napi batches run on all cores");
    assert!(
        lib.contains("Float64Array"),
        "number results cross as one typed array"
    );
    assert!(lib.contains("pub fn rank("));
    let cargo = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    assert!(cargo.contains("rayon"));

    // An index in port.rs replaces the default one.
    let mut port = std::fs::read_to_string(dir.join("src/port.rs")).unwrap();
    port.push_str(
        "pub struct CountIndex(Vec<String>);\n\
         impl CountIndex {\n    pub fn new(items: Vec<String>) -> Self { Self(items) }\n    \
         pub fn run(&self) -> Result<Vec<f64>, String> { Ok(self.0.iter().map(|t| count(t.clone())).collect()) }\n}\n",
    );
    std::fs::write(dir.join("src/port.rs"), port).unwrap();
    bridge::refresh(&dir).unwrap();
    let lib = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
    assert!(lib.contains("inner: port::CountIndex,"));
    assert!(!lib.contains("CountDefaultIndex"));
    assert!(
        lib.contains("self.inner.run()"),
        "no `rank` of its own yet: {lib}"
    );
    // An index with its own `rank` (say, narrowing as the user types) is called for `rank`.
    let mut port = std::fs::read_to_string(dir.join("src/port.rs")).unwrap();
    port = port.replace(
        "    pub fn run(&self)",
        "    pub fn rank(&self, min: f64) -> Result<Vec<u32>, String> {\n        let _ = min;\n        Ok(Vec::new())\n    }\n    pub fn run(&self)",
    );
    std::fs::write(dir.join("src/port.rs"), port).unwrap();
    bridge::refresh(&dir).unwrap();
    let lib = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
    assert!(lib.contains("self.inner.rank(min)"), "{lib}");
    std::fs::remove_dir_all(&dir).unwrap();
}
