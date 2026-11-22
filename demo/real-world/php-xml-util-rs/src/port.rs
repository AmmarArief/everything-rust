//! Rust port of PEAR XML_Util 1.4.5 (`XML/Util.php`, New BSD License, Stephan Schmidt), reached
//! through the facade in `original/xml_util.php`.

#![forbid(unsafe_code)]

use serde_json::{Map, Value};
use std::cmp::Ordering;

const LATIN1: [&str; 96] = [
    "nbsp", "iexcl", "cent", "pound", "curren", "yen", "brvbar", "sect", "uml", "copy", "ordf",
    "laquo", "not", "shy", "reg", "macr", "deg", "plusmn", "sup2", "sup3", "acute", "micro",
    "para", "middot", "cedil", "sup1", "ordm", "raquo", "frac14", "frac12", "frac34", "iquest",
    "Agrave", "Aacute", "Acirc", "Atilde", "Auml", "Aring", "AElig", "Ccedil", "Egrave", "Eacute",
    "Ecirc", "Euml", "Igrave", "Iacute", "Icirc", "Iuml", "ETH", "Ntilde", "Ograve", "Oacute",
    "Ocirc", "Otilde", "Ouml", "times", "Oslash", "Ugrave", "Uacute", "Ucirc", "Uuml", "Yacute",
    "THORN", "szlig", "agrave", "aacute", "acirc", "atilde", "auml", "aring", "aelig", "ccedil",
    "egrave", "eacute", "ecirc", "euml", "igrave", "iacute", "icirc", "iuml", "eth", "ntilde",
    "ograve", "oacute", "ocirc", "otilde", "ouml", "divide", "oslash", "ugrave", "uacute", "ucirc",
    "uuml", "yacute", "thorn", "yuml",
];

fn bytes_to_string(bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|_| "UnexpectedValueException: not UTF-8".to_string())
}

fn strtr(s: &str, map: &[(char, &str)]) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match map.iter().find(|(k, _)| *k == c) {
            Some((_, v)) => out.push_str(v),
            None => out.push(c),
        }
    }
    out
}

fn htmlentities_latin1(s: &str) -> Result<String, String> {
    let mut out = Vec::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'&' => out.extend_from_slice(b"&amp;"),
            b'"' => out.extend_from_slice(b"&quot;"),
            b'<' => out.extend_from_slice(b"&lt;"),
            b'>' => out.extend_from_slice(b"&gt;"),
            0xA0..=0xFF => {
                out.push(b'&');
                out.extend_from_slice(LATIN1[usize::from(b - 0xA0)].as_bytes());
                out.push(b';');
            }
            _ => out.push(b),
        }
    }
    bytes_to_string(out)
}

fn named_entity(name: &str) -> Option<u32> {
    match name {
        "amp" => Some(u32::from(b'&')),
        "lt" => Some(u32::from(b'<')),
        "gt" => Some(u32::from(b'>')),
        "quot" => Some(u32::from(b'"')),
        _ => LATIN1
            .iter()
            .position(|n| *n == name)
            .and_then(|i| u32::try_from(i).ok())
            .map(|i| 0xA0 + i),
    }
}

fn html_entity_decode_latin1(s: &str) -> Result<String, String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'&' {
            out.push(b[i]);
            i += 1;
            continue;
        }
        let decoded = if b.get(i + 1) == Some(&b'#') {
            let hex = matches!(b.get(i + 2), Some(b'x' | b'X'));
            let start = i + if hex { 3 } else { 2 };
            let mut end = start;
            while end < b.len()
                && (if hex {
                    b[end].is_ascii_hexdigit()
                } else {
                    b[end].is_ascii_digit()
                })
            {
                end += 1;
            }
            if end == start || b.get(end) != Some(&b';') {
                None
            } else {
                let digits = std::str::from_utf8(&b[start..end]).unwrap_or("");
                u32::from_str_radix(digits, if hex { 16 } else { 10 })
                    .ok()
                    .filter(|&cp| cp <= 0x10FFFF)
                    .map(|cp| (cp, end + 1))
            }
        } else {
            let start = i + 1;
            let mut end = start;
            while end < b.len() && b[end].is_ascii_alphanumeric() {
                end += 1;
            }
            if end == start || b.get(end) != Some(&b';') {
                None
            } else {
                let name = std::str::from_utf8(&b[start..end]).unwrap_or("");
                named_entity(name).map(|cp| (cp, end + 1))
            }
        };
        match decoded {
            Some((cp, next)) if matches!(cp, 9 | 10 | 13 | 32..=38 | 40..=126 | 160..=255) => {
                out.push(u8::try_from(cp).unwrap_or(b'?'));
                i = next;
            }
            _ => {
                out.push(b'&');
                i += 1;
            }
        }
    }
    bytes_to_string(out)
}

