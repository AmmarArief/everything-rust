//! What a recording was made from, and the differences from it a port is allowed to keep.
//!
//! `record` saves a fingerprint of the original's unit (`recorded-from.json`): when the original
//! changes later, `diff` and `gate` say the recording is stale instead of passing against old
//! behavior. `known-diffs.jsonl` lists recorded cases where the port is meant to differ, each
//! with the Rust result it gives and why; any other difference still fails, and so does an entry
//! that no longer happens, so the list cannot outlive the differences it explains.

use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{Value, json};

use crate::bridge::Manifest;
use crate::harness::DiffOutcome;

pub const RECORDED_FROM: &str = "recorded-from.json";
pub const KNOWN_DIFFS: &str = "known-diffs.jsonl";

/// FNV-1a over the unit's files (relative path and contents, in path order): stable across Rust
/// versions and platforms, unlike `DefaultHasher`.
pub fn fingerprint(m: &Manifest) -> Option<(String, usize)> {
    let unit = m.root.join(&m.unit);
    let mut files: Vec<std::path::PathBuf> = if unit.is_file() {
        vec![unit.clone()]
    } else if unit.is_dir() {
        crate::walk::filtered(&unit, |e| {
            !matches!(
                e.file_name().to_str(),
                Some("node_modules" | "target" | ".git" | "__pycache__")
            )
        })
        .build()
        .flatten()
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .map(|e| e.into_path())
        .collect()
    } else {
        return None;
    };
    files.sort();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            hash ^= u64::from(*b);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for file in &files {
        let relative = file.strip_prefix(&m.root).unwrap_or(file);
        eat(relative.to_string_lossy().replace('\\', "/").as_bytes());
        eat(&[0]);
        eat(&std::fs::read(file).ok()?);
        eat(&[0]);
    }
    Some((format!("{hash:016x}"), files.len()))
}

/// Saves the fingerprint of the original next to `cases.jsonl`.
pub fn save_fingerprint(crate_dir: &Path, m: &Manifest) -> Result<()> {
    let path = crate_dir.join(RECORDED_FROM);
    match fingerprint(m) {
        Some((hash, files)) => std::fs::write(
            &path,
            serde_json::to_string_pretty(
                &json!({ "unit": m.unit, "files": files, "fingerprint": hash }),
            )? + "\n",
        )
        .with_context(|| format!("writing {}", path.display())),
        None => Ok(()),
    }
}

/// Why the recording no longer describes the original, if it does not: the unit's files changed
/// since `record`. `None` when they did not, or when there is nothing to compare (a recording
/// made before fingerprints, or the original not at hand).
pub fn stale(crate_dir: &Path, m: &Manifest) -> Option<String> {
    let saved: Value =
        serde_json::from_str(&std::fs::read_to_string(crate_dir.join(RECORDED_FROM)).ok()?).ok()?;
    let (now, _) = fingerprint(m)?;
    (saved["fingerprint"].as_str()? != now).then(|| {
        format!(
            "{} changed since it was recorded: re-run `rust-anything record {}` so the port is \
             checked against what the original does now",
            m.unit,
            crate_dir.display()
        )
    })
}

/// The entries of `known-diffs.jsonl`: `{"fn", "args", "actual", "reason"}` per line, `actual`
/// being the port's result as `diff --json` prints it. An entry without a reason is refused.
pub fn known_diffs(crate_dir: &Path) -> Result<Vec<Value>> {
    let path = crate_dir.join(KNOWN_DIFFS);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(Vec::new());
    };
    let mut entries = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let entry: Value = serde_json::from_str(line)
            .with_context(|| format!("{}:{}: not JSON", path.display(), i + 1))?;
        for key in ["fn", "args", "actual"] {
            anyhow::ensure!(
                entry.get(key).is_some(),
                "{}:{}: an entry needs `{key}`",
                path.display(),
                i + 1
            );
        }
        anyhow::ensure!(
            entry["reason"]
                .as_str()
                .is_some_and(|r| !r.trim().is_empty()),
            "{}:{}: an entry needs a `reason`: why the port is meant to differ here",
            path.display(),
            i + 1
        );
        entries.push(entry);
    }
    Ok(entries)
}

/// Counts the mismatches a known-diffs entry explains as passed (in `accepted`), and records the
/// entries no mismatch matched (in `stale_known`). A mismatch is explained only when the port
/// gives exactly the entry's `actual`: a different wrong result still fails.
pub fn apply_known(mut outcome: DiffOutcome, entries: &[Value]) -> DiffOutcome {
    if entries.is_empty() {
        return outcome;
    }
    let mut used = vec![false; entries.len()];
    outcome.mismatches.retain(|m| {
        let hit = entries.iter().position(|e| {
            e["fn"] == m["fn"] && e["args"] == m["args"] && e["actual"] == m["actual"]
        });
        match hit {
            Some(i) => {
                used[i] = true;
                false
            }
            None => true,
        }
    });
    let accepted = used.iter().filter(|u| **u).count();
    outcome.accepted = accepted;
    outcome.failed -= accepted.min(outcome.failed);
    outcome.passed += accepted;
    outcome.stale_known = entries
        .iter()
        .zip(&used)
        .filter(|(_, u)| !**u)
        .map(|(e, _)| json!({ "fn": e["fn"], "args": e["args"], "reason": e["reason"] }))
        .collect();
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome() -> DiffOutcome {
        DiffOutcome {
            total: 3,
            passed: 1,
            failed: 2,
            mismatches: vec![
                json!({"fn": "f", "args": [1], "expected": {"ok": 1}, "actual": {"ok": 2}}),
                json!({"fn": "f", "args": [2], "expected": {"ok": 1}, "actual": {"ok": 3}}),
            ],
            ..DiffOutcome::default()
        }
    }

    #[test]
    fn known_diffs_pass_only_with_the_listed_result() {
        let entries = [
            json!({"fn": "f", "args": [1], "actual": {"ok": 2}, "reason": "fixed a bug"}),
            json!({"fn": "f", "args": [2], "actual": {"ok": 4}, "reason": "other result"}),
            json!({"fn": "g", "args": [], "actual": {"ok": 0}, "reason": "gone"}),
        ];
        let out = apply_known(outcome(), &entries);
        assert_eq!((out.passed, out.failed, out.accepted), (2, 1, 1));
        assert_eq!(out.mismatches.len(), 1);
        assert_eq!(out.mismatches[0]["args"], json!([2]));
        assert_eq!(out.stale_known.len(), 2);
    }

    #[test]
    fn entries_need_a_reason() {
        let dir = std::env::temp_dir().join(format!("ra-known-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(KNOWN_DIFFS),
            "// comment\n{\"fn\": \"f\", \"args\": [1], \"actual\": {\"ok\": 2}}\n",
        )
        .unwrap();
        let err = known_diffs(&dir).unwrap_err();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(format!("{err:#}").contains("reason"), "{err:#}");
    }
}
