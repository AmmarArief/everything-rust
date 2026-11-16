//! Step 2: scaffold a Rust crate for one unit, with JS bindings and a manifest the later
//! steps read.
//!
//! The crate splits bindings from logic: `src/lib.rs` holds thin `#[napi]` or
//! `#[wasm_bindgen]` wrappers, `src/port.rs` holds the port in plain Rust with
//! `#![forbid(unsafe_code)]`, so it can be tested without a JS runtime.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::analyze::{self, Bridge, Status};
use crate::codegen;
use crate::risks::Risk;
use crate::ts::exports::{
    ExportedClass, ExportedFn, LocalType, exported_classes, exported_functions, local_types,
};

pub const MANIFEST: &str = "carcinize.json";
pub const SCHEMA: &str = "carcinize/bridge@1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Napi,
    Wasm,
}

/// What `bridge` generated; every later step starts from it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: String,
    /// Absolute repository root.
    pub root: PathBuf,
    pub unit: String,
    pub kind: Kind,
    pub crate_name: String,
    /// Exports ported to Rust.
    pub exports: Vec<ExportedFn>,
    /// Exported classes ported to Rust: a struct with `new` and methods.
    #[serde(default)]
    pub classes: Vec<ExportedClass>,
    /// Exports that cannot cross the bridge as they are; they stay in TS and the shadow module
    /// re-exports them from the original.
    #[serde(default)]
    pub js_only: Vec<JsOnly>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsOnly {
    pub name: String,
    pub file: String,
    pub line: u32,
    pub reason: String,
}

/// Whether values of the local type `name` can hold functions: the type has a function or
/// method member, or extends or mentions a type that does.
fn has_functions(name: &str, local_types: &[LocalType], seen: &mut Vec<String>) -> bool {
    if seen.iter().any(|s| s == name) {
        return false;
    }
    seen.push(name.to_string());
    let Some(t) = local_types.iter().find(|t| t.name == name) else {
        return false;
    };
    let text = &t.text;
    let method = text.lines().skip(1).any(|l| {
        let l = l.trim().trim_start_matches("readonly ");
        l.contains('(')
            && l.split('(').next().is_some_and(|head| {
                !head.is_empty()
                    && head
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '?')
            })
    });
    if text.contains("=>") || method {
        return true;
    }
    text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .filter(|w| !w.is_empty() && *w != name)
        .any(|w| has_functions(w, local_types, seen))
}

/// Why an export cannot cross a JS/Rust bridge unchanged, if it cannot.
///
/// Functions, iterators, RegExp objects and objects with function members are JS values a
/// Rust function can neither take nor return; porting them would change the public API.
pub fn js_only_reason(f: &ExportedFn, local_types: &[LocalType]) -> Option<String> {
    if f.is_generator {
        return Some("generator: returns a lazy iterator".into());
    }
    if f.uses_instanceof {
        return Some("checks `instanceof`: class identity does not survive the bridge".into());
    }
    let types: Vec<&str> = f
        .params
        .iter()
        .map(|p| p.ts_type.as_str())
        .chain(std::iter::once(f.returns.as_str()))
        .collect();
    if types.iter().any(|t| t.contains("=>")) {
        return Some("takes or returns a function".into());
    }
    let words = |t: &str| -> Vec<String> {
        t.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect()
    };
    for t in &types {
        for word in words(t) {
            match word.as_str() {
                "RegExp" => return Some("takes or returns a RegExp".into()),
                "Generator" | "Iterator" | "IterableIterator" | "AsyncGenerator"
                | "AsyncIterable" => {
                    return Some(format!("uses `{word}`, a lazy iterator"));
                }
                "Function" => return Some("takes or returns a function".into()),
                _ => {}
            }
            if f.type_params.contains(&word) {
                return Some(format!(
                    "generic over `{word}`: values of any shape, functions included"
                ));
            }
            if has_functions(&word, local_types, &mut Vec::new()) {
                return Some(format!("uses `{word}`, which has function members"));
            }
        }
    }
    None
}

