//! Batch classes: a function run over many items that stay on the Rust side.
//!
//! `bridge --batch fuzzyScore:text` gives the crate a `FuzzyScoreBatch` class: the constructor
//! takes the items (the `text` values) once; `run(query)` applies the port to every item on all
//! cores and returns the results as one typed array; `rank(query, min)` returns the indexes of
//! the items whose result is at least `min`, best first. One call does what a loop of per-item
//! calls did, so the crossing is paid once instead of per item.
//!
//! The default index calls the port per item. A port can do better by defining
//! `pub struct FuzzyScoreIndex` in `port.rs` with `pub fn new(items: Vec<T>) -> Self` and
//! `pub fn run(&self, <other params>) -> Result<Vec<R>, String>`: it can prepare the items once
//! (lowercase them, build masks) and the class uses it instead.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::bridge::{Kind, Manifest, snake_case};
use crate::codegen::{PortApi, PortFn};

/// `--batch function:param`: run `function` over many values of `param`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Batch {
    pub function: String,
    pub param: String,
}

impl Batch {
    pub fn parse(spec: &str) -> Option<Self> {
        let (function, param) = spec.split_once(':')?;
        Some(Self {
            function: function.trim().into(),
            param: param.trim().into(),
        })
    }
}

pub fn pascal(name: &str) -> String {
    let mut out = String::new();
    let mut up = true;
    for c in name.chars() {
        if c == '_' || c == '-' {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// A batch the port's signature allows.
struct Spec<'a> {
    f: &'a PortFn,
    item: usize,
    /// The stored item type (`&str` items are stored as `String`).
    item_ty: String,
    class: String,
    index: String,
    custom: bool,
}

const ITEMS: &[&str] = &["String", "&str", "f64", "i64", "bool"];
/// Whether a parameter type is `Copy`, so each item's call takes it as is; the rest are cloned.
fn is_copy(ty: &str) -> bool {
    let ty = ty.trim();
    if let Some(inner) = ty.strip_prefix("Option<").and_then(|t| t.strip_suffix('>')) {
        return is_copy(inner);
    }
    if let Some(items) = ty.strip_prefix('(').and_then(|t| t.strip_suffix(')')) {
        return items
            .split(',')
            .filter(|t| !t.trim().is_empty())
            .all(is_copy);
    }
    matches!(
        ty,
        "f64"
            | "f32"
            | "i64"
            | "i32"
            | "i16"
            | "i8"
            | "u64"
            | "u32"
            | "u16"
            | "u8"
            | "usize"
            | "isize"
            | "bool"
            | "char"
    )
}

const RESULTS: &[&str] = &["f64", "i64", "bool", "String"];

/// Why a batch cannot be generated, or its spec.
fn spec<'a>(b: &Batch, api: &'a PortApi) -> Result<Spec<'a>, String> {
    let f = api
        .functions
        .iter()
        .find(|f| f.js_name == b.function)
        .ok_or_else(|| format!("`{}` is not a function of src/port.rs", b.function))?;
    let wanted = snake_case(&b.param);
    let item = f
        .params
        .iter()
        .position(|(n, _)| *n == wanted || *n == b.param)
        .ok_or_else(|| format!("`{}` has no parameter `{}`", b.function, b.param))?;
    let ty = f.params[item].1.trim();
    if !ITEMS.contains(&ty) {
        return Err(format!(
            "batch items must be strings, numbers or booleans; `{}` is `{ty}`",
            b.param
        ));
    }
    if f.is_async {
        return Err(format!("`{}` is async", b.function));
    }
    if !RESULTS.contains(&f.ret.trim()) {
        return Err(format!(
            "batch results must be numbers, booleans or strings; `{}` returns `{}`",
            b.function, f.ret
        ));
    }
    let base = pascal(&f.js_name);
    let custom = api.structs.iter().any(|s| *s == format!("{base}Index"));
    Ok(Spec {
        f,
        item,
        item_ty: if ty == "&str" {
            "String".into()
        } else {
            ty.into()
        },
        class: format!("{base}Batch"),
        index: if custom {
            format!("port::{base}Index")
        } else {
            format!("{base}DefaultIndex")
        },
        custom,
    })
}