pub fn replace_entities(string: String, replace_entities: i64) -> Result<String, String> {
    match replace_entities {
        1 => Ok(strtr(
            &string,
            &[
                ('&', "&amp;"),
                ('>', "&gt;"),
                ('<', "&lt;"),
                ('"', "&quot;"),
                ('\'', "&apos;"),
            ],
        )),
        2 => Ok(strtr(
            &string,
            &[('&', "&amp;"), ('<', "&lt;"), ('"', "&quot;")],
        )),
        3 => htmlentities_latin1(&string),
        _ => Ok(string),
    }
}

fn reverse_strtr(s: &str, map: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    'outer: while !rest.is_empty() {
        for (from, to) in map {
            if let Some(tail) = rest.strip_prefix(from) {
                out.push_str(to);
                rest = tail;
                continue 'outer;
            }
        }
        let c = rest.chars().next().unwrap_or_default();
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

pub fn reverse_entities(string: String, replace_entities: i64) -> Result<String, String> {
    match replace_entities {
        1 => Ok(reverse_strtr(
            &string,
            &[
                ("&amp;", "&"),
                ("&gt;", ">"),
                ("&lt;", "<"),
                ("&quot;", "\""),
                ("&apos;", "'"),
            ],
        )),
        2 => Ok(reverse_strtr(
            &string,
            &[("&amp;", "&"), ("&lt;", "<"), ("&quot;", "\"")],
        )),
        3 => html_entity_decode_latin1(&string),
        _ => Ok(string),
    }
}

#[derive(Clone)]
enum Key {
    Int(i64),
    Str(String),
}

impl Key {
    fn from_json(key: &str) -> Self {
        let canonical = key.parse::<i64>().ok().filter(|n| n.to_string() == key);
        canonical.map_or_else(|| Self::Str(key.to_string()), Self::Int)
    }

    fn text(&self) -> String {
        match self {
            Self::Int(n) => n.to_string(),
            Self::Str(s) => s.clone(),
        }
    }
}

fn numeric(s: &str) -> Option<f64> {
    let t = s.trim_start_matches([' ', '\t', '\n', '\r', '\x0B', '\x0C']);
    let t = t.trim_end_matches([' ', '\t', '\n', '\r', '\x0B', '\x0C']);
    let body = t.strip_prefix(['+', '-']).unwrap_or(t);
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(i) => (&body[..i], Some(&body[i + 1..])),
        None => (body, None),
    };
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |p: &str| p.bytes().all(|c| c.is_ascii_digit());
    if (int.is_empty() && frac.is_empty()) || !digits(int) || !digits(frac) {
        return None;
    }
    if let Some(e) = exponent {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        if e.is_empty() || !digits(e) {
            return None;
        }
    }
    t.parse::<f64>().ok()
}

fn compare_keys(a: &Key, b: &Key) -> Ordering {
    match (a, b) {
        (Key::Int(x), Key::Int(y)) => x.cmp(y),
        (Key::Str(x), Key::Str(y)) => match (numeric(x), numeric(y)) {
            (Some(p), Some(q)) => p.partial_cmp(&q).unwrap_or(Ordering::Equal),
            _ => x.as_bytes().cmp(y.as_bytes()),
        },
        (Key::Int(x), Key::Str(y)) => match numeric(y) {
            Some(q) => (*x as f64).partial_cmp(&q).unwrap_or(Ordering::Equal),
            None => x.to_string().as_bytes().cmp(y.as_bytes()),
        },
        (Key::Str(_), Key::Int(_)) => compare_keys(b, a).reverse(),
    }
}

fn php_string(v: &Value) -> Result<String, String> {
    if let Value::Object(o) = v
        && o.len() == 1
        && let Some(marker) = o.get("$num").and_then(Value::as_str)
    {
        return Ok(match marker {
            "NaN" => "NAN",
            "Infinity" => "INF",
            "-Infinity" => "-INF",
            _ => "-0",
        }
        .into());
    }
    match v {
        Value::String(s) => Ok(s.clone()),
        Value::Null | Value::Bool(false) => Ok(String::new()),
        Value::Bool(true) => Ok("1".into()),
        Value::Number(n) if n.is_i64() || n.is_u64() => Ok(n.to_string()),
        Value::Number(n) => Ok(n.as_f64().map(php_float).unwrap_or_default()),
        Value::Array(_) | Value::Object(_) => Err("array".into()),
    }
}

