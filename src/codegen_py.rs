//! PyO3 bindings and the Python wrapper package for a bridge crate.
//!
//! Like the JS side, the binding layer follows `port.rs`: strings, ints, floats, bools and
//! lists or options of those cross natively; anything structured crosses as JSON text, which
//! the wrapper package (`python/<lib>/__init__.py`) encodes and decodes with Python markers.

use std::fmt::Write as _;

use crate::bridge::Manifest;
use crate::codegen::{Modules, PortApi, PortFn, Wire, wire};
use crate::ts::exports::Param;

/// A Python signature for `#[pyo3(signature = ...)]`: optional parameters default to `None`,
/// keyword-only ones follow `*`, the rest parameter is `*name`.
fn signature(f: &PortFn, source: &[Param]) -> String {
    let mut parts = Vec::new();
    let mut keyword_only = false;
    for (name, _) in &f.params {
        let param = source
            .iter()
            .find(|p| crate::bridge::snake_case(&p.name) == *name);
        if param.is_some_and(|p| p.keyword_only) && !keyword_only {
            keyword_only = true;
            parts.push("*".to_string());
        }
        let py_name = name.trim_start_matches("r#");
        match param {
            Some(p) if p.optional => match p.default.as_deref().and_then(rust_literal) {
                Some(literal) => parts.push(format!("{py_name}={literal}")),
                None => parts.push(format!("{py_name}=None")),
            },
            _ => parts.push(py_name.to_string()),
        }
    }
    parts.join(", ")
}

/// A Python literal default as a Rust expression, for `#[pyo3(signature = ...)]`: strings,
/// numbers and booleans. `None` and other expressions return `None` (the parameter is an
/// `Option`, and `None` stands for the default).
pub fn rust_literal(default: &str) -> Option<String> {
    let d = default.trim();
    match d {
        // Python's `True`; Ruby's and Java's `true`.
        "True" | "true" => return Some("true".into()),
        "False" | "false" => return Some("false".into()),
        "None" => return None,
        _ => {}
    }
    let digits = d.replace('_', "");
    if digits.parse::<i64>().is_ok() {
        return Some(digits);
    }
    if (digits.contains('.') || digits.contains('e')) && digits.parse::<f64>().is_ok() {
        return Some(if digits.contains('.') {
            digits
        } else {
            format!("{digits}_f64")
        });
    }
    let quote = d.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let inner = d.strip_prefix(quote)?.strip_suffix(quote)?;
    // Plain literals only: escapes and quotes inside would need Python's own parser.
    if inner.contains('\\') || inner.contains('"') || inner.contains('\'') {
        return None;
    }
    Some(format!("String::from(\"{inner}\")"))
}

/// Whether a parameter keeps Python's literal default (and is a plain, non-`Option` type).
pub fn has_literal_default(p: &Param) -> bool {
    p.optional && p.default.as_deref().and_then(rust_literal).is_some()
}

fn params_of<'m>(m: &'m Manifest, f: &PortFn, class: Option<&str>) -> &'m [Param] {
    match class {
        Some(c) => m
            .classes
            .iter()
            .find(|k| k.name == c)
            .map(|k| {
                if f.rust_name == "new" {
                    k.constructor.as_slice()
                } else {
                    k.methods
                        .iter()
                        .find(|x| x.name == f.js_name)
                        .map_or(&[][..], |x| x.params.as_slice())
                }
            })
            .unwrap_or(&[]),
        None => m
            .exports
            .iter()
            .find(|e| e.name == f.js_name)
            .map_or(&[][..], |e| e.params.as_slice()),
    }
}

fn binding_type(ty: &str) -> String {
    match wire(ty) {
        Wire::Json => "String".to_string(),
        _ => ty.to_string(),
    }
}

fn body(f: &PortFn, target: &str, constructor: bool) -> String {
    let mut body = String::new();
    for (n, t) in &f.params {
        if wire(t) == Wire::Json {
            let _ = writeln!(body, "    let {n} = crate::js::from_json(&{n})?;");
        }
    }
    let args: Vec<&str> = f.params.iter().map(|(n, _)| n.as_str()).collect();
    let call = format!(
        "{target}({}){}",
        args.join(", "),
        if f.fallible { "?" } else { "" }
    );
    if !constructor && f.ret.trim() == "()" {
        let _ = writeln!(body, "    {call};\n    Ok(())");
        return body;
    }
    let _ = writeln!(body, "    let out = {call};");
    let _ = writeln!(
        body,
        "    {}",
        if !constructor && wire(&f.ret) == Wire::Json {
            "crate::js::to_json(&out)"
        } else {
            "Ok(out)"
        }
    );
    body
}

fn ret(f: &PortFn) -> String {
    match wire(&f.ret) {
        Wire::Json => "String".into(),
        _ => f.ret.clone(),
    }
}

