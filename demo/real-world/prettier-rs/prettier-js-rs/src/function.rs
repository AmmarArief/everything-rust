//! `src/language-js/print/function.js` and `function-parameters.js`.

use crate::value::Value;

use crate::call::ARG_EXPANSION_BAILOUT;
use crate::doc::*;
use crate::node::*;
use crate::printer::{Args, Printer, R, bail};
use crate::utils::*;

fn is_method_value(p: &Printer<'_>) -> bool {
    let node = p.node();
    p.key() == Some("value")
        && node.is("FunctionExpression")
        && p.parent().is_some_and(|pa| {
            matches!(
                pa.ty(),
                "ObjectMethod"
                    | "ClassMethod"
                    | "ClassPrivateMethod"
                    | "MethodDefinition"
                    | "TSAbstractMethodDefinition"
                    | "TSDeclareMethod"
            ) || (pa.is("Property") && crate::estree::is_method(pa))
        })
}

pub fn print_function(p: &mut Printer<'_>, args: &Args) -> R {
    if is_method_value(p) {
        return print_method_value(p);
    }
    let node = p.node();
    let mut expand_arg = false;
    if args.expand_last_arg
        && (node.is("FunctionDeclaration") || node.is("FunctionExpression"))
        && let Some(parent) = p.parent()
        && is_call_expression(parent)
        && (call_arguments(parent).len() > 1
            || function_parameters(node)
                .iter()
                .all(|pa| pa.is("Identifier") && pa.field("typeAnnotation").is_none()))
    {
        expand_arg = true;
    }
    let mut parts = vec![
        crate::typescript::print_declare_token(p),
        s(if node.flag("async") { "async " } else { "" }),
        s(if node.flag("generator") {
            "function* "
        } else {
            "function "
        }),
    ];
    if node.field("id").is_some() {
        parts.push(p.print("id")?);
    }
    let params = print_function_parameters(p, expand_arg)?;
    let return_type = crate::typescript::print_return_type(p)?;
    let group_params = crate::typescript::should_group_function_parameters(p, node, &return_type);
    parts.push(crate::typescript::print_function_type_parameters(p)?);
    parts.push(group(concat(vec![
        if group_params { group(params) } else { params },
        return_type,
    ])));
    let has_body = node.field("body").is_some();
    if has_body {
        parts.push(s(" "));
        parts.push(p.print("body")?);
    }
    if p.opts.semi && (node.flag("declare") || !has_body) {
        parts.push(s(";"));
    }
    Ok(concat(parts))
}

pub fn print_method(p: &mut Printer<'_>) -> R {
    let node = p.node();
    let kind = node.str_of("kind").unwrap_or("");
    let value = node.field("value").unwrap_or(node);
    let mut parts = Vec::new();
    if kind.is_empty() || kind == "init" || kind == "method" || kind == "constructor" {
        if value.flag("async") {
            parts.push(s("async "));
        }
    } else {
        parts.push(s(kind.to_string()));
        parts.push(s(" "));
    }
    if value.flag("generator") {
        parts.push(s("*"));
    }
    parts.push(crate::property::print_property_key(p)?);
    if node.flag("optional") {
        parts.push(s("?"));
    }
    if node.same(value) {
        parts.push(print_method_value(p)?);
    } else {
        parts.push(p.print("value")?);
    }
    Ok(concat(parts))
}

pub fn print_method_value_pub(p: &mut Printer<'_>) -> R {
    print_method_value(p)
}

fn print_method_value(p: &mut Printer<'_>) -> R {
    let node = p.node();
    let params = print_function_parameters(p, false)?;
    let return_type = crate::typescript::print_return_type(p)?;
    let should_break = function_parameters(node).len() > 1
        && function_parameters(node)
            .iter()
            .any(|pa| pa.is("TSParameterProperty"));
    let group_params = crate::typescript::should_group_function_parameters(p, node, &return_type);
    let params = if should_break {
        group_with(params, true, None)
    } else if group_params {
        group(params)
    } else {
        params
    };
    let mut parts = vec![
        crate::typescript::print_function_type_parameters(p)?,
        group(concat(vec![params, return_type])),
    ];
    if node.field("body").is_some() {
        parts.push(s(" "));
        parts.push(p.print("body")?);
    } else {
        parts.push(s(if p.opts.semi { ";" } else { "" }));
    }
    Ok(concat(parts))
}

