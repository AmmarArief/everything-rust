//! The Rust port of `src/index.js`'s `format` for JavaScript and TypeScript files the
//! `prettier-js-rs` printer covers. Other files return an error naming what is not ported.

#![forbid(unsafe_code)]

pub use prettier_js_rs as printer;
use prettier_js_rs::Options;
use prettier_js_rs::options::{EndOfLine, ProseWrap, QuoteProps, TrailingComma};
use serde_json::Value;

/// Port of `format` (`src/index.js:68`).
pub async fn format(text: Value, options: Value) -> Result<Value, String> {
    let Some(text) = text.as_str() else {
        return Err("TypeError: text must be a string".into());
    };
    let opts = read_options(&options)?;
    match prettier_js_rs::format(text, &opts) {
        Ok(out) => Ok(Value::String(out)),
        Err(e) => Err(format!("Unsupported: {}", e.0)),
    }
}

fn read_options(v: &Value) -> Result<Options, String> {
    let path = v
        .get("filepath")
        .and_then(Value::as_str)
        .ok_or("Unsupported: no filepath")?;
    let forced = match v.get("parser") {
        None => None,
        Some(Value::String(s)) if s == "graphql" => Some(prettier_js_rs::options::Parser::Graphql),
        Some(Value::String(s)) if s == "yaml" => Some(prettier_js_rs::options::Parser::Yaml),
        Some(Value::String(s)) if s == "markdown" => {
            Some(prettier_js_rs::options::Parser::Markdown)
        }
        Some(Value::String(s)) if s == "css" => Some(prettier_js_rs::options::Parser::Css),
        Some(Value::String(s)) if s == "scss" => Some(prettier_js_rs::options::Parser::Scss),
        Some(Value::String(s)) if s == "less" => Some(prettier_js_rs::options::Parser::Less),
        Some(_) => return Err("Unsupported: option parser".into()),
    };
    let mut o = Options::for_path_with(path, forced).ok_or("Unsupported: parser")?;
    let Some(map) = v.as_object() else {
        return Err("Unsupported: options".into());
    };
    for (key, value) in map {
        let unsupported = || format!("Unsupported: option {key}");
        match (key.as_str(), value) {
            ("filepath" | "parser", _) => {}
            ("proseWrap", Value::String(s)) => {
                o.prose_wrap = match s.as_str() {
                    "always" => ProseWrap::Always,
                    "never" => ProseWrap::Never,
                    "preserve" => ProseWrap::Preserve,
                    _ => return Err(unsupported()),
                }
            }
            ("endOfLine", Value::String(s)) => {
                o.end_of_line = match s.as_str() {
                    "lf" => EndOfLine::Lf,
                    "crlf" => EndOfLine::Crlf,
                    "cr" => EndOfLine::Cr,
                    "auto" => EndOfLine::Auto,
                    _ => return Err(unsupported()),
                }
            }
            ("printWidth", Value::Number(n)) => {
                o.print_width = n.as_u64().ok_or_else(unsupported)? as usize
            }
            ("tabWidth", Value::Number(n)) => {
                o.tab_width = n.as_u64().ok_or_else(unsupported)? as usize
            }
            ("useTabs", Value::Bool(b)) => o.use_tabs = *b,
            ("semi", Value::Bool(b)) => o.semi = *b,
            ("singleQuote", Value::Bool(b)) => o.single_quote = *b,
            ("jsxSingleQuote", Value::Bool(b)) => o.jsx_single_quote = *b,
            ("bracketSpacing", Value::Bool(b)) => o.bracket_spacing = *b,
            ("bracketSameLine", Value::Bool(b)) => o.bracket_same_line = *b,
            ("experimentalTernaries", Value::Bool(b)) => o.experimental_ternaries = *b,
            ("arrowParens", Value::String(s)) => {
                o.arrow_parens_always = match s.as_str() {
                    "always" => true,
                    "avoid" => false,
                    _ => return Err(unsupported()),
                }
            }
            ("trailingComma", Value::String(s)) => {
                o.trailing_comma = match s.as_str() {
                    "all" => TrailingComma::All,
                    "es5" => TrailingComma::Es5,
                    "none" => TrailingComma::None,
                    _ => return Err(unsupported()),
                }
            }
            ("quoteProps", Value::String(s)) => {
                o.quote_props = match s.as_str() {
                    "as-needed" => QuoteProps::AsNeeded,
                    "consistent" => QuoteProps::Consistent,
                    "preserve" => QuoteProps::Preserve,
                    _ => return Err(unsupported()),
                }
            }
            ("objectWrap", Value::String(s)) => {
                o.object_wrap_preserve = match s.as_str() {
                    "preserve" => true,
                    "collapse" => false,
                    _ => return Err(unsupported()),
                }
            }
            ("experimentalOperatorPosition", Value::String(s)) => {
                o.operator_position_start = match s.as_str() {
                    "start" => true,
                    "end" => false,
                    _ => return Err(unsupported()),
                }
            }
            _ => return Err(unsupported()),
        }
    }
    Ok(o)
}
