//! `src/language-js/print/array.js`, for array expressions and patterns.

use crate::value::Value;

use crate::comments::{Comments, Flags};
use crate::doc::*;
use crate::node::*;
use crate::printer::{Args, Printer, R, bail};
use crate::text;
use crate::utils::*;

pub fn print_array(p: &mut Printer<'_>) -> R {
    let node = p.node();
    let key: &'static str = if node.is("TSTupleType") {
        "elementTypes"
    } else {
        "elements"
    };
    let elements = node.list(key);
    if elements.is_empty() {
        let inner = if !p.has_comment(node, Flags::DANGLING) {
            concat(vec![s("["), empty(), s("]")])
        } else {
            group(concat(vec![
                s("["),
                empty(),
                p.print_dangling(node, true, None),
                softline(),
                s("]"),
            ]))
        };
        return Ok(concat(vec![
            inner,
            crate::typescript::print_optional_token(p),
            crate::typescript::print_type_annotation_property(p, "typeAnnotation")?,
        ]));
    }
    let last = elements.last();
    let can_have_trailing_comma = !last.is_some_and(|l| l.is("RestElement"));
    let needs_forced_trailing_comma = last.is_some_and(Value::is_null);
    let id = p.new_id("array");
    let should_break = !p.in_jest_each
        && elements.len() > 1
        && elements.iter().enumerate().all(|(i, el)| {
            if !el.is("ArrayExpression") && !el.is("ObjectExpression") {
                return false;
            }
            if let Some(next) = elements.get(i + 1)
                && next.ty() != el.ty()
            {
                return false;
            }
            let key = if el.is("ArrayExpression") {
                "elements"
            } else {
                "properties"
            };
            el.list(key).len() > 1
        });
    let concise = is_concisely_printed_array(node, p.opts, &p.comments, p.text);
    let trailing_comma = if !can_have_trailing_comma {
        empty()
    } else if needs_forced_trailing_comma {
        s(",")
    } else if !p.opts.should_print_comma(true) {
        empty()
    } else if concise {
        if_break_id(s(","), empty(), Some(id.clone()))
    } else {
        if_break(s(","), empty())
    };
    let body = if concise {
        print_elements_concisely(p, trailing_comma)?
    } else {
        concat(vec![print_elements(p, key)?, trailing_comma])
    };
    Ok(concat(vec![
        group_with(
            concat(vec![
                s("["),
                indent(concat(vec![
                    softline(),
                    body,
                    p.print_dangling(node, false, None),
                ])),
                softline(),
                s("]"),
            ]),
            should_break,
            Some(id),
        ),
        crate::typescript::print_optional_token(p),
        crate::typescript::print_type_annotation_property(p, "typeAnnotation")?,
    ]))
}

pub fn is_concisely_printed_array(
    n: &Value,
    _opts: &crate::Options,
    c: &Comments,
    src: &str,
) -> bool {
    n.is("ArrayExpression")
        && n.list("elements").len() > 1
        && n.list("elements").iter().all(|e| {
            (is_numeric_literal(e)
                || (is_signed_numeric_literal(e)
                    && !e.field("argument").is_some_and(|a| c.has_any(a))))
                && !c.has_with(e, Flags::TRAILING.with(Flags::LINE), &|cm| {
                    !text::has_newline(src, cm.start, true)
                })
        })
}

fn is_line_after_element_empty(n: &Value, src: &str) -> R<bool> {
    let mut idx = n.end();
    let mut guard = 0;
    while src.as_bytes().get(idx) != Some(&b',') {
        let next = text::skip_inline_comment(src, text::skip_trailing_comment(src, Some(idx + 1)));
        match next {
            Some(i) if i < src.len() => idx = i,
            _ => return bail("array element without comma"),
        }
        guard += 1;
        if guard > src.len() {
            return bail("array element scan");
        }
    }
    Ok(text::is_next_line_empty(src, idx))
}

fn print_elements(p: &mut Printer<'_>, key: &'static str) -> R {
    let src = p.text;
    let count = p.node().list(key).len();
    let mut parts = Vec::new();
    p.each(key, |p, i| {
        let n = p.node();
        let is_last = i + 1 == count;
        if n.is_null() {
            parts.push(empty());
        } else {
            parts.push(group(p.print_current(Args::default())?));
        }
        if !is_last {
            let gap = if !n.is_null() && is_line_after_element_empty(n, src)? {
                softline()
            } else {
                empty()
            };
            parts.push(concat(vec![s(","), line(), gap]));
        }
        Ok(())
    })?;
    Ok(concat(parts))
}

fn print_elements_concisely(p: &mut Printer<'_>, trailing_comma: Doc) -> R {
    let src = p.text;
    let elements = p.node().list("elements");
    let count = elements.len();
    let mut parts = Vec::new();
    p.each("elements", |p, i| {
        let is_last = i + 1 == count;
        let printed = p.print_current(Args::default())?;
        parts.push(concat(vec![
            printed,
            if is_last {
                trailing_comma.clone()
            } else {
                s(",")
            },
        ]));
        if !is_last {
            parts.push(if is_line_after_element_empty(p.node(), src)? {
                concat(vec![hardline(), hardline()])
            } else if elements
                .get(i + 1)
                .is_some_and(|next| p.has_comment(next, Flags::LEADING.with(Flags::LINE)))
            {
                hardline()
            } else {
                line()
            });
        }
        Ok(())
    })?;
    Ok(fill(parts))
}
