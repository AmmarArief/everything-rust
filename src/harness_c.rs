//! The C side of record, build, diff and bench: the original is compiled into a shared
//! library, and Python's ctypes calls it and the Rust library with the same signatures.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::bridge::Manifest;
use crate::harness::{BenchRow, CASES, DiffOutcome};
use crate::lang::c::{defines_main, scalar};

const WORK_DIR: &str = ".carcinize";

fn run(cmd: &mut Command, what: &str) -> Result<()> {
    let output = cmd.output().with_context(|| format!("running {what}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = stderr.lines().rev().take(25).collect();
        bail!(
            "{what} failed ({}):\n{}",
            output.status,
            tail.into_iter().rev().collect::<Vec<_>>().join("\n")
        );
    }
    Ok(())
}

fn compiler() -> &'static str {
    if Command::new("cc").arg("--version").output().is_ok() {
        "cc"
    } else {
        "gcc"
    }
}

/// ctypes signatures: `{ name: { "args": [...], "ret": ... } }`.
fn signatures(manifest: &Manifest) -> Value {
    let ctypes = |c: &str, ts: &str| -> String {
        if ts == "string" {
            "c_char_p".into()
        } else {
            scalar(c).map_or("c_int", |(_, _, _, ct)| ct).to_string()
        }
    };
    let map: serde_json::Map<String, Value> = manifest
        .exports
        .iter()
        .map(|f| {
            let args: Vec<String> = f
                .params
                .iter()
                .map(|p| ctypes(p.native_type.as_deref().unwrap_or("int"), &p.ts_type))
                .collect();
            let ret = ctypes(f.native_return.as_deref().unwrap_or("void"), &f.returns);
            (f.name.clone(), json!({ "args": args, "ret": ret }))
        })
        .collect();
    Value::Object(map)
}

/// Compiles the project's C sources (without `main` and tests) into one shared library.
fn build_original(crate_dir: &Path, manifest: &Manifest) -> Result<PathBuf> {
    let ws = crate::lang::c::discover(&manifest.root)?;
    let mut sources = Vec::new();
    let mut include_dirs = std::collections::BTreeSet::new();
    include_dirs.insert(manifest.root.clone());
    for file in ws
        .files
        .iter()
        .filter(|f| f.kind == crate::ts::discover::FileKind::Source)
    {
        let path = manifest.root.join(&file.path);
        if let Some(dir) = path.parent() {
            include_dirs.insert(dir.to_path_buf());
        }
        if file.path.ends_with(".c")
            && !defines_main(&std::fs::read_to_string(&path).unwrap_or_default())
        {
            sources.push(path);
        }
    }
    // Headers paired with a `.c` file are not in the workspace listing; their folders still
    // go on the include path.
    for entry in ignore::WalkBuilder::new(&manifest.root)
        .require_git(false)
        .build()
        .flatten()
    {
        if entry.path().extension().is_some_and(|e| e == "h")
            && let Some(dir) = entry.path().parent()
        {
            include_dirs.insert(dir.to_path_buf());
        }
    }
    let out = crate_dir.join(WORK_DIR).join("original.so");
    std::fs::create_dir_all(crate_dir.join(WORK_DIR))?;
    let mut cmd = Command::new(compiler());
    cmd.args(["-shared", "-fPIC", "-O1", "-w", "-o"]).arg(&out);
    for dir in &include_dirs {
        cmd.arg(format!("-I{}", dir.display()));
    }
    cmd.args(&sources).arg("-lm");
    run(&mut cmd, "cc (original sources)")?;
    Ok(out)
}

fn driver(crate_dir: &Path, manifest: &Manifest) -> Result<(PathBuf, PathBuf)> {
    let work = crate_dir.join(WORK_DIR);
    std::fs::create_dir_all(&work)?;
    let path = work.join("driver_c.py");
    std::fs::write(&path, DRIVER_C_PY)?;
    let sigs = work.join("signatures.json");
    std::fs::write(&sigs, serde_json::to_string(&signatures(manifest))?)?;
    Ok((path, sigs))
}

fn python() -> Command {
    let mut cmd = Command::new("python3");
    cmd.env("PYTHONDONTWRITEBYTECODE", "1");
    cmd
}

