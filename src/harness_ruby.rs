//! The Ruby side of record, build, diff and bench. A generated driver calls the module
//! functions by name: with the original files loaded to record, and with the Fiddle wrapper
//! over the Rust library loaded to diff.

use crate::walk::Canonical;
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
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = stderr.lines().rev().take(30).collect();
        bail!(
            "{what} failed ({}):\n{}",
            output.status,
            tail.into_iter().rev().collect::<Vec<_>>().join("\n")
        );
    }
    Ok(())
}

/// The wrapper file, relative to the crate.
pub fn wrapper_path(m: &Manifest) -> String {
    format!("ruby/{}.rb", m.lib_name())
}

/// The files to load: the unit's sources, or the wrapper.
fn files(crate_dir: &Path, m: &Manifest, rust: bool) -> Result<Vec<PathBuf>> {
    if rust {
        return Ok(vec![crate_dir.join(wrapper_path(m)).canonical()?]);
    }
    let mut files: Vec<PathBuf> = m
        .exports
        .iter()
        .map(|f| m.root.join(&f.file))
        .chain(m.js_only.iter().map(|j| m.root.join(&j.file)))
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
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
    let work = crate_dir.join(WORK_DIR);
    std::fs::create_dir_all(&work)?;
    let script = work.join("driver.rb");
    std::fs::write(&script, crate::codegen_ruby::driver())?;
    let targets = work.join("targets.json");
    std::fs::write(&targets, crate::codegen_ruby::targets(m).to_string())?;
    let mut cmd = Command::new("ruby");
    // Source and data are UTF-8 whatever the machine's locale says.
    cmd.args(["-E", "UTF-8:UTF-8"]);
    // The original's `require "name"` finds its own files through each package's lib folder.
    if !rust && let Ok(ws) = crate::lang::ruby::discover(&m.root) {
        for p in &ws.packages {
            cmd.arg(format!("-I{}", m.root.join(&p.src_root).display()));
        }
    }
    cmd.arg(&script)
        .arg(mode)
        .arg(&targets)
        .arg(cases)
        .arg(out)
        .arg(min_ms.to_string())
        .args(files(crate_dir, m, rust)?);
    run(&mut cmd, &format!("ruby driver ({mode})"))
}

pub fn record(crate_dir: &Path, m: &Manifest, cases: &[Value]) -> Result<PathBuf> {
    let inputs = crate_dir.join(WORK_DIR).join("inputs.json");
    std::fs::create_dir_all(crate_dir.join(WORK_DIR))?;
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

/// Builds the extension and puts it beside the wrapper as `ruby/<lib>_native.so`.
pub fn build(crate_dir: &Path, m: &Manifest) -> Result<PathBuf> {
    let lib = crate::harness_c::build(crate_dir, m)?;
    let ext = if cfg!(target_os = "macos") {
        "bundle"
    } else {
        "so"
    };
    let target = crate_dir.join(format!("ruby/{}_native.{ext}", m.lib_name()));
    std::fs::create_dir_all(crate_dir.join("ruby"))?;
    std::fs::copy(&lib, &target)?;
    Ok(target)
}