/// The non-item parameters, as `name: Type` (by value: `&str` becomes `String`).
fn fixed(s: &Spec) -> Vec<(String, String)> {
    s.f.params
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != s.item)
        .map(|(_, (n, t))| {
            (
                n.clone(),
                if t.trim() == "&str" {
                    "String".to_string()
                } else {
                    t.clone()
                },
            )
        })
        .collect()
}

/// The default index: the items as the port takes them, the port called per item.
fn default_index(s: &Spec, parallel: bool) -> String {
    let fixed = fixed(s);
    let params: Vec<String> = fixed.iter().map(|(n, t)| format!("{n}: {t}")).collect();
    let args: Vec<String> =
        s.f.params
            .iter()
            .enumerate()
            .map(|(i, (n, t))| {
                let by_ref = t.trim() == "&str";
                match (i == s.item, by_ref) {
                    (true, true) => "item.as_str()".to_string(),
                    (true, false) if s.item_ty == "String" => "item.clone()".into(),
                    (true, false) => "*item".into(),
                    (false, true) => format!("{n}.as_str()"),
                    (false, false) if is_copy(t) => n.clone(),
                    (false, false) => format!("{n}.clone()"),
                }
            })
            .collect();
    let call = format!("port::{}({})", s.f.rust_name, args.join(", "));
    let call = if s.f.fallible {
        call
    } else {
        format!("Ok::<_, String>({call})")
    };
    let iter = if parallel { "par_iter" } else { "iter" };
    format!(
        "\n/// The items of `{class}` as the port takes them; the port runs per item. Define\n\
         /// `pub struct {base}Index` in `port.rs` (with `new` and `run`) to prepare items once.\n\
         pub struct {index} {{\n    items: Vec<{item}>,\n}}\n\n\
         impl {index} {{\n    pub fn new(items: Vec<{item}>) -> Self {{\n        Self {{ items }}\n    }}\n\n    \
         pub fn run(&self, {params}) -> Result<Vec<{ret}>, String> {{\n        \
         {use_rayon}self.items.{iter}().map(|item| {call}).collect::<Vec<_>>().into_iter().collect()\n    }}\n}}\n",
        class = s.class,
        base = s.class.trim_end_matches("Batch"),
        index = s.index,
        item = s.item_ty,
        params = params.join(", "),
        ret = s.f.ret,
        use_rayon = if parallel {
            "use rayon::prelude::*;\n        "
        } else {
            ""
        },
    )
}

/// How each language hands back a `Vec<R>`.
fn napi_out(ret: &str) -> (&'static str, &'static str) {
    match ret {
        "f64" => (
            "napi::bindgen_prelude::Float64Array",
            "napi::bindgen_prelude::Float64Array::new(out)",
        ),
        "i64" => (
            "napi::bindgen_prelude::Float64Array",
            "napi::bindgen_prelude::Float64Array::new(out.into_iter().map(|x| x as f64).collect())",
        ),
        "bool" => (
            "napi::bindgen_prelude::Uint8Array",
            "napi::bindgen_prelude::Uint8Array::new(out.into_iter().map(u8::from).collect())",
        ),
        _ => ("Vec<String>", "out"),
    }
}

fn rank_body(ret: &str) -> Option<String> {
    let value = match ret {
        "f64" => "r",
        "i64" => "r as f64",
        _ => return None,
    };
    Some(format!(
        "let mut hits: Vec<(f64, u32)> = out\n            .into_iter()\n            .enumerate()\n            \
         .map(|(i, r)| ({value}, u32::try_from(i).unwrap_or(u32::MAX)))\n            .filter(|(r, _)| *r >= min)\n            .collect();\n        \
         // Stable: equal results keep the items' order, as JS `sort` and Python `sorted` do.\n        \
         hits.sort_by(|a, b| b.0.total_cmp(&a.0));\n        let order: Vec<u32> = hits.into_iter().map(|(_, i)| i).collect();"
    ))
}

