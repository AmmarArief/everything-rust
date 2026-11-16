//! Step 2: scaffold a Rust crate for one unit, with JS bindings and a manifest the later
//! steps read.
//!
//! The crate splits bindings from logic: `src/lib.rs` holds thin `#[napi]` or
//! `#[wasm_bindgen]` wrappers, `src/port.rs` holds the port in plain Rust with
//! `#![forbid(unsafe_code)]`, so it can be tested without a JS runtime.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::analyze::{self, Bridge, Status};
use crate::risks::Risk;
use crate::ts::exports::{ExportedFn, exported_functions};

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
    pub exports: Vec<ExportedFn>,
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
    for file in &unit.files {
        if file.ends_with(".vue") || file.ends_with(".svelte") || file.ends_with(".astro") {
            continue;
        }
        let text =
            std::fs::read_to_string(root.join(file)).with_context(|| format!("reading {file}"))?;
        exports.extend(exported_functions(file, &text));
    }
    if exports.is_empty() {
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
    };
    let json = exports.iter().any(|f| {
        f.params.iter().any(|p| param_type(p).contains_json())
            || RType::from_ts(&f.returns).contains_json()
    });

    let mut files: Vec<(String, String)> = vec![
        ("Cargo.toml".into(), cargo_toml(&manifest, json)),
        ("src/lib.rs".into(), lib_rs(&manifest, utf16)),
        ("src/port.rs".into(), port_rs(&manifest)),
        ("index.cjs".into(), index_cjs(&manifest)),
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
    for (path, text) in &files {
        let target = crate_dir.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, text).with_context(|| format!("writing {}", target.display()))?;
    }

    let mut notes = vec![format!(
        "Port each function in src/port.rs, then: carcinize record {0} && carcinize build {0} && carcinize diff {0}",
        crate_dir.display()
    )];
    if utf16 {
        notes.push("The unit works with string offsets: src/utf16.rs has UTF-16 helpers that match JS indexing.".into());
    }
    if exports.iter().any(|f| f.is_async) {
        notes.push("Some functions are async in TS; the port is synchronous and the bridge returns plain values.".into());
    }
    Ok(Outcome {
        crate_dir,
        kind,
        files: files.into_iter().map(|(p, _)| p).collect(),
        exports: exports.iter().map(|f| f.name.clone()).collect(),
        notes,
    })
}

fn cargo_toml(m: &Manifest, json: bool) -> String {
    let mut deps = String::new();
    match m.kind {
        Kind::Napi => {
            let features = if json {
                r#", features = ["serde-json"]"#
            } else {
                ""
            };
            let _ = writeln!(deps, "napi = {{ version = \"3\"{features} }}");
            let _ = writeln!(deps, "napi-derive = \"3\"");
        }
        Kind::Wasm => {
            let _ = writeln!(deps, "wasm-bindgen = \"0.2\"");
            if json {
                let _ = writeln!(deps, "serde-wasm-bindgen = \"0.6\"");
            }
        }
    }
    if json {
        let _ = writeln!(deps, "serde_json = \"1\"");
    }
    let build = if m.kind == Kind::Napi {
        "\n[build-dependencies]\nnapi-build = \"2\"\n"
    } else {
        ""
    };
    format!(
        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\ndescription = \"Rust port of `{}`, generated by carcinize\"\n\n\
         [lib]\ncrate-type = [\"cdylib\", \"rlib\"]\n\n[dependencies]\n{deps}{build}\n\
         [profile.release]\nlto = true\n\n[lints.clippy]\nall = {{ level = \"warn\", priority = -1 }}\n",
        m.crate_name, m.unit
    )
}

fn signature(f: &ExportedFn) -> (String, String) {
    let params = f
        .params
        .iter()
        .map(|p| format!("{}: {}", snake_case(&p.name), param_type(p).rust()))
        .collect::<Vec<_>>()
        .join(", ");
    let ret = match RType::from_ts(&f.returns) {
        RType::Unit => String::new(),
        t => format!(" -> {}", t.rust()),
    };
    (params, ret)
}