pub fn can_print_params_without_parens(n: &Value, c: &crate::comments::Comments) -> bool {
    let params = function_parameters(n);
    params.len() == 1
        && n.field("typeParameters").is_none()
        && !c.has(n, crate::comments::Flags::DANGLING)
        && params[0].is("Identifier")
        && params[0].field("typeAnnotation").is_none()
        && !c.has_any(params[0])
        && !params[0].flag("optional")
        && n.field("predicate").is_none()
        && n.field("returnType").is_none()
}

pub fn should_print_params_without_parens(p: &Printer<'_>) -> bool {
    if p.opts.arrow_parens_always {
        return false;
    }
    can_print_params_without_parens(p.node(), &p.comments)
}

pub fn print_return_or_throw_argument(p: &mut Printer<'_>) -> R {
    let node = p.node();
    let mut parts = Vec::new();
    if let Some(arg) = node.field("argument") {
        let mut doc = p.print("argument")?;
        if return_argument_has_leading_comment(p, arg) {
            doc = concat(vec![
                s("("),
                indent(concat(vec![hardline(), doc])),
                hardline(),
                s(")"),
            ]);
        } else if is_binaryish(arg)
            || (p.opts.experimental_ternaries
                && arg.is("ConditionalExpression")
                && (arg
                    .field("consequent")
                    .is_some_and(|c| c.is("ConditionalExpression"))
                    || arg
                        .field("alternate")
                        .is_some_and(|c| c.is("ConditionalExpression"))))
        {
            doc = group(concat(vec![
                if_break(s("("), empty()),
                indent(concat(vec![softline(), doc])),
                softline(),
                if_break(s(")"), empty()),
            ]));
        }
        parts.push(s(" "));
        parts.push(doc);
    }
    let semi = p.opts.semi;
    let has_dangling = p.has_comment(node, crate::comments::Flags::DANGLING);
    let semi_before = semi
        && has_dangling
        && p.comments.has(
            node,
            crate::comments::Flags {
                last: true,
                line: true,
                ..crate::comments::Flags::ANY
            },
        );
    if semi_before {
        parts.push(s(";"));
    }
    if has_dangling {
        parts.push(s(" "));
        parts.push(p.print_dangling(node, false, None));
    }
    if !semi_before {
        parts.push(s(if semi { ";" } else { "" }));
    }
    Ok(concat(parts))
}

fn get_left_side(n: &Value) -> Option<&Value> {
    if let Some(exprs) = n.get("expressions").and_then(Value::as_array) {
        return exprs.first();
    }
    [
        "left",
        "test",
        "callee",
        "object",
        "tag",
        "argument",
        "expression",
    ]
    .iter()
    .find_map(|k| n.field(k))
}

fn return_argument_has_leading_comment(p: &Printer<'_>, arg: &Value) -> bool {
    let text = p.text;
    if has_leading_own_line_comment(&p.comments, text, arg)
        || (p
            .comments
            .has_with(arg, crate::comments::Flags::LEADING, &|c| {
                crate::text::has_newline_in_range(text, c.start, c.end)
            })
            && !is_jsx_element(arg))
    {
        return true;
    }
    if has_naked_left_side(arg) {
        let mut left_most = arg;
        while let Some(next) = get_left_side(left_most) {
            left_most = next;
            if has_leading_own_line_comment(&p.comments, text, left_most) {
                return true;
            }
        }
    }
    false
}

pub fn should_hug_the_only_function_parameter(n: &Value, c: &crate::comments::Comments) -> bool {
    let params = function_parameters(n);
    if params.len() != 1 {
        return false;
    }
    let param = params[0];
    !c.has_any(param)
        && (param.is("ObjectPattern")
            || (param.is("Identifier")
                && param.field("typeAnnotation").is_some_and(|t| {
                    (t.is("TSTypeAnnotation") || t.is("TypeAnnotation"))
                        && t.field("typeAnnotation")
                            .is_some_and(crate::typescript::is_object_type)
                }))
            || param.is("ArrayPattern")
            || (param.is("AssignmentPattern")
                && param
                    .field("left")
                    .is_some_and(|l| l.is("ObjectPattern") || l.is("ArrayPattern"))
                && param.field("right").is_some_and(|r| {
                    r.is("Identifier")
                        || (r.is("ObjectExpression") && r.list("properties").is_empty())
                        || (r.is("ArrayExpression") && r.list("elements").is_empty())
                })))
}

