//! `profile`: run a real workload under the language's own profiler and say what to port.
//!
//! Porting a leaf function rarely pays: each call into Rust costs more than the leaf's work.
//! What pays is moving a hot loop with its callers, keeping data on the Rust side, and using all
//! cores. This step finds those: hot functions in the repository, by inclusive time, with the
//! callers that spend it, call counts where the profiler has them, and a recommendation each.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::Value;

use crate::lang::Lang;

pub const SCHEMA: &str = "carcinize/profile@1";

#[derive(Debug, Serialize)]
pub struct Caller {
    pub name: String,
    pub file: String,
    pub line: u32,
    /// Time of the callee spent under this caller.
    pub ms: f64,
    /// Calls from this caller, when the profiler counts calls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calls: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct Hot {
    pub name: String,
    pub file: String,
    pub line: u32,
    /// The analyzed unit holding the function, and whether the unit exports it.
    pub unit: Option<String>,
    pub exported: bool,
    pub self_ms: f64,
    pub total_ms: f64,
    pub total_pct: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calls: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ns_per_call: Option<f64>,
    pub callers: Vec<Caller>,
    pub advice: String,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub language: Lang,
    pub command: Vec<String>,
    pub total_ms: f64,
    /// Share of the run spent in the repository's own code (the rest: runtime, libraries).
    pub repo_pct: f64,
    pub hot: Vec<Hot>,
}

/// One function's numbers before advice: key is (file, name, line).
#[derive(Default)]
struct Acc {
    self_ms: f64,
    total_ms: f64,
    calls: Option<u64>,
    callers: HashMap<(String, String, u32), (f64, Option<u64>)>,
}

type Key = (String, String, u32);

