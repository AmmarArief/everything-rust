//! `src/language-js/print/object.js`, for object expressions and patterns.

use crate::value::Value;

use crate::doc::*;
use crate::function::should_hug_the_only_function_parameter;
use crate::node::*;
use crate::printer::{Args, Printer, R};
use crate::text::has_newline_in_range;
use crate::utils::*;

pub fn print_object(p: &mut Printer<'_>) -> R {
    let node = p.node();
    let text = p.text;
    let semi = if p.opts.semi { ";" } else { "" };
    let is_enum_body = node.is("TSEnumBody");
    let field: &'static str = if node.is("TSTypeLiteral") || is_enum_body {
        "members"
    } else if node.is("TSInterfaceBody") {
        "body"
    } else {
        "properties"
    };
    let mut props_and_loc: Vec<(&Value, Doc, usize)> = Vec::new();
    p.each(field, |p, _| {
        let n = p.node();
        let printed = p.print_current(Args::default())?;
        props_and_loc.push((n, printed, n.start()));
        Ok(())
    })?;
    let parent = p.parent();
    let parent_ty = parent.map(|pa| pa.ty()).unwrap_or("");
    let should_break = node.is("TSInterfaceBody")
        || is_enum_body
        || (node.is("ObjectPattern")
            && !matches!(
                parent_ty,
                "FunctionDeclaration"
                    | "FunctionExpression"
                    | "ArrowFunctionExpression"
                    | "ObjectMethod"
                    | "ClassMethod"
                    | "ClassPrivateMethod"
                    | "AssignmentPattern"
                    | "CatchClause"
            )
            && node.list("properties").iter().any(|pr| {
                pr.field("value")
                    .is_some_and(|v| v.is("ObjectPattern") || v.is("ArrayPattern"))
            }))
        || (!node.is("ObjectPattern")
            && p.opts.object_wrap_preserve
            && !props_and_loc.is_empty()
            && has_newline_in_range(text, node.start(), props_and_loc[0].2));
    let ts_body = node.is("TSInterfaceBody") || node.is("TSTypeLiteral");
    let separator = |_: ()| -> Doc {
        if ts_body {
            if_break(s(semi), s(";"))
        } else {
            s(",")
        }
    };
    let is_ts_sig = |n: &Value| {
        matches!(
            n.ty(),
            "TSPropertySignature"
                | "TSMethodSignature"
                | "TSConstructSignatureDeclaration"
                | "TSCallSignatureDeclaration"
        )
    };
    let mut separator_parts: Vec<Doc> = Vec::new();
    let mut props = Vec::new();
    let last_elem = props_and_loc.last().map(|(n, _, _)| *n);
    for (n, printed, _) in props_and_loc {
        let mut result = std::mem::take(&mut separator_parts);
        result.push(group(printed));
        separator_parts = vec![separator(()), line()];
        if is_ts_sig(n) && p.has_comment(n, crate::comments::Flags::PRETTIER_IGNORE) {
            separator_parts.remove(0);
        }
        if is_next_line_empty(n, text) {
            separator_parts.push(hardline());
        }
        props.push(concat(result));
    }
    let can_have_trailing_separator = !(last_elem.is_some_and(|l| {
        l.is("RestElement")
            || ((is_ts_sig(l) || l.is("TSIndexSignature"))
                && p.has_comment(l, crate::comments::Flags::PRETTIER_IGNORE))
    }) || p.matches(&[
        None,
        Some(&|n, k| n.is("TSImportType") && k == Some("options")),
    ]));
    let content = if props.is_empty() {
        if !p.has_comment(node, crate::comments::Flags::DANGLING) {
            return Ok(concat(vec![
                s("{"),
                s("}"),
                crate::typescript::print_type_annotation_property(p, "typeAnnotation")?,
            ]));
        }
        group(concat(vec![
            s("{"),
            p.print_dangling(node, true, None),
            softline(),
            s("}"),
            crate::typescript::print_optional_token(p),
            crate::typescript::print_type_annotation_property(p, "typeAnnotation")?,
        ]))
    } else {
        let bracket_line = if p.opts.bracket_spacing {
            line()
        } else {
            softline()
        };
        let mut inner = vec![bracket_line.clone()];
        inner.extend(props);
        let trailing =
            if can_have_trailing_separator && (ts_body || p.opts.should_print_comma(true)) {
                separator(())
            } else {
                s("")
            };
        concat(vec![
            empty(),
            s("{"),
            indent(concat(inner)),
            if_break(trailing, empty()),
            bracket_line,
            s("}"),
            crate::typescript::print_optional_token(p),
            crate::typescript::print_type_annotation_property(p, "typeAnnotation")?,
        ])
    };
    let hug_the_only = |n: &Value, k: Option<&str>| {
        matches!(k, Some("params" | "parameters" | "this" | "rest"))
            && should_hug_the_only_function_parameter(n, &p.comments)
    };
    let hug_param = p.matches(&[
        Some(&|n, _| n.is("ObjectPattern") && n.list("decorators").is_empty()),
        Some(&hug_the_only),
    ]) || (crate::typescript::is_object_type(node)
        && (p.matches(&[
            None,
            Some(&|_, k| k == Some("typeAnnotation")),
            Some(&|_, k| k == Some("typeAnnotation")),
            Some(&hug_the_only),
        ]) || p.matches(&[
            None,
            Some(&|n, k| n.is("FunctionTypeParam") && k == Some("typeAnnotation")),
            Some(&hug_the_only),
        ])));
    let pattern_in_assignment = !should_break
        && p.matches(&[
            Some(&|n, _| n.is("ObjectPattern")),
            Some(&|n, _| n.is("AssignmentExpression") || n.is("VariableDeclarator")),
        ]);
    if hug_param || pattern_in_assignment {
        return Ok(content);
    }
    Ok(group_with(content, should_break, None))
}
