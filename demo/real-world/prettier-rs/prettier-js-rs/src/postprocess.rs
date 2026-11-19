//! `src/language-js/parse/postprocess/index.js`, for oxc's ESTree output.

use crate::value::{Map, Value};

use crate::node::NodeExt;
use crate::printer::R;

pub fn postprocess(
    ast: &mut Value,
    text: &str,
    comments: &[crate::comments::Comment],
    is_ts: bool,
) -> R<()> {
    let casts: Vec<usize> = comments
        .iter()
        .filter(|c| c.is_type_cast())
        .map(|c| c.end)
        .collect();
    visit(ast, text, &casts, is_ts);
    if let Value::Object(map) = ast {
        map.remove("hashbang");
        map.insert("start".into(), Value::from(0));
        map.insert("end".into(), Value::from(text.len()));
    }
    Ok(())
}

fn visit(node: &mut Value, text: &str, casts: &[usize], is_ts: bool) {
    match node {
        Value::Array(items) => {
            for item in items {
                visit(item, text, casts, is_ts);
            }
            return;
        }
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if key != "extra" {
                    visit(value, text, casts, is_ts);
                }
            }
        }
        _ => return,
    }
    loop {
        match node.ty() {
            "ParenthesizedExpression" => {
                let start = node.start();
                if casts
                    .iter()
                    .rev()
                    .find(|&&end| end <= start)
                    .is_some_and(|&end| text[end..start].trim().is_empty())
                {
                    return;
                }
                let Some(mut expression) = node.get_mut("expression").map(Value::take) else {
                    return;
                };
                if let Value::Object(map) = &mut expression {
                    let mut extra = map
                        .get("extra")
                        .and_then(Value::as_object)
                        .cloned()
                        .unwrap_or_default();
                    extra.insert("parenthesized".into(), Value::Bool(true));
                    map.insert("extra".into(), Value::Object(extra));
                }
                *node = expression;
            }
            "LogicalExpression" if is_unbalanced(node) => {
                *node = rebalance(node.take());
            }
            "TemplateElement" if is_ts => {
                let (start, end, tail) = (node.start(), node.end(), node.flag("tail"));
                if let Value::Object(map) = node {
                    map.insert("start".into(), Value::from(start + 1));
                    map.insert("end".into(), Value::from(end - if tail { 1 } else { 2 }));
                }
                return;
            }
            "TSImportType" => {
                if let Value::Object(map) = node
                    && let Some(source) = map.remove("source")
                {
                    map.insert("argument".into(), source);
                }
                return;
            }
            "VariableDeclaration" => {
                let last = node
                    .list("declarations")
                    .last()
                    .map(|d| (d.field("init").is_some(), d.end()));
                if let Some((true, end)) = last
                    && text.as_bytes().get(end) != Some(&b';')
                    && let Value::Object(map) = node
                {
                    map.insert("end".into(), Value::from(end));
                }
                return;
            }
            "TSParenthesizedType" => {
                let Some(inner) = node.get_mut("typeAnnotation").map(Value::take) else {
                    return;
                };
                *node = inner;
            }
            "TSUnionType" | "TSIntersectionType" if node.list("types").len() == 1 => {
                let inner = node
                    .get_mut("types")
                    .and_then(|t| t.get_mut(0))
                    .map(Value::take);
                if let Some(inner) = inner {
                    *node = inner;
                }
            }
            _ => return,
        }
    }
}

fn is_unbalanced(node: &Value) -> bool {
    node.is("LogicalExpression")
        && node.field("right").is_some_and(|r| {
            r.is("LogicalExpression") && r.str_of("operator") == node.str_of("operator")
        })
}

fn rebalance(mut node: Value) -> Value {
    if !is_unbalanced(&node) {
        return node;
    }
    let operator = node["operator"].clone();
    let start = node.start();
    let end = node.end();
    let left = node.get_mut("left").map(Value::take).unwrap_or_default();
    let mut right = node.get_mut("right").map(Value::take).unwrap_or_default();
    let right_left = right.get_mut("left").map(Value::take).unwrap_or_default();
    let right_right = right.get_mut("right").map(Value::take).unwrap_or_default();
    let mut inner = Map::new();
    inner.insert("type".into(), "LogicalExpression".into());
    inner.insert("operator".into(), operator.clone());
    inner.insert("start".into(), Value::from(left.start()));
    inner.insert("end".into(), Value::from(right_left.end()));
    inner.insert("left".into(), left);
    inner.insert("right".into(), right_left);
    let mut outer = Map::new();
    outer.insert("type".into(), "LogicalExpression".into());
    outer.insert("operator".into(), operator);
    outer.insert("left".into(), rebalance(Value::Object(inner)));
    outer.insert("right".into(), right_right);
    outer.insert("start".into(), Value::from(start));
    outer.insert("end".into(), Value::from(end));
    rebalance(Value::Object(outer))
}
