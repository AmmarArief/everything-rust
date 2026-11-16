//! The binary wire for Go and Java: values cross as compact bytes whose encoders and decoders
//! are generated per type on both sides, without JSON text or reflection.
//!
//! Layout, little endian: `I64` and `F64` as 8 bytes, `Bool` as 1 byte. `Str`, `List`, `Map`
//! and `Struct` start with a presence byte (0 for null/nil, 1 otherwise); strings then carry a
//! u32 length and UTF-8 bytes, lists and maps a u32 count and their items (maps as string key,
//! value). `Opt` is a presence byte and the value. `Tuple` (several results) is its items in order.

use std::fmt::Write as _;

/// A source-language type the wire carries. `native` is the type as the source spells it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ty {
    I64(String),
    F64(String),
    Bool,
    Str,
    /// `array` marks a Java array (`int[]`) rather than a list.
    List {
        item: Box<Ty>,
        native: String,
        array: bool,
    },
    Map {
        value: Box<Ty>,
        native: String,
    },
    /// Fields: (source field name, JSON key, type).
    Struct {
        name: String,
        fields: Vec<(String, String, Ty)>,
    },
    Opt {
        inner: Box<Ty>,
        native: String,
    },
    Tuple(Vec<Ty>),
}

/// The `wire::Schema` constant expression for a type, for `read_value`/`write_value`.
pub fn schema_expr(t: &Ty) -> String {
    match t {
        Ty::I64(_) => "wire::Schema::I64".into(),
        Ty::F64(_) => "wire::Schema::F64".into(),
        Ty::Bool => "wire::Schema::Bool".into(),
        Ty::Str => "wire::Schema::Str".into(),
        Ty::List { item, .. } => format!("wire::Schema::List(&{})", schema_expr(item)),
        Ty::Map { value, .. } => format!("wire::Schema::Map(&{})", schema_expr(value)),
        Ty::Opt { inner, .. } => format!("wire::Schema::Opt(&{})", schema_expr(inner)),
        Ty::Struct { fields, .. } => format!(
            "wire::Schema::Struct(&[{}])",
            fields
                .iter()
                .map(|(_, key, t)| format!("({key:?}, {})", schema_expr(t)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Ty::Tuple(items) => format!(
            "wire::Schema::Tuple(&[{}])",
            items.iter().map(schema_expr).collect::<Vec<_>>().join(", ")
        ),
    }
}

fn generic_arg<'a>(rust: &'a str, head: &str) -> Option<&'a str> {
    let rust = rust.trim();
    let rest = rust
        .strip_prefix(head)
        .or_else(|| rust.strip_prefix(&format!("std::collections::{head}")))?;
    rest.strip_prefix('<')?.strip_suffix('>').map(str::trim)
}

fn map_value(rust: &str) -> Option<&str> {
    for head in ["HashMap", "BTreeMap", "IndexMap"] {
        if let Some(args) = generic_arg(rust, head) {
            let (key, value) = args.split_once(',')?;
            return (key.trim() == "String").then_some(value.trim());
        }
    }
    None
}

fn is_value(rust: &str) -> bool {
    matches!(rust.trim(), "serde_json::Value" | "Value")
}

/// A closure body: `r.f64()` stays as is, anything else is wrapped in `Ok(...)`.
fn body(read: &str) -> String {
    match read.strip_suffix('?') {
        Some(call) if call.starts_with("r.") && call.ends_with("()") => call.to_string(),
        _ => format!("Ok({read})"),
    }
}

/// Rust code reading a value of `t` into the port type `rust` from reader `r`; `None` when the
/// port type does not fit the wire type (the function then keeps the JSON bridge).
pub fn rust_read(t: &Ty, rust: &str, schema: &str) -> Option<String> {
    if is_value(rust) {
        return Some(format!("wire::read_value(r, &{schema})?"));
    }
    if rust.contains("Value") {
        return Some(format!(
            "serde_json::from_value(wire::read_value(r, &{schema})?).map_err(|e| format!(\"TypeError: {{e}}\"))?"
        ));
    }
    let rust = rust.trim();
    Some(match t {
        Ty::I64(_) if rust == "i64" => "r.i64()?".into(),
        Ty::F64(_) if rust == "f64" => "r.f64()?".into(),
        Ty::Bool if rust == "bool" => "r.bool()?".into(),
        Ty::Str if rust == "String" => "r.string()?".into(),
        Ty::Opt { inner, .. } => match generic_arg(rust, "Option") {
            Some(inner_rust) => format!(
                "r.opt(|r| {})?",
                body(&rust_read(inner, inner_rust, &format!("{schema}.inner()"))?)
            ),
            None => format!(
                "r.required(|r| {})?",
                body(&rust_read(inner, rust, &format!("{schema}.inner()"))?)
            ),
        },
        Ty::List { item, .. } => format!(
            "r.list(|r| {})?",
            body(&rust_read(
                item,
                generic_arg(rust, "Vec")?,
                &format!("{schema}.inner()")
            )?)
        ),
        Ty::Map { value, .. } => format!(
            "r.map(|r| {})?.into_iter().collect()",
            body(&rust_read(
                value,
                map_value(rust)?,
                &format!("{schema}.inner()")
            )?)
        ),
        _ => return None,
    })
}

/// Rust statements writing `expr` (a reference to the port type `rust`) as `t` into writer `w`.
pub fn rust_write(t: &Ty, rust: &str, expr: &str, schema: &str, depth: usize) -> Option<String> {
    if rust.contains("Value") {
        let value = if is_value(rust) {
            expr.to_string()
        } else {
            format!("&serde_json::to_value({expr}).map_err(|e| e.to_string())?")
        };
        return Some(format!("wire::write_value(w, {value}, &{schema})?;"));
    }
    let rust = rust.trim();
    let x = format!("x{depth}");
    // A method receiver: `out` as is, `&out` in parentheses.
    let r = if expr.starts_with('&') {
        format!("({expr})")
    } else {
        expr.to_string()
    };
    Some(match t {
        Ty::I64(_) if rust == "i64" => format!("w.i64(*{expr});"),
        Ty::F64(_) if rust == "f64" => format!("w.f64(*{expr});"),
        Ty::Bool if rust == "bool" => format!("w.bool(*{expr});"),
        Ty::Str if rust == "String" => format!("w.string({expr});"),
        Ty::Opt { inner, .. } => {
            let inner_schema = format!("{schema}.inner()");
            match generic_arg(rust, "Option") {
                Some(inner_rust) => format!(
                    "match {expr} {{ Some({x}) => {{ w.tag(1); {} }} None => w.tag(0) }}",
                    rust_write(inner, inner_rust, &x, &inner_schema, depth + 1)?
                ),
                None => format!(
                    "w.tag(1); {}",
                    rust_write(inner, rust, expr, &inner_schema, depth + 1)?
                ),
            }
        }
        Ty::List { item, .. } => format!(
            "w.tag(1); w.count({r}.len()); for {x} in {r}.iter() {{ {} }}",
            rust_write(
                item,
                generic_arg(rust, "Vec")?,
                &x,
                &format!("{schema}.inner()"),
                depth + 1
            )?
        ),
        Ty::Map { value, .. } => format!(
            "w.tag(1); w.count({r}.len()); for (k{depth}, {x}) in {r}.iter() {{ w.string(k{depth}); {} }}",
            rust_write(
                value,
                map_value(rust)?,
                &x,
                &format!("{schema}.inner()"),
                depth + 1
            )?
        ),
        _ => return None,
    })
}

/// `src/wire.rs` of a generated crate: the reader, the writer, and schema-driven conversion of
/// `serde_json::Value` (for ports that keep structured values as JSON values).
pub const WIRE_RS: &str = r#"//! The binary wire between the source language and the port. Generated by carcinize.

#![forbid(unsafe_code)]
#![allow(dead_code)]

use serde_json::{Map, Value, json};

/// A wire type, for converting `serde_json::Value`s.
pub enum Schema {
    I64,
    F64,
    Bool,
    Str,
    List(&'static Schema),
    Map(&'static Schema),
    Opt(&'static Schema),
    Struct(&'static [(&'static str, Schema)]),
    Tuple(&'static [Schema]),
}

impl Schema {
    /// The item type of a list, map or option.
    pub const fn inner(&self) -> &Schema {
        match self {
            Self::List(s) | Self::Map(s) | Self::Opt(s) => s,
            _ => self,
        }
    }
}

pub struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

const SHORT: &str = "TypeError: wire data ended early";

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.bytes.len()).ok_or(SHORT)?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    pub fn tag(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    pub fn count(&mut self) -> Result<usize, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    }

    pub fn i64(&mut self) -> Result<i64, String> {
        let b = self.take(8)?;
        Ok(i64::from_le_bytes(b.try_into().map_err(|_| SHORT)?))
    }

    pub fn f64(&mut self) -> Result<f64, String> {
        let b = self.take(8)?;
        Ok(f64::from_le_bytes(b.try_into().map_err(|_| SHORT)?))
    }

    pub fn bool(&mut self) -> Result<bool, String> {
        Ok(self.tag()? != 0)
    }

    /// A string; `None` for null.
    pub fn opt_string(&mut self) -> Result<Option<String>, String> {
        if self.tag()? == 0 {
            return Ok(None);
        }
        let n = self.count()?;
        Ok(Some(String::from_utf8_lossy(self.take(n)?).into_owned()))
    }

    pub fn string(&mut self) -> Result<String, String> {
        self.opt_string()?
            .ok_or_else(|| "NullPointerException: null string".to_string())
    }

    pub fn opt<T>(&mut self, item: impl FnOnce(&mut Self) -> Result<T, String>) -> Result<Option<T>, String> {
        if self.tag()? == 0 { Ok(None) } else { item(self).map(Some) }
    }

    pub fn required<T>(&mut self, item: impl FnOnce(&mut Self) -> Result<T, String>) -> Result<T, String> {
        self.opt(item)?
            .ok_or_else(|| "NullPointerException: null value".to_string())
    }

    /// A list; nil reads as empty.
    pub fn list<T>(&mut self, mut item: impl FnMut(&mut Self) -> Result<T, String>) -> Result<Vec<T>, String> {
        if self.tag()? == 0 {
            return Ok(Vec::new());
        }
        let n = self.count()?;
        let mut out = Vec::with_capacity(n.min(1 << 16));
        for _ in 0..n {
            out.push(item(self)?);
        }
        Ok(out)
    }

    /// A map's entries; nil reads as empty.
    pub fn map<T>(&mut self, mut item: impl FnMut(&mut Self) -> Result<T, String>) -> Result<Vec<(String, T)>, String> {
        if self.tag()? == 0 {
            return Ok(Vec::new());
        }
        let n = self.count()?;
        let mut out = Vec::with_capacity(n.min(1 << 16));
        for _ in 0..n {
            let key = self.string()?;
            out.push((key, item(self)?));
        }
        Ok(out)
    }
}

#[derive(Default)]
pub struct Writer(pub Vec<u8>);

impl Writer {
    pub fn tag(&mut self, t: u8) {
        self.0.push(t);
    }

    pub fn count(&mut self, n: usize) {
        self.0.extend_from_slice(&u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes());
    }

    pub fn i64(&mut self, v: i64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    pub fn f64(&mut self, v: f64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    pub fn bool(&mut self, v: bool) {
        self.0.push(u8::from(v));
    }

    pub fn string(&mut self, s: &str) {
        self.tag(1);
        self.count(s.len());
        self.0.extend_from_slice(s.as_bytes());
    }
}

fn float_value(x: f64) -> Value {
    if x.is_nan() {
        json!({"$num": "NaN"})
    } else if x.is_infinite() {
        json!({"$num": if x > 0.0 { "Infinity" } else { "-Infinity" }})
    } else if x == 0.0 && x.is_sign_negative() {
        json!({"$num": "-0"})
    } else {
        json!(x)
    }
}

fn value_float(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::Object(o) => match o.get("$num")?.as_str()? {
            "NaN" => Some(f64::NAN),
            "Infinity" => Some(f64::INFINITY),
            "-Infinity" => Some(f64::NEG_INFINITY),
            "-0" => Some(-0.0),
            _ => None,
        },
        _ => None,
    }
}

/// Reads a value of `schema` as JSON: null for nil, `{"$num": ...}` for NaN and infinities.
pub fn read_value(r: &mut Reader, schema: &Schema) -> Result<Value, String> {
    Ok(match schema {
        Schema::I64 => json!(r.i64()?),
        Schema::F64 => float_value(r.f64()?),
        Schema::Bool => Value::Bool(r.bool()?),
        Schema::Str => r.opt_string()?.map_or(Value::Null, Value::String),
        Schema::Opt(inner) => r.opt(|r| read_value(r, inner))?.unwrap_or(Value::Null),
        Schema::List(item) => {
            if r.tag()? == 0 {
                Value::Null
            } else {
                let n = r.count()?;
                let mut out = Vec::with_capacity(n.min(1 << 16));
                for _ in 0..n {
                    out.push(read_value(r, item)?);
                }
                Value::Array(out)
            }
        }
        Schema::Map(item) => {
            if r.tag()? == 0 {
                Value::Null
            } else {
                let n = r.count()?;
                let mut out = Map::new();
                for _ in 0..n {
                    let key = r.string()?;
                    out.insert(key, read_value(r, item)?);
                }
                Value::Object(out)
            }
        }
        Schema::Struct(fields) => {
            if r.tag()? == 0 {
                Value::Null
            } else {
                let mut out = Map::new();
                for (key, field) in *fields {
                    out.insert((*key).to_string(), read_value(r, field)?);
                }
                Value::Object(out)
            }
        }
        Schema::Tuple(items) => Value::Array(items.iter().map(|s| read_value(r, s)).collect::<Result<_, _>>()?),
    })
}

fn mismatch(schema: &str, v: &Value) -> String {
    format!("TypeError: the port returned {v} where the original returns {schema}")
}

/// Writes a JSON value as `schema`; the port's value must fit the original's type.
pub fn write_value(w: &mut Writer, v: &Value, schema: &Schema) -> Result<(), String> {
    match schema {
        Schema::I64 => w.i64(v.as_i64().or_else(|| v.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64)).ok_or_else(|| mismatch("an integer", v))?),
        Schema::F64 => w.f64(value_float(v).ok_or_else(|| mismatch("a float", v))?),
        Schema::Bool => w.bool(v.as_bool().ok_or_else(|| mismatch("a bool", v))?),
        Schema::Str => match v {
            Value::Null => w.tag(0),
            Value::String(s) => w.string(s),
            _ => return Err(mismatch("a string", v)),
        },
        Schema::Opt(inner) => {
            if v.is_null() {
                w.tag(0);
            } else {
                w.tag(1);
                write_value(w, v, inner)?;
            }
        }
        Schema::List(item) => match v {
            Value::Null => w.tag(0),
            Value::Array(items) => {
                w.tag(1);
                w.count(items.len());
                for x in items {
                    write_value(w, x, item)?;
                }
            }
            _ => return Err(mismatch("a list", v)),
        },
        Schema::Map(item) => match v {
            Value::Null => w.tag(0),
            Value::Object(o) => {
                w.tag(1);
                w.count(o.len());
                for (k, x) in o {
                    w.string(k);
                    write_value(w, x, item)?;
                }
            }
            _ => return Err(mismatch("a map", v)),
        },
        Schema::Struct(fields) => match v {
            Value::Null => w.tag(0),
            Value::Object(o) => {
                w.tag(1);
                for (key, field) in *fields {
                    write_value(w, o.get(*key).unwrap_or(&Value::Null), field)?;
                }
            }
            _ => return Err(mismatch("a struct", v)),
        },
        Schema::Tuple(items) => match v {
            Value::Array(values) if values.len() == items.len() => {
                for (x, s) in values.iter().zip(*items) {
                    write_value(w, x, s)?;
                }
            }
            _ => return Err(mismatch("several results", v)),
        },
    }
    Ok(())
}
"#;

/// Rust statements that read every argument of a port function from `r`; `None` when one does
/// not fit.
pub fn rust_read_args(
    params: &[(String, Ty)],
    port_params: &[(String, String)],
    schema_prefix: &str,
    statics: &mut String,
) -> Option<String> {
    let mut out = String::new();
    for (i, ((_, ty), (name, rust))) in params.iter().zip(port_params).enumerate() {
        let schema = format!("{schema_prefix}_{i}");
        let read = rust_read(ty, rust, &schema)?;
        if read.contains(&schema) {
            let _ = writeln!(
                statics,
                "static {schema}: wire::Schema = {};",
                schema_expr(ty)
            );
        }
        let _ = writeln!(out, "        let {name}: {rust} = {read};");
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(item: Ty) -> Ty {
        Ty::List {
            item: Box::new(item),
            native: String::new(),
            array: false,
        }
    }

    #[test]
    fn reads_fit_the_port_types() {
        let strings = list(Ty::Str);
        assert_eq!(
            rust_read(&strings, "Vec<String>", "S").unwrap(),
            "r.list(|r| r.string())?"
        );
        let boxed = Ty::Opt {
            inner: Box::new(Ty::I64("int".into())),
            native: "Integer".into(),
        };
        assert_eq!(
            rust_read(&boxed, "i64", "S").unwrap(),
            "r.required(|r| r.i64())?"
        );
        assert_eq!(
            rust_read(&boxed, "Option<i64>", "S").unwrap(),
            "r.opt(|r| r.i64())?"
        );
        assert_eq!(
            rust_read(&strings, "serde_json::Value", "S").unwrap(),
            "wire::read_value(r, &S)?"
        );
        // A port type that does not fit the wire type keeps the JSON bridge.
        assert!(rust_read(&strings, "Vec<i64>", "S").is_none());
    }

    #[test]
    fn writes_fit_the_port_types() {
        let floats = list(Ty::F64("double".into()));
        assert_eq!(
            rust_write(&floats, "Vec<f64>", "out", "S", 0).unwrap(),
            "w.tag(1); w.count(out.len()); for x0 in out.iter() { w.f64(*x0); }"
        );
        assert_eq!(
            schema_expr(&floats),
            "wire::Schema::List(&wire::Schema::F64)"
        );
    }
}