pub fn profile(root: &Path, command: &[String], lang: Option<Lang>) -> Result<Report> {
    if command.is_empty() {
        bail!("give the workload to run after `--`, e.g. `carcinize profile . -- node bench.mjs`");
    }
    let root = root
        .canonicalize()
        .with_context(|| format!("resolving {}", root.display()))?;
    let lang = lang.unwrap_or_else(|| Lang::detect(&root));
    let work = std::env::temp_dir().join(format!("carcinize-profile-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;
    let (functions, total_ms) = match lang {
        Lang::Ts => node(&root, command, &work)?,
        Lang::Python => python(&root, command, &work)?,
        other => bail!(
            "`profile` runs TS/JS (node --cpu-prof) and Python (cProfile) workloads; for {} use its \
             profiler (Go: pprof, Java: JFR, Ruby: stackprof, C: perf) and port the hot loops it shows",
            other.name()
        ),
    };
    let _ = std::fs::remove_dir_all(&work);
    Ok(report(&root, lang, command, functions, total_ms))
}

/// A frame's file: repository-relative, or absolute for user code elsewhere (a workload
/// script); `None` for the runtime and installed libraries.
fn relative(root: &Path, file: &str) -> Option<String> {
    let path = file.strip_prefix("file://").unwrap_or(file);
    if !path.starts_with('/') && !path.contains(":\\") {
        return None; // node:internal/..., <frozen ...>, ~
    }
    if path.split(['/', '\\']).any(|p| {
        matches!(
            p,
            "node_modules" | "site-packages" | "dist-packages" | ".venv" | "venv"
        )
    }) || path.contains("/lib/python3")
    {
        return None;
    }
    let p = PathBuf::from(path);
    Some(match p.strip_prefix(root) {
        Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
        Err(_) => path.to_string(),
    })
}

/// Whether a frame file is inside the repository (relative).
fn in_repo(file: &str) -> bool {
    !file.starts_with('/')
}

// ---- node --cpu-prof ------------------------------------------------------------------------

fn node(root: &Path, command: &[String], work: &Path) -> Result<(HashMap<Key, Acc>, f64)> {
    let dir = work.join("node");
    std::fs::create_dir_all(&dir)?;
    let mut options = std::env::var("NODE_OPTIONS").unwrap_or_default();
    options.push_str(&format!(" --cpu-prof --cpu-prof-dir={}", dir.display()));
    let status = Command::new(&command[0])
        .args(&command[1..])
        .current_dir(root)
        .env("NODE_OPTIONS", options.trim())
        .status()
        .with_context(|| format!("running {}", command[0]))?;
    if !status.success() {
        bail!("the workload failed ({status})");
    }
    let mut functions: HashMap<Key, Acc> = HashMap::new();
    let mut total = 0.0;
    for entry in std::fs::read_dir(&dir)?.flatten() {
        let profile: Value = serde_json::from_str(&std::fs::read_to_string(entry.path())?)?;
        total += merge_cpuprofile(root, &profile, &mut functions);
    }
    if total == 0.0 {
        bail!("no CPU profile was written: is the workload a node process?");
    }
    Ok((functions, total))
}

/// A `.cpuprofile` node: its frame when it is user code, its own time, its children.
struct Frame {
    key: Option<Key>,
    self_ms: f64,
    children: Vec<u64>,
}

/// Adds one `.cpuprofile` to `functions`; returns its duration in ms.
fn merge_cpuprofile(root: &Path, profile: &Value, functions: &mut HashMap<Key, Acc>) -> f64 {
    let Some(nodes) = profile["nodes"].as_array() else {
        return 0.0;
    };
    let samples = profile["samples"].as_array().map_or(0, Vec::len).max(1);
    let duration_ms = (profile["endTime"].as_f64().unwrap_or(0.0)
        - profile["startTime"].as_f64().unwrap_or(0.0))
        / 1000.0;
    let per_sample = duration_ms / samples as f64;
    let mut by_id: HashMap<u64, Frame> = HashMap::new();
    let mut is_child = HashSet::new();
    for n in nodes {
        let id = n["id"].as_u64().unwrap_or(0);
        let frame = &n["callFrame"];
        let name = frame["functionName"].as_str().unwrap_or("").to_string();
        let url = frame["url"].as_str().unwrap_or("");
        let line = u32::try_from(frame["lineNumber"].as_i64().unwrap_or(0) + 1).unwrap_or(0);
        let key = relative(root, url).map(|file| {
            let shown = if name.is_empty() {
                format!("(anonymous):{line}")
            } else {
                name.clone()
            };
            (file, shown, line)
        });
        let children: Vec<u64> = n["children"]
            .as_array()
            .map_or_else(Vec::new, |c| c.iter().filter_map(Value::as_u64).collect());
        is_child.extend(children.iter().copied());
        let self_ms = n["hitCount"].as_f64().unwrap_or(0.0) * per_sample;
        by_id.insert(
            id,
            Frame {
                key,
                self_ms,
                children,
            },
        );
    }
    // Inclusive time per node, depth first.
    fn total(id: u64, by_id: &HashMap<u64, Frame>, memo: &mut HashMap<u64, f64>) -> f64 {
        if let Some(t) = memo.get(&id) {
            return *t;
        }
        let Some(frame) = by_id.get(&id) else {
            return 0.0;
        };
        let t = frame.self_ms
            + frame
                .children
                .iter()
                .map(|c| total(*c, by_id, memo))
                .sum::<f64>();
        memo.insert(id, t);
        t
    }
    let mut memo = HashMap::new();
    // Walk from the roots, tracking the repo frames on the stack: recursion counts once, and a
    // callee's caller is the nearest *named* repo frame above it (callbacks are skipped).
    let roots: Vec<u64> = by_id
        .keys()
        .filter(|id| !is_child.contains(id))
        .copied()
        .collect();
    let mut stack: Vec<(u64, Vec<Key>)> = roots.into_iter().map(|r| (r, Vec::new())).collect();
    while let Some((id, active)) = stack.pop() {
        let Some(Frame {
            key,
            self_ms,
            children,
        }) = by_id.get(&id)
        else {
            continue;
        };
        let mut next = active.clone();
        if let Some(k) = key {
            let inclusive = total(id, &by_id, &mut memo);
            let acc = functions.entry(k.clone()).or_default();
            acc.self_ms += self_ms;
            if !active.contains(k) {
                acc.total_ms += inclusive;
                if let Some(caller) = active
                    .iter()
                    .rev()
                    .find(|c| !c.1.starts_with("(anonymous)"))
                    && caller != k
                {
                    acc.callers.entry(caller.clone()).or_default().0 += inclusive;
                }
            }
            // Anonymous callbacks stay on the stack too; the caller lookup above skips them.
            next.push(k.clone());
        }
        for c in children {
            stack.push((*c, next.clone()));
        }
    }
    duration_ms
}

// ---- Python cProfile ------------------------------------------------------------------------

const PSTATS_PY: &str = r#"
import json, pstats, sys
stats = pstats.Stats(sys.argv[1]).stats
out = []
for (file, line, name), (cc, nc, tt, ct, callers) in stats.items():
    out.append({
        "file": file, "line": line, "name": name, "calls": nc, "self": tt * 1000, "total": ct * 1000,
        "callers": [{"file": f, "line": l, "name": n, "calls": c[1], "total": c[3] * 1000} for (f, l, n), c in callers.items()],
    })
json.dump(out, open(sys.argv[2], "w"))
"#;

fn python(root: &Path, command: &[String], work: &Path) -> Result<(HashMap<Key, Acc>, f64)> {
    let program = Path::new(&command[0])
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !program.starts_with("python") {
        bail!(
            "for Python, give the interpreter and script: `carcinize profile . -- python3 bench.py`"
        );
    }
    let stats = work.join("profile.pstats");
    let start = std::time::Instant::now();
    let status = Command::new(&command[0])
        .args(["-m", "cProfile", "-o"])
        .arg(&stats)
        .args(&command[1..])
        .current_dir(root)
        .status()
        .with_context(|| format!("running {}", command[0]))?;
    let total_ms = start.elapsed().as_secs_f64() * 1000.0;
    if !status.success() {
        bail!("the workload failed ({status})");
    }
    let json = work.join("profile.json");
    let script = work.join("pstats_json.py");
    std::fs::write(&script, PSTATS_PY)?;
    let status = Command::new(&command[0])
        .arg(&script)
        .arg(&stats)
        .arg(&json)
        .status()?;
    if !status.success() {
        bail!("reading the cProfile output failed");
    }
    let entries: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&json)?)?;
    let mut functions: HashMap<Key, Acc> = HashMap::new();
    for e in &entries {
        let Some(file) = relative(root, e["file"].as_str().unwrap_or("")) else {
            continue;
        };
        let key = (
            file,
            e["name"].as_str().unwrap_or("").to_string(),
            u32::try_from(e["line"].as_u64().unwrap_or(0)).unwrap_or(0),
        );
        let acc = functions.entry(key).or_default();
        acc.self_ms += e["self"].as_f64().unwrap_or(0.0);
        acc.total_ms += e["total"].as_f64().unwrap_or(0.0);
        acc.calls = Some(acc.calls.unwrap_or(0) + e["calls"].as_u64().unwrap_or(0));
        for c in e["callers"].as_array().into_iter().flatten() {
            let Some(cfile) = relative(root, c["file"].as_str().unwrap_or("")) else {
                continue;
            };
            let ckey = (
                cfile,
                c["name"].as_str().unwrap_or("").to_string(),
                u32::try_from(c["line"].as_u64().unwrap_or(0)).unwrap_or(0),
            );
            let slot = acc.callers.entry(ckey).or_default();
            slot.0 += c["total"].as_f64().unwrap_or(0.0);
            slot.1 = Some(slot.1.unwrap_or(0) + c["calls"].as_u64().unwrap_or(0));
        }
    }
    Ok((functions, total_ms))
}

