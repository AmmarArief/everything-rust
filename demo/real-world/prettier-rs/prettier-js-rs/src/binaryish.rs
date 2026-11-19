//! `src/language-js/print/binaryish.js`.

use crate::value::Value;

use crate::comments::Flags;
use crate::doc::*;
use crate::node::*;
use crate::printer::{Args, Printer, R};
use crate::utils::*;

pub fn print_binaryish(p: &mut Printer<'_>) -> R {
    let node = p.node();
    let parent = p.parent().unwrap_or(node);
    let grandparent = p.grandparent();
    let key = p.key();
    let inside_parens = key != Some("body")
        && matches!(
            parent.ty(),
            "IfStatement" | "WhileStatement" | "SwitchStatement" | "DoWhileStatement"
        );
    let parts = print_binaryish_expressions(p, inside_parens, false)?;
    if inside_parens {
        return Ok(concat(parts));
    }
    if (is_call_expression(parent) && parent.field("callee").is_some_and(|c| c.same(node)))
        || parent.is("UnaryExpression")
        || (is_member_expression(parent) && !parent.flag("computed"))
    {
        let mut inner = vec![softline()];
        inner.extend(parts);
        return Ok(group(concat(vec![indent(concat(inner)), softline()])));
    }
    let is_body = parent.field("body").is_some_and(|b| b.same(node));
    let gp_ty = grandparent.map(|g| g.ty()).unwrap_or("");
    let should_not_indent = parent.is("ReturnStatement")
        || parent.is("ThrowStatement")
        || (parent.is("JSXExpressionContainer") && gp_ty == "JSXAttribute")
        || (is_body && parent.is("ArrowFunctionExpression"))
        || (!is_body && parent.is("ForStatement"))
        || (parent.is("ConditionalExpression")
            && gp_ty != "ReturnStatement"
            && gp_ty != "ThrowStatement"
            && !grandparent.is_some_and(is_call_expression))
        || parent.is("TemplateLiteral");
    let should_indent_if_inlining = matches!(
        parent.ty(),
        "AssignmentExpression"
            | "VariableDeclarator"
            | "ClassProperty"
            | "PropertyDefinition"
            | "TSAbstractPropertyDefinition"
            | "ClassPrivateProperty"
    ) || is_object_property(parent);
    let left = node.field("left");
    let same_precedence_sub = left.is_some_and(|l| {
        is_binaryish(l)
            && should_flatten(
                node.str_of("operator").unwrap_or(""),
                l.str_of("operator").unwrap_or(""),
            )
    });
    let inline = should_inline_logical_expression(node);
    if should_not_indent
        || (inline && !same_precedence_sub)
        || (!inline && should_indent_if_inlining)
    {
        return Ok(group(concat(parts)));
    }
    if parts.is_empty() {
        return Ok(empty());
    }
    let has_jsx = node.field("right").is_some_and(is_jsx_element);
    let first_group = parts.iter().position(|d| match d {
        Doc::Shared(inner) => matches!(&**inner, Doc::Group { .. }),
        other => matches!(other, Doc::Group { .. }),
    });
    let head_len = first_group.map_or(1, |i| i + 1);
    let mut parts = parts;
    let jsx_part = if has_jsx { parts.pop() } else { None };
    let rest: Vec<Doc> = parts.split_off(head_len.min(parts.len()));
    let id = p.new_id("logicalChain");
    parts.push(indent(concat(rest)));
    let chain = group_with(concat(parts), false, Some(id.clone()));
    match jsx_part {
        None => Ok(chain),
        Some(j) => Ok(group(concat(vec![
            chain,
            indent_if_break(j, Some(id), false),
        ]))),
    }
}

fn split_first(doc: &Doc) -> (Doc, Doc) {
    match doc {
        Doc::Shared(inner) => split_first(inner),
        Doc::Array(items) if !items.is_empty() => {
            (items[0].clone(), Doc::Array(items[1..].to_vec()))
        }
        Doc::Label(name, contents) => {
            let (first, rest) = split_first(contents);
            (first, Doc::Label(name.clone(), Box::new(rest)))
        }
        other => (empty(), other.clone()),
    }
}

fn print_binaryish_expressions(
    p: &mut Printer<'_>,
    inside_parens: bool,
    is_nested: bool,
) -> R<Vec<Doc>> {
    let node = p.node();
    if !is_binaryish(node) {
        return Ok(vec![group(p.print_current(Args::default())?)]);
    }
    let operator = node.str_of("operator").unwrap_or("");
    let left = node.field("left");
    let mut parts =
        if left.is_some_and(|l| should_flatten(operator, l.str_of("operator").unwrap_or(""))) {
            p.call("left", |p| {
                print_binaryish_expressions(p, inside_parens, true)
            })
            .unwrap_or(Ok(Vec::new()))?
        } else {
            vec![group(p.print("left")?)]
        };
    let inline = should_inline_logical_expression(node);
    let right_node = node.field("right");
    let text = p.text;
    let right_own_line =
        right_node.is_some_and(|r| has_leading_own_line_comment(&p.comments, text, r));
    let line_before_operator = operator == "|>" && !right_own_line;
    let has_type_cast_comment = right_node.is_some_and(|r| {
        p.comments
            .has_with(r, Flags::LEADING, &|c| c.is_type_cast())
    });
    let comment_before_operator = !has_type_cast_comment && right_own_line;
    let mut right = if inline {
        let printed = p.print("right")?;
        concat(vec![
            s(operator.to_string()),
            if right_own_line {
                indent(concat(vec![line(), printed, empty()]))
            } else {
                concat(vec![s(" "), printed, empty()])
            },
        ])
    } else {
        let right_content = p.print("right")?;
        if p.opts.operator_position_start {
            let (comment, right_content) = if comment_before_operator {
                split_first(&right_content)
            } else {
                (empty(), right_content)
            };
            concat(vec![
                line(),
                comment,
                s(operator.to_string()),
                s(" "),
                right_content,
                empty(),
            ])
        } else {
            concat(vec![
                if line_before_operator {
                    line()
                } else {
                    empty()
                },
                s(operator.to_string()),
                if line_before_operator { s(" ") } else { line() },
                right_content,
                empty(),
            ])
        }
    };
    let parent = p.parent();
    let should_break = left.is_some_and(|l| p.has_comment(l, Flags::TRAILING.with(Flags::LINE)));
    let should_group = should_break
        || (!(inside_parens && node.is("LogicalExpression"))
            && parent.is_none_or(|pa| pa.ty() != node.ty())
            && left.is_none_or(|l| l.ty() != node.ty())
            && right_node.is_none_or(|r| r.ty() != node.ty()));
    if should_group {
        right = group_with(right, should_break, None);
    }
    if p.opts.operator_position_start {
        parts.push(s(if inline || comment_before_operator {
            " "
        } else {
            ""
        }));
    } else {
        parts.push(s(if line_before_operator { "" } else { " " }));
    }
    parts.push(right);
    if is_nested && p.has_any_comment(node) {
        let printed = clean_doc(&crate::comments::print_comments(
            &p.comments,
            node,
            concat(parts),
            text,
        ));
        return Ok(match printed {
            Doc::Fill(items) | Doc::Array(items) => items,
            other => vec![other],
        });
    }
    Ok(parts)
}

pub fn should_inline_logical_expression(n: &Value) -> bool {
    if !n.is("LogicalExpression") {
        return false;
    }
    let Some(right) = n.field("right") else {
        return false;
    };
    (right.is("ObjectExpression") && !right.list("properties").is_empty())
        || (right.is("ArrayExpression") && !right.list("elements").is_empty())
        || is_jsx_element(right)
}