pub fn record(crate_dir: &Path, manifest: &Manifest, cases: &[Value]) -> Result<PathBuf> {
    let original = build_original(crate_dir, manifest)?;
    let (driver, sigs) = driver(crate_dir, manifest)?;
    let inputs = crate_dir.join(WORK_DIR).join("inputs.json");
    std::fs::write(&inputs, serde_json::to_string(cases)?)?;
    let out = crate_dir.join(CASES);
    run(
        python()
            .arg(&driver)
            .arg("record")
            .arg(&original)
            .arg(&sigs)
            .arg(&inputs)
            .arg(&out),
        "python (ctypes)",
    )?;
    Ok(out)
}

/// The Rust library: `lib<name>.so`, `.dylib` or `.dll`.
pub fn build(crate_dir: &Path, manifest: &Manifest) -> Result<PathBuf> {
    let target = crate_dir.join("target");
    run(
        Command::new("cargo")
            .args(["build", "--release", "--manifest-path"])
            .arg(crate_dir.join("Cargo.toml"))
            .env("CARGO_TARGET_DIR", &target),
        "cargo build",
    )?;
    let lib = manifest.lib_name();
    let release = target.join("release");
    [
        format!("lib{lib}.so"),
        format!("lib{lib}.dylib"),
        format!("{lib}.dll"),
    ]
    .iter()
    .map(|n| release.join(n))
    .find(|p| p.exists())
    .with_context(|| format!("no shared library for `{lib}` in {}", release.display()))
}

fn built(crate_dir: &Path, manifest: &Manifest) -> Result<PathBuf> {
    let lib = manifest.lib_name();
    let release = crate_dir.join("target/release");
    [
        format!("lib{lib}.so"),
        format!("lib{lib}.dylib"),
        format!("{lib}.dll"),
    ]
    .iter()
    .map(|n| release.join(n))
    .find(|p| p.exists())
    .with_context(|| {
        format!(
            "no built library in {} (run `carcinize build` first)",
            release.display()
        )
    })
}

pub fn diff(crate_dir: &Path, manifest: &Manifest) -> Result<DiffOutcome> {
    let cases = crate_dir.join(CASES);
    if !cases.exists() {
        bail!(
            "no {CASES} in {} (run `carcinize record` first)",
            crate_dir.display()
        );
    }
    let rust = built(crate_dir, manifest)?;
    let (driver, sigs) = driver(crate_dir, manifest)?;
    let out = crate_dir.join(WORK_DIR).join("diff.json");
    run(
        python()
            .arg(&driver)
            .arg("diff")
            .arg(&rust)
            .arg(&sigs)
            .arg(&cases)
            .arg(&out),
        "python (ctypes)",
    )?;
    Ok(serde_json::from_str(&std::fs::read_to_string(&out)?)?)
}

pub fn bench(crate_dir: &Path, manifest: &Manifest, min_ms: u64) -> Result<Vec<BenchRow>> {
    let cases = crate_dir.join(CASES);
    if !cases.exists() {
        bail!(
            "no {CASES} in {} (run `carcinize record` first)",
            crate_dir.display()
        );
    }
    let original = build_original(crate_dir, manifest)?;
    let rust = built(crate_dir, manifest)?;
    let (driver, sigs) = driver(crate_dir, manifest)?;
    let out = crate_dir.join(WORK_DIR).join("bench.json");
    run(
        python()
            .arg(&driver)
            .arg("bench")
            .arg(&original)
            .arg(&rust)
            .arg(&sigs)
            .arg(&cases)
            .arg(&out)
            .arg(min_ms.to_string()),
        "python (ctypes)",
    )?;
    Ok(serde_json::from_str(&std::fs::read_to_string(&out)?)?)
}

const DRIVER_C_PY: &str = r#"
import ctypes
import json
import math
import sys
import time


def enc(v):
    if isinstance(v, float):
        if math.isnan(v):
            return {"$num": "NaN"}
        if math.isinf(v):
            return {"$num": "Infinity" if v > 0 else "-Infinity"}
        if v == 0 and math.copysign(1, v) < 0:
            return {"$num": "-0"}
    if isinstance(v, bytes):
        return {"$bytes": list(v)}
    return v