fn is_decorated_function(p: &Printer<'_>) -> bool {
    p.matches(&[
        Some(&|n, _| {
            n.is("ArrowFunctionExpression")
                && n.field("body").is_some_and(|b| b.is("BlockStatement"))
        }),
        Some(&|n, k| {
            if n.is("CallExpression")
                && k == Some("arguments")
                && n.list("arguments").len() == 1
                && n.field("callee").is_some_and(|c| c.is("CallExpression"))
            {
                let decorator = n.field("callee").and_then(|c| c.field("callee"));
                return decorator.is_some_and(|d| {
                    d.is("Identifier")
                        || (d.is("MemberExpression")
                            && !d.flag("computed")
                            && d.field("object").is_some_and(|o| o.is("Identifier"))
                            && d.field("property").is_some_and(|o| o.is("Identifier")))
                });
            }
            false
        }),
        Some(&|n, k| {
            (n.is("VariableDeclarator") && k == Some("init"))
                || (n.is("ExportDefaultDeclaration") && k == Some("declaration"))
                || (n.is("TSExportAssignment") && k == Some("expression"))
                || (n.is("AssignmentExpression")
                    && k == Some("right")
                    && n.field("left").is_some_and(|l| {
                        l.is("MemberExpression")
                            && l.field("object").is_some_and(|o| {
                                o.is("Identifier") && o.str_of("name") == Some("module")
                            })
                            && l.field("property").is_some_and(|o| {
                                o.is("Identifier") && o.str_of("name") == Some("exports")
                            })
                    }))
        }),
        Some(&|n, _| {
            !n.is("VariableDeclaration")
                || (n.str_of("kind") == Some("const") && n.list("declarations").len() == 1)
        }),
    ])
}

pub fn print_function_parameters(p: &mut Printer<'_>, expand_arg: bool) -> R {
    print_function_parameters_ext(p, expand_arg, false)
}

pub fn print_function_parameters_ext(
    p: &mut Printer<'_>,
    expand_arg: bool,
    print_type_params: bool,
) -> R {
    let node = p.node();
    let params = function_parameters(node);
    if node.field("this").is_some() || node.field("rest").is_some() {
        return bail("unusual parameters");
    }
    let type_params = if print_type_params {
        crate::typescript::print_function_type_parameters(p)?
    } else {
        empty()
    };
    if params.is_empty() {
        let text = p.text;
        let dangling = crate::comments::print_dangling_comments(
            &p.comments,
            node,
            text,
            false,
            None,
            Some(&|c: &crate::comments::Comment| {
                crate::text::next_non_space_non_comment_char(text, c.end) == Some(')')
            }),
        );
        return Ok(concat(vec![type_params, s("("), dangling, s(")")]));
    }
    let parent = p.parent();
    let in_test_call = parent.is_some_and(|pa| is_test_call(pa, None));
    let hug = should_hug_the_only_function_parameter(node, &p.comments);
    let text = p.text;
    let count = params.len();
    let mut printed = Vec::new();
    p.each("params", |p, i| {
        printed.push(p.print_current(Args::default())?);
        if i + 1 == count {
            return Ok(());
        }
        printed.push(s(","));
        if in_test_call || hug {
            printed.push(s(" "));
        } else if is_next_line_empty(p.node(), text) {
            printed.push(hardline());
            printed.push(hardline());
        } else {
            printed.push(line());
        }
        Ok(())
    })?;
    if expand_arg && !is_decorated_function(p) {
        let printed_doc = concat(printed);
        if will_break(&type_params) || will_break(&printed_doc) {
            return bail(ARG_EXPANSION_BAILOUT);
        }
        return Ok(group(concat(vec![
            remove_lines(&type_params),
            s("("),
            remove_lines(&printed_doc),
            s(")"),
        ])));
    }
    let no_param_decorators = params.iter().all(|pa| pa.list("decorators").is_empty());
    if (hug && no_param_decorators) || in_test_call {
        let mut v = vec![type_params, s("(")];
        v.extend(printed);
        v.push(s(")"));
        return Ok(concat(v));
    }
    let mut inner = vec![softline()];
    inner.extend(printed);
    Ok(concat(vec![
        type_params,
        s("("),
        indent(concat(inner)),
        if_break(
            s(
                if !has_rest_parameter(node) && p.opts.should_print_comma(false) {
                    ","
                } else {
                    ""
                },
            ),
            empty(),
        ),
        softline(),
        s(")"),
    ]))
}
