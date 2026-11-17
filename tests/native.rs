//! Bridges built for real: compile the generated crate, load it from the original language and
//! replay the recording. Run on every CI platform, since linking differs between them.

use std::path::{Path, PathBuf};
use std::process::Command;

use rust_anything::bridge::{self, Kind};
use rust_anything::harness::{self, DiffOptions, RecordOptions};

fn available(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "rust-anything-native-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    root
}

/// Bridge `unit`, record the original, port `name` with `body`, build, and replay.
fn round_trip(root: &Path, unit: &str, kind: Kind, name: &str, body: &str) {
    let dir = root.join("port");
    bridge::bridge(&bridge::Options {
        root,
        unit,
        kind: Some(kind),
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
        count: 30,
        inputs: None,
        module: None,
        resolve_from: None,
        node_args: &[],
    })
    .unwrap();
    assert!(recorded.cases > 0, "{recorded:?}");

    let port_path = dir.join("src/port.rs");
    let port = std::fs::read_to_string(&port_path).unwrap();
    let start = port
        .find(&format!("pub fn {name}("))
        .unwrap_or_else(|| panic!("no {name} in\n{port}"));
    let open = start + port[start..].find('{').unwrap();
    let close = open + port[open..].find("\n}").unwrap();
    let ported = format!("{}{{\n    {body}{}", &port[..open], &port[close..]);
    std::fs::write(&port_path, ported).unwrap();

    harness::build(&dir).unwrap();
    let outcome = harness::diff(&DiffOptions {
        crate_dir: &dir,
        module: None,
        ignore_key_order: false,
        node_args: &[],
        float_tolerance: None,
    })
    .unwrap();
    assert_eq!(
        (outcome.failed, outcome.passed),
        (0, outcome.total),
        "{:#?}",
        outcome.mismatches
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn napi_addon_builds_loads_and_matches() {
    if !available("node") {
        eprintln!("skipped: node not found");
        return;
    }
    let root = project(
        "napi",
        &[
            ("package.json", r#"{"name":"shout","version":"0.1.0"}"#),
            (
                "src/shout.ts",
                "export function shout(text: string): string {\n  return text + \"!\";\n}\n",
            ),
        ],
    );
    round_trip(
        &root,
        "src/shout.ts",
        Kind::Napi,
        "shout",
        "Ok(format!(\"{text}!\"))",
    );
}

#[test]
fn pyo3_extension_builds_imports_and_matches() {
    if !available("python3") {
        eprintln!("skipped: python3 not found");
        return;
    }
    let root = project(
        "pyo3",
        &[
            (
                "pyproject.toml",
                "[project]\nname = \"shout\"\nversion = \"0.1.0\"\n",
            ),
            (
                "shout/__init__.py",
                "def shout(text: str) -> str:\n    return text + \"!\"\n",
            ),
        ],
    );
    round_trip(
        &root,
        "shout/",
        Kind::Pyo3,
        "shout",
        "Ok(format!(\"{text}!\"))",
    );
}