/// A port's `Err("KeyError: missing")` raises that built-in exception type with the message;
/// without a known type prefix, `ValueError`. Panics become `PanicException`.
const PY_ERROR: &str = r#"
fn error(message: String) -> PyErr {
    use pyo3::exceptions as e;
    let (kind, text) = message.split_once(": ").unwrap_or(("", message.as_str()));
    let text = text.to_string();
    match kind {
        "TypeError" => e::PyTypeError::new_err(text),
        "KeyError" => e::PyKeyError::new_err(text),
        "IndexError" => e::PyIndexError::new_err(text),
        "OverflowError" => e::PyOverflowError::new_err(text),
        "ZeroDivisionError" => e::PyZeroDivisionError::new_err(text),
        "AttributeError" => e::PyAttributeError::new_err(text),
        "RuntimeError" => e::PyRuntimeError::new_err(text),
        "NotImplementedError" => e::PyNotImplementedError::new_err(text),
        "LookupError" => e::PyLookupError::new_err(text),
        "ArithmeticError" => e::PyArithmeticError::new_err(text),
        "UnicodeError" => e::PyUnicodeError::new_err(text),
        "ValueError" => e::PyValueError::new_err(text),
        _ => e::PyValueError::new_err(message),
    }
}
"#;

pub fn lib_rs(m: &Manifest, api: &PortApi, modules: &Modules) -> String {
    let mut out = format!(
        "//! Python bindings for `{}`: generated from the signatures in `port.rs` by carcinize.\n\
         //! Regenerate with `carcinize refresh` after changing them.\n\n\
         use pyo3::prelude::*;\n\n",
        m.unit
    );
    if modules.js {
        out.push_str("pub mod js;\n");
    }
    out.push_str("pub mod port;\n");
    if modules.utf16 {
        out.push_str("pub mod utf16;\n");
    }
    out.push_str(PY_ERROR);
    let mut registrations = Vec::new();
    for f in &api.functions {
        let params: Vec<String> = f
            .params
            .iter()
            .map(|(n, t)| format!("{n}: {}", binding_type(t)))
            .collect();
        let inner = format!("{}_inner", f.rust_name.trim_start_matches("r#"));
        let args: Vec<&str> = f.params.iter().map(|(n, _)| n.as_str()).collect();
        let _ = writeln!(
            out,
            "\nfn {inner}({}) -> Result<{}, String> {{\n{}}}\n",
            params.join(", "),
            ret(f),
            body(f, &format!("port::{}", f.rust_name), false)
        );
        let _ = writeln!(out, "/// `{}`.", f.js_name);
        let _ = writeln!(out, "#[pyfunction]");
        let _ = writeln!(
            out,
            "#[pyo3(name = \"{}\", signature = ({}))]",
            f.js_name,
            signature(f, params_of(m, f, None))
        );
        let _ = writeln!(
            out,
            "fn {}({}) -> PyResult<{}> {{",
            f.rust_name,
            params.join(", "),
            ret(f)
        );
        let _ = writeln!(out, "    {inner}({}).map_err(error)\n}}", args.join(", "));
        registrations.push(format!(
            "    m.add_function(wrap_pyfunction!({}, m)?)?;",
            f.rust_name
        ));
    }
    for c in &api.classes {
        let (name, js) = (&c.rust_name, &c.js_name);
        let snake = crate::bridge::snake_case(name);
        let ctor_params: Vec<String> = c
            .constructor
            .params
            .iter()
            .map(|(n, t)| format!("{n}: {}", binding_type(t)))
            .collect();
        let ctor_args: Vec<&str> = c
            .constructor
            .params
            .iter()
            .map(|(n, _)| n.as_str())
            .collect();
        let _ = writeln!(
            out,
            "\nfn {snake}_new_inner({}) -> Result<port::{name}, String> {{\n{}}}",
            ctor_params.join(", "),
            body(&c.constructor, &format!("port::{name}::new"), true)
        );
        for (method, mutable) in &c.methods {
            let receiver = if *mutable {
                format!("inner: &mut port::{name}")
            } else {
                format!("inner: &port::{name}")
            };
            let params: Vec<String> = std::iter::once(receiver)
                .chain(
                    method
                        .params
                        .iter()
                        .map(|(n, t)| format!("{n}: {}", binding_type(t))),
                )
                .collect();
            let _ = writeln!(
                out,
                "\nfn {snake}_{}_inner({}) -> Result<{}, String> {{\n{}}}",
                method.rust_name.trim_start_matches("r#"),
                params.join(", "),
                ret(method),
                body(method, &format!("inner.{}", method.rust_name), false)
            );
        }
        let _ = writeln!(
            out,
            "\n/// Class `{js}`.\n#[pyclass(name = \"{js}\")]\npub struct {name} {{\n    inner: port::{name},\n}}\n"
        );
        let _ = writeln!(out, "#[pymethods]\nimpl {name} {{");
        let _ = writeln!(
            out,
            "    #[new]\n    #[pyo3(signature = ({}))]\n    fn new({}) -> PyResult<Self> {{\n        \
             {snake}_new_inner({}).map(|inner| Self {{ inner }}).map_err(error)\n    }}",
            signature(&c.constructor, params_of(m, &c.constructor, Some(js))),
            ctor_params.join(", "),
            ctor_args.join(", ")
        );
        for (method, mutable) in &c.methods {
            let receiver = if *mutable { "&mut self" } else { "&self" };
            let params: Vec<String> = method
                .params
                .iter()
                .map(|(n, t)| format!("{n}: {}", binding_type(t)))
                .collect();
            let sig = std::iter::once(receiver.to_string())
                .chain(params)
                .collect::<Vec<_>>()
                .join(", ");
            let borrow = if *mutable {
                "&mut self.inner"
            } else {
                "&self.inner"
            };
            let args = std::iter::once(borrow)
                .chain(method.params.iter().map(|(n, _)| n.as_str()))
                .collect::<Vec<_>>();
            let _ = writeln!(
                out,
                "\n    #[pyo3(name = \"{}\", signature = ({}))]\n    fn {}({sig}) -> PyResult<{}> {{\n        \
                 {snake}_{}_inner({}).map_err(error)\n    }}",
                method.js_name,
                signature(method, params_of(m, method, Some(js))),
                method.rust_name,
                ret(method),
                method.rust_name.trim_start_matches("r#"),
                args.join(", ")
            );
        }
        out.push_str("}\n");
        registrations.push(format!("    m.add_class::<{name}>()?;"));
    }
    let _ = writeln!(
        out,
        "\n#[pymodule]\n#[pyo3(name = \"_native\")]\nfn native_module(m: &Bound<'_, PyModule>) -> PyResult<()> {{\n{}\n    Ok(())\n}}",
        registrations.join("\n")
    );
    out
}