fn php_float(x: f64) -> String {
    if x == 0.0 {
        return if x.is_sign_negative() { "-0" } else { "0" }.into();
    }
    let s = format!("{:.13e}", x);
    let (mantissa, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
    if (-4..14).contains(&exp) {
        let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
        let negative = x < 0.0;
        let point = exp + 1;
        let body = if point <= 0 {
            format!("0.{}{}", "0".repeat(point.unsigned_abs() as usize), digits)
        } else if point as usize >= digits.len() {
            format!("{digits}{}", "0".repeat(point as usize - digits.len()))
        } else {
            format!(
                "{}.{}",
                &digits[..point as usize],
                &digits[point as usize..]
            )
        };
        format!("{}{body}", if negative { "-" } else { "" })
    } else {
        let mantissa = if mantissa.contains('.') {
            mantissa.to_string()
        } else {
            format!("{mantissa}.0")
        };
        format!(
            "{mantissa}E{}{}",
            if exp < 0 { "-" } else { "+" },
            exp.abs()
        )
    }
}

fn attribute_value(value: &Value, entities: i64) -> Result<String, String> {
    if entities != 0 && matches!(entities, 1..=3) {
        let text = php_string(value).map_err(|_| {
            let f = if entities == 3 {
                "htmlentities"
            } else {
                "strtr"
            };
            format!("TypeError: {f}(): Argument #1 ($string) must be of type string, array given")
        })?;
        return replace_entities(text, entities);
    }
    Ok(php_string(value).unwrap_or_else(|_| "Array".into()))
}

pub fn attributes_to_string(
    attributes: Value,
    sort: bool,
    multiline: bool,
    indent: String,
    linebreak: Option<String>,
    entities: i64,
) -> Result<String, String> {
    let linebreak = linebreak.unwrap_or_else(|| "\n".into());
    let mut entries: Vec<(Key, Value)> = match attributes {
        Value::Object(map) => map
            .into_iter()
            .map(|(k, v)| (Key::from_json(&k), v))
            .collect(),
        Value::Array(items) => items
            .into_iter()
            .enumerate()
            .map(|(i, v)| (Key::Int(i64::try_from(i).unwrap_or(i64::MAX)), v))
            .collect(),
        _ => return Ok(String::new()),
    };
    if entries.is_empty() {
        return Ok(String::new());
    }
    if sort {
        for i in 1..entries.len() {
            let mut j = i;
            while j > 0 && compare_keys(&entries[j - 1].0, &entries[j].0) == Ordering::Greater {
                entries.swap(j - 1, j);
                j -= 1;
            }
        }
    }
    let mut out = String::new();
    if !multiline || entries.len() == 1 {
        let mut entities = entities;
        for (key, value) in &entries {
            if entities != 0 && entities == 5 {
                entities = 1;
            }
            let value = attribute_value(value, entities)?;
            out.push_str(&format!(" {}=\"{value}\"", key.text()));
        }
    } else {
        for (i, (key, value)) in entries.iter().enumerate() {
            let value = attribute_value(value, entities)?;
            if i == 0 {
                out.push_str(&format!(" {}=\"{value}\"", key.text()));
            } else {
                out.push_str(&format!("{linebreak}{indent}{}=\"{value}\"", key.text()));
            }
        }
    }
    Ok(out)
}

pub fn split_qualified_name(qname: String, default_ns: Option<String>) -> Result<Value, String> {
    let mut out = Map::new();
    if let Some((ns, rest)) = qname.split_once(':') {
        let local = rest.split(':').next().unwrap_or("");
        out.insert("namespace".into(), Value::String(ns.into()));
        out.insert("localPart".into(), Value::String(local.into()));
    } else {
        out.insert(
            "namespace".into(),
            default_ns.map_or(Value::Null, Value::String),
        );
        out.insert("localPart".into(), Value::String(qname));
    }
    Ok(Value::Object(out))
}

pub fn create_comment(content: String) -> Result<String, String> {
    Ok(format!("<!-- {content} -->"))
}

pub fn create_cdata_section(data: String) -> Result<String, String> {
    Ok(format!(
        "<![CDATA[{}]]>",
        data.replace("]]>", "]]]]><![CDATA[>")
    ))
}

pub fn create_end_element(qname: String) -> Result<String, String> {
    Ok(format!("</{qname}>"))
}