// ---- the report -----------------------------------------------------------------------------

/// The argument a caller passes per loop item: an arrow or for-of variable among `f`'s call
/// arguments in the caller's source. `None` when the source does not show one.
fn batch_param(root: &Path, caller: &Key, callee: &str, params: &[String]) -> Option<String> {
    let text = std::fs::read_to_string(root.join(&caller.0)).ok()?;
    let lines: Vec<&str> = text.lines().collect();
    let start = (caller.2 as usize).saturating_sub(1);
    let body = lines.get(start..(start + 80).min(lines.len()))?.join("\n");
    let mut loop_vars: Vec<String> = Vec::new();
    for (i, _) in body.match_indices("=>") {
        let before = body[..i].trim_end().trim_end_matches(')');
        let name: String = before
            .chars()
            .rev()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        if !name.is_empty() {
            loop_vars.push(name);
        }
    }
    for word in ["const ", "let ", "var ", "for "] {
        for (i, _) in body.match_indices(word) {
            let rest = &body[i + word.len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if rest[name.len()..].trim_start().starts_with("of ")
                || rest[name.len()..].trim_start().starts_with("in ")
            {
                loop_vars.push(name);
            }
        }
    }
    let call = body.find(&format!("{callee}("))?;
    let args_text = &body[call + callee.len() + 1..];
    let mut depth = 0;
    let mut args = vec![String::new()];
    for c in args_text.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => break,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                args.push(String::new());
                continue;
            }
            _ => {}
        }
        args.last_mut()?.push(c);
    }
    let position = args
        .iter()
        .position(|a| loop_vars.iter().any(|v| v == a.trim()))?;
    params.get(position).cloned()
}