/// The wrapper package: imports `_native`, converting structured values.
pub fn package_init(m: &Manifest, api: &PortApi) -> String {
    let structured = |f: &PortFn| -> (Vec<String>, bool) {
        (
            f.params
                .iter()
                .filter(|(_, t)| wire(t) == Wire::Json)
                .map(|(n, _)| n.trim_start_matches("r#").to_string())
                .collect(),
            wire(&f.ret) == Wire::Json,
        )
    };
    let mut out = format!(
        "\"\"\"Rust port of `{}`, generated by carcinize. Import this package where the code\n\
         imported the original module.\"\"\"\n\nfrom . import _native\n{CODEC_PY}\n",
        m.unit
    );
    let mut names = Vec::new();
    for f in &api.functions {
        let (json_params, json_ret) = structured(f);
        let positions: Vec<String> = f
            .params
            .iter()
            .enumerate()
            .filter(|(_, (_, t))| wire(t) == Wire::Json)
            .map(|(i, _)| i.to_string())
            .collect();
        if json_params.is_empty() && !json_ret {
            let _ = writeln!(out, "{} = _native.{}", f.js_name, f.js_name);
        } else {
            let _ = writeln!(
                out,
                "{name} = _wrap(_native.{name}, ({positions}), ({names_list}), {json_ret})",
                name = f.js_name,
                positions = positions
                    .iter()
                    .map(|p| format!("{p},"))
                    .collect::<String>(),
                names_list = json_params
                    .iter()
                    .map(|n| format!("'{n}',"))
                    .collect::<String>(),
                json_ret = if json_ret { "True" } else { "False" }
            );
        }
        names.push(f.js_name.clone());
    }
    for c in &api.classes {
        let needs = std::iter::once(&c.constructor)
            .chain(c.methods.iter().map(|(x, _)| x))
            .any(|f| {
                let (p, r) = structured(f);
                !p.is_empty() || r
            });
        if !needs {
            let _ = writeln!(out, "{} = _native.{}", c.js_name, c.js_name);
        } else {
            let methods: String = c
                .methods
                .iter()
                .map(|(method, _)| {
                    let (params, json_ret) = structured(method);
                    let positions: String = method
                        .params
                        .iter()
                        .enumerate()
                        .filter(|(_, (_, t))| wire(t) == Wire::Json)
                        .map(|(i, _)| format!("{i},"))
                        .collect();
                    format!(
                        "    {name} = _method('{name}', ({positions}), ({params}), {ret})\n",
                        name = method.js_name,
                        params = params.iter().map(|n| format!("'{n}',")).collect::<String>(),
                        ret = if json_ret { "True" } else { "False" }
                    )
                })
                .collect();
            let (ctor_params, _) = structured(&c.constructor);
            let ctor_positions: String = c
                .constructor
                .params
                .iter()
                .enumerate()
                .filter(|(_, (_, t))| wire(t) == Wire::Json)
                .map(|(i, _)| format!("{i},"))
                .collect();
            let _ = writeln!(
                out,
                "\nclass {js}:\n    def __init__(self, *args, **kwargs):\n        \
                 args, kwargs = _encode_args(args, kwargs, ({ctor_positions}), ({}))\n        \
                 self._inner = _native.{js}(*args, **kwargs)\n{methods}",
                ctor_params
                    .iter()
                    .map(|n| format!("'{n}',"))
                    .collect::<String>(),
                js = c.js_name
            );
        }
        names.push(c.js_name.clone());
    }
    let _ = writeln!(
        out,
        "\n__all__ = [{}]",
        names
            .iter()
            .map(|n| format!("'{n}'"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    out
}

const CODEC_PY: &str = r#"
import json as _json
import math as _math

# Structured values cross as JSON text. JSON has no NaN, tuples, sets, bytes, big ints or
# non-string keys: those travel as markers (see `js.rs`) and come back as Python values.
_BIG = 2 ** 53


def _enc(v):
    if v is None or isinstance(v, (bool, str)):
        return v
    if isinstance(v, int):
        return v if -_BIG <= v <= _BIG else {"$int": str(v)}
    if isinstance(v, float):
        if _math.isnan(v):
            return {"$num": "NaN"}
        if _math.isinf(v):
            return {"$num": "Infinity" if v > 0 else "-Infinity"}
        if v == 0 and _math.copysign(1, v) < 0:
            return {"$num": "-0"}
        return v
    if isinstance(v, list):
        return [_enc(x) for x in v]
    if isinstance(v, tuple):
        return {"$tuple": [_enc(x) for x in v]}
    if isinstance(v, (set, frozenset)):
        return {"$set": sorted((_enc(x) for x in v), key=_json.dumps)}
    if isinstance(v, (bytes, bytearray)):
        return {"$bytes": list(v)}
    if isinstance(v, dict):
        if all(isinstance(k, str) for k in v):
            return {k: _enc(x) for k, x in v.items()}
        return {"$dict": [[_enc(k), _enc(x)] for k, x in v.items()]}
    return {"$repr": repr(v)}


def _dec(v):
    if isinstance(v, list):
        return [_dec(x) for x in v]
    if not isinstance(v, dict):
        return v
    if len(v) == 1:
        (key, inner), = v.items()
        if key == "$undefined":
            return None
        if key == "$num":
            return {"NaN": _math.nan, "Infinity": _math.inf, "-Infinity": -_math.inf, "-0": -0.0}[inner]
        if key == "$int":
            return int(inner)
        if key == "$tuple":
            return tuple(_dec(x) for x in inner)
        if key == "$set":
            return {_dec(x) for x in inner}
        if key == "$bytes":
            return bytes(inner)
        if key == "$dict":
            return {_dec(k): _dec(x) for k, x in inner}
    return {k: _dec(x) for k, x in v.items()}


def _encode_args(args, kwargs, positions, names):
    args = tuple(_json.dumps(_enc(a)) if i in positions else a for i, a in enumerate(args))
    kwargs = {k: (_json.dumps(_enc(a)) if k in names else a) for k, a in kwargs.items()}
    return args, kwargs


def _wrap(fn, positions, names, json_result):
    def call(*args, **kwargs):
        args, kwargs = _encode_args(args, kwargs, positions, names)
        out = fn(*args, **kwargs)
        return _dec(_json.loads(out)) if json_result else out
    call.__name__ = fn.__name__
    call.__doc__ = fn.__doc__
    return call


def _method(name, positions, names, json_result):
    def call(self, *args, **kwargs):
        args, kwargs = _encode_args(args, kwargs, positions, names)
        out = getattr(self._inner, name)(*args, **kwargs)
        return _dec(_json.loads(out)) if json_result else out
    call.__name__ = name
    return call
"#;

#[cfg(test)]
mod tests {
    use super::rust_literal;

    #[test]
    fn converts_python_literals() {
        assert_eq!(rust_literal("'-'").as_deref(), Some("String::from(\"-\")"));
        assert_eq!(rust_literal("8").as_deref(), Some("8"));
        assert_eq!(rust_literal("1_000").as_deref(), Some("1000"));
        assert_eq!(rust_literal("0.5").as_deref(), Some("0.5"));
        assert_eq!(rust_literal("True").as_deref(), Some("true"));
        assert_eq!(rust_literal("None"), None);
        assert_eq!(rust_literal("[]"), None);
        assert_eq!(rust_literal("'a\\nb'"), None);
    }
}