impl Manifest {
    pub fn load(crate_dir: &Path) -> Result<Self> {
        let path = crate_dir.join(MANIFEST);
        let text = std::fs::read_to_string(&path).with_context(|| {
            format!("reading {} (run `carcinize bridge` first)", path.display())
        })?;
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    /// Rust library name: the crate name with `-` replaced.
    pub fn lib_name(&self) -> String {
        self.crate_name.replace('-', "_")
    }
}

pub struct Options<'a> {
    pub root: &'a Path,
    pub unit: &'a str,
    pub kind: Option<Kind>,
    pub out: Option<PathBuf>,
    pub name: Option<String>,
    /// Bridge a unit the plan marks blocked or keep.
    pub force: bool,
    /// Exports to keep in TS, with an optional reason: `name` or `name: reason`.
    pub keep: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Outcome {
    pub crate_dir: PathBuf,
    pub kind: Kind,
    pub files: Vec<String>,
    pub exports: Vec<String>,
    pub notes: Vec<String>,
}

/// A Rust type as seen by the port, from a TS type annotation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RType {
    Str,
    F64,
    Bool,
    Unit,
    Vec(Box<RType>),
    Opt(Box<RType>),
    /// Anything structured: crosses the bridge as JSON.
    Json,
}

impl RType {
    pub fn from_ts(ts: &str) -> Self {
        let ts = ts.trim().trim_start_matches("readonly ").trim();
        // `(string | null)`: one parenthesized group around the whole type.
        if let Some(inner) = ts.strip_prefix('(').and_then(|t| t.strip_suffix(')'))
            && balanced(inner)
        {
            return Self::from_ts(inner);
        }
        if let Some(inner) = ts
            .strip_prefix("Promise<")
            .and_then(|t| t.strip_suffix('>'))
        {
            return Self::from_ts(inner);
        }
        let parts = split_top(ts, '|');
        if parts.len() > 1 {
            let (nullish, rest): (Vec<&str>, Vec<&str>) = parts
                .iter()
                .partition(|p| matches!(p.trim(), "undefined" | "null" | "void"));
            if rest.iter().all(|p| is_string_literal(p)) {
                return Self::wrap_opt(Self::Str, !nullish.is_empty());
            }
            if rest.len() == 1 {
                return Self::wrap_opt(Self::from_ts(rest[0]), !nullish.is_empty());
            }
            return Self::Json;
        }
        if let Some(inner) = ts.strip_suffix("[]") {
            return Self::Vec(Box::new(Self::from_ts(
                inner.trim_start_matches('(').trim_end_matches(')'),
            )));
        }
        for wrapper in ["Array<", "ReadonlyArray<"] {
            if let Some(inner) = ts.strip_prefix(wrapper).and_then(|t| t.strip_suffix('>')) {
                return Self::Vec(Box::new(Self::from_ts(inner)));
            }
        }
        // `value is MessageLeaf`: a type predicate returns a boolean; `asserts x` returns nothing.
        if ts.starts_with("asserts ") {
            return Self::Unit;
        }
        if let Some((subject, _)) = ts.split_once(" is ")
            && subject
                .trim()
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        {
            return Self::Bool;
        }
        match ts {
            "string" => Self::Str,
            "number" => Self::F64,
            "boolean" => Self::Bool,
            "void" | "undefined" => Self::Unit,
            _ if is_string_literal(ts) => Self::Str,
            _ => Self::Json,
        }
    }

    fn wrap_opt(t: Self, optional: bool) -> Self {
        if optional { Self::Opt(Box::new(t)) } else { t }
    }

    pub fn rust(&self) -> String {
        match self {
            Self::Str => "String".into(),
            Self::F64 => "f64".into(),
            Self::Bool => "bool".into(),
            Self::Unit => "()".into(),
            Self::Vec(t) => format!("Vec<{}>", t.rust()),
            Self::Opt(t) => format!("Option<{}>", t.rust()),
            Self::Json => "serde_json::Value".into(),
        }
    }

    pub fn contains_json(&self) -> bool {
        match self {
            Self::Json => true,
            Self::Vec(t) | Self::Opt(t) => t.contains_json(),
            _ => false,
        }
    }
}

fn is_string_literal(ts: &str) -> bool {
    let ts = ts.trim();
    ts.len() >= 2
        && ((ts.starts_with('\'') && ts.ends_with('\''))
            || (ts.starts_with('"') && ts.ends_with('"')))
}