fn report(
    root: &Path,
    lang: Lang,
    command: &[String],
    functions: HashMap<Key, Acc>,
    total_ms: f64,
) -> Report {
    let analysis = crate::analyze::analyze_lang(root, lang).ok();
    let unit_of = |file: &str| -> Option<(String, bool)> {
        let a = analysis.as_ref()?;
        let u = a.units.iter().find(|u| u.files.iter().any(|f| f == file))?;
        let portable = !matches!(
            u.status,
            crate::analyze::Status::Keep | crate::analyze::Status::Blocked
        );
        Some((u.id.clone(), portable))
    };
    let exports_of = |file: &str, name: &str| -> Option<crate::ts::exports::ExportedFn> {
        let text = std::fs::read_to_string(root.join(file)).ok()?;
        let fns = if lang == Lang::Ts {
            crate::ts::exports::exported_functions(file, &text)
        } else {
            crate::lang::exports(lang, file, &text).0
        };
        fns.into_iter().find(|f| f.name == name)
    };
    let repo_self: f64 = functions
        .iter()
        .filter(|(k, _)| in_repo(&k.0))
        .map(|(_, a)| a.self_ms)
        .sum();
    let mut hot: Vec<Hot> = functions
        .into_iter()
        .filter(|((file, _, _), a)| in_repo(file) && total_ms > 0.0 && a.total_ms / total_ms >= 0.01)
        .map(|((file, name, line), a)| {
            let unit = unit_of(&file);
            let export = exports_of(&file, &name);
            let mut callers: Vec<Caller> = a
                .callers
                .iter()
                .map(|((f, n, l), (ms, calls))| Caller { name: n.clone(), file: f.clone(), line: *l, ms: *ms, calls: *calls })
                .collect();
            callers.sort_by(|x, y| y.ms.total_cmp(&x.ms));
            callers.truncate(5);
            let ns_per_call = a.calls.filter(|c| *c > 0).map(|c| a.total_ms * 1e6 / c as f64);
            let total_pct = 100.0 * a.total_ms / total_ms;
            let top = callers.first();
            let small_per_call = ns_per_call.is_some_and(|ns| ns < 20_000.0) || (a.calls.is_none() && top.is_some_and(|c| c.ms >= 0.5 * a.total_ms));
            let advice = match (&unit, &export) {
                (Some((u, true)), Some(e)) if small_per_call && top.is_some() => {
                    let c = top.expect("checked");
                    let params: Vec<String> = e.params.iter().map(|p| p.name.clone()).collect();
                    let batch = batch_param(root, &(c.file.clone(), c.name.clone(), c.line), &name, &params);
                    let calls = a.calls.map_or_else(String::new, |n| format!(", {n} calls"));
                    match batch {
                        Some(param) => format!(
                            "Called in a loop by `{}` ({}:{}{calls}): a call per item would cost more than the work. \
                             Batch it, keeping the items in Rust and using all cores: \
                             `carcinize bridge {} {u} --batch {name}:{param}`, then prepare the items once in a \
                             `{}Index` in port.rs. Better still, port `{}` with it.",
                            c.name, c.file, c.line, root.display(), crate::codegen_batch::pascal(&name), c.name
                        ),
                        None => format!(
                            "Mostly called by `{}` ({}:{}{calls}): port `{}` together with `{name}`, so one call into Rust does the whole loop.",
                            c.name, c.file, c.line, c.name
                        ),
                    }
                }
                (Some((u, true)), Some(_)) => format!(
                    "Hot and exported by `{u}`: port it (`carcinize bridge {} {u}`), then check `carcinize bench`.",
                    root.display()
                ),
                (Some((u, true)), None) => format!("Hot inside `{u}`: port the exported function that calls it, with it."),
                (Some((u, false)), _) => format!("Hot, but `{u}` must stay in the source language (see `carcinize analyze`)."),
                (None, _) => "Hot, outside the analyzed units.".into(),
            };
            Hot {
                unit: unit.map(|(u, _)| u),
                exported: export.is_some(),
                name,
                file,
                line,
                self_ms: a.self_ms,
                total_ms: a.total_ms,
                total_pct,
                calls: a.calls,
                ns_per_call,
                callers,
                advice,
            }
        })
        .collect();
    hot.sort_by(|a, b| b.total_ms.total_cmp(&a.total_ms));
    hot.truncate(25);
    Report {
        schema: SCHEMA,
        language: lang,
        command: command.to_vec(),
        total_ms,
        repo_pct: if total_ms > 0.0 {
            (100.0 * repo_self / total_ms).min(100.0)
        } else {
            0.0
        },
        hot,
    }
}