def dec(v):
    if isinstance(v, dict) and len(v) == 1:
        (key, inner), = v.items()
        if key == "$undefined":
            return None
        if key == "$num":
            return {"NaN": math.nan, "Infinity": math.inf, "-Infinity": -math.inf, "-0": -0.0}[inner]
    return v


def bind(path, signatures):
    lib = ctypes.CDLL(path)
    functions = {}
    for name, sig in signatures.items():
        try:
            fn = getattr(lib, name)
        except AttributeError:
            continue
        fn.argtypes = [getattr(ctypes, t) for t in sig["args"]]
        fn.restype = None if sig["ret"] == "None" else getattr(ctypes, sig["ret"])
        functions[name] = (fn, sig)
    return functions


def call(functions, c):
    fn, sig = functions[c["fn"]]
    args = []
    for value, kind in zip([dec(a) for a in c["args"]], sig["args"]):
        if kind == "c_char_p":
            args.append(None if value is None else str(value).encode("utf-8"))
        elif kind in ("c_float", "c_double"):
            args.append(float(value) if isinstance(value, (int, float)) else 0.0)
        elif kind == "c_bool":
            args.append(bool(value))
        else:
            args.append(int(value) if isinstance(value, (int, float)) and math.isfinite(value) else 0)
    while len(args) < len(sig["args"]):
        args.append(None if sig["args"][len(args)] == "c_char_p" else 0)
    return {"ok": enc(fn(*args))}


def cases_of(path):
    return [json.loads(line) for line in open(path, encoding="utf-8") if line.strip()]


def record(lib, sigs, inputs, out_path):
    functions = bind(lib, json.load(open(sigs)))
    with open(out_path, "w", encoding="utf-8") as out:
        for c in json.load(open(inputs, encoding="utf-8")):
            line = {"fn": c["fn"], "args": c["args"]}
            line.update(call(functions, c))
            out.write(json.dumps(line, ensure_ascii=False) + "\n")


def diff(lib, sigs, cases_path, out_path):
    functions = bind(lib, json.load(open(sigs)))
    summary = {"total": 0, "passed": 0, "failed": 0, "missing": [], "mismatches": []}
    for c in cases_of(cases_path):
        summary["total"] += 1
        if c["fn"] not in functions:
            if c["fn"] not in summary["missing"]:
                summary["missing"].append(c["fn"])
            summary["failed"] += 1
            continue
        got = call(functions, c)
        expected = {"ok": c.get("ok")}
        if json.dumps(got) == json.dumps(expected):
            summary["passed"] += 1
        else:
            summary["failed"] += 1
            if len(summary["mismatches"]) < 20:
                summary["mismatches"].append({"fn": c["fn"], "args": c["args"], "expected": expected, "actual": got})
    json.dump(summary, open(out_path, "w", encoding="utf-8"), ensure_ascii=False)


def bench(original, rust, sigs, cases_path, out_path, min_ms):
    signatures = json.load(open(sigs))
    old, new = bind(original, signatures), bind(rust, signatures)
    groups = {}
    for c in cases_of(cases_path):
        groups.setdefault(c["fn"], []).append(c)
    rows = []
    for name, cases in groups.items():
        if name not in old or name not in new:
            continue

        def timed(functions):
            def once():
                for c in cases:
                    call(functions, c)
            once()
            passes, start = 0, time.perf_counter()
            while True:
                once()
                passes += 1
                elapsed = time.perf_counter() - start
                if elapsed * 1000 >= min_ms:
                    return elapsed * 1e9 / (passes * len(cases))
        original_ns, rust_ns = timed(old), timed(new)
        rows.append({"fn": name, "cases": len(cases), "original_ns": original_ns, "rust_ns": rust_ns, "speedup": original_ns / rust_ns})
    json.dump(rows, open(out_path, "w"))


if __name__ == "__main__":
    mode, *rest = sys.argv[1:]
    if mode == "record":
        record(*rest)
    elif mode == "diff":
        diff(*rest)
    elif mode == "bench":
        bench(rest[0], rest[1], rest[2], rest[3], rest[4], int(rest[5]))
"#;
