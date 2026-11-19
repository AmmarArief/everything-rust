//! `src/language-js/print/assignment.js`.

use crate::value::Value;

use crate::binaryish::should_inline_logical_expression;
use crate::doc::*;
use crate::node::*;
use crate::printer::{Args, Printer, R};
use crate::utils::*;

pub fn print_assignment_expression(p: &mut Printer<'_>) -> R {
    let left = p.print("left")?;
    let op = p.node().str_of("operator").unwrap_or("=").to_string();
    print_assignment(p, left, concat(vec![s(" "), s(op)]), Some("right"))
}

pub fn print_variable_declarator(p: &mut Printer<'_>) -> R {
    let left = p.print("id")?;
    print_assignment(p, left, s(" ="), Some("init"))
}

pub fn print_assignment(
    p: &mut Printer<'_>,
    left: Doc,
    operator: Doc,
    right_key: Option<&'static str>,
) -> R {
    let layout = choose_layout(p, &left, right_key)?;
    let right = match right_key {
        Some(k) => p.print_args(
            k,
            Args {
                assignment_layout: Some(layout),
                ..Args::default()
            },
        )?,
        None => empty(),
    };
    Ok(match layout {
        "break-after-operator" => group(concat(vec![
            group(left),
            operator,
            group(indent(concat(vec![line(), right]))),
        ])),
        "never-break-after-operator" => group(concat(vec![group(left), operator, s(" "), right])),
        "fluid" => {
            let id = p.new_id("assignment");
            group(concat(vec![
                group(left),
                operator,
                group_with(indent(line()), false, Some(id.clone())),
                line_suffix_boundary(),
                indent_if_break(right, Some(id), false),
            ]))
        }
        "break-lhs" => group(concat(vec![left, operator, s(" "), group(right)])),
        "chain" => concat(vec![group(left), operator, line(), right]),
        "chain-tail" => concat(vec![
            group(left),
            operator,
            indent(concat(vec![line(), right])),
        ]),
        "chain-tail-arrow-chain" => concat(vec![group(left), operator, right]),
        _ => left,
    })
}

fn is_assignment(n: &Value) -> bool {
    n.is("AssignmentExpression")
}

fn is_assignment_or_declarator(n: &Value) -> bool {
    is_assignment(n) || n.is("VariableDeclarator")
}

fn choose_layout(
    p: &mut Printer<'_>,
    left: &Doc,
    right_key: Option<&'static str>,
) -> R<&'static str> {
    let node = p.node();
    let Some(right) = right_key.and_then(|k| node.field(k)) else {
        return Ok("only-left");
    };
    let is_tail = !is_assignment(right);
    let chain = p.matches(&[
        Some(&|n, _| is_assignment(n)),
        Some(&|n, _| is_assignment_or_declarator(n)),
        Some(&|n, _| !is_tail || (!n.is("ExpressionStatement") && !n.is("VariableDeclaration"))),
    ]);
    if chain {
        return Ok(if !is_tail {
            "chain"
        } else if right.is("ArrowFunctionExpression")
            && right
                .field("body")
                .is_some_and(|b| b.is("ArrowFunctionExpression"))
        {
            "chain-tail-arrow-chain"
        } else {
            "chain-tail"
        });
    }
    let is_head_of_long_chain = !is_tail && right.field("right").is_some_and(is_assignment);
    if is_head_of_long_chain || has_leading_own_line_comment(&p.comments, p.text, right) {
        return Ok("break-after-operator");
    }
    if node.is("ImportAttribute")
        || matches!(
            p.opts.parser,
            crate::options::Parser::Json
                | crate::options::Parser::Json5
                | crate::options::Parser::Jsonc
        )
        || (right.is("CallExpression")
            && right
                .field("callee")
                .is_some_and(|c| c.str_of("name") == Some("require")))
    {
        return Ok("never-break-after-operator");
    }
    let can_break_left = can_break(left);
    if is_complex_destructuring(node)
        || has_complex_type_annotation(node)
        || (node.is("VariableDeclarator")
            && node
                .field("init")
                .is_some_and(|i| i.is("ArrowFunctionExpression"))
            && can_break_left)
    {
        return Ok("break-lhs");
    }
    let has_short_key = is_object_property_with_short_key(node, left, p.opts.tab_width);
    let key = right_key.unwrap_or("right");
    if p.call(key, |p| should_break_after_operator(p, has_short_key))
        .unwrap_or(Ok(false))?
    {
        return Ok("break-after-operator");
    }
    if is_complex_type_alias_params(node) {
        return Ok("break-lhs");
    }
    if !can_break_left
        && (has_short_key
            || right.is("TemplateLiteral")
            || right.is("TaggedTemplateExpression")
            || is_boolean_literal(right)
            || is_numeric_literal(right)
            || right.is("ClassExpression"))
    {
        return Ok("never-break-after-operator");
    }
    Ok("fluid")
}