pub fn markdown(r: &Report) -> String {
    use std::fmt::Write as _;
    let mut out = format!(
        "# Profile: `{}`\n\n{:.0} ms in all; {:.0}% of it in the repository's own code.\n\n\
         | Function | File | Total | Self | Calls | Unit |\n| --- | --- | ---: | ---: | ---: | --- |\n",
        r.command.join(" "),
        r.total_ms,
        r.repo_pct
    );
    for h in &r.hot {
        let _ = writeln!(
            out,
            "| `{}` | `{}:{}` | {:.0}% ({:.0} ms) | {:.0} ms | {} | {} |",
            h.name,
            h.file,
            h.line,
            h.total_pct,
            h.total_ms,
            h.self_ms,
            h.calls.map_or_else(|| "-".into(), |c| c.to_string()),
            h.unit
                .as_deref()
                .map_or_else(|| "-".into(), |u| format!("`{u}`"))
        );
    }
    out.push_str("\n## What to port\n\n");
    let mut seen = BTreeMap::new();
    for h in r.hot.iter().filter(|h| h.unit.is_some()) {
        if seen.insert(h.name.clone(), ()).is_some() {
            continue;
        }
        let _ = writeln!(
            out,
            "- **`{}`** ({:.0}%): {}",
            h.name, h.total_pct, h.advice
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frames_outside_user_code_are_dropped() {
        let root = Path::new("/repo");
        assert_eq!(relative(root, "node:internal/main"), None);
        assert_eq!(relative(root, "file:///repo/node_modules/x/index.js"), None);
        assert_eq!(relative(root, "/usr/lib/python3.13/json/__init__.py"), None);
        assert_eq!(
            relative(root, "file:///repo/src/fuzzy.ts").as_deref(),
            Some("src/fuzzy.ts")
        );
        // A workload script elsewhere is user code, shown with its full path.
        assert_eq!(
            relative(root, "/tmp/bench.ts").as_deref(),
            Some("/tmp/bench.ts")
        );
        assert!(in_repo("src/fuzzy.ts") && !in_repo("/tmp/bench.ts"));
    }

    fn node(id: u64, name: &str, url: &str, line: i64, hits: u64, children: &[u64]) -> Value {
        json!({ "id": id, "hitCount": hits, "children": children,
                "callFrame": { "functionName": name, "url": url, "lineNumber": line - 1 } })
    }

    #[test]
    fn cpuprofile_times_and_callers() {
        let f = "file:///repo/src/search.ts";
        // (root) -> matched -> (anonymous) -> fuzzyScore -> fuzzyScore (recursion); 1 ms a sample.
        let profile = json!({
            "startTime": 0, "endTime": 10_000, "samples": vec![0; 10],
            "nodes": [
                node(1, "(root)", "", 0, 0, &[2]),
                node(2, "matched", f, 9, 1, &[3]),
                node(3, "", f, 10, 1, &[4]),
                node(4, "fuzzyScore", f, 2, 5, &[5]),
                node(5, "fuzzyScore", f, 2, 3, &[]),
            ],
        });
        let mut functions = HashMap::new();
        let ms = merge_cpuprofile(Path::new("/repo"), &profile, &mut functions);
        assert_eq!(ms, 10.0);
        let key = |n: &str, l: u32| ("src/search.ts".to_string(), n.to_string(), l);
        let fuzzy = &functions[&key("fuzzyScore", 2)];
        assert_eq!(
            (fuzzy.self_ms, fuzzy.total_ms),
            (8.0, 8.0),
            "recursion counts once"
        );
        // The caller is the named function, not the arrow callback in between.
        assert_eq!(
            fuzzy.callers.keys().collect::<Vec<_>>(),
            [&key("matched", 9)]
        );
        assert_eq!(functions[&key("matched", 9)].total_ms, 10.0);
        assert!(functions.contains_key(&key("(anonymous):10", 10)));
    }

    #[test]
    fn finds_the_argument_that_varies_per_item() {
        let root = std::env::temp_dir().join(format!("carcinize-profile-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("search.ts"),
            "export function matched(query: string, all: string[]) {\n  return all.map(key => ({ key, score: fuzzyScore(query, key) }))\n}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("search.py"),
            "def best(q, keys):\n    for k in keys:\n        yield score(q, k)\n",
        )
        .unwrap();
        let params = ["query".to_string(), "text".to_string()];
        let caller = |file: &str, name: &str| (file.to_string(), name.to_string(), 1);
        assert_eq!(
            batch_param(
                &root,
                &caller("search.ts", "matched"),
                "fuzzyScore",
                &params
            )
            .as_deref(),
            Some("text")
        );
        assert_eq!(
            batch_param(&root, &caller("search.py", "best"), "score", &params).as_deref(),
            Some("text")
        );
        // `query` is the same for every item: no batch parameter to give.
        assert_eq!(
            batch_param(&root, &caller("search.ts", "matched"), "missing", &params),
            None
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