/// Batch classes for a napi-rs or wasm-bindgen `lib.rs`.
pub fn js_rs(m: &Manifest, api: &PortApi) -> Result<String, String> {
    let mut out = String::new();
    let parallel = m.kind == Kind::Napi;
    for b in &m.batches {
        let s = spec(b, api)?;
        if !s.custom {
            out.push_str(&default_index(&s, parallel));
        }
        let fixed = fixed(&s);
        let params: Vec<String> = fixed.iter().map(|(n, t)| format!("{n}: {t}")).collect();
        let names: Vec<&str> = fixed.iter().map(|(n, _)| n.as_str()).collect();
        let ret = s.f.ret.trim();
        let (out_ty, out_expr) = napi_out(ret);
        let wasm = m.kind == Kind::Wasm;
        let (attr, ctor_attr, err, res) = if wasm {
            (
                "wasm_bindgen",
                "#[wasm_bindgen(constructor)]",
                ".map_err(|e| JsError::new(&e))",
                "Result<{T}, JsError>",
            )
        } else {
            ("napi", "#[napi(constructor)]", "", "napi::Result<{T}>")
        };
        let call = |body: &str| -> String {
            if wasm {
                format!(
                    "let out = self.inner.run({}){err}?;\n        {body}",
                    names.join(", ")
                )
            } else {
                format!(
                    "let out = run(|| self.inner.run({}))?;\n        {body}",
                    names.join(", ")
                )
            }
        };
        let out_ty = if wasm {
            out_ty
                .replace("napi::bindgen_prelude::", "Vec<")
                .replace("Float64Array", "f64>")
                .replace("Uint8Array", "u8>")
        } else {
            out_ty.to_string()
        };
        let out_expr = if wasm {
            match ret {
                "i64" => "out.into_iter().map(|x| x as f64).collect()".to_string(),
                "bool" => "out.into_iter().map(u8::from).collect()".to_string(),
                _ => "out".to_string(),
            }
        } else {
            out_expr.to_string()
        };
        let _ = write!(
            out,
            "\n/// `{js}` over many `{param}` values kept in Rust: `run` maps them all in one call{cores};\n\
             /// `rank` returns the indexes of results >= `min`, best first.\n\
             #[{attr}(js_name = \"{class}\")]\npub struct {class} {{\n    inner: {index},\n}}\n\n\
             {impl_attr}\nimpl {class} {{\n    {ctor_attr}\n    pub fn new(items: Vec<{item}>) -> Self {{\n        Self {{ inner: {index}::new(items) }}\n    }}\n\n    \
             /// The result for every item, in order.\n    #[{attr}]\n    pub fn run(&self, {params}) -> {res_run} {{\n        {run_body}\n    }}\n",
            impl_attr = if wasm {
                format!("#[wasm_bindgen(js_class = \"{}\")]", s.class)
            } else {
                "#[napi]".to_string()
            },
            js = s.f.js_name,
            param = b.param,
            cores = if parallel { ", on all cores" } else { "" },
            class = s.class,
            index = s.index,
            item = s.item_ty,
            params = params.join(", "),
            res_run = res.replace("{T}", &out_ty),
            run_body = call(&format!("Ok({out_expr})")),
        );
        if let Some(rank) = rank_body(ret) {
            let order_ty = if wasm {
                "Vec<u32>"
            } else {
                "napi::bindgen_prelude::Uint32Array"
            };
            let order_expr = if wasm {
                "order"
            } else {
                "napi::bindgen_prelude::Uint32Array::new(order)"
            };
            let sep = if params.is_empty() { "" } else { ", " };
            let _ = write!(
                out,
                "\n    /// Indexes of the items whose result is at least `min`, highest result first.\n    #[{attr}]\n    \
                 pub fn rank(&self, {}{sep}min: f64) -> {} {{\n        {}\n    }}\n",
                params.join(", "),
                res.replace("{T}", order_ty),
                call(&format!("{rank}\n        Ok({order_expr})"))
            );
        }
        out.push_str("}\n");
    }
    Ok(out)
}