fn should_break_after_operator(p: &mut Printer<'_>, has_short_key: bool) -> R<bool> {
    let right = p.node();
    if is_binaryish(right) && !should_inline_logical_expression(right) {
        return Ok(true);
    }
    match right.ty() {
        "StringLiteralTypeAnnotation" | "SequenceExpression" => return Ok(true),
        "TSConditionalType" | "ConditionalTypeAnnotation" => {
            if p.opts.experimental_ternaries || should_break_before_conditional_type(right) {
                return Ok(true);
            }
        }
        "ConditionalExpression" => {
            if !p.opts.experimental_ternaries {
                let test = right.field("test");
                return Ok(
                    test.is_some_and(|t| is_binaryish(t) && !should_inline_logical_expression(t))
                );
            }
            return Ok(right
                .field("consequent")
                .is_some_and(|c| c.is("ConditionalExpression"))
                || right
                    .field("alternate")
                    .is_some_and(|c| c.is("ConditionalExpression")));
        }
        "ClassExpression" => return Ok(!right.list("decorators").is_empty()),
        _ => {}
    }
    if has_short_key {
        return Ok(false);
    }
    let mut node = right;
    let mut path: Vec<(&'static str, Option<usize>)> = Vec::new();
    loop {
        if node.is("UnaryExpression")
            || node.is("AwaitExpression")
            || (node.is("YieldExpression") && node.field("argument").is_some())
        {
            match node.field("argument") {
                Some(a) => {
                    node = a;
                    path.push(("argument", None));
                }
                None => break,
            }
        } else if node.is("TSNonNullExpression") {
            match node.field("expression") {
                Some(a) => {
                    node = a;
                    path.push(("expression", None));
                }
                None => break,
            }
        } else {
            break;
        }
    }
    if is_string_literal(node) {
        return Ok(true);
    }
    p.call_path(&path, |p| {
        is_poorly_breakable_member_or_call_chain(p, false)
    })
    .unwrap_or(Ok(false))
}

pub fn is_arrow_function_variable_declarator(n: &Value) -> bool {
    n.is("VariableDeclarator")
        && n.field("init")
            .is_some_and(|i| i.is("ArrowFunctionExpression"))
}

fn type_params_of_reference(n: &Value) -> Option<&[Value]> {
    if n.is("TSTypeReference") || n.is("GenericTypeAnnotation") {
        return n
            .field("typeArguments")
            .or_else(|| n.field("typeParameters"))
            .map(|t| t.list("params"));
    }
    None
}

fn has_complex_type_annotation(n: &Value) -> bool {
    if !n.is("VariableDeclarator") {
        return false;
    }
    let Some(annotation) = n
        .field("id")
        .and_then(|id| id.field("typeAnnotation"))
        .and_then(|t| t.field("typeAnnotation"))
    else {
        return false;
    };
    let Some(params) = type_params_of_reference(annotation) else {
        return false;
    };
    params.len() > 1
        && params.iter().any(|pa| {
            type_params_of_reference(pa).is_some_and(|x| !x.is_empty())
                || pa.is("TSConditionalType")
        })
}

fn is_complex_type_alias_params(n: &Value) -> bool {
    if !(n.is("TSTypeAliasDeclaration") || n.is("TypeAlias")) {
        return false;
    }
    let key = if n.is("TSTypeAliasDeclaration") {
        "constraint"
    } else {
        "bound"
    };
    let params = n
        .field("typeParameters")
        .map(|t| t.list("params"))
        .unwrap_or(&[]);
    params.len() > 1
        && params
            .iter()
            .any(|pa| pa.field(key).is_some() || pa.field("default").is_some())
}

fn should_break_before_conditional_type(n: &Value) -> bool {
    let is_generic = |sub: Option<&Value>| {
        sub.is_some_and(|s| match s.ty() {
            "FunctionTypeAnnotation" | "GenericTypeAnnotation" | "TSFunctionType" => {
                s.field("typeParameters").is_some()
            }
            "TSTypeReference" => {
                s.field("typeArguments").is_some() || s.field("typeParameters").is_some()
            }
            _ => false,
        })
    };
    is_generic(n.field("checkType")) || is_generic(n.field("extendsType"))
}

fn is_call_expression_with_complex_type_arguments(p: &mut Printer<'_>) -> R<bool> {
    let node = p.node();
    let Some(type_args) = node
        .field("typeParameters")
        .or_else(|| node.field("typeArguments"))
    else {
        return Ok(false);
    };
    let params = type_args.list("params");
    if params.is_empty() {
        return Ok(false);
    }
    if params.len() > 1 {
        return Ok(true);
    }
    let first = &params[0];
    if matches!(
        first.ty(),
        "TSUnionType"
            | "UnionTypeAnnotation"
            | "TSIntersectionType"
            | "IntersectionTypeAnnotation"
            | "TSTypeLiteral"
            | "ObjectTypeAnnotation"
    ) {
        return Ok(true);
    }
    let key = if node.field("typeParameters").is_some() {
        "typeParameters"
    } else {
        "typeArguments"
    };
    Ok(will_break(&p.print(key)?))
}

fn is_complex_destructuring(n: &Value) -> bool {
    if !is_assignment_or_declarator(n) {
        return false;
    }
    let Some(left) = n.field("left").or_else(|| n.field("id")) else {
        return false;
    };
    let props = left.list("properties");
    left.is("ObjectPattern")
        && props.len() > 2
        && props.iter().any(|pr| {
            is_object_property(pr)
                && (!pr.flag("shorthand")
                    || pr.field("value").is_some_and(|v| v.is("AssignmentPattern")))
        })
}

fn is_poorly_breakable_member_or_call_chain(p: &mut Printer<'_>, deep: bool) -> R<bool> {
    let node = p.node();
    if node.is("ChainExpression") || node.is("TSNonNullExpression") {
        return p
            .call("expression", |p| {
                is_poorly_breakable_member_or_call_chain(p, true)
            })
            .unwrap_or(Ok(false));
    }
    if is_call_expression(node) {
        let doc = crate::call::print_call_expression(p)?;
        if label_of(&doc) == Some("member-chain") {
            return Ok(false);
        }
        let args = call_arguments(node);
        let poorly = args.is_empty()
            || (args.len() == 1
                && is_lone_short_argument(args[0], p.opts.print_width, p.opts, &p.comments));
        if !poorly {
            return Ok(false);
        }
        if is_call_expression_with_complex_type_arguments(p)? {
            return Ok(false);
        }
        return p
            .call("callee", |p| {
                is_poorly_breakable_member_or_call_chain(p, true)
            })
            .unwrap_or(Ok(false));
    }
    if is_member_expression(node) {
        return p
            .call("object", |p| {
                is_poorly_breakable_member_or_call_chain(p, true)
            })
            .unwrap_or(Ok(false));
    }
    Ok(deep && (node.is("Identifier") || node.is("ThisExpression")))
}

fn is_object_property_with_short_key(n: &Value, key: &Doc, tab_width: usize) -> bool {
    if !is_object_property(n) {
        return false;
    }
    match clean_string(key) {
        Some(k) => width(&k) < tab_width + 3,
        None => false,
    }
}
