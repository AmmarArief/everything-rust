//! `src/language-js/print/property.js`.

use crate::value::Value;

use crate::doc::*;
use crate::node::*;
use crate::options::{Parser, QuoteProps};
use crate::printer::{Printer, R, bail};
use crate::utils::*;

fn is_simple_number(text: &str) -> bool {
    let mut parts = text.splitn(2, '.');
    let int = parts.next().unwrap_or("");
    let frac = parts.next();
    !int.is_empty()
        && int.bytes().all(|b| b.is_ascii_digit())
        && frac.is_none_or(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()))
}

fn js_number_string(text: &str) -> Option<String> {
    let v: f64 = text.parse().ok()?;
    Some(doc_printer_rs::js::number_to_string(v))
}

fn is_es5_identifier_name(text: &str) -> R<bool> {
    if text.is_ascii() {
        let mut chars = text.chars();
        return Ok(chars
            .next()
            .is_some_and(|c| c == '$' || c == '_' || c.is_ascii_alphabetic())
            && chars.all(|c| c == '$' || c == '_' || c.is_ascii_alphanumeric()));
    }
    if text.chars().any(|c| c as u32 > 0xFFFF) {
        return Ok(false);
    }
    static RE: std::sync::OnceLock<Option<regex::Regex>> = std::sync::OnceLock::new();
    match RE.get_or_init(|| regex::Regex::new(include_str!("es5-identifier.txt").trim()).ok()) {
        Some(re) => Ok(re.is_match(text)),
        None => bail("es5 identifier pattern"),
    }
}

fn is_string_key_safe_to_unquote(n: &Value, p: &Printer<'_>) -> R<bool> {
    if matches!(p.opts.parser, Parser::Json | Parser::Jsonc) {
        return Ok(false);
    }
    let Some(key) = n.field("key").filter(|k| is_string_literal(k)) else {
        return Ok(false);
    };
    let value = key.str_of("value").unwrap_or("");
    let raw = key.str_of("raw").unwrap_or("");
    let printed = print_string(raw, p.opts);
    if printed[1..printed.len() - 1] != *value {
        return Ok(false);
    }
    if is_es5_identifier_name(value)?
        && !(p.opts.parser == Parser::Typescript && n.is("PropertyDefinition"))
    {
        return Ok(true);
    }
    if is_simple_number(value)
        && js_number_string(value).as_deref() == Some(value)
        && !n.is("ImportAttribute")
        && p.opts.parser == Parser::Babel
    {
        return Ok(true);
    }
    Ok(false)
}

fn needs_quote_props(p: &Printer<'_>) -> R<bool> {
    let Some(parent) = p.parent() else {
        return Ok(false);
    };
    let siblings = match p.key() {
        Some(k) => parent.list(k),
        None => &[],
    };
    for prop in siblings {
        if !prop.flag("computed")
            && prop.field("key").is_some_and(is_string_literal)
            && !is_string_key_safe_to_unquote(prop, p)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn print_property_key(p: &mut Printer<'_>) -> R {
    let node = p.node();
    if node.flag("computed") {
        return Ok(concat(vec![s("["), p.print("key")?, s("]")]));
    }
    let Some(key) = node.field("key") else {
        return p.print("key");
    };
    let consistent = p.opts.quote_props == QuoteProps::Consistent;
    let needs = if consistent {
        needs_quote_props(p)?
    } else {
        false
    };
    let json_quotes = matches!(p.opts.parser, Parser::Json | Parser::Jsonc);
    if json_quotes || (consistent && needs) {
        let quotable = key.is("Identifier")
            || (is_numeric_literal(key) && {
                let printed = print_number(key.str_of("raw").unwrap_or(""));
                is_simple_number(&printed)
                    && key
                        .get("value")
                        .and_then(Value::as_f64)
                        .map(doc_printer_rs::js::number_to_string)
                        == Some(printed.clone())
                    && p.opts.parser != Parser::Typescript
            });
        if quotable {
            let text = if key.is("Identifier") {
                key.str_of("name").unwrap_or("").to_string()
            } else {
                key.get("value")
                    .and_then(Value::as_f64)
                    .map(doc_printer_rs::js::number_to_string)
                    .unwrap_or_default()
            };
            let json = serde_json::to_string(&text).unwrap_or_default();
            return Ok(crate::comments::print_comments(
                &p.comments,
                key,
                s(print_string(&json, p.opts)),
                p.text,
            ));
        }
    }
    if is_string_key_safe_to_unquote(node, p)?
        && (p.opts.quote_props == QuoteProps::AsNeeded || (consistent && !needs))
    {
        let value = key.str_of("value").unwrap_or("");
        let printed = s(if value.starts_with(|c: char| c.is_ascii_digit()) {
            print_number(value)
        } else {
            value.to_string()
        });
        return Ok(crate::comments::print_comments(
            &p.comments,
            key,
            printed,
            p.text,
        ));
    }
    p.print("key")
}

pub fn print_property(p: &mut Printer<'_>) -> R {
    let node = p.node();
    if node.flag("shorthand") {
        return p.print("value");
    }
    let key = print_property_key(p)?;
    crate::assignment::print_assignment(p, key, s(":"), Some("value"))
}