fn balanced(ts: &str) -> bool {
    let mut depth = 0i32;
    for c in ts.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Splits on `sep` outside brackets.
fn split_top(ts: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in ts.char_indices() {
        match c {
            '<' | '(' | '[' | '{' => depth += 1,
            '>' | ')' | ']' | '}' => depth -= 1,
            c if c == sep && depth == 0 => {
                out.push(ts[start..i].trim());
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(ts[start..].trim());
    out.retain(|p| !p.is_empty());
    out
}

/// A parameter's type for the port, rest and optional parameters included.
pub fn param_type(p: &crate::ts::exports::Param) -> RType {
    let base = RType::from_ts(&p.ts_type);
    let base = if p.rest {
        match base {
            RType::Vec(_) => base,
            other => RType::Vec(Box::new(other)),
        }
    } else {
        base
    };
    if p.optional && !matches!(base, RType::Opt(_)) {
        RType::Opt(Box::new(base))
    } else {
        base
    }
}

pub fn snake_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev = i.checked_sub(1).map(|j| chars[j]);
            let next = chars.get(i + 1);
            let boundary = prev.is_some_and(|p| p.is_lowercase() || p.is_ascii_digit())
                || (prev.is_some_and(char::is_uppercase) && next.is_some_and(|n| n.is_lowercase()));
            if boundary {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else if c.is_alphanumeric() {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    const KEYWORDS: &[&str] = &[
        "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
        "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
        "return", "self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use",
        "where", "while", "async", "await", "dyn", "gen",
    ];
    if KEYWORDS.contains(&out.as_str()) {
        format!("r#{out}")
    } else {
        out
    }
}

/// Types declared in `file` and in the files it imports relatively (one level deep).
pub fn types_in_scope(root: &Path, file: &str, text: &str) -> Vec<LocalType> {
    let mut types = Vec::new();
    let mut seen = std::collections::HashSet::new();
    collect_types(root, file, text, 0, &mut seen, &mut types);
    types
}

/// Types of a file and of the files it imports or re-exports, a few levels deep
/// (`./types` -> `types/index.ts` -> `export * from './host'`).
fn collect_types(
    root: &Path,
    file: &str,
    text: &str,
    depth: usize,
    seen: &mut std::collections::HashSet<String>,
    types: &mut Vec<LocalType>,
) {
    if depth > 4 || !seen.insert(file.to_string()) {
        return;
    }
    types.extend(local_types(file, text));
    let dir = file.rsplit_once('/').map_or("", |(d, _)| d);
    for import in crate::ts::scan::scan(file, text).imports {
        let spec = import.specifier;
        if !spec.starts_with('.') {
            continue;
        }
        let joined = if dir.is_empty() {
            spec.clone()
        } else {
            format!("{dir}/{spec}")
        };
        let Some(base) = normalize_rel(&joined) else {
            continue;
        };
        let stem = base.trim_end_matches(".js");
        let candidates = [
            format!("{stem}.ts"),
            format!("{stem}.mts"),
            format!("{stem}.d.ts"),
            format!("{stem}/index.ts"),
            base.clone(),
        ];
        if let Some(found) = candidates.iter().find(|c| root.join(c).is_file())
            && let Ok(imported) = std::fs::read_to_string(root.join(found))
        {
            collect_types(root, found, &imported, depth + 1, seen, types);
        }
    }
}

/// Collapses `.` and `..` in a root-relative path.
fn normalize_rel(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// Replaces simple type aliases (`type KeyString = string`) by what they stand for.
pub fn resolve_aliases(ts: &str, types: &[LocalType]) -> String {
    let mut out = ts.to_string();
    for _ in 0..4 {
        let mut changed = false;
        let mut next = String::with_capacity(out.len());
        let mut word = String::new();
        let flush = |word: &mut String, next: &mut String, changed: &mut bool| {
            if let Some(t) = types.iter().find(|t| t.alias && t.name == *word) {
                let text = t.text.trim();
                if text.contains('|') {
                    next.push('(');
                    next.push_str(text);
                    next.push(')');
                } else {
                    next.push_str(text);
                }
                *changed = true;
            } else {
                next.push_str(word);
            }
            word.clear();
        };
        for c in out.chars() {
            if c.is_alphanumeric() || c == '_' || c == '$' {
                word.push(c);
            } else {
                flush(&mut word, &mut next, &mut changed);
                next.push(c);
            }
        }
        flush(&mut word, &mut next, &mut changed);
        out = next;
        if !changed {
            break;
        }
    }
    out
}

fn needs_js(api: &codegen::PortApi) -> bool {
    api.needs_json()
}

/// The reason given with `--keep name` or `--keep "name: reason"`, if `name` is kept.
fn keep_reason(keep: &[String], name: &str) -> Option<String> {
    keep.iter().find_map(|k| {
        let (kept, reason) = k
            .split_once(':')
            .map_or((k.as_str(), ""), |(n, r)| (n.trim(), r.trim()));
        (kept == name).then(|| {
            if reason.is_empty() {
                "kept in TS on request".to_string()
            } else {
                reason.to_string()
            }
        })
    })
}

/// Formats the generated Rust, best effort: the gate checks formatting.
fn format_rust(crate_dir: &Path) {
    let files: Vec<PathBuf> = std::fs::read_dir(crate_dir.join("src"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .collect();
    let _ = std::process::Command::new("rustfmt")
        .args(["--edition", "2024"])
        .args(&files)
        .output();
}

/// What `refresh` rewrote.
#[derive(Debug, Serialize)]
pub struct Refreshed {
    pub functions: Vec<String>,
    pub files: Vec<String>,
}

/// Regenerates the binding layer (`lib.rs`, `index.cjs`, `js.rs`) from the signatures in
/// `port.rs`, and adds the dependencies they need to Cargo.toml. Never touches `port.rs`.
pub fn refresh(crate_dir: &Path) -> Result<Refreshed> {
    let manifest = Manifest::load(crate_dir)?;
    let port =
        std::fs::read_to_string(crate_dir.join("src/port.rs")).context("reading src/port.rs")?;
    let fns = codegen::parse_port(&port);
    if fns.functions.is_empty() && fns.classes.is_empty() {
        bail!("src/port.rs has no `pub fn` or ported class: nothing to bind");
    }
    let modules = codegen::Modules {
        utf16: crate_dir.join("src/utf16.rs").exists(),
        js: needs_js(&fns) || crate_dir.join("src/js.rs").exists(),
    };
    let mut files = vec![
        ("src/lib.rs", codegen::lib_rs(&manifest, &fns, &modules)),
        ("index.cjs", codegen::index_cjs(&manifest, &fns)),
    ];
    if modules.js {
        files.push(("src/js.rs", codegen::JS_RS.to_string()));
    }
    let cargo_path = crate_dir.join("Cargo.toml");
    let cargo = std::fs::read_to_string(&cargo_path).context("reading Cargo.toml")?;
    let cargo = codegen::ensure_deps(&cargo, manifest.kind, &fns).map_err(anyhow::Error::msg)?;
    files.push(("Cargo.toml", cargo));
    for (path, text) in &files {
        std::fs::write(crate_dir.join(path), text).with_context(|| format!("writing {path}"))?;
    }
    format_rust(crate_dir);
    Ok(Refreshed {
        functions: fns
            .functions
            .iter()
            .map(|f| f.js_name.clone())
            .chain(fns.classes.iter().map(|c| format!("class {}", c.js_name)))
            .collect(),
        files: files.iter().map(|(p, _)| (*p).to_string()).collect(),
    })
}

pub fn bridge(opts: &Options) -> Result<Outcome> {
    let root = opts
        .root
        .canonicalize()
        .with_context(|| format!("resolving {}", opts.root.display()))?;
    let report = analyze::analyze(&root)?;
    let wanted = opts.unit.trim_start_matches("./");
    let unit = report
        .units
        .iter()
        .find(|u| u.id == wanted || u.id.trim_end_matches('/') == wanted.trim_end_matches('/'))
        .with_context(|| format!("no unit `{wanted}`; `carcinize analyze` lists unit ids"))?;
    if matches!(unit.status, Status::Keep | Status::Blocked) && !opts.force {
        bail!(
            "unit `{}` is {:?}: {}. Pass --force to bridge it anyway.",
            unit.id,
            unit.status,
            unit.notes.first().map_or("", String::as_str)
        );
    }
    let kind = opts.kind.unwrap_or(match unit.bridge {
        Bridge::WasmBindgen => Kind::Wasm,
        _ => Kind::Napi,
    });

    let mut exports = Vec::new();
    let mut classes = Vec::new();
    let mut js_only = Vec::new();
    for file in &unit.files {
        if file.ends_with(".vue") || file.ends_with(".svelte") || file.ends_with(".astro") {
            continue;
        }
        let text =
            std::fs::read_to_string(root.join(file)).with_context(|| format!("reading {file}"))?;
        let types = types_in_scope(&root, file, &text);
        for mut class in exported_classes(file, &text) {
            let kept = keep_reason(&opts.keep, &class.name);
            let constructor = ExportedFn {
                name: format!("{} constructor", class.name),
                file: class.file.clone(),
                line: class.line,
                params: class.constructor.clone(),
                returns: "void".into(),
                is_async: false,
                is_generator: false,
                type_params: Vec::new(),
                uses_instanceof: false,
            };
            let method_reason = std::iter::once(&constructor)
                .chain(&class.methods)
                .find_map(|m| js_only_reason(m, &types).map(|r| format!("`{}`: {r}", m.name)));
            match kept.or_else(|| class.blocker.clone()).or(method_reason) {
                Some(reason) => js_only.push(JsOnly {
                    name: class.name,
                    file: class.file,
                    line: class.line,
                    reason: format!("class: {reason}"),
                }),
                None => {
                    for p in &mut class.constructor {
                        p.ts_type = resolve_aliases(&p.ts_type, &types);
                    }
                    for m in &mut class.methods {
                        for p in &mut m.params {
                            p.ts_type = resolve_aliases(&p.ts_type, &types);
                        }
                        m.returns = resolve_aliases(&m.returns, &types);
                    }
                    classes.push(class);
                }
            }
        }
        for mut f in exported_functions(file, &text) {
            for p in &mut f.params {
                p.ts_type = resolve_aliases(&p.ts_type, &types);
            }
            f.returns = resolve_aliases(&f.returns, &types);
            let kept = keep_reason(&opts.keep, &f.name);
            match kept.or_else(|| js_only_reason(&f, &types)) {
                Some(reason) => js_only.push(JsOnly {
                    name: f.name,
                    file: f.file,
                    line: f.line,
                    reason,
                }),
                None => exports.push(f),
            }
        }
    }
    if exports.is_empty() && classes.is_empty() && !js_only.is_empty() {
        bail!(
            "every export of `{}` must stay in JS: {}",
            unit.id,
            js_only
                .iter()
                .map(|j| format!("{} ({})", j.name, j.reason))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if exports.is_empty() && classes.is_empty() {
        bail!(
            "unit `{}` exports no functions; bridge a unit with an exported function API",
            unit.id
        );
    }

    let stem = unit
        .id
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("unit")
        .split('.')
        .next()
        .unwrap_or("unit");
    let crate_name = opts.name.clone().unwrap_or_else(|| {
        let clean: String = stem
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        format!("{}-rs", clean.trim_matches('-'))
    });
    let crate_dir = opts
        .out
        .clone()
        .unwrap_or_else(|| root.join("rust").join(&crate_name));
    if crate_dir.exists() && std::fs::read_dir(&crate_dir)?.next().is_some() {
        bail!(
            "{} already exists and is not empty; carcinize never overwrites a port in progress",
            crate_dir.display()
        );
    }

    let utf16 = unit.risks.iter().any(|r| r.risk == Risk::StringOffsets);
    let manifest = Manifest {
        schema: SCHEMA.into(),
        root: root.clone(),
        unit: unit.id.clone(),
        kind,
        crate_name: crate_name.clone(),
        exports: exports.clone(),
        classes: classes.clone(),
        js_only: js_only.clone(),
    };
    let port = codegen::port_rs(&manifest);
    let fns = codegen::parse_port(&port);
    let modules = codegen::Modules {
        utf16,
        js: needs_js(&fns),
    };
    let mut files: Vec<(String, String)> = vec![
        ("Cargo.toml".into(), codegen::cargo_toml(&manifest, &fns)),
        (
            "src/lib.rs".into(),
            codegen::lib_rs(&manifest, &fns, &modules),
        ),
        ("src/port.rs".into(), port),
        ("index.cjs".into(), codegen::index_cjs(&manifest, &fns)),
        (
            ".gitignore".into(),
            "target/\n*.node\npkg/\n.carcinize/\n".into(),
        ),
        ("README.md".into(), readme(&manifest)),
        (
            MANIFEST.into(),
            serde_json::to_string_pretty(&manifest)? + "\n",
        ),
    ];
    if kind == Kind::Napi {
        files.push((
            "build.rs".into(),
            "fn main() {\n    napi_build::setup();\n}\n".into(),
        ));
    }
    if utf16 {
        files.push(("src/utf16.rs".into(), UTF16_RS.into()));
    }
    if modules.js {
        files.push(("src/js.rs".into(), codegen::JS_RS.into()));
    }
    for (path, text) in &files {
        let target = crate_dir.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, text).with_context(|| format!("writing {}", target.display()))?;
    }

    format_rust(&crate_dir);
    let mut notes = vec![format!(
        "Port each function in src/port.rs, then: carcinize record {0} && carcinize build {0} && carcinize diff {0}",
        crate_dir.display()
    )];
    if utf16 {
        notes.push("The unit works with string offsets: src/utf16.rs has UTF-16 helpers that match JS indexing.".into());
    }
    for j in &js_only {
        notes.push(format!(
            "`{}` stays in TS: {}. The shadow module re-exports the original.",
            j.name, j.reason
        ));
    }
    if exports.iter().any(codegen::is_async) {
        notes.push(
            "Async TS functions are `async fn` in the port; the bindings return promises.".into(),
        );
    }
    Ok(Outcome {
        crate_dir,
        kind,
        files: files.into_iter().map(|(p, _)| p).collect(),
        exports: exports
            .iter()
            .map(|f| f.name.clone())
            .chain(classes.iter().map(|c| format!("class {}", c.name)))
            .collect(),
        notes,
    })
}

fn readme(m: &Manifest) -> String {
    let functions: String = m
        .exports
        .iter()
        .map(|f| format!("- `{}` (`{}:{}`)\n", f.name, f.file, f.line))
        .collect();
    format!(
        "# {name}\n\nRust port of `{unit}`, bridged with {kind}. Generated by carcinize.\n\n## Functions\n\n{functions}\n\
         ## Loop\n\n```sh\ncarcinize record .   # record what the TS code does\ncarcinize build .    # build the {kind} module\n\
         carcinize diff .     # run the recorded cases against Rust\ncarcinize shadow .   # run both in the app, log mismatches\n\
         carcinize gate .     # fmt, clippy, tests, unsafe, API, diff\n```\n",
        name = m.crate_name,
        unit = m.unit,
        kind = match m.kind {
            Kind::Napi => "napi-rs",
            Kind::Wasm => "wasm-bindgen",
        },
    )
}

const UTF16_RS: &str = r#"//! JS string indexing in Rust.
//!
//! JS strings are indexed in UTF-16 code units; Rust `str` in UTF-8 bytes. A port that returns
//! or consumes offsets must use these helpers, or non-ASCII text gets wrong results silently.

#![forbid(unsafe_code)]

/// The UTF-16 code units of `s`, as JS sees the string.
pub fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// The string `units[start..end]` decodes to, like JS `slice` (lone surrogates become U+FFFD).
pub fn slice(units: &[u16], start: usize, end: usize) -> String {
    let end = end.min(units.len());
    let start = start.min(end);
    String::from_utf16_lossy(&units[start..end])
}

/// JS `haystack.indexOf(needle, from)`, in UTF-16 code units.
pub fn index_of(haystack: &[u16], needle: &[u16], from: usize) -> Option<usize> {
    let from = from.min(haystack.len());
    if needle.is_empty() {
        return Some(from);
    }
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| i + from)
}

/// JS `haystack.lastIndexOf(needle)`, in UTF-16 code units.
pub fn last_index_of(haystack: &[u16], needle: &[u16]) -> Option<usize> {
    if needle.is_empty() {
        return Some(haystack.len());
    }
    haystack.windows(needle.len()).rposition(|w| w == needle)
}

/// UTF-8 byte offset of a UTF-16 offset in `s`, for slicing a Rust `str`.
pub fn byte_offset(s: &str, utf16_offset: usize) -> usize {
    let mut seen = 0;
    for (byte, c) in s.char_indices() {
        if seen >= utf16_offset {
            return byte;
        }
        seen += c.len_utf16();
    }
    s.len()
}

/// UTF-16 offset of a UTF-8 byte offset in `s`, for returning ranges to JS.
pub fn utf16_offset(s: &str, byte_offset: usize) -> usize {
    s[..byte_offset.min(s.len())].encode_utf16().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_js_indexing() {
        let s = units("a👍b");
        assert_eq!(s.len(), 4);
        assert_eq!(index_of(&s, &units("b"), 0), Some(3));
        assert_eq!(last_index_of(&s, &units("a")), Some(0));
        assert_eq!(slice(&s, 3, 4), "b");
        assert_eq!(byte_offset("a👍b", 3), 5);
        assert_eq!(utf16_offset("a👍b", 5), 3);
    }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_ts_types() {
        assert_eq!(RType::from_ts("string").rust(), "String");
        assert_eq!(RType::from_ts("readonly string[]").rust(), "Vec<String>");
        assert_eq!(RType::from_ts("Array<number>").rust(), "Vec<f64>");
        assert_eq!(
            RType::from_ts("string | undefined").rust(),
            "Option<String>"
        );
        assert_eq!(RType::from_ts("'a' | 'b'").rust(), "String");
        assert_eq!(RType::from_ts("Promise<boolean>").rust(), "bool");
        assert_eq!(
            RType::from_ts("Map<string, number>").rust(),
            "serde_json::Value"
        );
        assert_eq!(
            RType::from_ts("string | number").rust(),
            "serde_json::Value"
        );
        assert_eq!(
            RType::from_ts("Record<string, string | number>").rust(),
            "serde_json::Value"
        );
        assert_eq!(
            RType::from_ts("(string | undefined)").rust(),
            "Option<String>"
        );
        assert_eq!(RType::from_ts("(a) | (b)").rust(), "serde_json::Value");
    }

    #[test]
    fn keeps_unbridgeable_exports_in_js() {
        let source = "export interface LineIndex {\n  readonly lineCount: number\n  position(offset: number): number\n}\n\
            export interface Range { start: number, end: number }\n\
            export function keyMatcher(globs: readonly string[]): (key: string) => boolean { return () => true }\n\
            export function globToRegExp(glob: string): RegExp { return /x/ }\n\
            export function createLineIndex(text: string): LineIndex { return null! }\n\
            export function* walk(tree: object) {}\n\
            export function sortTree(tree: object, compare: (a: string, b: string) => number) {}\n\
            export function contains(range: Range, offset: number): boolean { return true }\n";
        let types = local_types("a.ts", source);
        let reasons: Vec<(String, Option<String>)> = exported_functions("a.ts", source)
            .iter()
            .map(|f| (f.name.clone(), js_only_reason(f, &types)))
            .collect();
        let js_only: Vec<&str> = reasons
            .iter()
            .filter(|(_, r)| r.is_some())
            .map(|(n, _)| n.as_str())
            .collect();
        assert_eq!(
            js_only,
            [
                "keyMatcher",
                "globToRegExp",
                "createLineIndex",
                "walk",
                "sortTree"
            ]
        );
    }

    #[test]
    fn resolves_simple_aliases() {
        let types = local_types(
            "m.ts",
            "export type KeyString = string\nexport type Leaf = string | null\nexport type Map2<T> = T\n",
        );
        assert_eq!(
            resolve_aliases("readonly KeyString[]", &types),
            "readonly string[]"
        );
        assert_eq!(resolve_aliases("Leaf", &types), "(string | null)");
        assert_eq!(resolve_aliases("Map2<KeyString>", &types), "Map2<string>");
    }

    #[test]
    fn makes_rust_names() {
        assert_eq!(snake_case("fuzzyScore"), "fuzzy_score");
        assert_eq!(snake_case("type"), "r#type");
        assert_eq!(snake_case("toURL"), "to_url");
        assert_eq!(snake_case("parseHTMLString"), "parse_html_string");
    }
}
