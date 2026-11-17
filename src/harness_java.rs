//! The Java side of record, build, diff and bench. A generated driver calls the unit's static
//! methods by reflection: compiled with the original sources to record, and with the JNI
//! wrapper classes over the Rust library to diff.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::bridge::Manifest;
use crate::harness::{BenchRow, CASES, DiffOutcome};

const WORK_DIR: &str = ".rust-anything";

fn run(cmd: &mut Command, what: &str) -> Result<()> {
    let output = cmd.output().with_context(|| format!("running {what}"))?;
    if !output.status.success() {
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let tail: Vec<&str> = text
            .lines()
            .filter(|l| !l.starts_with("Picked up JAVA_TOOL_OPTIONS"))
            .rev()
            .take(30)
            .collect();
        bail!(
            "{what} failed ({}):\n{}",
            output.status,
            tail.into_iter().rev().collect::<Vec<_>>().join("\n")
        );
    }
    Ok(())
}

fn java_files(dir: &Path) -> Vec<PathBuf> {
    crate::walk::builder(dir)
        .require_git(false)
        .build()
        .flatten()
        .map(|e| e.into_path())
        .filter(|p| p.extension().is_some_and(|e| e == "java"))
        .collect()
}

/// Compiles the driver with the original sources (`rust: false`) or the wrapper classes.
fn compile(crate_dir: &Path, m: &Manifest, rust: bool) -> Result<PathBuf> {
    let work = crate_dir.canonicalize()?.join(WORK_DIR);
    let support = work.join("java-support/rust-anything");
    std::fs::create_dir_all(&support)?;
    std::fs::write(
        support.join("RustAnythingJson.java"),
        crate::codegen_java::JSON_JAVA,
    )?;
    std::fs::write(
        support.join("RustAnythingDriver.java"),
        crate::codegen_java::DRIVER_JAVA,
    )?;
    let classes = work.join(if rust { "java-rust" } else { "java-original" });
    if classes.exists() {
        std::fs::remove_dir_all(&classes)?;
    }
    std::fs::create_dir_all(&classes)?;
    let mut sources = java_files(&work.join("java-support"));
    if rust {
        sources.extend(
            java_files(&crate_dir.join("java"))
                .into_iter()
                .filter(|p| !p.ends_with("rustanything/RustAnythingJson.java")),
        );
    } else {
        // The whole source tree: the unit's classes may use others from the same module.
        let ws = crate::lang::java::discover(&m.root)?;
        sources.extend(
            ws.files
                .iter()
                .filter(|f| f.kind == crate::ts::discover::FileKind::Source)
                .map(|f| m.root.join(&f.path)),
        );
    }
    run(
        Command::new("javac")
            .args(["-encoding", "UTF-8", "-nowarn", "-d"])
            .arg(&classes)
            .args(&sources),
        if rust {
            "javac (driver and wrapper classes)"
        } else {
            "javac (driver and original sources)"
        },
    )?;
    Ok(classes)
}

fn driver(
    crate_dir: &Path,
    m: &Manifest,
    mode: &str,
    cases: &Path,
    out: &Path,
    min_ms: u64,
    rust: bool,
) -> Result<()> {
    let classes = compile(crate_dir, m, rust)?;
    let work = crate_dir.join(WORK_DIR);
    let targets = work.join("targets.json");
    std::fs::write(&targets, crate::codegen_java::targets(m).to_string())?;
    let mut cmd = Command::new("java");
    cmd.arg("-Dfile.encoding=UTF-8");
    if rust {
        let lib = crate::harness_c::built_library(crate_dir, m)?;
        cmd.arg(format!(
            "-Drust-anything.lib.{}={}",
            m.lib_name(),
            lib.canonicalize()?.display()
        ));
    }
    cmd.arg("-cp")
        .arg(&classes)
        .arg("rustanything.RustAnythingDriver")
        .arg(mode)
        .arg(&targets)
        .arg(cases)
        .arg(out)
        .arg(min_ms.to_string());
    run(&mut cmd, &format!("java driver ({mode})"))
}

pub fn record(crate_dir: &Path, m: &Manifest, cases: &[Value]) -> Result<PathBuf> {
    std::fs::create_dir_all(crate_dir.join(WORK_DIR))?;
    let inputs = crate_dir.join(WORK_DIR).join("inputs.json");
    std::fs::write(&inputs, serde_json::to_string(cases)?)?;
    let out = crate_dir.join(CASES);
    driver(crate_dir, m, "record", &inputs, &out, 0, false)?;
    Ok(out)
}

pub fn diff(crate_dir: &Path, m: &Manifest) -> Result<DiffOutcome> {
    let cases = crate_dir.join(CASES);
    if !cases.exists() {
        bail!(
            "no {CASES} in {} (run `rust-anything record` first)",
            crate_dir.display()
        );
    }
    let out = crate_dir.join(WORK_DIR).join("diff.json");
    driver(crate_dir, m, "diff", &cases, &out, 0, true)?;
    Ok(serde_json::from_str(&std::fs::read_to_string(&out)?)?)
}

pub fn bench(crate_dir: &Path, m: &Manifest, min_ms: u64) -> Result<Vec<BenchRow>> {
    let cases = crate_dir.join(CASES);
    if !cases.exists() {
        bail!(
            "no {CASES} in {} (run `rust-anything record` first)",
            crate_dir.display()
        );
    }
    let mut timings = Vec::new();
    for rust in [false, true] {
        let out = crate_dir.join(WORK_DIR).join(if rust {
            "bench-rust.json"
        } else {
            "bench-original.json"
        });
        driver(crate_dir, m, "bench", &cases, &out, min_ms, rust)?;
        timings.push(serde_json::from_str(&std::fs::read_to_string(&out)?)?);
    }
    Ok(crate::harness::bench_rows(m, &timings[0], &timings[1]))
}