/// Batch classes for a PyO3 `lib.rs`, and the names to register.
pub fn py_rs(m: &Manifest, api: &PortApi) -> Result<(String, Vec<String>), String> {
    let mut out = String::new();
    let mut classes = Vec::new();
    for b in &m.batches {
        let s = spec(b, api)?;
        if !s.custom {
            out.push_str(&default_index(&s, true));
        }
        let fixed = fixed(&s);
        let params: Vec<String> = fixed.iter().map(|(n, t)| format!("{n}: {t}")).collect();
        let names: Vec<&str> = fixed.iter().map(|(n, _)| n.as_str()).collect();
        let ret = s.f.ret.trim();
        let sep = if params.is_empty() { "" } else { ", " };
        let _ = write!(
            out,
            "\n/// `{js}` over many `{param}` values kept in Rust: `run` maps them all in one call, on all\n\
             /// cores, without the GIL; `rank` returns the indexes of results >= `min`, best first.\n\
             #[pyclass(name = \"{class}\")]\npub struct {class} {{\n    inner: {index},\n}}\n\n#[pymethods]\nimpl {class} {{\n    \
             #[new]\n    fn new(items: Vec<{item}>) -> Self {{\n        Self {{ inner: {index}::new(items) }}\n    }}\n\n    \
             /// The result for every item, in order.\n    fn run(&self, py: Python<'_>{sep}{params}) -> PyResult<Vec<{ret}>> {{\n        \
             py.detach(|| self.inner.run({names})).map_err(error)\n    }}\n",
            js = s.f.js_name,
            param = b.param,
            class = s.class,
            index = s.index,
            item = s.item_ty,
            params = params.join(", "),
            names = names.join(", "),
        );
        if let Some(rank) = rank_body(ret) {
            let _ = write!(
                out,
                "\n    /// Indexes of the items whose result is at least `min`, highest result first.\n    \
                 fn rank(&self, py: Python<'_>{sep}{params}, min: f64) -> PyResult<Vec<u32>> {{\n        \
                 let out = py.detach(|| self.inner.run({names})).map_err(error)?;\n        {rank}\n        Ok(order)\n    }}\n",
                params = params.join(", "),
                names = names.join(", "),
            );
        }
        out.push_str("}\n");
        classes.push(s.class);
    }
    Ok((out, classes))
}

/// Whether the crate depends on rayon: every crate with batches. Wasm gets it too, so a
/// `port.rs` index using it builds for both kinds: without threads, rayon runs on the calling
/// thread.
pub fn needs_rayon(m: &Manifest) -> bool {
    !m.batches.is_empty() && matches!(m.kind, Kind::Napi | Kind::Wasm | Kind::Pyo3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_only_copy_types() {
        for t in ["f64", "Option<i64>", "(f64, bool)", "Option<(u32, char)>"] {
            assert!(is_copy(t), "{t}");
        }
        for t in [
            "String",
            "Option<String>",
            "Vec<f64>",
            "(f64, String)",
            "serde_json::Value",
        ] {
            assert!(!is_copy(t), "{t}");
        }
    }

    #[test]
    fn parses_and_names() {
        let b = Batch::parse("fuzzyScore:text").unwrap();
        assert_eq!(
            b,
            Batch {
                function: "fuzzyScore".into(),
                param: "text".into()
            }
        );
        assert_eq!(pascal("fuzzyScore"), "FuzzyScore");
        assert_eq!(pascal("rgb_to_hsv"), "RgbToHsv");
    }
}