fn lib_rs(m: &Manifest, utf16: bool) -> String {
    let mut out = format!(
        "//! JS bindings for `{}`: thin wrappers over [`port`]. Generated by carcinize; edit freely.\n\n",
        m.unit
    );
    if m.kind == Kind::Wasm {
        out.push_str("use wasm_bindgen::prelude::*;\n\n");
    } else {
        out.push_str("use napi_derive::napi;\n\n");
    }
    out.push_str("pub mod port;\n");
    if utf16 {
        out.push_str("pub mod utf16;\n");
    }
    if m.kind == Kind::Napi {
        out.push_str(NAPI_CATCH_PANIC);
    }
    for f in &m.exports {
        let rust_name = snake_case(&f.name);
        let ret_type = RType::from_ts(&f.returns);
        let args: Vec<String> = f.params.iter().map(|p| snake_case(&p.name)).collect();
        out.push('\n');
        let _ = writeln!(out, "/// `{}` from `{}:{}`.", f.name, f.file, f.line);
        match m.kind {
            Kind::Napi => {
                let (params, _) = signature(f);
                let ret = match &ret_type {
                    RType::Unit => "()".to_string(),
                    t => t.rust(),
                };
                let _ = writeln!(out, "#[napi(js_name = \"{}\")]", f.name);
                let _ = writeln!(
                    out,
                    "pub fn {rust_name}({params}) -> napi::Result<{ret}> {{"
                );
                let _ = writeln!(
                    out,
                    "    catch_panic(std::panic::AssertUnwindSafe(|| port::{rust_name}({})))",
                    args.join(", ")
                );
                out.push_str("}\n");
            }
            Kind::Wasm => {
                // wasm-bindgen cannot take serde_json::Value: those cross as JsValue.
                let params = f
                    .params
                    .iter()
                    .map(|p| {
                        let t = param_type(p);
                        let ty = if t.contains_json() {
                            "JsValue".to_string()
                        } else {
                            t.rust()
                        };
                        format!("{}: {ty}", snake_case(&p.name))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let json_ret = ret_type.contains_json();
                let any_json = json_ret || f.params.iter().any(|p| param_type(p).contains_json());
                let ret = match (&ret_type, any_json) {
                    (RType::Unit, false) => String::new(),
                    (RType::Unit, true) => " -> Result<(), JsError>".into(),
                    (_, false) => format!(" -> {}", ret_type.rust()),
                    (_, true) if json_ret => " -> Result<JsValue, JsError>".into(),
                    (t, true) => format!(" -> Result<{}, JsError>", t.rust()),
                };
                let _ = writeln!(out, "#[wasm_bindgen(js_name = \"{}\")]", f.name);
                let _ = writeln!(out, "pub fn {rust_name}({params}){ret} {{");
                for p in &f.params {
                    if param_type(p).contains_json() {
                        let n = snake_case(&p.name);
                        let _ =
                            writeln!(out, "    let {n} = serde_wasm_bindgen::from_value({n})?;");
                    }
                }
                let call = format!("port::{rust_name}({})", args.join(", "));
                if json_ret {
                    let _ = writeln!(out, "    Ok(serde_wasm_bindgen::to_value(&{call})?)");
                } else if any_json {
                    let _ = writeln!(out, "    Ok({call})");
                } else {
                    let _ = writeln!(out, "    {call}");
                }
                out.push_str("}\n");
            }
        }
    }
    out
}

fn port_rs(m: &Manifest) -> String {
    let mut out = format!(
        "//! The Rust port of `{}`.\n//!\n//! Replace each `unimplemented!` with the port of the TS function. Keep the signatures in sync\n\
         //! with `lib.rs`. `carcinize diff` checks the result against recorded TS behavior.\n\n\
         #![forbid(unsafe_code)]\n",
        m.unit
    );
    for f in &m.exports {
        let (params, ret) = signature(f);
        let names: Vec<String> = f.params.iter().map(|p| snake_case(&p.name)).collect();
        out.push('\n');
        let _ = writeln!(out, "/// Port of `{}` (`{}:{}`).", f.name, f.file, f.line);
        let _ = writeln!(out, "pub fn {}({params}){ret} {{", snake_case(&f.name));
        match names.len() {
            0 => {}
            1 => {
                let _ = writeln!(out, "    let _ = &{};", names[0]);
            }
            _ => {
                let refs: Vec<String> = names.iter().map(|n| format!("&{n}")).collect();
                let _ = writeln!(out, "    let _ = ({});", refs.join(", "));
            }
        }
        let _ = writeln!(
            out,
            "    unimplemented!(\"port `{}` from {}\")",
            f.name, f.file
        );
        out.push_str("}\n");
    }
    out
}

fn index_cjs(m: &Manifest) -> String {
    match m.kind {
        Kind::Napi => format!(
            "// Loads the native module built by `carcinize build`.\nmodule.exports = require('./{}.node')\n",
            m.lib_name()
        ),
        Kind::Wasm => format!(
            "// Loads the wasm-bindgen (nodejs target) output built by `carcinize build`.\nmodule.exports = require('./pkg/{}.js')\n",
            m.lib_name()
        ),
    }
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

/// A panic must not unwind into Node: it would abort the process, and in shadow mode take
/// the app down. Panics become JS errors instead.
const NAPI_CATCH_PANIC: &str = r#"
fn catch_panic<T>(f: impl FnOnce() -> T + std::panic::UnwindSafe) -> napi::Result<T> {
    std::panic::catch_unwind(f).map_err(|payload| {
        let message = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".into());
        napi::Error::from_reason(format!("Rust panic: {message}"))
    })
}
"#;

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
    }

    #[test]
    fn makes_rust_names() {
        assert_eq!(snake_case("fuzzyScore"), "fuzzy_score");
        assert_eq!(snake_case("type"), "r#type");
        assert_eq!(snake_case("toURL"), "to_url");
        assert_eq!(snake_case("parseHTMLString"), "parse_html_string");
    }
}
